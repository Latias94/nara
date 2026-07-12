//! Domain-neutral values shared by persistent file format owners.

use std::{
    cmp::Ordering,
    error::Error,
    fmt::{self, Display, Formatter},
    num::NonZeroU32,
    str::FromStr,
};

use crate::ByteLimit;

/// A bounded persistent file kind such as `scene` or `scene_patch`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FormatKind(String);

impl FormatKind {
    pub const MAX_BYTES: usize = 64;

    pub fn new(value: impl Into<String>) -> Result<Self, FormatKindError> {
        let value = value.into();
        validate_format_kind(&value)?;
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for FormatKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatKindError {
    Empty,
    TooLong { length: usize, maximum: usize },
    InvalidStart(char),
    InvalidCharacter { index: usize, character: char },
}

impl Display for FormatKindError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("format kind is empty"),
            Self::TooLong { length, maximum } => {
                write!(
                    formatter,
                    "format kind has {length} bytes, maximum is {maximum}"
                )
            }
            Self::InvalidStart(character) => {
                write!(
                    formatter,
                    "format kind must start with a lowercase ASCII letter, found '{character}'"
                )
            }
            Self::InvalidCharacter { index, character } => {
                write!(
                    formatter,
                    "format kind contains invalid character '{character}' at byte {index}"
                )
            }
        }
    }
}

impl Error for FormatKindError {}

fn validate_format_kind(value: &str) -> Result<(), FormatKindError> {
    let Some(first) = value.chars().next() else {
        return Err(FormatKindError::Empty);
    };
    if value.len() > FormatKind::MAX_BYTES {
        return Err(FormatKindError::TooLong {
            length: value.len(),
            maximum: FormatKind::MAX_BYTES,
        });
    }

    if !first.is_ascii_lowercase() {
        return Err(FormatKindError::InvalidStart(first));
    }
    if let Some((index, character)) = value.char_indices().find(|(_, character)| {
        !character.is_ascii_lowercase() && !character.is_ascii_digit() && *character != '_'
    }) {
        return Err(FormatKindError::InvalidCharacter { index, character });
    }
    Ok(())
}

#[cfg(feature = "serde")]
impl serde::Serialize for FormatKind {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for FormatKind {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <String as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

/// A non-zero version of one persistent file kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(transparent)]
pub struct FormatVersion(NonZeroU32);

impl FormatVersion {
    pub const ONE: Self = Self(NonZeroU32::MIN);

    #[must_use]
    pub const fn new(value: u32) -> Option<Self> {
        match NonZeroU32::new(value) {
            Some(value) => Some(Self(value)),
            None => None,
        }
    }

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

impl Display for FormatVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.get(), formatter)
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for FormatVersion {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_u32(self.get())
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for FormatVersion {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = <u32 as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(value).ok_or_else(|| serde::de::Error::custom("format version must be non-zero"))
    }
}

/// A validated semantic engine version.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(transparent))]
pub struct EngineVersion(semver::Version);

impl EngineVersion {
    pub fn parse(value: &str) -> Result<Self, EngineVersionParseError> {
        value.parse()
    }

    #[must_use]
    pub const fn as_semver(&self) -> &semver::Version {
        &self.0
    }

    /// Returns whether this engine can read data requiring `minimum`.
    ///
    /// Semantic-version build metadata is intentionally ignored for
    /// compatibility, as required by SemVer precedence rules.
    #[must_use]
    pub fn meets_minimum(&self, minimum: &Self) -> bool {
        self.0.cmp_precedence(&minimum.0) != Ordering::Less
    }
}

impl Display for EngineVersion {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.0, formatter)
    }
}

impl FromStr for EngineVersion {
    type Err = EngineVersionParseError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        semver::Version::parse(value)
            .map(Self)
            .map_err(EngineVersionParseError)
    }
}

#[derive(Debug)]
pub struct EngineVersionParseError(semver::Error);

impl Display for EngineVersionParseError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("engine version is not valid semantic version data")
    }
}

impl Error for EngineVersionParseError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.0)
    }
}

/// Identifies the tool and tool version that generated a persistent file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatGenerator {
    name: String,
    version: EngineVersion,
}

impl FormatGenerator {
    pub const MAX_NAME_BYTES: usize = 64;

    pub fn new(
        name: impl Into<String>,
        version: EngineVersion,
    ) -> Result<Self, FormatGeneratorError> {
        let name = name.into();
        validate_generator_name(&name)?;
        Ok(Self { name, version })
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn version(&self) -> &EngineVersion {
        &self.version
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FormatGeneratorError {
    Empty,
    TooLong { length: usize, maximum: usize },
    InvalidCharacter { index: usize, character: char },
}

impl Display for FormatGeneratorError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => formatter.write_str("format generator name is empty"),
            Self::TooLong { length, maximum } => {
                write!(
                    formatter,
                    "format generator name has {length} bytes, maximum is {maximum}"
                )
            }
            Self::InvalidCharacter { index, character } => {
                write!(
                    formatter,
                    "format generator name contains invalid character '{character}' at byte {index}"
                )
            }
        }
    }
}

impl Error for FormatGeneratorError {}

fn validate_generator_name(value: &str) -> Result<(), FormatGeneratorError> {
    if value.is_empty() {
        return Err(FormatGeneratorError::Empty);
    }
    if value.len() > FormatGenerator::MAX_NAME_BYTES {
        return Err(FormatGeneratorError::TooLong {
            length: value.len(),
            maximum: FormatGenerator::MAX_NAME_BYTES,
        });
    }
    if let Some((index, character)) = value.char_indices().find(|(_, character)| {
        !character.is_ascii_alphanumeric() && !matches!(character, '_' | '-' | '.')
    }) {
        return Err(FormatGeneratorError::InvalidCharacter { index, character });
    }
    Ok(())
}

#[cfg(feature = "serde")]
impl serde::Serialize for FormatGenerator {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("FormatGenerator", 2)?;
        state.serialize_field("name", self.name())?;
        state.serialize_field("version", self.version())?;
        state.end()
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for FormatGenerator {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            name: String,
            version: EngineVersion,
        }

        let wire = Wire::deserialize(deserializer)?;
        Self::new(wire.name, wire.version).map_err(serde::de::Error::custom)
    }
}

/// The validated header of one persistent file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentFileHeader {
    kind: FormatKind,
    format_version: FormatVersion,
    engine_min_version: EngineVersion,
    generator: FormatGenerator,
}

impl PersistentFileHeader {
    #[must_use]
    pub const fn kind(&self) -> &FormatKind {
        &self.kind
    }

    #[must_use]
    pub const fn format_version(&self) -> FormatVersion {
        self.format_version
    }

    #[must_use]
    pub const fn engine_min_version(&self) -> &EngineVersion {
        &self.engine_min_version
    }

    #[must_use]
    pub const fn generator(&self) -> &FormatGenerator {
        &self.generator
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for PersistentFileHeader {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct Wire {
            kind: FormatKind,
            format_version: FormatVersion,
            engine_min_version: EngineVersion,
            generator: FormatGenerator,
            payload: serde::de::IgnoredAny,
        }

        let wire = Wire::deserialize(deserializer)?;
        let _ = wire.payload;
        Ok(Self {
            kind: wire.kind,
            format_version: wire.format_version,
            engine_min_version: wire.engine_min_version,
            generator: wire.generator,
        })
    }
}

/// Compatibility requirements for one persistent file kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PersistentFileContract {
    expected_kind: FormatKind,
    supported_version: FormatVersion,
    current_engine_version: EngineVersion,
}

impl PersistentFileContract {
    #[must_use]
    pub fn canonical_v1(expected_kind: FormatKind, current_engine_version: EngineVersion) -> Self {
        Self {
            expected_kind,
            supported_version: FormatVersion::ONE,
            current_engine_version,
        }
    }

    #[must_use]
    pub const fn expected_kind(&self) -> &FormatKind {
        &self.expected_kind
    }

    #[must_use]
    pub const fn supported_version(&self) -> FormatVersion {
        self.supported_version
    }

    #[must_use]
    pub const fn current_engine_version(&self) -> &EngineVersion {
        &self.current_engine_version
    }

    pub fn validate_header(
        &self,
        header: &PersistentFileHeader,
    ) -> Result<(), PersistentFileContractError> {
        if header.kind != self.expected_kind {
            return Err(PersistentFileContractError::WrongKind {
                expected: self.expected_kind.clone(),
                actual: header.kind.clone(),
            });
        }
        if header.format_version != self.supported_version {
            return Err(PersistentFileContractError::UnsupportedFormatVersion {
                kind: self.expected_kind.clone(),
                supported: self.supported_version,
                actual: header.format_version,
            });
        }
        if !self
            .current_engine_version
            .meets_minimum(&header.engine_min_version)
        {
            return Err(PersistentFileContractError::EngineVersionTooOld {
                current: self.current_engine_version.clone(),
                required: header.engine_min_version.clone(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PersistentFileContractError {
    WrongKind {
        expected: FormatKind,
        actual: FormatKind,
    },
    UnsupportedFormatVersion {
        kind: FormatKind,
        supported: FormatVersion,
        actual: FormatVersion,
    },
    EngineVersionTooOld {
        current: EngineVersion,
        required: EngineVersion,
    },
}

impl Display for PersistentFileContractError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongKind { expected, actual } => {
                write!(
                    formatter,
                    "persistent file kind is '{actual}', expected '{expected}'"
                )
            }
            Self::UnsupportedFormatVersion {
                kind,
                supported,
                actual,
            } => write!(
                formatter,
                "persistent file kind '{kind}' uses format version {actual}, supported version is {supported}"
            ),
            Self::EngineVersionTooOld { current, required } => write!(
                formatter,
                "engine version {current} does not meet persistent file minimum {required}"
            ),
        }
    }
}

impl Error for PersistentFileContractError {}

/// Failure from the ordered persistent-file decode boundary.
#[derive(Debug)]
pub enum PersistentFileDecodeError<E> {
    EncodedBytesExceeded { observed: usize, maximum: usize },
    Shape(E),
    Header(E),
    Contract(PersistentFileContractError),
    Payload(E),
}

impl<E: Display> Display for PersistentFileDecodeError<E> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EncodedBytesExceeded { observed, maximum } => write!(
                formatter,
                "persistent file contains {observed} encoded bytes, maximum is {maximum}"
            ),
            Self::Shape(error) => write!(formatter, "persistent file shape is invalid: {error}"),
            Self::Header(error) => write!(formatter, "persistent file header is invalid: {error}"),
            Self::Contract(error) => Display::fmt(error, formatter),
            Self::Payload(error) => {
                write!(formatter, "persistent file payload is invalid: {error}")
            }
        }
    }
}

impl<E> Error for PersistentFileDecodeError<E>
where
    E: Error + 'static,
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Shape(error) | Self::Header(error) | Self::Payload(error) => Some(error),
            Self::Contract(error) => Some(error),
            Self::EncodedBytesExceeded { .. } => None,
        }
    }
}

/// Enforces byte, header, and compatibility gates before payload construction.
pub fn decode_persistent_file<T, E>(
    encoded: &[u8],
    maximum_bytes: ByteLimit,
    contract: &PersistentFileContract,
    decode_header: impl FnOnce(&[u8]) -> Result<PersistentFileHeader, E>,
    decode_payload: impl FnOnce(&[u8]) -> Result<PersistentFileEnvelope<T>, E>,
) -> Result<PersistentFileEnvelope<T>, PersistentFileDecodeError<E>> {
    decode_persistent_file_with_preflight(
        encoded,
        maximum_bytes,
        contract,
        |_| Ok(()),
        decode_header,
        decode_payload,
    )
}

/// Enforces an owner-provided structural preflight before decoding persistent-file headers.
///
/// The caller owns syntax-specific structure limits. This shared boundary guarantees that the
/// byte cap and preflight run before the header and semantic payload are decoded.
pub fn decode_persistent_file_with_preflight<T, E>(
    encoded: &[u8],
    maximum_bytes: ByteLimit,
    contract: &PersistentFileContract,
    preflight: impl FnOnce(&[u8]) -> Result<(), E>,
    decode_header: impl FnOnce(&[u8]) -> Result<PersistentFileHeader, E>,
    decode_payload: impl FnOnce(&[u8]) -> Result<PersistentFileEnvelope<T>, E>,
) -> Result<PersistentFileEnvelope<T>, PersistentFileDecodeError<E>> {
    if encoded.len() > maximum_bytes.get() {
        return Err(PersistentFileDecodeError::EncodedBytesExceeded {
            observed: encoded.len(),
            maximum: maximum_bytes.get(),
        });
    }

    preflight(encoded).map_err(PersistentFileDecodeError::Shape)?;
    let header = decode_header(encoded).map_err(PersistentFileDecodeError::Header)?;
    contract
        .validate_header(&header)
        .map_err(PersistentFileDecodeError::Contract)?;
    decode_payload(encoded).map_err(PersistentFileDecodeError::Payload)
}

/// A strict top-level envelope around one format-owned semantic payload.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PersistentFileEnvelope<T> {
    kind: FormatKind,
    format_version: FormatVersion,
    engine_min_version: EngineVersion,
    generator: FormatGenerator,
    payload: T,
}

impl<T> PersistentFileEnvelope<T> {
    #[must_use]
    pub fn canonical_v1(
        kind: FormatKind,
        engine_min_version: EngineVersion,
        generator: FormatGenerator,
        payload: T,
    ) -> Self {
        Self {
            kind,
            format_version: FormatVersion::ONE,
            engine_min_version,
            generator,
            payload,
        }
    }

    #[must_use]
    pub const fn kind(&self) -> &FormatKind {
        &self.kind
    }

    #[must_use]
    pub const fn format_version(&self) -> FormatVersion {
        self.format_version
    }

    #[must_use]
    pub const fn engine_min_version(&self) -> &EngineVersion {
        &self.engine_min_version
    }

    #[must_use]
    pub const fn generator(&self) -> &FormatGenerator {
        &self.generator
    }

    #[must_use]
    pub const fn payload(&self) -> &T {
        &self.payload
    }

    #[must_use]
    pub fn into_payload(self) -> T {
        self.payload
    }

    /// Transforms only the semantic payload while retaining a verified envelope header.
    #[must_use]
    pub fn map_payload<U>(self, transform: impl FnOnce(T) -> U) -> PersistentFileEnvelope<U> {
        PersistentFileEnvelope {
            kind: self.kind,
            format_version: self.format_version,
            engine_min_version: self.engine_min_version,
            generator: self.generator,
            payload: transform(self.payload),
        }
    }
}
