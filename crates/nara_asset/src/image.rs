//! Immutable CPU image values and the optional PNG loader.

use std::{fmt, sync::Arc};

#[cfg(feature = "image-import")]
mod decode;
#[cfg(all(feature = "image-import", any(unix, windows)))]
mod load;

#[cfg(feature = "image-import")]
pub use decode::{IMAGE_PLUGIN_DECLARATION, IMAGE_PLUGIN_ID, ImageDecodeError, ImagePlugin};
#[cfg(all(feature = "image-import", any(unix, windows)))]
pub use load::{ImageLoadError, ImageSourceDirectory, load_image};

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
        if extent.width == 0 || extent.height == 0 {
            return Err(ImageAssetCreateError::ZeroExtent { extent });
        }
        let expected = match format {
            ImageFormat::Rgba8 => extent.pixel_count().checked_mul(4),
        }
        .and_then(|bytes| usize::try_from(bytes).ok())
        .ok_or(ImageAssetCreateError::ByteLengthOverflow { extent })?;
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
