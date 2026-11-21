use std::sync::Arc;

use nara_reflect::{ComponentFieldPath, ComponentValue};

use crate::{
    EditorDocumentKind, EditorPropertyReadLimits, EditorPropertyTarget,
    document_query::EditorDocumentQueryOwner,
};

/// A bounded read of one selected property value or one relative location inside it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorPropertyPageRead {
    pub position: EditorPropertyPagePosition,
    pub limits: EditorPropertyReadLimits,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorPropertyPagePosition {
    Start {
        target: EditorPropertyTarget,
        /// A read position inside the resolved value. Empty selects the entire property.
        /// List indices and wire keys here carry no durable edit identity.
        path: ComponentFieldPath,
    },
    Continue(EditorPropertyPageCursor),
}

/// Replayable next page for the original live document or prepared edit and value location.
/// Retaining the cursor holds metadata and Schema admission, not the document or its payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorPropertyPageCursor {
    pub(crate) owner: EditorDocumentQueryOwner,
    pub(crate) kind: EditorDocumentKind,
    pub(crate) target: Arc<PropertyValuePageTarget>,
    pub(crate) read_path: Arc<ComponentFieldPath>,
    pub(crate) next: PropertyValuePagePosition,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PropertyValuePageTarget {
    pub(crate) property: EditorPropertyTarget,
    pub(crate) path: ComponentFieldPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PropertyValuePagePosition {
    Start,
    List(usize),
    Map(Arc<str>),
    Text(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditorPropertyPageValue {
    Present(EditorPropertyPageData),
    Defaulted(EditorPropertyPageData),
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditorPropertyPageData {
    /// A complete non-container scalar other than text, including explicit Null.
    Scalar(ComponentValue),
    /// A whole-code-point UTF-8 chunk beginning at the returned byte offset.
    Text {
        total_bytes: usize,
        start: usize,
        text: String,
    },
    /// A contiguous range of immediate children, in stored order.
    List {
        total_items: usize,
        start: usize,
        items: Vec<EditorPropertyPageItem>,
    },
    /// Immediate children in canonical key order.
    Map {
        total_entries: usize,
        entries: Vec<(Arc<str>, EditorPropertyPageItem)>,
    },
}

/// One immediate child in a collection page. A subsequent read can open a branch or text
/// value by appending the row's wire key or list index to the original read position.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorPropertyPageItem {
    Scalar(ComponentValue),
    Text { bytes: usize },
    List { len: usize },
    Map { len: usize },
}

/// Owned historical value page and its optional exact continuation.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorPropertyPage {
    pub(crate) target: Arc<PropertyValuePageTarget>,
    pub value: EditorPropertyPageValue,
    /// Durable target identity bytes. Relative wire probes use the scan allowance.
    pub metadata_bytes: usize,
    /// Container and text nodes plus the canonical cost of returned scalars. Missing costs zero.
    pub value_nodes: usize,
    /// Bytes of copied text, Map keys and scalar payload. A Map cursor shares its anchor key.
    pub value_logical_bytes: usize,
    pub scanned_entries: usize,
    pub scanned_logical_bytes: usize,
    pub next: Option<EditorPropertyPageCursor>,
}

impl EditorPropertyPage {
    #[must_use]
    pub fn target(&self) -> &EditorPropertyTarget {
        &self.target.property
    }

    #[must_use]
    pub fn path(&self) -> &ComponentFieldPath {
        &self.target.path
    }
}
