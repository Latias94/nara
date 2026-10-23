//! Backend-free component values used by scene, prefab, patch, and schema code.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Display, Formatter},
};

use nara_identity::EntityReference;

use crate::{
    codec::ComponentCodecError,
    path::{
        ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment, component_path_with,
    },
    schema::ComponentValueKind,
};

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

/// Deterministic structural cost for a [`ComponentValue`].
///
/// Nodes bound container/object overhead while logical bytes bound owned scalar and map-key
/// payload. The accounting is independent of a serializer and uses saturating arithmetic so it
/// remains safe for programmatically constructed values.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ComponentValueCost {
    nodes: usize,
    logical_bytes: usize,
}

impl ComponentValueCost {
    pub const ZERO: Self = Self {
        nodes: 0,
        logical_bytes: 0,
    };

    #[must_use]
    pub const fn nodes(self) -> usize {
        self.nodes
    }

    #[must_use]
    pub const fn logical_bytes(self) -> usize {
        self.logical_bytes
    }

    #[must_use]
    pub const fn saturating_add(self, other: Self) -> Self {
        Self {
            nodes: self.nodes.saturating_add(other.nodes),
            logical_bytes: self.logical_bytes.saturating_add(other.logical_bytes),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "type",
        content = "value",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
pub enum ComponentValue {
    Null,
    Bool(bool),
    I64(i64),
    U64(u64),
    F64(ComponentFloat),
    String(String),
    List(Vec<ComponentValue>),
    Map(BTreeMap<String, ComponentValue>),
    #[cfg_attr(feature = "serde", serde(rename = "entity_ref"))]
    EntityReference(EntityReference),
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
    pub fn cost(&self) -> ComponentValueCost {
        let mut cost = ComponentValueCost::ZERO;
        self.cost_up_to_impl(usize::MAX, usize::MAX, &mut cost);
        cost
    }

    /// Returns the same saturating cost as [`Self::cost`] if both inclusive ceilings hold.
    ///
    /// Returns `None` as soon as either ceiling is provably exceeded, including from a
    /// container's known minimum node count. Zero ceilings are valid: every value costs at
    /// least one node, while nulls, empty containers, and empty strings can have zero logical
    /// bytes. Traversal borrows the value and keeps one iterator frame per open container.
    #[must_use]
    pub fn cost_up_to(
        &self,
        max_nodes: usize,
        max_logical_bytes: usize,
    ) -> Option<ComponentValueCost> {
        let mut cost = ComponentValueCost::ZERO;
        self.cost_up_to_impl(max_nodes, max_logical_bytes, &mut cost)
            .then_some(cost)
    }

    fn cost_up_to_impl(
        &self,
        max_nodes: usize,
        max_logical_bytes: usize,
        cost: &mut ComponentValueCost,
    ) -> bool {
        const PERSISTENT_RUNTIME_ID_BYTES: usize = 16;

        enum Frame<'a> {
            List(std::slice::Iter<'a, ComponentValue>),
            Map(std::collections::btree_map::Iter<'a, String, ComponentValue>),
        }

        if max_nodes == 0 {
            return false;
        }

        let mut pending = Vec::new();
        let mut current = Some((0_usize, self));
        // Count visited nodes and all known children, including unvisited ancestor siblings.
        let mut minimum_nodes = 1_usize;
        while let Some((key_bytes, value)) = current {
            cost.nodes = cost.nodes.saturating_add(1);
            cost.logical_bytes = cost.logical_bytes.saturating_add(key_bytes);
            let (logical_bytes, children) = match value {
                Self::Null => (0, 0),
                Self::Bool(_) => (1, 0),
                Self::I64(_) | Self::U64(_) | Self::F64(_) => (8, 0),
                Self::String(value) => (value.len(), 0),
                Self::List(values) => (0, values.len()),
                Self::Map(values) => (0, values.len()),
                Self::EntityReference(EntityReference::SceneLocal { entity }) => {
                    (entity.as_str().len(), 0)
                }
                Self::EntityReference(EntityReference::Persistent { entity }) => (
                    entity
                        .namespace
                        .as_str()
                        .len()
                        .saturating_add(PERSISTENT_RUNTIME_ID_BYTES),
                    0,
                ),
            };
            cost.logical_bytes = cost.logical_bytes.saturating_add(logical_bytes);
            minimum_nodes = minimum_nodes.saturating_add(children);
            if minimum_nodes > max_nodes || cost.logical_bytes > max_logical_bytes {
                return false;
            }

            if children != 0 {
                match value {
                    Self::List(values) => pending.push(Frame::List(values.iter())),
                    Self::Map(values) => pending.push(Frame::Map(values.iter())),
                    _ => {}
                }
            }

            current = loop {
                let next = match pending.last_mut() {
                    Some(Frame::List(values)) => values.next().map(|value| (0, value)),
                    Some(Frame::Map(values)) => {
                        values.next().map(|(key, value)| (key.len(), value))
                    }
                    None => break None,
                };
                if next.is_some() {
                    break next;
                }
                pending.pop();
            };
        }
        true
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
            Self::EntityReference(_) => ComponentValueKind::EntityRef,
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

    pub fn field_entity_reference(
        &self,
        field: &str,
    ) -> Result<&EntityReference, ComponentCodecError> {
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
    pub const fn as_entity_reference(&self) -> Option<&EntityReference> {
        match self {
            Self::EntityReference(reference) => Some(reference),
            _ => None,
        }
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

#[cfg(test)]
mod cost_traversal_tests {
    use super::{ComponentValue, ComponentValueCost};

    #[test]
    fn wide_containers_reject_before_visiting_children() {
        let width = 1_024;
        let values = [
            ComponentValue::List(vec![ComponentValue::Null; width]),
            ComponentValue::map((0..width).map(|index| (index.to_string(), ComponentValue::Null))),
        ];

        for value in values {
            let mut visited_cost = ComponentValueCost::ZERO;
            assert!(!value.cost_up_to_impl(0, usize::MAX, &mut visited_cost));
            assert_eq!(visited_cost.nodes(), 0);
            assert!(!value.cost_up_to_impl(width, usize::MAX, &mut visited_cost));
            assert_eq!(visited_cost.nodes(), 1);
        }
    }

    #[test]
    fn unbounded_cost_accumulation_preserves_saturation() {
        let value = ComponentValue::List(vec![ComponentValue::Bool(true), ComponentValue::U64(7)]);
        let mut cost = ComponentValueCost {
            nodes: usize::MAX - 1,
            logical_bytes: usize::MAX - 1,
        };

        assert!(value.cost_up_to_impl(usize::MAX, usize::MAX, &mut cost));
        assert_eq!(cost.nodes(), usize::MAX);
        assert_eq!(cost.logical_bytes(), usize::MAX);
    }

    #[test]
    fn minimum_nodes_include_unvisited_ancestor_siblings() {
        let value = ComponentValue::List(vec![
            ComponentValue::List(vec![ComponentValue::Null; 3]),
            ComponentValue::Null,
        ]);

        let mut visited_cost = ComponentValueCost::ZERO;
        assert!(!value.cost_up_to_impl(5, usize::MAX, &mut visited_cost));
        assert_eq!(visited_cost.nodes(), 2);
    }

    #[test]
    fn logical_byte_rejection_does_not_visit_later_siblings() {
        let value = ComponentValue::List(vec![
            ComponentValue::String("too large".to_owned()),
            ComponentValue::List(vec![ComponentValue::Null; 1_024]),
        ]);

        let mut visited_cost = ComponentValueCost::ZERO;
        assert!(!value.cost_up_to_impl(usize::MAX, 8, &mut visited_cost));
        assert_eq!(visited_cost.nodes(), 2);
    }

    #[test]
    fn map_key_rejection_does_not_visit_child_container_contents() {
        let value = ComponentValue::map([(
            "too large",
            ComponentValue::List(vec![ComponentValue::Null; 1_024]),
        )]);

        let mut visited_cost = ComponentValueCost::ZERO;
        assert!(!value.cost_up_to_impl(usize::MAX, 8, &mut visited_cost));
        assert_eq!(visited_cost.nodes(), 2);
    }
}
