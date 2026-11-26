use nara_app::{App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId};
use nara_ecs::{Resource, World};

use crate::{
    ComponentCodecError, ComponentRegistryError, ComponentSchemaCatalog, ComponentTypeId, Schema,
};

pub const COMPONENT_REGISTRY_PLUGIN_ID: PluginId = PluginId::new("nara.reflect.registry");
pub const COMPONENT_REGISTRY_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(COMPONENT_REGISTRY_PLUGIN_ID, PluginCategory::Core);

#[derive(Debug, Default, Clone, Copy)]
pub struct ComponentRegistryPlugin;

#[derive(Default, Resource)]
struct RegisteredSchemas(ComponentSchemaCatalog);

impl Plugin for ComponentRegistryPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &COMPONENT_REGISTRY_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?.init_resource::<RegisteredSchemas>();
        Ok(())
    }
}

/// Returns the component metadata registered by the running game or its Schema exporter.
#[must_use]
pub fn component_registry(world: &World) -> Option<&ComponentSchemaCatalog> {
    world
        .get_resource::<RegisteredSchemas>()
        .map(|registry| &registry.0)
}

pub trait RegisterSchema {
    /// Registers a component's Schema in an app with [`ComponentRegistryPlugin`] installed.
    fn register_schema<T: Schema>(&mut self) -> Result<&mut Self, SchemaRegistrationError>;
}

impl RegisterSchema for App {
    fn register_schema<T: Schema>(&mut self) -> Result<&mut Self, SchemaRegistrationError> {
        let mut registry = self
            .world_mut()?
            .get_resource_mut::<RegisteredSchemas>()
            .ok_or(SchemaRegistrationError::MissingRegistry)?;
        let schema = T::type_schema()?;
        if schema.id().as_str() != T::COMPONENT_ID {
            return Err(SchemaRegistrationError::ComponentIdMismatch {
                expected: T::COMPONENT_ID,
                actual: schema.id().clone(),
            });
        }
        registry.0.insert(schema)?;
        Ok(self)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SchemaRegistrationError {
    #[error("the app has no component registry")]
    MissingRegistry,
    #[error("the component declares '{expected}' but provided schema '{actual}'")]
    ComponentIdMismatch {
        expected: &'static str,
        actual: ComponentTypeId,
    },
    #[error(transparent)]
    App(#[from] PluginError),
    #[error(transparent)]
    Codec(#[from] ComponentCodecError),
    #[error(transparent)]
    Catalog(#[from] ComponentRegistryError),
}

/// Serializes the registered Schema catalog without running startup or opening any platform.
#[cfg(feature = "serde")]
pub fn schema_json(app: &App) -> Result<String, SchemaExportError> {
    let catalog = component_registry(app.world()).ok_or(SchemaExportError::MissingRegistry)?;
    Ok(catalog.to_json_string()?)
}

#[cfg(feature = "serde")]
#[derive(Debug, thiserror::Error)]
pub enum SchemaExportError {
    #[error("the app has no component registry")]
    MissingRegistry,
    #[error(transparent)]
    Encode(#[from] crate::ComponentCatalogFileError),
}
