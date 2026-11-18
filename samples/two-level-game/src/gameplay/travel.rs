mod markers;
mod reconstruction;

use markers::add_marker;
pub(super) use markers::{DynamicMarker, MarkerInstance, observe_markers};
pub(super) use reconstruction::{hide_collected, reconstruct};

use std::collections::BTreeSet;

use nara::{
    advanced_prelude::{
        CurrentScene, commit_prepared_project_scene, restart_current_scene_with_product,
    },
    core::ItemLimit,
    ecs::World,
    gameplay::{GameplayCommandBatch, GameplayCommandValue},
    identity::SceneEntityId,
    prelude::Resource,
    project_host::{
        ProjectSceneRequestId, ProjectSceneRequestState, ProjectSceneSelection, ProjectScenes,
    },
    scene::advanced::{SceneProductRetirement, SceneProductTransactionLimits},
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
impl Travel {
    pub(super) fn is_idle(&self) -> bool {
        self.pending.is_none()
    }
}
struct Pending {
    checkpoint: Checkpoint,
    target: LevelKey,
    validate: Vec<LevelId>,
    request: Option<ProjectSceneRequestId>,
    restore: bool,
}

pub(super) fn consume(world: &mut World) {
    let launch_error = super::player_launch::consume(world).err();
    if let Some(error) = &launch_error {
        eprintln!("two_level_game: Player launch rejected: {error}");
        world.resource_mut::<Session>().error = Some(error.clone());
    }
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
        if kind == PLAYER_LAUNCH {
            continue;
        }
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
            super::resume::RESTORE_STATE => {
                let result = super::resume::begin_from_command(world, &payload);
                if result.is_err() {
                    super::resume::reject(world);
                }
                result
            }
            TRAVEL | INSTANCE => {
                let key = world.resource::<ActiveLevel>().key();
                let target = if kind == INSTANCE {
                    LevelKey::new(key.level(), if key.instance() == 0 { 1 } else { 0 }).unwrap()
                } else {
                    LevelKey::main(if key.level() == LevelId::Meadow {
                        LevelId::Cavern
                    } else {
                        LevelId::Meadow
                    })
                };
                let mut checkpoint = world.resource::<Progress>().checkpoint().clone();
                checkpoint.location = CheckpointLocation {
                    level: target,
                    anchor: SceneEntityId::new("checkpoint").unwrap(),
                };
                begin(world, checkpoint, false)
            }
            RETRY => retry_current(world),
            MARKER => add_marker(world),
            SAVE => {
                let checkpoint = world.resource::<Progress>().checkpoint().clone();
                let result = world.resource::<Storage>().save(&checkpoint);
                if result.is_ok() {
                    let mut session = world.resource_mut::<Session>();
                    session.saves = session.saves.saturating_add(1);
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
    if let Some(error) = launch_error {
        world.resource_mut::<Session>().error = Some(error);
    }
}

fn retry_current(world: &mut World) -> Result<(), String> {
    let key = world.resource::<ActiveLevel>().key();
    let progress = world.resource::<Progress>().checkpoint().clone();
    let (expected, content) = {
        let current = CurrentScene::get(world).ok_or("current Scene is unavailable")?;
        let content = Content::read_current(world)?;
        content.validate_progress(key.level(), &progress)?;
        (current.receipt().clone(), content)
    };
    let markers = world
        .resource::<Session>()
        .markers
        .iter()
        .map(|marker| SceneProductRetirement::Instance(marker.receipt.clone()))
        .collect::<Vec<_>>();
    let report = restart_current_scene_with_product(
        world,
        &expected,
        SceneProductTransactionLimits::new(
            ItemLimit::new(4).unwrap(),
            ItemLimit::new(MAX_MARKERS * 16).unwrap(),
        ),
        &markers,
        |writer| {
            writer
                .replace_resource(Progress::new(progress))
                .replace_resource(ActiveLevel::new(key, Some(content)));
        },
    );
    if report.diagnostics.has_errors() || report.instance.is_none() {
        return Err(format!(
            "current level restart rejected: {:?}",
            report.diagnostics
        ));
    }
    #[cfg(all(windows, feature = "audio-cpal"))]
    crate::audio::scene_replaced(world);
    {
        let mut session = world.resource_mut::<Session>();
        session.markers.clear();
        session.held = [false; 4];
        session.restarts = session.restarts.saturating_add(1);
        session.phase = GamePhase::Reconstructing;
    }
    match reconstruct(world) {
        Ok(()) => world.resource_mut::<Session>().phase = GamePhase::Running,
        Err(error) => fail(world, error),
    }
    Ok(())
}

pub(super) fn begin(
    world: &mut World,
    checkpoint: Checkpoint,
    restore: bool,
) -> Result<(), String> {
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
    validate.extend(super::resume::required_levels(world));
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
        let Some(current) = CurrentScene::get(world) else {
            reject(world, "current Scene is unavailable".into());
            return;
        };
        let selection = if current.source_view().with_runtime_scene(|_| ()).is_some() {
            ProjectSceneSelection::Product(level.scene_product())
        } else {
            ProjectSceneSelection::Source(level.scene_path())
        };
        let result = ProjectScenes::scope(world, |scenes| scenes.request(selection));
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
    #[cfg(all(windows, feature = "audio-cpal"))]
    let audio_bank = crate::audio::bank_guard(world);
    let prepared = ProjectScenes::scope(world, |scenes| match scenes.state(request) {
        Some(ProjectSceneRequestState::Ready) => scenes.prepared(request).map(|view| {
            let marker = view
                .scene_dependency(crate::markers::marker_product())
                .ok_or("prepared level lacks its Immediate marker Scene dependency")?;
            let content = Content::read(view.scene(), view.domain_assets(), marker)?;
            #[cfg(all(windows, feature = "audio-cpal"))]
            if let Some(bank) = &audio_bank {
                bank.validate(view.domain_assets(), &content.bank)?;
            }
            Ok((content, view.source_content_stamp()))
        }),
        Some(ProjectSceneRequestState::Preparing | ProjectSceneRequestState::Cancelling) => None,
        state => Some(Err(format!(
            "level preparation rejected: {state:?}; {:?}",
            scenes.diagnostics(request)
        ))),
    });
    let Some(prepared) = prepared else {
        world.resource_mut::<Travel>().pending = Some(pending);
        return;
    };
    let (content, stamp) = match prepared {
        Ok(content) => content,
        Err(error) => {
            cancel_ack(world, request);
            reject(world, error);
            return;
        }
    };
    if let Err(error) = super::resume::validate_content(world, level, stamp) {
        cancel_ack(world, request);
        reject(world, error);
        return;
    }
    if !pending.restore && level == pending.target.level() {
        pending.checkpoint.location.anchor = content.checkpoint.clone();
    }
    if let Err(error) = content.validate_progress(level, &pending.checkpoint) {
        cancel_ack(world, request);
        reject(world, error);
        return;
    }
    if let Err(error) = super::resume::validate_prepared_placement(world, level, &content) {
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
    let old_markers = world
        .resource::<Session>()
        .markers
        .iter()
        .map(|marker| SceneProductRetirement::Instance(marker.receipt.clone()))
        .collect::<Vec<_>>();
    let report = commit_prepared_project_scene(
        world,
        request,
        SceneProductTransactionLimits::new(
            ItemLimit::new(4).unwrap(),
            ItemLimit::new(MAX_MARKERS * 16).unwrap(),
        ),
        &old_markers,
        |writer| {
            writer
                .replace_resource(Progress::new(pending.checkpoint.clone()))
                .replace_resource(ActiveLevel::new(pending.target, Some(content)));
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
    #[cfg(all(windows, feature = "audio-cpal"))]
    crate::audio::scene_replaced(world);
    ack(world, request);
    {
        let mut session = world.resource_mut::<Session>();
        session.markers.clear();
        session.phase = GamePhase::Reconstructing;
        session.travel_count = session.travel_count.saturating_add(1);
        session.restores = session.restores.saturating_add(u32::from(pending.restore));
    }
    match reconstruct(world) {
        Ok(()) => {
            world.resource_mut::<Session>().phase = GamePhase::Running;
            #[cfg(all(windows, feature = "audio-cpal"))]
            if !pending.restore {
                crate::audio::travel_entered(world, pending.target);
            }
            println!(
                "two_level_game: entered {:?} score={} restored={}",
                pending.target, pending.checkpoint.score, pending.restore
            );
        }
        Err(error) => fail(world, error),
    }
}

fn reject(world: &mut World, error: String) {
    super::resume::reject_pending(world);
    eprintln!("two_level_game: level preparation/commit rejected; current game retained: {error}");
    let mut session = world.resource_mut::<Session>();
    session.phase = GamePhase::Running;
    session.error = Some(error);
}
fn ack(world: &mut World, request: ProjectSceneRequestId) {
    ProjectScenes::scope(world, |scenes| scenes.acknowledge(request));
}
fn cancel_ack(world: &mut World, request: ProjectSceneRequestId) {
    ProjectScenes::scope(world, |scenes| {
        scenes.cancel(request);
        scenes.acknowledge(request);
    });
}

#[cfg(test)]
mod tests;
