//! Authored source namespaces, independent of runtime or filesystem custody.

use std::{error::Error, fmt, str::FromStr};

use uuid::Uuid;

/// Authored namespace for named and project-location sources.
///
/// This value does not identify a runtime, an opened document incarnation, or a stable asset.
/// Parsing never generates identity. Creation owners supply and persist their own fresh UUID.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SourceUniverseId(Uuid);

impl SourceUniverseId {
    pub const fn try_from_uuid(value: Uuid) -> Result<Self, SourceUniverseIdError> {
        if value.is_nil() {
            Err(SourceUniverseIdError::Nil)
        } else {
            Ok(Self(value))
        }
    }

    #[must_use]
    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl FromStr for SourceUniverseId {
    type Err = SourceUniverseIdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::try_from_uuid(Uuid::parse_str(value).map_err(|_| SourceUniverseIdError::Invalid)?)
    }
}

impl fmt::Display for SourceUniverseId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceUniverseIdError {
    Invalid,
    Nil,
}

impl fmt::Display for SourceUniverseIdError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid => formatter.write_str("source universe must be a UUID"),
            Self::Nil => formatter.write_str("source universe must not be nil"),
        }
    }
}

impl Error for SourceUniverseIdError {}

#[cfg(feature = "serde")]
impl serde::Serialize for SourceUniverseId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for SourceUniverseId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_universe_is_non_nil_and_uses_uuid_equality() {
        assert_eq!(
            SourceUniverseId::try_from_uuid(Uuid::nil()),
            Err(SourceUniverseIdError::Nil)
        );
        let value: SourceUniverseId = "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap();
        let upper: SourceUniverseId = "72011426-37E2-48BA-BC06-78841468889A".parse().unwrap();
        assert_eq!(value, upper);
        assert_eq!(value.to_string(), "72011426-37e2-48ba-bc06-78841468889a");
        assert!("not-a-universe".parse::<SourceUniverseId>().is_err());
    }

    #[cfg(feature = "serde")]
    #[test]
    fn source_universe_serde_preserves_the_validated_value() {
        let value: SourceUniverseId = "72011426-37e2-48ba-bc06-78841468889a".parse().unwrap();
        let bytes = serde_json::to_vec(&value).unwrap();
        assert_eq!(
            serde_json::from_slice::<SourceUniverseId>(&bytes).unwrap(),
            value
        );
        assert!(
            serde_json::from_str::<SourceUniverseId>("\"00000000-0000-0000-0000-000000000000\"")
                .is_err()
        );
    }
}
