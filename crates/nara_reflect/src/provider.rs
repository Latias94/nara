use std::fmt;

use nara_app::PluginSchemaProviderId;

use crate::{ComponentRegistry, ComponentRegistryError};

type RegisterComponentSchemas = fn(&mut ComponentRegistry) -> Result<(), ComponentRegistryError>;

/// Stable identity of one native schema-registration policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentSchemaProviderBindingId {
    id: &'static str,
    version: u32,
}

impl ComponentSchemaProviderBindingId {
    #[must_use]
    pub const fn new(id: &'static str, version: u32) -> Self {
        Self { id, version }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.id
    }

    #[must_use]
    pub const fn version(self) -> u32 {
        self.version
    }
}

/// A stable, replayable component-schema contribution owned by one plugin declaration.
#[derive(Clone, Copy)]
pub struct ComponentSchemaProviderDefinition {
    id: PluginSchemaProviderId,
    binding: ComponentSchemaProviderBindingId,
    register: RegisterComponentSchemas,
}

impl ComponentSchemaProviderDefinition {
    #[must_use]
    pub const fn new(
        id: PluginSchemaProviderId,
        binding: ComponentSchemaProviderBindingId,
        register: RegisterComponentSchemas,
    ) -> Self {
        Self {
            id,
            binding,
            register,
        }
    }

    #[must_use]
    pub const fn id(self) -> PluginSchemaProviderId {
        self.id
    }

    #[must_use]
    pub const fn binding(self) -> ComponentSchemaProviderBindingId {
        self.binding
    }

    #[doc(hidden)]
    #[must_use]
    pub fn has_same_binding(self, other: Self) -> bool {
        self.id == other.id && self.binding == other.binding
    }

    pub fn register_into(
        self,
        registry: &mut ComponentRegistry,
    ) -> Result<(), ComponentRegistryError> {
        (self.register)(registry)
    }
}

impl fmt::Debug for ComponentSchemaProviderDefinition {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ComponentSchemaProviderDefinition")
            .field("id", &self.id)
            .field("binding", &self.binding)
            .finish_non_exhaustive()
    }
}
