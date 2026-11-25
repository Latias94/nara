use std::path::Path;

use nara::fs::{DirectoryCapability, FsError};
use nara::project_host::{open_explicit_product_project_root, open_product_project_root};
use nara::scene::fixed_spatial::FixedSpatialPlugin;

/// Returns the one product recipe shared by the generated targets.
#[must_use]
pub fn product_recipe() -> Result<nara::ProductRecipe, nara::ProductRecipeError> {
    let recipe = nara::ProductRecipe::new()
        .add_plugin::<FixedSpatialPlugin>()?
        .add_contribution(collect_gameplay::contribution()?)?;
    #[cfg(feature = "desktop")]
    let recipe = recipe.add_plugin::<collect_gameplay::DesktopInputPlugin>()?;
    Ok(recipe)
}

/// Opens the ordinary project source without consulting the process CWD.
///
/// Development binaries use the Cargo manifest directory. A packaged executable under `bin/`
/// uses its sibling `project/` directory.
pub fn open_project_root() -> Result<DirectoryCapability, FsError> {
    open_product_project_root(env!("CARGO_MANIFEST_DIR"))
}

/// Opens an explicit absolute project root for an authored content fixture.
#[doc(hidden)]
pub fn open_explicit_project_root(root: &Path) -> Result<DirectoryCapability, FsError> {
    open_explicit_product_project_root(root)
}
