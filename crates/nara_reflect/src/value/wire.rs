//! String discriminants remain intact when a containing format buffers a value before its tag.

use serde::{Deserialize, Deserializer, Serialize, Serializer, ser::SerializeStruct};

use super::{ComponentFloat, ComponentValue};

// These are exactly the existing payload types, buffered by serde only when their tag follows
// the payload. No source interpretation, reference resolution or second value model is added.
#[derive(Deserialize)]
#[serde(untagged)]
enum Payload {
    Bool(bool),
    Signed(i64),
    Unsigned(u64),
    Float(ComponentFloat),
    Text(String),
    List(Vec<ComponentValue>),
    Map(std::collections::BTreeMap<String, ComponentValue>),
    Reference(nara_identity::EntityReference),
    Unit(()),
}

fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Payload>, D::Error> {
    Payload::deserialize(deserializer).map(Some)
}

impl<'de> Deserialize<'de> for ComponentValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct ValueWire {
            #[serde(rename = "type")]
            kind: String,
            #[serde(default, deserialize_with = "present")]
            value: Option<Payload>,
        }
        let wire = ValueWire::deserialize(deserializer)?;
        match (wire.kind.as_str(), wire.value) {
            ("null", None | Some(Payload::Unit(()))) => Ok(Self::Null),
            ("bool", Some(Payload::Bool(value))) => Ok(Self::Bool(value)),
            ("i64", Some(Payload::Signed(value))) => Ok(Self::I64(value)),
            ("i64", Some(Payload::Unsigned(value))) => i64::try_from(value)
                .map(Self::I64)
                .map_err(serde::de::Error::custom),
            ("u64", Some(Payload::Unsigned(value))) => Ok(Self::U64(value)),
            ("u64", Some(Payload::Signed(value))) => u64::try_from(value)
                .map(Self::U64)
                .map_err(serde::de::Error::custom),
            ("f64", Some(Payload::Float(value))) => Ok(Self::F64(value)),
            ("f64", Some(Payload::Signed(value))) => ComponentFloat::new(value as f64)
                .map(Self::F64)
                .map_err(serde::de::Error::custom),
            ("f64", Some(Payload::Unsigned(value))) => ComponentFloat::new(value as f64)
                .map(Self::F64)
                .map_err(serde::de::Error::custom),
            ("string", Some(Payload::Text(value))) => Ok(Self::String(value)),
            ("list", Some(Payload::List(value))) => Ok(Self::List(value)),
            ("map", Some(Payload::Map(value))) => Ok(Self::Map(value)),
            ("entity_ref", Some(Payload::Reference(value))) => Ok(Self::EntityReference(value)),
            _ => Err(serde::de::Error::custom(
                "component value payload does not match its type",
            )),
        }
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
            Self::EntityReference(_) => "entity_ref",
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
            Self::EntityReference(value) => wire.serialize_field("value", value)?,
        }
        wire.end()
    }
}
