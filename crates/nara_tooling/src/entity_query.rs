//! Bounded, selection-independent enumeration of open authoring source records.

use nara_scene::SceneEntityId;

use crate::document_query::EditorDocumentQueryOwner;
use crate::{EditorDocumentKind, EditorDocumentQueryBasis, EditorDocumentQueryRejection};

/// Maximum source records inspected by one entity page operation.
pub const MAX_EDITOR_ENTITY_QUERY_SCAN_RECORDS: usize = 4_096;
/// Maximum entity identities returned by one entity page operation.
pub const MAX_EDITOR_ENTITY_QUERY_ROWS: usize = 1_024;

/// Positive, independently capped scan-work and output-row budgets.
///
/// These are record-count limits, not byte or elapsed-time guarantees. They apply to
/// one query operation, not an entire action batch or subsequent Editor frame drive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorEntityQueryLimits {
    max_scan_records: usize,
    max_rows: usize,
}

impl EditorEntityQueryLimits {
    /// Rejects zero or a value above the corresponding public maximum.
    pub fn new(
        max_scan_records: usize,
        max_rows: usize,
    ) -> Result<Self, EditorDocumentQueryRejection> {
        if !(1..=MAX_EDITOR_ENTITY_QUERY_SCAN_RECORDS).contains(&max_scan_records)
            || !(1..=MAX_EDITOR_ENTITY_QUERY_ROWS).contains(&max_rows)
        {
            return Err(EditorDocumentQueryRejection::InvalidLimits {
                max_scan_records,
                max_rows,
            });
        }
        Ok(Self {
            max_scan_records,
            max_rows,
        })
    }

    /// Maximum source records inspected, independently of the returned row limit.
    #[must_use]
    pub const fn max_scan_records(self) -> usize {
        self.max_scan_records
    }

    /// Maximum entity identities returned, independently of the scan-work limit.
    #[must_use]
    pub const fn max_rows(self) -> usize {
        self.max_rows
    }
}

/// Starts at an exact document basis or replays an owner-created continuation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorEntityQueryPosition {
    Start(EditorDocumentQueryBasis),
    Continue(EditorEntityCursor),
}

/// One read-only page request for direct records of an explicitly named open Scene or Prefab.
///
/// No selection, active tab, history or dirty state changes. Limits may change between pages.
/// This operation does not enumerate expanded Prefab projections, fields or unopened sources.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorEntityQuery {
    pub position: EditorEntityQueryPosition,
    pub limits: EditorEntityQueryLimits,
}

/// A replayable, process-local continuation for one exact workspace and source-record query.
///
/// There is no public constructor or serialized representation. Retaining or cloning this value
/// neither retains an open document nor grants mutation or persistence authority. A changed
/// document revision, source incarnation or Schema admission invalidates it; navigation and
/// unrelated document edits do not. Reusing it with unchanged limits returns the same page while
/// its basis is current.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorEntityCursor {
    pub(crate) owner: EditorDocumentQueryOwner,
    pub(crate) basis: EditorDocumentQueryBasis,
    pub(crate) kind: EditorDocumentKind,
    pub(crate) next_record: usize,
}

/// An owned observation of direct source-record identities in document record order.
///
/// Identities are source entities, not Runtime entities or expanded instance identities. All
/// inspected records produce a row in this query, so `scanned_records == entities.len()`.
/// `next == None` means enumeration is complete, including an empty document. Retained pages
/// remain historical observations after their basis becomes stale, not current edit authority.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorEntityPage {
    pub basis: EditorDocumentQueryBasis,
    pub entities: Vec<SceneEntityId>,
    pub scanned_records: usize,
    pub next: Option<EditorEntityCursor>,
}
