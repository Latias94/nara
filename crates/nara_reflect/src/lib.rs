//! Reflection and schema metadata boundary for nara data-facing components.

use std::{
    any::TypeId,
    collections::{BTreeMap, HashMap},
};

pub use bevy_reflect;
pub use bevy_reflect::prelude::*;
use bevy_reflect::{GetTypeRegistration, TypeRegistry};
use nara_ecs::Component;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ComponentTypeId(String);

impl ComponentTypeId {
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ComponentSchemaVersion(pub u32);

impl Default for ComponentSchemaVersion {
    fn default() -> Self {
        Self(1)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ComponentSchema {
    pub id: ComponentTypeId,
    pub version: ComponentSchemaVersion,
    pub rust_type_path: String,
}

pub struct ComponentRegistry {
    type_registry: TypeRegistry,
    schemas: BTreeMap<ComponentTypeId, ComponentSchema>,
    rust_type_ids: HashMap<TypeId, ComponentTypeId>,
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl ComponentRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            type_registry: TypeRegistry::default(),
            schemas: BTreeMap::new(),
            rust_type_ids: HashMap::new(),
        }
    }

    pub fn register_component<T>(
        &mut self,
        id: ComponentTypeId,
        version: ComponentSchemaVersion,
    ) -> &mut Self
    where
        T: Component + Reflect + GetTypeRegistration,
    {
        self.type_registry.register::<T>();
        let schema = ComponentSchema {
            id: id.clone(),
            version,
            rust_type_path: std::any::type_name::<T>().to_string(),
        };
        self.rust_type_ids.insert(TypeId::of::<T>(), id.clone());
        self.schemas.insert(id, schema);
        self
    }

    #[must_use]
    pub fn schema(&self, id: &ComponentTypeId) -> Option<&ComponentSchema> {
        self.schemas.get(id)
    }

    #[must_use]
    pub fn schema_for_type<T: 'static>(&self) -> Option<&ComponentSchema> {
        self.rust_type_ids
            .get(&TypeId::of::<T>())
            .and_then(|id| self.schemas.get(id))
    }

    #[must_use]
    pub fn type_registry(&self) -> &TypeRegistry {
        &self.type_registry
    }

    pub fn type_registry_mut(&mut self) -> &mut TypeRegistry {
        &mut self.type_registry
    }
}

pub mod prelude {
    pub use crate::{ComponentRegistry, ComponentSchema, ComponentSchemaVersion, ComponentTypeId};
    pub use bevy_reflect::prelude::*;
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_ecs::Component;

    #[derive(Component, Reflect)]
    struct Position {
        x: f32,
        y: f32,
    }

    #[test]
    fn registers_component_schema_by_stable_id_and_rust_type() {
        let mut registry = ComponentRegistry::new();
        let id = ComponentTypeId::new("nara.test.Position");

        registry.register_component::<Position>(id.clone(), ComponentSchemaVersion(1));

        assert_eq!(
            registry.schema(&id).unwrap().version,
            ComponentSchemaVersion(1)
        );
        assert_eq!(
            registry.schema_for_type::<Position>().unwrap().id.as_str(),
            "nara.test.Position"
        );
    }
}
