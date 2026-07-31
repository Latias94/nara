use nara::prelude::*;

const DEBUG_OVERLAY_PLUGIN_ID: PluginId = PluginId::new("example.debug-overlay");
const DEBUG_OVERLAY_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(DEBUG_OVERLAY_PLUGIN_ID, PluginCategory::Runtime);

#[derive(Default)]
struct DebugOverlayPlugin;

impl Plugin for DebugOverlayPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &DEBUG_OVERLAY_PLUGIN_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn recipe() -> Result<ProductRecipe, ProductRecipeError> {
    ProductRecipe::new().add_plugin::<DebugOverlayPlugin>()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, recipe()?))?;
    Ok(())
}
