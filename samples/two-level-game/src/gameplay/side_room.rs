use nara::{
    advanced_prelude::CurrentScene,
    asset::{AssetProductRef, AssetRef, ImportedProductId},
    ecs::World,
    identity::{EntityLookup, SceneEntityId, SpawnedSceneInstance},
    prelude::Resource,
    project_host::{
        ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes,
        activate_prepared_project_scene, unload_project_scene_addition,
    },
    scene::SceneOccurrenceKey,
    sprite::Sprite,
    transform::fixed_spatial_snapshot,
};

use crate::side_room::{
    LOOKOUT_RELIC, LookoutPortal, RELIC, SideRoomPortal, lookout_scene_product, scene_product,
};

use super::{
    GamePhase, Session,
    commands::{LOOKOUT, SIDE_ROOM},
    flow, player,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SideRoomPhase {
    #[default]
    Closed,
    Preparing,
    Open,
    RelicFound,
}

#[derive(Clone, Copy)]
pub(super) enum Room {
    SideRoom,
    Lookout,
}

impl Room {
    const ALL: [Self; 2] = [Self::SideRoom, Self::Lookout];

    fn name(self) -> &'static str {
        match self {
            Self::SideRoom => "side room",
            Self::Lookout => "lookout",
        }
    }
}

#[derive(Resource, Default)]
pub(super) struct Rooms([RoomState; 2]);

#[derive(Default)]
enum RoomState {
    #[default]
    Closed,
    Preparing(ProjectSceneRequestId),
    Open {
        receipt: SpawnedSceneInstance,
        relic: SceneEntityId,
        found: bool,
    },
}

impl Rooms {
    pub(super) fn phase(&self, room: Room) -> SideRoomPhase {
        match self.0[room as usize] {
            RoomState::Closed => SideRoomPhase::Closed,
            RoomState::Preparing(_) => SideRoomPhase::Preparing,
            RoomState::Open { found: false, .. } => SideRoomPhase::Open,
            RoomState::Open { found: true, .. } => SideRoomPhase::RelicFound,
        }
    }

    pub(super) fn require_closed(world: &World) -> Result<(), String> {
        let rooms = world.resource::<Self>();
        for room in Room::ALL {
            if rooms.phase(room) != SideRoomPhase::Closed {
                return Err(format!(
                    "close the {} before retry or exact capture",
                    room.name()
                ));
            }
        }
        Ok(())
    }

    pub(super) fn live_members(&self) -> usize {
        self.0
            .iter()
            .map(|state| match state {
                RoomState::Open { receipt, .. } => receipt.len(),
                _ => 0,
            })
            .sum()
    }
}

pub(super) fn toggle(world: &mut World, room: Room) -> Result<(), String> {
    let index = room as usize;
    match &world.resource::<Rooms>().0[index] {
        RoomState::Closed => {
            let product = portal_product(world, room)?;
            let request = ProjectScenes::scope(world, |scenes| scenes.request_additive(product))
                .map_err(|error| error.to_string())?;
            world.resource_mut::<Rooms>().0[index] = RoomState::Preparing(request);
        }
        RoomState::Preparing(request) => {
            let request = *request;
            ProjectScenes::scope(world, |scenes| scenes.cancel(request));
        }
        RoomState::Open { receipt, .. } => {
            let receipt = receipt.clone();
            unload_project_scene_addition(world, &receipt).map_err(|diagnostics| {
                format!("{} unload rejected: {diagnostics:?}", room.name())
            })?;
            world.resource_mut::<Rooms>().0[index] = RoomState::Closed;
        }
    }
    Ok(())
}

fn portal_product(world: &World, room: Room) -> Result<AssetProductRef, String> {
    let current = CurrentScene::get(world).ok_or("the main level is unavailable")?;
    let gate = SceneEntityId::new("portal").expect("static portal ID");
    let missing_portal = || format!("the current level has no {} portal", room.name());
    let EntityLookup::Resolved(gate) = current.receipt().resolve(world, &gate) else {
        return Err(missing_portal());
    };
    let (reference, expected) = match room {
        Room::SideRoom => (
            world.get::<SideRoomPortal>(gate).map(|portal| &portal.room),
            scene_product(),
        ),
        Room::Lookout => (
            world.get::<LookoutPortal>(gate).map(|portal| &portal.room),
            lookout_scene_product(),
        ),
    };
    let product = match reference.ok_or_else(missing_portal)? {
        AssetRef::Product(product) => Some(*product),
        AssetRef::StableId(id) => AssetProductRef::new(*id, ImportedProductId::PRIMARY).ok(),
        _ => None,
    };
    product
        .filter(|product| *product == expected)
        .ok_or_else(|| {
            format!(
                "the {} portal must select the authored Scene Product",
                room.name()
            )
        })
}

pub(super) fn advance(world: &mut World) {
    let rooms = world.resource::<Rooms>();
    let Some((room, request)) = Room::ALL.into_iter().find_map(|room| {
        if let RoomState::Preparing(request) = rooms.0[room as usize] {
            Some((room, request))
        } else {
            None
        }
    }) else {
        return;
    };
    let (owner, relic) = match room {
        Room::SideRoom => (SIDE_ROOM, RELIC),
        Room::Lookout => (LOOKOUT, LOOKOUT_RELIC),
    };
    let name = room.name();
    let relic = SceneEntityId::new(relic).expect("static relic ID");
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
                .ok_or_else(|| format!("the {name} requires its authored relic")),
        ),
        Some(ProjectSceneRequestState::Cancelled) => Some(Ok(false)),
        state => Some(Err(format!(
            "{name} preparation rejected: {state:?}; {:?}",
            scenes.diagnostics(request)
        ))),
    });
    let Some(ready) = ready else { return };
    let result = ready.and_then(|activate| {
        if !activate {
            return Ok(RoomState::Closed);
        }
        let occurrence = SceneOccurrenceKey::new(owner, "meadow").expect("static room occurrence");
        let report = activate_prepared_project_scene(world, request, &occurrence);
        let receipt = report
            .instance
            .ok_or_else(|| format!("{name} activation rejected: {:?}", report.diagnostics))?;
        Ok(RoomState::Open {
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
    let state = match result {
        Ok(state) => state,
        Err(error) => {
            world.resource_mut::<Session>().error = Some(error);
            RoomState::Closed
        }
    };
    world.resource_mut::<Rooms>().0[room as usize] = state;
}

pub(super) fn find_relics(world: &mut World) {
    if !flow::simulation_enabled(world) || world.resource::<Session>().phase != GamePhase::Running {
        return;
    }
    for room in Room::ALL {
        let index = room as usize;
        let RoomState::Open {
            receipt,
            relic,
            found: false,
        } = &world.resource::<Rooms>().0[index]
        else {
            continue;
        };
        let EntityLookup::Resolved(entity) = receipt.resolve(world, relic) else {
            continue;
        };
        let Some(player) = player(world) else { return };
        let nearby = fixed_spatial_snapshot(world).is_ok_and(|spatial| {
            let (Ok(player), Ok(relic)) = (spatial.global(player), spatial.global(entity)) else {
                return false;
            };
            relic.translation().distance_squared(player.translation()) <= 10.0 * 10.0
        });
        if nearby && let Some(mut sprite) = world.get_mut::<Sprite>(entity) {
            sprite.material.tint.a = 0.0;
            if let RoomState::Open { found, .. } = &mut world.resource_mut::<Rooms>().0[index] {
                *found = true;
            }
        }
    }
}
