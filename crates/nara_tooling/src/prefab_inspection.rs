//! UI-neutral actions and observations for one Host-owned effective Prefab inspection.

use nara_diagnostic::DiagnosticReport;
use nara_reflect::{ComponentPropertyPath, ComponentTypeId};
use nara_scene::{
    AuthoringPrefabPropertyClaimKind, PrefabComponentEdit, PrefabDeclarationLimits,
    PrefabEntityTarget, PrefabPropertyIntent, SceneAuthoringRevision, SceneEntityId,
};

use crate::{
    EditorDocumentId, EditorDocumentQueryBasis, EditorFrameRevision, PreparedEditorDocumentEdit,
};

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

/// A component address in one selected Prefab occurrence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorPrefabComponentAddress {
    /// The first hop enters the instance selected by [`EditorPrefabInspectionAction::Inspect`].
    pub entity: PrefabEntityTarget,
    pub component: ComponentTypeId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPrefabComponentEditKind {
    Added,
    Replaced,
    Suppressed,
    Reverted,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EditorPrefabInspectionPhase {
    #[default]
    Idle,
    Inspecting,
    Ready,
    PreparingEdit,
    Review,
    AwaitingDestinationSave,
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
    InvalidComponent,
    ConflictingClaim,
    AlreadyWritten,
    NoCurrentClaim,
    InvalidDestination,
    ApplySourceSaveRequired,
    DestinationSaveRequired,
}

/// The destination working revision used by the first step of Apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorPrefabApplyDestination {
    pub document: EditorDocumentId,
    pub revision: SceneAuthoringRevision,
    /// Whether this Apply published an edit to the destination working copy.
    pub changed: bool,
}

/// The two source-owned candidates retained by one inspected Apply.
///
/// The destination tab shows the source change inherited by other occurrences. It can be
/// unchanged when an earlier attempt already wrote the same value.
#[derive(Debug, Clone, Copy)]
pub struct EditorPrefabApplyReview<'a> {
    pub containing: &'a PreparedEditorDocumentEdit,
    pub destination: &'a EditorDocumentQueryBasis,
    pub destination_edit: Option<&'a PreparedEditorDocumentEdit>,
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
    ComponentEdited {
        inspection: EditorPrefabInspectionId,
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        kind: EditorPrefabComponentEditKind,
    },
    Applied {
        inspection: EditorPrefabInspectionId,
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        destination: EditorPrefabApplyDestination,
    },
    /// Apply reached the destination stage and left the containing override in place.
    ApplyStopped {
        inspection: EditorPrefabInspectionId,
        destination: EditorPrefabApplyDestination,
        failure: Option<EditorPrefabInspectionRejection>,
        diagnostics: DiagnosticReport,
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
    pub apply_destination: Option<EditorPrefabApplyDestination>,
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
    /// Reviews a whole component declaration in the selected layer.
    ///
    /// Set supplies the complete authored value. Revert removes the selected layer's
    /// declaration for this component, including suppression of an inherited record.
    PrepareComponentEdit {
        inspection: EditorPrefabInspectionId,
        address: EditorPrefabComponentAddress,
        edit: PrefabComponentEdit,
        limits: PrefabDeclarationLimits,
    },
    /// Promotes the selected exact override to the explicitly named open Prefab source.
    PrepareApply {
        inspection: EditorPrefabInspectionId,
        address: EditorPrefabPropertyAddress,
        destination: EditorDocumentQueryBasis,
        limits: PrefabDeclarationLimits,
    },
    /// Applies the reviewed edit. For Apply this first edits the destination working copy.
    Commit {
        inspection: EditorPrefabInspectionId,
    },
    /// Removes the reviewed override after ordinary Save has made the destination current on disk.
    FinishApply {
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
