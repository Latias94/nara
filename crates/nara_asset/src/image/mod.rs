//! CPU image assets, cooked products and optional source importing.
//!
//! The `serde` feature enables the processed image codec. `image-import` enables PNG import,
//! publication and task-driven reload. These APIs do not depend on a renderer.

mod asset;
mod budget;
mod cook;
#[cfg(feature = "image-import")]
mod import;
mod limits;
#[cfg(feature = "image-import")]
mod reload;
mod runtime;
mod settings;

pub use asset::{
    ImageAsset, ImageAssetCreateError, ImageColorSpace, ImageContentHash, ImageExtent, ImageFormat,
    ImagePixels, ImageSourceMetadata, ImageSourceMetadataCreateError,
};
pub use budget::{ImageImportBudgetHost, ImageImportBudgetSnapshot};
pub use cook::{
    ImageCookError, ImageCookLimits, ImageCookSettings, ImageCookTarget, ImageOversizePolicy,
    cook_runtime_image,
};
#[cfg(feature = "image-import")]
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
#[cfg(feature = "image-import")]
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
#[path = "asset/tests.rs"]
mod asset_tests;

#[cfg(all(test, feature = "image-import"))]
mod tests;
