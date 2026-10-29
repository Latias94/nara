use std::{
    any::Any,
    error::Error,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use nara_asset::{
    AssetPath, AssetProductIdentityError, AssetProductRef, ImportArtifactRecord,
    ImportSettingsHash, ImportedProductId, SourceHash, StableAssetId,
};

use crate::ImageImportSettings;
use crate::budget::ImageImportPixelLease;

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
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
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

/// Exact backend-neutral descriptor and pixel content, independent of logical asset identity.
///
/// The digest includes extent, format, color space and every pixel byte. Product identity,
/// source paths and import recipes do not participate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageContentHash([u8; 32]);

impl ImageContentHash {
    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }

    fn compute(
        extent: ImageExtent,
        format: ImageFormat,
        color_space: ImageColorSpace,
        pixels: &[u8],
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"nara.image.content.v1\0");
        hasher.update(&extent.width.to_le_bytes());
        hasher.update(&extent.height.to_le_bytes());
        hasher.update(&[match format {
            ImageFormat::Rgba8 => 1,
        }]);
        hasher.update(&[match color_space {
            ImageColorSpace::Srgb => 1,
            ImageColorSpace::Linear => 2,
        }]);
        hasher.update(pixels);
        Self(*hasher.finalize().as_bytes())
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
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    product: Option<AssetProductRef>,
    extent: ImageExtent,
    format: ImageFormat,
    color_space: ImageColorSpace,
    #[cfg_attr(feature = "serde", serde(serialize_with = "shared_bytes::serialize"))]
    pixels: Arc<Box<[u8]>>,
    #[cfg_attr(feature = "serde", serde(skip_serializing_if = "Option::is_none"))]
    source: Option<ImageSourceMetadata>,
    #[cfg_attr(feature = "serde", serde(skip))]
    content_hash: ImageContentHash,
    #[cfg_attr(feature = "serde", serde(skip))]
    retention: Option<ImagePixelRetention>,
}

#[derive(Clone)]
pub(crate) enum ImagePixelRetention {
    LifetimeOwner { _owner: Arc<dyn Any + Send + Sync> },
    ImportBudget(ImageImportPixelLease),
}

impl ImageAsset {
    /// Constructs anonymous immutable content after validating its extent and exact byte length.
    ///
    /// Pixel storage is normalized to a fixed-size shared allocation, so retained-memory
    /// accounting cannot be bypassed through unused `Vec` capacity. Procedural content needs
    /// neither a persistent Product identity nor source-import provenance.
    pub fn new(
        extent: ImageExtent,
        format: ImageFormat,
        color_space: ImageColorSpace,
        pixels: Vec<u8>,
    ) -> Result<Self, ImageAssetCreateError> {
        Self::validate_rgba_bytes(extent, format, pixels.len())?;
        let content_hash = ImageContentHash::compute(extent, format, color_space, &pixels);
        Ok(Self {
            product: None,
            extent,
            format,
            color_space,
            pixels: Arc::new(pixels.into_boxed_slice()),
            source: None,
            content_hash,
            retention: None,
        })
    }

    /// Associates independently delivered content with its complete persistent Product.
    ///
    /// The association supplies no source provenance, catalog membership or publication authority.
    pub fn from_product(
        product: AssetProductRef,
        extent: ImageExtent,
        format: ImageFormat,
        color_space: ImageColorSpace,
        pixels: Vec<u8>,
    ) -> Result<Self, ImageAssetCreateError> {
        let mut image = Self::new(extent, format, color_space, pixels)?;
        image.product = Some(product);
        Ok(image)
    }

    /// Constructs the source's primary image with its actual import provenance.
    pub fn from_import(
        source: ImageSourceMetadata,
        extent: ImageExtent,
        format: ImageFormat,
        pixels: Vec<u8>,
    ) -> Result<Self, ImageAssetCreateError> {
        let product = AssetProductRef::new(source.stable_id(), ImportedProductId::PRIMARY)
            .map_err(ImageAssetCreateError::InvalidProduct)?;
        let mut image = Self::from_product(
            product,
            extent,
            format,
            source.settings().color_space(),
            pixels,
        )?;
        image.source = Some(source);
        Ok(image)
    }

    /// Installs an owner whose lifetime must follow this shared pixel allocation.
    ///
    /// This owner does not prove an [`crate::ImageImportBudgetHost`] charge and therefore never
    /// exempts the allocation from publication-overlap accounting. Returns `false` without
    /// replacing the existing owner when one is already attached.
    #[must_use]
    pub fn try_attach_retention_owner<R>(&mut self, retention: Arc<R>) -> bool
    where
        R: Send + Sync + 'static,
    {
        if self.retention.is_some() {
            return false;
        }
        self.retention = Some(ImagePixelRetention::LifetimeOwner { _owner: retention });
        true
    }

    /// Shares immutable pixels only when their existing accounting owner can be retained.
    #[must_use]
    pub fn share_retained(&self) -> Option<Self> {
        Some(Self {
            product: self.product,
            source: self.source.clone(),
            extent: self.extent,
            format: self.format,
            color_space: self.color_space,
            pixels: Arc::clone(&self.pixels),
            content_hash: self.content_hash,
            retention: Some(self.retention.as_ref()?.clone()),
        })
    }

    pub(crate) fn share_immutable(&self) -> Self {
        Self {
            product: self.product,
            source: self.source.clone(),
            extent: self.extent,
            format: self.format,
            color_space: self.color_space,
            pixels: Arc::clone(&self.pixels),
            content_hash: self.content_hash,
            retention: self.retention.clone(),
        }
    }

    pub(crate) fn same_allocation(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.pixels, &other.pixels)
    }

    /// Shares this exact pixel allocation while retaining both its current lease and `owner`.
    ///
    /// This only extends Rust ownership. It does not establish publication, budget admission or
    /// source provenance, and it does not mutate this image or the supplied owner.
    #[must_use]
    pub fn share_with_retention_owner<R>(&self, owner: Arc<R>) -> Self
    where
        R: Send + Sync + 'static,
    {
        struct CompositeOwner<R> {
            _previous: Option<ImagePixelRetention>,
            _owner: Arc<R>,
        }

        let mut shared = self.share_immutable();
        shared.retention = Some(ImagePixelRetention::LifetimeOwner {
            _owner: Arc::new(CompositeOwner {
                _previous: self.retention.clone(),
                _owner: owner,
            }),
        });
        shared
    }

    #[must_use]
    pub const fn source(&self) -> Option<&ImageSourceMetadata> {
        self.source.as_ref()
    }

    #[must_use]
    pub const fn product(&self) -> Option<AssetProductRef> {
        self.product
    }

    #[must_use]
    pub const fn content_hash(&self) -> ImageContentHash {
        self.content_hash
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
        self.color_space
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        self.pixels.as_ref().as_ref()
    }

    #[cfg(any(feature = "render-prepare", test))]
    pub(crate) fn shared_pixels(&self) -> Arc<Box<[u8]>> {
        Arc::clone(&self.pixels)
    }

    #[cfg(any(feature = "render-prepare", test))]
    pub(crate) fn shared_pixel_retention(&self) -> Option<ImagePixelRetention> {
        self.retention.clone()
    }

    pub(crate) fn shared_import_pixel_lease(&self) -> Option<ImageImportPixelLease> {
        match self.retention.as_ref()? {
            ImagePixelRetention::LifetimeOwner { .. } => None,
            ImagePixelRetention::ImportBudget(lease) => Some(lease.clone()),
        }
    }

    pub(crate) fn try_attach_import_pixel_lease(&mut self, lease: ImageImportPixelLease) -> bool {
        if self.retention.is_some() {
            return false;
        }
        self.retention = Some(ImagePixelRetention::ImportBudget(lease));
        true
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
    /// Actual import provenance did not provide a valid non-nil product identity.
    InvalidProduct(AssetProductIdentityError),
    /// Import provenance names a different source or an auxiliary product.
    SourceProductMismatch {
        expected: AssetProductRef,
        actual: AssetProductRef,
    },
    /// The explicit descriptor disagrees with the actual import settings.
    SourceColorSpaceMismatch {
        expected: ImageColorSpace,
        actual: ImageColorSpace,
    },
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
            Self::InvalidProduct(error) => Display::fmt(error, formatter),
            Self::SourceProductMismatch { expected, actual } => write!(
                formatter,
                "image product {actual} does not match import source product {expected}"
            ),
            Self::SourceColorSpaceMismatch { expected, actual } => write!(
                formatter,
                "image color space {actual:?} does not match import color space {expected:?}"
            ),
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

impl Error for ImageAssetCreateError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidProduct(error) => Some(error),
            _ => None,
        }
    }
}

impl PartialEq for ImageAsset {
    fn eq(&self, other: &Self) -> bool {
        self.product == other.product
            && self.source == other.source
            && self.extent == other.extent
            && self.format == other.format
            && self.color_space == other.color_space
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
        #[serde(deny_unknown_fields)]
        struct ImageAssetWire {
            #[serde(default)]
            product: Option<AssetProductRef>,
            extent: ImageExtent,
            format: ImageFormat,
            color_space: ImageColorSpace,
            pixels: Vec<u8>,
            #[serde(default)]
            source: Option<ImageSourceMetadata>,
        }

        let wire = <ImageAssetWire as serde::Deserialize>::deserialize(deserializer)?;
        match wire.source {
            Some(source) => {
                let expected = AssetProductRef::new(source.stable_id(), ImportedProductId::PRIMARY)
                    .map_err(serde::de::Error::custom)?;
                let actual = wire
                    .product
                    .ok_or_else(|| serde::de::Error::missing_field("product"))?;
                if actual != expected {
                    return Err(serde::de::Error::custom(
                        ImageAssetCreateError::SourceProductMismatch { expected, actual },
                    ));
                }
                if wire.color_space != source.settings().color_space() {
                    return Err(serde::de::Error::custom(
                        ImageAssetCreateError::SourceColorSpaceMismatch {
                            expected: source.settings().color_space(),
                            actual: wire.color_space,
                        },
                    ));
                }
                Self::from_import(source, wire.extent, wire.format, wire.pixels)
                    .map_err(serde::de::Error::custom)
            }
            None => match wire.product {
                Some(product) => Self::from_product(
                    product,
                    wire.extent,
                    wire.format,
                    wire.color_space,
                    wire.pixels,
                ),
                None => Self::new(wire.extent, wire.format, wire.color_space, wire.pixels),
            }
            .map_err(serde::de::Error::custom),
        }
    }
}

impl fmt::Debug for ImageAsset {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageAsset")
            .field("product", &self.product)
            .field("source", &self.source)
            .field("extent", &self.extent)
            .field("format", &self.format)
            .field("color_space", &self.color_space())
            .field("content_hash", &self.content_hash)
            .field("pixel_len", &self.pixels.len())
            .field("retained", &self.retention.is_some())
            .finish()
    }
}

#[cfg(test)]
mod tests;
