use nara_app::{
    App, CoreStage, Plugin, PluginDefinition, PluginDefinitionId, PluginError,
    PluginPreflightContext,
};
use nara_ecs::schedule::IntoScheduleConfigs;

use crate::{
    FontFace, UiActionJournal, UiAdapterDiagnostics, UiRouteRejectionLog, UiTheme, interaction,
    register_ui_components, runtime, text,
};

#[derive(Debug, Default, Clone)]
pub struct UiPlugin {
    font: Option<FontFace>,
}

impl UiPlugin {
    /// Selects one admitted font for every surface compiled by this plugin instance.
    #[must_use]
    pub fn with_font(font: FontFace) -> Self {
        Self { font: Some(font) }
    }
}

const UI_PLUGIN_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.ui.configured", 1);

/// Creates a repeatable UI plugin definition whose identity includes the selected font content.
#[must_use]
pub fn plugin(font: FontFace) -> PluginDefinition {
    let mut configuration = b"nara.ui.font.v1\0".to_vec();
    configuration.extend_from_slice(font.id().as_bytes());
    PluginDefinition::infallible::<UiPlugin, _>(UI_PLUGIN_DEFINITION_ID, configuration, move || {
        UiPlugin::with_font(font.clone())
    })
}

pub const UI_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.ui");
pub const UI_SCHEMA_PROVIDER_ID: nara_app::PluginSchemaProviderId =
    nara_app::PluginSchemaProviderId::new("nara.ui.components");
pub const UI_SCHEMA_OWNER_ID: nara_reflect::ComponentSchemaOwnerId =
    nara_reflect::ComponentSchemaOwnerId::new("nara.ui.components");
pub const UI_SCHEMA_PROVIDER: nara_reflect::ComponentSchemaProviderDefinition =
    nara_reflect::ComponentSchemaProviderDefinition::with_validation(
        UI_SCHEMA_OWNER_ID,
        UI_SCHEMA_PROVIDER_ID,
        nara_reflect::ComponentSchemaProviderBindingId::new("nara.ui.components.native", 1),
        ui_schema_catalog,
        crate::codec::validate_ui_components,
        register_ui_components,
    );
const UI_PLUGIN_REQUIREMENTS: &[nara_app::PluginId] = &[
    nara_reflect::COMPONENT_REGISTRY_PLUGIN_ID,
    nara_input::INPUT_PLUGIN_ID,
    nara_scene::hierarchy::HIERARCHY_PLUGIN_ID,
];
const UI_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("runtime-ui")];
pub const UI_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(UI_PLUGIN_ID, nara_app::PluginCategory::Runtime)
        .requires_plugins(UI_PLUGIN_REQUIREMENTS)
        .requires_product_capabilities(UI_PRODUCT_REQUIREMENTS)
        .provides_schema(&[UI_SCHEMA_PROVIDER_ID]);

fn ui_schema_catalog()
-> Result<nara_reflect::ComponentSchemaCatalog, nara_reflect::ComponentSchemaProviderSourceError> {
    Ok(crate::codec::ui_schema_catalog())
}

impl Plugin for UiPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &UI_PLUGIN_DECLARATION
    }

    fn preflight(&self, context: &PluginPreflightContext<'_>) -> Result<(), PluginError> {
        nara_reflect::validate_schema_provider_for_plugin(
            context,
            UI_PLUGIN_ID,
            UI_SCHEMA_PROVIDER_ID.as_str(),
            &UI_SCHEMA_PROVIDER,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let font = match self.font.clone() {
            Some(font) => font,
            None => text::bundled_font_face().map_err(|error| PluginError::SetupFailed {
                plugin: UI_PLUGIN_ID,
                message: error.to_string(),
            })?,
        };
        nara_reflect::register_schema_provider_for_plugin(
            app,
            UI_PLUGIN_ID,
            UI_SCHEMA_PROVIDER_ID.as_str(),
            &UI_SCHEMA_PROVIDER,
        )?;
        if !app.world().contains_resource::<runtime::UiDefaultTheme>() {
            app.insert_resource(runtime::UiDefaultTheme(UiTheme::default()))?;
        }
        app.insert_resource(runtime::UiRuntimeState::new(font))?;
        app.init_resource::<UiAdapterDiagnostics>()?;
        app.init_resource::<UiActionJournal>()?;
        app.init_resource::<UiRouteRejectionLog>()?;
        app.add_systems(
            CoreStage::First,
            interaction::route_timeline_ui_input.in_set(nara_input::InputSet::RouteUi),
        )?;
        app.add_systems(
            CoreStage::Extract,
            runtime::compile_ui_frames
                .after(nara_scene::hierarchy::HierarchySet::ValidateAndComplete),
        )?;
        Ok(())
    }
}
