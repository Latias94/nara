use nara_app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};

use crate::{ASSET_PLUGIN_ID, Assets};

use super::ImageAsset;

/// Initializes the typed image store for code-first loading and rendering.
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
        app.init_resource::<Assets<ImageAsset>>()?;
        Ok(())
    }
}
