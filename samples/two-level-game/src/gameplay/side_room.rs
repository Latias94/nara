use nara::{
    advanced_prelude::CurrentScene,
    asset::{AssetProductRef, AssetRef, ImportedProductId},
    ecs::World,
    identity::{EntityLookup, SceneEntityId, SpawnedSceneInstance},
    prelude::{Resource, Transform2d, Vec2},
    project_host::{
        ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes,
        activate_prepared_project_scene, unload_project_scene_addition,
    },
    scene::SceneOccurrenceKey,
    sprite::Sprite,
};

use crate::side_room::{RELIC, SideRoomPortal, scene_product};

use super::Session;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SideRoomPhase {
    #[default]
    Closed,
    Preparing,
    Open,
    RelicFound,
}

#[derive(Resource, Default)]
pub(super) enum SideRoom {
    #[default]
    Closed,
    Preparing(ProjectSceneRequestId),
    Open {
        receipt: SpawnedSceneInstance,
        relic: SceneEntityId,
        found: bool,
    },
}

impl SideRoom {
    pub(super) fn phase(&self) -> SideRoomPhase {
        match self {
            Self::Closed => SideRoomPhase::Closed,
            Self::Preparing(_) => SideRoomPhase::Preparing,
            Self::Open { found: false, .. } => SideRoomPhase::Open,
            Self::Open { found: true, .. } => SideRoomPhase::RelicFound,
        }
    }

    pub(super) fn require_closed(world: &World) -> Result<(), String> {
        if matches!(world.resource::<Self>(), Self::Closed) {
            Ok(())
        } else {
            Err("close the side room before replacing or capturing the main level".into())
        }
    }
}

pub(super) fn toggle(world: &mut World) -> Result<(), String> {
    match world.resource::<SideRoom>() {
        SideRoom::Closed => {
            let product = portal_product(world)?;
            let request = ProjectScenes::scope(world, |scenes| scenes.request_additive(product))
                .map_err(|error| error.to_string())?;
            *world.resource_mut::<SideRoom>() = SideRoom::Preparing(request);
        }
        SideRoom::Preparing(request) => {
            let request = *request;
            ProjectScenes::scope(world, |scenes| scenes.cancel(request));
        }
        SideRoom::Open { receipt, .. } => {
            let receipt = receipt.clone();
            unload_project_scene_addition(world, &receipt)
                .map_err(|diagnostics| format!("side room unload rejected: {diagnostics:?}"))?;
            *world.resource_mut::<SideRoom>() = SideRoom::Closed;
        }
    }
    Ok(())
}

fn portal_product(world: &World) -> Result<AssetProductRef, String> {
    let current = CurrentScene::get(world).ok_or("the main level is unavailable")?;
    let gate = SceneEntityId::new("portal").expect("static portal ID");
    let EntityLookup::Resolved(gate) = current.receipt().resolve(world, &gate) else {
        return Err("the current level has no side room portal".into());
    };
    let portal = world
        .get::<SideRoomPortal>(gate)
        .ok_or("the current level has no side room portal")?;
    let product = match &portal.room {
        AssetRef::Product(product) => Some(*product),
        AssetRef::StableId(id) => AssetProductRef::new(*id, ImportedProductId::PRIMARY).ok(),
        _ => None,
    };
    product
        .filter(|product| *product == scene_product())
        .ok_or("the side room portal must select the authored Scene Product".into())
}

pub(super) fn advance(world: &mut World) {
    let SideRoom::Preparing(request) = *world.resource::<SideRoom>() else {
        return;
    };
    let relic = SceneEntityId::new(RELIC).expect("static relic ID");
    let ready = ProjectScenes::scope(world, |scenes| match scenes.state(request) {
        Some(ProjectSceneRequestState::Preparing | ProjectSceneRequestState::Cancelling) => None,
        Some(ProjectSceneRequestState::Ready) => Some(
            scenes
                .prepared(request)
                .filter(|view| {
                    view.scene().entities().iter().any(|entity| {
                        entity.id() == &relic
                            && entity
                                .components()
                                .keys()
                                .any(|kind| kind.as_str() == "nara.transform.Transform2d")
                            && entity
                                .components()
                                .keys()
                                .any(|kind| kind.as_str() == "nara.sprite.Sprite")
                    })
                })
                .map(|_| true)
                .ok_or_else(|| "the side room requires its authored relic".into()),
        ),
        Some(ProjectSceneRequestState::Cancelled) => Some(Ok(false)),
        state => Some(Err(format!(
            "side room preparation rejected: {state:?}; {:?}",
            scenes.diagnostics(request)
        ))),
    });
    let Some(ready) = ready else { return };
    let result = ready.and_then(|activate| {
        if !activate {
            return Ok(SideRoom::Closed);
        }
        let occurrence = SceneOccurrenceKey::new("two-level-game.side-room", "meadow")
            .expect("static side room occurrence");
        let report = activate_prepared_project_scene(world, request, &occurrence);
        let receipt = report
            .instance
            .ok_or_else(|| format!("side room activation rejected: {:?}", report.diagnostics))?;
        Ok(SideRoom::Open {
            receipt,
            relic,
            found: false,
        })
    });
    ProjectScenes::scope(world, |scenes| {
        if result.is_err() {
            scenes.cancel(request);
        }
        scenes.acknowledge(request);
    });
    match result {
        Ok(state) => *world.resource_mut::<SideRoom>() = state,
        Err(error) => {
            *world.resource_mut::<SideRoom>() = SideRoom::Closed;
            world.resource_mut::<Session>().error = Some(error);
        }
    }
}

pub(super) fn find_relic(world: &mut World, position: Vec2) {
    let SideRoom::Open {
        receipt,
        relic,
        found: false,
    } = world.resource::<SideRoom>()
    else {
        return;
    };
    let EntityLookup::Resolved(entity) = receipt.resolve(world, relic) else {
        return;
    };
    if world
        .get::<Transform2d>(entity)
        .is_some_and(|transform| transform.translation.distance_squared(position) <= 10.0 * 10.0)
        && let Some(mut sprite) = world.get_mut::<Sprite>(entity)
    {
        sprite.material.tint.a = 0.0;
        if let SideRoom::Open { found, .. } = &mut *world.resource_mut::<SideRoom>() {
            *found = true;
        }
    }
}
