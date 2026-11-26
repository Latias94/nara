//! In-memory fixture assembly from repository-style test directories.

use nara_core::vfs::capabilities::{DirectoryCapability, RelativePath};

use super::*;
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
        let members = admitted.catalog.members();
        let backing = PackBacking::capture_from(
            catalog_bytes,
            members.iter().map(RuntimeContentMember::digest),
            |index, digest| {
                let path = RelativePath::new(members[index].path())
                    .expect("catalog admits portable relative member paths");
                directory
                    .open_file(&path)?
                    .read_to_end_bounded(digest.length())
            },
        )?;
        Ok(admitted.publish(backing, retained))
    }
}
