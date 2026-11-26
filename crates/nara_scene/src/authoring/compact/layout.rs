use std::collections::BTreeMap;

use nara_core::AssetId;
use nara_reflect::{
    ComponentFieldPathSegment, ComponentFieldSchema, ComponentNumericDomain, ComponentSchema,
    ComponentShapeMember, ComponentValue, ComponentValueShape, decode_asset_reference_value,
};

use super::CompactValueError;

pub(super) enum Layout<'a> {
    Leaf(&'a ComponentFieldSchema),
    Fields(Vec<(&'a str, Self)>),
    Indices(BTreeMap<u32, Self>),
}

impl<'a> Layout<'a> {
    pub(super) fn new(schema: &'a ComponentSchema) -> Result<Self, CompactValueError> {
        for field in schema.fields() {
            if let Some(default) = field.default_value() {
                field.shape().validate()?;
                validate_asset_defaults(field.shape(), default)?;
            }
        }
        if let [field] = schema.fields()
            && field.path().is_empty()
        {
            return Ok(Self::Leaf(field));
        }
        let mut root = match schema
            .fields()
            .first()
            .and_then(|field| field.path().segments().first())
        {
            Some(ComponentFieldPathSegment::Index(_)) => Self::Indices(BTreeMap::new()),
            _ => Self::Fields(Vec::new()),
        };
        for field in schema.fields() {
            if !root.insert(field.path().segments(), field) {
                return Err(CompactValueError::Layout(field.path().to_string()));
            }
        }
        Ok(root)
    }

    fn insert(
        &mut self,
        path: &'a [ComponentFieldPathSegment],
        field: &'a ComponentFieldSchema,
    ) -> bool {
        let Some((segment, remaining)) = path.split_first() else {
            return false;
        };
        let child = || match remaining.first() {
            Some(ComponentFieldPathSegment::Field(_)) => Self::Fields(Vec::new()),
            Some(ComponentFieldPathSegment::Index(_)) => Self::Indices(BTreeMap::new()),
            None => Self::Leaf(field),
        };
        let child = match (self, segment) {
            (Self::Fields(fields), ComponentFieldPathSegment::Field(name)) => {
                if let Some(index) = fields.iter().position(|(key, _)| *key == name) {
                    &mut fields[index].1
                } else {
                    fields.push((name, child()));
                    let (_, entry) = fields.last_mut().expect("field was just added");
                    if remaining.is_empty() {
                        return true;
                    }
                    entry
                }
            }
            (Self::Indices(indices), ComponentFieldPathSegment::Index(index)) => {
                use std::collections::btree_map::Entry;
                match indices.entry(*index) {
                    Entry::Occupied(entry) => entry.into_mut(),
                    Entry::Vacant(entry) => {
                        let entry = entry.insert(child());
                        if remaining.is_empty() {
                            return true;
                        }
                        entry
                    }
                }
            }
            _ => return false,
        };
        child.insert(remaining, field)
    }
}

pub(super) fn exact_asset_id(value: &ComponentValue) -> Result<AssetId, CompactValueError> {
    if let Some(id) = value.as_asset_reference() {
        return Ok(*id);
    }
    Err(match decode_asset_reference_value(value) {
        Ok(locator) => CompactValueError::InvalidAssetId(locator.to_string()),
        Err(_) => CompactValueError::InvalidValue(value.kind()),
    })
}

fn validate_asset_defaults(
    shape: &ComponentValueShape,
    default: &ComponentValue,
) -> Result<(), CompactValueError> {
    use ComponentValueShape as Shape;
    match (shape, default) {
        (Shape::AssetRef { .. } | Shape::StableAssetRef { .. }, _) => {
            exact_asset_id(default)?;
        }
        (Shape::Optional { value }, default) if *default != ComponentValue::Null => {
            validate_asset_defaults(value, default)?;
        }
        (Shape::List { item } | Shape::KeyedList { item, .. }, ComponentValue::List(defaults)) => {
            for default in defaults {
                validate_asset_defaults(item, default)?;
            }
        }
        (Shape::Map { value }, ComponentValue::Map(defaults)) => {
            for default in defaults.values() {
                validate_asset_defaults(value, default)?;
            }
        }
        (Shape::Record { members }, ComponentValue::Map(defaults)) => {
            for member in members {
                if let Some(default) = defaults.get(&member.name) {
                    validate_asset_defaults(&member.shape, default)?;
                }
            }
        }
        _ => {}
    }
    Ok(())
}

#[derive(Clone, Copy)]
pub(super) enum Node<'a> {
    Layout(&'a Layout<'a>),
    Shape(&'a ComponentValueShape),
}

impl<'a> Node<'a> {
    pub(super) fn named_fields(self) -> bool {
        match self {
            Self::Layout(Layout::Fields(fields)) => {
                fields.iter().all(|(name, _)| plain_identifier(name))
            }
            Self::Shape(ComponentValueShape::Record { members }) => {
                members.iter().all(|member| plain_identifier(&member.name))
            }
            _ => false,
        }
    }

    pub(super) fn field(self, name: &str) -> Option<Self> {
        match self {
            Self::Layout(Layout::Fields(fields)) => fields
                .iter()
                .find(|(key, _)| *key == name)
                .map(|(_, layout)| Self::Layout(layout)),
            Self::Shape(ComponentValueShape::Record { members }) => members
                .iter()
                .find(|member| member.name == name)
                .map(|member| Self::Shape(&member.shape)),
            Self::Shape(ComponentValueShape::Map { value }) => Some(Self::Shape(value)),
            _ => None,
        }
    }

    pub(super) fn item(self, index: usize) -> Option<Self> {
        match self {
            Self::Layout(Layout::Indices(indices)) => {
                indices.get(&u32::try_from(index).ok()?).map(Self::Layout)
            }
            Self::Shape(
                ComponentValueShape::List { item } | ComponentValueShape::KeyedList { item, .. },
            ) => Some(Self::Shape(item)),
            _ => None,
        }
    }

    pub(super) fn is_default(self, value: &ComponentValue) -> bool {
        match self {
            Self::Layout(Layout::Leaf(field)) => field
                .default_value()
                .is_some_and(|default| wire_eq(field.shape(), default, value)),
            _ => false,
        }
    }
}

fn wire_eq(shape: &ComponentValueShape, left: &ComponentValue, right: &ComponentValue) -> bool {
    use ComponentValueShape as Shape;
    if left == right {
        return true;
    }
    match (shape, left, right) {
        (Shape::Optional { value }, _, _) => wire_eq(value, left, right),
        (
            Shape::Scalar {
                numeric_domain: Some(ComponentNumericDomain::F32),
                ..
            },
            ComponentValue::F64(left),
            ComponentValue::F64(right),
        ) => left.get() as f32 == right.get() as f32,
        (Shape::Record { members }, ComponentValue::Map(left), ComponentValue::Map(right)) => {
            members.iter().all(
                |member| match (left.get(&member.name), right.get(&member.name)) {
                    (Some(left), Some(right)) => wire_eq(&member.shape, left, right),
                    (None, None) => true,
                    _ => false,
                },
            )
        }
        (Shape::Map { value }, ComponentValue::Map(left), ComponentValue::Map(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, left)| {
                    right
                        .get(key)
                        .is_some_and(|right| wire_eq(value, left, right))
                })
        }
        (
            Shape::List { item } | Shape::KeyedList { item, .. },
            ComponentValue::List(left),
            ComponentValue::List(right),
        ) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| wire_eq(item, left, right))
        }
        _ => false,
    }
}

pub(super) fn math_members(members: &[ComponentShapeMember]) -> Option<&'static [&'static str]> {
    const COORDINATES: &[&str] = &["x", "y", "z", "w"];
    const COLOR: &[&str] = &["r", "g", "b", "a"];
    if !(2..=4).contains(&members.len())
        || members.iter().any(|member| {
            !matches!(
                member.shape,
                ComponentValueShape::Scalar {
                    numeric_domain: Some(ComponentNumericDomain::F32),
                    ..
                }
            )
        })
    {
        return None;
    }
    [&COORDINATES[..members.len()], COLOR]
        .into_iter()
        .find(|names| {
            names.len() == members.len()
                && names
                    .iter()
                    .all(|name| members.iter().any(|member| member.name == *name))
        })
}

fn plain_identifier(key: &str) -> bool {
    let mut bytes = key.bytes();
    bytes
        .next()
        .is_some_and(|byte| byte == b'_' || byte.is_ascii_alphabetic())
        && bytes.all(|byte| byte == b'_' || byte.is_ascii_alphanumeric())
}
