use std::{collections::BTreeMap, fmt, mem::size_of};

use super::{ComponentFieldId, ComponentFieldPathSegment, ComponentPropertyKey};
use crate::registry::VALIDATION_BTREE_ENTRY_BYTES;

/// Positions collected during validation of one exact effective component value.
///
/// The source owner pairs this index with the same admitted Schema and field values. It may
/// reuse the index for identical values, including their selected defaults.
#[derive(Default, PartialEq, Eq)]
pub struct ComponentPropertyReadIndex {
    fields: BTreeMap<ComponentFieldId, KeyedListLocations>,
    retained_bytes: usize,
}

impl fmt::Debug for ComponentPropertyReadIndex {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ComponentPropertyReadIndex")
            .field("fields", &self.fields.len())
            .field("retained_bytes", &self.retained_bytes())
            .finish()
    }
}

impl ComponentPropertyReadIndex {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Modeled retained bytes including the index object and shared allocation overhead.
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        Self::ALLOCATION_BYTES.saturating_add(self.retained_bytes)
    }

    pub(crate) const ALLOCATION_BYTES: usize = size_of::<Self>() + 4 * size_of::<usize>();

    pub(crate) fn field_bytes(field: &ComponentFieldId) -> usize {
        KeyedListLocations::edge_bytes(field.as_str().len())
    }

    pub(crate) fn retain_field(&mut self, field: &ComponentFieldId, lists: KeyedListLocations) {
        if !lists.is_empty() {
            self.retained_bytes = self
                .retained_bytes
                .saturating_add(Self::field_bytes(field))
                .saturating_add(lists.retained_bytes);
            self.fields.insert(field.clone(), lists);
        }
    }

    pub(super) fn lists(&self, field: &ComponentFieldId) -> Option<&KeyedListLocations> {
        self.fields.get(field)
    }
}

#[derive(Default, PartialEq, Eq)]
pub(crate) struct KeyedListLocations {
    rows: BTreeMap<ComponentPropertyKey, usize>,
    fields: BTreeMap<String, Self>,
    items: BTreeMap<usize, Self>,
    retained_bytes: usize,
}

impl KeyedListLocations {
    pub(crate) fn edge_bytes(name_bytes: usize) -> usize {
        VALIDATION_BTREE_ENTRY_BYTES
            .saturating_add(size_of::<Self>())
            .saturating_add(name_bytes)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.fields.is_empty() && self.items.is_empty()
    }

    pub(crate) fn retain_child(&mut self, segment: &ComponentFieldPathSegment, child: Self) {
        let name_bytes = match segment {
            ComponentFieldPathSegment::Field(name) => name.len(),
            ComponentFieldPathSegment::Index(_) => 0,
        };
        self.retained_bytes = self
            .retained_bytes
            .saturating_add(Self::edge_bytes(name_bytes))
            .saturating_add(child.retained_bytes);
        match segment {
            ComponentFieldPathSegment::Field(name) => self.fields.insert(name.clone(), child),
            ComponentFieldPathSegment::Index(index) => self.items.insert(*index as usize, child),
        };
    }

    pub(crate) fn retain_rows(
        &mut self,
        rows: BTreeMap<ComponentPropertyKey, usize>,
        key_string_bytes: usize,
    ) {
        self.retained_bytes = self
            .retained_bytes
            .saturating_add(rows.len().saturating_mul(VALIDATION_BTREE_ENTRY_BYTES))
            .saturating_add(key_string_bytes);
        self.rows = rows;
    }

    pub(super) fn rows(&self) -> Option<&BTreeMap<ComponentPropertyKey, usize>> {
        (!self.rows.is_empty()).then_some(&self.rows)
    }

    pub(super) fn field(&self, name: &str) -> Option<&Self> {
        self.fields.get(name)
    }

    pub(super) fn item(&self, index: usize) -> Option<&Self> {
        self.items.get(&index)
    }
}
