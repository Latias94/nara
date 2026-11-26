//! Image asset APIs and renderer-facing image snapshots.
//!
//! Image data and source loading are owned by [`nara_asset::image`].
//! `render-prepare` adds captured resource values. Combining it with `image-runtime` enables
//! automatic preparation from published assets.

#[cfg(any(feature = "render-prepare", test))]
mod prepare;

#[cfg(feature = "image-runtime")]
pub use nara_asset::image::{
    IMAGE_PLUGIN_DECLARATION, IMAGE_PLUGIN_ID, ImageDecodeError, ImagePlugin,
};
pub use nara_asset::image::{
    ImageAsset, ImageAssetCreateError, ImageColorSpace, ImageContentHash, ImageExtent, ImageFormat,
    ImagePixels,
};
#[cfg(all(feature = "image-runtime", any(unix, windows)))]
pub use nara_asset::image::{ImageLoadError, ImageSourceDirectory, load_image};
#[cfg(all(feature = "image-runtime", feature = "render-prepare"))]
pub use prepare::runtime::{
    IMAGE_PREPARE_PLUGIN_DECLARATION, IMAGE_PREPARE_PLUGIN_ID, ImagePreparePlugin,
    ImagePrepareStats, prepare_images,
};
#[cfg(any(feature = "render-prepare", test))]
pub use prepare::{
    PreparedImageResource, PreparedImageSnapshot, PreparedImages, image_content_hash,
    image_resource_key,
};
