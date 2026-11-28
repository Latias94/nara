//! Immutable CPU image values, typed storage, and format-specific codecs.

use std::{fmt, sync::Arc};

#[cfg(feature = "cooked")]
mod cooked;
#[cfg(any(feature = "image-import", feature = "cook"))]
mod decode;
#[cfg(all(feature = "image-import", any(unix, windows)))]
mod load;
#[cfg(feature = "runtime")]
mod plugin;
#[cfg(feature = "runtime")]
mod runtime_loader;
#[cfg(all(feature = "authoring", any(feature = "image-import", feature = "cook")))]
mod source_import;

#[cfg(feature = "cooked")]
pub use cooked::CookedImageError;
#[cfg(any(feature = "image-import", feature = "cook"))]
pub use decode::ImageDecodeError;
#[cfg(all(feature = "image-import", any(unix, windows)))]
pub use load::{ImageLoadError, ImageSourceDirectory, load_image};
#[cfg(feature = "runtime")]
pub use plugin::{IMAGE_PLUGIN_DECLARATION, IMAGE_PLUGIN_ID, ImagePlugin};
#[cfg(feature = "runtime")]
pub use runtime_loader::{ImageAssetLoader, ImageAssetLoaderError};
#[cfg(all(feature = "authoring", any(feature = "image-import", feature = "cook")))]
pub use source_import::{
    ImageImportError, ImageImportFilter, ImageImportSettings, decode_png_source,
};

#[cfg(any(feature = "cooked", feature = "image-import"))]
const MAX_IMPORT_DIMENSION: u32 = 16_384;
#[cfg(any(feature = "cooked", feature = "image-import"))]
const MAX_IMPORT_PIXELS: u64 = 16_777_216;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum FilterMode {
    Nearest,
    Linear,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum AddressMode {
    ClampToEdge,
    Repeat,
    MirrorRepeat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SamplerDescriptor {
    pub min_filter: FilterMode,
    pub mag_filter: FilterMode,
    pub mipmap_filter: FilterMode,
    pub address_mode_u: AddressMode,
    pub address_mode_v: AddressMode,
}

impl SamplerDescriptor {
    pub const LINEAR_CLAMP: Self = Self {
        min_filter: FilterMode::Linear,
        mag_filter: FilterMode::Linear,
        mipmap_filter: FilterMode::Linear,
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
    };

    pub const NEAREST_CLAMP: Self = Self {
        min_filter: FilterMode::Nearest,
        mag_filter: FilterMode::Nearest,
        mipmap_filter: FilterMode::Nearest,
        address_mode_u: AddressMode::ClampToEdge,
        address_mode_v: AddressMode::ClampToEdge,
    };
}

impl Default for SamplerDescriptor {
    fn default() -> Self {
        Self::LINEAR_CLAMP
    }
}

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

/// A hash of the image descriptor and its pixels, for renderer caching.
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
        color: ImageColorSpace,
        pixels: &[u8],
    ) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"nara.image.content.v1\0");
        hasher.update(&extent.width.to_le_bytes());
        hasher.update(&extent.height.to_le_bytes());
        hasher.update(&[match format {
            ImageFormat::Rgba8 => 1,
        }]);
        hasher.update(&[match color {
            ImageColorSpace::Srgb => 1,
            ImageColorSpace::Linear => 2,
        }]);
        hasher.update(pixels);
        Self(*hasher.finalize().as_bytes())
    }
}

/// Shared immutable pixel bytes for a renderer snapshot.
#[derive(Clone, PartialEq, Eq)]
pub struct ImagePixels(Arc<Vec<u8>>);

impl ImagePixels {
    #[must_use]
    pub fn as_slice(&self) -> &[u8] {
        self.0.as_slice()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct ImageAsset {
    extent: ImageExtent,
    format: ImageFormat,
    color_space: ImageColorSpace,
    content_hash: ImageContentHash,
    pixels: ImagePixels,
}

impl ImageAsset {
    /// Takes ownership of tightly packed CPU pixels for a procedural image.
    pub fn new(
        extent: ImageExtent,
        format: ImageFormat,
        color_space: ImageColorSpace,
        pixels: Vec<u8>,
    ) -> Result<Self, ImageAssetCreateError> {
        let expected = pixel_byte_len(extent, format)?;
        if pixels.len() != expected {
            return Err(ImageAssetCreateError::PixelLengthMismatch {
                extent,
                expected,
                actual: pixels.len(),
            });
        }
        Ok(Self {
            extent,
            format,
            color_space,
            content_hash: ImageContentHash::compute(extent, format, color_space, &pixels),
            pixels: ImagePixels(Arc::new(pixels)),
        })
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
    pub const fn content_hash(&self) -> ImageContentHash {
        self.content_hash
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        self.pixels.as_slice()
    }

    #[must_use]
    pub fn shared_pixels(&self) -> ImagePixels {
        self.pixels.clone()
    }
}

fn pixel_byte_len(
    extent: ImageExtent,
    format: ImageFormat,
) -> Result<usize, ImageAssetCreateError> {
    if extent.width == 0 || extent.height == 0 {
        return Err(ImageAssetCreateError::ZeroExtent { extent });
    }
    match format {
        ImageFormat::Rgba8 => extent.pixel_count().checked_mul(4),
    }
    .and_then(|bytes| usize::try_from(bytes).ok())
    .ok_or(ImageAssetCreateError::ByteLengthOverflow { extent })
}

impl fmt::Debug for ImageAsset {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageAsset")
            .field("extent", &self.extent)
            .field("format", &self.format)
            .field("color_space", &self.color_space)
            .field("content_hash", &self.content_hash)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ImageAssetCreateError {
    #[error("image extent must be non-zero, got {extent:?}")]
    ZeroExtent { extent: ImageExtent },
    #[error("image extent {extent:?} overflows the RGBA byte length")]
    ByteLengthOverflow { extent: ImageExtent },
    #[error("image extent {extent:?} requires {expected} RGBA bytes, got {actual}")]
    PixelLengthMismatch {
        extent: ImageExtent,
        expected: usize,
        actual: usize,
    },
}
