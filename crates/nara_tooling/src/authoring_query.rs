use nara_reflect::{
    ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
};
use nara_scene::{AuthoringRecordReadiness, SceneEntityId};

use crate::document_query::EditorDocumentQueryOwner;
use crate::{EditorDocumentKind, EditorDocumentQueryBasis, EditorDocumentQueryRejection};

pub const MAX_EDITOR_AUTHORING_QUERY_ENTRIES: usize = 256;
pub const MAX_EDITOR_AUTHORING_QUERY_METADATA_BYTES: usize = 64 * 1024;
pub const MAX_EDITOR_ROOT_READ_VALUE_NODES: usize = 4_096;
pub const MAX_EDITOR_ROOT_READ_LOGICAL_BYTES: usize = 256 * 1024;

/// Operation-local discovery limits. Metadata counts all returned/cursor UTF-8 ID bytes.
/// Fixed-size fields are bounded independently by row count; this is not an allocator bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorAuthoringQueryLimits {
    max_scan_entries: usize,
    max_rows: usize,
    max_metadata_bytes: usize,
}

impl EditorAuthoringQueryLimits {
    pub fn new(
        max_scan_entries: usize,
        max_rows: usize,
        max_metadata_bytes: usize,
    ) -> Result<Self, EditorDocumentQueryRejection> {
        if !(1..=MAX_EDITOR_AUTHORING_QUERY_ENTRIES).contains(&max_scan_entries)
            || !(1..=MAX_EDITOR_AUTHORING_QUERY_ENTRIES).contains(&max_rows)
            || !(1..=MAX_EDITOR_AUTHORING_QUERY_METADATA_BYTES).contains(&max_metadata_bytes)
        {
            return Err(EditorDocumentQueryRejection::InvalidAuthoringLimits);
        }
        Ok(Self {
            max_scan_entries,
            max_rows,
            max_metadata_bytes,
        })
    }
    #[must_use]
    pub const fn max_scan_entries(self) -> usize {
        self.max_scan_entries
    }
    #[must_use]
    pub const fn max_rows(self) -> usize {
        self.max_rows
    }
    #[must_use]
    pub const fn max_metadata_bytes(self) -> usize {
        self.max_metadata_bytes
    }
}

/// Discovery never copies values, even when every value would fit a later read budget.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAuthoringProjection {
    Components,
    Roots { component: ComponentTypeId },
}

/// Exact source and projection to discover, independent of active navigation and selection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorAuthoringQueryTarget {
    pub basis: EditorDocumentQueryBasis,
    pub entity: SceneEntityId,
    pub projection: EditorAuthoringProjection,
}

impl EditorAuthoringQueryTarget {
    pub(crate) fn metadata_bytes(&self) -> usize {
        self.entity
            .as_str()
            .len()
            .saturating_add(match &self.projection {
                EditorAuthoringProjection::Components => 0,
                EditorAuthoringProjection::Roots { component } => component.as_str().len(),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAuthoringQueryPosition {
    Start(EditorAuthoringQueryTarget),
    Continue(EditorAuthoringCursor),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorAuthoringQuery {
    pub position: EditorAuthoringQueryPosition,
    pub limits: EditorAuthoringQueryLimits,
}

/// Replayable process-local continuation. Its complete target is private and cannot be retargeted.
/// It retains an immutable Schema admission, not an open document or permission to publish edits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorAuthoringCursor {
    pub(crate) owner: EditorDocumentQueryOwner,
    pub(crate) target: EditorAuthoringQueryTarget,
    pub(crate) kind: EditorDocumentKind,
    pub(crate) position: AuthoringPagePosition,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AuthoringPagePosition {
    Components {
        after: Option<ComponentTypeId>,
        visited: usize,
    },
    Roots {
        next_root: usize,
    },
}

/// Compact actual record/root metadata, not a Schema, creation capability or interpreted payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorAuthoringRow {
    Component {
        component: ComponentTypeId,
        version: ComponentSchemaVersion,
        readiness: AuthoringRecordReadiness,
    },
    Root {
        field: ComponentFieldId,
        kind: ComponentValueKind,
        required: bool,
        /// Declared Edit capability only; final candidate admission still decides an edit.
        edit_capability: bool,
    },
}

/// Owned historical observation. A current page is not mutation authority after its basis changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorAuthoringPage {
    pub target: EditorAuthoringQueryTarget,
    pub rows: Vec<EditorAuthoringRow>,
    pub scanned_entries: usize,
    /// Aggregate returned and continuation UTF-8 ID bytes, including duplicated identities.
    pub metadata_bytes: usize,
    pub next: Option<EditorAuthoringCursor>,
}

/// Separately capped metadata, complete-value nodes and complete-value logical bytes for one root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorRootReadLimits {
    max_metadata_bytes: usize,
    max_value_nodes: usize,
    max_logical_bytes: usize,
}

impl EditorRootReadLimits {
    pub fn new(
        max_metadata_bytes: usize,
        max_value_nodes: usize,
        max_logical_bytes: usize,
    ) -> Result<Self, EditorDocumentQueryRejection> {
        if !(1..=MAX_EDITOR_AUTHORING_QUERY_METADATA_BYTES).contains(&max_metadata_bytes)
            || !(1..=MAX_EDITOR_ROOT_READ_VALUE_NODES).contains(&max_value_nodes)
            || !(1..=MAX_EDITOR_ROOT_READ_LOGICAL_BYTES).contains(&max_logical_bytes)
        {
            return Err(EditorDocumentQueryRejection::InvalidRootReadLimits);
        }
        Ok(Self {
            max_metadata_bytes,
            max_value_nodes,
            max_logical_bytes,
        })
    }
    #[must_use]
    pub const fn max_metadata_bytes(self) -> usize {
        self.max_metadata_bytes
    }
    #[must_use]
    pub const fn max_value_nodes(self) -> usize {
        self.max_value_nodes
    }
    #[must_use]
    pub const fn max_logical_bytes(self) -> usize {
        self.max_logical_bytes
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorRootTarget {
    pub basis: EditorDocumentQueryBasis,
    pub entity: SceneEntityId,
    pub component: ComponentTypeId,
    pub field: ComponentFieldId,
}

impl EditorRootTarget {
    pub(crate) fn metadata_bytes(&self) -> usize {
        self.entity
            .as_str()
            .len()
            .saturating_add(self.component.as_str().len())
            .saturating_add(self.field.as_str().len())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorRootRead {
    pub target: EditorRootTarget,
    pub limits: EditorRootReadLimits,
}

/// Explicit Null is Present; a defaulted Null is Defaulted. Missing never means an unknown field.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorRootValue {
    Present(ComponentValue),
    Defaulted(ComponentValue),
    Missing,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorRootObservation {
    pub target: EditorRootTarget,
    pub value: EditorRootValue,
    pub metadata_bytes: usize,
    pub value_nodes: usize,
    pub value_logical_bytes: usize,
}
