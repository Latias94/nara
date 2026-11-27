//! Component Schema metadata, dynamic field access, and typed codecs.
//!
//! Derive [`Schema`] alongside `Component`. A component supplies a stable `id` and a nonzero
//! `version`. Fields use their Rust names unless given an explicit `id`. Exported fields retain
//! declaration order.
//!
//! ```
//! use nara_core::Vec2;
//! use nara_ecs::Component;
//! use nara_reflect::{ComponentValue, FieldPath, Schema};
//!
//! #[derive(Component, Default, Schema)]
//! #[schema(id = "game.Motion", version = 1, group = "Gameplay", default)]
//! struct Motion {
//!     direction: Vec2,
//!     #[schema(range = 0.0..=400.0, step = 0.5, unit = "px/s", default = 160.0)]
//!     speed: f32,
//!     #[schema(skip)]
//!     elapsed: f32,
//! }
//!
//! let schema = Motion::type_schema()?;
//! let value = schema.with_defaults(ComponentValue::Map(Default::default()))?;
//! let mut motion = Motion::decode_component((&value).into())?;
//! let speed: FieldPath = "speed".parse()?;
//! motion.set_field(&speed, ComponentValue::f64(200.0)?)?;
//! assert_eq!(motion.speed, 200.0);
//! assert_eq!(motion.elapsed, 0.0);
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Component-level `default` uses the component's `Default` for omitted fields. Field-level
//! `default` uses the field type's `Default`, and `default = expression` supplies a typed override.
//! Without an explicit default, `Option<T>` defaults to `None`. `skip` omits a field from all
//! codecs and initializes it from a field default when constructing a component. Dynamic edits
//! preserve the existing skipped field. `read_only` keeps a field in the codecs and blocks
//! dynamic writes. `range` and `choices` validate authored values. `step`, `unit`, `tooltip`, and
//! `color_srgb` provide editor metadata. Color channels remain linear.
//!
//! Use the [`PackedU16`], [`PackedU32`], [`PackedI32`], [`PackedF32`], and [`Bytes`] field types for
//! dense data. Ordinary `Vec<T>` keeps its list schema. Core `AssetId` and `EntityUuid` fields
//! carry stable UUIDs. With the `postcard` feature, derived components serialize typed fields in
//! declaration order and decode directly into Rust values. The enclosing format owns component
//! framing and schema hash checks.

mod asset_reference;
mod authoring;
#[cfg(feature = "postcard")]
mod binary;
mod codec;
#[cfg(feature = "serde")]
mod format;
mod numeric_interval;
mod packed;
mod path;
mod plugin;
mod registry;
pub mod schema;
mod value;
mod value_ref;

pub use asset_reference::{decode_asset_reference_value, encode_asset_reference_value};
pub use authoring::Schema;
#[cfg(feature = "postcard")]
pub use binary::PostcardReference;
pub use codec::ComponentCodecError;
#[cfg(feature = "serde")]
pub use format::{ComponentCatalogFileError, ComponentCatalogFileLimits};
pub use nara_asset::AssetReferenceDemand;
pub use nara_reflect_derive::Schema;
pub use numeric_interval::{ComponentNumericBound, ComponentNumericInterval};
pub use packed::{Bytes, PackedF32, PackedI32, PackedU16, PackedU32};
/// A path relative to a component's stable ID.
pub use path::ComponentFieldPath as FieldPath;
pub use path::{
    ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathParseError,
    ComponentFieldPathSegment,
};
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
    ComponentTypeId, ComponentTypeIdError, ComponentValueKind, ComponentValueShape, SchemaHash,
    TypeSchema,
};
pub use value::{ComponentFloat, ComponentValue, ComponentValueError};
pub use value_ref::{
    ComponentListIter, ComponentListRef, ComponentMapIter, ComponentMapRef, ComponentValueRef,
};

#[doc(hidden)]
pub mod __macro_support {
    pub use crate::__schema_postcard_methods;
    #[cfg(feature = "postcard")]
    pub use crate::binary::{
        PostcardReference, check_postcard_choice, finish_postcard, read_postcard_field,
        visit_postcard_field_references, write_postcard_field,
    };
    pub use crate::{
        AssetReferenceDemand, ComponentCapability, ComponentCodecError, ComponentFieldEditorHint,
        ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentFloat,
        ComponentNumericBound, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
        ComponentValueRef, ComponentValueShape, Schema, TypeSchema,
        authoring::{
            PersistentFieldCodec, PersistentNumericField, decode_persistent_field,
            encode_persistent_field, split_field_path, update_schema_field,
        },
        validate_component_value,
    };
}

#[cfg(feature = "postcard")]
#[doc(hidden)]
#[macro_export]
macro_rules! __schema_postcard_methods {
    ($($items:item)*) => { $($items)* };
}

#[cfg(not(feature = "postcard"))]
#[doc(hidden)]
#[macro_export]
macro_rules! __schema_postcard_methods {
    ($($items:item)*) => {};
}

pub mod prelude {
    pub use crate::{
        ComponentCapability, ComponentFieldEditorHint, ComponentFieldSchema, ComponentSchema,
        ComponentSchemaCatalog, ComponentValue, ComponentValueRef, ComponentValueShape, FieldPath,
        RegisterSchema, Schema, TypeSchema,
    };
}
