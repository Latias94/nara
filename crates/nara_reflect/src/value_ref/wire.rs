//! Borrowed serialization using the existing component value grammar.

use serde::{
    Serialize, Serializer,
    ser::{SerializeMap, SerializeSeq, SerializeStruct},
};

use crate::ComponentValueKind;

use super::{ComponentListRef, ComponentMapRef, ComponentValueRef};

impl Serialize for ComponentValueRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if let Some(value) = self.as_value() {
            return value.serialize(serializer);
        }
        let mut wire = serializer.serialize_struct("ComponentValue", 2)?;
        match self.kind() {
            ComponentValueKind::Map => {
                wire.serialize_field("type", "map")?;
                wire.serialize_field("value", &self.as_map().expect("Map kind checked"))?;
            }
            ComponentValueKind::List => {
                wire.serialize_field("type", "list")?;
                wire.serialize_field("value", &self.as_list().expect("List kind checked"))?;
            }
            _ => unreachable!("frozen insertion branches are containers"),
        }
        wire.end()
    }
}

impl Serialize for ComponentMapRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.len()))?;
        for (name, value) in self.iter() {
            map.serialize_entry(name, &value)?;
        }
        map.end()
    }
}

impl Serialize for ComponentListRef<'_> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut list = serializer.serialize_seq(Some(self.len()))?;
        for value in self.iter() {
            list.serialize_element(&value)?;
        }
        list.end()
    }
}
