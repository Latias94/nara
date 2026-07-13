//! Persistent Rust component authoring support.

use nara_core::Vec2;
use nara_ecs::Component;
use nara_identity::EntityReference;

use crate::{ComponentCodecError, ComponentSchema, ComponentValue, ComponentValueKind};

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
    let value = value.field(axis)?.as_f64().ok_or_else(|| {
        ComponentCodecError::invalid_field(format!("{field}.{axis}"), "finite f32")
    })?;
    if value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
        return Err(ComponentCodecError::invalid_field(
            format!("{field}.{axis}"),
            "finite f32",
        ));
    }
    Ok(value as f32)
}

mod private {
    use nara_core::Vec2;
    use nara_identity::EntityReference;

    pub trait Sealed {}

    impl Sealed for i64 {}
    impl Sealed for u64 {}
    impl Sealed for Vec2 {}
    impl Sealed for EntityReference {}
}
