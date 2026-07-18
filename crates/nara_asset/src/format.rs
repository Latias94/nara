//! Canonical source-asset metadata file boundary.

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_core::{
    ByteLimit, DepthLimit, EngineVersion, FormatGenerator, FormatKind, ItemLimit,
    PersistentFileContract, PersistentFileContractError, PersistentFileDecodeError,
    PersistentFileEnvelope, PersistentFileHeader, SerdeShapeLimits,
    decode_persistent_file_with_preflight, preflight_serde_shape,
};

use crate::AssetMeta;

const ASSET_META_KIND: &str = "asset_meta";
const CANONICAL_V1_ENGINE_MIN_VERSION: &str = "0.1.0";
const FORMAT_GENERATOR_NAME: &str = "nara";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetMetaFileLimits {
    encoded_bytes: ByteLimit,
    shape: SerdeShapeLimits,
}

impl Default for AssetMetaFileLimits {
    fn default() -> Self {
        Self {
            encoded_bytes: ByteLimit::new(64 * 1024)
                .expect("asset metadata byte limit is non-zero"),
            shape: SerdeShapeLimits::new(
                DepthLimit::new(16).expect("asset metadata depth limit is non-zero"),
                ItemLimit::new(256).expect("asset metadata node limit is non-zero"),
                ItemLimit::new(64).expect("asset metadata item limit is non-zero"),
                ByteLimit::new(8 * 1024).expect("asset metadata string limit is non-zero"),
                ByteLimit::new(32 * 1024).expect("asset metadata total string limit is non-zero"),
            ),
        }
    }
}

impl AssetMetaFileLimits {
    #[must_use]
    pub const fn encoded_bytes(self) -> ByteLimit {
        self.encoded_bytes
    }

    #[must_use]
    pub const fn shape(self) -> SerdeShapeLimits {
        self.shape
    }

    #[must_use]
    pub const fn with_encoded_bytes(mut self, limit: ByteLimit) -> Self {
        self.encoded_bytes = limit;
        self
    }

    #[must_use]
    pub const fn with_shape(mut self, limits: SerdeShapeLimits) -> Self {
        self.shape = limits;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssetMetaCandidate {
    meta: AssetMeta,
}

impl AssetMetaCandidate {
    pub fn decode_json_bytes(input: &[u8]) -> Result<Self, AssetMetaFormatError> {
        Self::decode_json_bytes_with_limits(input, AssetMetaFileLimits::default())
    }

    pub fn decode_json_bytes_with_limits(
        input: &[u8],
        limits: AssetMetaFileLimits,
    ) -> Result<Self, AssetMetaFormatError> {
        let contract = PersistentFileContract::canonical_v1(
            format_kind()?,
            engine_version(env!("CARGO_PKG_VERSION"))?,
        );
        let envelope = decode_persistent_file_with_preflight(
            input,
            limits.encoded_bytes(),
            &contract,
            |bytes| {
                let mut deserializer = serde_json::Deserializer::from_slice(bytes);
                preflight_serde_shape(&mut deserializer, limits.shape())
                    .map_err(AssetMetaJsonError::Shape)
            },
            |bytes| {
                serde_json::from_slice::<PersistentFileHeader>(bytes)
                    .map_err(AssetMetaJsonError::Syntax)
            },
            |bytes| {
                serde_json::from_slice::<PersistentFileEnvelope<AssetMeta>>(bytes)
                    .map_err(AssetMetaJsonError::Syntax)
            },
        )
        .map_err(map_decode_error)?;
        Ok(Self {
            meta: envelope.into_payload(),
        })
    }

    #[must_use]
    pub fn meta(&self) -> &AssetMeta {
        &self.meta
    }

    #[must_use]
    pub fn into_meta(self) -> AssetMeta {
        self.meta
    }
}

impl AssetMeta {
    pub fn to_json_string(&self) -> Result<String, AssetMetaFormatError> {
        self.to_json_string_with_limits(AssetMetaFileLimits::default())
    }

    pub fn to_json_string_with_limits(
        &self,
        limits: AssetMetaFileLimits,
    ) -> Result<String, AssetMetaFormatError> {
        let envelope = PersistentFileEnvelope::canonical_v1(
            format_kind()?,
            engine_version(CANONICAL_V1_ENGINE_MIN_VERSION)?,
            FormatGenerator::new(
                FORMAT_GENERATOR_NAME,
                engine_version(env!("CARGO_PKG_VERSION"))?,
            )
            .map_err(metadata_error)?,
            self.clone(),
        );
        let encoded =
            serde_json::to_string_pretty(&envelope).map_err(AssetMetaFormatError::Encode)?;
        let maximum = limits.encoded_bytes().get();
        if encoded.len() > maximum {
            return Err(AssetMetaFormatError::EncodedBytesExceeded {
                observed: encoded.len(),
                maximum,
            });
        }
        let mut deserializer = serde_json::Deserializer::from_str(&encoded);
        preflight_serde_shape(&mut deserializer, limits.shape())
            .map_err(|source| AssetMetaFormatError::Shape(Box::new(source)))?;
        Ok(encoded)
    }
}

#[derive(Debug)]
pub enum AssetMetaFormatError {
    EncodedBytesExceeded { observed: usize, maximum: usize },
    Shape(Box<dyn Error + Send + Sync>),
    Header(Box<dyn Error + Send + Sync>),
    Contract(PersistentFileContractError),
    Payload(Box<dyn Error + Send + Sync>),
    Encode(serde_json::Error),
    Metadata(Box<dyn Error + Send + Sync>),
}

impl Display for AssetMetaFormatError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::EncodedBytesExceeded { observed, maximum } => write!(
                formatter,
                "asset metadata contains {observed} encoded bytes, maximum is {maximum}"
            ),
            Self::Shape(source) => write!(formatter, "asset metadata shape is invalid: {source}"),
            Self::Header(source) => write!(formatter, "asset metadata header is invalid: {source}"),
            Self::Contract(source) => Display::fmt(source, formatter),
            Self::Payload(source) => {
                write!(formatter, "asset metadata payload is invalid: {source}")
            }
            Self::Encode(source) => write!(formatter, "asset metadata encoding failed: {source}"),
            Self::Metadata(source) => write!(
                formatter,
                "asset metadata format identity is invalid: {source}"
            ),
        }
    }
}

impl Error for AssetMetaFormatError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Shape(source)
            | Self::Header(source)
            | Self::Payload(source)
            | Self::Metadata(source) => Some(source.as_ref()),
            Self::Contract(source) => Some(source),
            Self::Encode(source) => Some(source),
            Self::EncodedBytesExceeded { .. } => None,
        }
    }
}

#[derive(Debug)]
enum AssetMetaJsonError {
    Shape(nara_core::SerdeShapePreflightError<serde_json::Error>),
    Syntax(serde_json::Error),
}

impl Display for AssetMetaJsonError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Shape(source) => Display::fmt(source, formatter),
            Self::Syntax(source) => Display::fmt(source, formatter),
        }
    }
}

impl Error for AssetMetaJsonError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Shape(source) => Some(source),
            Self::Syntax(source) => Some(source),
        }
    }
}

fn map_decode_error(error: PersistentFileDecodeError<AssetMetaJsonError>) -> AssetMetaFormatError {
    match error {
        PersistentFileDecodeError::EncodedBytesExceeded { observed, maximum } => {
            AssetMetaFormatError::EncodedBytesExceeded { observed, maximum }
        }
        PersistentFileDecodeError::Shape(source) => AssetMetaFormatError::Shape(Box::new(source)),
        PersistentFileDecodeError::Header(source) => AssetMetaFormatError::Header(Box::new(source)),
        PersistentFileDecodeError::Contract(source) => AssetMetaFormatError::Contract(source),
        PersistentFileDecodeError::Payload(source) => {
            AssetMetaFormatError::Payload(Box::new(source))
        }
    }
}

fn format_kind() -> Result<FormatKind, AssetMetaFormatError> {
    FormatKind::new(ASSET_META_KIND).map_err(metadata_error)
}

fn engine_version(version: &str) -> Result<EngineVersion, AssetMetaFormatError> {
    EngineVersion::parse(version).map_err(metadata_error)
}

fn metadata_error(error: impl Error + Send + Sync + 'static) -> AssetMetaFormatError {
    AssetMetaFormatError::Metadata(Box::new(error))
}
