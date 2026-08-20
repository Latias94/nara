use std::{
    any::Any,
    error::Error,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use nara_asset::{AssetPath, ImportArtifactRecord, ImportSettingsHash, SourceHash, StableAssetId};

use crate::ImageImportSettings;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ImageFormat {
    Rgba8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum ImageColorSpace {
    Srgb,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImageExtent {
    pub width: u32,
    pub height: u32,
}

impl ImageExtent {
    #[must_use]
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    #[must_use]
    pub const fn pixel_count(self) -> u64 {
        self.width as u64 * self.height as u64
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ImageSourceMetadata {
    path: AssetPath,
    settings: ImageImportSettings,
    artifact: ImportArtifactRecord,
}

impl ImageSourceMetadata {
    pub fn new(
        path: AssetPath,
        settings: ImageImportSettings,
        artifact: ImportArtifactRecord,
    ) -> Result<Self, ImageSourceMetadataCreateError> {
        let expected = settings.settings_hash();
        let actual = artifact.key().settings_hash();
        if actual != expected {
            return Err(ImageSourceMetadataCreateError::SettingsHashMismatch { expected, actual });
        }

        Ok(Self {
            path,
            settings,
            artifact,
        })
    }

    #[must_use]
    pub const fn stable_id(&self) -> StableAssetId {
        self.artifact.key().stable_id()
    }

    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    #[must_use]
    pub const fn source_hash(&self) -> SourceHash {
        self.artifact.key().source_hash()
    }

    #[must_use]
    pub const fn settings(&self) -> ImageImportSettings {
        self.settings
    }

    #[must_use]
    pub const fn artifact(&self) -> &ImportArtifactRecord {
        &self.artifact
    }
}

/// Failure to compose image source metadata from one self-consistent typed recipe.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageSourceMetadataCreateError {
    /// The artifact was authenticated from settings other than the supplied typed settings.
    SettingsHashMismatch {
        expected: ImportSettingsHash,
        actual: ImportSettingsHash,
    },
}

impl Display for ImageSourceMetadataCreateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::SettingsHashMismatch { .. } => formatter
                .write_str("image artifact settings hash does not match typed import settings"),
        }
    }
}

impl Error for ImageSourceMetadataCreateError {}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ImageSourceMetadata {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ImageSourceMetadataWire {
            path: AssetPath,
            settings: ImageImportSettings,
            artifact: ImportArtifactRecord,
        }

        let wire = ImageSourceMetadataWire::deserialize(deserializer)?;
        Self::new(wire.path, wire.settings, wire.artifact).map_err(serde::de::Error::custom)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct ImageAsset {
    source: ImageSourceMetadata,
    extent: ImageExtent,
    format: ImageFormat,
    #[cfg_attr(feature = "serde", serde(serialize_with = "shared_bytes::serialize"))]
    pixels: Arc<Box<[u8]>>,
    #[cfg_attr(feature = "serde", serde(skip))]
    retention: Option<Arc<dyn Any + Send + Sync>>,
}

impl ImageAsset {
    /// Constructs an immutable RGBA image after validating its extent and exact byte length.
    ///
    /// Pixel storage is normalized to a fixed-size shared allocation, so retained-memory
    /// accounting cannot be bypassed through unused `Vec` capacity.
    pub fn new(
        source: ImageSourceMetadata,
        extent: ImageExtent,
        format: ImageFormat,
        pixels: Vec<u8>,
    ) -> Result<Self, ImageAssetCreateError> {
        Self::validate_rgba_bytes(extent, format, pixels.len())?;
        Ok(Self {
            source,
            extent,
            format,
            pixels: Arc::new(pixels.into_boxed_slice()),
            retention: None,
        })
    }

    /// Installs the owner that accounts for this shared pixel allocation.
    ///
    /// Returns `false` without replacing the existing owner when one is already attached.
    #[must_use]
    pub fn try_attach_retention_owner<R>(&mut self, retention: Arc<R>) -> bool
    where
        R: Send + Sync + 'static,
    {
        if self.retention.is_some() {
            return false;
        }
        self.retention = Some(retention);
        true
    }

    /// Shares immutable pixels only when their existing accounting owner can be retained.
    #[must_use]
    pub fn share_retained(&self) -> Option<Self> {
        Some(Self {
            source: self.source.clone(),
            extent: self.extent,
            format: self.format,
            pixels: Arc::clone(&self.pixels),
            retention: Some(Arc::clone(self.retention.as_ref()?)),
        })
    }

    #[must_use]
    pub const fn source(&self) -> &ImageSourceMetadata {
        &self.source
    }

    #[must_use]
    pub const fn extent(&self) -> ImageExtent {
        self.extent
    }

    #[must_use]
    pub const fn format(&self) -> ImageFormat {
        self.format
    }

    #[must_use]
    pub const fn color_space(&self) -> ImageColorSpace {
        self.source.settings().color_space()
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        self.pixels.as_ref().as_ref()
    }

    pub(crate) fn shared_pixels(&self) -> Arc<Box<[u8]>> {
        Arc::clone(&self.pixels)
    }

    pub(crate) fn shared_retention_owner(&self) -> Option<Arc<dyn Any + Send + Sync>> {
        self.retention.as_ref().map(Arc::clone)
    }

    fn validate_rgba_bytes(
        extent: ImageExtent,
        format: ImageFormat,
        actual: usize,
    ) -> Result<(), ImageAssetCreateError> {
        if extent.width == 0 || extent.height == 0 {
            return Err(ImageAssetCreateError::ZeroExtent { extent });
        }

        let expected = match format {
            ImageFormat::Rgba8 => u64::from(extent.width)
                .checked_mul(u64::from(extent.height))
                .and_then(|pixels| pixels.checked_mul(4))
                .and_then(|bytes| usize::try_from(bytes).ok())
                .ok_or(ImageAssetCreateError::ByteLengthOverflow { extent })?,
        };
        if actual != expected {
            return Err(ImageAssetCreateError::PixelLengthMismatch {
                extent,
                expected,
                actual,
            });
        }
        Ok(())
    }
}

/// Failure to construct an [`ImageAsset`] with a valid fixed-size pixel allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageAssetCreateError {
    /// At least one image dimension was zero.
    ZeroExtent { extent: ImageExtent },
    /// The required RGBA byte length overflowed `u64` or the current platform's `usize`.
    ByteLengthOverflow { extent: ImageExtent },
    /// The supplied bytes did not exactly cover the RGBA extent.
    PixelLengthMismatch {
        extent: ImageExtent,
        expected: usize,
        actual: usize,
    },
}

impl Display for ImageAssetCreateError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroExtent { extent } => write!(
                formatter,
                "image extent must be non-zero, got {}x{}",
                extent.width, extent.height
            ),
            Self::ByteLengthOverflow { extent } => write!(
                formatter,
                "RGBA byte length for image extent {}x{} is not representable",
                extent.width, extent.height
            ),
            Self::PixelLengthMismatch {
                extent,
                expected,
                actual,
            } => write!(
                formatter,
                "image extent {}x{} requires {expected} RGBA bytes, got {actual}",
                extent.width, extent.height
            ),
        }
    }
}

impl Error for ImageAssetCreateError {}

impl PartialEq for ImageAsset {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
            && self.extent == other.extent
            && self.format == other.format
            && self.pixels == other.pixels
    }
}

impl Eq for ImageAsset {}

#[cfg(feature = "serde")]
mod shared_bytes {
    use std::sync::Arc;

    use serde::Serialize;

    pub fn serialize<S>(value: &Arc<Box<[u8]>>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        value.as_ref().as_ref().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ImageAsset {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(serde::Deserialize)]
        struct ImageAssetWire {
            source: ImageSourceMetadata,
            extent: ImageExtent,
            format: ImageFormat,
            pixels: Vec<u8>,
        }

        let wire = <ImageAssetWire as serde::Deserialize>::deserialize(deserializer)?;
        Self::new(wire.source, wire.extent, wire.format, wire.pixels)
            .map_err(serde::de::Error::custom)
    }
}

impl fmt::Debug for ImageAsset {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageAsset")
            .field("source", &self.source)
            .field("extent", &self.extent)
            .field("format", &self.format)
            .field("color_space", &self.color_space())
            .field("pixel_len", &self.pixels.len())
            .field("retained", &self.retention.is_some())
            .finish()
    }
}
