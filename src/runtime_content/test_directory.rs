//! In-memory fixture assembly from repository-style test directories.

use nara_core::vfs::capabilities::{DirectoryCapability, RelativePath};

use super::*;
use nara_asset::pack::CapturedPack;

impl RuntimeContentBase {
    pub(crate) fn capture_test_directory(
        directory: &DirectoryCapability,
        expected: &RuntimeContentExpectation,
        recipe: ProductRecipe,
        limits: RuntimeContentLimits,
    ) -> Result<Self, RuntimeContentError> {
        let (_, catalog_limits) = admission::input_limits(expected, limits)?;
        let path =
            RelativePath::new(RUNTIME_CONTENT_CATALOG_FILE).expect("fixed valid catalog path");
        let catalog_bytes = directory
            .open_file(&path)
            .map_err(RuntimeContentError::Host)?
            .read_to_end_bounded(expected.catalog_digest().length())
            .map_err(RuntimeContentError::Host)?;
        let admitted = admission::AdmittedCatalog::new(
            &catalog_bytes,
            expected,
            recipe,
            limits,
            catalog_limits,
        )?;
        let retained = admitted.retained_bytes(true)?;
        let mut members = Vec::with_capacity(admitted.catalog.members().len());
        for member in admitted.catalog.members() {
            let path = RelativePath::new(member.path())
                .expect("catalog admits portable relative member paths");
            let bytes = directory
                .open_file(&path)
                .map_err(RuntimeContentError::Host)?
                .read_to_end_bounded(member.digest().length())
                .map_err(RuntimeContentError::Host)?;
            if ContentDigest::of_bytes(&bytes) != member.digest() {
                return Err(RuntimeContentError::MemberDigestMismatch);
            }
            members.push(bytes);
        }
        let pack = CapturedPack::new(catalog_bytes, members).map_err(backing::pack_error)?;
        Ok(admitted.publish(EncodedBacking::Captured(pack), retained))
    }
}
