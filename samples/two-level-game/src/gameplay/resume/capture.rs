#[cfg(all(feature = "desktop", any(feature = "development", feature = "cook")))]
use nara::project_host::DesktopRun;
use nara::{
    advanced_prelude::CurrentScene,
    app::FixedTime,
    ecs::World,
    fs::ContentDigest,
    prelude::Transform2d,
    tasks::{TaskPools, TaskReturnState},
};
#[cfg(any(feature = "development", feature = "cook"))]
use nara::{
    app::RuntimeTimeSettings,
    asset::Assets,
    material::{Material2d, Material2dAsset},
    project_host::HeadlessSession,
    window::backend::WindowPresentationRequirement,
};

use super::{GameResumeError, ResumeControl};
use crate::command::CommandSchedule;
#[cfg(any(feature = "development", feature = "cook"))]
use crate::{
    gameplay::{ActiveLevel, side_room::Rooms},
    resume::ResumeCompatibility,
};
use crate::{
    gameplay::{
        GamePhase, Progress, Session, player, storage::Storage, travel, verification::Verification,
    },
    resume::{
        PlayerState, ResumeState, RuntimeTimeState, SessionCounters,
        processed::ProcessedResumeCompatibility,
    },
};

#[cfg(any(feature = "development", feature = "cook"))]
pub(super) fn eligible(world: &World) -> Result<(), GameResumeError> {
    let reject = |reason: &str| GameResumeError::Ineligible(reason.into());
    if !world.resource::<Session>().local_player {
        return Err(reject(
            "exact reconstruction requires the local source player",
        ));
    }
    quiescent(world, false)?;
    if world.resource::<RuntimeTimeSettings>().time_scale() != 1.0 {
        return Err(reject(
            "this game does not support continuation at a non-unit time scale",
        ));
    }
    require_unconfigured_checkpoint(world)?;
    Rooms::require_closed(world).map_err(GameResumeError::Ineligible)?;
    super::input_capture_eligible(world)?;
    if world
        .get_resource::<Assets<Material2dAsset>>()
        .is_some_and(|assets| !assets.is_empty())
    {
        return Err(reject("material assets are not represented by ResumeState"));
    }
    if world
        .iter_entities()
        .any(|entity| entity.contains::<Material2d>())
    {
        return Err(reject(
            "material components are not represented by ResumeState",
        ));
    }
    Ok(())
}

pub(super) fn hosted_catalog(world: &World) -> Result<ContentDigest, GameResumeError> {
    if world
        .get_resource::<Session>()
        .is_none_or(|session| session.local_player)
    {
        return Err(GameResumeError::Ineligible(
            "processed reconstruction requires the dedicated Server recipe".into(),
        ));
    }
    let (product, catalog) = CurrentScene::get(world)
        .and_then(|current| current.source_view().hosted_runtime_scene_identity())
        .ok_or_else(|| {
            GameResumeError::Ineligible("the active Scene has no hosted Runtime catalog".into())
        })?;
    let level = world.resource::<Progress>().checkpoint().location.level;
    if product != level.level().scene_product() {
        return Err(GameResumeError::Ineligible(
            "the active Scene Product differs from the Server level".into(),
        ));
    }
    Ok(catalog)
}

pub(super) fn quiescent(
    world: &World,
    allow_pending_commands: bool,
) -> Result<(), GameResumeError> {
    let reject = |reason: &str| GameResumeError::Ineligible(reason.into());
    let session = world.resource::<Session>();
    if !session.initialized
        || session.phase != GamePhase::Running
        || session.error.is_some()
        || !world.resource::<travel::Travel>().is_idle()
        || world.resource::<ResumeControl>().pending.is_some()
    {
        return Err(reject("gameplay or reconstruction is not quiescent"));
    }
    if !world.resource::<Verification>().is_interactive() {
        return Err(reject(
            "scripted verification is not an interactive resume state",
        ));
    }
    let control = world.resource::<ResumeControl>();
    if !control.unsupported.is_empty() {
        return Err(reject(&format!(
            "unsupported services: {:?}",
            control.unsupported
        )));
    }
    if control.rejected {
        return Err(reject("this candidate rejected its reconstruction"));
    }
    let stats = world.resource::<CommandSchedule>().stats();
    let time = world.resource::<FixedTime>();
    if stats.closed_through_tick != time.tick()
        || stats.acknowledged_through_tick != time.tick()
        || stats.active_commands != 0
        || (!allow_pending_commands && stats.retained_commands != 0)
    {
        return Err(reject(
            "gameplay commands are pending, active or unacknowledged",
        ));
    }
    match world.resource::<TaskPools>().return_state() {
        TaskReturnState::Returned => {}
        TaskReturnState::Pending => {
            return Err(reject("an authoritative task has not physically returned"));
        }
        TaskReturnState::CleanupFailed => {
            return Err(reject("authoritative task cleanup failed"));
        }
    }
    Ok(())
}

pub(super) fn require_unconfigured_checkpoint(world: &World) -> Result<(), GameResumeError> {
    if !world.resource::<Storage>().is_unconfigured() {
        return Err(GameResumeError::Ineligible(
            "an open SaveSlot has no candidate reconstruction policy for this operation".into(),
        ));
    }
    Ok(())
}

/// Captures complete supported game semantics at the Session's completed paused boundary.
///
/// The shipped source/headless recipe and explicit game-owned development audio reset policy
/// are supported. Ordinary audio, unknown plugin services, processed/direct sources, pending
/// commands/travel/tasks, scripted verification and unrepresented time policy refuse capture,
/// as do material assets or components not represented by `ResumeState`.
/// Empty material storage and its installed Schema are supported. This value contains no
/// World identity or native-service authority.
#[cfg(any(feature = "development", feature = "cook"))]
pub fn capture_resume(session: &HeadlessSession) -> Result<ResumeState, GameResumeError> {
    let runtime_time = RuntimeTimeState::from_continuation(session.capture_time_continuation()?)?;
    session.capture(|world| {
        if world.contains_resource::<WindowPresentationRequirement>() {
            return Err(GameResumeError::Ineligible(
                "native presentation requires the desktop capture boundary".into(),
            ));
        }
        capture_world(world, runtime_time)
    })?
}

/// Captures the same game-owned state through a real healthy paused desktop Runtime.
///
/// Native handles, WGPU resources and presentation receipts remain owned by `DesktopRun`; only
/// the game-owned logical state crosses this boundary. A fresh desktop candidate must reconstruct
/// those native services before a caller can use this value for restore. Development audio resets
/// its voices and effect history with publication disabled; no audio state is reconstructed.
#[cfg(all(feature = "desktop", any(feature = "development", feature = "cook")))]
pub fn capture_desktop_resume(session: &DesktopRun) -> Result<ResumeState, GameResumeError> {
    let runtime_time = RuntimeTimeState::from_continuation(session.capture_time_continuation()?)?;
    let bindings = crate::movement_keyboard_bindings(session)?;
    let holds = session.capture_keyboard_holds(&bindings)?;
    let state = session.capture(|world| capture_world(world, runtime_time))??;
    if holds.iter().map(|hold| hold.held).ne(state.held) {
        return Err(GameResumeError::Ineligible(
            "game movement intent differs from the original keyboard inventory".into(),
        ));
    }
    Ok(state)
}

#[cfg(any(feature = "development", feature = "cook"))]
pub(crate) fn capture_world(
    world: &World,
    runtime_time: RuntimeTimeState,
) -> Result<ResumeState, GameResumeError> {
    eligible(world)?;
    let reject = |reason: &str| GameResumeError::Ineligible(reason.into());
    let control = world.resource::<ResumeControl>();
    let active = world.resource::<ActiveLevel>();
    let current = CurrentScene::get(world).ok_or_else(|| reject("current Scene is unavailable"))?;
    let source = current
        .source_content_stamp()
        .ok_or_else(|| reject("admitted source facts are unavailable"))?;
    let level = active.key().level();
    if control.levels.get(&level) != Some(&super::stamp(level, source)) {
        return Err(reject(
            "current source no longer matches game-owned admission facts",
        ));
    }
    let entity = player(world).ok_or_else(|| reject("the current player is unavailable"))?;
    let presentation = active
        .content()
        .and_then(|content| content.presentation.as_ref())
        .ok_or_else(|| reject("active animation content is unavailable"))?;
    if !world
        .get::<crate::animation::ExplorerAnimation>(entity)
        .is_some_and(|component| presentation.animation.matches(component))
    {
        return Err(reject(
            "runtime animation configuration is not represented by admitted source",
        ));
    }
    let facts = compatibility(world)?;
    let state = game(
        world,
        runtime_time,
        ResumeCompatibility {
            schema: facts.schema,
            configuration: facts.configuration,
            levels: control.levels.values().copied().collect(),
        },
    )?;
    state.validate()?;
    Ok(state)
}

pub(super) fn compatibility(
    world: &World,
) -> Result<ProcessedResumeCompatibility, GameResumeError> {
    let reject = |reason: &str| GameResumeError::Ineligible(reason.into());
    let registry = nara::reflect::component_registry(world)
        .ok_or_else(|| reject("game Schema is unavailable"))?;
    let schema = registry
        .schema_composition_fingerprint()
        .map_err(|error| reject(&error.to_string()))?
        .as_bytes();
    Ok(ProcessedResumeCompatibility {
        schema,
        configuration: world.resource::<ResumeControl>().configuration,
    })
}

pub(super) fn game<C>(
    world: &World,
    runtime_time: RuntimeTimeState,
    compatibility: C,
) -> Result<ResumeState<C>, GameResumeError> {
    let reject = |reason: &str| GameResumeError::Ineligible(reason.into());
    travel::observe_markers(world).map_err(GameResumeError::Ineligible)?;
    let mut progress = world.resource::<Progress>().checkpoint().clone();
    progress.collected.sort();
    progress.markers.sort_by_key(|record| record.id);
    let entity = player(world).ok_or_else(|| reject("the current player is unavailable"))?;
    crate::gameplay::collision::capture_eligible(world, entity)
        .map_err(GameResumeError::Ineligible)?;
    let player = world
        .get::<Transform2d>(entity)
        .ok_or_else(|| reject("the current player transform is unavailable"))?;
    let session = world.resource::<Session>();
    Ok(ResumeState {
        progress,
        player: PlayerState {
            translation: player.translation.to_array(),
            rotation: player.rotation,
            scale: player.scale.to_array(),
        },
        held: session.held,
        clock: session.clock,
        runtime_time,
        counters: SessionCounters {
            travel_count: session.travel_count,
            restarts: session.restarts,
            saves: session.saves,
            restores: session.restores,
        },
        compatibility,
    })
}
