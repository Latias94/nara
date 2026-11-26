use std::{collections::BTreeMap, fmt};

use nara_reflect::{
    ComponentNumericDomain, ComponentValue, ComponentValueKind, ComponentValueShape,
};
use ron::{Deserializer, Options, extensions::Extensions};
use serde::{
    Deserialize,
    de::{DeserializeSeed, Error, IgnoredAny, MapAccess, SeqAccess, Visitor},
};

use super::{
    Assets, CompactValueError, RawRon, decode_asset,
    layout::{Layout, Node, math_members},
};

pub(super) fn decode(
    raw: &RawRon,
    node: Node<'_>,
    assets: &Assets,
) -> Result<ComponentValue, CompactValueError> {
    let options = Options::default().with_default_extension(Extensions::IMPLICIT_SOME);
    let mut deserializer = Deserializer::from_str_with_options(raw.as_ron(), &options)?;
    let value = Decode { node, assets }
        .deserialize(&mut deserializer)
        .map_err(|error| deserializer.span_error(error))?;
    deserializer
        .end()
        .map_err(|error| deserializer.span_error(error))?;
    Ok(value)
}

#[derive(Clone, Copy)]
struct Decode<'a> {
    node: Node<'a>,
    assets: &'a Assets,
}

impl<'a> Decode<'a> {
    fn at(self, node: Node<'a>) -> Self {
        Self { node, ..self }
    }
}

impl<'de> DeserializeSeed<'de> for Decode<'_> {
    type Value = ComponentValue;

    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        use ComponentValueShape as Shape;
        match self.node {
            Node::Layout(Layout::Leaf(field)) => self
                .at(Node::Shape(field.shape()))
                .deserialize(deserializer),
            Node::Shape(Shape::Record { members }) if let Some(names) = math_members(members) => {
                deserializer.deserialize_tuple(names.len(), self)
            }
            Node::Layout(Layout::Fields(_)) | Node::Shape(Shape::Record { .. }) => {
                if self.node.named_fields() {
                    deserializer.deserialize_struct("", &[], self)
                } else {
                    deserializer.deserialize_map(self)
                }
            }
            Node::Layout(Layout::Indices(_))
            | Node::Shape(Shape::List { .. } | Shape::KeyedList { .. }) => {
                deserializer.deserialize_seq(self)
            }
            Node::Shape(Shape::Map { .. }) => deserializer.deserialize_map(self),
            Node::Shape(Shape::Optional { .. }) => deserializer.deserialize_option(self),
            Node::Shape(Shape::EntityRef) => String::deserialize(deserializer)?
                .parse()
                .map(ComponentValue::EntityReference)
                .map_err(D::Error::custom),
            Node::Shape(Shape::AssetRef { .. } | Shape::StableAssetRef { .. }) => {
                let reference = String::deserialize(deserializer)?;
                decode_asset(&reference, self.assets)
                    .map(ComponentValue::AssetReference)
                    .map_err(D::Error::custom)
            }
            Node::Shape(Shape::Scalar {
                numeric_domain: Some(ComponentNumericDomain::F32),
                ..
            }) => ComponentValue::f64(f32::deserialize(deserializer)?.into())
                .map_err(D::Error::custom),
            Node::Shape(Shape::Scalar { kind, .. }) => decode_scalar(*kind, deserializer),
            Node::Shape(Shape::Opaque { kind }) => {
                Err(D::Error::custom(CompactValueError::OpaqueShape(*kind)))
            }
        }
    }
}

impl<'de> Visitor<'de> for Decode<'_> {
    type Value = ComponentValue;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a compact value matching the scene schema")
    }

    fn visit_none<E: Error>(self) -> Result<Self::Value, E> {
        Ok(ComponentValue::Null)
    }

    fn visit_some<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        if let Node::Shape(ComponentValueShape::Optional { value }) = self.node {
            self.at(Node::Shape(value)).deserialize(deserializer)
        } else {
            Err(D::Error::custom(
                "unexpected Some value for non-optional field",
            ))
        }
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut fields = BTreeMap::new();
        while let Some(name) = map.next_key::<String>()? {
            let Some(node) = self.node.field(&name) else {
                return Err(A::Error::custom(format_args!("undeclared field {name:?}")));
            };
            match fields.entry(name) {
                std::collections::btree_map::Entry::Vacant(entry) => {
                    entry.insert(map.next_value_seed(self.at(node))?);
                }
                std::collections::btree_map::Entry::Occupied(entry) => {
                    return Err(A::Error::custom(format_args!(
                        "duplicate field {:?}",
                        entry.key()
                    )));
                }
            }
        }
        Ok(ComponentValue::Map(fields))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        if let Node::Shape(ComponentValueShape::Record { members }) = self.node
            && let Some(names) = math_members(members)
        {
            let mut fields = BTreeMap::new();
            for name in names {
                let node = self
                    .node
                    .field(name)
                    .ok_or_else(|| A::Error::custom("missing math member"))?;
                let value = seq.next_element_seed(self.at(node))?.ok_or_else(|| {
                    A::Error::custom(format_args!("missing math coordinate {name}"))
                })?;
                fields.insert((*name).to_owned(), value);
            }
            if seq.next_element::<IgnoredAny>()?.is_some() {
                return Err(A::Error::custom("too many math coordinates"));
            }
            return Ok(ComponentValue::Map(fields));
        }

        let mut items = Vec::new();
        while let Some(node) = self.node.item(items.len()) {
            let Some(value) = seq.next_element_seed(self.at(node))? else {
                return Ok(ComponentValue::List(items));
            };
            items.push(value);
        }
        if seq.next_element::<IgnoredAny>()?.is_some() {
            return Err(A::Error::custom(format_args!(
                "undeclared list index {}",
                items.len()
            )));
        }
        Ok(ComponentValue::List(items))
    }
}

fn decode_scalar<'de, D: serde::Deserializer<'de>>(
    kind: ComponentValueKind,
    deserializer: D,
) -> Result<ComponentValue, D::Error> {
    use ComponentValueKind as Kind;
    match kind {
        Kind::Null => <()>::deserialize(deserializer).map(|()| ComponentValue::Null),
        Kind::Bool => bool::deserialize(deserializer).map(ComponentValue::Bool),
        Kind::I64 => i64::deserialize(deserializer).map(ComponentValue::I64),
        Kind::U64 => u64::deserialize(deserializer).map(ComponentValue::U64),
        Kind::F64 => ComponentValue::f64(f64::deserialize(deserializer)?).map_err(D::Error::custom),
        Kind::String => String::deserialize(deserializer).map(ComponentValue::String),
        Kind::PackedU16 => Vec::<u16>::deserialize(deserializer).map(ComponentValue::PackedU16),
        Kind::PackedU32 => Vec::<u32>::deserialize(deserializer).map(ComponentValue::PackedU32),
        Kind::PackedI32 => Vec::<i32>::deserialize(deserializer).map(ComponentValue::PackedI32),
        Kind::PackedF32 => Vec::<f32>::deserialize(deserializer).map(ComponentValue::PackedF32),
        Kind::Bytes => Vec::<u8>::deserialize(deserializer).map(ComponentValue::Bytes),
        _ => Err(D::Error::custom(CompactValueError::OpaqueShape(kind))),
    }
}
