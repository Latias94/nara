//! Reflection and schema metadata boundary for nara data-facing components.

pub use bevy_reflect;
pub use bevy_reflect::prelude::*;

mod asset_reference;
mod authoring;
mod codec;
mod entity_reference;
#[cfg(feature = "serde")]
mod format;
mod migration;
mod path;
mod persistent_apply;
mod plugin;
mod provider;
mod registry;
mod schema;
mod value;

pub use asset_reference::{
    DeclaredAssetReference, DeclaredAssetReferenceError, collect_declared_asset_references,
};
pub use authoring::PersistentComponentProvider;
pub use codec::{
    ComponentCodec, ComponentCodecError, ComponentDecodeContext, ComponentEncodeContext,
};
pub use entity_reference::{
    ComponentEntityReferenceRewriteError, EntityReferenceTraversalLimits,
    remap_declared_entity_references, rewrite_declared_entity_references,
};
#[cfg(feature = "serde")]
pub use format::{
    ComponentCatalogFileBudgetError, ComponentCatalogFileBudgetKind, ComponentCatalogFileEncoding,
    ComponentCatalogFileError, ComponentCatalogFileLimits,
};
pub use migration::{ComponentMigrationError, MigratedComponentValue};
pub use nara_identity::EntityReference;
pub use nara_reflect_derive::PersistentComponent;
pub use path::{ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment};
pub use persistent_apply::{
    ComponentApplyBatch, ComponentApplyContext, PersistentApplyRejection, PersistentLifecycleEvent,
    PersistentObserverScope, PreparedComponent, PreparedComponentCandidate,
};
pub use plugin::{
    COMPONENT_REGISTRY_PLUGIN_ID, COMPONENT_REGISTRY_PLUGIN_REQUIREMENT,
    ComponentRegistryAuthorityError, ComponentRegistryPlugin, component_registry,
    preloaded_component_registry_plugin, register_schema_provider_for_plugin,
    registry_for_plugin_preflight, report_component_registry_authority_fault,
    validate_component_registry_authority,
};
pub use provider::{
    ComponentSchemaContributionReceipt, ComponentSchemaOwnerContributionReceipt,
    ComponentSchemaOwnerFingerprint, ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
    ComponentSchemaProviderDefinition, ComponentSchemaProviderReceipt,
    ComponentSchemaProviderSource, ComponentSchemaProviderSourceError,
    ExecutableRegistryFingerprint, SchemaCompositionFingerprint,
};
pub use registry::{
    ComponentProjectionError, ComponentRegistry, ComponentRegistryError, ComponentRegistrySnapshot,
};
pub use schema::{
    AliasError, CatalogFingerprint, CatalogFingerprintParseError, ComponentCapability,
    ComponentCatalogGenerationError, ComponentFieldId, ComponentFieldIdError, ComponentFieldSchema,
    ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId,
    ComponentTypeIdError, ComponentValueKind,
};
pub use value::{ComponentFloat, ComponentValue, ComponentValueCost, ComponentValueError};

#[doc(hidden)]
pub mod __macro_support {
    pub use crate::{
        ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
        ComponentFieldSchema, ComponentSchema, ComponentSchemaVersion, ComponentTypeId,
        ComponentValue, PersistentComponentProvider,
        authoring::{PersistentFieldCodec, decode_persistent_field, encode_persistent_field},
    };
}

#[doc(hidden)]
pub mod __private {
    pub use crate::entity_reference::{
        DeclaredEntityReferencePlan, plan_declared_entity_references,
    };
    pub use crate::persistent_apply::{
        PreparedComponentApplyBatch, declare_persistent_apply_targets,
        validate_declared_persistent_apply_targets, validate_fresh_persistent_component_apply,
        validate_persistent_apply_support_topology,
    };
    pub use crate::provider::{ComponentSchemaOwnerRecord, ResolvedComponentSchemaProvider};
    pub use crate::registry::ComponentRegistrySnapshotWitness;

    use crate::{
        ComponentRegistry, ComponentRegistryError, ComponentRegistrySnapshot,
        ComponentSchemaProviderDefinition,
    };

    pub fn resolve_schema_provider(
        provider: ComponentSchemaProviderDefinition,
    ) -> Result<ResolvedComponentSchemaProvider, ComponentRegistryError> {
        provider.resolve()
    }

    pub fn register_or_validate_resolved_schema_provider(
        registry: &mut ComponentRegistry,
        provider: ResolvedComponentSchemaProvider,
    ) -> Result<&mut ComponentRegistry, ComponentRegistryError> {
        registry.register_or_validate_resolved_schema_provider(provider)
    }

    pub fn component_registry_snapshot_witness(
        snapshot: &ComponentRegistrySnapshot,
    ) -> ComponentRegistrySnapshotWitness {
        snapshot.witness()
    }

    pub fn component_registry_snapshot_witness_matches(
        witness: &ComponentRegistrySnapshotWitness,
        snapshot: &ComponentRegistrySnapshot,
    ) -> bool {
        witness.matches(snapshot)
    }
}

pub mod prelude {
    pub use crate::{
        ComponentApplyContext, ComponentCapability, ComponentCatalogGenerationError,
        ComponentCodec, ComponentCodecError, ComponentDecodeContext, ComponentEncodeContext,
        ComponentEntityReferenceRewriteError, ComponentFieldId, ComponentFieldPath,
        ComponentFieldPathError, ComponentFieldPathSegment, ComponentFieldSchema, ComponentFloat,
        ComponentMigrationError, ComponentRegistry, ComponentRegistryError, ComponentSchema,
        ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueError, ComponentValueKind, DeclaredAssetReference,
        DeclaredAssetReferenceError, EntityReference, EntityReferenceTraversalLimits,
        MigratedComponentValue, PersistentComponent, PersistentComponentProvider,
        PreparedComponent, PreparedComponentCandidate, collect_declared_asset_references,
        remap_declared_entity_references, rewrite_declared_entity_references,
    };
    pub use bevy_reflect::prelude::*;
}

#[cfg(test)]
mod tests;
