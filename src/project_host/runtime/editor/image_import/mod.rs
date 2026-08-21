mod catalog;
mod hydration;
mod projection;
mod store;
mod transaction;

use nara_asset::AssetMetaFileLimits;

pub(crate) use hydration::EditorImageHydrationFailureKind;
pub(crate) use projection::EditorImageImportProjection;
pub(super) use projection::{domain_settings, project_request_outcome};
#[cfg(test)]
pub(crate) use store::EditorImageLastGood;
#[cfg(test)]
pub(crate) use transaction::EditorImageImportState;
pub(crate) use transaction::{
    EditorImageImportFailure, EditorImageImportFrame, EditorImageImportHost,
    EditorImageImportRequestResult,
};

fn metadata_byte_limit() -> u64 {
    u64::try_from(AssetMetaFileLimits::default().encoded_bytes().get()).unwrap_or(u64::MAX)
}
