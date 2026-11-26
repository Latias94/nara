//! Backend-free values for component fields.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_core::{AssetId, EntityUuid};

use crate::{
    codec::ComponentCodecError,
    path::{
        ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment, component_path_with,
    },
    schema::ComponentValueKind,
};

#[cfg(feature = "serde")]
mod wire;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ComponentFloat(f64);

impl ComponentFloat {
    pub fn new(value: f64) -> Result<Self, ComponentValueError> {
        if value.is_finite() {
            Ok(Self(value))
        } else {
            Err(ComponentValueError::NonFiniteFloat)
        }
    }

    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for ComponentFloat {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_f64(self.0)
    }
}

#[cfg(feature = "serde")]
impl<'de> serde::Deserialize<'de> for ComponentFloat {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = f64::deserialize(deserializer)?;
        Self::new(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ComponentValue {
    Null,
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(ComponentFloat),
    String(String),
    List(Vec<ComponentValue>),
    Map(BTreeMap<String, ComponentValue>),
    AssetReference(AssetId),
    EntityReference(EntityUuid),
    PackedU16(Vec<u16>),
    PackedU32(Vec<u32>),
    PackedI32(Vec<i32>),
    PackedF32(Vec<f32>),
    Bytes(Vec<u8>),
}

impl ComponentValue {
    pub fn f64(value: f64) -> Result<Self, ComponentValueError> {
        Ok(Self::F64(ComponentFloat::new(value)?))
    }

    #[must_use]
    pub fn map(entries: impl IntoIterator<Item = (impl Into<String>, ComponentValue)>) -> Self {
        Self::Map(
            entries
                .into_iter()
                .map(|(key, value)| (key.into(), value))
                .collect(),
        )
    }

    #[must_use]
    pub fn get(&self, field: &str) -> Option<&ComponentValue> {
        match self {
            Self::Map(fields) => fields.get(field),
            _ => None,
        }
    }

    pub fn field(&self, field: &str) -> Result<&ComponentValue, ComponentCodecError> {
        self.get(field)
            .ok_or_else(|| ComponentCodecError::missing_field(field))
    }

    #[must_use]
    pub const fn kind(&self) -> ComponentValueKind {
        match self {
            Self::Null => ComponentValueKind::Null,
            Self::Bool(_) => ComponentValueKind::Bool,
            Self::I64(_) => ComponentValueKind::I64,
            Self::U64(_) => ComponentValueKind::U64,
            Self::F64(_) => ComponentValueKind::F64,
            Self::String(_) => ComponentValueKind::String,
            Self::List(_) => ComponentValueKind::List,
            Self::Map(_) => ComponentValueKind::Map,
            Self::AssetReference(_) => ComponentValueKind::AssetRef,
            Self::EntityReference(_) => ComponentValueKind::EntityRef,
            Self::PackedU16(_) => ComponentValueKind::PackedU16,
            Self::PackedU32(_) => ComponentValueKind::PackedU32,
            Self::PackedI32(_) => ComponentValueKind::PackedI32,
            Self::PackedF32(_) => ComponentValueKind::PackedF32,
            Self::Bytes(_) => ComponentValueKind::Bytes,
        }
    }

    pub fn get_path(
        &self,
        path: &ComponentFieldPath,
    ) -> Result<&ComponentValue, ComponentFieldPathError> {
        let mut current = self;
        let mut visited = Vec::new();

        for segment in path.segments() {
            match segment {
                ComponentFieldPathSegment::Field(field) => {
                    let child_path = component_path_with(&visited, segment);
                    let ComponentValue::Map(fields) = current else {
                        return Err(ComponentFieldPathError::ExpectedMap {
                            path: ComponentFieldPath::new(visited.iter().cloned()),
                        });
                    };
                    current =
                        fields
                            .get(field)
                            .ok_or_else(|| ComponentFieldPathError::MissingField {
                                path: child_path,
                                field: field.clone(),
                            })?;
                }
                ComponentFieldPathSegment::Index(index) => {
                    let child_path = component_path_with(&visited, segment);
                    let ComponentValue::List(items) = current else {
                        return Err(ComponentFieldPathError::ExpectedList {
                            path: ComponentFieldPath::new(visited.iter().cloned()),
                        });
                    };
                    let requested_index = *index;
                    let item_index = usize::try_from(requested_index).unwrap_or(usize::MAX);
                    current = items.get(item_index).ok_or({
                        ComponentFieldPathError::IndexOutOfBounds {
                            path: child_path,
                            index: requested_index,
                            len: items.len(),
                        }
                    })?;
                }
            }
            visited.push(segment.clone());
        }

        Ok(current)
    }

    pub fn get_path_mut(
        &mut self,
        path: &ComponentFieldPath,
    ) -> Result<&mut ComponentValue, ComponentFieldPathError> {
        self.get_path_mut_segments(path.segments())
    }

    /// Reads an owned field value, including a scalar addressed inside a packed block.
    pub fn read_path(&self, path: &ComponentFieldPath) -> Result<Self, ComponentFieldPathError> {
        if let Some((ComponentFieldPathSegment::Index(index), parent)) =
            path.segments().split_last()
        {
            let parent = ComponentFieldPath::new(parent.iter().cloned());
            let value = self.get_path(&parent)?;
            if let Some(len) = value.packed_len() {
                let item = packed_index(path, *index, len)?;
                match value {
                    Self::PackedU16(items) => return Ok(Self::U64(items[item].into())),
                    Self::PackedU32(items) => return Ok(Self::U64(items[item].into())),
                    Self::PackedI32(items) => return Ok(Self::I64(items[item].into())),
                    Self::PackedF32(items) => {
                        return Self::f64(items[item].into()).map_err(|_| {
                            ComponentFieldPathError::InvalidPackedElement { path: path.clone() }
                        });
                    }
                    Self::Bytes(items) => return Ok(Self::U64(items[item].into())),
                    _ => {}
                }
            }
        }
        self.get_path(path).cloned()
    }

    pub fn set_path(
        &mut self,
        path: &ComponentFieldPath,
        value: ComponentValue,
    ) -> Result<Option<ComponentValue>, ComponentFieldPathError> {
        if path.is_empty() {
            return Ok(Some(std::mem::replace(self, value)));
        }
        let (last, parent_segments) = path
            .segments()
            .split_last()
            .ok_or(ComponentFieldPathError::EmptyPath)?;
        let parent = self.get_path_mut_segments(parent_segments)?;
        let parent_path = ComponentFieldPath::new(parent_segments.iter().cloned());

        match last {
            ComponentFieldPathSegment::Field(field) => {
                let ComponentValue::Map(fields) = parent else {
                    return Err(ComponentFieldPathError::ExpectedMap { path: parent_path });
                };
                Ok(fields.insert(field.clone(), value))
            }
            ComponentFieldPathSegment::Index(index) => {
                if let Some(result) = parent.replace_packed(path, *index, &value) {
                    return result.map(Some);
                }
                let ComponentValue::List(items) = parent else {
                    return Err(ComponentFieldPathError::ExpectedList { path: parent_path });
                };
                let target_path = component_path_with(parent_segments, last);
                let requested_index = *index;
                let item_index = usize::try_from(requested_index).unwrap_or(usize::MAX);
                if item_index >= items.len() {
                    return Err(ComponentFieldPathError::IndexOutOfBounds {
                        path: target_path,
                        index: requested_index,
                        len: items.len(),
                    });
                }
                Ok(Some(std::mem::replace(&mut items[item_index], value)))
            }
        }
    }

    pub fn replace_path(
        &mut self,
        path: &ComponentFieldPath,
        value: ComponentValue,
    ) -> Result<ComponentValue, ComponentFieldPathError> {
        if path.is_empty() {
            return Ok(std::mem::replace(self, value));
        }
        let (last, parent_segments) = path
            .segments()
            .split_last()
            .ok_or(ComponentFieldPathError::EmptyPath)?;
        let parent = self.get_path_mut_segments(parent_segments)?;
        let parent_path = ComponentFieldPath::new(parent_segments.iter().cloned());

        match last {
            ComponentFieldPathSegment::Field(field) => {
                let target_path = component_path_with(parent_segments, last);
                let ComponentValue::Map(fields) = parent else {
                    return Err(ComponentFieldPathError::ExpectedMap { path: parent_path });
                };
                if !fields.contains_key(field) {
                    return Err(ComponentFieldPathError::MissingField {
                        path: target_path,
                        field: field.clone(),
                    });
                }
                Ok(fields
                    .insert(field.clone(), value)
                    .expect("field presence checked before replacement"))
            }
            ComponentFieldPathSegment::Index(index) => {
                if let Some(result) = parent.replace_packed(path, *index, &value) {
                    return result;
                }
                let ComponentValue::List(items) = parent else {
                    return Err(ComponentFieldPathError::ExpectedList { path: parent_path });
                };
                let target_path = component_path_with(parent_segments, last);
                let requested_index = *index;
                let item_index = usize::try_from(requested_index).unwrap_or(usize::MAX);
                if item_index >= items.len() {
                    return Err(ComponentFieldPathError::IndexOutOfBounds {
                        path: target_path,
                        index: requested_index,
                        len: items.len(),
                    });
                }
                Ok(std::mem::replace(&mut items[item_index], value))
            }
        }
    }

    pub fn remove_path(
        &mut self,
        path: &ComponentFieldPath,
    ) -> Result<ComponentValue, ComponentFieldPathError> {
        let (last, parent_segments) = path
            .segments()
            .split_last()
            .ok_or(ComponentFieldPathError::EmptyPath)?;
        let parent = self.get_path_mut_segments(parent_segments)?;
        let parent_path = ComponentFieldPath::new(parent_segments.iter().cloned());

        match last {
            ComponentFieldPathSegment::Field(field) => {
                let target_path = component_path_with(parent_segments, last);
                let ComponentValue::Map(fields) = parent else {
                    return Err(ComponentFieldPathError::ExpectedMap { path: parent_path });
                };
                fields
                    .remove(field)
                    .ok_or(ComponentFieldPathError::MissingField {
                        path: target_path,
                        field: field.clone(),
                    })
            }
            ComponentFieldPathSegment::Index(index) => {
                let ComponentValue::List(items) = parent else {
                    return Err(ComponentFieldPathError::ExpectedList { path: parent_path });
                };
                let target_path = component_path_with(parent_segments, last);
                let requested_index = *index;
                let item_index = usize::try_from(requested_index).unwrap_or(usize::MAX);
                if item_index >= items.len() {
                    return Err(ComponentFieldPathError::IndexOutOfBounds {
                        path: target_path,
                        index: requested_index,
                        len: items.len(),
                    });
                }
                Ok(items.remove(item_index))
            }
        }
    }

    fn get_path_mut_segments(
        &mut self,
        segments: &[ComponentFieldPathSegment],
    ) -> Result<&mut ComponentValue, ComponentFieldPathError> {
        let mut current = self;
        let mut visited = Vec::new();

        for segment in segments {
            match segment {
                ComponentFieldPathSegment::Field(field) => {
                    let child_path = component_path_with(&visited, segment);
                    let ComponentValue::Map(fields) = current else {
                        return Err(ComponentFieldPathError::ExpectedMap {
                            path: ComponentFieldPath::new(visited.iter().cloned()),
                        });
                    };
                    current = fields.get_mut(field).ok_or_else(|| {
                        ComponentFieldPathError::MissingField {
                            path: child_path,
                            field: field.clone(),
                        }
                    })?;
                }
                ComponentFieldPathSegment::Index(index) => {
                    let child_path = component_path_with(&visited, segment);
                    let ComponentValue::List(items) = current else {
                        return Err(ComponentFieldPathError::ExpectedList {
                            path: ComponentFieldPath::new(visited.iter().cloned()),
                        });
                    };
                    let requested_index = *index;
                    let item_index = usize::try_from(requested_index).unwrap_or(usize::MAX);
                    if item_index >= items.len() {
                        return Err(ComponentFieldPathError::IndexOutOfBounds {
                            path: child_path,
                            index: requested_index,
                            len: items.len(),
                        });
                    }
                    current = &mut items[item_index];
                }
            }
            visited.push(segment.clone());
        }

        Ok(current)
    }

    fn packed_len(&self) -> Option<usize> {
        match self {
            Self::PackedU16(items) => Some(items.len()),
            Self::PackedU32(items) => Some(items.len()),
            Self::PackedI32(items) => Some(items.len()),
            Self::PackedF32(items) => Some(items.len()),
            Self::Bytes(items) => Some(items.len()),
            _ => None,
        }
    }

    fn replace_packed(
        &mut self,
        path: &ComponentFieldPath,
        index: u32,
        value: &Self,
    ) -> Option<Result<Self, ComponentFieldPathError>> {
        let len = self.packed_len()?;
        let item = match packed_index(path, index, len) {
            Ok(item) => item,
            Err(error) => return Some(Err(error)),
        };
        let invalid = || ComponentFieldPathError::InvalidPackedElement { path: path.clone() };
        macro_rules! replace_integer {
            ($items:expr, $read:ident, $kind:ident, $native:ty) => {
                value
                    .$read()
                    .and_then(|value| <$native>::try_from(value).ok())
                    .ok_or_else(invalid)
                    .map(|value| Self::$kind(std::mem::replace(&mut $items[item], value).into()))
            };
        }
        Some(match self {
            Self::PackedU16(items) => replace_integer!(items, as_u64, U64, u16),
            Self::PackedU32(items) => replace_integer!(items, as_u64, U64, u32),
            Self::PackedI32(items) => replace_integer!(items, as_i64, I64, i32),
            Self::PackedF32(items) => value
                .as_f64()
                .map(|value| value as f32)
                .filter(|value| value.is_finite())
                .and_then(|value| {
                    let old = Self::f64(items[item].into()).ok()?;
                    items[item] = value;
                    Some(old)
                })
                .ok_or_else(invalid),
            Self::Bytes(items) => replace_integer!(items, as_u64, U64, u8),
            _ => return None,
        })
    }

    #[must_use]
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn field_bool(&self, field: &str) -> Result<bool, ComponentCodecError> {
        self.field(field)?
            .as_bool()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "bool"))
    }

    pub fn field_i64(&self, field: &str) -> Result<i64, ComponentCodecError> {
        self.field(field)?
            .as_i64()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "i64"))
    }

    pub fn field_u64(&self, field: &str) -> Result<u64, ComponentCodecError> {
        self.field(field)?
            .as_u64()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "u64"))
    }

    pub fn field_f64(&self, field: &str) -> Result<f64, ComponentCodecError> {
        self.field(field)?
            .as_f64()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "finite float"))
    }

    pub fn field_str(&self, field: &str) -> Result<&str, ComponentCodecError> {
        self.field(field)?
            .as_str()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "string"))
    }

    pub fn field_entity_reference(&self, field: &str) -> Result<&EntityUuid, ComponentCodecError> {
        self.field(field)?
            .as_entity_reference()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "entity reference"))
    }

    #[must_use]
    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::I64(value) => Some(*value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Self::U64(value) => Some(*value),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Self::F64(value) => Some(value.get()),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_entity_reference(&self) -> Option<&EntityUuid> {
        match self {
            Self::EntityReference(reference) => Some(reference),
            _ => None,
        }
    }

    #[must_use]
    pub const fn as_asset_reference(&self) -> Option<&AssetId> {
        match self {
            Self::AssetReference(reference) => Some(reference),
            _ => None,
        }
    }
}

fn packed_index(
    path: &ComponentFieldPath,
    index: u32,
    len: usize,
) -> Result<usize, ComponentFieldPathError> {
    match usize::try_from(index) {
        Ok(index) if index < len => Ok(index),
        _ => Err(ComponentFieldPathError::IndexOutOfBounds {
            path: path.clone(),
            index,
            len,
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ComponentValueError {
    NonFiniteFloat,
}

impl Display for ComponentValueError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteFloat => formatter.write_str("component float must be finite"),
        }
    }
}

impl Error for ComponentValueError {}
