//! Image asset APIs and renderer-facing image snapshots.
//!
//! Asset types, CPU codecs and source import services are re-exported from [`nara_asset::image`].
//! `render-prepare` adds captured resource values. Combining it with `image-runtime` enables
//! automatic preparation from published assets.

mod content;
#[cfg(any(feature = "render-prepare", test))]
mod prepare;

pub use content::{ImageContentError, ImageContentSnapshot};
#[cfg(feature = "serde")]
pub use nara_asset::image::ImageRuntimeCodec;
#[cfg(feature = "image-runtime")]
pub use nara_asset::image::{
    AdmittedImageImport, IMAGE_PLUGIN_DECLARATION, IMAGE_PLUGIN_ID,
    IMAGE_SOURCE_PLUGIN_DECLARATION, IMAGE_SOURCE_PLUGIN_ID, ImageBytesImportRequest,
    ImageFileImportRequest, ImageImportError, ImageImportStage, ImageImportedAsset, ImageImporter,
    ImageImporterCreateError, ImagePlugin, ImagePngFailureKind, ImagePublicationFailureKind,
    ImageReloadError, ImageReloadStats, ImageSourceDirectory, ImageSourceFailureKind,
    ImageSourcePlugin, ImageUnsupportedFeature, UnpublishedImageImport, source_plugin,
};
pub use nara_asset::image::{
    IMAGE_IMPORT_MEMORY_PLAN_VERSION, ImageAsset, ImageAssetCreateError, ImageColorSpace,
    ImageContentHash, ImageCookError, ImageCookLimits, ImageCookSettings, ImageCookTarget,
    ImageDomain, ImageExtent, ImageFormat, ImageImportBudgetError, ImageImportBudgetHost,
    ImageImportBudgetSnapshot, ImageImportLimitKind, ImageImportLimits, ImageImportLimitsError,
    ImageImportMemoryPlan, ImageImportSettings, ImageOversizePolicy, ImageSourceMetadata,
    ImageSourceMetadataCreateError, cook_runtime_image,
};
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
