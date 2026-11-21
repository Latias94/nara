//! Game-owned exact state over the original Headless, command and Scene owners.

mod capture;
#[cfg(feature = "desktop")]
mod desktop;
mod processed;

use std::collections::BTreeMap;
use std::time::Duration;

#[cfg(feature = "desktop")]
use nara::project_host::DesktopControlError;
#[cfg(feature = "local-player")]
use nara::{advanced_prelude::CurrentScene, project_host::ProjectSceneContentStamp};
use nara::{
    app::{FixedTime, RuntimeState, RuntimeTimeContinuation, RuntimeTimeSettings},
    ecs::World,
    gameplay::{
        GameplayCommandBatch, GameplayCommandDraft, GameplayCommandIngressSource,
        GameplayCommandPayload, GameplayCommandQueue, GameplayCommandSourceSequence,
        GameplayCommandSubmission, GameplayCommandTick, GameplayCommandTypeId,
        GameplayCommandValue,
    },
    prelude::{App, PluginError, Resource, Transform2d, Vec2},
    project_host::{HeadlessSession, HeadlessSessionError},
};

use super::{GamePhase, Progress, Session, fail, player, side_room, travel};
use crate::{
    levels::LevelId,
    resume::{
        LevelContentStamp, ResumeCompatibility, ResumeError, ResumeState, RuntimeTimeState,
        decode_resume, encode_resume,
        processed::{ProcessedResumeError, ProcessedRooms, decode_processed_resume},
    },
    save::io::SaveIoError,
};

#[cfg(feature = "desktop")]
pub use capture::capture_desktop_resume;
pub use capture::capture_resume;
#[cfg(all(feature = "desktop", feature = "development"))]
pub(crate) use capture::capture_world;
#[cfg(feature = "desktop")]
pub use desktop::restore_desktop_resume;
pub use processed::{
    capture_processed_resume, claim_processed_checkpoint, restore_processed_resume,
};
pub(super) const RESTORE_STATE: &str = "two-level-game.resume-state";
const RESUME_SOURCE: &str = "two-level-game.resume";

/// Exact reconstruction refusal. A separate candidate never modifies the retained incumbent.
#[derive(Debug, thiserror::Error)]
pub enum GameResumeError {
    #[error(transparent)]
    Session(#[from] HeadlessSessionError),
    #[cfg(feature = "desktop")]
    #[error(transparent)]
    Desktop(#[from] DesktopControlError),
    #[error(transparent)]
    Format(#[from] ResumeError),
    #[error(transparent)]
    ProcessedFormat(#[from] ProcessedResumeError),
    #[error(transparent)]
    Checkpoint(#[from] SaveIoError),
    #[error("game state is not eligible for reconstruction: {0}")]
    Ineligible(String),
}

#[derive(Default, Resource)]
struct ResumeControl {
    configuration: [u8; 32],
    unsupported: Vec<String>,
    levels: BTreeMap<LevelId, LevelContentStamp>,
    pending: Option<ResumeActivation>,
    restored: bool,
    rejected: bool,
}

struct ResumeActivation {
    game: ResumeState<()>,
    content: ResumeContent,
}

enum ResumeContent {
    Source(ResumeCompatibility),
    Processed(ProcessedRooms),
}

pub(super) fn install(app: &mut App) -> Result<(), PluginError> {
    app.init_resource::<ResumeControl>()?;
    Ok(())
}

pub(super) fn record_configuration(app: &mut App) -> Result<(), PluginError> {
    let local_player = app.world().resource::<Session>().local_player;
    let configuration = app.configuration_fingerprint().as_bytes();
    let unsupported = app
        .installed_plugins()
        .filter(|declaration| {
            let supported = supported_service(declaration.id.as_str(), local_player);
            #[cfg(all(windows, feature = "audio-cpal", feature = "local-player"))]
            let supported = supported
                || (local_player
                    && declaration.id == crate::audio::AUDIO_PLUGIN_ID
                    && crate::audio::development_reset_installed(app.world()));
            !supported
        })
        .map(|declaration| declaration.id.as_str().to_owned())
        .collect();
    let mut control = app.world_mut()?.resource_mut::<ResumeControl>();
    control.configuration = configuration;
    control.unsupported = unsupported;
    Ok(())
}

fn supported_service(id: &str, local_player: bool) -> bool {
    matches!(
        id,
        "two-level-game.exploration"
            | "nara.product.runtime-time"
            | "nara.product-recipe.schema-authority"
            | "nara.reflect.registry"
            | "nara.scene.component-bindings"
            | "nara.scene.components"
            | "nara.hierarchy"
            | "nara.diagnostic"
            | "nara.tasks"
            | "nara.asset"
            | "nara.transform"
            | "nara.gameplay.commands"
            | "nara.startup-scene-activation"
    ) || if local_player {
        matches!(
            id,
            "nara.input"
                | "nara.material2d"
                | "nara.render-components"
                | "nara.window"
                | "nara.render"
                | "nara.render-wgpu.capture"
                | "nara.render-wgpu.window-presentation"
                | "nara.render-wgpu.stock-capture"
                | "nara.render-wgpu.stock-policy"
                | "nara.render-wgpu.stock-sprite"
                | "nara.sprite-render"
                | "nara.image.prepare"
                | "nara.sprite"
                | "nara.tilemap"
                | "nara.tilemap.assets"
                | "nara.image"
                | "two-level-game.input"
                | "two-level-game.replay"
                | "two-level-game.test.desktop-capture"
                | "two-level-game.development.presentation"
        )
    } else {
        id == "nara.server-time-policy"
    }
}

#[cfg(feature = "local-player")]
pub(super) fn stamp(level: LevelId, source: ProjectSceneContentStamp) -> LevelContentStamp {
    LevelContentStamp {
        level,
        manifest: *source.manifest_digest().as_bytes(),
        content: *source.content_digest().as_bytes(),
    }
}

#[cfg(feature = "local-player")]
pub(super) fn remember_current(world: &mut World) {
    if world.resource::<Session>().local_player
        && let Some(source) = CurrentScene::get(world).and_then(|view| view.source_content_stamp())
    {
        world
            .resource_mut::<ResumeControl>()
            .levels
            .insert(LevelId::STARTUP, stamp(LevelId::STARTUP, source));
    }
}

#[cfg(feature = "local-player")]
pub(super) fn input_capture_eligible(world: &World) -> Result<(), GameResumeError> {
    let baseline = nara::input::startup_action_map_bindings(world)
        .ok_or_else(|| GameResumeError::Ineligible("admitted game input is unavailable".into()))?;
    let map = world
        .get_resource::<nara::input::ActionMaps>()
        .and_then(|maps| maps.get(nara::input::InputSeatId::PRIMARY))
        .ok_or_else(|| GameResumeError::Ineligible("PRIMARY game input is unavailable".into()))?;
    if !baseline.shares_owner_with(&map.bindings_handle())
        || map
            .definition()
            .contexts()
            .iter()
            .any(|entry| map.is_context_enabled(&entry.context) != entry.enabled)
    {
        return Err(GameResumeError::Ineligible(
            "runtime input changes are not represented by admitted source".into(),
        ));
    }
    Ok(())
}

pub(super) fn required_levels(world: &World) -> Vec<LevelId> {
    world
        .resource::<ResumeControl>()
        .pending
        .as_ref()
        .and_then(|state| match &state.content {
            ResumeContent::Source(source) => {
                Some(source.levels.iter().map(|record| record.level).collect())
            }
            ResumeContent::Processed(_) => None,
        })
        .unwrap_or_default()
}

pub(super) fn validate_content(
    world: &World,
    source: Option<(LevelContentStamp, [u8; 32])>,
) -> Result<Option<LevelContentStamp>, String> {
    let control = world.resource::<ResumeControl>();
    if let Some(state) = &control.pending {
        match (&state.content, source) {
            (ResumeContent::Source(expected), Some((record, schema))) => {
                if schema != expected.schema || !expected.levels.contains(&record) {
                    return Err(format!(
                        "resume content compatibility rejected {:?}",
                        record.level
                    ));
                }
            }
            (ResumeContent::Source(_), None) => {
                return Err("exact reconstruction requires admitted source content facts".into());
            }
            (ResumeContent::Processed(_), Some(_)) => {
                return Err("processed reconstruction received source content".into());
            }
            (ResumeContent::Processed(_), None) => {}
        }
    }
    Ok(source.map(|(record, _)| record))
}

pub(super) fn commit_content(world: &mut World, stamps: Vec<LevelContentStamp>) {
    world
        .resource_mut::<ResumeControl>()
        .levels
        .extend(stamps.into_iter().map(|record| (record.level, record)));
}

pub(super) fn is_pending(world: &World) -> bool {
    world.resource::<ResumeControl>().pending.is_some()
}

pub(super) fn validate_prepared_placement(
    world: &World,
    level: LevelId,
    content: &super::content::Content,
) -> Result<(), String> {
    let control = world.resource::<ResumeControl>();
    if let Some(state) = control.pending.as_ref()
        && state.game.progress.location.level.level() == level
    {
        content
            .collision
            .validate_placement(player_pose(&state.game))?;
    }
    Ok(())
}

fn player_pose<C>(state: &ResumeState<C>) -> Transform2d {
    Transform2d {
        translation: Vec2::from_array(state.player.translation),
        rotation: state.player.rotation,
        scale: Vec2::from_array(state.player.scale),
    }
}

fn validate_current_placement<C>(
    world: &World,
    state: &ResumeState<C>,
) -> Result<(), GameResumeError> {
    let level = world.resource::<super::ActiveLevel>();
    if level.key().level() == state.progress.location.level.level() {
        level
            .content()
            .ok_or_else(|| {
                GameResumeError::Ineligible("active collision content is unavailable".into())
            })?
            .collision
            .validate_placement(player_pose(state))
            .map_err(GameResumeError::Ineligible)?;
    }
    Ok(())
}

pub(super) fn reject(world: &mut World) {
    let mut control = world.resource_mut::<ResumeControl>();
    control.pending = None;
    control.rejected = true;
}

pub(super) fn reject_pending(world: &mut World) {
    if is_pending(world) {
        reject(world);
    }
}

pub(super) fn advance_clock(world: &mut World) -> Result<(), String> {
    let mut session = world.resource_mut::<Session>();
    let clock = &mut session.clock;
    let next_tick = clock
        .ticks
        .checked_add(1)
        .ok_or("logical game tick is exhausted")?;
    let elapsed = clock
        .elapsed_nanos
        .checked_add(clock.timestep_nanos)
        .ok_or("logical game time is exhausted")?;
    clock.ticks = next_tick;
    clock.elapsed_nanos = elapsed;
    Ok(())
}

/// Starts reconstruction only in a fresh paused candidate, using its original gameplay queue.
///
/// Complete paused maintenance frames until `GameSnapshot::restoring` clears. Then compare
/// `capture_resume` with the original state before allowing gameplay to continue. This request
/// does not replace another process, publish a last-good executable or close the incumbent.
pub fn request_resume(
    session: &mut HeadlessSession,
    state: &ResumeState,
) -> Result<(), GameResumeError> {
    state.validate()?;
    let bytes = encode_resume(state)?;
    session.capture(fresh_candidate)??;
    session.capture(|world| validate_current_placement(world, state))??;
    enter_headless_resume(session, state.runtime_time, restore_command(bytes)?)
}

fn enter_headless_resume(
    session: &mut HeadlessSession,
    time: RuntimeTimeState,
    command: GameplayCommandSubmission,
) -> Result<(), GameResumeError> {
    session.restore_time_continuation(time.to_continuation()?)?;
    if session.frame(Duration::ZERO)?.is_some() {
        return Err(GameResumeError::Ineligible(
            "candidate exited during time continuation maintenance".into(),
        ));
    }
    session.submit(command)?;
    if session.step()?.is_some() || session.state() != Some(RuntimeState::Paused) {
        return Err(GameResumeError::Ineligible(
            "candidate exited during reconstruction entry".into(),
        ));
    }
    let error = session.observe(|world| world.resource::<Session>().error.clone())?;
    if let Some(error) = error {
        return Err(GameResumeError::Ineligible(error));
    }
    Ok(())
}

fn fresh_candidate(world: &World) -> Result<(), GameResumeError> {
    fresh_runtime(world)?;
    capture::eligible(world)
}

fn fresh_runtime(world: &World) -> Result<(), GameResumeError> {
    if world.resource::<FixedTime>().tick() != 0
        || world.resource::<Session>().clock.ticks != 0
        || world.resource::<ResumeControl>().restored
    {
        return Err(GameResumeError::Ineligible(
            "candidate is not a fresh game".into(),
        ));
    }
    Ok(())
}

fn restore_command(bytes: Vec<u8>) -> Result<GameplayCommandSubmission, GameResumeError> {
    let text = String::from_utf8(bytes).expect("resume encoding is UTF-8 JSON");
    let mut payload = GameplayCommandPayload::new();
    payload
        .insert("state", GameplayCommandValue::String(text))
        .map_err(|error| GameResumeError::Ineligible(error.to_string()))?;
    Ok(GameplayCommandSubmission::new(
        GameplayCommandTick::new(1).unwrap(),
        GameplayCommandIngressSource::external(RESUME_SOURCE).unwrap(),
        GameplayCommandSourceSequence::new(1).unwrap(),
        GameplayCommandDraft::new(GameplayCommandTypeId::new(RESTORE_STATE).unwrap())
            .with_payload(payload),
    ))
}

pub(super) fn begin_from_command(
    world: &mut World,
    payload: &GameplayCommandPayload,
) -> Result<(), String> {
    let batch = world.resource::<GameplayCommandBatch>();
    if world.resource::<FixedTime>().tick() != 1
        || !world.resource::<RuntimeTimeSettings>().paused()
        || world.resource::<Session>().clock.ticks != 0
        || world.resource::<ResumeControl>().restored
        || world.resource::<ResumeControl>().rejected
        || is_pending(world)
        || batch.len() != 1
        || world
            .resource::<GameplayCommandQueue>()
            .stats()
            .pending_commands
            != 0
    {
        return Err("resume command requires one isolated fresh-runtime transaction".into());
    }
    let command = &batch.commands()[0];
    if command
        .source()
        .id()
        .is_none_or(|id| id.as_str() != RESUME_SOURCE)
        || !matches!(
            command.source(),
            nara::gameplay::GameplayCommandSource::External { .. }
        )
        || command.source_sequence().get() != 1
        || command.target().is_some()
        || payload.len() != 1
        || !world
            .resource::<super::storage::Storage>()
            .is_unconfigured()
    {
        return Err(
            "resume command requires its original isolated ingress and no open SaveSlot".into(),
        );
    }
    let Some(GameplayCommandValue::String(encoded)) = payload.get("state") else {
        return Err("resume command lacks its bounded state".into());
    };
    let (activation, facts) = if world.resource::<Session>().local_player {
        let state = decode_resume(encoded.as_bytes()).map_err(|error| error.to_string())?;
        #[cfg(feature = "local-player")]
        {
            let current = CurrentScene::get(world)
                .and_then(|current| current.source_content_stamp())
                .ok_or("resume startup source facts are unavailable")?;
            if !state
                .compatibility
                .levels
                .contains(&stamp(LevelId::STARTUP, current))
            {
                return Err("resume startup content is incompatible".into());
            }
        }
        let source = state.compatibility.clone();
        let facts = (source.schema, source.configuration);
        (
            ResumeActivation {
                game: state.with_compatibility(()),
                content: ResumeContent::Source(source),
            },
            facts,
        )
    } else {
        let state =
            decode_processed_resume(encoded.as_bytes()).map_err(|error| error.to_string())?;
        if !state.pending.is_empty()
            || state.checkpoint.is_some()
            || capture::hosted_catalog(world).map_err(|error| error.to_string())?
                != state.catalog.into()
        {
            return Err(
                "processed resume requires its original Runtime catalog and isolated initializer"
                    .into(),
            );
        }
        let facts = (
            state.game.compatibility.schema,
            state.game.compatibility.configuration,
        );
        (
            ResumeActivation {
                game: state.game.with_compatibility(()),
                content: ResumeContent::Processed(state.rooms),
            },
            facts,
        )
    };
    validate_current_placement(world, &activation.game).map_err(|error| error.to_string())?;
    let fixed = world.resource::<FixedTime>();
    let pending = fixed
        .debt()
        .checked_add(fixed.remainder())
        .ok_or("candidate scheduling phase overflowed")?;
    let actual_time =
        RuntimeTimeContinuation::new(world.resource::<RuntimeTimeSettings>(), fixed, pending)
            .map_err(|error| error.to_string())?;
    if activation
        .game
        .runtime_time
        .to_continuation()
        .map_err(|error| error.to_string())?
        != actual_time
    {
        return Err("resume scheduling phase or policy is incompatible".into());
    }
    let control = world.resource::<ResumeControl>();
    if !control.unsupported.is_empty() || control.configuration != facts.1 {
        return Err("resume configuration or service inventory is incompatible".into());
    }
    let registry =
        nara::reflect::component_registry(world).ok_or("resume Schema is unavailable")?;
    let schema = registry
        .schema_composition_fingerprint()
        .map_err(|error| error.to_string())?;
    if schema.as_bytes() != facts.0
        || activation.game.clock.timestep_nanos != world.resource::<Session>().clock.timestep_nanos
    {
        return Err("resume Schema or game timestep is incompatible".into());
    }
    let progress = activation.game.progress.clone();
    world.resource_mut::<ResumeControl>().pending = Some(activation);
    if let Err(error) = travel::begin(world, progress, travel::TravelKind::Resume) {
        world.resource_mut::<ResumeControl>().pending = None;
        return Err(error);
    }
    Ok(())
}

pub(super) fn finish(world: &mut World) {
    let Some(activation) = world.resource::<ResumeControl>().pending.as_ref() else {
        return;
    };
    if !world.resource::<travel::Travel>().is_idle() {
        return;
    }
    if world.resource::<Session>().error.is_some()
        || world.resource::<Session>().phase != GamePhase::Running
    {
        return;
    }
    let ready = match &activation.content {
        ResumeContent::Source(source) => {
            if world
                .resource::<ResumeControl>()
                .levels
                .values()
                .eq(&source.levels)
            {
                Ok(true)
            } else {
                Err("not every captured level has matching admitted content".into())
            }
        }
        ResumeContent::Processed(rooms) => side_room::restore_processed(world, *rooms),
    };
    match ready {
        Ok(false) => return,
        Ok(true) => {}
        Err(error) => {
            fail(world, error);
            return;
        }
    }
    let state = world
        .resource_mut::<ResumeControl>()
        .pending
        .take()
        .expect("pending game reconstruction")
        .game;
    let Some(player) = player(world) else {
        reject(world);
        fail(world, "reconstructed player identity is unavailable".into());
        return;
    };
    if world.resource::<Progress>().checkpoint() != &state.progress {
        reject(world);
        fail(
            world,
            "reconstructed progress disagrees with the captured state".into(),
        );
        return;
    }
    if world.get::<Transform2d>(player).is_none() {
        reject(world);
        fail(
            world,
            "reconstructed player transform is unavailable".into(),
        );
        return;
    }
    let pose = player_pose(&state);
    if let Err(error) = super::collision::place(world, player, pose) {
        reject(world);
        fail(world, error);
        return;
    }
    let mut session = world.resource_mut::<Session>();
    session.held = state.held;
    session.clock = state.clock;
    session.travel_count = state.counters.travel_count;
    session.restarts = state.counters.restarts;
    session.saves = state.counters.saves;
    session.restores = state.counters.restores;
    world.resource_mut::<ResumeControl>().restored = true;
}
