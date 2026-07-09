//! Reflection and schema metadata boundary for nara data-facing components.

pub use bevy_reflect;
pub use bevy_reflect::prelude::*;

mod codec;
mod migration;
mod path;
mod registry;
mod schema;
mod value;

pub use codec::{
    ComponentCodec, ComponentCodecError, ComponentDecodeContext, ComponentEncodeContext,
    PreparedComponent,
};
pub use migration::{ComponentMigrationError, MigratedComponentValue};
pub use path::{ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment};
pub use registry::{ComponentRegistry, ComponentRegistryError};
pub use schema::{
    ComponentCapability, ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentTypeId, ComponentValueKind,
};
pub use value::{ComponentFloat, ComponentValue, ComponentValueError};

pub mod prelude {
    pub use crate::{
        ComponentCapability, ComponentCodec, ComponentCodecError, ComponentDecodeContext,
        ComponentEncodeContext, ComponentFieldPath, ComponentFieldPathError,
        ComponentFieldPathSegment, ComponentFieldSchema, ComponentFloat, ComponentMigrationError,
        ComponentRegistry, ComponentRegistryError, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueError,
        ComponentValueKind, MigratedComponentValue, PreparedComponent,
    };
    pub use bevy_reflect::prelude::*;
}

#[cfg(test)]
mod tests;
