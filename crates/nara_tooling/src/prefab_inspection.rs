//! UI-neutral actions and observations for one Host-owned effective Prefab inspection.

use nara_diagnostic::DiagnosticReport;
use nara_reflect::{ComponentPropertyPath, ComponentTypeId};
use nara_scene::{
    AuthoringPrefabPropertyClaimKind, PrefabDeclarationLimits, PrefabEntityTarget,
    PrefabPropertyIntent, SceneAuthoringRevision, SceneEntityId,
};

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

/// A property address relative to the containing Scene or Prefab source.
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
    PreparingEdit,
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
            Self::Inspecting | Self::PreparingEdit | Self::Validating | Self::Cancelling
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
    ConflictingClaim,
    AlreadyWritten,
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
    Written {
        inspection: EditorPrefabInspectionId,
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        kind: AuthoringPrefabPropertyClaimKind,
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

#[derive(Debug, Clone, PartialEq)]
pub enum EditorPrefabInspectionAction {
    Inspect {
        basis: EditorDocumentQueryBasis,
        anchor: SceneEntityId,
    },
    PrepareRevert {
        inspection: EditorPrefabInspectionId,
        address: EditorPrefabPropertyAddress,
        /// Logical limits for preparing the source declaration patch.
        limits: PrefabDeclarationLimits,
    },
    /// Writes an explicit claim in the selected instance layer of the containing source.
    ///
    /// Set pins an editable scalar or a declared entity-reference leaf. References are
    /// authored in the containing source's scope. The prepared candidate must resolve the
    /// reference before review. Absent clears an editable optional or default-bearing root.
    PreparePropertyWrite {
        inspection: EditorPrefabInspectionId,
        address: EditorPrefabPropertyAddress,
        intent: PrefabPropertyIntent,
        limits: PrefabDeclarationLimits,
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
