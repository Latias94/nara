//! Persistent Rust component authoring support.

use nara_core::Vec2;
use nara_ecs::Component;
use nara_identity::EntityReference;

use crate::{
    ComponentCodecError, ComponentNumericDomain, ComponentSchema, ComponentValue,
    ComponentValueKind,
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
    const VALUE_KIND: ComponentValueKind;
    const NUMERIC_DOMAIN: Option<ComponentNumericDomain> = None;

    fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError>;

    fn encode(&self) -> Result<ComponentValue, ComponentCodecError>;
}

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
    const VALUE_KIND: ComponentValueKind = ComponentValueKind::Bool;

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
    const VALUE_KIND: ComponentValueKind = ComponentValueKind::String;

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
            const VALUE_KIND: ComponentValueKind = ComponentValueKind::$kind;
            const NUMERIC_DOMAIN: Option<ComponentNumericDomain> =
                Some(ComponentNumericDomain::$domain);

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
    const VALUE_KIND: ComponentValueKind = ComponentValueKind::F64;
    const NUMERIC_DOMAIN: Option<ComponentNumericDomain> = Some(ComponentNumericDomain::F32);

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
    const VALUE_KIND: ComponentValueKind = ComponentValueKind::F64;

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
    const VALUE_KIND: ComponentValueKind = ComponentValueKind::I64;

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
    const VALUE_KIND: ComponentValueKind = ComponentValueKind::U64;

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
    const VALUE_KIND: ComponentValueKind = ComponentValueKind::Map;

    fn decode(value: &ComponentValue, field: &str) -> Result<Self, ComponentCodecError> {
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
    const VALUE_KIND: ComponentValueKind = ComponentValueKind::EntityRef;

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

fn decode_f32_component(
    value: &ComponentValue,
    field: &str,
    axis: &str,
) -> Result<f32, ComponentCodecError> {
    <f32 as PersistentFieldCodec>::decode(value.field(axis)?, axis)
        .map_err(|_| ComponentCodecError::invalid_field(format!("{field}.{axis}"), "finite f32"))
}

mod private {
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
}
