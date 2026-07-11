//! Reflection and schema metadata boundary for nara data-facing components.

pub use bevy_reflect;
pub use bevy_reflect::prelude::*;

mod codec;
mod entity_reference;
mod migration;
mod path;
mod registry;
mod schema;
mod value;

pub use codec::{
    ComponentApplyBatch, ComponentApplyContext, ComponentCodec, ComponentCodecError,
    ComponentDecodeContext, ComponentEncodeContext, PreparedComponent,
};
pub use entity_reference::{
    ComponentEntityReferenceRewriteError, EntityReferenceTraversalLimits,
    remap_declared_entity_references, rewrite_declared_entity_references,
};
pub use migration::{ComponentMigrationError, MigratedComponentValue};
pub use nara_identity::EntityReference;
pub use path::{ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment};
pub use registry::{ComponentRegistry, ComponentRegistryError};
pub use schema::{
    ComponentCapability, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValueKind,
};
pub use value::{ComponentFloat, ComponentValue, ComponentValueError};

pub mod prelude {
    pub use crate::{
        ComponentApplyContext, ComponentCapability, ComponentCodec, ComponentCodecError,
        ComponentDecodeContext, ComponentEncodeContext, ComponentEntityReferenceRewriteError,
        ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment,
        ComponentFieldSchema, ComponentFloat, ComponentMigrationError, ComponentRegistry,
        ComponentRegistryError, ComponentSchema, ComponentSchemaCatalog, ComponentSchemaVersion,
        ComponentTypeId, ComponentValue, ComponentValueError, ComponentValueKind, EntityReference,
        EntityReferenceTraversalLimits, MigratedComponentValue, PreparedComponent,
        remap_declared_entity_references, rewrite_declared_entity_references,
    };
    pub use bevy_reflect::prelude::*;
}

#[cfg(test)]
mod tests;
