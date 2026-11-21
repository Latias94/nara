use nara::{
    advanced_prelude::CurrentScene,
    asset::{AssetProductRef, AssetRef, ImportedProductId},
    ecs::World,
    identity::{EntityLookup, SceneEntityId, SpawnedSceneInstance, WorldIdentityDomain},
    prelude::{Resource, Transform2d},
    project_host::{
        ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes,
        activate_prepared_project_scene, unload_project_scene_addition,
    },
    scene::SceneOccurrenceKey,
    transform::fixed_spatial_snapshot,
};

use crate::{
    resume::processed::{ProcessedRoomState, ProcessedRooms},
    side_room::{
        LOOKOUT_RELIC, LookoutPortal, RELIC, SideRoomPortal, lookout_scene_product, scene_product,
    },
};

use super::{
    Session,
    commands::{LOOKOUT, SIDE_ROOM},
    player, simulation_enabled,
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
    Preparing {
        request: ProjectSceneRequestId,
        restored_found: Option<bool>,
    },
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
            RoomState::Preparing { .. } => SideRoomPhase::Preparing,
            RoomState::Open { found: false, .. } => SideRoomPhase::Open,
            RoomState::Open { found: true, .. } => SideRoomPhase::RelicFound,
        }
    }

    pub(super) fn require_closed(world: &World) -> Result<(), String> {
        let rooms = world.resource::<Self>();
        for room in Room::ALL {
            if rooms.phase(room) != SideRoomPhase::Closed {
                return Err(format!(
                    "close the {} before retry or source reconstruction",
                    room.name()
                ));
            }
        }
        Ok(())
    }

    pub(super) fn capture_processed(world: &World) -> Result<ProcessedRooms, String> {
        let rooms = world.resource::<Self>();
        let domain = world.resource::<WorldIdentityDomain>();
        let captured = ProcessedRooms {
            side_room: rooms.capture_room(world, domain, Room::SideRoom)?,
            lookout: rooms.capture_room(world, domain, Room::Lookout)?,
        };
        let expected_instances = 1
            + world.resource::<Session>().markers.len()
            + usize::from(captured.side_room != ProcessedRoomState::Closed)
            + usize::from(captured.lookout != ProcessedRoomState::Closed);
        if domain.stats().active_scene_instances != expected_instances {
            return Err(
                "live Scene instances differ from the Server's continuation inventory".into(),
            );
        }
        Ok(captured)
    }

    fn capture_room(
        &self,
        world: &World,
        domain: &WorldIdentityDomain,
        room: Room,
    ) -> Result<ProcessedRoomState, String> {
        match &self.0[room as usize] {
            RoomState::Closed => Ok(ProcessedRoomState::Closed),
            RoomState::Preparing { .. } => Err(format!("the {} is still preparing", room.name())),
            RoomState::Open {
                receipt,
                relic,
                found,
            } => {
                if !domain
                    .active_scene_instance(world, receipt.instance_id())
                    .is_ok_and(|live| live == *receipt)
                    || !matches!(receipt.resolve(world, relic), EntityLookup::Resolved(entity)
                        if world.get::<Transform2d>(entity).is_some())
                {
                    return Err(format!(
                        "the {} lost its original Scene membership or relic",
                        room.name()
                    ));
                }
                Ok(if *found {
                    ProcessedRoomState::RelicFound
                } else {
                    ProcessedRoomState::Open
                })
            }
        }
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
        RoomState::Closed => request_open(world, room, None)?,
        RoomState::Preparing { request, .. } => {
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

pub(super) fn restore_processed(
    world: &mut World,
    expected: ProcessedRooms,
) -> Result<bool, String> {
    use ProcessedRoomState as Captured;
    use SideRoomPhase as Live;

    for (room, expected) in [
        (Room::SideRoom, expected.side_room),
        (Room::Lookout, expected.lookout),
    ] {
        match (world.resource::<Rooms>().phase(room), expected) {
            (Live::Closed, Captured::Closed)
            | (Live::Open, Captured::Open)
            | (Live::RelicFound, Captured::RelicFound) => {}
            (Live::Closed, Captured::Open | Captured::RelicFound) => {
                request_open(world, room, Some(expected == Captured::RelicFound))?;
                return Ok(false);
            }
            (Live::Preparing, Captured::Open | Captured::RelicFound) => return Ok(false),
            _ => {
                return Err(format!(
                    "the reconstructed {} differs from its captured state",
                    room.name()
                ));
            }
        }
    }
    Ok(true)
}

fn request_open(world: &mut World, room: Room, restored_found: Option<bool>) -> Result<(), String> {
    let product = portal_product(world, room)?;
    let request = ProjectScenes::scope(world, |scenes| scenes.request_additive(product))
        .map_err(|error| error.to_string())?;
    world.resource_mut::<Rooms>().0[room as usize] = RoomState::Preparing {
        request,
        restored_found,
    };
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
    let Some((room, request, restored_found)) = Room::ALL.into_iter().find_map(|room| {
        if let RoomState::Preparing {
            request,
            restored_found,
        } = rooms.0[room as usize]
        {
            Some((room, request, restored_found))
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
    let local_player = world.resource::<Session>().local_player;
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
                            && (!local_player
                                || entity
                                    .components()
                                    .keys()
                                    .any(|kind| kind.as_str() == "nara.sprite.Sprite"))
                    })
                })
                .map(|_| true)
                .ok_or_else(|| format!("the {name} requires its authored relic")),
        ),
        Some(ProjectSceneRequestState::Cancelled) if restored_found.is_none() => Some(Ok(false)),
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
            found: restored_found.unwrap_or_default(),
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
            if restored_found.is_some() {
                super::fail(world, error);
            } else {
                world.resource_mut::<Session>().error = Some(error);
            }
            RoomState::Closed
        }
    };
    world.resource_mut::<Rooms>().0[room as usize] = state;
}

pub(super) fn find_relics(world: &mut World) {
    if !simulation_enabled(world) {
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
        if nearby {
            #[cfg(feature = "local-player")]
            if world.resource::<Session>().local_player
                && world.get::<nara::sprite::Sprite>(entity).is_none()
            {
                continue;
            }
            if let RoomState::Open { found, .. } = &mut world.resource_mut::<Rooms>().0[index] {
                *found = true;
            }
            #[cfg(feature = "local-player")]
            if world.resource::<Session>().local_player {
                super::hide_sprite(world, entity);
            }
        }
    }
}
