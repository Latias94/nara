//! Backend-neutral image assets and PNG-first importing.
//!
//! Image content and Runtime codecs need no App/ECS installation. `render-prepare` adds captured
//! resource values; `runtime` enables PNG import/reload services and, with `render-prepare`, the
//! automatic preparation plugin. The independent `serde` feature enables serialization.

mod asset;
mod budget;
mod content;
mod cook;
#[cfg(feature = "runtime")]
mod import;
mod limits;
#[cfg(any(feature = "render-prepare", test))]
mod prepare;
#[cfg(feature = "runtime")]
mod reload;
mod runtime;
mod settings;

pub use asset::{
    ImageAsset, ImageAssetCreateError, ImageColorSpace, ImageContentHash, ImageExtent, ImageFormat,
    ImageSourceMetadata, ImageSourceMetadataCreateError,
};
pub use budget::{ImageImportBudgetHost, ImageImportBudgetSnapshot};
pub use content::{ImageContentError, ImageContentSnapshot};
pub use cook::{
    ImageCookError, ImageCookLimits, ImageCookSettings, ImageCookTarget, ImageOversizePolicy,
    cook_runtime_image,
};
#[cfg(feature = "runtime")]
pub use import::{
    AdmittedImageImport, ImageBytesImportRequest, ImageFileImportRequest, ImageImportError,
    ImageImportStage, ImageImportedAsset, ImageImporter, ImageImporterCreateError,
    ImagePngFailureKind, ImagePublicationFailureKind, ImageSourceDirectory, ImageSourceFailureKind,
    ImageUnsupportedFeature, UnpublishedImageImport,
};
pub use limits::{
    IMAGE_IMPORT_MEMORY_PLAN_VERSION, ImageImportBudgetError, ImageImportLimitKind,
    ImageImportLimits, ImageImportLimitsError, ImageImportMemoryPlan,
};
#[cfg(all(feature = "runtime", feature = "render-prepare"))]
pub use prepare::runtime::{
    IMAGE_PREPARE_PLUGIN_DECLARATION, IMAGE_PREPARE_PLUGIN_ID, ImagePreparePlugin,
    ImagePrepareStats, prepare_images,
};
#[cfg(any(feature = "render-prepare", test))]
pub use prepare::{
    PreparedImageResource, PreparedImageSnapshot, PreparedImages, image_content_hash,
    image_resource_key,
};
#[cfg(feature = "runtime")]
pub use reload::{
    IMAGE_PLUGIN_DECLARATION, IMAGE_PLUGIN_ID, IMAGE_SOURCE_PLUGIN_DECLARATION,
    IMAGE_SOURCE_PLUGIN_ID, ImagePlugin, ImageReloadError, ImageReloadStats, ImageSourcePlugin,
    source_plugin,
};
pub use runtime::ImageDomain;
#[cfg(feature = "serde")]
pub use runtime::ImageRuntimeCodec;
pub use settings::ImageImportSettings;

#[cfg(all(test, feature = "runtime"))]
mod tests;
