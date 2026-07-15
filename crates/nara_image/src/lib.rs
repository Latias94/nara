//! Backend-neutral image assets and PNG-first importing.

mod asset;
mod budget;
mod import;
mod limits;
mod prepare;
mod reload;

pub use asset::{ImageAsset, ImageColorSpace, ImageExtent, ImageFormat, ImageSourceMetadata};
pub use budget::{ImageImportBudgetHost, ImageImportBudgetSnapshot};
pub use import::{
    AdmittedImageImport, ImageBytesImportRequest, ImageFileImportRequest, ImageImportError,
    ImageImportStage, ImageImportedAsset, ImageImporter, ImageImporterCreateError,
    ImagePngFailureKind, ImagePublicationFailureKind, ImageSourceDirectory, ImageSourceFailureKind,
    ImageUnsupportedFeature,
};
pub use limits::{
    IMAGE_IMPORT_MEMORY_PLAN_VERSION, ImageImportBudgetError, ImageImportLimitKind,
    ImageImportLimits, ImageImportLimitsError, ImageImportMemoryPlan,
};
pub use prepare::{
    IMAGE_PREPARE_PLUGIN_DECLARATION, IMAGE_PREPARE_PLUGIN_ID, ImagePreparePlugin,
    ImagePrepareStats, PreparedImageResource, image_descriptor_hash, image_resource_key,
    prepare_images,
};
pub use reload::{
    IMAGE_PLUGIN_DECLARATION, IMAGE_PLUGIN_ID, ImagePlugin, ImageReloadError, ImageReloadStats,
    plugin,
};

#[cfg(test)]
mod tests;
