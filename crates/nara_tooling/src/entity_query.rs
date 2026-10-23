//! Bounded, selection-independent enumeration of open authoring source records.

use std::{error::Error, fmt, sync::Arc};

use nara_scene::{AuthoringSchemaBasis, SceneAuthoringRevision, SceneEntityId};

use crate::{EditorDocumentId, EditorDocumentKind};

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
    ) -> Result<Self, EditorEntityQueryRejection> {
        if !(1..=MAX_EDITOR_ENTITY_QUERY_SCAN_RECORDS).contains(&max_scan_records)
            || !(1..=MAX_EDITOR_ENTITY_QUERY_ROWS).contains(&max_rows)
        {
            return Err(EditorEntityQueryRejection::InvalidLimits {
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

/// Exact open-document facts used to begin enumeration or describe an owned page.
///
/// Obtain these facts from the document tab model. The revision includes its source
/// incarnation; a source locator or saved content identity is not a substitute.
/// `schema` is `None` for sessions without an authoring Schema admission, not a wildcard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorEntityQueryBasis {
    pub document: EditorDocumentId,
    pub revision: SceneAuthoringRevision,
    pub schema: Option<AuthoringSchemaBasis>,
}

/// Starts at an exact document basis or replays an owner-created continuation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorEntityQueryPosition {
    Start(EditorEntityQueryBasis),
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
    pub(crate) owner: EditorEntityQueryOwner,
    pub(crate) basis: EditorEntityQueryBasis,
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
    pub basis: EditorEntityQueryBasis,
    pub entities: Vec<SceneEntityId>,
    pub scanned_records: usize,
    pub next: Option<EditorEntityCursor>,
}

/// Exact rejection of a query without changing document or presentation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorEntityQueryRejection {
    /// The continuation belongs to another workspace, including a replaced project workspace.
    WrongWorkspace,
    /// The explicitly named document is no longer open, or was never open here.
    DocumentNotOpen { document: EditorDocumentId },
    /// Only direct Scene and Prefab source records are supported.
    UnsupportedDocumentKind { kind: EditorDocumentKind },
    /// The slot no longer has the source kind captured by the continuation.
    DocumentKindChanged {
        expected: EditorDocumentKind,
        actual: EditorDocumentKind,
    },
    /// The document was reopened or replaced; a matching locator cannot rebase the query.
    StaleIncarnation,
    /// The document changed within the captured incarnation, even if Undo restores its content.
    StaleRevision,
    /// The exact retained Schema admission changed; equal catalogs are not equal admissions.
    StaleSchema,
    /// A positive limit exceeded its cap, or either limit was zero.
    InvalidLimits {
        max_scan_records: usize,
        max_rows: usize,
    },
    /// The owner-created continuation does not identify a valid position in its exact revision.
    InvalidContinuation,
}

impl fmt::Display for EditorEntityQueryRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongWorkspace => {
                formatter.write_str("entity query belongs to another workspace")
            }
            Self::DocumentNotOpen { .. } => {
                formatter.write_str("entity query document is not open")
            }
            Self::UnsupportedDocumentKind { .. } => {
                formatter.write_str("entity query requires a Scene or Prefab source")
            }
            Self::DocumentKindChanged { .. } => {
                formatter.write_str("entity query document kind changed")
            }
            Self::StaleIncarnation => {
                formatter.write_str("entity query source incarnation changed")
            }
            Self::StaleRevision => formatter.write_str("entity query document revision changed"),
            Self::StaleSchema => formatter.write_str("entity query Schema admission changed"),
            Self::InvalidLimits { .. } => {
                formatter.write_str("entity query limits are out of range")
            }
            Self::InvalidContinuation => {
                formatter.write_str("entity query continuation is invalid")
            }
        }
    }
}

impl Error for EditorEntityQueryRejection {}

#[derive(Debug, Clone, Default)]
pub(crate) struct EditorEntityQueryOwner(Arc<()>);

impl PartialEq for EditorEntityQueryOwner {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for EditorEntityQueryOwner {}
