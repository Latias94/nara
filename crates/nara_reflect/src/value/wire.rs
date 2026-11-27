//! String discriminants remain intact when a containing format buffers a value before its tag.

#[cfg(test)]
mod tests;

use std::{
    collections::{BTreeMap, btree_map::Entry},
    fmt,
};

use serde::{
    Deserialize, Deserializer, Serialize, Serializer,
    de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor},
    ser::SerializeStruct,
};

use super::{ComponentFloat, ComponentValue};

#[derive(Deserialize)]
#[serde(untagged)]
enum Payload {
    Bool(bool),
    Signed(i64),
    Unsigned(u64),
    Float(ComponentFloat),
    Text(String),
    List(Vec<ComponentValue>),
    Numbers(Vec<PackedNumber>),
    Map(#[serde(deserialize_with = "deserialize_value_map")] BTreeMap<String, ComponentValue>),
    Unit(()),
}

#[derive(Deserialize)]
#[serde(untagged)]
enum PackedNumber {
    Integer(i64),
    Float(ComponentFloat),
}

enum Field {
    Type,
    Value,
}

impl<'de> Deserialize<'de> for Field {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(FieldVisitor)
    }
}

struct FieldVisitor;

impl<'de> Visitor<'de> for FieldVisitor {
    type Value = Field;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a component value field")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        match value {
            "type" => Ok(Field::Type),
            "value" => Ok(Field::Value),
            _ => Err(E::unknown_field(value, &["type", "value"])),
        }
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        match value {
            0 => Ok(Field::Type),
            1 => Ok(Field::Value),
            _ => Err(E::invalid_value(de::Unexpected::Unsigned(value), &self)),
        }
    }

    fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
        match std::str::from_utf8(value) {
            Ok(value) => self.visit_str(value),
            Err(_) => Err(E::invalid_value(de::Unexpected::Bytes(value), &self)),
        }
    }
}

impl<'de> Deserialize<'de> for ComponentValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_struct("ValueWire", &["type", "value"], ValueVisitor)
    }
}

struct ValueVisitor;

impl<'de> Visitor<'de> for ValueVisitor {
    type Value = ComponentValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("struct ValueWire")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut kind = None;
        let mut value = None;
        while let Some(field) = map.next_key::<Field>()? {
            match field {
                Field::Type => {
                    if kind.is_some() {
                        return Err(de::Error::duplicate_field("type"));
                    }
                    kind = Some(map.next_value::<String>()?);
                }
                Field::Value => {
                    if value.is_some() {
                        return Err(de::Error::duplicate_field("value"));
                    }
                    value = Some(map.next_value_seed(PayloadSeed(kind.as_deref()))?);
                }
            }
        }
        decode_value(
            kind.as_deref()
                .ok_or_else(|| de::Error::missing_field("type"))?,
            value,
        )
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let kind = sequence
            .next_element::<String>()?
            .ok_or_else(|| de::Error::invalid_length(0, &self))?;
        let value = sequence.next_element_seed(PayloadSeed(Some(&kind)))?;
        decode_value(&kind, value)
    }
}

fn decode_value<E: de::Error>(kind: &str, value: Option<Payload>) -> Result<ComponentValue, E> {
    match (kind, value) {
        ("null", None | Some(Payload::Unit(()))) => Ok(ComponentValue::Null),
        ("bool", Some(Payload::Bool(value))) => Ok(ComponentValue::Bool(value)),
        ("i64", Some(Payload::Signed(value))) => Ok(ComponentValue::I64(value)),
        ("i64", Some(Payload::Unsigned(value))) => i64::try_from(value)
            .map(ComponentValue::I64)
            .map_err(serde::de::Error::custom),
        ("u64", Some(Payload::Unsigned(value))) => Ok(ComponentValue::U64(value)),
        ("u64", Some(Payload::Signed(value))) => u64::try_from(value)
            .map(ComponentValue::U64)
            .map_err(serde::de::Error::custom),
        ("f64", Some(Payload::Float(value))) => Ok(ComponentValue::F64(value)),
        ("f64", Some(Payload::Signed(value))) => ComponentFloat::new(value as f64)
            .map(ComponentValue::F64)
            .map_err(serde::de::Error::custom),
        ("f64", Some(Payload::Unsigned(value))) => ComponentFloat::new(value as f64)
            .map(ComponentValue::F64)
            .map_err(serde::de::Error::custom),
        ("string", Some(Payload::Text(value))) => Ok(ComponentValue::String(value)),
        ("list", Some(Payload::List(value))) => Ok(ComponentValue::List(value)),
        ("map", Some(Payload::Map(value))) => Ok(ComponentValue::Map(value)),
        ("asset_ref", Some(Payload::Text(value))) => value
            .parse()
            .map(ComponentValue::AssetReference)
            .map_err(de::Error::custom),
        ("entity_ref", Some(Payload::Text(value))) => value
            .parse()
            .map(ComponentValue::EntityReference)
            .map_err(de::Error::custom),
        (kind @ ("packed_u16" | "packed_u32" | "packed_i32" | "packed_f32" | "bytes"), value) => {
            match value {
                Some(Payload::Numbers(values)) => decode_packed(kind, values),
                Some(Payload::List(values)) if values.is_empty() => decode_packed(kind, Vec::new()),
                _ => Err(E::custom("packed field requires a numeric sequence")),
            }
        }
        _ => Err(serde::de::Error::custom(
            "component value payload does not match its type",
        )),
    }
}

fn decode_packed<E: de::Error>(kind: &str, values: Vec<PackedNumber>) -> Result<ComponentValue, E> {
    fn integers<T: TryFrom<i64>, E: de::Error>(values: Vec<PackedNumber>) -> Result<Vec<T>, E> {
        values
            .into_iter()
            .map(|value| match value {
                PackedNumber::Integer(value) => T::try_from(value)
                    .map_err(|_| E::custom("packed integer is outside its type's range")),
                PackedNumber::Float(_) => Err(E::custom("packed integer field requires integers")),
            })
            .collect()
    }
    match kind {
        "packed_u16" => integers(values).map(ComponentValue::PackedU16),
        "packed_u32" => integers(values).map(ComponentValue::PackedU32),
        "packed_i32" => integers(values).map(ComponentValue::PackedI32),
        "bytes" => integers(values).map(ComponentValue::Bytes),
        "packed_f32" => values
            .into_iter()
            .map(|value| {
                let value = match value {
                    PackedNumber::Integer(value) => value as f32,
                    PackedNumber::Float(value) => value.get() as f32,
                };
                if value.is_finite() {
                    Ok(value)
                } else {
                    Err(E::custom("packed float requires finite f32 values"))
                }
            })
            .collect::<Result<_, _>>()
            .map(ComponentValue::PackedF32),
        _ => Err(E::custom("unknown packed field type")),
    }
}

struct PayloadSeed<'a>(Option<&'a str>);

impl<'de> DeserializeSeed<'de> for PayloadSeed<'_> {
    type Value = Payload;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        match self.0 {
            Some("list") => deserializer.deserialize_any(ListVisitor).map(Payload::List),
            Some("map") => deserialize_value_map(deserializer).map(Payload::Map),
            Some("bool" | "i64" | "u64" | "f64" | "string" | "asset_ref" | "entity_ref") => {
                deserializer.deserialize_any(ScalarPayloadVisitor)
            }
            Some("packed_u16" | "packed_u32" | "packed_i32" | "packed_f32" | "bytes") => {
                Vec::deserialize(deserializer).map(Payload::Numbers)
            }
            _ => Payload::deserialize(deserializer),
        }
    }
}

struct ScalarPayloadVisitor;

impl<'de> Visitor<'de> for ScalarPayloadVisitor {
    type Value = Payload;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a scalar component value payload")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Payload::Bool(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Payload::Signed(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(i64::try_from(value).map_or(Payload::Unsigned(value), Payload::Signed))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        ComponentFloat::new(value)
            .map(Payload::Float)
            .map_err(E::custom)
    }

    fn visit_char<E: de::Error>(self, value: char) -> Result<Self::Value, E> {
        Err(E::invalid_type(de::Unexpected::Char(value), &self))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Payload::Text(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        Ok(Payload::Text(value))
    }

    fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
        std::str::from_utf8(value)
            .map(|value| Payload::Text(value.to_owned()))
            .map_err(|_| E::invalid_value(de::Unexpected::Bytes(value), &self))
    }

    fn visit_byte_buf<E: de::Error>(self, value: Vec<u8>) -> Result<Self::Value, E> {
        String::from_utf8(value)
            .map(Payload::Text)
            .map_err(|error| E::invalid_value(de::Unexpected::Bytes(error.as_bytes()), &self))
    }
}

struct ListVisitor;

impl<'de> Visitor<'de> for ListVisitor {
    type Value = Vec<ComponentValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a sequence of component values")
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut sequence: A) -> Result<Self::Value, A::Error> {
        let mut values = Vec::new();
        while let Some(value) = sequence.next_element_seed(NestedValueSeed)? {
            values.push(value);
        }
        Ok(values)
    }
}

struct NestedValueSeed;

impl<'de> DeserializeSeed<'de> for NestedValueSeed {
    type Value = ComponentValue;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(ValueVisitor)
    }
}

fn deserialize_value_map<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, ComponentValue>, D::Error> {
    deserializer.deserialize_any(MapVisitor)
}

struct MapVisitor;

impl<'de> Visitor<'de> for MapVisitor {
    type Value = BTreeMap<String, ComponentValue>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a map of component values")
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut values = BTreeMap::new();
        while let Some(key) = map.next_key_seed(StringKeySeed)? {
            match values.entry(key) {
                Entry::Vacant(entry) => {
                    entry.insert(map.next_value_seed(NestedValueSeed)?);
                }
                Entry::Occupied(entry) => {
                    return Err(de::Error::custom(format_args!(
                        "duplicate component value map key {:?}",
                        entry.key()
                    )));
                }
            }
        }
        Ok(values)
    }
}

struct StringKeySeed;

impl<'de> DeserializeSeed<'de> for StringKeySeed {
    type Value = String;

    fn deserialize<D: Deserializer<'de>>(self, deserializer: D) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(StringKeyVisitor)
    }
}

struct StringKeyVisitor;

impl<'de> Visitor<'de> for StringKeyVisitor {
    type Value = String;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a string map key")
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(value.to_owned())
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        Ok(value)
    }

    fn visit_bytes<E: de::Error>(self, value: &[u8]) -> Result<Self::Value, E> {
        std::str::from_utf8(value)
            .map(str::to_owned)
            .map_err(|_| E::invalid_value(de::Unexpected::Bytes(value), &self))
    }

    fn visit_byte_buf<E: de::Error>(self, value: Vec<u8>) -> Result<Self::Value, E> {
        String::from_utf8(value)
            .map_err(|error| E::invalid_value(de::Unexpected::Bytes(error.as_bytes()), &self))
    }
}

impl Serialize for ComponentValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let tag = match self {
            Self::Null => "null",
            Self::Bool(_) => "bool",
            Self::I64(_) => "i64",
            Self::U64(_) => "u64",
            Self::F64(_) => "f64",
            Self::String(_) => "string",
            Self::List(_) => "list",
            Self::Map(_) => "map",
            Self::AssetReference(_) => "asset_ref",
            Self::EntityReference(_) => "entity_ref",
            Self::PackedU16(_) => "packed_u16",
            Self::PackedU32(_) => "packed_u32",
            Self::PackedI32(_) => "packed_i32",
            Self::PackedF32(_) => "packed_f32",
            Self::Bytes(_) => "bytes",
        };
        let mut wire = serializer.serialize_struct(
            "ComponentValue",
            if matches!(self, Self::Null) { 1 } else { 2 },
        )?;
        wire.serialize_field("type", tag)?;
        match self {
            Self::Null => {}
            Self::Bool(value) => wire.serialize_field("value", value)?,
            Self::I64(value) => wire.serialize_field("value", value)?,
            Self::U64(value) => wire.serialize_field("value", value)?,
            Self::F64(value) => wire.serialize_field("value", value)?,
            Self::String(value) => wire.serialize_field("value", value)?,
            Self::List(value) => wire.serialize_field("value", value)?,
            Self::Map(value) => wire.serialize_field("value", value)?,
            Self::AssetReference(value) => wire.serialize_field("value", &value.to_string())?,
            Self::EntityReference(value) => wire.serialize_field("value", &value.to_string())?,
            Self::PackedU16(value) => wire.serialize_field("value", value)?,
            Self::PackedU32(value) => wire.serialize_field("value", value)?,
            Self::PackedI32(value) => wire.serialize_field("value", value)?,
            Self::PackedF32(value) => {
                if value.iter().any(|value| !value.is_finite()) {
                    return Err(serde::ser::Error::custom(
                        "packed float requires finite f32 values",
                    ));
                }
                wire.serialize_field("value", value)?;
            }
            Self::Bytes(value) => wire.serialize_field("value", value)?,
        }
        wire.end()
    }
}
