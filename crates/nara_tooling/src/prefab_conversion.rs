//! UI-neutral observations for one Host-owned complete instance conversion.

use std::sync::Arc;

use nara_diagnostic::DiagnosticReport;
use nara_scene::SceneEntityId;

use crate::{
    CommittedEditorPrefabConversion, EditorDocumentId, EditorDocumentQueryBasis,
    EditorFrameRevision,
};

/// A session-provenanced operation identity, never a substitute for the retained candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorPrefabConversionId {
    source: EditorFrameRevision,
    sequence: u64,
}

impl EditorPrefabConversionId {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(source: EditorFrameRevision, sequence: u64) -> Self {
        Self { source, sequence }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EditorPrefabConversionPhase {
    #[default]
    Idle,
    Preparing,
    Review,
    Validating,
    Cancelling,
    Terminal,
}

impl EditorPrefabConversionPhase {
    #[must_use]
    pub const fn work_pending(self) -> bool {
        matches!(self, Self::Preparing | Self::Validating | Self::Cancelling)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPrefabConversionRejection {
    Closing,
    Busy,
    ResultPending,
    WrongOperation,
    WrongPhase,
    UnsupportedMode,
    SourceUnavailable,
    StaleSource,
    SourceRejected,
    BudgetExceeded,
    PreparationFailed,
    ValidationFailed,
    TaskUnavailable,
    IdExhausted,
    InvalidReviewPage,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorPrefabConversionMappingRow {
    /// A display breadcrumb through captured sources, not a persistent source locator.
    pub origin: Arc<str>,
    pub entity: SceneEntityId,
}

/// A bounded page from the retained mapping; pagination never recompiles or re-prepares.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorPrefabConversionReview {
    pub basis: EditorDocumentQueryBasis,
    pub offset: usize,
    pub total_entities: usize,
    pub rows: Vec<EditorPrefabConversionMappingRow>,
    pub next_offset: Option<usize>,
    pub source_count: usize,
    pub working_copy_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditorPrefabConversionResult {
    Committed {
        operation: EditorPrefabConversionId,
        receipt: Arc<CommittedEditorPrefabConversion>,
    },
    Cancelled {
        operation: EditorPrefabConversionId,
    },
    Failed {
        operation: EditorPrefabConversionId,
        reason: EditorPrefabConversionRejection,
        diagnostics: DiagnosticReport,
    },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditorPrefabConversionView {
    pub operation: Option<EditorPrefabConversionId>,
    pub document: Option<EditorDocumentId>,
    pub anchor: Option<Arc<str>>,
    pub phase: EditorPrefabConversionPhase,
    pub review: Option<Arc<EditorPrefabConversionReview>>,
    pub result: Option<EditorPrefabConversionResult>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EditorPrefabConversionAction {
    Prepare {
        basis: EditorDocumentQueryBasis,
        anchor: SceneEntityId,
    },
    Commit {
        operation: EditorPrefabConversionId,
    },
    Cancel {
        operation: EditorPrefabConversionId,
    },
    ReviewPage {
        operation: EditorPrefabConversionId,
        offset: usize,
    },
    Acknowledge {
        operation: EditorPrefabConversionId,
    },
}

/// Dispatch acceptance is distinct from source publication in the terminal result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPrefabConversionActionOutcome {
    Accepted {
        operation: EditorPrefabConversionId,
        phase: EditorPrefabConversionPhase,
    },
    Rejected {
        operation: Option<EditorPrefabConversionId>,
        reason: EditorPrefabConversionRejection,
    },
}
