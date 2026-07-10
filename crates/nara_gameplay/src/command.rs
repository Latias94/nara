use std::{
    cmp::Ordering,
    collections::BTreeMap,
    fmt::{self, Display, Formatter},
    num::NonZeroU64,
};

use thiserror::Error;

macro_rules! string_id_serde {
    ($type:ty, $constructor:ident) => {
        #[cfg(feature = "serde")]
        impl serde::Serialize for $type {
            fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
            where
                S: serde::Serializer,
            {
                serializer.serialize_str(self.as_str())
            }
        }

        #[cfg(feature = "serde")]
        impl<'de> serde::Deserialize<'de> for $type {
            fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
            where
                D: serde::Deserializer<'de>,
            {
                let id = <String as serde::Deserialize>::deserialize(deserializer)?;
                Self::$constructor(id).map_err(serde::de::Error::custom)
            }
        }
    };
}

pub const MAX_GAMEPLAY_COMMAND_ID_BYTES: usize = 256;
pub const MAX_GAMEPLAY_COMMAND_PAYLOAD_ITEMS: usize = 256;
pub const MAX_GAMEPLAY_COMMAND_PAYLOAD_BYTES: usize = 256 * 1024;
pub const MAX_GAMEPLAY_COMMAND_PAYLOAD_KEY_BYTES: usize = 256;
pub const MAX_GAMEPLAY_COMMAND_STRING_BYTES: usize = 128 * 1024;

const VARIANT_TAG_BYTES: usize = 1;
const U64_BYTES: usize = 8;
const I64_BYTES: usize = 8;
const F64_BYTES: usize = 8;
const BOOL_BYTES: usize = 1;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GameplayCommandTypeId(String);

impl GameplayCommandTypeId {
    pub fn new(id: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        let id = id.into();
        validate_id("command type id", &id)?;
        Ok(Self(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for GameplayCommandTypeId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

string_id_serde!(GameplayCommandTypeId, new);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GameplayCommandSourceId(String);

impl GameplayCommandSourceId {
    pub fn new(id: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        let id = id.into();
        validate_id("command source id", &id)?;
        Ok(Self(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for GameplayCommandSourceId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

string_id_serde!(GameplayCommandSourceId, new);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GameplayCommandTargetId(String);

impl GameplayCommandTargetId {
    pub fn new(id: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        let id = id.into();
        validate_id("named command target", &id)?;
        Ok(Self(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for GameplayCommandTargetId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

string_id_serde!(GameplayCommandTargetId, new);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SceneStableId(String);

impl SceneStableId {
    pub fn new(id: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        let id = id.into();
        validate_id("scene stable id", &id)?;
        Ok(Self(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for SceneStableId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

string_id_serde!(SceneStableId, new);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PersistentRuntimeId(String);

impl PersistentRuntimeId {
    pub fn parse_str(id: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        let id = id.into();
        let id = uuid::Uuid::parse_str(&id).map_err(|_| GameplayCommandIdError::InvalidUuid)?;
        Ok(Self(id.to_string()))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for PersistentRuntimeId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

string_id_serde!(PersistentRuntimeId, parse_str);

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GameplayCommandIdError {
    #[error("{kind} cannot be empty")]
    Empty { kind: &'static str },
    #[error("{kind} cannot contain control characters")]
    ContainsControl { kind: &'static str },
    #[error("{kind} exceeds its byte limit")]
    TooLong {
        kind: &'static str,
        length: usize,
        maximum: usize,
    },
    #[error("persistent runtime id must be a UUID")]
    InvalidUuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GameplayCommandTarget {
    Scene(SceneStableId),
    Persistent(PersistentRuntimeId),
    Named(GameplayCommandTargetId),
}

impl GameplayCommandTarget {
    pub fn named(id: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        GameplayCommandTargetId::new(id).map(Self::Named)
    }

    pub(crate) fn logical_bytes(&self) -> Option<usize> {
        let id_bytes = match self {
            Self::Scene(id) => id.as_str().len(),
            Self::Persistent(id) => id.as_str().len(),
            Self::Named(id) => id.as_str().len(),
        };
        VARIANT_TAG_BYTES.checked_add(id_bytes)
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub enum GameplayCommandValue {
    Bool(bool),
    I64(i64),
    F64(f64),
    String(String),
}

impl GameplayCommandValue {
    fn validate(&self) -> Result<(), GameplayCommandPayloadError> {
        match self {
            Self::F64(value) if !value.is_finite() => {
                Err(GameplayCommandPayloadError::NonFiniteFloat)
            }
            Self::String(value) if value.len() > MAX_GAMEPLAY_COMMAND_STRING_BYTES => {
                Err(GameplayCommandPayloadError::StringTooLong {
                    length: value.len(),
                    maximum: MAX_GAMEPLAY_COMMAND_STRING_BYTES,
                })
            }
            _ => Ok(()),
        }
    }

    fn logical_bytes(&self) -> Option<usize> {
        let value_bytes = match self {
            Self::Bool(_) => BOOL_BYTES,
            Self::I64(_) => I64_BYTES,
            Self::F64(_) => F64_BYTES,
            Self::String(value) => value.len(),
        };
        VARIANT_TAG_BYTES.checked_add(value_bytes)
    }
}

/// Semantically bounded command payload data.
///
/// Deserialization validates decoded keys and values. Format adapters remain responsible for an
/// outer encoded-byte and nesting budget before invoking serde on untrusted input (ADR 0049).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct GameplayCommandPayload {
    values: BTreeMap<String, GameplayCommandValue>,
    logical_bytes: usize,
}

impl GameplayCommandPayload {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(
        &mut self,
        key: impl Into<String>,
        value: GameplayCommandValue,
    ) -> Result<Option<GameplayCommandValue>, GameplayCommandPayloadError> {
        let key = key.into();
        validate_payload_key(&key)?;
        value.validate()?;

        let new_entry_bytes = payload_entry_bytes(&key, &value)?;
        let previous_entry_bytes = self
            .values
            .get(&key)
            .map(|previous| payload_entry_bytes(&key, previous))
            .transpose()?
            .unwrap_or(0);
        let next_items = self
            .values
            .len()
            .checked_add(usize::from(!self.values.contains_key(&key)))
            .ok_or(GameplayCommandPayloadError::LogicalSizeOverflow)?;
        if next_items > MAX_GAMEPLAY_COMMAND_PAYLOAD_ITEMS {
            return Err(GameplayCommandPayloadError::TooManyItems {
                requested: next_items,
                maximum: MAX_GAMEPLAY_COMMAND_PAYLOAD_ITEMS,
            });
        }
        let next_bytes = self
            .logical_bytes
            .checked_sub(previous_entry_bytes)
            .and_then(|bytes| bytes.checked_add(new_entry_bytes))
            .ok_or(GameplayCommandPayloadError::LogicalSizeOverflow)?;
        if next_bytes > MAX_GAMEPLAY_COMMAND_PAYLOAD_BYTES {
            return Err(GameplayCommandPayloadError::TooManyBytes {
                requested: next_bytes,
                maximum: MAX_GAMEPLAY_COMMAND_PAYLOAD_BYTES,
            });
        }

        let previous = self.values.insert(key, value);
        self.logical_bytes = next_bytes;
        Ok(previous)
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&GameplayCommandValue> {
        self.values.get(key)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    #[must_use]
    pub const fn logical_bytes(&self) -> usize {
        self.logical_bytes
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &GameplayCommandValue)> {
        self.values.iter()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for GameplayCommandPayload {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde::Serialize::serialize(&self.values, serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for GameplayCommandPayload {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct PayloadVisitor;

        impl<'de> serde::de::Visitor<'de> for PayloadVisitor {
            type Value = GameplayCommandPayload;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
                formatter.write_str("a bounded gameplay command payload map")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut payload = GameplayCommandPayload::new();
                while let Some((key, value)) = map.next_entry::<String, GameplayCommandValue>()? {
                    if payload.values.contains_key(&key) {
                        return Err(serde::de::Error::custom(
                            GameplayCommandPayloadError::DuplicateKey,
                        ));
                    }
                    payload
                        .insert(key, value)
                        .map_err(serde::de::Error::custom)?;
                }
                Ok(payload)
            }
        }

        deserializer.deserialize_map(PayloadVisitor)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum GameplayCommandPayloadError {
    #[error("payload key cannot be empty")]
    EmptyKey,
    #[error("payload key cannot contain control characters")]
    ContainsControl,
    #[error("payload key exceeds its byte limit")]
    KeyTooLong { length: usize, maximum: usize },
    #[error("payload contains a non-finite float")]
    NonFiniteFloat,
    #[error("payload string exceeds its byte limit")]
    StringTooLong { length: usize, maximum: usize },
    #[error("payload exceeds its item limit")]
    TooManyItems { requested: usize, maximum: usize },
    #[error("payload exceeds its byte limit")]
    TooManyBytes { requested: usize, maximum: usize },
    #[error("payload contains a duplicate key")]
    DuplicateKey,
    #[error("payload logical size overflowed")]
    LogicalSizeOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub enum GameplayCommandIngressSource {
    Test { driver: GameplayCommandSourceId },
    Replay { stream: GameplayCommandSourceId },
    Ai { agent: GameplayCommandSourceId },
    External { producer: GameplayCommandSourceId },
}

impl GameplayCommandIngressSource {
    pub fn test(driver: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        GameplayCommandSourceId::new(driver).map(|driver| Self::Test { driver })
    }

    pub fn replay(stream: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        GameplayCommandSourceId::new(stream).map(|stream| Self::Replay { stream })
    }

    pub fn ai(agent: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        GameplayCommandSourceId::new(agent).map(|agent| Self::Ai { agent })
    }

    pub fn external(producer: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        GameplayCommandSourceId::new(producer).map(|producer| Self::External { producer })
    }

    #[must_use]
    pub fn id(&self) -> &GameplayCommandSourceId {
        match self {
            Self::Test { driver } => driver,
            Self::Replay { stream } => stream,
            Self::Ai { agent } => agent,
            Self::External { producer } => producer,
        }
    }

    pub(crate) fn into_admitted(self) -> GameplayCommandSource {
        match self {
            Self::Test { driver } => GameplayCommandSource::Test { driver },
            Self::Replay { stream } => GameplayCommandSource::Replay { stream },
            Self::Ai { agent } => GameplayCommandSource::Ai { agent },
            Self::External { producer } => GameplayCommandSource::External { producer },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub enum GameplayCommandSource {
    LocalAction,
    Test { driver: GameplayCommandSourceId },
    Replay { stream: GameplayCommandSourceId },
    Ai { agent: GameplayCommandSourceId },
    External { producer: GameplayCommandSourceId },
}

impl GameplayCommandSource {
    pub fn test(driver: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        GameplayCommandSourceId::new(driver).map(|driver| Self::Test { driver })
    }

    pub fn replay(stream: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        GameplayCommandSourceId::new(stream).map(|stream| Self::Replay { stream })
    }

    pub fn ai(agent: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        GameplayCommandSourceId::new(agent).map(|agent| Self::Ai { agent })
    }

    pub fn external(producer: impl Into<String>) -> Result<Self, GameplayCommandIdError> {
        GameplayCommandSourceId::new(producer).map(|producer| Self::External { producer })
    }

    #[must_use]
    pub const fn kind_rank(&self) -> u8 {
        match self {
            Self::LocalAction => 0,
            Self::Test { .. } => 1,
            Self::Replay { .. } => 2,
            Self::Ai { .. } => 3,
            Self::External { .. } => 4,
        }
    }

    #[must_use]
    pub fn id(&self) -> Option<&GameplayCommandSourceId> {
        match self {
            Self::LocalAction => None,
            Self::Test { driver } => Some(driver),
            Self::Replay { stream } => Some(stream),
            Self::Ai { agent } => Some(agent),
            Self::External { producer } => Some(producer),
        }
    }

    pub(crate) fn logical_bytes(&self) -> Option<usize> {
        VARIANT_TAG_BYTES.checked_add(self.id().map_or(0, |id| id.as_str().len()))
    }
}

impl PartialOrd for GameplayCommandSource {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GameplayCommandSource {
    fn cmp(&self, other: &Self) -> Ordering {
        self.kind_rank()
            .cmp(&other.kind_rank())
            .then_with(|| self.id().cmp(&other.id()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct GameplayCommandTick(NonZeroU64);

impl GameplayCommandTick {
    #[must_use]
    pub const fn new(tick: u64) -> Option<Self> {
        match NonZeroU64::new(tick) {
            Some(tick) => Some(Self(tick)),
            None => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct GameplayCommandSourceSequence(NonZeroU64);

impl GameplayCommandSourceSequence {
    #[must_use]
    pub const fn new(sequence: u64) -> Option<Self> {
        match NonZeroU64::new(sequence) {
            Some(sequence) => Some(Self(sequence)),
            None => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct GameplayCommandDraft {
    command_type: GameplayCommandTypeId,
    #[cfg_attr(feature = "serde", serde(default))]
    target: Option<GameplayCommandTarget>,
    #[cfg_attr(feature = "serde", serde(default))]
    payload: GameplayCommandPayload,
}

impl GameplayCommandDraft {
    #[must_use]
    pub fn new(command_type: GameplayCommandTypeId) -> Self {
        Self {
            command_type,
            target: None,
            payload: GameplayCommandPayload::new(),
        }
    }

    #[must_use]
    pub fn with_target(mut self, target: GameplayCommandTarget) -> Self {
        self.target = Some(target);
        self
    }

    #[must_use]
    pub fn with_payload(mut self, payload: GameplayCommandPayload) -> Self {
        self.payload = payload;
        self
    }

    #[must_use]
    pub const fn command_type(&self) -> &GameplayCommandTypeId {
        &self.command_type
    }

    #[must_use]
    pub const fn target(&self) -> Option<&GameplayCommandTarget> {
        self.target.as_ref()
    }

    #[must_use]
    pub const fn payload(&self) -> &GameplayCommandPayload {
        &self.payload
    }

    pub(crate) fn logical_bytes(&self) -> Option<usize> {
        self.command_type
            .as_str()
            .len()
            .checked_add(VARIANT_TAG_BYTES)
            .and_then(|bytes| {
                self.target
                    .as_ref()
                    .map_or(Some(0), GameplayCommandTarget::logical_bytes)
                    .and_then(|target| bytes.checked_add(target))
            })
            .and_then(|bytes| bytes.checked_add(self.payload.logical_bytes()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct GameplayCommandKey {
    tick: GameplayCommandTick,
    source: GameplayCommandSource,
    source_sequence: GameplayCommandSourceSequence,
}

impl GameplayCommandKey {
    #[must_use]
    pub fn new(
        tick: GameplayCommandTick,
        source: GameplayCommandSource,
        source_sequence: GameplayCommandSourceSequence,
    ) -> Self {
        Self {
            tick,
            source,
            source_sequence,
        }
    }

    #[must_use]
    pub const fn tick(&self) -> GameplayCommandTick {
        self.tick
    }

    #[must_use]
    pub const fn source(&self) -> &GameplayCommandSource {
        &self.source
    }

    #[must_use]
    pub const fn source_sequence(&self) -> GameplayCommandSourceSequence {
        self.source_sequence
    }

    pub(crate) fn into_source_sequence(
        self,
    ) -> (GameplayCommandSource, GameplayCommandSourceSequence) {
        (self.source, self.source_sequence)
    }

    pub(crate) fn logical_bytes(&self) -> Option<usize> {
        U64_BYTES.checked_add(U64_BYTES).and_then(|bytes| {
            self.source
                .logical_bytes()
                .and_then(|source| bytes.checked_add(source))
        })
    }
}

/// Explicit non-local command ingress for one authoritative tick.
///
/// `Deserialize` is a semantic data-model implementation, not an untrusted transport reader.
/// Replay, file, and network adapters must enforce their encoded-byte/depth budget first.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct GameplayCommandSubmission {
    tick: GameplayCommandTick,
    source: GameplayCommandIngressSource,
    source_sequence: GameplayCommandSourceSequence,
    command: GameplayCommandDraft,
}

impl GameplayCommandSubmission {
    #[must_use]
    pub fn new(
        tick: GameplayCommandTick,
        source: GameplayCommandIngressSource,
        source_sequence: GameplayCommandSourceSequence,
        command: GameplayCommandDraft,
    ) -> Self {
        Self {
            tick,
            source,
            source_sequence,
            command,
        }
    }

    #[must_use]
    pub fn key(&self) -> GameplayCommandKey {
        GameplayCommandKey::new(
            self.tick,
            self.source.clone().into_admitted(),
            self.source_sequence,
        )
    }

    #[must_use]
    pub const fn tick(&self) -> GameplayCommandTick {
        self.tick
    }

    #[must_use]
    pub const fn source(&self) -> &GameplayCommandIngressSource {
        &self.source
    }

    #[must_use]
    pub const fn source_sequence(&self) -> GameplayCommandSourceSequence {
        self.source_sequence
    }

    #[must_use]
    pub const fn command(&self) -> &GameplayCommandDraft {
        &self.command
    }

    #[must_use]
    pub fn logical_bytes(&self) -> Option<usize> {
        self.key().logical_bytes().and_then(|bytes| {
            self.command
                .logical_bytes()
                .and_then(|command| bytes.checked_add(command))
        })
    }

    pub(crate) fn into_parts(self) -> (GameplayCommandKey, GameplayCommandDraft) {
        (
            GameplayCommandKey::new(self.tick, self.source.into_admitted(), self.source_sequence),
            self.command,
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct GameplayCommandEnvelope {
    #[cfg_attr(feature = "serde", serde(flatten))]
    key: GameplayCommandKey,
    command: GameplayCommandDraft,
    #[cfg_attr(feature = "serde", serde(skip))]
    logical_bytes: usize,
}

impl GameplayCommandEnvelope {
    pub(crate) fn admitted(
        key: GameplayCommandKey,
        command: GameplayCommandDraft,
        logical_bytes: usize,
    ) -> Self {
        Self {
            key,
            command,
            logical_bytes,
        }
    }

    #[must_use]
    pub const fn key(&self) -> &GameplayCommandKey {
        &self.key
    }

    #[must_use]
    pub const fn tick(&self) -> GameplayCommandTick {
        self.key.tick()
    }

    #[must_use]
    pub const fn source(&self) -> &GameplayCommandSource {
        self.key.source()
    }

    #[must_use]
    pub const fn source_sequence(&self) -> GameplayCommandSourceSequence {
        self.key.source_sequence()
    }

    #[must_use]
    pub const fn command(&self) -> &GameplayCommandDraft {
        &self.command
    }

    #[must_use]
    pub const fn command_type(&self) -> &GameplayCommandTypeId {
        self.command.command_type()
    }

    #[must_use]
    pub const fn target(&self) -> Option<&GameplayCommandTarget> {
        self.command.target()
    }

    #[must_use]
    pub const fn payload(&self) -> &GameplayCommandPayload {
        self.command.payload()
    }

    #[must_use]
    pub const fn logical_bytes(&self) -> usize {
        self.logical_bytes
    }
}

fn validate_id(kind: &'static str, id: &str) -> Result<(), GameplayCommandIdError> {
    if id.is_empty() {
        return Err(GameplayCommandIdError::Empty { kind });
    }
    if id.chars().any(char::is_control) {
        return Err(GameplayCommandIdError::ContainsControl { kind });
    }
    if id.len() > MAX_GAMEPLAY_COMMAND_ID_BYTES {
        return Err(GameplayCommandIdError::TooLong {
            kind,
            length: id.len(),
            maximum: MAX_GAMEPLAY_COMMAND_ID_BYTES,
        });
    }
    Ok(())
}

fn validate_payload_key(key: &str) -> Result<(), GameplayCommandPayloadError> {
    if key.is_empty() {
        return Err(GameplayCommandPayloadError::EmptyKey);
    }
    if key.chars().any(char::is_control) {
        return Err(GameplayCommandPayloadError::ContainsControl);
    }
    if key.len() > MAX_GAMEPLAY_COMMAND_PAYLOAD_KEY_BYTES {
        return Err(GameplayCommandPayloadError::KeyTooLong {
            length: key.len(),
            maximum: MAX_GAMEPLAY_COMMAND_PAYLOAD_KEY_BYTES,
        });
    }
    Ok(())
}

fn payload_entry_bytes(
    key: &str,
    value: &GameplayCommandValue,
) -> Result<usize, GameplayCommandPayloadError> {
    key.len()
        .checked_add(VARIANT_TAG_BYTES)
        .and_then(|bytes| {
            value
                .logical_bytes()
                .and_then(|value| bytes.checked_add(value))
        })
        .ok_or(GameplayCommandPayloadError::LogicalSizeOverflow)
}
