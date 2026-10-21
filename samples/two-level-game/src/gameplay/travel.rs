mod reconstruction;

use reconstruction::add_marker;
pub(super) use reconstruction::{DynamicMarker, hide_collected, observe_markers, reconstruct};

use std::collections::BTreeSet;

use nara::{
    advanced_prelude::{
        CurrentScene, commit_prepared_project_scene, restart_current_scene_with_product,
    },
    core::ItemLimit,
    ecs::{World, system::SystemState},
    gameplay::{GameplayCommandBatch, GameplayCommandValue},
    identity::SceneEntityId,
    prelude::Resource,
    project_host::{ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes},
    scene::advanced::SceneProductTransactionLimits,
};

use super::{
    ActiveLevel, GamePhase, Progress, Session, commands::*, content::Content, fail,
    storage::Storage,
};
use crate::{
    levels::{LevelId, LevelKey},
    save::{Checkpoint, CheckpointLocation, MAX_MARKERS},
};

#[derive(Resource, Default)]
pub(super) struct Travel {
    pending: Option<Pending>,
}
struct Pending {
    checkpoint: Checkpoint,
    target: LevelKey,
    validate: Vec<LevelId>,
    request: Option<ProjectSceneRequestId>,
    restore: bool,
}

pub(super) fn consume(world: &mut World) {
    let commands = world
        .resource::<GameplayCommandBatch>()
        .commands()
        .iter()
        .map(|command| {
            (
                command.command_type().as_str().to_owned(),
                command.payload().clone(),
            )
        })
        .collect::<Vec<_>>();
    for (kind, payload) in commands {
        if kind == MOVE {
            if let (
                Some(GameplayCommandValue::I64(direction)),
                Some(GameplayCommandValue::Bool(pressed)),
            ) = (payload.get("direction"), payload.get("pressed"))
                && let Ok(index) = usize::try_from(*direction)
                && (!*pressed || world.resource::<Session>().phase == GamePhase::Running)
                && let Some(held) = world.resource_mut::<Session>().held.get_mut(index)
            {
                *held = *pressed;
            }
            continue;
        }
        if kind == RETRY && world.resource::<Session>().phase == GamePhase::Degraded {
            world.resource_mut::<Session>().phase = GamePhase::Reconstructing;
            match reconstruct(world) {
                Ok(()) => {
                    let mut session = world.resource_mut::<Session>();
                    session.held = [false; 4];
                    session.phase = GamePhase::Running;
                    session.error = None;
                }
                Err(error) => fail(world, error),
            }
            continue;
        }
        if world.resource::<Session>().phase != GamePhase::Running {
            continue;
        }
        let result = match kind.as_str() {
            TRAVEL | INSTANCE => {
                let key = world.resource::<ActiveLevel>().key;
                let target = if kind == INSTANCE {
                    LevelKey::new(key.level(), if key.instance() == 0 { 1 } else { 0 }).unwrap()
                } else {
                    LevelKey::main(if key.level() == LevelId::Meadow {
                        LevelId::Cavern
                    } else {
                        LevelId::Meadow
                    })
                };
                let mut checkpoint = world.resource::<Progress>().0.clone();
                checkpoint.location = CheckpointLocation {
                    level: target,
                    anchor: SceneEntityId::new("checkpoint").unwrap(),
                };
                begin(world, checkpoint, false)
            }
            RETRY => retry_current(world),
            MARKER => add_marker(world),
            SAVE => {
                let checkpoint = world.resource::<Progress>().0.clone();
                let result = world.resource::<Storage>().save(&checkpoint);
                if result.is_ok() {
                    world.resource_mut::<Session>().saves += 1;
                    println!(
                        "two_level_game: checkpoint file published and verified at {:?} score={}",
                        checkpoint.location.level, checkpoint.score
                    );
                }
                result
            }
            RESTORE => match world.resource::<Storage>().load() {
                Ok(checkpoint) => begin(world, checkpoint, true),
                Err(error) => Err(error),
            },
            _ => Ok(()),
        };
        if let Err(error) = result {
            eprintln!("two_level_game: action rejected: {error}");
            world.resource_mut::<Session>().error = Some(error);
        } else if world.resource::<Session>().phase == GamePhase::Running {
            world.resource_mut::<Session>().error = None;
        }
    }
}

fn retry_current(world: &mut World) -> Result<(), String> {
    let key = world.resource::<ActiveLevel>().key;
    let progress = world.resource::<Progress>().0.clone();
    let (expected, content) = {
        let mut access = SystemState::<CurrentScene>::new(world);
        let current = access.get(world).map_err(|error| error.to_string())?;
        let content = Content::read(current.source())?;
        content.validate_progress(key.level(), &progress)?;
        (current.receipt().clone(), content)
    };
    let markers = world.resource::<Session>().markers.clone();
    let report = restart_current_scene_with_product(
        world,
        &expected,
        SceneProductTransactionLimits::new(
            ItemLimit::new(4).unwrap(),
            ItemLimit::new(MAX_MARKERS).unwrap(),
        ),
        &markers,
        |writer| {
            writer
                .replace_resource(Progress(progress))
                .replace_resource(ActiveLevel {
                    key,
                    content: Some(content),
                });
        },
    );
    if report.diagnostics.has_errors() || report.instance.is_none() {
        return Err(format!(
            "current level restart rejected: {:?}",
            report.diagnostics
        ));
    }
    {
        let mut session = world.resource_mut::<Session>();
        session.markers.clear();
        session.held = [false; 4];
        session.restarts += 1;
        session.phase = GamePhase::Reconstructing;
    }
    match reconstruct(world) {
        Ok(()) => world.resource_mut::<Session>().phase = GamePhase::Running,
        Err(error) => fail(world, error),
    }
    Ok(())
}

fn begin(world: &mut World, checkpoint: Checkpoint, restore: bool) -> Result<(), String> {
    checkpoint.validate().map_err(|error| error.to_string())?;
    if world.resource::<Travel>().pending.is_some() {
        return Err("level preparation is already pending".into());
    }
    let mut validate = BTreeSet::new();
    validate.extend(
        checkpoint
            .collected
            .iter()
            .map(|pickup| pickup.level.level()),
    );
    validate.extend(checkpoint.markers.iter().map(|marker| marker.level.level()));
    // Validate other sources first; keep the final target's prepared generation for commit.
    validate.remove(&checkpoint.location.level.level());
    world.resource_mut::<Travel>().pending = Some(Pending {
        target: checkpoint.location.level,
        checkpoint,
        validate: validate.into_iter().collect(),
        request: None,
        restore,
    });
    let mut session = world.resource_mut::<Session>();
    session.held = [false; 4];
    session.phase = GamePhase::Preparing;
    session.error = None;
    Ok(())
}

pub(super) fn advance(world: &mut World) {
    let Some(mut pending) = world.resource_mut::<Travel>().pending.take() else {
        return;
    };
    let level = pending
        .validate
        .last()
        .copied()
        .unwrap_or(pending.target.level());
    if pending.request.is_none() {
        let mut access = SystemState::<ProjectScenes>::new(world);
        let result = access.get_mut(world).unwrap().request(level.scene_path());
        match result {
            Ok(request) => pending.request = Some(request),
            Err(error) => {
                reject(world, error.to_string());
                return;
            }
        }
        world.resource_mut::<Travel>().pending = Some(pending);
        return;
    }
    let request = pending.request.unwrap();
    let prepared = {
        let mut access = SystemState::<ProjectScenes>::new(world);
        let scenes = access.get_mut(world).unwrap();
        match scenes.state(request) {
            Some(ProjectSceneRequestState::Ready) => scenes
                .prepared(request)
                .map(|view| Content::read(view.scene())),
            Some(ProjectSceneRequestState::Preparing | ProjectSceneRequestState::Cancelling) => {
                None
            }
            state => Some(Err(format!(
                "level preparation rejected: {state:?}; {:?}",
                scenes.diagnostics(request)
            ))),
        }
    };
    let Some(prepared) = prepared else {
        world.resource_mut::<Travel>().pending = Some(pending);
        return;
    };
    let content = match prepared {
        Ok(content) => content,
        Err(error) => {
            cancel_ack(world, request);
            reject(world, error);
            return;
        }
    };
    if !pending.restore && level == pending.target.level() {
        pending.checkpoint.location.anchor = content.checkpoint.clone();
    }
    if let Err(error) = content.validate_progress(level, &pending.checkpoint) {
        cancel_ack(world, request);
        reject(world, error);
        return;
    }
    if !pending.validate.is_empty() {
        // Only ordinary owned IDs/positions survive this inspection. Drop the decoded generation
        // before requesting the next source, preserving the Host's two-generation budget.
        cancel_ack(world, request);
        pending.validate.pop();
        pending.request = None;
        world.resource_mut::<Travel>().pending = Some(pending);
        return;
    }
    let old_markers = world.resource::<Session>().markers.clone();
    let report = commit_prepared_project_scene(
        world,
        request,
        SceneProductTransactionLimits::new(
            ItemLimit::new(4).unwrap(),
            ItemLimit::new(MAX_MARKERS).unwrap(),
        ),
        &old_markers,
        |writer| {
            writer
                .replace_resource(Progress(pending.checkpoint.clone()))
                .replace_resource(ActiveLevel {
                    key: pending.target,
                    content: Some(content),
                });
        },
    );
    if report.diagnostics.has_errors() || report.instance.is_none() {
        // Early owner preflight refusal can retain Ready for an explicit retry. This game aborts
        // that attempt, so revoke exactly its request before acknowledging its terminal result.
        cancel_ack(world, request);
        reject(
            world,
            format!("level commit rejected: {:?}", report.diagnostics),
        );
        return;
    }
    ack(world, request);
    {
        let mut session = world.resource_mut::<Session>();
        session.markers.clear();
        session.phase = GamePhase::Reconstructing;
        session.travel_count += 1;
        session.restores += u32::from(pending.restore);
    }
    match reconstruct(world) {
        Ok(()) => {
            world.resource_mut::<Session>().phase = GamePhase::Running;
            println!(
                "two_level_game: entered {:?} score={} restored={}",
                pending.target, pending.checkpoint.score, pending.restore
            );
        }
        Err(error) => fail(world, error),
    }
}

fn reject(world: &mut World, error: String) {
    eprintln!("two_level_game: level preparation/commit rejected; current game retained: {error}");
    let mut session = world.resource_mut::<Session>();
    session.phase = GamePhase::Running;
    session.error = Some(error);
}
fn ack(world: &mut World, request: ProjectSceneRequestId) {
    let mut access = SystemState::<ProjectScenes>::new(world);
    access.get_mut(world).unwrap().acknowledge(request);
}
fn cancel_ack(world: &mut World, request: ProjectSceneRequestId) {
    let mut access = SystemState::<ProjectScenes>::new(world);
    let mut scenes = access.get_mut(world).unwrap();
    scenes.cancel(request);
    scenes.acknowledge(request);
}

#[cfg(test)]
mod tests;
