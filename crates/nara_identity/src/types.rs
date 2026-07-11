use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    num::NonZeroU64,
};

use nara_ecs::Component;
use thiserror::Error;
use uuid::Uuid;

use crate::IdentityAllocationError;

pub const MAX_PERSISTENT_RUNTIME_NAMESPACE_BYTES: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Component)]
pub struct SceneEntityId(String);

impl SceneEntityId {
    pub fn new(id: impl Into<String>) -> Result<Self, SceneEntityIdError> {
        let id = id.into();
        validate_scene_entity_id(&id)?;
        Ok(Self(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for SceneEntityId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for SceneEntityId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for SceneEntityId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(id).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneEntityIdError {
    Empty,
    LeadingSlash,
    TrailingSlash,
    EmptySegment,
    CurrentDirectorySegment,
    ParentDirectorySegment,
    InvalidCharacter(char),
}

impl Display for SceneEntityIdError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("scene entity id is empty"),
            Self::LeadingSlash => formatter.write_str("scene entity id must not start with '/'"),
            Self::TrailingSlash => formatter.write_str("scene entity id must not end with '/'"),
            Self::EmptySegment => formatter.write_str("scene entity id has an empty segment"),
            Self::CurrentDirectorySegment => {
                formatter.write_str("scene entity id must not contain '.' segments")
            }
            Self::ParentDirectorySegment => {
                formatter.write_str("scene entity id must not contain '..' segments")
            }
            Self::InvalidCharacter(character) => {
                write!(
                    formatter,
                    "scene entity id contains invalid character '{character}'"
                )
            }
        }
    }
}

impl Error for SceneEntityIdError {}

fn validate_scene_entity_id(id: &str) -> Result<(), SceneEntityIdError> {
    if id.is_empty() {
        return Err(SceneEntityIdError::Empty);
    }
    if id.starts_with('/') {
        return Err(SceneEntityIdError::LeadingSlash);
    }
    if id.ends_with('/') {
        return Err(SceneEntityIdError::TrailingSlash);
    }

    for segment in id.split('/') {
        match segment {
            "" => return Err(SceneEntityIdError::EmptySegment),
            "." => return Err(SceneEntityIdError::CurrentDirectorySegment),
            ".." => return Err(SceneEntityIdError::ParentDirectorySegment),
            _ => {}
        }
    }

    for character in id.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '_' | '-' | '.' | '/') {
            continue;
        }
        return Err(SceneEntityIdError::InvalidCharacter(character));
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SceneInstanceId(NonZeroU64);

impl SceneInstanceId {
    pub fn new(raw: u64) -> Result<Self, IdentityAllocationError> {
        NonZeroU64::new(raw)
            .map(Self)
            .ok_or(IdentityAllocationError::Zero)
    }

    pub(crate) const fn from_non_zero(raw: NonZeroU64) -> Self {
        Self(raw)
    }

    pub(crate) const fn non_zero(self) -> NonZeroU64 {
        self.0
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for SceneInstanceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(self.get())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for SceneInstanceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = <u64 as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WorldIdentityDomainId(NonZeroU64);

impl WorldIdentityDomainId {
    pub(crate) const fn from_non_zero(raw: NonZeroU64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for WorldIdentityDomainId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u64(self.get())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for WorldIdentityDomainId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = <u64 as serde::Deserialize>::deserialize(deserializer)?;
        NonZeroU64::new(raw)
            .map(Self)
            .ok_or_else(|| serde::de::Error::custom("world identity domain id must be non-zero"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PersistentRuntimeNamespaceId(String);

impl PersistentRuntimeNamespaceId {
    pub fn new(id: impl Into<String>) -> Result<Self, PersistentRuntimeNamespaceIdError> {
        let id = id.into();
        if id.is_empty() {
            return Err(PersistentRuntimeNamespaceIdError::Empty);
        }
        if id.len() > MAX_PERSISTENT_RUNTIME_NAMESPACE_BYTES {
            return Err(PersistentRuntimeNamespaceIdError::TooLong {
                length: id.len(),
                maximum: MAX_PERSISTENT_RUNTIME_NAMESPACE_BYTES,
            });
        }
        if let Some(character) = id.chars().find(|character| {
            !character.is_ascii_alphanumeric() && !matches!(character, '_' | '-' | '.')
        }) {
            return Err(PersistentRuntimeNamespaceIdError::InvalidCharacter(
                character,
            ));
        }
        Ok(Self(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for PersistentRuntimeNamespaceId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for PersistentRuntimeNamespaceId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for PersistentRuntimeNamespaceId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(id).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum PersistentRuntimeNamespaceIdError {
    #[error("persistent runtime namespace cannot be empty")]
    Empty,
    #[error("persistent runtime namespace exceeds its byte limit")]
    TooLong { length: usize, maximum: usize },
    #[error("persistent runtime namespace contains an invalid character")]
    InvalidCharacter(char),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PersistentRuntimeId(Uuid);

impl PersistentRuntimeId {
    pub fn parse_str(id: impl AsRef<str>) -> Result<Self, PersistentRuntimeIdError> {
        Uuid::parse_str(id.as_ref())
            .map(Self)
            .map_err(|_| PersistentRuntimeIdError::InvalidUuid)
    }

    #[must_use]
    pub const fn from_uuid(id: Uuid) -> Self {
        Self(id)
    }

    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Display for PersistentRuntimeId {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for PersistentRuntimeId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for PersistentRuntimeId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let id = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::parse_str(id).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum PersistentRuntimeIdError {
    #[error("persistent runtime id must be a UUID")]
    InvalidUuid,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PersistentRuntimeReference {
    pub namespace: PersistentRuntimeNamespaceId,
    pub entity: PersistentRuntimeId,
}

impl PersistentRuntimeReference {
    #[must_use]
    pub const fn new(namespace: PersistentRuntimeNamespaceId, entity: PersistentRuntimeId) -> Self {
        Self { namespace, entity }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)
)]
pub enum EntityReference {
    SceneLocal { entity: SceneEntityId },
    Persistent { entity: PersistentRuntimeReference },
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)
)]
pub enum RuntimeEntityReference {
    Scene {
        instance: SceneInstanceId,
        entity: SceneEntityId,
    },
    Persistent {
        entity: PersistentRuntimeReference,
    },
}

impl RuntimeEntityReference {
    #[must_use]
    pub const fn scene(instance: SceneInstanceId, entity: SceneEntityId) -> Self {
        Self::Scene { instance, entity }
    }

    #[must_use]
    pub const fn persistent(entity: PersistentRuntimeReference) -> Self {
        Self::Persistent { entity }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct WorldEntityLocator {
    domain: WorldIdentityDomainId,
    entity: RuntimeEntityReference,
}

impl WorldEntityLocator {
    #[must_use]
    pub const fn new(domain: WorldIdentityDomainId, entity: RuntimeEntityReference) -> Self {
        Self { domain, entity }
    }

    #[must_use]
    pub const fn domain_id(&self) -> WorldIdentityDomainId {
        self.domain
    }

    #[must_use]
    pub const fn entity(&self) -> &RuntimeEntityReference {
        &self.entity
    }
}
