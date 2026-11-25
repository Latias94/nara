use std::path::Path;

use nara::fs::{DirectoryCapability, FsError};
use nara::project_host::{open_explicit_product_project_root, open_product_project_root};

mod gameplay;

/// Returns the one product recipe shared by the generated targets.
#[must_use]
pub fn product_recipe() -> Result<nara::ProductRecipe, nara::ProductRecipeError> {
    nara::ProductRecipe::new()
        .add_runtime_image_codec::<nara::image::ImageRuntimeCodec>(
            nara::runtime_content::RuntimeCodecDefinitionId::new(
                "nara.starter.desktop-2d.image",
                1,
            ),
        )?
        .add_runtime_asset_codec::<nara::input::ActionMapDomain, nara::input::ActionMapRuntimeCodec>(
            nara::runtime_content::RuntimeCodecDefinitionId::new(
                "nara.starter.desktop-2d.actions",
                1,
            ),
        )?
        .add_plugin::<gameplay::GameplayPlugin>()
}

/// Returns the v2 recipe with the optional native controller collector for the desktop target.
pub fn desktop_product_recipe() -> Result<nara::ProductRecipe, nara::ProductRecipeError> {
    let recipe = product_recipe()?;
    #[cfg(all(windows, feature = "desktop"))]
    let recipe = recipe.add_configured_plugin((), |_| {
        use nara::input::{GamepadRoute, GilrsInputPlugin, InputSeatId, InputWindowId};

        GilrsInputPlugin::new(Some(GamepadRoute::new(
            InputWindowId::new(nara::window::WindowId::PRIMARY.get()),
            InputSeatId::PRIMARY,
        )))
    })?;

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
