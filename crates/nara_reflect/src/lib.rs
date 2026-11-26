//! Component Schema metadata and value codecs for tooling.

mod asset_reference;
mod authoring;
mod codec;
#[cfg(feature = "serde")]
mod format;
mod numeric_interval;
mod path;
mod plugin;
mod registry;
pub mod schema;
mod value;
mod value_ref;

pub use asset_reference::{decode_asset_reference_value, encode_asset_reference_value};
pub use authoring::PersistentComponentProvider;
pub use codec::ComponentCodecError;
#[cfg(feature = "serde")]
pub use format::{ComponentCatalogFileError, ComponentCatalogFileLimits};
pub use nara_asset::AssetReferenceDemand;
pub use nara_reflect_derive::PersistentComponent;
pub use numeric_interval::{ComponentNumericBound, ComponentNumericInterval};
pub use path::{ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment};
pub use plugin::{
    COMPONENT_REGISTRY_PLUGIN_DECLARATION, COMPONENT_REGISTRY_PLUGIN_ID, ComponentRegistryPlugin,
    RegisterSchema, SchemaRegistrationError, component_registry,
};
#[cfg(feature = "serde")]
pub use plugin::{SchemaExportError, schema_json};
pub use registry::{ComponentRegistryError, validate_component_value};
pub use schema::{
    AliasError, CatalogFingerprint, CatalogFingerprintParseError, ComponentCapability,
    ComponentFieldEditorHint, ComponentFieldId, ComponentFieldIdError, ComponentFieldSchema,
    ComponentMemberPath, ComponentMemberPathError, ComponentMemberPathSegment,
    ComponentNumericDomain, ComponentPropertyKey, ComponentSchema, ComponentSchemaCatalog,
    ComponentSchemaVersion, ComponentShapeError, ComponentShapeErrorKind, ComponentShapeMember,
    ComponentTypeId, ComponentTypeIdError, ComponentValueKind, ComponentValueShape,
};
pub use value::{ComponentFloat, ComponentValue, ComponentValueError};
pub use value_ref::{
    ComponentListIter, ComponentListRef, ComponentMapIter, ComponentMapRef, ComponentValueRef,
};

#[doc(hidden)]
pub mod __macro_support {
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
        ComponentCapability, ComponentFieldEditorHint, ComponentFieldSchema, ComponentSchema,
        ComponentSchemaCatalog, ComponentValue, ComponentValueRef, ComponentValueShape,
        PersistentComponent, PersistentComponentProvider, RegisterSchema,
    };
}
