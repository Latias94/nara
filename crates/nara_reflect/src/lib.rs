//! Reflection and schema metadata boundary for nara data-facing components.
//!
//! Nara's component registry owns semantic Schemas, native bindings and codecs.

pub mod asset_reference;
mod authoring;
#[cfg(feature = "serde")]
mod authoring_bundle;
mod codec;
mod domain_error;
pub mod effective;
pub mod entity_reference;
#[cfg(feature = "serde")]
mod format;
mod inventory;
mod migration;
mod numeric_interval;
mod path;
pub mod persistent_apply;
mod plugin;
pub mod property;
pub mod provider;
pub mod registry;
pub mod schema;
mod schema_owner;
mod value;
mod value_ref;

pub use value_ref::{
    ComponentListIter, ComponentListRef, ComponentMapIter, ComponentMapRef, ComponentValueRef,
};

pub use asset_reference::{
    AssetReferenceRewriteError, DeclaredAssetReference, DeclaredAssetReferenceError,
    asset_reference_decode_work_bytes, collect_declared_asset_references,
    collect_declared_asset_references_from_schema,
    collect_declared_asset_references_from_schema_with_limits,
    collect_declared_asset_references_with_limits, collect_declared_field_asset_references,
    collect_declared_field_asset_references_with_limits,
    collect_declared_value_asset_references_with_limits, decode_asset_reference_value,
    encode_asset_reference_value, rewrite_declared_asset_references,
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
pub use nara_asset::AssetReferenceDemand;
pub use nara_ecs::identity::EntityReference;
pub use nara_reflect_derive::PersistentComponent;
pub use numeric_interval::{ComponentNumericBound, ComponentNumericInterval};
pub use path::{ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment};
pub use persistent_apply::{
    ComponentApplyBatch, ComponentApplyContext, PersistentApplyRejection, PersistentLifecycleEvent,
    PersistentObserverScope, PreparedComponent, PreparedComponentCandidate,
};
pub use plugin::{
    COMPONENT_REGISTRY_PLUGIN_ID, COMPONENT_REGISTRY_PLUGIN_REQUIREMENT,
    ComponentRegistryAuthorityError, ComponentRegistryPlugin, PreloadedComponentRegistryPlugin,
    RegisterSchema, component_registry, component_registry_is_installed,
    preloaded_component_registry_plugin, register_schema_provider_for_plugin,
    report_component_registry_authority_fault, validate_component_registry_authority,
    validate_schema_provider_for_plugin,
};
#[cfg(feature = "serde")]
pub use plugin::{SchemaExportError, schema_json};
pub use property::{
    ComponentPropertyError, ComponentPropertyKey, ComponentPropertyPath,
    ComponentPropertyReadBudget, ComponentPropertySchemaClaim, ComponentPropertyStep,
    ResolvedComponentProperty, replace_component_property, resolve_component_property,
    resolve_component_property_in_root, resolve_component_property_schema_claim,
    resolve_component_property_schema_in_root, resolve_component_property_with_limits,
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
    ComponentProjectionError, ComponentRegistryBuilder, ComponentRegistryError,
    ComponentRegistryFreezeError, ComponentRegistrySnapshot, validate_component_value,
    validate_component_value_with_limits,
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
        AssetReferenceDemand, ComponentCapability, ComponentCodecError, ComponentFieldId,
        ComponentFieldPath, ComponentFieldSchema, ComponentNumericBound, ComponentSchema,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueRef,
        ComponentValueShape, PersistentComponentProvider,
        authoring::{
            PersistentFieldCodec, PersistentNumericField, decode_persistent_field,
            encode_persistent_field,
        },
    };
}

pub mod prelude {
    pub use crate::{
        AssetReferenceDemand, ComponentApplyContext, ComponentCapability,
        ComponentCatalogGenerationError, ComponentCodecError, ComponentDecodeContext,
        ComponentDomainError, ComponentEncodeContext, ComponentEntityReferenceRewriteError,
        ComponentFieldId, ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment,
        ComponentFieldSchema, ComponentFloat, ComponentMemberPath, ComponentMemberPathError,
        ComponentMemberPathSegment, ComponentMigrationError, ComponentNumericBound,
        ComponentNumericDomain, ComponentNumericInterval, ComponentPropertyError,
        ComponentPropertyKey, ComponentPropertyPath, ComponentPropertyReadBudget,
        ComponentPropertySchemaClaim, ComponentPropertyStep, ComponentRegistryBuilder,
        ComponentRegistryError, ComponentRegistryFreezeError, ComponentRegistrySnapshot,
        ComponentSchema, ComponentSchemaCatalog, ComponentSchemaOwnerSnapshot,
        ComponentSchemaVersion, ComponentShapeError, ComponentShapeErrorKind, ComponentShapeMember,
        ComponentTypeId, ComponentValue, ComponentValueError, ComponentValueKind,
        ComponentValueRef, ComponentValueShape, DeclaredAssetReference,
        DeclaredAssetReferenceError, EntityReference, EntityReferenceTraversalLimits,
        MigratedComponentValue, PersistentComponent, PersistentComponentProvider,
        PreparedComponent, PreparedComponentCandidate, ResolvedComponentProperty,
        collect_declared_asset_references, collect_declared_value_asset_references_with_limits,
        remap_declared_entity_references, replace_component_property, resolve_component_property,
        resolve_component_property_in_root, resolve_component_property_schema_claim,
        resolve_component_property_with_limits, rewrite_declared_entity_references,
        validate_component_value,
    };
}

#[cfg(test)]
mod inline_reference_tests;

#[cfg(test)]
mod tests;
