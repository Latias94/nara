use nara_app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};

use crate::{ASSET_PLUGIN_ID, AssetAppExt};

use super::ImageAssetLoader;

/// Initializes the typed image store and registers its runtime loader.
#[derive(Debug, Default, Clone, Copy)]
pub struct ImagePlugin;

pub const IMAGE_PLUGIN_ID: PluginId = PluginId::new("nara.image");
pub const IMAGE_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(IMAGE_PLUGIN_ID, PluginCategory::Asset)
        .requires_plugins(&[ASSET_PLUGIN_ID]);

impl Plugin for ImagePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &IMAGE_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.register_asset_loader(ImageAssetLoader)?;
        Ok(())
    }
}
