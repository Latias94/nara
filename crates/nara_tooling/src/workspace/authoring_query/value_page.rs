use std::{
    collections::BTreeMap,
    ops::Bound::{Excluded, Unbounded},
    sync::Arc,
};

use nara_reflect::{
    ComponentFieldPath, ComponentFieldPathSegment, ComponentPropertyError,
    ComponentPropertyReadBudget, ComponentValue,
};
use nara_scene::{AuthoringPropertyValue, AuthoringReadError};

use super::{EditorWorkspace, property_component};
use crate::{
    EditorDocumentQueryRejection, EditorPropertyPage, EditorPropertyPageCursor,
    EditorPropertyPageData, EditorPropertyPageItem, EditorPropertyPagePosition,
    EditorPropertyPageRead, EditorPropertyPageValue, EditorPropertyReadLimits,
    authoring_query::{PropertyValuePagePosition, PropertyValuePageTarget},
};

impl EditorWorkspace {
    /// Reads one value window from the exact document basis, with independent scan and copy limits.
    /// Relative positions are read-only. Use the Schema selector to prepare a durable edit.
    pub fn read_document_property_page(
        &self,
        request: EditorPropertyPageRead,
    ) -> Result<EditorPropertyPage, EditorDocumentQueryRejection> {
        let (target, expected_kind, position, read_path) = match request.position {
            EditorPropertyPagePosition::Start { target, path } => (
                Arc::new(PropertyValuePageTarget {
                    property: target,
                    path,
                }),
                None,
                PropertyValuePagePosition::Start,
                None,
            ),
            EditorPropertyPagePosition::Continue(cursor) => {
                if cursor.owner != self.document_query_owner {
                    return Err(EditorDocumentQueryRejection::WrongWorkspace);
                }
                (
                    cursor.target,
                    Some(cursor.kind),
                    cursor.next,
                    Some(cursor.read_path),
                )
            }
        };
        let slot = self.resolve_document_query_slot(&target.property.basis, expected_kind)?;
        let metadata_bytes = target
            .property
            .admitted_metadata_bytes(request.limits.max_metadata_bytes())?;
        let mut reader = ValuePageReader {
            limits: request.limits,
            scan: request.limits.scan_budget(),
            nodes: 0,
            logical_bytes: 0,
        };
        let component = property_component(slot, &target.property)?;
        let resuming = read_path.is_some();
        let (value, read_path) = match read_path {
            Some(read_path) => (
                component.property_root_value(&target.property.property.root)?,
                read_path,
            ),
            None => {
                let (value, read_path) = component
                    .property_value_with_read_path(&target.property.property, &mut reader.scan)?;
                (value, Arc::new(read_path))
            }
        };
        let (value, next) = match value {
            AuthoringPropertyValue::Present(root) | AuthoringPropertyValue::Defaulted(root) => {
                let selected = if resuming {
                    reader.select(root, &read_path)?
                } else {
                    root
                };
                let selected = reader.select(selected, &target.path)?;
                let (data, next) = reader.page(selected, position)?;
                let value = match value {
                    AuthoringPropertyValue::Defaulted(_) => {
                        EditorPropertyPageValue::Defaulted(data)
                    }
                    _ => EditorPropertyPageValue::Present(data),
                };
                (value, next)
            }
            AuthoringPropertyValue::Missing => {
                if resuming {
                    return Err(EditorDocumentQueryRejection::InvalidContinuation);
                }
                if !target.path.is_empty() {
                    return Err(EditorDocumentQueryRejection::InvalidValuePath { step: 0 });
                }
                (EditorPropertyPageValue::Missing, None)
            }
        };
        let next = next.map(|next| EditorPropertyPageCursor {
            owner: self.document_query_owner.clone(),
            kind: slot.kind(),
            target: Arc::clone(&target),
            read_path,
            next,
        });
        Ok(EditorPropertyPage {
            target,
            value,
            metadata_bytes,
            value_nodes: reader.nodes,
            value_logical_bytes: reader.logical_bytes,
            scanned_entries: reader.scan.scanned_entries(),
            scanned_logical_bytes: reader.scan.scanned_logical_bytes(),
            next,
        })
    }
}

fn path_segment_bytes(segment: &ComponentFieldPathSegment) -> usize {
    match segment {
        ComponentFieldPathSegment::Field(key) => key.len(),
        ComponentFieldPathSegment::Index(_) => size_of::<u32>(),
    }
}

struct ValuePageReader {
    limits: EditorPropertyReadLimits,
    scan: ComponentPropertyReadBudget,
    nodes: usize,
    logical_bytes: usize,
}

impl ValuePageReader {
    fn observe(&mut self, bytes: usize) -> Result<(), EditorDocumentQueryRejection> {
        Ok(self
            .scan
            .observe_entry(bytes)
            .map_err(AuthoringReadError::from)?)
    }

    fn select<'a>(
        &mut self,
        mut value: &'a ComponentValue,
        path: &ComponentFieldPath,
    ) -> Result<&'a ComponentValue, EditorDocumentQueryRejection> {
        for (step, segment) in path.segments().iter().enumerate() {
            self.observe(path_segment_bytes(segment))?;
            value = match (value, segment) {
                (ComponentValue::Map(fields), ComponentFieldPathSegment::Field(key)) => {
                    fields.get(key)
                }
                (ComponentValue::List(items), ComponentFieldPathSegment::Index(index)) => {
                    usize::try_from(*index)
                        .ok()
                        .and_then(|index| items.get(index))
                }
                _ => None,
            }
            .ok_or(EditorDocumentQueryRejection::InvalidValuePath { step })?;
        }
        Ok(value)
    }

    fn page(
        &mut self,
        value: &ComponentValue,
        position: PropertyValuePagePosition,
    ) -> Result<
        (EditorPropertyPageData, Option<PropertyValuePagePosition>),
        EditorDocumentQueryRejection,
    > {
        use PropertyValuePagePosition::{List, Map, Start, Text};

        self.nodes = 1;
        match (value, position) {
            (ComponentValue::List(items), Start) => self.list(items, 0),
            (ComponentValue::List(items), List(start)) if start < items.len() => {
                self.list(items, start)
            }
            (ComponentValue::Map(fields), Start) => self.map(fields, None),
            (ComponentValue::Map(fields), Map(after)) => self.map(fields, Some(&after)),
            (ComponentValue::String(text), Start) => self.text(text, 0),
            (ComponentValue::String(text), Text(start))
                if start < text.len() && text.is_char_boundary(start) =>
            {
                self.text(text, start)
            }
            (_, Start) => {
                let cost = value
                    .cost_up_to(
                        self.limits.max_value_nodes(),
                        self.limits.max_logical_bytes(),
                    )
                    .ok_or(EditorDocumentQueryRejection::ValueBudgetExceeded)?;
                self.nodes = cost.nodes();
                self.logical_bytes = cost.logical_bytes();
                Ok((EditorPropertyPageData::Scalar(value.clone()), None))
            }
            _ => Err(EditorDocumentQueryRejection::InvalidContinuation),
        }
    }

    fn text(
        &mut self,
        text: &str,
        start: usize,
    ) -> Result<
        (EditorPropertyPageData, Option<PropertyValuePagePosition>),
        EditorDocumentQueryRejection,
    > {
        let total_bytes = text.len();
        let allowance = self.limits.max_logical_bytes().min(total_bytes - start);
        let end = text.floor_char_boundary(start + allowance);
        if start == end && end < total_bytes {
            return Err(EditorDocumentQueryRejection::ValueBudgetExceeded);
        }
        self.logical_bytes = end - start;
        Ok((
            EditorPropertyPageData::Text {
                total_bytes,
                start,
                text: text[start..end].to_owned(),
            },
            (end < total_bytes).then_some(PropertyValuePagePosition::Text(end)),
        ))
    }

    fn list(
        &mut self,
        values: &[ComponentValue],
        start: usize,
    ) -> Result<
        (EditorPropertyPageData, Option<PropertyValuePagePosition>),
        EditorDocumentQueryRejection,
    > {
        let mut items = Vec::new();
        for value in &values[start..] {
            let Some(item) = self.child(0, value)? else {
                break;
            };
            items.push(item);
        }
        let end = start + items.len();
        Ok((
            EditorPropertyPageData::List {
                total_items: values.len(),
                start,
                items,
            },
            (end < values.len()).then_some(PropertyValuePagePosition::List(end)),
        ))
    }

    fn map(
        &mut self,
        fields: &BTreeMap<String, ComponentValue>,
        after: Option<&str>,
    ) -> Result<
        (EditorPropertyPageData, Option<PropertyValuePagePosition>),
        EditorDocumentQueryRejection,
    > {
        let mut entries: Vec<(Arc<str>, EditorPropertyPageItem)> = Vec::new();
        let mut next = None;
        for (key, value) in fields.range::<str, _>((after.map_or(Unbounded, Excluded), Unbounded)) {
            let Some(item) = self.child(key.len(), value)? else {
                next = entries
                    .last()
                    .map(|(after, _)| PropertyValuePagePosition::Map(Arc::clone(after)));
                break;
            };
            entries.push((Arc::from(key.as_str()), item));
        }
        if after.is_some() && entries.is_empty() {
            return Err(EditorDocumentQueryRejection::InvalidContinuation);
        }
        Ok((
            EditorPropertyPageData::Map {
                total_entries: fields.len(),
                entries,
            },
            next,
        ))
    }

    fn child(
        &mut self,
        key_bytes: usize,
        value: &ComponentValue,
    ) -> Result<Option<EditorPropertyPageItem>, EditorDocumentQueryRejection> {
        match self.copy_child(key_bytes, value) {
            Ok(item) => Ok(Some(item)),
            Err(_) if self.nodes > 1 => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn copy_child(
        &mut self,
        key_bytes: usize,
        value: &ComponentValue,
    ) -> Result<EditorPropertyPageItem, EditorDocumentQueryRejection> {
        let remaining_nodes = self.limits.max_value_nodes() - self.nodes;
        if remaining_nodes == 0 {
            return Err(EditorDocumentQueryRejection::ValueBudgetExceeded);
        }
        self.observe(key_bytes)?;
        let remaining_bytes = (self.limits.max_logical_bytes() - self.logical_bytes)
            .checked_sub(key_bytes)
            .ok_or(EditorDocumentQueryRejection::ValueBudgetExceeded)?;
        let (item, nodes, bytes) = match value {
            ComponentValue::String(text) => {
                (EditorPropertyPageItem::Text { bytes: text.len() }, 1, 0)
            }
            ComponentValue::List(items) => {
                (EditorPropertyPageItem::List { len: items.len() }, 1, 0)
            }
            ComponentValue::Map(fields) => {
                (EditorPropertyPageItem::Map { len: fields.len() }, 1, 0)
            }
            _ => {
                let cost = value
                    .cost_up_to(remaining_nodes, remaining_bytes)
                    .ok_or(EditorDocumentQueryRejection::ValueBudgetExceeded)?;
                (
                    EditorPropertyPageItem::Scalar(value.clone()),
                    cost.nodes(),
                    cost.logical_bytes(),
                )
            }
        };
        self.nodes += nodes;
        self.logical_bytes += key_bytes + bytes;
        Ok(item)
    }
}
