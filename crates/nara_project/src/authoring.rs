use std::fmt;

use serde::de::{Error, MapAccess, Visitor, value::MapAccessDeserializer};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

use crate::ProjectPath;

/// Maximum bytes in an engine-owned, project-relative authored-upgrade recovery path.
pub const MAX_PROJECT_AUTHORING_RECOVERY_PATH_BYTES: usize = 1_024;

/// Project-wide source state, preserved unchanged by profile resolution.
///
/// Ready is authored as `[authoring] state = "ready"`. Pending is an engine-owned table:
///
/// ```toml
/// [authoring.state.pending]
/// recovery_path = ".nara/upgrades/recovery.json"
/// recovery_digest = { length = 123, hash = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef" }
/// ```
///
/// Parsing either state grants no execution or filesystem authority. Source Hosts must capture
/// and revalidate Ready before admitting source execution; Pending supports code-free recovery.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum ProjectAuthoringState {
    Ready,
    Pending(ProjectAuthoringRecovery),
}

/// Exact recovery package reference for one pending authored upgrade.
///
/// The digest may also identify the immutable operation. This reference cannot prove that the
/// package exists, matches its digest, or contains admissible recovery data. Those checks belong
/// to the source Host before publication or recovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectAuthoringRecovery {
    #[serde(serialize_with = "serialize_recovery_path")]
    recovery_path: ProjectPath,
    recovery_digest: ProjectAuthoringRecoveryDigest,
}

impl<'de> Deserialize<'de> for ProjectAuthoringRecovery {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct RecoveryRecord {
            #[serde(deserialize_with = "deserialize_recovery_path")]
            recovery_path: ProjectPath,
            recovery_digest: ProjectAuthoringRecoveryDigest,
        }

        struct RecoveryVisitor;

        impl<'de> Visitor<'de> for RecoveryVisitor {
            type Value = ProjectAuthoringRecovery;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a recovery map with recovery_path and recovery_digest")
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                let record = RecoveryRecord::deserialize(MapAccessDeserializer::new(map))?;
                Ok(ProjectAuthoringRecovery {
                    recovery_path: record.recovery_path,
                    recovery_digest: record.recovery_digest,
                })
            }
        }

        deserializer.deserialize_map(RecoveryVisitor)
    }
}

impl ProjectAuthoringRecovery {
    /// Binds an exact content digest to a bounded recovery location within the project.
    ///
    /// Recovery paths use only ASCII letters, digits, `-`, `_`, `.`, and `/`, in addition to
    /// the ordinary [`ProjectPath`] constraints. This engine-owned syntax is platform independent.
    pub fn new(
        recovery_path: ProjectPath,
        recovery_digest: ProjectAuthoringRecoveryDigest,
    ) -> Result<Self, ProjectAuthoringRecoveryPathError> {
        validate_recovery_path(recovery_path.as_str())?;
        Ok(Self {
            recovery_path,
            recovery_digest,
        })
    }

    #[must_use]
    pub const fn recovery_path(&self) -> &ProjectPath {
        &self.recovery_path
    }

    #[must_use]
    pub const fn recovery_digest(&self) -> ProjectAuthoringRecoveryDigest {
        self.recovery_digest
    }
}

/// BLAKE3 hash and exact byte length of the referenced immutable recovery package.
///
/// This value describes bytes; it neither hashes content nor admits a recovery package. Its parts
/// correspond exactly to the file publication owner's content digest, without a filesystem
/// dependency. The wire hash is always exactly 64 lowercase hexadecimal digits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectAuthoringRecoveryDigest {
    length: u64,
    #[serde(with = "digest_hex")]
    hash: [u8; 32],
}

impl<'de> Deserialize<'de> for ProjectAuthoringRecoveryDigest {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct DigestRecord {
            length: u64,
            #[serde(with = "digest_hex")]
            hash: [u8; 32],
        }

        struct DigestRecordVisitor;

        impl<'de> Visitor<'de> for DigestRecordVisitor {
            type Value = ProjectAuthoringRecoveryDigest;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a recovery digest map with length and hash")
            }

            fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                let record = DigestRecord::deserialize(MapAccessDeserializer::new(map))?;
                Ok(ProjectAuthoringRecoveryDigest::from_parts(
                    record.length,
                    record.hash,
                ))
            }
        }

        deserializer.deserialize_map(DigestRecordVisitor)
    }
}

impl ProjectAuthoringRecoveryDigest {
    #[must_use]
    pub const fn from_parts(length: u64, hash: [u8; 32]) -> Self {
        Self { length, hash }
    }

    pub fn from_hex(length: u64, hash: &str) -> Result<Self, ProjectAuthoringRecoveryDigestError> {
        Ok(Self::from_parts(length, decode_digest_hex(hash)?))
    }

    #[must_use]
    pub const fn length(self) -> u64 {
        self.length
    }

    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.hash
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ProjectAuthoringRecoveryPathError {
    #[error("recovery path exceeds the {limit}-byte limit (actual: {actual})")]
    TooLong { actual: usize, limit: usize },
    #[error("recovery path must use only ASCII letters, digits, '-', '_', '.', and '/'")]
    UnsupportedCharacter,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("recovery digest hash must contain exactly 64 lowercase hexadecimal digits")]
pub struct ProjectAuthoringRecoveryDigestError;

fn validate_recovery_path(path: &str) -> Result<(), ProjectAuthoringRecoveryPathError> {
    if path.len() > MAX_PROJECT_AUTHORING_RECOVERY_PATH_BYTES {
        return Err(ProjectAuthoringRecoveryPathError::TooLong {
            actual: path.len(),
            limit: MAX_PROJECT_AUTHORING_RECOVERY_PATH_BYTES,
        });
    }
    if !path
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'/'))
    {
        return Err(ProjectAuthoringRecoveryPathError::UnsupportedCharacter);
    }
    Ok(())
}

fn serialize_recovery_path<S: Serializer>(
    path: &ProjectPath,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(path.as_str())
}

fn deserialize_recovery_path<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<ProjectPath, D::Error> {
    struct RecoveryPathVisitor;

    impl Visitor<'_> for RecoveryPathVisitor {
        type Value = ProjectPath;

        fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str("a bounded project-relative recovery path")
        }

        fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
            validate_recovery_path(value).map_err(E::custom)?;
            ProjectPath::new(value).map_err(E::custom)
        }
    }

    deserializer.deserialize_str(RecoveryPathVisitor)
}

fn decode_digest_hex(hash: &str) -> Result<[u8; 32], ProjectAuthoringRecoveryDigestError> {
    if hash.len() != 64 {
        return Err(ProjectAuthoringRecoveryDigestError);
    }
    let mut decoded = [0; 32];
    for (destination, pair) in decoded.iter_mut().zip(hash.as_bytes().chunks_exact(2)) {
        let [high, low] = pair else {
            return Err(ProjectAuthoringRecoveryDigestError);
        };
        let digit = |byte| match byte {
            b'0'..=b'9' => Ok(byte - b'0'),
            b'a'..=b'f' => Ok(byte - b'a' + 10),
            _ => Err(ProjectAuthoringRecoveryDigestError),
        };
        *destination = (digit(*high)? << 4) | digit(*low)?;
    }
    Ok(decoded)
}

mod digest_hex {
    use super::*;

    pub(super) fn serialize<S: Serializer>(
        hash: &[u8; 32],
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut encoded = String::with_capacity(64);
        for byte in hash {
            encoded.push(char::from(HEX[usize::from(byte >> 4)]));
            encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
        }
        serializer.serialize_str(&encoded)
    }

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<[u8; 32], D::Error> {
        struct DigestVisitor;

        impl Visitor<'_> for DigestVisitor {
            type Value = [u8; 32];

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("exactly 64 lowercase hexadecimal digits")
            }

            fn visit_str<E: Error>(self, value: &str) -> Result<Self::Value, E> {
                decode_digest_hex(value).map_err(E::custom)
            }
        }

        deserializer.deserialize_str(DigestVisitor)
    }
}

#[cfg(test)]
mod tests;
