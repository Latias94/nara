use std::{
    path::Path,
    thread,
    time::{Duration, Instant},
};

use nara::{
    app::{FixedCatchUpPolicy, FixedTime},
    project_host::HeadlessSession,
    tasks::{TaskPools, TaskReturnState},
};

use super::{
    GameResumeError, ResumeControl, capture, enter_headless_resume, fresh_runtime, restore_command,
    validate_current_placement,
};
use crate::{
    GameSnapshot,
    command::{
        CommandSchedule, CommandSequence, CommandSource, CommandTick, GameCommand,
        MAX_COMMAND_TICK_OFFSET, MAX_SCHEDULED_COMMANDS, SERVER_COMMAND_SOURCE, ScheduledCommand,
    },
    gameplay::{side_room::Rooms, storage::Storage},
    resume::{
        RuntimeCatchUpPolicy, RuntimeTimeState,
        processed::{
            MAX_PENDING_SERVER_COMMANDS, PendingServerCommand, ProcessedResumeError,
            ProcessedResumeState, encode_processed_resume,
        },
    },
    save::io::SaveSlot,
};

/// Captures the game, live rooms and pending external intent from a completed paused Server frame.
///
/// The one supported producer is the game's local Server command stream. Commands retain their
/// relative fixed tick and canonical order. Historical queue keys and service handles do not
/// cross the process boundary. A coordinated checkpoint contributes its verified local baseline.
/// An unsettled room, changed checkpoint, or unknown producer refuses capture.
pub fn capture_processed_resume(
    session: &HeadlessSession,
) -> Result<ProcessedResumeState, GameResumeError> {
    capture_state(session, None)
}

fn capture_state(
    session: &HeadlessSession,
    staged_checkpoint: Option<&SaveSlot>,
) -> Result<ProcessedResumeState, GameResumeError> {
    let time = RuntimeTimeState::from_continuation(session.capture_time_continuation()?)?;
    session.capture(|world| {
        let catalog = capture::hosted_catalog(world)?.into();
        let rooms = Rooms::capture_processed(world).map_err(refuse)?;
        capture::quiescent(world, true)?;
        let checkpoint = match staged_checkpoint {
            Some(slot) => Some(slot.handoff_stamp()?),
            None => world.resource::<Storage>().handoff_stamp()?,
        };
        if time.catch_up_policy != RuntimeCatchUpPolicy::PreserveDebt {
            return Err(refuse(
                "processed continuation requires the Server debt policy",
            ));
        }
        let queue = world.resource::<CommandSchedule>();
        let stats = queue.stats();
        if stats.pending_commands > MAX_PENDING_SERVER_COMMANDS {
            return Err(
                ProcessedResumeError::TooManyPendingCommands(stats.pending_commands).into(),
            );
        }
        let mut pending = Vec::with_capacity(stats.pending_commands);
        for (key, intent) in queue.pending_commands() {
            if !key.source().is_external(SERVER_COMMAND_SOURCE) {
                return Err(refuse("pending command belongs to an unsupported producer"));
            }
            let tick_offset = key
                .tick()
                .get()
                .checked_sub(stats.acknowledged_through_tick)
                .ok_or_else(|| refuse("pending command precedes the acknowledged tick"))?;
            let GameCommand::Player(command) = intent else {
                return Err(refuse("pending command is not a Server operation"));
            };
            pending.push(PendingServerCommand {
                tick_offset,
                command: command.clone(),
            });
        }
        let game = capture::game(world, time, capture::compatibility(world)?)?;
        let state = ProcessedResumeState {
            game,
            catalog,
            rooms,
            checkpoint,
            pending,
        };
        state.validate()?;
        Ok(state)
    })?
}

/// Reconstructs a fresh paused processed Server and compares its complete canonical capture.
///
/// Callers retain and close the original candidate after any failure. A failure after entry may
/// leave that candidate changed. These operations do not act on the incumbent or adopt a route.
/// The deadline bounds cooperative maintenance around ordinary Runtime and Scene operations.
/// A captured checkpoint needs the separately supplied absolute path. The checkpoint remains
/// unavailable for Save and Restore until [`claim_processed_checkpoint`] succeeds.
pub fn restore_processed_resume(
    session: &mut HeadlessSession,
    state: &ProcessedResumeState,
    checkpoint_path: Option<&Path>,
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
        capture::require_unconfigured_checkpoint(world)?;
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
        if state.pending.len() > MAX_SCHEDULED_COMMANDS
            || state
                .pending
                .last()
                .is_some_and(|command| command.tick_offset > MAX_COMMAND_TICK_OFFSET)
        {
            return Err(refuse(
                "pending continuation exceeds the candidate queue's limits",
            ));
        }
        validate_current_placement(world, &state.game)
    })??;

    let staged = match (state.checkpoint, checkpoint_path) {
        (Some(stamp), Some(path)) => Some(SaveSlot::prepare_server_handoff(path, stamp)?),
        (None, None) => None,
        _ => {
            return Err(refuse(
                "the checkpoint option must match the captured Server state",
            ));
        }
    };
    let mut initializer = state.clone();
    initializer.pending.clear();
    initializer.checkpoint = None;
    let entry = restore_command(encode_processed_resume(&initializer)?);
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
            .and_then(CommandTick::new)
            .ok_or_else(|| refuse("pending command tick cannot be represented in the candidate"))?;
        let sequence =
            CommandSequence::new(index as u64 + 1).expect("bounded pending command ordinal");
        let source =
            CommandSource::external(SERVER_COMMAND_SOURCE).expect("static Server command source");
        let intent = pending.command.clone().into_intent().map_err(refuse)?;
        ScheduledCommand::new(tick, source, sequence, intent).submit_headless(session)?;
    }
    if !state.pending.is_empty() && session.frame(Duration::ZERO)?.is_some() {
        return Err(refuse(
            "Server candidate exited after restoring pending operations",
        ));
    }
    let actual = capture_state(session, staged.as_ref())?;
    if encode_processed_resume(&actual)? != expected {
        return Err(refuse(
            "Server reconstruction differs from the complete canonical state",
        ));
    }
    check_deadline(deadline)?;
    if let Some(slot) = staged {
        session.capture(|world| world.resource::<Storage>().stage_server_slot(slot))??;
    }
    Ok(())
}

/// Claims the candidate's staged checkpoint after the incumbent releases the writer lock.
///
/// The original checkpoint and lock marker are rechecked under the new lock. A failed claim
/// leaves the candidate paused with the same staged checkpoint observation.
pub fn claim_processed_checkpoint(session: &HeadlessSession) -> Result<(), GameResumeError> {
    session.capture(|world| -> Result<(), GameResumeError> {
        capture::hosted_catalog(world)?;
        world.resource::<Storage>().claim_server_slot()?;
        Ok(())
    })?
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
