use nara_app::{App, Plugin, PluginCategory, PluginError, PluginId, PluginMetadata};

use crate::ComponentRegistry;

pub const COMPONENT_REGISTRY_PLUGIN_ID: PluginId = PluginId::new("nara.reflect.registry");
pub const COMPONENT_REGISTRY_PLUGIN_REQUIREMENT: &[PluginId] = &[COMPONENT_REGISTRY_PLUGIN_ID];

#[derive(Debug, Default, Clone, Copy)]
pub struct ComponentRegistryPlugin;

impl Plugin for ComponentRegistryPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata::new(COMPONENT_REGISTRY_PLUGIN_ID, PluginCategory::Core)
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
                    self.plugin_id(),
                    "component-schema-catalog",
                    error,
                )
            })?;
        Ok(())
    }
}
