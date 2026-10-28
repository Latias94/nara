//! Reflection and schema metadata boundary for nara data-facing components.

pub use bevy_reflect;
pub use bevy_reflect::prelude::*;

mod asset_reference;
mod authoring;
#[cfg(feature = "serde")]
mod authoring_bundle;
mod codec;
mod domain_error;
mod effective;
mod entity_reference;
#[cfg(feature = "serde")]
mod format;
mod inventory;
mod migration;
mod numeric_interval;
mod path;
mod persistent_apply;
mod plugin;
mod property;
mod provider;
mod registry;
mod schema;
mod schema_owner;
mod value;

pub(crate) use asset_reference::decode_asset_reference_value;
pub use asset_reference::{
    AssetReferenceRewriteError, DeclaredAssetReference, DeclaredAssetReferenceError,
    collect_declared_asset_references, collect_declared_asset_references_from_schema,
    collect_declared_asset_references_from_schema_with_limits,
    collect_declared_asset_references_with_limits, collect_declared_field_asset_references,
    collect_declared_field_asset_references_with_limits,
    collect_declared_value_asset_references_with_limits, rewrite_declared_asset_references,
    rewrite_declared_asset_references_with_admission,
};
pub use authoring::PersistentComponentProvider;
#[cfg(feature = "serde")]
pub use authoring_bundle::{
    AuthoringSchemaBundle, AuthoringSchemaBundleError, AuthoringSchemaBundleLimits,
    AuthoringSchemaBundleOwner,
};
pub use codec::{ComponentCodecError, ComponentDecodeContext, ComponentEncodeContext};
pub use domain_error::ComponentDomainError;
pub use effective::{
    ComponentValueMaterializationLimits, effective_component_value, effective_component_value_owned,
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
pub use numeric_interval::{ComponentNumericBound, ComponentNumericInterval};
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
pub use property::{
    ComponentPropertyError, ComponentPropertyKey, ComponentPropertyPath,
    ComponentPropertyReadBudget, ComponentPropertySchemaClaim, ComponentPropertyStep,
    ResolvedComponentProperty, replace_component_property, resolve_component_property,
    resolve_component_property_in_root, resolve_component_property_schema_claim,
    resolve_component_property_with_limits,
};
pub use provider::{
    ComponentSchemaContributionReceipt, ComponentSchemaOwnerContributionReceipt,
    ComponentSchemaOwnerFingerprint, ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
    ComponentSchemaProviderDefinition, ComponentSchemaProviderReceipt,
    ComponentSchemaProviderSource, ComponentSchemaProviderSourceError,
    ExecutableRegistryFingerprint, PersistentComponentSchemaOwnerId,
    PersistentComponentSchemaOwnerIdError, SchemaCompositionFingerprint,
};
pub use registry::{
    ComponentProjectionError, ComponentRegistry, ComponentRegistryError, ComponentRegistrySnapshot,
    validate_component_value, validate_component_value_with_limits,
};
pub use schema::{
    AliasError, CatalogFingerprint, CatalogFingerprintParseError, ComponentCapability,
    ComponentCatalogGenerationError, ComponentFieldId, ComponentFieldIdError, ComponentFieldSchema,
    ComponentMemberPath, ComponentMemberPathError, ComponentMemberPathSegment,
    ComponentNumericDomain, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
    ComponentShapeError, ComponentShapeErrorKind, ComponentShapeMember, ComponentTypeId,
    ComponentTypeIdError, ComponentValueKind, ComponentValueShape,
};
pub use schema_owner::{ComponentSchemaOwnerChange, ComponentSchemaOwnerSnapshot};
pub use value::{ComponentFloat, ComponentValue, ComponentValueCost, ComponentValueError};

#[doc(hidden)]
pub mod __macro_support {
    pub use crate::inventory::validate_inventory;
    pub use crate::{
        ComponentCapability, ComponentCodecError, ComponentFieldId, ComponentFieldPath,
        ComponentFieldSchema, ComponentNumericBound, ComponentSchema, ComponentSchemaVersion,
        ComponentTypeId, ComponentValue, ComponentValueShape, PersistentComponentProvider,
        authoring::{
            PersistentFieldCodec, PersistentNumericField, decode_persistent_field,
            encode_persistent_field,
        },
    };
}

#[doc(hidden)]
pub mod __private {
    pub use crate::asset_reference::{
        AssetReferenceCollectionError, collect_declared_effective_asset_references_with_admission,
        collect_declared_field_asset_references_with_admission,
        collect_declared_value_asset_references_with_admission,
    };
    pub use crate::effective::{
        EffectiveValueAdmissionError, effective_component_value_with_admission,
        effective_component_value_with_budget,
    };
    pub use crate::entity_reference::{
        DeclaredEntityReferencePlan, EntityReferencePlanError, plan_declared_entity_references,
        plan_declared_entity_references_with_admission, plan_declared_value_entity_references,
    };
    pub use crate::persistent_apply::{
        PreparedComponentApplyBatch, UnboundComponentApplyBatch, declare_persistent_apply_targets,
        validate_declared_persistent_apply_targets, validate_fresh_persistent_component_apply,
        validate_persistent_apply_support_topology, validate_persistent_export_coverage,
        validate_registered_persistent_apply_targets,
    };
    pub use crate::property::{
        ComponentPropertyAdmissionError, resolve_component_property_schema_claim_with_admission,
    };
    pub use crate::provider::{
        ComponentSchemaOwnerRecord, ResolvedComponentSchemaProvider,
        scene_component_composition_fingerprint,
    };
    pub use crate::registry::{
        ComponentRegistrySnapshotWitness, ComponentValueAdmissionError,
        validate_component_value_with_admission, validate_schema_capabilities,
    };

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

    pub fn component_snapshot_matches_runtime_type(
        snapshot: &ComponentRegistrySnapshot,
        world: &nara_ecs::World,
        component: &crate::ComponentTypeId,
        runtime: nara_ecs::bevy_ecs::component::ComponentId,
    ) -> bool {
        crate::registry::component_snapshot_matches_runtime_type(
            snapshot, world, component, runtime,
        )
    }
}

pub mod prelude {
    pub use crate::{
        ComponentApplyContext, ComponentCapability, ComponentCatalogGenerationError,
        ComponentCodecError, ComponentDecodeContext, ComponentDomainError, ComponentEncodeContext,
        ComponentEntityReferenceRewriteError, ComponentFieldId, ComponentFieldPath,
        ComponentFieldPathError, ComponentFieldPathSegment, ComponentFieldSchema, ComponentFloat,
        ComponentMemberPath, ComponentMemberPathError, ComponentMemberPathSegment,
        ComponentMigrationError, ComponentNumericBound, ComponentNumericDomain,
        ComponentNumericInterval, ComponentPropertyError, ComponentPropertyKey,
        ComponentPropertyPath, ComponentPropertyReadBudget, ComponentPropertySchemaClaim,
        ComponentPropertyStep, ComponentRegistry, ComponentRegistryError,
        ComponentRegistrySnapshot, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentShapeError,
        ComponentShapeErrorKind, ComponentShapeMember, ComponentTypeId, ComponentValue,
        ComponentValueError, ComponentValueKind, ComponentValueShape, DeclaredAssetReference,
        DeclaredAssetReferenceError, EntityReference, EntityReferenceTraversalLimits,
        MigratedComponentValue, PersistentComponent, PersistentComponentProvider,
        PreparedComponent, PreparedComponentCandidate, ResolvedComponentProperty,
        collect_declared_asset_references, collect_declared_value_asset_references_with_limits,
        remap_declared_entity_references, replace_component_property, resolve_component_property,
        resolve_component_property_in_root, resolve_component_property_schema_claim,
        resolve_component_property_with_limits, rewrite_declared_entity_references,
        validate_component_value,
    };
    pub use bevy_reflect::prelude::*;
}

#[cfg(test)]
mod tests;
