use engine::app::{PluginGroup, PluginGroupBuilder, PluginGroupId};
use engine::{PresentationUiPlugins, ProductRecipe, ProductRecipeError, RuntimeUiPlugins};

pub fn product_groups() -> (RuntimeUiPlugins, PresentationUiPlugins) {
    (RuntimeUiPlugins, PresentationUiPlugins)
}

/// The same pure recipe is accepted by file-backed Hosts and direct composition.
pub fn product_recipe(font: engine::ui::FontFace) -> Result<ProductRecipe, ProductRecipeError> {
    ProductRecipe::new().with_ui_font(font)
}

pub struct UiDefaults;

impl PluginGroup for UiDefaults {
    const ID: PluginGroupId = PluginGroupId::new("consumer.ui-defaults");

    fn build(self) -> PluginGroupBuilder {
        PluginGroupBuilder::new()
            .add_group(RuntimeUiPlugins)
            .add_group(PresentationUiPlugins)
    }
}

pub fn configured_product(
    font: engine::ui::FontFace,
) -> Result<impl PluginGroup, ProductRecipeError> {
    Ok(product_recipe(font)?.with_defaults(UiDefaults))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renamed_root_consumer_configures_both_default_owners() {
        let font = engine::ui::FontFace::from_bytes(
            include_bytes!("../../../../../crates/nara_text/tests/fixtures/FiraMono-Medium.ttf")
                .to_vec(),
            0,
        )
        .unwrap();
        let plan =
            engine::app::PluginPlan::resolve(configured_product(font.clone()).unwrap()).unwrap();
        for (plugin, expected) in [
            (engine::ui::UI_PLUGIN_ID, engine::ui::plugin(font.clone())),
            (
                engine::ui_render::UI_RENDER_PLUGIN_ID,
                engine::ui_render::plugin(font),
            ),
        ] {
            let entry = plan
                .entries()
                .iter()
                .find(|entry| entry.plugin_id() == plugin)
                .unwrap();
            assert_eq!(entry.definition_key(), expected.key());
        }
        plan.instantiate().unwrap();
    }
}
