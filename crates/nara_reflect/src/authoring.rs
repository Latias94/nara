//! Persistent Rust component authoring support.

use std::collections::BTreeMap;

use nara_asset::AssetRef;
use nara_core::Vec2;
use nara_ecs::Component;
use nara_identity::EntityReference;

use crate::{
    ComponentCodecError, ComponentNumericDomain, ComponentSchema, ComponentValue,
    ComponentValueKind, ComponentValueShape,
};

pub trait PersistentComponentProvider: Component + Sized + 'static {
    fn persistent_component_schema() -> ComponentSchema;

    #[doc(hidden)]
    fn __decode_persistent_component(value: &ComponentValue) -> Result<Self, ComponentCodecError>;

    #[doc(hidden)]
    fn __encode_persistent_component(&self) -> Result<ComponentValue, ComponentCodecError>;
}

#[doc(hidden)]
pub trait PersistentFieldCodec: private::Sealed + Sized + 'static {
    fn value_shape() -> ComponentValueShape;

    fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError>;

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
    value: &ComponentValue,
    field: &str,
) -> Result<T, ComponentCodecError>
where
    T: PersistentFieldCodec,
{
    T::decode(value.field(field)?, field)
}

#[doc(hidden)]
pub fn encode_persistent_field<T>(value: &T) -> Result<ComponentValue, ComponentCodecError>
where
    T: PersistentFieldCodec,
{
    value.encode()
}

impl PersistentFieldCodec for bool {
    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::scalar(ComponentValueKind::Bool)
    }

    fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError> {
        value
            .as_bool()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "bool"))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::Bool(*self))
    }
}

impl PersistentFieldCodec for String {
    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::scalar(ComponentValueKind::String)
    }

    fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError> {
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
            fn value_shape() -> ComponentValueShape {
                ComponentValueShape::scalar(ComponentValueKind::$kind)
                    .with_numeric_domain(ComponentNumericDomain::$domain)
            }

            fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError> {
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
    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::scalar(ComponentValueKind::F64)
            .with_numeric_domain(ComponentNumericDomain::F32)
    }

    fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError> {
        match value {
            ComponentValue::F64(number) if ComponentNumericDomain::F32.accepts_value(value) => {
                // The domain check excludes overflow; ordinary finite rounding is intentional.
                Ok(number.get() as f32)
            }
            _ => Err(ComponentCodecError::invalid_field(field, "finite f32")),
        }
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::f64(f64::from(*self))?)
    }
}

impl PersistentFieldCodec for f64 {
    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::scalar(ComponentValueKind::F64)
    }

    fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError> {
        value
            .as_f64()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "finite f64"))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::f64(*self)?)
    }
}

impl PersistentFieldCodec for i64 {
    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::scalar(ComponentValueKind::I64)
    }

    fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError> {
        value
            .as_i64()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "i64"))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::I64(*self))
    }
}

impl PersistentFieldCodec for u64 {
    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::scalar(ComponentValueKind::U64)
    }

    fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError> {
        value
            .as_u64()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "u64"))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::U64(*self))
    }
}

impl PersistentFieldCodec for Vec2 {
    fn value_shape() -> ComponentValueShape {
        let axis = <f32 as PersistentFieldCodec>::value_shape();
        ComponentValueShape::record([("x".to_owned(), axis.clone()), ("y".to_owned(), axis)])
    }

    fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError> {
        if !matches!(value, ComponentValue::Map(members) if members.len() == 2) {
            return Err(ComponentCodecError::invalid_field(
                field,
                "a closed x/y record of finite f32 values",
            ));
        }
        let x = decode_f32_component(value, field, "x")?;
        let y = decode_f32_component(value, field, "y")?;
        Ok(Self::new(x, y))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::map([
            ("x", ComponentValue::f64(f64::from(self.x))?),
            ("y", ComponentValue::f64(f64::from(self.y))?),
        ]))
    }
}

impl PersistentFieldCodec for EntityReference {
    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::EntityRef
    }

    fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError> {
        value
            .as_entity_reference()
            .cloned()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "entity reference"))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(ComponentValue::EntityReference(self.clone()))
    }
}

impl PersistentFieldCodec for AssetRef {
    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::AssetRef
    }

    fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError> {
        crate::decode_asset_reference_value(value)
            .map_err(|_| ComponentCodecError::invalid_field(field, "canonical asset reference"))
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        Ok(crate::encode_asset_reference_value(self))
    }
}

impl<T: PersistentFieldCodec> PersistentFieldCodec for Vec<T> {
    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::list(T::value_shape())
    }

    fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError> {
        let ComponentValue::List(items) = value else {
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
    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::map(T::value_shape())
    }

    fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError> {
        let ComponentValue::Map(entries) = value else {
            return Err(ComponentCodecError::invalid_field(field, "string-key map"));
        };
        entries
            .iter()
            .map(|(key, entry)| {
                T::decode(entry, &format!("{field}[{key:?}]")).map(|decoded| (key.clone(), decoded))
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
    fn value_shape() -> ComponentValueShape {
        ComponentValueShape::Optional {
            value: Box::new(T::value_shape()),
        }
    }

    fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError> {
        match value {
            ComponentValue::Null => Ok(None),
            value => T::decode(value, field).map(Some),
        }
    }

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError> {
        match self {
            None => Ok(ComponentValue::Null),
            Some(value) => value.encode(),
        }
    }
}

fn decode_f32_component(
    value: &ComponentValue,
    field: &str,
    axis: &str,
) -> Result<f32, ComponentCodecError> {
    <f32 as PersistentFieldCodec>::decode(value.field(axis)?, axis)
        .map_err(|_| ComponentCodecError::invalid_field(format!("{field}.{axis}"), "finite f32"))
}

mod private {
    use std::collections::BTreeMap;

    use nara_asset::AssetRef;
    use nara_core::Vec2;
    use nara_identity::EntityReference;

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
    impl Sealed for EntityReference {}
    impl Sealed for AssetRef {}
    impl<T: super::PersistentFieldCodec> Sealed for Vec<T> {}
    impl<T: super::PersistentFieldCodec> Sealed for BTreeMap<String, T> {}
    impl<T: super::PersistentFieldCodec> Sealed for Option<T> {}
}
