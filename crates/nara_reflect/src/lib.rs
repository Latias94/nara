//! Reflection and schema metadata boundary for nara data-facing components.

pub use bevy_reflect;
pub use bevy_reflect::prelude::*;

mod authoring;
mod codec;
mod entity_reference;
#[cfg(feature = "serde")]
mod format;
mod migration;
mod path;
mod plugin;
mod registry;
mod schema;
mod value;

pub use authoring::PersistentComponentProvider;
pub use codec::{
    ComponentApplyBatch, ComponentApplyContext, ComponentCodec, ComponentCodecError,
    ComponentDecodeContext, ComponentEncodeContext, PreparedComponent,
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
pub use plugin::{
    COMPONENT_REGISTRY_PLUGIN_ID, COMPONENT_REGISTRY_PLUGIN_REQUIREMENT, ComponentRegistryPlugin,
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

pub mod prelude {
    pub use crate::{
        ComponentApplyContext, ComponentCapability, ComponentCatalogGenerationError,
        ComponentCodec, ComponentCodecError, ComponentDecodeContext, ComponentEncodeContext,
        ComponentEntityReferenceRewriteError, ComponentFieldId, ComponentFieldPath,
        ComponentFieldPathError, ComponentFieldPathSegment, ComponentFieldSchema, ComponentFloat,
        ComponentMigrationError, ComponentRegistry, ComponentRegistryError, ComponentSchema,
        ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueError, ComponentValueKind, EntityReference, EntityReferenceTraversalLimits,
        MigratedComponentValue, PersistentComponent, PersistentComponentProvider,
        PreparedComponent, remap_declared_entity_references, rewrite_declared_entity_references,
    };
    pub use bevy_reflect::prelude::*;
}

#[cfg(test)]
mod tests;
