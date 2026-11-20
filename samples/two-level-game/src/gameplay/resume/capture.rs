#[cfg(feature = "desktop")]
use nara::project_host::DesktopRun;
use nara::{
    advanced_prelude::CurrentScene,
    app::{FixedTime, RuntimeTimeSettings},
    ecs::World,
    gameplay::{GameplayCommandBatch, GameplayCommandQueue, LocalGameplayCommands},
    prelude::Transform2d,
    project_host::HeadlessSession,
    tasks::{TaskPools, TaskReturnState},
};
#[cfg(feature = "local-player")]
use nara::{
    asset::Assets,
    material::{Material2d, Material2dAsset},
};

use super::{GameResumeError, ResumeControl};
use crate::{
    gameplay::{
        ActiveLevel, GamePhase, Progress, Session, player, travel, verification::Verification,
    },
    resume::{PlayerState, ResumeCompatibility, ResumeState, RuntimeTimeState, SessionCounters},
};

pub(super) fn eligible(world: &World) -> Result<(), GameResumeError> {
    let reject = |reason: &str| GameResumeError::Ineligible(reason.into());
    let session = world.resource::<Session>();
    if !session.local_player {
        return Err(reject(
            "exact reconstruction requires the local source player",
        ));
    }
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
    crate::gameplay::side_room::Rooms::require_closed(world)
        .map_err(GameResumeError::Ineligible)?;
    if !world
        .resource::<crate::gameplay::storage::Storage>()
        .permits_resume()
    {
        return Err(reject(
            "an open SaveSlot has no candidate reconstruction policy",
        ));
    }
    let control = world.resource::<ResumeControl>();
    if !control.unsupported.is_empty() {
        return Err(reject(&format!(
            "unsupported services: {:?}",
            control.unsupported
        )));
    }
    #[cfg(feature = "local-player")]
    super::input_capture_eligible(world)?;
    if control.rejected {
        return Err(reject("this candidate rejected its reconstruction"));
    }
    #[cfg(feature = "local-player")]
    if world
        .get_resource::<Assets<Material2dAsset>>()
        .is_some_and(|assets| !assets.is_empty())
    {
        return Err(reject("material assets are not represented by ResumeState"));
    }
    #[cfg(feature = "local-player")]
    if world
        .iter_entities()
        .any(|entity| entity.contains::<Material2d>())
    {
        return Err(reject(
            "material components are not represented by ResumeState",
        ));
    }
    let queue = world.resource::<GameplayCommandQueue>();
    let stats = queue.stats();
    let time = world.resource::<FixedTime>();
    if !queue.is_idle()
        || stats.acknowledged_through_tick != time.tick()
        || !world.resource::<GameplayCommandBatch>().is_empty()
        || !world.resource::<LocalGameplayCommands>().is_empty()
    {
        return Err(reject(
            "gameplay commands are pending, active or unacknowledged",
        ));
    }
    if world.resource::<RuntimeTimeSettings>().time_scale() != 1.0 {
        return Err(reject(
            "this game does not support continuation at a non-unit time scale",
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

/// Captures complete supported game semantics at the Session's completed paused boundary.
///
/// The shipped source/headless recipe and explicit game-owned development audio reset policy
/// are supported. Ordinary audio, unknown plugin services, processed/direct sources, pending
/// commands/travel/tasks, scripted verification and unrepresented time policy refuse capture,
/// as do material assets or components not represented by `ResumeState`.
/// Empty material storage and its installed Schema are supported. This value contains no
/// World identity or native-service authority.
pub fn capture_resume(session: &HeadlessSession) -> Result<ResumeState, GameResumeError> {
    let runtime_time = RuntimeTimeState::from_continuation(session.capture_time_continuation()?)?;
    session.capture(|world| {
        #[cfg(feature = "local-player")]
        if world.contains_resource::<nara::window::backend::WindowPresentationRequirement>() {
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
#[cfg(feature = "desktop")]
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

pub(crate) fn capture_world(
    world: &World,
    runtime_time: RuntimeTimeState,
) -> Result<ResumeState, GameResumeError> {
    eligible(world)?;
    let reject = |reason: &str| GameResumeError::Ineligible(reason.into());
    let current = CurrentScene::get(world).ok_or_else(|| reject("current Scene is unavailable"))?;
    let source = current
        .source_content_stamp()
        .ok_or_else(|| reject("admitted source facts are unavailable"))?;
    let key = world.resource::<ActiveLevel>().key();
    let control = world.resource::<ResumeControl>();
    if control.levels.get(&key.level()) != Some(&super::stamp(key.level(), source)) {
        return Err(reject(
            "current source no longer matches game-owned admission facts",
        ));
    }
    travel::observe_markers(world).map_err(GameResumeError::Ineligible)?;
    let mut progress = world.resource::<Progress>().checkpoint().clone();
    progress.collected.sort();
    progress.markers.sort_by_key(|record| record.id);
    let entity = player(world).ok_or_else(|| reject("the current player is unavailable"))?;
    #[cfg(feature = "local-player")]
    {
        let presentation = world
            .resource::<ActiveLevel>()
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
    }
    crate::gameplay::collision::capture_eligible(world, entity)
        .map_err(GameResumeError::Ineligible)?;
    let player = Some(entity)
        .and_then(|entity| world.get::<Transform2d>(entity))
        .ok_or_else(|| reject("the current player transform is unavailable"))?;
    let session = world.resource::<Session>();
    let registry = nara::reflect::component_registry(world)
        .ok_or_else(|| reject("game Schema is unavailable"))?;
    let schema = registry
        .schema_composition_fingerprint()
        .map_err(|error| reject(&error.to_string()))?
        .as_bytes();
    let state = ResumeState {
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
        compatibility: ResumeCompatibility {
            schema,
            configuration: control.configuration,
            levels: control.levels.values().cloned().collect(),
        },
    };
    state.validate()?;
    Ok(state)
}
