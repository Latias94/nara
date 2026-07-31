use nara::prelude::*;

const ONE_SHOT_PLUGIN_ID: PluginId = PluginId::new("example.one-shot");
const ONE_SHOT_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(ONE_SHOT_PLUGIN_ID, PluginCategory::Runtime);

struct OneShotPlugin;

impl Plugin for OneShotPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &ONE_SHOT_PLUGIN_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }
}

fn main() {
    let _recipe = ProductRecipe::new().add_plugin(OneShotPlugin);
}
