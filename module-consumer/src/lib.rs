use nara_reflect::{ComponentRegistry, ComponentRegistryError};
use nara_scene::register_scene_components;

/// Builds the exact frozen registry used by the direct scene-module example.
pub fn frozen_scene_registry() -> Result<ComponentRegistry, ComponentRegistryError> {
    let mut registry = ComponentRegistry::new();
    register_scene_components(&mut registry)?;
    registry.freeze()?;
    Ok(registry)
}
