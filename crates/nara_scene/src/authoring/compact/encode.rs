use nara_reflect::{
    ComponentNumericDomain, ComponentValue, ComponentValueKind, ComponentValueShape,
};
use serde::Serialize;

use super::{
    Assets, CompactValueError, RawRon, asset_text,
    layout::{Layout, Node, exact_asset_id, math_members},
    make_raw,
};

pub(super) fn encode(
    value: &ComponentValue,
    node: Node<'_>,
    assets: &Assets,
) -> Result<RawRon, CompactValueError> {
    let mut writer = Writer {
        output: String::new(),
        assets,
    };
    writer.value(node, value)?;
    make_raw(writer.output)
}

struct Writer<'a> {
    output: String,
    assets: &'a Assets,
}

impl Writer<'_> {
    fn value(&mut self, node: Node<'_>, value: &ComponentValue) -> Result<(), CompactValueError> {
        use ComponentValueShape as Shape;
        match (node, value) {
            (Node::Layout(Layout::Leaf(field)), _) => self.value(Node::Shape(field.shape()), value),
            (Node::Shape(Shape::Optional { .. }), ComponentValue::Null) => self.token(&None::<()>),
            (Node::Shape(Shape::Optional { value: inner }), _) => {
                self.value(Node::Shape(inner), value)
            }
            (Node::Layout(Layout::Fields(fields)), ComponentValue::Map(values)) => {
                let present = fields.iter().filter_map(|(name, layout)| {
                    let value = values.get(*name)?;
                    let node = Node::Layout(layout);
                    (!node.is_default(value)).then_some((*name, node, value))
                });
                self.fields(node.named_fields(), present)
            }
            (Node::Shape(Shape::Record { members }), ComponentValue::Map(values)) => {
                if let Some(names) = math_members(members) {
                    self.output.push('(');
                    for (position, name) in names.iter().enumerate() {
                        self.separator(position);
                        let item = values
                            .get(*name)
                            .ok_or_else(|| CompactValueError::Layout((*name).into()))?;
                        let shape = node
                            .field(name)
                            .ok_or_else(|| CompactValueError::Layout((*name).into()))?;
                        self.value(shape, item)?;
                    }
                    self.output.push(')');
                    Ok(())
                } else {
                    let present = members.iter().filter_map(|member| {
                        let value = values.get(&member.name)?;
                        Some((member.name.as_str(), Node::Shape(&member.shape), value))
                    });
                    self.fields(node.named_fields(), present)
                }
            }
            (Node::Shape(Shape::Map { value: inner }), ComponentValue::Map(values)) => self.fields(
                false,
                values
                    .iter()
                    .map(|(name, value)| (name.as_str(), Node::Shape(inner), value)),
            ),
            (
                Node::Layout(Layout::Indices(_))
                | Node::Shape(Shape::List { .. } | Shape::KeyedList { .. }),
                ComponentValue::List(values),
            ) => {
                self.output.push('[');
                for (index, item) in values.iter().enumerate() {
                    self.separator(index);
                    let item_node = node
                        .item(index)
                        .ok_or_else(|| CompactValueError::Layout(format!("list index {index}")))?;
                    self.value(item_node, item)?;
                }
                self.output.push(']');
                Ok(())
            }
            (Node::Shape(Shape::EntityRef), ComponentValue::EntityReference(id)) => {
                self.token(&id.to_string())
            }
            (Node::Shape(Shape::AssetRef { .. } | Shape::StableAssetRef { .. }), _) => {
                let id = exact_asset_id(value)?;
                self.token(&asset_text(id, self.assets))
            }
            (
                Node::Shape(
                    shape @ Shape::Scalar {
                        numeric_domain: Some(ComponentNumericDomain::F32),
                        ..
                    },
                ),
                ComponentValue::F64(value),
            ) => {
                let rounded = value.get() as f32;
                let wire = ComponentValue::f64(rounded.into())
                    .map_err(|_| CompactValueError::InvalidValue(ComponentValueKind::F64))?;
                shape.validate_value(&wire)?;
                self.token(&rounded)
            }
            (Node::Shape(Shape::Scalar { .. }), _) => self.scalar(value),
            (Node::Shape(Shape::Opaque { kind }), _) => Err(CompactValueError::OpaqueShape(*kind)),
            _ => Err(CompactValueError::InvalidValue(value.kind())),
        }
    }

    fn fields<'a>(
        &mut self,
        identifiers: bool,
        fields: impl IntoIterator<Item = (&'a str, Node<'a>, &'a ComponentValue)>,
    ) -> Result<(), CompactValueError> {
        self.output.push(if identifiers { '(' } else { '{' });
        for (position, (key, node, value)) in fields.into_iter().enumerate() {
            self.separator(position);
            if identifiers {
                self.output.push_str(key);
            } else {
                self.token(&key)?;
            }
            self.output.push_str(": ");
            self.value(node, value)?;
        }
        self.output.push(if identifiers { ')' } else { '}' });
        Ok(())
    }

    fn scalar(&mut self, value: &ComponentValue) -> Result<(), CompactValueError> {
        match value {
            ComponentValue::Null => self.token(&()),
            ComponentValue::Bool(value) => self.token(value),
            ComponentValue::I64(value) => self.token(value),
            ComponentValue::U64(value) => self.token(value),
            ComponentValue::F64(value) => self.token(&value.get()),
            ComponentValue::String(value) => self.token(value),
            ComponentValue::PackedU16(value) => self.token(value),
            ComponentValue::PackedU32(value) => self.token(value),
            ComponentValue::PackedI32(value) => self.token(value),
            ComponentValue::PackedF32(value) => self.token(value),
            ComponentValue::Bytes(value) => self.token(value),
            _ => Err(CompactValueError::InvalidValue(value.kind())),
        }
    }

    fn token(&mut self, value: &impl Serialize) -> Result<(), CompactValueError> {
        let mut serializer = ron::ser::Serializer::new(&mut self.output, None)?;
        Ok(value.serialize(&mut serializer)?)
    }

    fn separator(&mut self, index: usize) {
        if index != 0 {
            self.output.push_str(", ");
        }
    }
}
