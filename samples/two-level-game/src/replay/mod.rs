//! Finite source-game recordings made through the original exact-step Runtime.
//!
//! Each row includes actual successful command consumption and the supported game outcome after
//! Scene activation and reconstruction settle. Loading waits use paused maintenance, never extra
//! fixed ticks. Playback owns a fresh Replay-only queue and checks every newly produced outcome.
//! This profile excludes native output, SaveSlots, arbitrary live input and cooked Player replay.
//! Startup panics retain the engine's original quarantine contract; this tool does not turn an
//! empty outer Session into a physical-retirement receipt for that quarantined owner.

mod format;
mod runtime;

use std::{
    fmt,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::Path,
    thread,
    time::{Duration, Instant},
};

use nara::project_host::{HeadlessSession, HeadlessSessionClose};
use serde::Serialize;

pub use format::{
    GameProgram, GameRecording, RecordedCommand, RecordedState, RecordedStep, decode_program,
    decode_recording, encode_program, encode_recording,
};
use runtime::{Game, Mode};

const DEADLINE: Duration = Duration::from_secs(10);
const MAX_RECORDING_BYTES: usize = 16 * 1024 * 1024;

/// A failure retains the original Session if its physical cleanup is still pending.
pub struct ReplayError {
    message: String,
    cleanup: Option<Box<HeadlessSession>>,
    cleanup_status: HeadlessSessionClose,
}

impl ReplayError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            cleanup: None,
            cleanup_status: HeadlessSessionClose::Complete,
        }
    }

    #[must_use]
    pub fn cleanup_pending(&self) -> bool {
        self.cleanup.is_some()
    }

    /// Retries only the original physical Close; it never resumes failed gameplay.
    pub fn retry_cleanup(&mut self) -> HeadlessSessionClose {
        if let Some(session) = self.cleanup.as_mut() {
            self.cleanup_status = session.close();
            if self.cleanup_status == HeadlessSessionClose::Failed {
                self.message
                    .push_str("; original Runtime physically retired with a reported failure");
            }
            if self.cleanup_status != HeadlessSessionClose::Pending {
                self.cleanup = None;
            }
        }
        self.cleanup_status
    }
}

impl fmt::Display for ReplayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl fmt::Debug for ReplayError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReplayError")
            .field("message", &self.message)
            .field("cleanup_status", &self.cleanup_status)
            .finish()
    }
}

impl std::error::Error for ReplayError {}

impl Drop for ReplayError {
    fn drop(&mut self) {
        if let Some(mut session) = self.cleanup.take()
            && session.close() == HeadlessSessionClose::Pending
        {
            std::mem::forget(session);
        }
    }
}

/// A completed playback; the original Runtime has physically retired before this is returned.
#[derive(Debug, Serialize)]
pub struct ReplayReport {
    pub steps: usize,
    pub commands: usize,
    pub final_state: RecordedState,
}

/// Records a finite semantic program against the actual source game, including empty ticks.
///
/// Commands are copied from the original successful Capture set and published only after its
/// exact-generation acknowledgement and game-owned Scene-ready barrier. No saved result is used
/// to mutate simulation state. The whole operation fails if any row or physical Close fails.
pub fn record_program(project: &Path, program: &GameProgram) -> Result<GameRecording, ReplayError> {
    encode_program(program).map_err(ReplayError::new)?;
    with_game(project, Mode::Record, |game| {
        let initial = game.capture()?;
        let configuration = initial.compatibility.configuration;
        let initial = RecordedState::from(initial);
        let mut bytes = initial.canonical_bytes().map_err(ReplayError::new)?.len() + 1024;
        let mut steps = Vec::with_capacity(program.steps().len());
        for commands in program.steps() {
            let (tick, captured, after) = game.step(commands)?;
            let row = RecordedStep {
                tick,
                commands: captured
                    .iter()
                    .map(RecordedCommand::from_envelope)
                    .collect::<Result<Vec<_>, _>>()
                    .map_err(ReplayError::new)?,
                after: RecordedState::from(after),
            };
            bytes = bytes
                .checked_add(
                    serde_json::to_vec(&row)
                        .map_err(|error| ReplayError::new(error.to_string()))?
                        .len()
                        + 1,
                )
                .ok_or_else(|| ReplayError::new("recording byte accounting overflow"))?;
            if bytes > MAX_RECORDING_BYTES {
                return Err(ReplayError::new(
                    "recording exceeds its retained byte limit",
                ));
            }
            steps.push(row);
        }
        let recording = GameRecording {
            format: 1,
            configuration,
            initial,
            steps,
        };
        encode_recording(&recording).map_err(ReplayError::new)?;
        Ok(recording)
    })
}

/// Reexecutes a validated recording in a fresh game with immutable Replay-only command ingress.
///
/// Record and playback have different fixed queue policies. The game compares its selected state,
/// Schema and admitted content, excluding the App configuration fingerprint from state equality.
/// This is same-profile evidence, not an executable hash or cross-platform determinism promise.
pub fn replay_recording(
    project: &Path,
    recording: &GameRecording,
) -> Result<ReplayReport, ReplayError> {
    recording.validate().map_err(ReplayError::new)?;
    with_game(project, Mode::Replay, |game| {
        let initial = RecordedState::from(game.capture()?);
        compare_state(0, &recording.initial, &initial)?;
        let mut commands = 0;
        let mut final_state = initial;
        for row in &recording.steps {
            let semantic = row
                .commands
                .iter()
                .map(|record| record.command.clone())
                .collect::<Vec<_>>();
            let (tick, _, state) = game.step(&semantic)?;
            if tick != row.tick {
                return Err(ReplayError::new(
                    "playback engine tick differs from recorded step",
                ));
            }
            final_state = RecordedState::from(state);
            compare_state(tick, &row.after, &final_state)?;
            commands += semantic.len();
        }
        Ok(ReplayReport {
            steps: recording.steps.len(),
            commands,
            final_state,
        })
    })
}

fn compare_state(
    tick: u64,
    expected: &RecordedState,
    actual: &RecordedState,
) -> Result<(), ReplayError> {
    if expected.canonical_bytes().map_err(ReplayError::new)?
        != actual.canonical_bytes().map_err(ReplayError::new)?
    {
        return Err(ReplayError::new(format!(
            "playback state mismatch after step {tick}"
        )));
    }
    Ok(())
}

fn with_game<T>(
    project: &Path,
    mode: Mode,
    operation: impl FnOnce(&mut Game) -> Result<T, ReplayError>,
) -> Result<T, ReplayError> {
    let mut game = Game::new(project, mode)?;
    let started = game
        .session
        .start()
        .map_err(|error| ReplayError::new(error.to_string()));
    let result = catch_unwind(AssertUnwindSafe(|| {
        started
            .and_then(|()| game.pause_at_startup())
            .and_then(|()| operation(&mut game))
    }));
    let deadline = Instant::now() + DEADLINE;
    let terminal = loop {
        let status = game.session.close();
        if status != HeadlessSessionClose::Pending || Instant::now() >= deadline {
            break status;
        }
        thread::sleep(Duration::from_millis(1));
    };
    let result = match result {
        Ok(result) => result,
        Err(payload) => {
            if terminal == HeadlessSessionClose::Pending {
                std::mem::forget(game.session);
            }
            resume_unwind(payload);
        }
    };
    match (result, terminal) {
        (Ok(output), HeadlessSessionClose::Complete) => Ok(output),
        (result, terminal) => {
            let mut error = result.err().unwrap_or_else(|| {
                ReplayError::new("original game physical Close did not complete successfully")
            });
            error.cleanup_status = terminal;
            if terminal == HeadlessSessionClose::Pending {
                error.cleanup = Some(Box::new(game.session));
            } else if terminal == HeadlessSessionClose::Failed {
                error
                    .message
                    .push_str("; original Runtime physically retired with a reported failure");
            }
            Err(error)
        }
    }
}
