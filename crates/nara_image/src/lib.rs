//! Backend-neutral image assets and PNG-first importing.

// Bevy derives resolve this path; Nara's ECS facade remains the actual dependency.
mod asset;
mod budget;
mod content;
mod import;
mod limits;
#[cfg(any(feature = "render-prepare", test))]
mod prepare;
mod reload;
mod runtime;
mod settings;

pub use asset::{
    ImageAsset, ImageAssetCreateError, ImageColorSpace, ImageContentHash, ImageExtent, ImageFormat,
    ImageSourceMetadata, ImageSourceMetadataCreateError,
};
pub use budget::{ImageImportBudgetHost, ImageImportBudgetSnapshot};
pub use content::{ImageContentError, ImageContentSnapshot};
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
#[cfg(any(feature = "render-prepare", test))]
pub use prepare::{
    IMAGE_PREPARE_PLUGIN_DECLARATION, IMAGE_PREPARE_PLUGIN_ID, ImagePreparePlugin,
    ImagePrepareStats, PreparedImageResource, PreparedImageSnapshot, PreparedImages,
    image_content_hash, image_resource_key, prepare_images,
};
pub use reload::{
    IMAGE_PLUGIN_DECLARATION, IMAGE_PLUGIN_ID, IMAGE_SOURCE_PLUGIN_DECLARATION,
    IMAGE_SOURCE_PLUGIN_ID, ImagePlugin, ImageReloadError, ImageReloadStats, ImageSourcePlugin,
    source_plugin,
};
pub use runtime::ImageDomain;
#[cfg(feature = "serde")]
pub use runtime::ImageRuntimeCodec;
pub use settings::ImageImportSettings;

#[cfg(test)]
mod tests;
