use std::fmt::{self, Formatter};

use nara_asset::{AssetPath, ImportArtifactRecord, SourceHash, StableAssetId};

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
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImageSourceMetadata {
    stable_id: StableAssetId,
    path: AssetPath,
    source_hash: SourceHash,
    artifact: ImportArtifactRecord,
}

impl ImageSourceMetadata {
    #[must_use]
    pub fn new(
        stable_id: StableAssetId,
        path: AssetPath,
        source_hash: SourceHash,
        artifact: ImportArtifactRecord,
    ) -> Self {
        Self {
            stable_id,
            path,
            source_hash,
            artifact,
        }
    }

    #[must_use]
    pub const fn stable_id(&self) -> StableAssetId {
        self.stable_id
    }

    #[must_use]
    pub const fn path(&self) -> &AssetPath {
        &self.path
    }

    #[must_use]
    pub const fn source_hash(&self) -> SourceHash {
        self.source_hash
    }

    #[must_use]
    pub const fn artifact(&self) -> &ImportArtifactRecord {
        &self.artifact
    }
}

#[derive(PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ImageAsset {
    source: ImageSourceMetadata,
    extent: ImageExtent,
    format: ImageFormat,
    color_space: ImageColorSpace,
    pixels: Vec<u8>,
}

impl ImageAsset {
    #[must_use]
    pub fn new(
        source: ImageSourceMetadata,
        extent: ImageExtent,
        format: ImageFormat,
        color_space: ImageColorSpace,
        pixels: Vec<u8>,
    ) -> Self {
        Self {
            source,
            extent,
            format,
            color_space,
            pixels,
        }
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
        self.color_space
    }

    #[must_use]
    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }
}

impl fmt::Debug for ImageAsset {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ImageAsset")
            .field("source", &self.source)
            .field("extent", &self.extent)
            .field("format", &self.format)
            .field("color_space", &self.color_space)
            .field("pixel_len", &self.pixels.len())
            .finish()
    }
}
