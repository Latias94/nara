//! CPU import and local cooked artifact caching for authoring tools.

#[cfg(any(unix, windows))]
mod cache;

pub use crate::image::{ImageImportError, ImageImportFilter, ImageImportSettings};
#[cfg(any(unix, windows))]
pub use cache::{ImportCache, ImportCacheError, ImportCacheKey};

use crate::{AssetMeta, image::decode_png_source};

/// Version of the PNG-to-NIMG importer, for cache keys.
pub const IMAGE_IMPORTER_VERSION: &str = "nara.image.png-rgba8.v1";

/// Decodes source PNG bytes and encodes a cooked NIMG payload for the RGBA8 profile.
pub fn cook_png(source: &[u8], meta: &AssetMeta) -> Result<Vec<u8>, ImageImportError> {
    Ok(decode_png_source(source, meta)?.encode_cooked()?)
}
