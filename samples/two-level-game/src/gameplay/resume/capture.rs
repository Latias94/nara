use std::collections::BTreeSet;

use nara::{
    advanced_prelude::CurrentScene,
    app::{FixedTime, RuntimeTimeSettings},
    ecs::World,
    gameplay::{GameplayCommandBatch, GameplayCommandQueue, LocalGameplayCommands},
    prelude::Transform2d,
    project_host::HeadlessSession,
    tasks::{TaskPoolKind, TaskPools},
};

use super::{GameResumeError, ResumeControl};
use crate::{
    gameplay::{
        ActiveLevel, GamePhase, Progress, Session, player, travel, verification::Verification,
    },
    resume::{PlayerState, ResumeCompatibility, ResumeState, SessionCounters},
};

pub(super) fn eligible(world: &World) -> Result<(), GameResumeError> {
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
    if control.rejected {
        return Err(reject("this candidate rejected its reconstruction"));
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
    if !time.remainder().is_zero()
        || !time.debt().is_zero()
        || world.resource::<RuntimeTimeSettings>().time_scale() != 1.0
    {
        return Err(reject(
            "unrepresented Runtime time policy or remainder is active",
        ));
    }
    let tasks = world.resource::<TaskPools>().stats();
    if TaskPoolKind::ALL.into_iter().any(|kind| {
        let stats = tasks.for_kind(kind);
        stats.queued != 0 || stats.running != 0
    }) {
        return Err(reject("an authoritative task has not returned"));
    }
    Ok(())
}

/// Captures complete supported game semantics at the Session's completed paused boundary.
///
/// The shipped source/headless recipe is supported. Unknown plugin services, processed/direct
/// sources, pending commands/travel/tasks, scripted verification and unrepresented time policy
/// refuse capture. This value contains no World identity or native-service authority.
pub fn capture_resume(session: &HeadlessSession) -> Result<ResumeState, GameResumeError> {
    session.capture(capture_world)?
}

fn capture_world(world: &World) -> Result<ResumeState, GameResumeError> {
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
    let marker_ids = travel::observe_markers(world).map_err(GameResumeError::Ineligible)?;
    let mut progress = world.resource::<Progress>().checkpoint().clone();
    progress.collected.sort();
    progress.markers.sort_by_key(|record| record.id);
    let expected_ids = progress
        .markers
        .iter()
        .filter(|record| record.level == key)
        .map(|record| record.id)
        .collect::<BTreeSet<_>>();
    if marker_ids.into_iter().collect::<BTreeSet<_>>() != expected_ids {
        return Err(reject("current dynamic marker state is inconsistent"));
    }
    let player = player(world)
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
