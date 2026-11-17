//! Game-owned exact state over the original Headless, command and Scene owners.

mod capture;
#[cfg(feature = "desktop")]
mod desktop;

use std::collections::BTreeMap;
use std::time::Duration;

#[cfg(feature = "desktop")]
use nara::project_host::DesktopControlError;
use nara::{
    advanced_prelude::CurrentScene,
    app::{FixedTime, RuntimeState, RuntimeTimeContinuation, RuntimeTimeSettings},
    ecs::World,
    gameplay::{
        GameplayCommandBatch, GameplayCommandDraft, GameplayCommandIngressSource,
        GameplayCommandPayload, GameplayCommandQueue, GameplayCommandSourceSequence,
        GameplayCommandSubmission, GameplayCommandTick, GameplayCommandTypeId,
        GameplayCommandValue,
    },
    prelude::{App, PluginError, Resource, Transform2d, Vec2},
    project_host::{HeadlessSession, HeadlessSessionError, ProjectSceneContentStamp},
};

use super::{GamePhase, Progress, Session, fail, player, travel};
use crate::{
    levels::LevelId,
    resume::{LevelContentStamp, ResumeError, ResumeState, decode_resume, encode_resume},
};

#[cfg(feature = "desktop")]
pub use capture::capture_desktop_resume;
pub use capture::capture_resume;
#[cfg(all(feature = "desktop", feature = "development"))]
pub(crate) use capture::capture_world;
#[cfg(feature = "desktop")]
pub use desktop::restore_desktop_resume;
pub(super) const RESTORE_STATE: &str = "two-level-game.resume-state";

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
    #[error("game state is not eligible for reconstruction: {0}")]
    Ineligible(String),
}

#[derive(Default, Resource)]
struct ResumeControl {
    configuration: [u8; 32],
    unsupported: Vec<String>,
    levels: BTreeMap<LevelId, LevelContentStamp>,
    pending: Option<ResumeState>,
    restored: bool,
    rejected: bool,
}

pub(super) fn install(app: &mut App) -> Result<(), PluginError> {
    app.init_resource::<ResumeControl>()?;
    Ok(())
}

pub(super) fn record_configuration(app: &mut App) -> Result<(), PluginError> {
    let configuration = app.configuration_fingerprint().as_bytes();
    let unsupported = app
        .installed_plugins()
        .filter(|declaration| {
            let supported = supported_service(declaration.id.as_str());
            #[cfg(all(windows, feature = "audio-cpal"))]
            let supported = supported
                || (declaration.id == crate::audio::AUDIO_PLUGIN_ID
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

fn supported_service(id: &str) -> bool {
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
            | "nara.input"
            | "nara.gameplay.commands"
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
            | "nara.startup-scene-activation"
            | "two-level-game.desktop-input"
            | "two-level-game.test.desktop-capture"
            | "two-level-game.development.presentation"
    )
}

fn stamp(level: LevelId, source: ProjectSceneContentStamp) -> LevelContentStamp {
    LevelContentStamp {
        level,
        manifest: *source.manifest_digest().as_bytes(),
        content: *source.content_digest().as_bytes(),
    }
}

pub(super) fn remember_current(world: &mut World) {
    if let Some(source) = CurrentScene::get(world).and_then(|view| view.source_content_stamp()) {
        world
            .resource_mut::<ResumeControl>()
            .levels
            .insert(LevelId::STARTUP, stamp(LevelId::STARTUP, source));
    }
}

pub(super) fn required_levels(world: &World) -> Vec<LevelId> {
    world
        .resource::<ResumeControl>()
        .pending
        .as_ref()
        .map(|state| {
            state
                .compatibility
                .levels
                .iter()
                .map(|record| record.level)
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn validate_content(
    world: &mut World,
    level: LevelId,
    source: Option<ProjectSceneContentStamp>,
) -> Result<(), String> {
    let Some(source) = source else {
        if is_pending(world) {
            return Err("exact reconstruction requires admitted source content facts".into());
        }
        return Ok(());
    };
    let record = stamp(level, source);
    let control = world.resource::<ResumeControl>();
    if let Some(state) = &control.pending {
        if source.schema_fingerprint().as_bytes() != state.compatibility.schema
            || !state
                .compatibility
                .levels
                .iter()
                .any(|expected| expected == &record)
        {
            return Err(format!("resume content compatibility rejected {level:?}"));
        }
    }
    world
        .resource_mut::<ResumeControl>()
        .levels
        .insert(level, record);
    Ok(())
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
        && state.progress.location.level.level() == level
    {
        content.collision.validate_placement(player_pose(state))?;
    }
    Ok(())
}

fn player_pose(state: &ResumeState) -> Transform2d {
    Transform2d {
        translation: Vec2::from_array(state.player.translation),
        rotation: state.player.rotation,
        scale: Vec2::from_array(state.player.scale),
    }
}

fn validate_current_placement(world: &World, state: &ResumeState) -> Result<(), GameResumeError> {
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
    session.restore_time_continuation(state.runtime_time.to_continuation()?)?;
    if session.frame(Duration::ZERO)?.is_some() {
        return Err(GameResumeError::Ineligible(
            "candidate exited during time continuation maintenance".into(),
        ));
    }
    session.submit(restore_command(bytes)?)?;
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
    if world.resource::<FixedTime>().tick() != 0
        || world.resource::<Session>().clock.ticks != 0
        || world.resource::<ResumeControl>().restored
    {
        return Err(GameResumeError::Ineligible(
            "candidate is not a fresh game".into(),
        ));
    }
    capture::eligible(world)
}

fn restore_command(bytes: Vec<u8>) -> Result<GameplayCommandSubmission, GameResumeError> {
    let text = String::from_utf8(bytes).expect("resume encoding is UTF-8 JSON");
    let mut payload = GameplayCommandPayload::new();
    payload
        .insert("state", GameplayCommandValue::String(text))
        .map_err(|error| GameResumeError::Ineligible(error.to_string()))?;
    Ok(GameplayCommandSubmission::new(
        GameplayCommandTick::new(1).unwrap(),
        GameplayCommandIngressSource::external("two-level-game.resume").unwrap(),
        GameplayCommandSourceSequence::new(1).unwrap(),
        GameplayCommandDraft::new(GameplayCommandTypeId::new(RESTORE_STATE).unwrap())
            .with_payload(payload),
    ))
}

pub(super) fn begin_from_command(
    world: &mut World,
    payload: &GameplayCommandPayload,
) -> Result<(), String> {
    if world.resource::<FixedTime>().tick() != 1
        || !world.resource::<RuntimeTimeSettings>().paused()
        || world.resource::<Session>().clock.ticks != 0
        || world.resource::<ResumeControl>().restored
        || world.resource::<ResumeControl>().rejected
        || is_pending(world)
        || world.resource::<GameplayCommandBatch>().len() != 1
        || world
            .resource::<GameplayCommandQueue>()
            .stats()
            .pending_commands
            != 0
    {
        return Err("resume command requires one isolated fresh-runtime transaction".into());
    }
    let Some(GameplayCommandValue::String(encoded)) = payload.get("state") else {
        return Err("resume command lacks its bounded state".into());
    };
    let state = decode_resume(encoded.as_bytes()).map_err(|error| error.to_string())?;
    validate_current_placement(world, &state).map_err(|error| error.to_string())?;
    let fixed = world.resource::<FixedTime>();
    let pending = fixed
        .debt()
        .checked_add(fixed.remainder())
        .ok_or("candidate scheduling phase overflowed")?;
    let actual_time =
        RuntimeTimeContinuation::new(world.resource::<RuntimeTimeSettings>(), fixed, pending)
            .map_err(|error| error.to_string())?;
    if state
        .runtime_time
        .to_continuation()
        .map_err(|error| error.to_string())?
        != actual_time
    {
        return Err("resume scheduling phase or policy is incompatible".into());
    }
    let control = world.resource::<ResumeControl>();
    if !control.unsupported.is_empty() || control.configuration != state.compatibility.configuration
    {
        return Err("resume configuration or service inventory is incompatible".into());
    }
    let registry =
        nara::reflect::component_registry(world).ok_or("resume Schema is unavailable")?;
    let schema = registry
        .schema_composition_fingerprint()
        .map_err(|error| error.to_string())?;
    if schema.as_bytes() != state.compatibility.schema
        || state.clock.timestep_nanos != world.resource::<Session>().clock.timestep_nanos
    {
        return Err("resume Schema or game timestep is incompatible".into());
    }
    let current = CurrentScene::get(world)
        .and_then(|current| current.source_content_stamp())
        .ok_or("resume startup source facts are unavailable")?;
    let startup = stamp(LevelId::STARTUP, current);
    if !state.compatibility.levels.contains(&startup) {
        return Err("resume startup content is incompatible".into());
    }
    let progress = state.progress.clone();
    world.resource_mut::<ResumeControl>().pending = Some(state);
    if let Err(error) = travel::begin(world, progress, true) {
        world.resource_mut::<ResumeControl>().pending = None;
        return Err(error);
    }
    Ok(())
}

pub(super) fn finish(world: &mut World) {
    if !is_pending(world) || !world.resource::<travel::Travel>().is_idle() {
        return;
    }
    if world.resource::<Session>().error.is_some()
        || world.resource::<Session>().phase != GamePhase::Running
    {
        return;
    }
    let Some(state) = world.resource_mut::<ResumeControl>().pending.take() else {
        return;
    };
    let observed = world
        .resource::<ResumeControl>()
        .levels
        .values()
        .copied()
        .collect::<Vec<_>>();
    if observed != state.compatibility.levels {
        reject(world);
        fail(
            world,
            "not every captured level has matching admitted content".into(),
        );
        return;
    }
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
