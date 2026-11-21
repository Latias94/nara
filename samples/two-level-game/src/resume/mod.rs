//! Bounded game-owned continuity data, separate from checkpoint saves and Runtime authority.
//!
//! Decoding checks the complete format, not compatibility with a running or prepared game. The
//! gameplay owner must compare the App configuration, Schema, and content stamps with fresh
//! admission before reconstructing state. No Runtime generation, World entity, input route, or
//! native handle is represented here. Use [`decode_resume`] to admit external bytes; derived Serde
//! alone does not enforce the byte, shape, or semantic limits.

pub mod processed;
mod runtime_time;

pub use runtime_time::{RuntimeCatchUpPolicy, RuntimeTimeState};

use nara::core::{
    ByteLimit, DepthLimit, ItemLimit, SerdeShapeError, SerdeShapeLimits, SerdeShapePreflightError,
    preflight_serde_shape,
};
use serde::{Deserialize, Serialize};

use crate::{
    levels::LevelId,
    save::{Checkpoint, CheckpointError, MAX_OBJECT_ID_BYTES},
};

const KIND: &str = "two_level_resume";
const MAX_TIMESTEP_NANOS: u64 = 1_000_000_000;
pub const CURRENT_FORMAT_VERSION: u32 = 3;
pub const GAME_COMPATIBILITY: u32 = 1;
pub const MAX_RESUME_BYTES: usize = 128 * 1024;

/// Complete semantic state selected by the game for reconstruction in a fresh Runtime.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeState<C = ResumeCompatibility> {
    pub progress: Checkpoint,
    pub player: PlayerState,
    pub held: [bool; 4],
    pub clock: GameClock,
    #[serde(deserialize_with = "RuntimeTimeState::deserialize_object")]
    pub runtime_time: RuntimeTimeState,
    pub counters: SessionCounters,
    pub compatibility: C,
}

/// The active player's local transform, independent of its fresh Scene/World identity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerState {
    pub translation: [f32; 2],
    pub rotation: f32,
    pub scale: [f32; 2],
}

/// Logical completed game time, separate from the fresh Runtime's clock identities.
///
/// Its timestep must match [`RuntimeTimeState::timestep_nanos`].
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameClock {
    pub ticks: u64,
    pub elapsed_nanos: u64,
    pub timestep_nanos: u64,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCounters {
    pub travel_count: u32,
    pub restarts: u32,
    pub saves: u32,
    pub restores: u32,
}

/// Recorded compatibility facts, not executable provenance or proof of current admission.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ResumeCompatibility {
    pub schema: [u8; 32],
    /// The ordinary product App's configuration fingerprint, separate from its Schema digest.
    pub configuration: [u8; 32],
    pub levels: Vec<LevelContentStamp>,
}

/// Full content identities for one catalog level; independent instances share these source facts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LevelContentStamp {
    pub level: LevelId,
    pub manifest: [u8; 32],
    pub content: [u8; 32],
}

impl ResumeState {
    /// Checks bounded, internally consistent game data without reading or activating content.
    pub fn validate(&self) -> Result<(), ResumeError> {
        self.validate_game_state()?;
        self.validate_source_levels()
    }

    fn validate_source_levels(&self) -> Result<(), ResumeError> {
        let levels = &self.compatibility.levels;
        if levels.is_empty() || levels.len() > LevelId::ALL.len() {
            return Err(ResumeError::InvalidContentRecordCount(levels.len()));
        }
        for (index, stamp) in levels.iter().enumerate() {
            if levels[..index]
                .iter()
                .any(|other| other.level == stamp.level)
            {
                return Err(ResumeError::DuplicateLevel(stamp.level));
            }
            if stamp.manifest != levels[0].manifest {
                return Err(ResumeError::ConflictingManifest(stamp.level));
            }
        }
        let required = [LevelId::STARTUP, self.progress.location.level.level()]
            .into_iter()
            .chain(
                self.progress
                    .collected
                    .iter()
                    .map(|pickup| pickup.level.level()),
            )
            .chain(
                self.progress
                    .markers
                    .iter()
                    .map(|marker| marker.level.level()),
            );
        for level in required {
            if !levels.iter().any(|stamp| stamp.level == level) {
                return Err(ResumeError::MissingLevel(level));
            }
        }
        Ok(())
    }

    fn canonicalize(&mut self) {
        self.canonicalize_game();
        self.compatibility.levels.sort_by_key(|stamp| stamp.level);
    }
}

impl<C> ResumeState<C> {
    pub(super) fn validate_game_state(&self) -> Result<(), ResumeError> {
        self.progress.validate()?;
        let expected_score = self.progress.collected.len() as u32 * 10;
        if self.progress.score != expected_score {
            return Err(ResumeError::ScoreMismatch {
                score: self.progress.score,
                collected: self.progress.collected.len(),
            });
        }

        for (axis, maximum) in [280.0, 150.0].into_iter().enumerate() {
            let value = self.player.translation[axis];
            if !value.is_finite() {
                return Err(ResumeError::NonFiniteTranslation { axis });
            }
            if value.abs() > maximum {
                return Err(ResumeError::TranslationOutOfRange {
                    axis,
                    value,
                    maximum,
                });
            }
        }
        if !self.player.rotation.is_finite() {
            return Err(ResumeError::NonFiniteRotation);
        }
        for (axis, value) in self.player.scale.into_iter().enumerate() {
            if !value.is_finite() {
                return Err(ResumeError::NonFiniteScale { axis });
            }
        }

        if self.clock.timestep_nanos == 0 || self.clock.timestep_nanos > MAX_TIMESTEP_NANOS {
            return Err(ResumeError::InvalidTimestep(self.clock.timestep_nanos));
        }
        if self.clock.ticks == u64::MAX {
            return Err(ResumeError::ClockExhausted);
        }
        let expected_elapsed = self
            .clock
            .ticks
            .checked_mul(self.clock.timestep_nanos)
            .ok_or(ResumeError::ClockOverflow)?;
        if self.clock.elapsed_nanos != expected_elapsed {
            return Err(ResumeError::ClockMismatch {
                expected: expected_elapsed,
                actual: self.clock.elapsed_nanos,
            });
        }
        self.runtime_time.validate()?;
        if self.clock.timestep_nanos != self.runtime_time.timestep_nanos {
            return Err(ResumeError::RuntimeTimestepMismatch {
                logical: self.clock.timestep_nanos,
                runtime: self.runtime_time.timestep_nanos,
            });
        }
        Ok(())
    }

    pub(super) fn canonicalize_game(&mut self) {
        self.progress.collected.sort();
        self.progress.markers.sort_by_key(|marker| marker.id);
    }

    pub(crate) fn with_compatibility<D>(self, compatibility: D) -> ResumeState<D> {
        ResumeState {
            progress: self.progress,
            player: self.player,
            held: self.held,
            clock: self.clock,
            runtime_time: self.runtime_time,
            counters: self.counters,
            compatibility,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ResumeError {
    #[error("resume data exceeds its encoded-byte limit")]
    ByteLimitExceeded,
    #[error("resume JSON is invalid")]
    Json(#[source] serde_json::Error),
    #[error("resume data exceeds its structural limits: {0}")]
    Shape(#[source] SerdeShapeError),
    #[error("resume kind does not belong to this game")]
    WrongKind,
    #[error("resume format version {0} is unsupported")]
    UnsupportedVersion(u32),
    #[error("resume game compatibility {0} is unsupported")]
    UnsupportedGameCompatibility(u32),
    #[error("resume progress is invalid: {0}")]
    Progress(#[from] CheckpointError),
    #[error("resume score {score} does not equal ten times its {collected} collected pickups")]
    ScoreMismatch { score: u32, collected: usize },
    #[error("resume player translation axis {axis} is not finite")]
    NonFiniteTranslation { axis: usize },
    #[error("resume player translation axis {axis} is {value}, outside +/-{maximum}")]
    TranslationOutOfRange {
        axis: usize,
        value: f32,
        maximum: f32,
    },
    #[error("resume player rotation is not finite")]
    NonFiniteRotation,
    #[error("resume player scale axis {axis} is not finite")]
    NonFiniteScale { axis: usize },
    #[error("resume timestep {0} ns is outside 1..=1000000000")]
    InvalidTimestep(u64),
    #[error("resume logical tick counter is exhausted")]
    ClockExhausted,
    #[error("resume logical tick count multiplied by its timestep overflows")]
    ClockOverflow,
    #[error("resume elapsed time {actual} ns differs from its tick-derived {expected} ns")]
    ClockMismatch { expected: u64, actual: u64 },
    #[error("resume logical timestep {logical} ns differs from its Runtime timestep {runtime} ns")]
    RuntimeTimestepMismatch { logical: u64, runtime: u64 },
    #[error("resume maximum fixed steps per frame must be non-zero")]
    ZeroMaxStepsPerFrame,
    #[error("resume maximum fixed debt steps must be non-zero")]
    ZeroMaxDebtSteps,
    #[error("resume time scale must be finite and non-negative, got {value}")]
    InvalidTimeScale { value: f32 },
    #[error("resume maximum real frame delta must be non-zero")]
    ZeroMaxDelta,
    #[error("resume pending debt of {pending_steps} ticks exceeds its limit of {max_debt_steps}")]
    ExcessPendingDebt {
        pending_steps: u64,
        max_debt_steps: u32,
    },
    #[error("resume scaled time remainder must be less than one nanosecond")]
    InvalidScaledRemainder,
    #[error("Runtime time duration {field} cannot be represented in u64 nanoseconds")]
    RuntimeDurationOutOfRange { field: &'static str },
    #[error("Runtime time continuation is invalid: {0}")]
    InvalidRuntimeTime(String),
    #[error("resume content record count {0} is outside the nonempty finite level catalog")]
    InvalidContentRecordCount(usize),
    #[error("resume contains duplicate content records for {0:?}")]
    DuplicateLevel(LevelId),
    #[error("resume content record for {0:?} has a conflicting project manifest")]
    ConflictingManifest(LevelId),
    #[error("resume is missing required content records for {0:?}")]
    MissingLevel(LevelId),
}

/// Produces canonical, bounded version-three bytes without file I/O or content admission.
pub fn encode_resume(state: &ResumeState) -> Result<Vec<u8>, ResumeError> {
    state.validate()?;
    let mut state = state.clone();
    state.canonicalize();
    let bytes = serde_json::to_vec(&Envelope {
        kind: KIND,
        format_version: CURRENT_FORMAT_VERSION,
        game_compatibility: GAME_COMPATIBILITY,
        payload: &state,
    })
    .map_err(ResumeError::Json)?;
    preflight(&bytes)?;
    Ok(bytes)
}

/// Checks byte and shape limits before allocating a DTO, then validates and canonicalizes it.
///
/// Successful decoding does not prove that recorded hashes match the fresh App and admitted
/// content or that referenced objects exist. Hashes are full recorded values; no special byte
/// pattern proves or disproves compatibility. This format has no checkpoint-save migration path.
/// Versions one and two are refused because they lack the exact scaled time remainder.
pub fn decode_resume(bytes: &[u8]) -> Result<ResumeState, ResumeError> {
    preflight(bytes)?;
    let header: Header = serde_json::from_slice(bytes).map_err(ResumeError::Json)?;
    if header.kind != KIND {
        return Err(ResumeError::WrongKind);
    }
    if header.format_version != CURRENT_FORMAT_VERSION {
        return Err(ResumeError::UnsupportedVersion(header.format_version));
    }
    if header.game_compatibility != GAME_COMPATIBILITY {
        return Err(ResumeError::UnsupportedGameCompatibility(
            header.game_compatibility,
        ));
    }
    let wire: Wire = serde_json::from_slice(bytes).map_err(ResumeError::Json)?;
    let mut state = wire.payload;
    state.validate()?;
    state.canonicalize();
    Ok(state)
}

fn preflight(bytes: &[u8]) -> Result<(), ResumeError> {
    if bytes.len() > MAX_RESUME_BYTES {
        return Err(ResumeError::ByteLimitExceeded);
    }
    let limits = SerdeShapeLimits::new(
        DepthLimit::new(16).expect("the resume depth limit is non-zero"),
        ItemLimit::new(4096).expect("the resume node limit is non-zero"),
        ItemLimit::new(256).expect("the resume container limit is non-zero"),
        ByteLimit::new(MAX_OBJECT_ID_BYTES).expect("the resume string limit is non-zero"),
        ByteLimit::new(32 * 1024).expect("the resume total string limit is non-zero"),
    );
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    preflight_serde_shape(&mut deserializer, limits).map_err(|error| match error {
        SerdeShapePreflightError::Shape(error) => ResumeError::Shape(error),
        SerdeShapePreflightError::Parse(error) => ResumeError::Json(error),
    })?;
    deserializer.end().map_err(ResumeError::Json)
}

#[derive(Serialize)]
struct Envelope<'a> {
    kind: &'static str,
    format_version: u32,
    game_compatibility: u32,
    payload: &'a ResumeState,
}

#[derive(Deserialize)]
struct Header {
    kind: String,
    format_version: u32,
    game_compatibility: u32,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    #[serde(rename = "kind")]
    _kind: String,
    #[serde(rename = "format_version")]
    _format_version: u32,
    #[serde(rename = "game_compatibility")]
    _game_compatibility: u32,
    payload: ResumeState,
}

#[cfg(test)]
mod tests;
