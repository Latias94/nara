mod markers;
mod reconstruction;

use markers::add_marker;
pub(super) use markers::{DynamicMarker, MarkerInstance, observe_markers};
pub(super) use reconstruction::reconstruct;

use std::collections::BTreeSet;

use nara::{
    advanced_prelude::{
        CurrentScene, commit_prepared_project_scene, restart_current_scene_with_product,
    },
    core::ItemLimit,
    ecs::World,
    gameplay::{
        GameplayCommandBatch, GameplayCommandKey, GameplayCommandPayload, GameplayCommandSource,
    },
    identity::SceneEntityId,
    prelude::Resource,
    project_host::{
        ProjectSceneRequestId, ProjectSceneRequestState, ProjectSceneSelection, ProjectScenes,
    },
    scene::advanced::{SceneProductRetirement, SceneProductTransactionLimits},
};

use super::{
    ActiveLevel, GamePhase, Progress, ServerCommandRejections, Session,
    commands::*,
    content::Content,
    fail,
    side_room::{self, Room, Rooms},
    storage::Storage,
};
use crate::{
    command::{SERVER_COMMAND_SOURCE, decode_movement},
    levels::{LevelId, LevelKey},
    resume::LevelContentStamp,
    save::{Checkpoint, CheckpointLocation, MAX_MARKERS, io::CheckpointBinding},
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
    content: Vec<LevelContentStamp>,
    request: Option<ProjectSceneRequestId>,
    kind: TravelKind,
}

pub(super) enum TravelKind {
    Travel,
    Resume,
    RestoreCheckpoint(CheckpointBinding),
}

enum ConsumedInput {
    Movement(Result<(usize, bool), String>),
    Action(String, GameplayCommandPayload),
}

pub(super) fn consume(world: &mut World) {
    let launch_error = super::player_launch::consume(world).err();
    if let Some(error) = &launch_error {
        let mut session = world.resource_mut::<Session>();
        if session.local_player {
            eprintln!("two_level_game: Player launch rejected: {error}");
        }
        session.error = Some(error.clone());
    }
    #[cfg(feature = "local-player")]
    let (blocked, mut round_saves) = if world.resource::<Session>().local_player {
        super::flow::fixed_input(world)
    } else {
        (false, 0)
    };
    #[cfg(not(feature = "local-player"))]
    let blocked = false;
    let observe_rejections = world.contains_resource::<ServerCommandRejections>();
    let commands = world
        .resource::<GameplayCommandBatch>()
        .commands()
        .iter()
        .filter_map(|command| {
            let input = match command.command_type().as_str() {
                MOVE => ConsumedInput::Movement(decode_movement(command)),
                ROUND_SAVE => {
                    #[cfg(feature = "local-player")]
                    if round_saves != 0 && command.source() == &GameplayCommandSource::LocalAction {
                        round_saves -= 1;
                        return Some((
                            ConsumedInput::Action(SAVE.to_owned(), command.payload().clone()),
                            None,
                        ));
                    }
                    return None;
                }
                PLAYER_LAUNCH => return None,
                _ if blocked => return None,
                kind => ConsumedInput::Action(kind.to_owned(), command.payload().clone()),
            };
            let server_key = if observe_rejections
                && let GameplayCommandSource::External { producer } = command.source()
                && producer.as_str() == SERVER_COMMAND_SOURCE
            {
                Some(command.key().clone())
            } else {
                None
            };
            Some((input, server_key))
        })
        .collect::<Vec<_>>();
    for (command, server_key) in commands {
        let (kind, payload) = match command {
            ConsumedInput::Movement(Ok((direction, pressed))) => {
                if pressed {
                    if blocked {
                        continue;
                    }
                    let phase = world.resource::<Session>().phase;
                    if phase != GamePhase::Running {
                        record_phase_rejection(world, server_key, phase);
                        continue;
                    }
                }
                world.resource_mut::<Session>().held[direction] = pressed;
                continue;
            }
            ConsumedInput::Movement(Err(_)) if blocked => continue,
            ConsumedInput::Movement(Err(error)) => {
                reject_action(world, server_key, error);
                continue;
            }
            ConsumedInput::Action(kind, payload) => (kind, payload),
        };
        let phase = world.resource::<Session>().phase;
        if kind == RETRY && phase == GamePhase::Degraded {
            world.resource_mut::<Session>().phase = GamePhase::Reconstructing;
            match reconstruct(world) {
                Ok(()) => {
                    let mut session = world.resource_mut::<Session>();
                    session.held = [false; 4];
                    session.phase = GamePhase::Running;
                    session.error = None;
                }
                Err(error) => {
                    record_server_rejection(world, server_key, &error);
                    fail(world, error);
                }
            }
            continue;
        }
        if phase != GamePhase::Running {
            record_phase_rejection(world, server_key, phase);
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
                begin(world, checkpoint, TravelKind::Travel)
            }
            RETRY => retry_current(world),
            MARKER => add_marker(world),
            SIDE_ROOM => side_room::toggle(world, Room::SideRoom),
            LOOKOUT => side_room::toggle(world, Room::Lookout),
            SAVE => save_checkpoint(world),
            RESTORE => match world.resource::<Storage>().prepare_load() {
                Ok((checkpoint, binding)) => {
                    begin(world, checkpoint, TravelKind::RestoreCheckpoint(binding))
                }
                Err(error) => Err(error),
            },
            _ => Ok(()),
        };
        if let Err(error) = result {
            reject_action(world, server_key, error);
        } else if world.resource::<Session>().phase == GamePhase::Running {
            world.resource_mut::<Session>().error = None;
        }
    }
    if let Some(error) = launch_error {
        world.resource_mut::<Session>().error = Some(error);
    }
}

fn reject_action(world: &mut World, key: Option<GameplayCommandKey>, error: String) {
    record_server_rejection(world, key, &error);
    let mut session = world.resource_mut::<Session>();
    if session.local_player {
        eprintln!("two_level_game: action rejected: {error}");
    }
    session.error = Some(error);
}

fn record_phase_rejection(world: &mut World, key: Option<GameplayCommandKey>, phase: GamePhase) {
    let message = match phase {
        GamePhase::Running => return,
        GamePhase::Preparing => "command skipped while level preparation is pending",
        GamePhase::Reconstructing => "command skipped during level reconstruction",
        GamePhase::Degraded => "command skipped while gameplay is degraded",
    };
    record_server_rejection(world, key, message);
}

fn record_server_rejection(world: &mut World, key: Option<GameplayCommandKey>, message: &str) {
    if let Some(key) = key {
        world
            .resource_mut::<ServerCommandRejections>()
            .record(key, message);
    }
}

fn save_checkpoint(world: &mut World) -> Result<(), String> {
    let checkpoint = world.resource::<Progress>().checkpoint();
    world.resource::<Storage>().save(checkpoint)?;
    if world.resource::<Session>().local_player {
        println!(
            "two_level_game: checkpoint file published and verified at {:?} score={}",
            checkpoint.location.level, checkpoint.score
        );
    }
    let mut session = world.resource_mut::<Session>();
    session.saves = session.saves.saturating_add(1);
    Ok(())
}

fn retry_current(world: &mut World) -> Result<(), String> {
    Rooms::require_closed(world)?;
    let key = world.resource::<ActiveLevel>().key();
    let progress = world.resource::<Progress>().checkpoint().clone();
    let (expected, content) = {
        let current = CurrentScene::get(world).ok_or("current Scene is unavailable")?;
        let content = Content::read_current(world)?;
        content.validate_progress(key.level(), &progress)?;
        (current.receipt().clone(), content)
    };
    let marker_receipts = world
        .resource::<Session>()
        .markers
        .iter()
        .map(|marker| marker.receipt.clone())
        .collect::<Vec<_>>();
    let markers = marker_receipts
        .iter()
        .map(SceneProductRetirement::Instance)
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
    #[cfg(all(windows, feature = "audio-cpal", feature = "local-player"))]
    if world.resource::<Session>().local_player {
        crate::audio::scene_replaced(world);
    }
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
    kind: TravelKind,
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
        content: Vec::new(),
        request: None,
        kind,
    });
    let mut session = world.resource_mut::<Session>();
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
        let selection = match CurrentScene::get(world) {
            #[cfg(feature = "local-player")]
            Some(current) if current.source_view().with_runtime_scene(|_| ()).is_none() => {
                ProjectSceneSelection::Source(level.scene_path())
            }
            Some(_) => ProjectSceneSelection::Product(level.scene_product()),
            None => {
                reject(world, "current Scene is unavailable".into());
                return;
            }
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
    let local_player = world.resource::<Session>().local_player;
    #[cfg(all(windows, feature = "audio-cpal", feature = "local-player"))]
    let audio_bank = if local_player {
        crate::audio::bank_guard(world)
    } else {
        None
    };
    let prepared = ProjectScenes::scope(world, |scenes| match scenes.state(request) {
        Some(ProjectSceneRequestState::Ready) => scenes.prepared(request).map(|view| {
            let marker = view
                .scene_dependency(crate::markers::marker_product())
                .ok_or("prepared level lacks its Immediate marker Scene dependency")?;
            let local_assets = local_player.then(|| view.domain_assets());
            let content = Content::read(view.scene(), local_assets, marker)?;
            #[cfg(all(windows, feature = "audio-cpal", feature = "local-player"))]
            if let Some(bank) = &audio_bank {
                let presentation = content
                    .presentation
                    .as_ref()
                    .ok_or("prepared level lacks its Sound Bank")?;
                bank.validate(view.domain_assets(), &presentation.bank)?;
            }
            #[cfg(feature = "local-player")]
            let source = view.source_content_stamp().map(|source| {
                (
                    super::resume::stamp(level, source),
                    source.schema_fingerprint().as_bytes(),
                )
            });
            #[cfg(not(feature = "local-player"))]
            let source = None;
            Ok((content, source))
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
    let (content, source) = match prepared {
        Ok(content) => content,
        Err(error) => {
            cancel_ack(world, request);
            reject(world, error);
            return;
        }
    };
    let stamp = match super::resume::validate_content(world, source) {
        Ok(stamp) => stamp,
        Err(error) => {
            cancel_ack(world, request);
            reject(world, error);
            return;
        }
    };
    let restore = !matches!(pending.kind, TravelKind::Travel);
    if !restore && level == pending.target.level() {
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
    pending.content.extend(stamp);
    if !pending.validate.is_empty() {
        cancel_ack(world, request);
        pending.validate.pop();
        pending.request = None;
        world.resource_mut::<Travel>().pending = Some(pending);
        return;
    }
    let marker_receipts = world
        .resource::<Session>()
        .markers
        .iter()
        .map(|marker| marker.receipt.clone())
        .collect::<Vec<_>>();
    let old_markers = marker_receipts
        .iter()
        .map(SceneProductRetirement::Instance)
        .collect::<Vec<_>>();
    let retirement_members = MAX_MARKERS * 16 + world.resource::<Rooms>().live_members();
    let report = commit_prepared_project_scene(
        world,
        request,
        SceneProductTransactionLimits::new(
            ItemLimit::new(4).unwrap(),
            ItemLimit::new(retirement_members).unwrap(),
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
    super::resume::commit_content(world, pending.content);
    *world.resource_mut::<Rooms>() = Rooms::default();
    #[cfg(all(windows, feature = "audio-cpal", feature = "local-player"))]
    if local_player {
        crate::audio::scene_replaced(world);
    }
    ack(world, request);
    {
        let mut session = world.resource_mut::<Session>();
        session.markers.clear();
        session.phase = GamePhase::Reconstructing;
        session.travel_count = session.travel_count.saturating_add(1);
        session.restores = session.restores.saturating_add(u32::from(restore));
    }
    match reconstruct(world) {
        Ok(()) => {
            if let TravelKind::RestoreCheckpoint(binding) = pending.kind {
                if let Err(error) = world.resource::<Storage>().accept_load(binding) {
                    fail(
                        world,
                        format!("restored checkpoint baseline changed: {error}"),
                    );
                    return;
                }
            }
            let held = [false; 4];
            #[cfg(feature = "local-player")]
            let held = if local_player
                && super::flow::simulation_enabled(world)
                && !super::resume::is_pending(world)
            {
                super::input::held_movement(world)
            } else {
                held
            };
            let mut session = world.resource_mut::<Session>();
            session.held = held;
            session.phase = GamePhase::Running;
            #[cfg(all(windows, feature = "audio-cpal", feature = "local-player"))]
            if local_player && !restore {
                crate::audio::travel_entered(world, pending.target);
            }
            if local_player {
                println!(
                    "two_level_game: entered {:?} score={} restored={}",
                    pending.target, pending.checkpoint.score, restore
                );
            }
        }
        Err(error) => fail(world, error),
    }
}

fn reject(world: &mut World, error: String) {
    super::resume::reject_pending(world);
    let mut session = world.resource_mut::<Session>();
    if session.local_player {
        eprintln!(
            "two_level_game: level preparation/commit rejected; current game retained: {error}"
        );
    }
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
#[path = "travel/tests/admission.rs"]
mod admission_tests;
#[cfg(all(test, feature = "local-player"))]
mod tests;
