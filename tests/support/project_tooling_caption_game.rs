use nara::{
    ProductRecipe, ProductRecipeError, SchemaContribution,
    app::{PluginCategory, PluginDeclaration, PluginError, PluginId, PluginSchemaProviderId},
    asset::AssetRef,
    ecs::{Query, Res},
    prelude::{App, Component, PersistentComponent, Plugin},
    project_host::ProjectDomainAssets,
    reflect::{
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId, ComponentSchemaProviderDefinition,
    },
};

use crate::text_domain::TextDomain;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "studio.caption.Caption",
    version = 1,
    alias = "Caption",
    component_capabilities(scene),
    field_capabilities(scene)
)]
pub struct Caption {
    #[nara(id = "text", alias = "Text", capabilities(scene, asset_ref))]
    pub text: AssetRef,
}

nara::persistent_components! { pub mod caption_components = [Caption]; }

const PROVIDER_ID: PluginSchemaProviderId = PluginSchemaProviderId::new("studio.caption.schema");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("studio.caption"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("studio.caption.native", 1),
    caption_components::initial_catalog,
    caption_components::register,
);

#[derive(Default)]
pub struct CaptionPlugin;

impl Plugin for CaptionPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PluginId::new("studio.caption"), PluginCategory::Runtime)
                .requires_plugins(&[
                    nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID,
                    nara::advanced_prelude::STARTUP_SCENE_ACTIVATION_PLUGIN_ID,
                ])
                .provides_schema(&[PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        nara::advanced_prelude::add_startup_scene_activation_systems(app, observe_caption)?;
        Ok(())
    }
}

fn observe_caption(captions: Query<&Caption>, assets: Res<ProjectDomainAssets>) {
    let marker = std::env::var_os("NARA_TEST_TEXT_MARKER");
    if let Some(path) = &marker {
        std::fs::write(path, "entered\n").expect("the fixture marker must be writable");
    }
    let caption = captions.single().expect("the fixture has one caption");
    let text = assets
        .collection::<TextDomain>()
        .and_then(|texts| texts.get(&caption.text))
        .expect("the selected external domain must publish the referenced text");
    assert_eq!(text.text, "Hello world");
    if let Some(path) = marker {
        std::fs::write(path, format!("{}\n", text.text))
            .expect("the fixture marker must be writable");
    }
}

pub fn with_caption(recipe: ProductRecipe) -> Result<ProductRecipe, ProductRecipeError> {
    recipe
        .add_contribution(SchemaContribution::<CaptionPlugin>::for_default([
            PROVIDER,
        ])?)?
        .add_asset_domain::<TextDomain>()
}
