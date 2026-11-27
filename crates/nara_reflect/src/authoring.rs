//! Native component and field codecs.

use std::collections::BTreeMap;

use nara_asset::AssetRef;
use nara_core::{AssetId, Color, EntityUuid, Vec2, Vec3, Vec4};
use nara_ecs::Component;

use crate::{
    AssetReferenceDemand, Bytes, ComponentCodecError, ComponentFieldEditorHint, ComponentFieldPath,
    ComponentFieldPathError, ComponentFieldSchema, ComponentNumericDomain, ComponentValue,
    ComponentValueError, ComponentValueKind, ComponentValueRef, ComponentValueShape, PackedF32,
    PackedI32, PackedU16, PackedU32, TypeSchema, validate_component_value,
};

pub trait Schema: Component + Sized + 'static {
    /// The stable schema identifier also used to register this type with an app.
    const COMPONENT_ID: &'static str;

    /// Returns the component's metadata. Handwritten native Postcard codecs declare support
    /// with [`TypeSchema::with_postcard_codec`] in every build.
    fn type_schema() -> Result<TypeSchema, ComponentCodecError>;

    /// Decodes the current component's stored fields without applying them to a world.
    /// Resolve the record's schema version before calling this codec.
    fn decode_component(value: ComponentValueRef<'_>) -> Result<Self, ComponentCodecError>;

    /// Encodes the current component's authored fields.
    fn encode_component(&self) -> Result<ComponentValue, ComponentCodecError>;

    /// Reads a field by its path relative to this component.
    fn get_field(&self, path: &ComponentFieldPath) -> Result<ComponentValue, ComponentCodecError> {
        Ok(self.encode_component()?.read_path(path)?)
    }

    /// Replaces an editable field by its path relative to this component.
    fn set_field(
        &mut self,
        path: &ComponentFieldPath,
        value: ComponentValue,
    ) -> Result<(), ComponentCodecError> {
        let schema = Self::type_schema()?;
        let field = schema
            .fields()
            .iter()
            .find(|field| path.segments().starts_with(field.path().segments()))
            .ok_or_else(|| ComponentCodecError::missing_field(path.to_string()))?;
        ensure_editable(field, path)?;
        let mut encoded = self.encode_component()?;
        encoded.replace_path(path, value)?;
        validate_component_value(&schema, &encoded)?;
        *self = Self::decode_component((&encoded).into())?;
        Ok(())
    }

    /// Encodes one complete component payload in declaration order, without field names.
    #[cfg(feature = "postcard")]
    fn encode_postcard(&self) -> Result<Vec<u8>, ComponentCodecError> {
        Err(ComponentCodecError::PostcardUnavailable)
    }

    /// Decodes exactly one complete component payload. The enclosing format checks the schema hash.
    #[cfg(feature = "postcard")]
    fn decode_postcard(_bytes: &[u8]) -> Result<Self, ComponentCodecError> {
        Err(ComponentCodecError::PostcardUnavailable)
    }

    /// Visits the stable references in a decoded component without building an authored value.
    /// A handwritten Postcard codec with reference fields must implement this method.
    #[cfg(feature = "postcard")]
    fn visit_postcard_references(
        &self,
        _visit: &mut dyn FnMut(crate::PostcardReference),
    ) -> Result<(), ComponentCodecError> {
        Err(ComponentCodecError::PostcardUnavailable)
    }
}

#[doc(hidden)]
pub trait PersistentFieldCodec: private::Sealed + Sized + 'static {
    /// Whether this codec's actual recursive shape declares any asset reference leaves.
    const HAS_ASSET_REFERENCES: bool;
    const EDITOR_HINT: Option<ComponentFieldEditorHint> = None;
    const IS_OPTION: bool = false;
    const HAS_NESTED_OPTION: bool = false;

    fn default_value() -> Option<ComponentValue> {
        None
    }

    fn value_shape() -> ComponentValueShape;

    fn decode(value: ComponentValueRef<'_>, field: &str) -> Result<Self, ComponentCodecError>;

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError>;
}

/// Numeric primitives supported by the derive's scalar range literals.
#[doc(hidden)]
pub trait PersistentNumericField: PersistentFieldCodec + Copy {
    fn numeric_interval(
        min: Option<crate::ComponentNumericBound<Self>>,
        max: Option<crate::ComponentNumericBound<Self>>,
    ) -> Result<crate::ComponentNumericInterval, crate::ComponentValueError>;
}

fn map_bound<T, U>(
    bound: Option<crate::ComponentNumericBound<T>>,
    convert: impl FnOnce(T) -> Result<U, crate::ComponentValueError>,
) -> Result<Option<crate::ComponentNumericBound<U>>, crate::ComponentValueError> {
    use crate::ComponentNumericBound::{Excluded, Included};
    match bound {
        Some(Included(value)) => convert(value).map(Included).map(Some),
        Some(Excluded(value)) => convert(value).map(Excluded).map(Some),
        None => Ok(None),
    }
}

macro_rules! numeric_interval_codec {
    ($variant:ident, $convert:expr; $($ty:ty),+ $(,)?) => {$(
        impl PersistentNumericField for $ty {
            fn numeric_interval(
                min: Option<crate::ComponentNumericBound<Self>>,
                max: Option<crate::ComponentNumericBound<Self>>,
            ) -> Result<crate::ComponentNumericInterval, crate::ComponentValueError> {
                Ok(crate::ComponentNumericInterval::$variant {
                    min: map_bound(min, $convert)?,
                    max: map_bound(max, $convert)?,
                })
            }
        }
    )+};
}

numeric_interval_codec!(I64, |value| Ok(i64::from(value)); i8, i16, i32, i64);
numeric_interval_codec!(U64, |value| Ok(u64::from(value)); u8, u16, u32, u64);
numeric_interval_codec!(F64, |value| crate::ComponentFloat::new(f64::from(value)); f32, f64);

#[doc(hidden)]
pub fn decode_persistent_field<T>(
    value: ComponentValueRef<'_>,
    field: &ComponentFieldSchema,
) -> Result<T, ComponentCodecError>
where
    T: PersistentFieldCodec,
{
    let field_id = field.id().as_str();
    check_optional_nesting::<T>()?;
    if let Some(value) = value.get(field_id) {
        T::decode(value, field_id)
    } else {
        let default = field
            .default_value()
            .ok_or_else(|| ComponentCodecError::missing_field(field_id))?;
        field
            .validate_value(default)
            .map_err(|error| ComponentCodecError::invalid_field(field_id, error.to_string()))?;
        T::decode(default.into(), field_id)
    }
}

#[doc(hidden)]
pub fn encode_persistent_field<T>(value: &T) -> Result<ComponentValue, ComponentCodecError>
where
    T: PersistentFieldCodec,
{
    check_optional_nesting::<T>()?;
    value.encode()
}

fn check_optional_nesting<T: PersistentFieldCodec>() -> Result<(), ComponentCodecError> {
    if T::HAS_NESTED_OPTION {
        return Err(ComponentCodecError::Message(
            "nested Option is not supported in reflected fields".into(),
        ));
    }
    Ok(())
}

#[doc(hidden)]
pub fn update_schema_field<T: PersistentFieldCodec>(
    native: &mut T,
    path: &ComponentFieldPath,
    value: ComponentValue,
    field: &ComponentFieldSchema,
) -> Result<(), ComponentCodecError> {
    ensure_editable(field, field.path())?;
    check_optional_nesting::<T>()?;
    let value = if path.is_empty() {
        value
    } else {
        let mut current = native.encode()?;
        current.replace_path(path, value)?;
        current
    };
    let field_id = field.id().as_str();
    field
        .validate_value(&value)
        .map_err(|error| ComponentCodecError::invalid_field(field_id, error.to_string()))?;
    *native = T::decode((&value).into(), field_id)?;
    Ok(())
}

fn ensure_editable(
    field: &ComponentFieldSchema,
    path: &ComponentFieldPath,
) -> Result<(), ComponentCodecError> {
    if field.is_read_only() {
        let field = if path.is_empty() { field.path() } else { path };
        return Err(ComponentCodecError::ReadOnlyField {
            field: field.to_string(),
        });
    }
    Ok(())
}

#[doc(hidden)]
pub fn split_field_path(
    path: &ComponentFieldPath,
) -> Result<(&str, ComponentFieldPath), ComponentCodecError> {
    use crate::ComponentFieldPathSegment;
    match path.segments().split_first() {
        Some((ComponentFieldPathSegment::Field(id), rest)) => {
            Ok((id, ComponentFieldPath::new(rest.iter().cloned())))
        }
        _ => Err(ComponentFieldPathError::EmptyPath.into()),
    }
}

impl PersistentFieldCodec for bool {
    const HAS_ASSET_REFERENCES: bool = false;

    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::scalar(ComponentValueKind::Bool)
    }

    fn decode(value: ComponentValueRef<'_>, field: &str) -> Result<Self, ComponentCodecError> {
        value
            .as_bool()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "bool"))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::Bool(*self))
    }
}

impl PersistentFieldCodec for String {
    const HAS_ASSET_REFERENCES: bool = false;

    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::scalar(ComponentValueKind::String)
    }

    fn decode(value: ComponentValueRef<'_>, field: &str) -> Result<Self, ComponentCodecError> {
        value
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "string"))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::String(self.clone()))
    }
}

macro_rules! narrow_integer_codec {
    ($ty:ty, $kind:ident, $getter:ident, $wire:ty, $domain:ident) => {
        impl PersistentFieldCodec for $ty {
            const HAS_ASSET_REFERENCES: bool = false;

            fn value_shape() -> ComponentValueShape {
                ComponentValueShape::scalar(ComponentValueKind::$kind)
                    .with_numeric_domain(ComponentNumericDomain::$domain)
            }

            fn decode(
                value: ComponentValueRef<'_>,
                field: &str,
            ) -> Result<Self, ComponentCodecError> {
                value
                    .$getter()
                    .and_then(|value| Self::try_from(value).ok())
                    .ok_or_else(|| ComponentCodecError::invalid_field(field, stringify!($ty)))
            }

            fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
                Ok(ComponentValue::$kind(<$wire>::from(*self)))
            }
        }
    };
}

narrow_integer_codec!(i8, I64, as_i64, i64, I8);
narrow_integer_codec!(i16, I64, as_i64, i64, I16);
narrow_integer_codec!(i32, I64, as_i64, i64, I32);
narrow_integer_codec!(u8, U64, as_u64, u64, U8);
narrow_integer_codec!(u16, U64, as_u64, u64, U16);
narrow_integer_codec!(u32, U64, as_u64, u64, U32);

impl PersistentFieldCodec for f32 {
    const HAS_ASSET_REFERENCES: bool = false;

    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::scalar(ComponentValueKind::F64)
            .with_numeric_domain(ComponentNumericDomain::F32)
    }

    fn decode(value: ComponentValueRef<'_>, field: &str) -> Result<Self, ComponentCodecError> {
        value
            .as_f64()
            .filter(|number| (f64::from(f32::MIN)..=f64::from(f32::MAX)).contains(number))
            .map(|number| number as f32)
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "finite f32"))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::f64(f64::from(*self))?)
    }
}

impl PersistentFieldCodec for f64 {
    const HAS_ASSET_REFERENCES: bool = false;

    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::scalar(ComponentValueKind::F64)
    }

    fn decode(value: ComponentValueRef<'_>, field: &str) -> Result<Self, ComponentCodecError> {
        value
            .as_f64()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "finite f64"))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::f64(*self)?)
    }
}

impl PersistentFieldCodec for i64 {
    const HAS_ASSET_REFERENCES: bool = false;

    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::scalar(ComponentValueKind::I64)
    }

    fn decode(value: ComponentValueRef<'_>, field: &str) -> Result<Self, ComponentCodecError> {
        value
            .as_i64()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "i64"))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::I64(*self))
    }
}

impl PersistentFieldCodec for u64 {
    const HAS_ASSET_REFERENCES: bool = false;

    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::scalar(ComponentValueKind::U64)
    }

    fn decode(value: ComponentValueRef<'_>, field: &str) -> Result<Self, ComponentCodecError> {
        value
            .as_u64()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "u64"))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::U64(*self))
    }
}

macro_rules! vector_codec {
    ($native:ty, $($axis:ident),+) => {
        impl PersistentFieldCodec for $native {
            const HAS_ASSET_REFERENCES: bool = false;

            fn value_shape() -> ComponentValueShape {
                ComponentValueShape::record([
                    $((stringify!($axis).to_owned(), f32::value_shape())),+
                ])
            }

            fn decode(value: ComponentValueRef<'_>, field: &str) -> Result<Self, ComponentCodecError> {
                let axes = [$(stringify!($axis)),+];
                if !value.as_map().is_some_and(|members| members.len() == axes.len()) {
                    return Err(ComponentCodecError::invalid_field(field, "a closed vector record"));
                }
                Ok(Self::new($(decode_f32_component(value, field, stringify!($axis))?),+))
            }

            fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
                Ok(ComponentValue::map([$((stringify!($axis), self.$axis.encode()?)),+]))
            }
        }
    };
}

vector_codec!(Vec2, x, y);
vector_codec!(Vec3, x, y, z);
vector_codec!(Vec4, x, y, z, w);

impl PersistentFieldCodec for Color {
    const HAS_ASSET_REFERENCES: bool = false;
    const EDITOR_HINT: Option<ComponentFieldEditorHint> =
        Some(ComponentFieldEditorHint::LinearRgba);

    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::record(
            ["r", "g", "b", "a"].map(|channel| (channel.to_owned(), f32::value_shape())),
        )
    }

    fn decode(value: ComponentValueRef<'_>, field: &str) -> Result<Self, ComponentCodecError> {
        if !value.as_map().is_some_and(|members| members.len() == 4) {
            return Err(ComponentCodecError::invalid_field(
                field,
                "a closed r/g/b/a record of finite linear f32 values",
            ));
        }
        Ok(Self::rgba(
            decode_f32_component(value, field, "r")?,
            decode_f32_component(value, field, "g")?,
            decode_f32_component(value, field, "b")?,
            decode_f32_component(value, field, "a")?,
        ))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::map([
            ("r", self.r.encode()?),
            ("g", self.g.encode()?),
            ("b", self.b.encode()?),
            ("a", self.a.encode()?),
        ]))
    }
}

impl PersistentFieldCodec for EntityUuid {
    const HAS_ASSET_REFERENCES: bool = false;

    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::EntityRef
    }

    fn decode(value: ComponentValueRef<'_>, field: &str) -> Result<Self, ComponentCodecError> {
        value
            .as_entity_reference()
            .copied()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "entity reference"))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::EntityReference(*self))
    }
}

impl PersistentFieldCodec for AssetId {
    const HAS_ASSET_REFERENCES: bool = true;

    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::stable_asset_ref(AssetReferenceDemand::Immediate)
    }

    fn decode(value: ComponentValueRef<'_>, field: &str) -> Result<Self, ComponentCodecError> {
        value
            .as_asset_reference()
            .copied()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "stable asset UUID"))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::AssetReference(*self))
    }
}

impl PersistentFieldCodec for AssetRef {
    const HAS_ASSET_REFERENCES: bool = true;

    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::asset_ref(AssetReferenceDemand::Immediate)
    }

    fn decode(value: ComponentValueRef<'_>, field: &str) -> Result<Self, ComponentCodecError> {
        crate::decode_asset_reference_value(value)
            .map_err(|_| ComponentCodecError::invalid_field(field, "canonical asset reference"))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        crate::encode_asset_reference_value(self)
    }
}

impl<T: PersistentFieldCodec> PersistentFieldCodec for Vec<T> {
    const HAS_ASSET_REFERENCES: bool = T::HAS_ASSET_REFERENCES;
    const HAS_NESTED_OPTION: bool = T::HAS_NESTED_OPTION;

    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::list(T::value_shape())
    }

    fn decode(value: ComponentValueRef<'_>, field: &str) -> Result<Self, ComponentCodecError> {
        let Some(items) = value.as_list() else {
            return Err(ComponentCodecError::invalid_field(field, "list"));
        };
        items
            .iter()
            .enumerate()
            .map(|(index, item)| T::decode(item, &format!("{field}[{index}]")))
            .collect()
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        self.iter()
            .map(PersistentFieldCodec::encode)
            .collect::<Result<Vec<_>, _>>()
            .map(ComponentValue::List)
    }
}

impl<T: PersistentFieldCodec> PersistentFieldCodec for BTreeMap<String, T> {
    const HAS_ASSET_REFERENCES: bool = T::HAS_ASSET_REFERENCES;
    const HAS_NESTED_OPTION: bool = T::HAS_NESTED_OPTION;

    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::map(T::value_shape())
    }

    fn decode(value: ComponentValueRef<'_>, field: &str) -> Result<Self, ComponentCodecError> {
        let Some(entries) = value.as_map() else {
            return Err(ComponentCodecError::invalid_field(field, "string-key map"));
        };
        entries
            .iter()
            .map(|(key, entry)| {
                T::decode(entry, &format!("{field}[{key:?}]"))
                    .map(|decoded| (key.to_owned(), decoded))
            })
            .collect()
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        self.iter()
            .map(|(key, entry)| entry.encode().map(|encoded| (key.clone(), encoded)))
            .collect::<Result<BTreeMap<_, _>, _>>()
            .map(ComponentValue::Map)
    }
}

impl<T: PersistentFieldCodec> PersistentFieldCodec for Option<T> {
    const HAS_ASSET_REFERENCES: bool = T::HAS_ASSET_REFERENCES;
    const IS_OPTION: bool = true;
    const HAS_NESTED_OPTION: bool = T::IS_OPTION || T::HAS_NESTED_OPTION;

    fn default_value() -> Option<ComponentValue> {
        Some(ComponentValue::Null)
    }

    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::Optional {
            value: Box::new(T::value_shape()),
        }
    }

    fn decode(value: ComponentValueRef<'_>, field: &str) -> Result<Self, ComponentCodecError> {
        match value.kind() {
            ComponentValueKind::Null => Ok(None),
            _ => T::decode(value, field).map(Some),
        }
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        match self {
            None => Ok(ComponentValue::Null),
            Some(value) => value.encode(),
        }
    }
}

macro_rules! packed_codec {
    ($kind:ident, $is_valid:expr) => {
        impl PersistentFieldCodec for $kind {
            const HAS_ASSET_REFERENCES: bool = false;

            fn value_shape() -> ComponentValueShape {
                ComponentValueShape::scalar(ComponentValueKind::$kind)
            }

            fn decode(
                value: ComponentValueRef<'_>,
                field: &str,
            ) -> Result<Self, ComponentCodecError> {
                match value.as_value() {
                    ComponentValue::$kind(values) if ($is_valid)(values.as_slice()) => {
                        Ok(Self(values.clone()))
                    }
                    _ => Err(ComponentCodecError::invalid_field(field, stringify!($kind))),
                }
            }

            fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
                if !($is_valid)(self.0.as_slice()) {
                    return Err(ComponentValueError::NonFiniteFloat.into());
                }
                Ok(ComponentValue::$kind(self.0.clone()))
            }
        }
    };
}

packed_codec!(PackedU16, |_| true);
packed_codec!(PackedU32, |_| true);
packed_codec!(PackedI32, |_| true);
packed_codec!(PackedF32, |values: &[f32]| values
    .iter()
    .all(|value| value.is_finite()));
packed_codec!(Bytes, |_| true);

fn decode_f32_component(
    value: ComponentValueRef<'_>,
    field: &str,
    axis: &str,
) -> Result<f32, ComponentCodecError> {
    <f32 as PersistentFieldCodec>::decode(value.field(axis)?, axis)
        .map_err(|_| ComponentCodecError::invalid_field(format!("{field}.{axis}"), "finite f32"))
}

mod private {
    use std::collections::BTreeMap;

    use nara_asset::AssetRef;
    use nara_core::{AssetId, Color, EntityUuid, Vec2, Vec3, Vec4};

    use crate::{Bytes, PackedF32, PackedI32, PackedU16, PackedU32};

    pub trait Sealed {}

    impl Sealed for bool {}
    impl Sealed for String {}
    impl Sealed for f32 {}
    impl Sealed for f64 {}
    impl Sealed for i8 {}
    impl Sealed for i16 {}
    impl Sealed for i32 {}
    impl Sealed for i64 {}
    impl Sealed for u8 {}
    impl Sealed for u16 {}
    impl Sealed for u32 {}
    impl Sealed for u64 {}
    impl Sealed for Vec2 {}
    impl Sealed for Vec3 {}
    impl Sealed for Vec4 {}
    impl Sealed for Color {}
    impl Sealed for EntityUuid {}
    impl Sealed for AssetId {}
    impl Sealed for AssetRef {}
    impl Sealed for PackedU16 {}
    impl Sealed for PackedU32 {}
    impl Sealed for PackedI32 {}
    impl Sealed for PackedF32 {}
    impl Sealed for Bytes {}
    impl<T: super::PersistentFieldCodec> Sealed for Vec<T> {}
    impl<T: super::PersistentFieldCodec> Sealed for BTreeMap<String, T> {}
    impl<T: super::PersistentFieldCodec> Sealed for Option<T> {}
}
