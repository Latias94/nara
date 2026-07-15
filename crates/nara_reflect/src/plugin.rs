use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId, PluginPreflightContext,
    PluginPreflightResource,
};

use crate::ComponentRegistry;

pub const COMPONENT_REGISTRY_PLUGIN_ID: PluginId = PluginId::new("nara.reflect.registry");
pub const COMPONENT_REGISTRY_PLUGIN_REQUIREMENT: &[PluginId] = &[COMPONENT_REGISTRY_PLUGIN_ID];
pub const COMPONENT_REGISTRY_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(COMPONENT_REGISTRY_PLUGIN_ID, PluginCategory::Core);

#[derive(Debug, Default, Clone, Copy)]
pub struct ComponentRegistryPlugin;

impl PluginPreflightResource for ComponentRegistry {}

pub fn registry_for_plugin_preflight<'a>(
    context: &'a PluginPreflightContext<'_>,
    plugin: PluginId,
    component: &str,
) -> Result<&'a ComponentRegistry, PluginError> {
    context
        .get_structural_resource::<ComponentRegistry>()
        .ok_or_else(|| {
            PluginError::component_registration(
                plugin,
                component,
                "component registry resource is unavailable",
            )
        })
}

impl Plugin for ComponentRegistryPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &COMPONENT_REGISTRY_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<ComponentRegistry>()?;
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?
            .resource_mut::<ComponentRegistry>()
            .freeze()
            .map_err(|error| {
                PluginError::component_registration(
                    COMPONENT_REGISTRY_PLUGIN_ID,
                    "component-schema-catalog",
                    error,
                )
            })?;
        Ok(())
    }
}
