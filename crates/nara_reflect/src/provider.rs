use std::fmt;

use nara_app::PluginSchemaProviderId;

use crate::{ComponentRegistry, ComponentRegistryError};

type RegisterComponentSchemas = fn(&mut ComponentRegistry) -> Result<(), ComponentRegistryError>;
type ValidateComponentSchemas = fn(&ComponentRegistry) -> Result<(), ComponentRegistryError>;

fn no_provider_validation(_: &ComponentRegistry) -> Result<(), ComponentRegistryError> {
    Ok(())
}

/// Stable identity of one native schema-registration policy.
///
/// The binding, codec, and migration versions are deliberately explicit. They are the
/// process-independent receipts for executable schema behavior; function addresses are never
/// used as identity because they are not stable across builds or reloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentSchemaProviderBindingId {
    id: &'static str,
    version: u32,
    codec_version: u32,
    migration_version: u32,
}

impl ComponentSchemaProviderBindingId {
    #[must_use]
    pub const fn new(id: &'static str, version: u32) -> Self {
        Self {
            id,
            version,
            codec_version: version,
            migration_version: version,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.id
    }

    #[must_use]
    pub const fn version(self) -> u32 {
        self.version
    }

    #[must_use]
    pub const fn codec_version(self) -> u32 {
        self.codec_version
    }

    #[must_use]
    pub const fn migration_version(self) -> u32 {
        self.migration_version
    }

    #[must_use]
    pub const fn with_codec_version(mut self, version: u32) -> Self {
        self.codec_version = version;
        self
    }

    #[must_use]
    pub const fn with_migration_version(mut self, version: u32) -> Self {
        self.migration_version = version;
        self
    }
}

/// Stable receipt for one provider's executable schema behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ComponentSchemaProviderReceipt {
    provider: PluginSchemaProviderId,
    binding: ComponentSchemaProviderBindingId,
}

impl ComponentSchemaProviderReceipt {
    #[must_use]
    pub const fn new(
        provider: PluginSchemaProviderId,
        binding: ComponentSchemaProviderBindingId,
    ) -> Self {
        Self { provider, binding }
    }

    #[must_use]
    pub const fn provider(self) -> PluginSchemaProviderId {
        self.provider
    }

    #[must_use]
    pub const fn binding(self) -> ComponentSchemaProviderBindingId {
        self.binding
    }
}

/// A stable, replayable component-schema contribution owned by one plugin declaration.
#[derive(Clone, Copy)]
pub struct ComponentSchemaProviderDefinition {
    id: PluginSchemaProviderId,
    binding: ComponentSchemaProviderBindingId,
    validate: ValidateComponentSchemas,
    register: RegisterComponentSchemas,
}

impl ComponentSchemaProviderDefinition {
    #[must_use]
    pub const fn new(
        id: PluginSchemaProviderId,
        binding: ComponentSchemaProviderBindingId,
        register: RegisterComponentSchemas,
    ) -> Self {
        Self::with_validation(id, binding, no_provider_validation, register)
    }

    #[must_use]
    pub const fn with_validation(
        id: PluginSchemaProviderId,
        binding: ComponentSchemaProviderBindingId,
        validate: ValidateComponentSchemas,
        register: RegisterComponentSchemas,
    ) -> Self {
        Self {
            id,
            binding,
            validate,
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

    #[must_use]
    pub const fn receipt(self) -> ComponentSchemaProviderReceipt {
        ComponentSchemaProviderReceipt::new(self.id, self.binding)
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

    /// Preflights this provider without mutating the registry.
    ///
    /// A building registry runs the provider's structural validation. A frozen registry already
    /// contains executable behavior, so preflight validates the stable receipt and never replays
    /// provider code.
    pub fn preflight(self, registry: &ComponentRegistry) -> Result<(), ComponentRegistryError> {
        if registry.is_frozen() {
            registry.validate_schema_provider(self)
        } else {
            (self.validate)(registry)
        }
    }

    /// Registers this provider once or validates it against an admitted frozen snapshot.
    pub fn register_or_validate_into(
        self,
        registry: &mut ComponentRegistry,
    ) -> Result<(), ComponentRegistryError> {
        registry
            .register_or_validate_schema_provider(self)
            .map(|_| ())
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
