mod catalog;
mod hydration;
mod projection;
mod store;
mod transaction;

use nara_asset::AssetMetaFileLimits;

pub(super) use crate::project_host::authoring::{
    MAX_AUTHORING_IMAGE_IDENTITIES as MAX_LAST_GOOD_IMAGES,
    MAX_AUTHORING_IMAGE_IDENTITY_PATH_BYTES as MAX_RETAINED_PATH_BYTES,
};

pub(crate) use hydration::EditorImageHydrationFailureKind;
pub(crate) use projection::EditorImageImportProjection;
pub(super) use projection::{domain_settings, project_request_outcome};
#[cfg(test)]
pub(crate) use store::EditorImageLastGood;
pub(crate) use transaction::EditorImageImportState;
pub(crate) use transaction::{
    EditorImageImportFailure, EditorImageImportFrame, EditorImageImportHost,
    EditorImageImportRequestResult,
};

impl super::EditorProjectSession {
    /// Drains admitted image work while ordinary image admission is fenced by the Session.
    pub(super) fn drain_schema_adoption_image_work(&mut self) {
        self.image_import.advance_schema_adoption_drain();
    }
}

fn metadata_byte_limit() -> u64 {
    u64::try_from(AssetMetaFileLimits::default().encoded_bytes().get()).unwrap_or(u64::MAX)
}
