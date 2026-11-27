use nara_asset::AssetRef;

use crate::{
    ComponentCodecError, ComponentNumericDomain, ComponentValue, ComponentValueShape, TypeSchema,
    validate_component_value,
};

use super::{PostcardReference, write_postcard_field};

impl TypeSchema {
    /// Encodes authored fields in the same order and widths as the native Postcard codec.
    /// Missing fields use their declared defaults. Asset locators require a stable UUID.
    pub fn encode_postcard_value(
        &self,
        value: &ComponentValue,
    ) -> Result<Vec<u8>, ComponentCodecError> {
        self.encode_postcard_value_with_references(value, |_| {})
    }

    /// Encodes a component and visits the references written to its payload, including defaults.
    pub fn encode_postcard_value_with_references(
        &self,
        value: &ComponentValue,
        mut visit: impl FnMut(PostcardReference),
    ) -> Result<Vec<u8>, ComponentCodecError> {
        validate_component_value(self, value)?;
        let mut bytes = Vec::new();
        let absent = ComponentValue::Null;
        for field in self.fields() {
            let name = field.id().as_str();
            let value = match value.get_path(field.path()) {
                Ok(value) => value,
                Err(error) => {
                    let default = field
                        .default_value()
                        .or_else(|| {
                            (!field.is_required()
                                && matches!(field.shape(), ComponentValueShape::Optional { .. }))
                            .then_some(&absent)
                        })
                        .ok_or(error)?;
                    field.validate_value(default).map_err(|error| {
                        ComponentCodecError::invalid_field(name, error.to_string())
                    })?;
                    default
                }
            };
            write_value(&mut bytes, value, field.shape(), name, &mut visit)?;
        }
        Ok(bytes)
    }
}

fn write_value(
    bytes: &mut Vec<u8>,
    value: &ComponentValue,
    shape: &ComponentValueShape,
    field: &str,
    visit: &mut dyn FnMut(PostcardReference),
) -> Result<(), ComponentCodecError> {
    use ComponentValue as Value;
    use ComponentValueShape as Shape;

    match (shape, value) {
        (Shape::Scalar { numeric_domain, .. }, value) => {
            write_scalar(bytes, value, *numeric_domain, field)?;
        }
        (Shape::EntityRef, Value::EntityReference(id)) => {
            write_postcard_field(bytes, id, field)?;
            visit(PostcardReference::Entity(*id));
        }
        (Shape::StableAssetRef { .. }, Value::AssetReference(id)) => {
            write_postcard_field(bytes, id, field)?;
            visit(PostcardReference::Asset(*id));
        }
        (Shape::AssetRef { .. }, Value::AssetReference(id)) => {
            write_postcard_field(bytes, &AssetRef::StableId((*id).into()), field)?;
            visit(PostcardReference::Asset(*id));
        }
        (Shape::AssetRef { .. }, _) => {
            return Err(ComponentCodecError::invalid_field(
                field,
                "a stable asset UUID",
            ));
        }
        (Shape::Optional { .. }, Value::Null) => bytes.push(0),
        (Shape::Optional { value: inner }, value) => {
            bytes.push(1);
            write_value(bytes, value, inner, field, visit)?;
        }
        (Shape::Record { members }, Value::Map(values)) => {
            for member in members {
                let value = values
                    .get(&member.name)
                    .ok_or_else(|| ComponentCodecError::missing_field(&member.name))?;
                write_value(bytes, value, &member.shape, field, visit)?;
            }
        }
        (Shape::List { item } | Shape::KeyedList { item, .. }, Value::List(values)) => {
            write_length(bytes, values.len(), field)?;
            for value in values {
                write_value(bytes, value, item, field, visit)?;
            }
        }
        (Shape::Map { value: item }, Value::Map(values)) => {
            write_length(bytes, values.len(), field)?;
            for (name, value) in values {
                write_postcard_field(bytes, name, field)?;
                write_value(bytes, value, item, field, visit)?;
            }
        }
        _ => {
            return Err(ComponentCodecError::invalid_field(
                field,
                "a typed Postcard shape",
            ));
        }
    }
    Ok(())
}

fn write_scalar(
    bytes: &mut Vec<u8>,
    value: &ComponentValue,
    domain: Option<ComponentNumericDomain>,
    field: &str,
) -> Result<(), ComponentCodecError> {
    use ComponentNumericDomain as Domain;
    use ComponentValue as Value;

    macro_rules! narrowed_integer {
        ($value:expr, $native:ty) => {{
            let value = <$native>::try_from(*$value)
                .map_err(|_| ComponentCodecError::invalid_field(field, stringify!($native)))?;
            write_postcard_field(bytes, &value, field)
        }};
    }

    match (value, domain) {
        (Value::Bool(value), None) => write_postcard_field(bytes, value, field),
        (Value::String(value), None) => write_postcard_field(bytes, value, field),
        (Value::I64(value), None) => write_postcard_field(bytes, value, field),
        (Value::I64(value), Some(Domain::I8)) => narrowed_integer!(value, i8),
        (Value::I64(value), Some(Domain::I16)) => narrowed_integer!(value, i16),
        (Value::I64(value), Some(Domain::I32)) => narrowed_integer!(value, i32),
        (Value::U64(value), None) => write_postcard_field(bytes, value, field),
        (Value::U64(value), Some(Domain::U8)) => narrowed_integer!(value, u8),
        (Value::U64(value), Some(Domain::U16)) => narrowed_integer!(value, u16),
        (Value::U64(value), Some(Domain::U32)) => narrowed_integer!(value, u32),
        (Value::F64(value), None) => write_postcard_field(bytes, &value.get(), field),
        (Value::F64(value), Some(Domain::F32)) => {
            write_postcard_field(bytes, &(value.get() as f32), field)
        }
        (Value::PackedU16(values), None) => write_packed(bytes, values, field, u16::to_le_bytes),
        (Value::PackedU32(values), None) => write_packed(bytes, values, field, u32::to_le_bytes),
        (Value::PackedI32(values), None) => write_packed(bytes, values, field, i32::to_le_bytes),
        (Value::PackedF32(values), None) => write_packed(bytes, values, field, f32::to_le_bytes),
        (Value::Bytes(values), None) => write_packed(bytes, values, field, u8::to_le_bytes),
        _ => Err(ComponentCodecError::invalid_field(
            field,
            "a typed Postcard scalar",
        )),
    }
}

fn write_length(
    bytes: &mut Vec<u8>,
    length: usize,
    field: &str,
) -> Result<(), ComponentCodecError> {
    let length = u32::try_from(length)
        .map_err(|_| ComponentCodecError::invalid_field(field, "a 32-bit Postcard length"))?;
    write_postcard_field(bytes, &length, field)
}

fn write_packed<T: bytemuck::Pod, const WIDTH: usize>(
    bytes: &mut Vec<u8>,
    values: &[T],
    field: &str,
    _to_le_bytes: impl Fn(T) -> [u8; WIDTH],
) -> Result<(), ComponentCodecError> {
    write_length(bytes, std::mem::size_of_val(values), field)?;
    #[cfg(target_endian = "little")]
    bytes.extend_from_slice(bytemuck::cast_slice(values));
    #[cfg(target_endian = "big")]
    bytes.extend(values.iter().copied().flat_map(_to_le_bytes));
    Ok(())
}
