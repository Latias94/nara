use std::{
    collections::{BTreeMap, btree_map},
    slice,
};

use nara_core::EntityUuid;

use crate::{
    ComponentCodecError, ComponentFieldPath, ComponentFieldPathError, ComponentValue,
    ComponentValueKind,
};

/// A borrowed component value used by field codecs.
#[derive(Debug, Clone, Copy)]
pub struct ComponentValueRef<'a>(&'a ComponentValue);

impl<'a> From<&'a ComponentValue> for ComponentValueRef<'a> {
    fn from(value: &'a ComponentValue) -> Self {
        Self(value)
    }
}

impl<'a> ComponentValueRef<'a> {
    #[must_use]
    pub const fn as_value(self) -> &'a ComponentValue {
        self.0
    }

    #[must_use]
    pub const fn kind(self) -> ComponentValueKind {
        self.0.kind()
    }

    #[must_use]
    pub fn get(self, field: &str) -> Option<Self> {
        self.0.get(field).map(Self::from)
    }

    pub fn field(self, field: &str) -> Result<Self, ComponentCodecError> {
        self.0.field(field).map(Self::from)
    }

    pub fn get_path(self, path: &ComponentFieldPath) -> Result<Self, ComponentFieldPathError> {
        self.0.get_path(path).map(Self::from)
    }

    #[must_use]
    pub fn as_bool(self) -> Option<bool> {
        self.0.as_bool()
    }

    #[must_use]
    pub fn as_i64(self) -> Option<i64> {
        self.0.as_i64()
    }

    #[must_use]
    pub fn as_u64(self) -> Option<u64> {
        self.0.as_u64()
    }

    #[must_use]
    pub fn as_f64(self) -> Option<f64> {
        self.0.as_f64()
    }

    #[must_use]
    pub fn as_str(self) -> Option<&'a str> {
        self.0.as_str()
    }

    #[must_use]
    pub fn as_entity_reference(self) -> Option<&'a EntityUuid> {
        self.0.as_entity_reference()
    }

    #[must_use]
    pub fn as_map(self) -> Option<ComponentMapRef<'a>> {
        match self.0 {
            ComponentValue::Map(values) => Some(ComponentMapRef(values)),
            _ => None,
        }
    }

    #[must_use]
    pub fn as_list(self) -> Option<ComponentListRef<'a>> {
        match self.0 {
            ComponentValue::List(values) => Some(ComponentListRef(values)),
            _ => None,
        }
    }

    #[must_use]
    pub fn to_owned(self) -> ComponentValue {
        self.0.clone()
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ComponentMapRef<'a>(&'a BTreeMap<String, ComponentValue>);

impl<'a> ComponentMapRef<'a> {
    #[must_use]
    pub fn len(self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn get(self, key: &str) -> Option<ComponentValueRef<'a>> {
        self.0.get(key).map(ComponentValueRef::from)
    }

    pub fn keys(self) -> impl ExactSizeIterator<Item = &'a str> {
        self.0.keys().map(String::as_str)
    }

    pub fn iter(self) -> ComponentMapIter<'a> {
        ComponentMapIter(self.0.iter())
    }
}

#[derive(Clone)]
pub struct ComponentMapIter<'a>(btree_map::Iter<'a, String, ComponentValue>);

impl<'a> Iterator for ComponentMapIter<'a> {
    type Item = (&'a str, ComponentValueRef<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        self.0
            .next()
            .map(|(key, value)| (key.as_str(), value.into()))
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl ExactSizeIterator for ComponentMapIter<'_> {}

#[derive(Debug, Clone, Copy)]
pub struct ComponentListRef<'a>(&'a [ComponentValue]);

impl<'a> ComponentListRef<'a> {
    #[must_use]
    pub fn len(self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn get(self, index: usize) -> Option<ComponentValueRef<'a>> {
        self.0.get(index).map(ComponentValueRef::from)
    }

    pub fn iter(self) -> ComponentListIter<'a> {
        ComponentListIter(self.0.iter())
    }
}

#[derive(Clone)]
pub struct ComponentListIter<'a>(slice::Iter<'a, ComponentValue>);

impl<'a> Iterator for ComponentListIter<'a> {
    type Item = ComponentValueRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        self.0.next().map(ComponentValueRef::from)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        self.0.size_hint()
    }
}

impl ExactSizeIterator for ComponentListIter<'_> {}
