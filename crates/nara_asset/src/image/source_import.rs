use serde::Deserialize;

use crate::{AssetMeta, meta::IMAGE_IMPORTER};

use super::{ImageAsset, ImageColorSpace, ImageDecodeError};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Deserialize)]
pub enum ImageImportFilter {
    #[default]
    Nearest,
    Linear,
}

/// Source settings for the RGBA8 image profile. `()` selects the defaults.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ImageImportSettings {
    pub srgb: bool,
    /// Sampling preference for authoring. Materials choose their sampler at draw time.
    pub filter: ImageImportFilter,
    /// The RGBA8 profile accepts only a single mip level.
    pub mipmaps: bool,
}

impl Default for ImageImportSettings {
    fn default() -> Self {
        Self {
            srgb: true,
            filter: ImageImportFilter::Nearest,
            mipmaps: false,
        }
    }
}

impl ImageImportSettings {
    pub fn from_meta(meta: &AssetMeta) -> Result<Self, ImageImportError> {
        if meta.importer() != IMAGE_IMPORTER {
            return Err(ImageImportError::WrongImporter(meta.importer().to_owned()));
        }
        if meta.settings::<()>().is_ok() {
            return Ok(Self::default());
        }
        Ok(meta.settings::<Self>()?)
    }

    #[must_use]
    pub const fn color_space(self) -> ImageColorSpace {
        if self.srgb {
            ImageColorSpace::Srgb
        } else {
            ImageColorSpace::Linear
        }
    }
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ImageImportError {
    #[error("PNG import requires importer '{IMAGE_IMPORTER}', got '{0}'")]
    WrongImporter(String),
    #[error("invalid image import settings: {0}")]
    Settings(#[from] ron::error::SpannedError),
    #[error("the RGBA8 image profile does not support generated mipmaps")]
    UnsupportedMipmaps,
    #[error(transparent)]
    Decode(#[from] ImageDecodeError),
    #[cfg(feature = "cook")]
    #[error(transparent)]
    Encode(#[from] super::CookedImageError),
}

/// Interprets a PNG using its `nara.image` sidecar, shared by authoring and Cook.
pub fn decode_png_source(bytes: &[u8], meta: &AssetMeta) -> Result<ImageAsset, ImageImportError> {
    let settings = ImageImportSettings::from_meta(meta)?;
    if settings.mipmaps {
        return Err(ImageImportError::UnsupportedMipmaps);
    }
    Ok(ImageAsset::from_png(bytes, settings.color_space())?)
}
