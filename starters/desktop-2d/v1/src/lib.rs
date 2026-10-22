use std::path::Path;

use nara::fs::{DirectoryCapability, FsError};
use nara::project_host::{open_explicit_product_project_root, open_product_project_root};

#[doc(hidden)]
pub use nara::project_tooling::{ProductObservationTarget, record_product_observation};

/// Returns the one product recipe shared by the generated targets.
#[must_use]
pub fn product_recipe() -> nara::ProductRecipe {
    nara::ProductRecipe::new()
}

/// Opens the ordinary project source without consulting the process CWD.
///
/// Development binaries use the explicit generated Cargo package root. Exported binaries under
/// `bin/` use the sibling `project/` directory. Supervised desktop launches use
/// [`open_explicit_project_root`] with the root carried by Nara's inherited launch context.
pub fn open_project_root() -> Result<DirectoryCapability, FsError> {
    open_product_project_root(env!("CARGO_MANIFEST_DIR"))
}

/// Opens the explicit absolute project root carried by a supervised desktop launch context.
#[doc(hidden)]
pub fn open_explicit_project_root(root: &Path) -> Result<DirectoryCapability, FsError> {
    open_explicit_product_project_root(root)
}
