use std::time::{Duration, Instant};

use nara::{
    advanced_prelude::CurrentScene,
    gameplay::{GameplayCommandQueue, GameplayCommandQueueStats},
    identity::{
        EntityLookup, IdentityDomainError, SceneEntityId, SpawnedSceneInstance, WorldIdentityDomain,
    },
    prelude::{Transform2d, Vec2},
    project_host::HeadlessSession,
    scene::SceneEntitySource,
};
use serde_json::{Value, json};
use two_level_game::{
    CollisionBox, GamePhase, GameSnapshot, SideRoomPhase,
    command::{SERVER_COMMAND_SOURCE, SemanticCommand, ServerCommand},
    levels::{LevelId, LevelKey},
    side_room::{LOOKOUT_RELIC, RELIC},
};

use super::runtime::{Failure, MAX_FRAMES, frame, submit_external};

const WEST_WALL: &str = "wall-west";
const CONTACT_MARGIN: f32 = 0.01;
const CONTACT_TOLERANCE: f32 = 0.001;
const MAX_MOVEMENT_TICKS: usize = 192;

pub(super) fn verify(
    session: &mut HeadlessSession,
    startup: GameSnapshot,
    timestep: Duration,
    deadline: Instant,
) -> Result<(GameSnapshot, Value), Failure> {
    let (meadow, instances) = session.observe(|world| {
        (
            CurrentScene::get(world).map(|scene| scene.receipt().clone()),
            world
                .resource::<WorldIdentityDomain>()
                .stats()
                .active_scene_instances,
        )
    })?;
    let meadow = meadow.ok_or("Server has no original Meadow Scene receipt")?;
    let mut driver = Driver {
        session,
        timestep,
        deadline,
        sequence: 0,
    };
    let queue = driver.queue()?;
    if queue.accepted != 0
        || queue.acknowledged != 0
        || startup.held != [false; 4]
        || startup.side_room != SideRoomPhase::Closed
        || startup.lookout != SideRoomPhase::Closed
    {
        return Err("Server region verification requires a fresh interactive game".into());
    }
    driver.submit(ServerCommand::SideRoom)?;
    let first_opened = driver.until(MAX_FRAMES, "Side Room to open", |snapshot| {
        snapshot.side_room == SideRoomPhase::Open
    })?;
    if first_opened.current_instance != startup.current_instance
        || first_opened.lookout != SideRoomPhase::Closed
    {
        return Err("Opening Side Room changed the main Scene or the closed Lookout".into());
    }
    let first_side_room = driver.room_receipt(RELIC)?;
    driver.verify_residency(instances + 1, &[&meadow, &first_side_room], &[])?;

    driver.submit(ServerCommand::Lookout)?;
    let opened = driver.until(MAX_FRAMES, "Lookout to open", |snapshot| {
        snapshot.lookout == SideRoomPhase::Open
    })?;
    if opened.current_instance != startup.current_instance
        || opened.side_room != SideRoomPhase::Open
    {
        return Err("Opening Lookout changed the main Scene or the open Side Room".into());
    }
    let lookout = driver.room_receipt(LOOKOUT_RELIC)?;
    driver.verify_residency(instances + 2, &[&meadow, &first_side_room, &lookout], &[])?;

    let side_room_found = driver.find_relic(2, |snapshot| snapshot.side_room)?;
    let lookout_found = driver.find_relic(3, |snapshot| snapshot.lookout)?;
    driver.submit(ServerCommand::SideRoom)?;
    let closed = driver.frame()?;
    if closed.side_room != SideRoomPhase::Closed
        || closed.lookout != SideRoomPhase::RelicFound
        || closed.current_instance != startup.current_instance
    {
        return Err("Closing Side Room did not preserve Lookout on the original Meadow".into());
    }
    driver.verify_residency(instances + 1, &[&meadow, &lookout], &[&first_side_room])?;

    driver.submit(ServerCommand::SideRoom)?;
    let reopened = driver.until(MAX_FRAMES, "Side Room to reopen", |snapshot| {
        snapshot.side_room == SideRoomPhase::Open
    })?;
    if reopened.current_instance != startup.current_instance
        || reopened.lookout != SideRoomPhase::RelicFound
    {
        return Err("Reopening Side Room did not preserve Lookout on the original Meadow".into());
    }
    let side_room = driver.room_receipt(RELIC)?;
    driver.verify_residency(
        instances + 2,
        &[&meadow, &side_room, &lookout],
        &[&first_side_room],
    )?;

    let target_x = west_contact(driver.session)?;
    if reopened.player.x <= target_x + CONTACT_TOLERANCE {
        return Err("Server actor started at or beyond the west-wall contact".into());
    }
    driver.movement(0, true)?;
    let contact = driver.until(
        MAX_MOVEMENT_TICKS,
        "contact with the west wall",
        |snapshot| snapshot.held[0] && (snapshot.player.x - target_x).abs() <= CONTACT_TOLERANCE,
    )?;
    let blocked = driver.frame()?;
    if !blocked.held[0]
        || blocked.tick != contact.tick + 1
        || blocked.player.distance(contact.player) > CONTACT_TOLERANCE
    {
        return Err(
            "Server actor continued moving into the wall while the command was held".into(),
        );
    }
    driver.movement(0, false)?;
    let released = driver.frame()?;
    if released.held[0]
        || released.current_instance != startup.current_instance
        || released.side_room != SideRoomPhase::Open
        || released.lookout != SideRoomPhase::RelicFound
    {
        return Err("Server did not consume the westward release with both regions open".into());
    }

    driver.verify_residency(instances + 2, &[&meadow, &side_room, &lookout], &[])?;
    driver.submit(ServerCommand::Gameplay(SemanticCommand::Travel))?;
    let travelled = driver.until(MAX_FRAMES, "the Cavern Travel to commit", |snapshot| {
        snapshot.phase == GamePhase::Running
            && snapshot.level == Some(LevelKey::main(LevelId::Cavern))
            && snapshot.travel_count == 1
    })?;
    if travelled.current_instance == startup.current_instance
        || travelled.score != 0
        || travelled.held != [false; 4]
        || travelled.side_room != SideRoomPhase::Closed
        || travelled.lookout != SideRoomPhase::Closed
    {
        return Err("Server Travel disagrees with the external command trajectory".into());
    }
    driver.verify_residency(instances, &[], &[&meadow, &side_room, &lookout])?;
    let queue = driver.queue()?;
    if queue.accepted != driver.sequence
        || queue.admitted != driver.sequence
        || queue.acknowledged != driver.sequence
        || queue.rejected != 0
        || queue.retained_commands != 0
    {
        return Err("Server did not acknowledge the exact external semantic command stream".into());
    }
    let evidence = json!({
        "source": SERVER_COMMAND_SOURCE,
        "commands_consumed": queue.acknowledged,
        "rooms": {
            "side_room": { "phase": side_room_found, "closed": travelled.side_room == SideRoomPhase::Closed },
            "lookout": { "phase": lookout_found, "closed": travelled.lookout == SideRoomPhase::Closed },
        },
        "collision": {
            "blocked": true,
            "wall": WEST_WALL,
            "contact_x": contact.player.x,
            "held_tick": blocked.tick,
        },
        "services_absent": true,
    });
    Ok((travelled, evidence))
}

struct Driver<'a> {
    session: &'a mut HeadlessSession,
    timestep: Duration,
    deadline: Instant,
    sequence: u64,
}

impl Driver<'_> {
    fn frame(&mut self) -> Result<GameSnapshot, Failure> {
        frame(self.session, self.timestep, self.deadline)
    }

    fn until(
        &mut self,
        ticks: usize,
        expected: &str,
        predicate: impl Fn(&GameSnapshot) -> bool,
    ) -> Result<GameSnapshot, Failure> {
        for _ in 0..ticks {
            let snapshot = self.frame()?;
            if predicate(&snapshot) {
                return Ok(snapshot);
            }
            std::thread::yield_now();
        }
        Err(Failure::Error(format!(
            "Server ran out of ticks waiting for {expected}"
        )))
    }

    fn queue(&self) -> Result<GameplayCommandQueueStats, Failure> {
        Ok(self
            .session
            .observe(|world| world.resource::<GameplayCommandQueue>().stats())?)
    }

    fn room_receipt(&self, relic: &str) -> Result<SpawnedSceneInstance, Failure> {
        self.session.observe(|world| {
            let id = SceneEntityId::new(relic).expect("static relic ID");
            let instance = world
                .try_query::<&SceneEntitySource>()
                .and_then(|mut query| {
                    query
                        .iter(world)
                        .find(|source| source.entity_id == id)
                        .map(|source| source.instance_id)
                })
                .ok_or_else(|| {
                    Failure::Error(format!("Server is missing the {relic} Scene source"))
                })?;
            world
                .resource::<WorldIdentityDomain>()
                .active_scene_instance(world, instance)
                .map_err(|error| {
                    Failure::Error(format!("Server cannot observe the {relic} Scene: {error}"))
                })
        })?
    }

    fn verify_residency(
        &self,
        expected: usize,
        live: &[&SpawnedSceneInstance],
        retired: &[&SpawnedSceneInstance],
    ) -> Result<(), Failure> {
        self.session.observe(|world| {
            let domain = world.resource::<WorldIdentityDomain>();
            let active = domain.stats().active_scene_instances;
            if active != expected {
                return Err(Failure::Error(format!(
                    "Server has {active} active Scenes, expected {expected}"
                )));
            }
            for &receipt in live {
                let instance = receipt.instance_id();
                if !matches!(domain.active_scene_instance(world, instance), Ok(current) if &current == receipt)
                {
                    return Err(Failure::Error(format!(
                        "Server Scene {instance:?} lost its original live members"
                    )));
                }
            }
            for &receipt in retired {
                let instance = receipt.instance_id();
                if !matches!(
                    domain.active_scene_instance(world, instance),
                    Err(IdentityDomainError::SceneInstanceNotActive { .. })
                ) || receipt
                    .entity_ids()
                    .iter()
                    .any(|id| !matches!(receipt.resolve(world, id), EntityLookup::Tombstoned(_)))
                {
                    return Err(Failure::Error(format!(
                        "Server Scene {instance:?} did not retire all its members"
                    )));
                }
            }
            Ok(())
        })?
    }

    fn find_relic(
        &mut self,
        direction: usize,
        phase: impl Fn(&GameSnapshot) -> SideRoomPhase,
    ) -> Result<SideRoomPhase, Failure> {
        self.movement(direction, true)?;
        self.until(MAX_MOVEMENT_TICKS, "the region relic", |snapshot| {
            phase(snapshot) == SideRoomPhase::RelicFound
        })?;
        self.movement(direction, false)?;
        let released = self.frame()?;
        if released.held[direction] || phase(&released) != SideRoomPhase::RelicFound {
            return Err(
                "Server did not retain the relic observation after releasing movement".into(),
            );
        }
        Ok(phase(&released))
    }

    fn movement(&mut self, direction: usize, pressed: bool) -> Result<(), Failure> {
        self.submit(ServerCommand::Gameplay(SemanticCommand::Move {
            direction,
            pressed,
        }))
    }

    fn submit(&mut self, command: ServerCommand) -> Result<(), Failure> {
        let draft = command.into_draft().map_err(Failure::Error)?;
        submit_external(self.session, &mut self.sequence, draft)?;
        Ok(())
    }
}

fn west_contact(session: &HeadlessSession) -> Result<f32, Failure> {
    session.observe(|world| -> Result<f32, Failure> {
        let current =
            CurrentScene::get(world).ok_or("Server has no active Meadow collision Scene")?;
        let collider = |name: &str| -> Result<(Transform2d, CollisionBox), Failure> {
            let id = SceneEntityId::new(name).expect("static authored entity ID");
            let EntityLookup::Resolved(entity) = current.receipt().resolve(world, &id) else {
                return Err(Failure::Error(format!(
                    "Server is missing its live {name} collider"
                )));
            };
            let transform = world
                .get::<Transform2d>(entity)
                .ok_or("collider transform is missing")?;
            let shape = world
                .get::<CollisionBox>(entity)
                .ok_or("collision box is missing")?;
            if transform.rotation != 0.0 || transform.scale != Vec2::ONE {
                return Err(
                    "Server west-wall contact fixture must have axis-aligned unit-scale colliders"
                        .into(),
                );
            }
            Ok((*transform, *shape))
        };
        let (west_wall, wall_box) = collider(WEST_WALL)?;
        let (_, actor_box) = collider("explorer")?;
        Ok(
            west_wall.translation.x + wall_box.center.x + wall_box.half_width - actor_box.center.x
                + actor_box.half_width
                + CONTACT_MARGIN,
        )
    })?
}
