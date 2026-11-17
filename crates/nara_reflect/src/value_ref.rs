//! Borrowed reads of explicit values and already selected default insertions.

mod cost;
#[cfg(feature = "serde")]
mod wire;

use std::{collections::btree_map, fmt, iter::Peekable};

use crate::{
    ComponentCodecError, ComponentFieldPath, ComponentFieldPathError, ComponentFieldPathSegment,
    ComponentValue, ComponentValueKind, EntityReference, effective::frozen::FrozenDefaultNode,
};

/// An immutable component value borrowed from its original admitted owner.
///
/// A read can cover an ordinary value or frozen explicit insertions over retained data. It does
/// not consult a Schema, own a backing allocation or extend an owner's lifetime. Use
/// [`Self::to_owned`] only at an explicitly admitted independent ownership or mutation boundary.
#[derive(Clone, Copy)]
pub struct ComponentValueRef<'a> {
    value: Option<&'a ComponentValue>,
    defaults: Option<&'a FrozenDefaultNode>,
}

impl<'a> From<&'a ComponentValue> for ComponentValueRef<'a> {
    fn from(value: &'a ComponentValue) -> Self {
        Self {
            value: Some(value),
            defaults: None,
        }
    }
}

impl<'a> ComponentValueRef<'a> {
    pub(crate) fn with_defaults(
        value: Option<&'a ComponentValue>,
        defaults: &'a FrozenDefaultNode,
    ) -> Self {
        match defaults {
            FrozenDefaultNode::Value(value) => Self::from(value),
            _ => Self {
                value,
                defaults: Some(defaults),
            },
        }
    }

    /// Borrows the unchanged owning subtree when this read has no inserted descendants.
    #[must_use]
    pub fn as_value(self) -> Option<&'a ComponentValue> {
        self.defaults.is_none().then_some(self.value).flatten()
    }

    #[must_use]
    pub fn kind(self) -> ComponentValueKind {
        match self.defaults {
            Some(FrozenDefaultNode::Map { .. }) => ComponentValueKind::Map,
            Some(FrozenDefaultNode::List { .. }) => ComponentValueKind::List,
            Some(FrozenDefaultNode::Value(value)) => value.kind(),
            None => self
                .value
                .expect("a borrowed value retains its original node")
                .kind(),
        }
    }

    #[must_use]
    pub fn get(self, field: &str) -> Option<Self> {
        self.as_map()?.get(field)
    }

    pub fn field(self, field: &str) -> Result<Self, ComponentCodecError> {
        self.get(field)
            .ok_or_else(|| ComponentCodecError::missing_field(field))
    }

    pub fn get_path(self, path: &ComponentFieldPath) -> Result<Self, ComponentFieldPathError> {
        let mut current = self;
        let mut visited = Vec::new();
        for segment in path.segments() {
            match segment {
                ComponentFieldPathSegment::Field(field) => {
                    let fields =
                        current
                            .as_map()
                            .ok_or_else(|| ComponentFieldPathError::ExpectedMap {
                                path: ComponentFieldPath::new(visited.iter().cloned()),
                            })?;
                    current =
                        fields
                            .get(field)
                            .ok_or_else(|| ComponentFieldPathError::MissingField {
                                path: crate::path::component_path_with(&visited, segment),
                                field: field.clone(),
                            })?;
                }
                ComponentFieldPathSegment::Index(index) => {
                    let items =
                        current
                            .as_list()
                            .ok_or_else(|| ComponentFieldPathError::ExpectedList {
                                path: ComponentFieldPath::new(visited.iter().cloned()),
                            })?;
                    current = items
                        .get(usize::try_from(*index).unwrap_or(usize::MAX))
                        .ok_or_else(|| ComponentFieldPathError::IndexOutOfBounds {
                            path: crate::path::component_path_with(&visited, segment),
                            index: *index,
                            len: items.len(),
                        })?;
                }
            }
            visited.push(segment.clone());
        }
        Ok(current)
    }

    #[must_use]
    pub fn as_bool(self) -> Option<bool> {
        self.as_value()?.as_bool()
    }

    #[must_use]
    pub fn as_i64(self) -> Option<i64> {
        self.as_value()?.as_i64()
    }

    #[must_use]
    pub fn as_u64(self) -> Option<u64> {
        self.as_value()?.as_u64()
    }

    #[must_use]
    pub fn as_f64(self) -> Option<f64> {
        self.as_value()?.as_f64()
    }

    #[must_use]
    pub fn as_str(self) -> Option<&'a str> {
        self.as_value()?.as_str()
    }

    #[must_use]
    pub fn as_entity_reference(self) -> Option<&'a EntityReference> {
        self.as_value()?.as_entity_reference()
    }

    pub fn field_bool(self, field: &str) -> Result<bool, ComponentCodecError> {
        self.field(field)?
            .as_bool()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "bool"))
    }

    pub fn field_i64(self, field: &str) -> Result<i64, ComponentCodecError> {
        self.field(field)?
            .as_i64()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "i64"))
    }

    pub fn field_u64(self, field: &str) -> Result<u64, ComponentCodecError> {
        self.field(field)?
            .as_u64()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "u64"))
    }

    pub fn field_f64(self, field: &str) -> Result<f64, ComponentCodecError> {
        self.field(field)?
            .as_f64()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "finite float"))
    }

    pub fn field_str(self, field: &str) -> Result<&'a str, ComponentCodecError> {
        self.field(field)?
            .as_str()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "string"))
    }

    pub fn field_entity_reference(
        self,
        field: &str,
    ) -> Result<&'a EntityReference, ComponentCodecError> {
        self.field(field)?
            .as_entity_reference()
            .ok_or_else(|| ComponentCodecError::invalid_field(field, "entity reference"))
    }

    #[must_use]
    pub fn as_map(self) -> Option<ComponentMapRef<'a>> {
        let raw = match self.value {
            Some(ComponentValue::Map(fields)) => Some(fields),
            None => None,
            _ => return None,
        };
        let (defaults, len) = match self.defaults {
            Some(FrozenDefaultNode::Map { children, len }) => (Some(children), *len),
            None => (None, raw?.len()),
            _ => return None,
        };
        Some(ComponentMapRef { raw, defaults, len })
    }

    #[must_use]
    pub fn as_list(self) -> Option<ComponentListRef<'a>> {
        let raw = match self.value {
            Some(ComponentValue::List(items)) => Some(items.as_slice()),
            None => None,
            _ => return None,
        };
        let (defaults, len) = match self.defaults {
            Some(FrozenDefaultNode::List { children, len }) => (Some(children), *len),
            None => (None, raw?.len()),
            _ => return None,
        };
        Some(ComponentListRef { raw, defaults, len })
    }

    /// Deeply materializes ordinary owned data without retaining its original backing.
    ///
    /// The caller admits the complete allocation before using this ownership operation.
    #[must_use]
    pub fn to_owned(self) -> ComponentValue {
        if let Some(value) = self.as_value() {
            return value.clone();
        }
        if let Some(fields) = self.as_map() {
            return ComponentValue::map(
                fields.iter().map(|(name, value)| (name, value.to_owned())),
            );
        }
        let items = self
            .as_list()
            .expect("a frozen insertion root is a Map or List");
        ComponentValue::List(items.iter().map(Self::to_owned).collect())
    }
}

/// A borrowed Map with canonical key order and no independently retained storage.
#[derive(Clone, Copy)]
pub struct ComponentMapRef<'a> {
    raw: Option<&'a std::collections::BTreeMap<String, ComponentValue>>,
    defaults: Option<&'a std::collections::BTreeMap<String, FrozenDefaultNode>>,
    len: usize,
}

impl<'a> ComponentMapRef<'a> {
    #[must_use]
    pub const fn len(self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn get(self, name: &str) -> Option<ComponentValueRef<'a>> {
        let raw = self.raw.and_then(|fields| fields.get(name));
        match self.defaults.and_then(|children| children.get(name)) {
            Some(node) => Some(ComponentValueRef::with_defaults(raw, node)),
            None => raw.map(ComponentValueRef::from),
        }
    }

    #[must_use]
    pub fn iter(self) -> ComponentMapIter<'a> {
        ComponentMapIter {
            raw: self.raw.map(|fields| fields.iter().peekable()),
            defaults: self.defaults.map(|children| children.iter().peekable()),
            remaining: self.len,
        }
    }

    #[must_use]
    pub fn keys(self) -> impl ExactSizeIterator<Item = &'a str> + 'a {
        self.iter().map(|(name, _)| name)
    }

    #[must_use]
    pub fn values(self) -> impl ExactSizeIterator<Item = ComponentValueRef<'a>> + 'a {
        self.iter().map(|(_, value)| value)
    }
}

impl<'a> IntoIterator for ComponentMapRef<'a> {
    type Item = (&'a str, ComponentValueRef<'a>);
    type IntoIter = ComponentMapIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &ComponentMapRef<'a> {
    type Item = (&'a str, ComponentValueRef<'a>);
    type IntoIter = ComponentMapIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterates canonical effective Map entries without allocating or flattening them.
pub struct ComponentMapIter<'a> {
    raw: Option<Peekable<btree_map::Iter<'a, String, ComponentValue>>>,
    defaults: Option<Peekable<btree_map::Iter<'a, String, FrozenDefaultNode>>>,
    remaining: usize,
}

impl<'a> Iterator for ComponentMapIter<'a> {
    type Item = (&'a str, ComponentValueRef<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        let raw = self.raw.as_mut().and_then(|fields| fields.peek().copied());
        let default = self
            .defaults
            .as_mut()
            .and_then(|fields| fields.peek().copied());
        let result = match (raw, default) {
            (Some((raw_name, _)), Some((default_name, _))) if raw_name == default_name => {
                let (name, value) = self.raw.as_mut()?.next()?;
                let (_, defaults) = self.defaults.as_mut()?.next()?;
                (
                    name.as_str(),
                    ComponentValueRef::with_defaults(Some(value), defaults),
                )
            }
            (Some((raw_name, _)), Some((default_name, _))) if raw_name < default_name => {
                let (name, value) = self.raw.as_mut()?.next()?;
                (name.as_str(), ComponentValueRef::from(value))
            }
            (Some(_), None) => {
                let (name, value) = self.raw.as_mut()?.next()?;
                (name.as_str(), ComponentValueRef::from(value))
            }
            (_, Some(_)) => {
                let (name, node) = self.defaults.as_mut()?.next()?;
                (name.as_str(), ComponentValueRef::with_defaults(None, node))
            }
            (None, None) => return None,
        };
        self.remaining -= 1;
        Some(result)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl ExactSizeIterator for ComponentMapIter<'_> {}
impl std::iter::FusedIterator for ComponentMapIter<'_> {}

/// A borrowed List, including already admitted contiguous default suffixes.
#[derive(Clone, Copy)]
pub struct ComponentListRef<'a> {
    raw: Option<&'a [ComponentValue]>,
    defaults: Option<&'a std::collections::BTreeMap<u32, FrozenDefaultNode>>,
    len: usize,
}

impl<'a> ComponentListRef<'a> {
    #[must_use]
    pub const fn len(self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn get(self, index: usize) -> Option<ComponentValueRef<'a>> {
        if index >= self.len {
            return None;
        }
        let raw = self.raw.and_then(|items| items.get(index));
        let node = self.defaults.and_then(|children| {
            u32::try_from(index)
                .ok()
                .and_then(|index| children.get(&index))
        });
        match node {
            Some(node) => Some(ComponentValueRef::with_defaults(raw, node)),
            None => raw.map(ComponentValueRef::from),
        }
    }

    #[must_use]
    pub fn iter(self) -> ComponentListIter<'a> {
        ComponentListIter {
            list: self,
            index: 0,
        }
    }
}

impl<'a> IntoIterator for ComponentListRef<'a> {
    type Item = ComponentValueRef<'a>;
    type IntoIter = ComponentListIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &ComponentListRef<'a> {
    type Item = ComponentValueRef<'a>;
    type IntoIter = ComponentListIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Iterates effective List items without allocating or flattening them.
pub struct ComponentListIter<'a> {
    list: ComponentListRef<'a>,
    index: usize,
}

impl<'a> Iterator for ComponentListIter<'a> {
    type Item = ComponentValueRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index == self.list.len {
            return None;
        }
        let value = self
            .list
            .get(self.index)
            .expect("an admitted frozen List has no sparse holes");
        self.index += 1;
        Some(value)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.list.len - self.index;
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for ComponentListIter<'_> {}
impl std::iter::FusedIterator for ComponentListIter<'_> {}

impl fmt::Debug for ComponentValueRef<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(value) = self.as_value() {
            return fmt::Debug::fmt(value, formatter);
        }
        if let Some(fields) = self.as_map() {
            return formatter.debug_map().entries(fields.iter()).finish();
        }
        formatter
            .debug_list()
            .entries(self.as_list().expect("a frozen root is a container").iter())
            .finish()
    }
}

impl<'a, 'b> PartialEq<ComponentValueRef<'b>> for ComponentValueRef<'a> {
    fn eq(&self, other: &ComponentValueRef<'b>) -> bool {
        match (self.as_value(), other.as_value()) {
            (Some(left), Some(right)) => left == right,
            _ if self.kind() != other.kind() => false,
            _ => match self.kind() {
                ComponentValueKind::Map => {
                    let left = self.as_map().expect("Map kind checked");
                    let right = other.as_map().expect("Map kind checked");
                    left.len() == right.len() && left.iter().eq(right.iter())
                }
                ComponentValueKind::List => {
                    let left = self.as_list().expect("List kind checked");
                    let right = other.as_list().expect("List kind checked");
                    left.len() == right.len() && left.iter().eq(right.iter())
                }
                _ => false,
            },
        }
    }
}

impl PartialEq<ComponentValue> for ComponentValueRef<'_> {
    fn eq(&self, other: &ComponentValue) -> bool {
        *self == ComponentValueRef::from(other)
    }
}

impl PartialEq<&ComponentValue> for ComponentValueRef<'_> {
    fn eq(&self, other: &&ComponentValue) -> bool {
        *self == ComponentValueRef::from(*other)
    }
}

impl PartialEq<ComponentValueRef<'_>> for ComponentValue {
    fn eq(&self, other: &ComponentValueRef<'_>) -> bool {
        ComponentValueRef::from(self) == *other
    }
}
