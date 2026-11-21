//! Transient continuation data for the cooked Two-Level Server.

use std::{fmt, marker::PhantomData};

use nara::fs::ContentDigest;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{MapAccess, Visitor, value::MapAccessDeserializer},
};

use super::{GAME_COMPATIBILITY, Header, MAX_RESUME_BYTES, ResumeError, ResumeState, preflight};
use crate::command::ServerCommand;

const KIND: &str = "two_level_processed_resume";
const FORMAT_VERSION: u32 = 4;
const MAX_TICK_OFFSET: u64 = 600;

/// Maximum encoded size of a processed continuation, including any JSON whitespace.
pub const MAX_PROCESSED_RESUME_BYTES: usize = MAX_RESUME_BYTES;

/// Maximum number of pending commands retained in one processed continuation.
pub const MAX_PENDING_SERVER_COMMANDS: usize = 128;

/// The game state, room states, catalog identity, and ordered pending intent of a paused Server.
///
/// Use [`decode_processed_resume`] to admit external bytes. The receiving game must check
/// compatibility and admit the commands against its own freshly constructed queue.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessedResumeState {
    #[serde(deserialize_with = "deserialize_object")]
    pub game: ResumeState<ProcessedResumeCompatibility>,
    #[serde(deserialize_with = "deserialize_object")]
    pub rooms: ProcessedRooms,
    #[serde(deserialize_with = "deserialize_object")]
    pub catalog: ProcessedCatalogStamp,
    /// Commitment to the observed same-host SaveSlot marker and conflict baseline.
    ///
    /// An unconfigured slot is encoded as `null`. The receiving Server verifies the
    /// commitment against its own slot and obtains write access separately.
    #[serde(deserialize_with = "Deserialize::deserialize")]
    pub checkpoint: Option<[u8; 32]>,
    pub pending: Vec<PendingServerCommand>,
}

/// Room states recorded at the completed frame boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessedRooms {
    #[serde(deserialize_with = "deserialize_room_state")]
    pub side_room: ProcessedRoomState,
    #[serde(deserialize_with = "deserialize_room_state")]
    pub lookout: ProcessedRoomState,
}

/// The room states supported by a processed continuation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessedRoomState {
    #[default]
    Closed,
    Open,
    RelicFound,
}

fn deserialize_room_state<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<ProcessedRoomState, D::Error> {
    let state = String::deserialize(deserializer)?;
    ProcessedRoomState::deserialize(serde::de::value::StrDeserializer::new(&state))
}

/// The committed Server's Schema and App configuration fingerprints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ProcessedResumeCompatibility {
    pub schema: [u8; 32],
    pub configuration: [u8; 32],
}

impl<'de> Deserialize<'de> for ProcessedResumeCompatibility {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            schema: [u8; 32],
            configuration: [u8; 32],
        }

        let Wire {
            schema,
            configuration,
        } = deserialize_object(deserializer)?;
        Ok(Self {
            schema,
            configuration,
        })
    }
}

/// The complete recorded digest of the admitted processed catalog.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessedCatalogStamp {
    pub length: u64,
    pub hash: [u8; 32],
}

impl From<ContentDigest> for ProcessedCatalogStamp {
    fn from(digest: ContentDigest) -> Self {
        Self {
            length: digest.length(),
            hash: *digest.as_bytes(),
        }
    }
}

impl From<ProcessedCatalogStamp> for ContentDigest {
    fn from(stamp: ProcessedCatalogStamp) -> Self {
        Self::from_parts(stamp.length, stamp.hash)
    }
}

/// A portable command scheduled relative to the capture queue's acknowledged tick.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PendingServerCommand {
    pub tick_offset: u64,
    pub command: ServerCommand,
}

impl<'de> Deserialize<'de> for PendingServerCommand {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            tick_offset: u64,
            #[serde(deserialize_with = "PendingCommandWire::deserialize")]
            command: ServerCommand,
        }

        let Wire {
            tick_offset,
            command,
        } = deserialize_object(deserializer)?;
        Ok(Self {
            tick_offset,
            command,
        })
    }
}

#[derive(Deserialize)]
#[serde(remote = "ServerCommand", rename_all = "snake_case")]
enum PendingCommandWire {
    Gameplay(#[serde(deserialize_with = "deserialize_object")] crate::command::SemanticCommand),
    SideRoom,
    Lookout,
    Save,
    Restore,
}

impl ProcessedResumeState {
    /// Checks the portable game state and pending queue order before capture or restoration.
    pub fn validate(&self) -> Result<(), ProcessedResumeError> {
        self.game.validate_game_state()?;
        if self.pending.len() > MAX_PENDING_SERVER_COMMANDS {
            return Err(ProcessedResumeError::TooManyPendingCommands(
                self.pending.len(),
            ));
        }

        let mut previous_offset = 0;
        for (index, entry) in self.pending.iter().enumerate() {
            let tick_offset = entry.tick_offset;
            if !(1..=MAX_TICK_OFFSET).contains(&tick_offset) {
                return Err(ProcessedResumeError::InvalidTickOffset { index, tick_offset });
            }
            if tick_offset < previous_offset {
                return Err(ProcessedResumeError::UnorderedTickOffset {
                    index,
                    tick_offset,
                    previous_offset,
                });
            }
            if self.checkpoint.is_none()
                && matches!(&entry.command, ServerCommand::Save | ServerCommand::Restore)
            {
                return Err(ProcessedResumeError::SaveSlotCommand { index });
            }
            entry
                .command
                .validate()
                .map_err(|reason| ProcessedResumeError::InvalidCommand { index, reason })?;
            previous_offset = tick_offset;
        }
        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ProcessedResumeError {
    #[error(transparent)]
    Resume(#[from] ResumeError),
    #[error("processed resume has {0} pending commands (maximum {MAX_PENDING_SERVER_COMMANDS})")]
    TooManyPendingCommands(usize),
    #[error("pending command {index} has tick offset {tick_offset} outside 1..={MAX_TICK_OFFSET}")]
    InvalidTickOffset { index: usize, tick_offset: u64 },
    #[error("pending command {index} has tick offset {tick_offset} before {previous_offset}")]
    UnorderedTickOffset {
        index: usize,
        tick_offset: u64,
        previous_offset: u64,
    },
    #[error("pending command {index} requires a SaveSlot commitment")]
    SaveSlotCommand { index: usize },
    #[error("pending command {index} is invalid: {reason}")]
    InvalidCommand { index: usize, reason: String },
}

/// Produces bounded, canonical version-four bytes, preserving the order of pending commands.
pub fn encode_processed_resume(
    state: &ProcessedResumeState,
) -> Result<Vec<u8>, ProcessedResumeError> {
    state.validate()?;
    let mut state = state.clone();
    state.game.canonicalize_game();
    let bytes = serde_json::to_vec(&Envelope {
        kind: KIND,
        format_version: FORMAT_VERSION,
        game_compatibility: GAME_COMPATIBILITY,
        payload: &state,
    })
    .map_err(ResumeError::Json)?;
    preflight(&bytes)?;
    Ok(bytes)
}

/// Checks the shared byte and shape limits before admitting and canonicalizing the payload.
/// The compatible-package handoff accepts only the current format version.
pub fn decode_processed_resume(bytes: &[u8]) -> Result<ProcessedResumeState, ProcessedResumeError> {
    preflight(bytes)?;
    let header: Header = serde_json::from_slice(bytes).map_err(ResumeError::Json)?;
    if header.kind != KIND {
        return Err(ResumeError::WrongKind.into());
    }
    if header.format_version != FORMAT_VERSION {
        return Err(ResumeError::UnsupportedVersion(header.format_version).into());
    }
    if header.game_compatibility != GAME_COMPATIBILITY {
        return Err(ResumeError::UnsupportedGameCompatibility(header.game_compatibility).into());
    }

    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let wire: Envelope<String, ProcessedResumeState> =
        deserialize_object(&mut deserializer).map_err(ResumeError::Json)?;
    let mut state = wire.payload;
    state.validate()?;
    state.game.canonicalize_game();
    Ok(state)
}

#[derive(Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    bound(deserialize = "K: Deserialize<'de>, P: Deserialize<'de>")
)]
struct Envelope<K, P> {
    kind: K,
    format_version: u32,
    game_compatibility: u32,
    #[serde(deserialize_with = "deserialize_object")]
    payload: P,
}

fn deserialize_object<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<T, D::Error> {
    struct ObjectVisitor<T>(PhantomData<T>);

    impl<'de, T: Deserialize<'de>> Visitor<'de> for ObjectVisitor<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a JSON object")
        }

        fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<T, A::Error> {
            T::deserialize(MapAccessDeserializer::new(map))
        }
    }

    deserializer.deserialize_map(ObjectVisitor(PhantomData))
}

#[cfg(test)]
mod tests;
