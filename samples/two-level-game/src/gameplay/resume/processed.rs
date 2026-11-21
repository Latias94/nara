use std::{
    thread,
    time::{Duration, Instant},
};

use nara::{
    app::{FixedCatchUpPolicy, FixedTime},
    gameplay::{
        GameplayCommandIngressSource, GameplayCommandQueue, GameplayCommandSource,
        GameplayCommandSourceSequence, GameplayCommandSubmission, GameplayCommandTick,
    },
    project_host::HeadlessSession,
    tasks::{TaskPools, TaskReturnState},
};

use super::{
    GameResumeError, ResumeControl, capture, enter_headless_resume, fresh_runtime, restore_command,
    validate_current_placement,
};
use crate::{
    GameSnapshot,
    command::{SERVER_COMMAND_SOURCE, ServerCommand},
    gameplay::side_room::Rooms,
    resume::{
        RuntimeCatchUpPolicy, RuntimeTimeState,
        processed::{
            MAX_PENDING_SERVER_COMMANDS, PendingServerCommand, ProcessedResumeError,
            ProcessedResumeState, encode_processed_resume,
        },
    },
};

/// Captures the game, live rooms and pending external intent from a completed paused Server frame.
///
/// The one supported producer is the game's local Server command stream. Commands retain their
/// relative fixed tick and canonical order. Historical queue keys and service handles do not
/// cross the process boundary. A live SaveSlot, unsettled room, or unknown producer refuses capture.
pub fn capture_processed_resume(
    session: &HeadlessSession,
) -> Result<ProcessedResumeState, GameResumeError> {
    let time = RuntimeTimeState::from_continuation(session.capture_time_continuation()?)?;
    session.capture(|world| {
        let catalog = capture::hosted_catalog(world)?.into();
        let rooms = Rooms::capture_processed(world).map_err(refuse)?;
        capture::quiescent(world, true)?;
        if time.catch_up_policy != RuntimeCatchUpPolicy::PreserveDebt {
            return Err(refuse("processed continuation requires the Server debt policy"));
        }
        let queue = world.resource::<GameplayCommandQueue>();
        let stats = queue.stats();
        if stats.pending_commands > MAX_PENDING_SERVER_COMMANDS {
            return Err(ProcessedResumeError::TooManyPendingCommands(stats.pending_commands).into());
        }
        let mut pending = Vec::with_capacity(stats.pending_commands);
        for (tick, source, _, draft) in queue.pending_commands() {
            if !matches!(source, GameplayCommandSource::External { producer } if producer.as_str() == SERVER_COMMAND_SOURCE) {
                return Err(refuse("pending command belongs to an unsupported producer"));
            }
            let tick_offset = tick.get()
                .checked_sub(stats.acknowledged_through_tick)
                .ok_or_else(|| refuse("pending command precedes the acknowledged tick"))?;
            pending.push(PendingServerCommand {
                tick_offset,
                command: ServerCommand::from_draft(draft).map_err(refuse)?,
            });
        }
        let game = capture::game(world, time, capture::compatibility(world)?)?;
        let state = ProcessedResumeState { game, catalog, rooms, pending };
        state.validate()?;
        Ok(state)
    })?
}

/// Reconstructs a fresh paused processed Server and compares its complete canonical capture.
///
/// Callers retain and close the original candidate after any failure. A failure after entry may
/// leave that candidate changed. These operations do not act on the incumbent or adopt a route.
/// The deadline bounds cooperative maintenance around ordinary Runtime and Scene operations.
pub fn restore_processed_resume(
    session: &mut HeadlessSession,
    state: &ProcessedResumeState,
    deadline: Instant,
) -> Result<(), GameResumeError> {
    check_deadline(deadline)?;
    let expected = encode_processed_resume(state)?;
    session.capture(|world| -> Result<(), GameResumeError> {
        if capture::hosted_catalog(world)? != state.catalog.into() {
            return Err(refuse(
                "resume Runtime catalog differs from the admitted Server catalog",
            ));
        }
        fresh_runtime(world)?;
        capture::quiescent(world, false)?;
        Rooms::require_closed(world).map_err(refuse)?;
        if capture::compatibility(world)? != state.game.compatibility {
            return Err(refuse(
                "resume Schema or App configuration differs from the candidate",
            ));
        }
        if world.resource::<FixedTime>().catch_up_policy() != FixedCatchUpPolicy::PreserveDebt {
            return Err(refuse(
                "processed continuation requires the Server debt policy",
            ));
        }
        let queue = world.resource::<GameplayCommandQueue>();
        let settings = queue.settings();
        if state.pending.len() > settings.retained_commands().get()
            || state
                .pending
                .last()
                .is_some_and(|command| command.tick_offset > settings.future_ticks().get())
        {
            return Err(refuse(
                "pending continuation exceeds the candidate queue's limits",
            ));
        }
        validate_current_placement(world, &state.game)
    })??;

    let mut initializer = state.clone();
    initializer.pending.clear();
    let entry = restore_command(encode_processed_resume(&initializer)?)?;
    check_deadline(deadline)?;
    enter_headless_resume(session, state.game.runtime_time, entry)?;
    let fixed_tick = loop {
        check_deadline(deadline)?;
        let status = session.capture(|world| -> Result<_, GameResumeError> {
            let snapshot = world.resource::<GameSnapshot>();
            if let Some(error) = &snapshot.error {
                return Err(refuse(error.clone()));
            }
            let control = world.resource::<ResumeControl>();
            if control.rejected || (!snapshot.restoring && !control.restored) {
                return Err(refuse("the Server reconstruction did not commit"));
            }
            match world.resource::<TaskPools>().return_state() {
                TaskReturnState::Returned if control.restored => {
                    Ok(Some(world.resource::<FixedTime>().tick()))
                }
                TaskReturnState::Returned | TaskReturnState::Pending => Ok(None),
                TaskReturnState::CleanupFailed => {
                    Err(refuse("original Server task cleanup failed"))
                }
            }
        })??;
        if let Some(fixed_tick) = status {
            break fixed_tick;
        }
        if session.frame(Duration::ZERO)?.is_some() {
            return Err(refuse(
                "Server candidate exited during paused reconstruction",
            ));
        }
        thread::sleep(Duration::from_millis(1));
    };
    for (index, pending) in state.pending.iter().enumerate() {
        check_deadline(deadline)?;
        let tick = fixed_tick
            .checked_add(pending.tick_offset)
            .and_then(GameplayCommandTick::new)
            .ok_or_else(|| refuse("pending command tick cannot be represented in the candidate"))?;
        let sequence = GameplayCommandSourceSequence::new(index as u64 + 1)
            .expect("bounded pending command ordinal");
        let source = GameplayCommandIngressSource::external(SERVER_COMMAND_SOURCE)
            .expect("static Server command source");
        let draft = pending.command.clone().into_draft().map_err(refuse)?;
        session.submit(GameplayCommandSubmission::new(
            tick, source, sequence, draft,
        ))?;
    }
    let actual = capture_processed_resume(session)?;
    if encode_processed_resume(&actual)? != expected {
        return Err(refuse(
            "Server reconstruction differs from the complete canonical state",
        ));
    }
    check_deadline(deadline)
}

fn check_deadline(deadline: Instant) -> Result<(), GameResumeError> {
    if Instant::now() >= deadline {
        return Err(refuse(
            "Server reconstruction reached its cooperative deadline",
        ));
    }
    Ok(())
}

fn refuse(message: impl Into<String>) -> GameResumeError {
    GameResumeError::Ineligible(message.into())
}
