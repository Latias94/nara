//! UI-neutral actions and observations for one Host-owned effective Prefab inspection.

use nara_diagnostic::DiagnosticReport;
use nara_reflect::{ComponentPropertyPath, ComponentTypeId};
use nara_scene::{PrefabEntityTarget, SceneAuthoringRevision, SceneEntityId};

use crate::{EditorDocumentId, EditorDocumentQueryBasis, EditorFrameRevision};

/// Session-provenanced identity of one retained effective Prefab inspection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorPrefabInspectionId {
    source: EditorFrameRevision,
    sequence: u64,
}

impl EditorPrefabInspectionId {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(source: EditorFrameRevision, sequence: u64) -> Self {
        Self { source, sequence }
    }
}

/// A read-only property address relative to the containing Scene or Prefab source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorPrefabPropertyAddress {
    /// The first hop enters the instance anchor selected by [`EditorPrefabInspectionAction::Inspect`].
    pub entity: PrefabEntityTarget,
    pub component: ComponentTypeId,
    pub property: ComponentPropertyPath,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EditorPrefabInspectionPhase {
    #[default]
    Idle,
    Inspecting,
    Ready,
    PreparingRevert,
    Review,
    Validating,
    Cancelling,
    Terminal,
}

impl EditorPrefabInspectionPhase {
    #[must_use]
    pub const fn work_pending(self) -> bool {
        matches!(
            self,
            Self::Inspecting | Self::PreparingRevert | Self::Validating | Self::Cancelling
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPrefabInspectionRejection {
    Closing,
    Busy,
    ResultPending,
    WrongInspection,
    WrongPhase,
    UnsupportedMode,
    SourceUnavailable,
    StaleSource,
    SourceRejected,
    BudgetExceeded,
    PreparationFailed,
    ValidationFailed,
    TaskUnavailable,
    TaskCleanupFailed,
    IdExhausted,
    InvalidProperty,
    NoCurrentClaim,
}

/// Terminal result retained by the Host for acknowledgment.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorPrefabInspectionResult {
    Cancelled {
        inspection: EditorPrefabInspectionId,
    },
    Reverted {
        inspection: EditorPrefabInspectionId,
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
    },
    Failed {
        inspection: EditorPrefabInspectionId,
        reason: EditorPrefabInspectionRejection,
        diagnostics: DiagnosticReport,
    },
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct EditorPrefabInspectionView {
    pub inspection: Option<EditorPrefabInspectionId>,
    pub phase: EditorPrefabInspectionPhase,
    pub result: Option<EditorPrefabInspectionResult>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorPrefabInspectionAction {
    Inspect {
        basis: EditorDocumentQueryBasis,
        anchor: SceneEntityId,
    },
    PrepareRevert {
        inspection: EditorPrefabInspectionId,
        address: EditorPrefabPropertyAddress,
    },
    Commit {
        inspection: EditorPrefabInspectionId,
    },
    Cancel {
        inspection: EditorPrefabInspectionId,
    },
    Acknowledge {
        inspection: EditorPrefabInspectionId,
    },
}

/// Dispatch admission for one action. The frame carries any terminal result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPrefabInspectionOutcome {
    Accepted {
        inspection: EditorPrefabInspectionId,
        phase: EditorPrefabInspectionPhase,
    },
    Rejected {
        inspection: Option<EditorPrefabInspectionId>,
        reason: EditorPrefabInspectionRejection,
    },
}
