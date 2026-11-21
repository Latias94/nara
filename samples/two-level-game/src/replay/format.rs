//! Bounded source-game programs and recordings, without Runtime or command ingress authority.
//!
//! Use the decode functions for external bytes. Derived Serde implementations alone do not
//! enforce this profile's byte, structural, command, or cross-step invariants. Recorded results
//! still need comparison with fresh source admission and actual playback.

use std::io::{self, Write};

use nara::{
    core::{ByteLimit, DepthLimit, ItemLimit, SerdeShapeLimits, preflight_serde_shape},
    gameplay::{
        GameplayCommandEnvelope, GameplayCommandKey, GameplayCommandSource,
        MAX_GAMEPLAY_COMMAND_ID_BYTES,
    },
};
use serde::{Deserialize, Serialize};

use crate::{
    command::SemanticCommand,
    resume::{
        GameClock, LevelContentStamp, PlayerState, ResumeCompatibility, ResumeState,
        RuntimeTimeState, SessionCounters, encode_resume,
    },
    save::Checkpoint,
};

const PROGRAM_FORMAT_VERSION: u32 = 1;
pub(super) const RECORDING_FORMAT_VERSION: u32 = 2;
pub const MAX_STEPS: usize = 1024;
pub const MAX_COMMANDS_PER_STEP: usize = 8;
pub const MAX_RECORDING_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_PROGRAM_BYTES: usize = MAX_RECORDING_BYTES;
pub(crate) const RECORDING_SOURCE: &str = "two-level-game.recording";

/// An admitted finite sequence of exact steps; an empty row still advances one engine tick.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GameProgram {
    format: u32,
    steps: Vec<Vec<SemanticCommand>>,
}

impl GameProgram {
    /// Admits the complete program before a recording Runtime is started.
    pub fn new(steps: Vec<Vec<SemanticCommand>>) -> Result<Self, String> {
        validate_step_count(steps.len())?;
        for (index, commands) in steps.iter().enumerate() {
            validate_commands(commands.iter(), index + 1)?;
        }
        Ok(Self {
            format: PROGRAM_FORMAT_VERSION,
            steps,
        })
    }

    /// Returns the original command order within each exact step.
    pub fn steps(&self) -> &[Vec<SemanticCommand>] {
        &self.steps
    }
}

/// Checks byte and shape limits before allocating and admitting a semantic program.
pub fn decode_program(bytes: &[u8]) -> Result<GameProgram, String> {
    preflight(bytes, MAX_PROGRAM_BYTES)?;
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Wire {
        format: u32,
        steps: Vec<Vec<SemanticCommand>>,
    }
    let wire: Wire = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if wire.format != PROGRAM_FORMAT_VERSION {
        return Err(format!(
            "unsupported source-game replay format {}",
            wire.format
        ));
    }
    GameProgram::new(wire.steps)
}

/// Encodes an admitted program as version-one JSON without file I/O.
pub fn encode_program(program: &GameProgram) -> Result<Vec<u8>, String> {
    let bytes = serde_json::to_vec(program).map_err(|error| error.to_string())?;
    preflight(&bytes, MAX_PROGRAM_BYTES)?;
    Ok(bytes)
}

/// Supported game state, excluding the recording and playback recipes' different configurations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedState {
    pub progress: Checkpoint,
    pub player: PlayerState,
    pub held: [bool; 4],
    pub clock: GameClock,
    #[serde(deserialize_with = "RuntimeTimeState::deserialize_object")]
    pub runtime_time: RuntimeTimeState,
    pub counters: SessionCounters,
    pub schema: [u8; 32],
    pub levels: Vec<LevelContentStamp>,
}

impl From<ResumeState> for RecordedState {
    fn from(state: ResumeState) -> Self {
        Self {
            progress: state.progress,
            player: state.player,
            held: state.held,
            clock: state.clock,
            runtime_time: state.runtime_time,
            counters: state.counters,
            schema: state.compatibility.schema,
            levels: state.compatibility.levels,
        }
    }
}

impl RecordedState {
    /// Reuses resume validation and canonicalization with a neutral configuration fingerprint.
    ///
    /// Compare these bytes rather than collection order or recipe-specific App configurations.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, String> {
        encode_resume(&ResumeState {
            progress: self.progress.clone(),
            player: self.player,
            held: self.held,
            clock: self.clock,
            runtime_time: self.runtime_time,
            counters: self.counters,
            compatibility: ResumeCompatibility {
                schema: self.schema,
                configuration: [0; 32],
                levels: self.levels.clone(),
            },
        })
        .map_err(|error| error.to_string())
    }
}

/// A closed semantic command and its original Capture key, retained as provenance data only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedCommand {
    pub command: SemanticCommand,
    pub original: GameplayCommandKey,
}

impl RecordedCommand {
    /// Extracts the exact supported semantics from an actual recording Capture envelope.
    pub(crate) fn from_envelope(envelope: &GameplayCommandEnvelope) -> Result<Self, String> {
        validate_source(envelope.key())?;
        Ok(Self {
            command: SemanticCommand::from_envelope(envelope)?,
            original: envelope.key().clone(),
        })
    }
}

/// One successful engine step and the game state after its paused maintenance settles.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordedStep {
    pub tick: u64,
    pub commands: Vec<RecordedCommand>,
    pub after: RecordedState,
}

/// A finite version-two recording from the fixed source/headless semantic-program profile.
///
/// The initial state and every completed step have zero pending whole and fractional time.
/// Configuration is original recording provenance. Playback uses its own Replay-only recipe
/// and compares [`RecordedState::canonical_bytes`] without importing old command authority.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameRecording {
    pub format: u32,
    pub configuration: [u8; 32],
    pub initial: RecordedState,
    pub steps: Vec<RecordedStep>,
}

impl GameRecording {
    /// Checks a complete import or finished recording, without starting or advancing a Runtime.
    ///
    /// State validation belongs to the resume encoder. Whole-file byte accounting is linear;
    /// live recording must charge each new row incrementally rather than call this after each row.
    pub fn validate(&self) -> Result<(), String> {
        validate_recording_version(self.format)?;
        validate_step_count(self.steps.len())?;
        self.initial
            .canonical_bytes()
            .map_err(|error| format!("recording initial state: {error}"))?;
        if self.initial.clock.ticks != 0 || self.initial.runtime_time.has_pending_time() {
            return Err("recording must start at logical tick zero with no pending time".into());
        }

        let mut previous = &self.initial;
        let mut previous_sequence = 0;
        for (index, step) in self.steps.iter().enumerate() {
            let expected_tick = index as u64 + 1;
            if step.tick != expected_tick {
                return Err(format!(
                    "recording step {} has engine tick {}, expected {expected_tick}",
                    index + 1,
                    step.tick,
                ));
            }
            validate_commands(step.commands.iter().map(|entry| &entry.command), index + 1)?;
            for command in &step.commands {
                validate_source(&command.original)?;
                if command.original.tick().get() != step.tick {
                    return Err(format!(
                        "recording command tick disagrees with step {}",
                        step.tick
                    ));
                }
                let sequence = command.original.source_sequence().get();
                if sequence <= previous_sequence {
                    return Err("original recording source sequence must strictly increase".into());
                }
                previous_sequence = sequence;
            }

            step.after
                .canonical_bytes()
                .map_err(|error| format!("recording step {} state: {error}", step.tick))?;
            if step.after.runtime_time.has_pending_time() {
                return Err(format!(
                    "recording step {} must end with no pending time",
                    step.tick
                ));
            }
            if step.after.schema != self.initial.schema {
                return Err(format!("recording Schema changed at step {}", step.tick));
            }
            for learned in &previous.levels {
                if !step.after.levels.contains(learned) {
                    return Err(format!(
                        "recording content stamp changed or disappeared for {:?} at step {}",
                        learned.level, step.tick,
                    ));
                }
            }
            previous = &step.after;
        }

        serde_json::to_writer(ByteCharge::default(), self).map_err(|error| error.to_string())
    }
}

/// Checks the entire file's limits and invariants before a fresh playback Runtime can start.
pub fn decode_recording(bytes: &[u8]) -> Result<GameRecording, String> {
    preflight(bytes, MAX_RECORDING_BYTES)?;
    let recording: GameRecording =
        serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    recording.validate()?;
    Ok(recording)
}

/// Validates and encodes one complete recording, bounded to sixteen MiB.
pub fn encode_recording(recording: &GameRecording) -> Result<Vec<u8>, String> {
    recording.validate()?;
    let bytes = serde_json::to_vec(recording).map_err(|error| error.to_string())?;
    preflight(&bytes, MAX_RECORDING_BYTES)?;
    Ok(bytes)
}

fn validate_recording_version(format: u32) -> Result<(), String> {
    if format != RECORDING_FORMAT_VERSION {
        return Err(format!("unsupported source-game recording format {format}"));
    }
    Ok(())
}

fn validate_step_count(count: usize) -> Result<(), String> {
    if count > MAX_STEPS {
        return Err(format!(
            "source-game replay exceeds its {MAX_STEPS}-step limit"
        ));
    }
    Ok(())
}

fn validate_commands<'a>(
    commands: impl Iterator<Item = &'a SemanticCommand>,
    step: usize,
) -> Result<(), String> {
    let mut actions = 0;
    for (index, command) in commands.enumerate() {
        if index >= MAX_COMMANDS_PER_STEP {
            return Err(format!(
                "step {step} exceeds its {MAX_COMMANDS_PER_STEP}-command limit"
            ));
        }
        command
            .validate()
            .map_err(|error| format!("step {step}: {error}"))?;
        if !matches!(command, SemanticCommand::Move { .. }) {
            actions += 1;
            if actions > 1 {
                return Err(format!(
                    "step {step} contains more than one non-Move operation"
                ));
            }
        }
    }
    Ok(())
}

fn validate_source(key: &GameplayCommandKey) -> Result<(), String> {
    match key.source() {
        GameplayCommandSource::External { producer } if producer.as_str() == RECORDING_SOURCE => {
            Ok(())
        }
        _ => Err(format!(
            "recording commands require the External producer {RECORDING_SOURCE}"
        )),
    }
}

fn preflight(bytes: &[u8], maximum: usize) -> Result<(), String> {
    if bytes.len() > maximum {
        return Err(format!(
            "source-game replay exceeds its {maximum}-byte limit"
        ));
    }
    let limits = SerdeShapeLimits::new(
        DepthLimit::new(16).expect("the replay depth limit is non-zero"),
        ItemLimit::new(maximum).expect("the replay node limit is non-zero"),
        ItemLimit::new(MAX_STEPS).expect("the replay container limit is non-zero"),
        ByteLimit::new(MAX_GAMEPLAY_COMMAND_ID_BYTES).expect("the replay string limit is non-zero"),
        ByteLimit::new(maximum).expect("the replay total string limit is non-zero"),
    );
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut deserializer, limits).map_err(|error| error.to_string())?;
    deserializer.end().map_err(|error| error.to_string())
}

#[derive(Default)]
struct ByteCharge {
    bytes: usize,
}

impl Write for ByteCharge {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let total = self
            .bytes
            .checked_add(bytes.len())
            .filter(|total| *total <= MAX_RECORDING_BYTES)
            .ok_or_else(|| io::Error::other("game recording exceeds its sixteen-MiB byte limit"))?;
        self.bytes = total;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
