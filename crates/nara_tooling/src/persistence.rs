//! UI-neutral editor persistence and dirty-close models.

use nara_scene::SceneAuthoringRevision;

use crate::{
    EditorApplyChangesResult, EditorPlayView, EditorRuntimeAvailability, EditorRuntimeEditResult,
};
use crate::{EditorClosePromptToken, EditorDocumentDigest, EditorDocumentId};

/// Authoring completeness of the exact project snapshot installed in an Editor session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAuthoringReadiness {
    Complete,
    Degraded,
}

/// Orthogonal Authoring and Runtime readiness facts for one installed Editor project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorProjectReadinessView {
    authoring: EditorAuthoringReadiness,
    runtime: EditorRuntimeAvailability,
}

impl EditorProjectReadinessView {
    #[must_use]
    pub const fn new(
        authoring: EditorAuthoringReadiness,
        runtime: EditorRuntimeAvailability,
    ) -> Self {
        Self { authoring, runtime }
    }

    #[must_use]
    pub const fn authoring(self) -> EditorAuthoringReadiness {
        self.authoring
    }

    #[must_use]
    pub const fn runtime(self) -> EditorRuntimeAvailability {
        self.runtime
    }
}

/// Monotonic Host publication identity for one immutable project-content snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditorProjectContentGeneration(u64);

impl EditorProjectContentGeneration {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(generation: u64) -> Self {
        Self(generation)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Typed reason why saved source has not become the current project-content snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorProjectContentRefreshFailure {
    SavedSourceChanged,
    InvalidContent,
    HostUnavailable,
    CapacityExceeded,
    RuntimeRetirementRequired,
    PreviewRejected,
}

impl EditorProjectContentRefreshFailure {
    #[must_use]
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::InvalidContent | Self::HostUnavailable | Self::PreviewRejected
        )
    }
}

/// Project-content freshness is independent from document persistence and Runtime lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorProjectContentView {
    /// No Runtime project-content lane is installed for this Authoring session.
    NotInstalled,
    /// The published closure satisfies every retained saved-source obligation.
    Current {
        generation: EditorProjectContentGeneration,
    },
    /// Refreshes the complete outstanding source set. These fields identify one representative
    /// obligation, not an exhaustive source list or the latest persistence operation.
    /// Its originating document may already be closed.
    RefreshPending {
        current_generation: EditorProjectContentGeneration,
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        saved_digest: EditorDocumentDigest,
    },
    /// Retains the last-good closure and all outstanding obligations after a failed refresh.
    /// These fields identify a representative obligation, possibly from a closed document, not
    /// necessarily the failing source. The failure concerns the complete refresh; diagnostics
    /// identify offending sources.
    Stale {
        current_generation: EditorProjectContentGeneration,
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        saved_digest: EditorDocumentDigest,
        failure: EditorProjectContentRefreshFailure,
    },
}

impl EditorProjectContentView {
    #[must_use]
    pub const fn current_generation(self) -> Option<EditorProjectContentGeneration> {
        match self {
            Self::NotInstalled => None,
            Self::Current { generation } => Some(generation),
            Self::RefreshPending {
                current_generation, ..
            }
            | Self::Stale {
                current_generation, ..
            } => Some(current_generation),
        }
    }

    #[must_use]
    pub const fn is_current(self) -> bool {
        matches!(self, Self::Current { .. })
    }

    #[must_use]
    pub const fn retryable(self) -> bool {
        matches!(
            self,
            Self::Stale {
                failure,
                ..
            } if failure.retryable()
        )
    }
}

/// Explicit remediation for a saved but stale project-content generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorProjectContentAction {
    RetryRefresh,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorProjectContentRefreshRejection {
    RefreshPending,
    ContentAlreadyCurrent,
    RetryNotAvailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorProjectContentRefreshRequestResult {
    Accepted,
    Rejected(EditorProjectContentRefreshRejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPersistenceCommand {
    Save { document: Option<EditorDocumentId> },
    Reopen { document: Option<EditorDocumentId> },
    AcknowledgeResult,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPersistenceOperation {
    Idle,
    Saving {
        document: EditorDocumentId,
        captured_revision: SceneAuthoringRevision,
    },
    Opening {
        document: EditorDocumentId,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPersistenceRequestKind {
    Save,
    Reopen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPersistenceRejection {
    Busy,
    ResultPending,
    NoActiveDocument,
    MissingDocument,
    DocumentKindMismatch,
    SourceBindingMismatch,
    NotDirty,
    StaleRevision,
    TargetChanged,
    TargetDeleted,
    LockUnavailable,
    UnsupportedGuarantee,
    RequiresReconcile,
    RuntimeActive,
    PreservationMismatch,
    CleanupPending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPersistenceFailureStage {
    Encode,
    OpenTarget,
    ReadTarget,
    CreateTemporary,
    WriteTemporary,
    SyncTemporary,
    ReplaceTarget,
    Decode,
    Validate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPersistenceResult {
    Saved {
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        digest: EditorDocumentDigest,
    },
    Opened {
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        digest: EditorDocumentDigest,
    },
    Rejected {
        request: EditorPersistenceRequestKind,
        document: Option<EditorDocumentId>,
        reason: EditorPersistenceRejection,
    },
    Failed {
        request: EditorPersistenceRequestKind,
        document: Option<EditorDocumentId>,
        stage: EditorPersistenceFailureStage,
    },
    PersistenceUncertain {
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        digest: EditorDocumentDigest,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorPersistenceView {
    operation: EditorPersistenceOperation,
    result: Option<EditorPersistenceResult>,
}

impl EditorPersistenceView {
    #[must_use]
    pub const fn new(
        operation: EditorPersistenceOperation,
        result: Option<EditorPersistenceResult>,
    ) -> Self {
        Self { operation, result }
    }

    #[must_use]
    pub const fn operation(self) -> EditorPersistenceOperation {
        self.operation
    }

    #[must_use]
    pub const fn result(self) -> Option<EditorPersistenceResult> {
        self.result
    }
}

impl Default for EditorPersistenceView {
    fn default() -> Self {
        Self::new(EditorPersistenceOperation::Idle, None)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPersistenceRequestResult {
    Accepted,
    Rejected(EditorPersistenceRejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorCloseDecision {
    Save,
    Discard,
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorWorkspaceIntent {
    CloseDocument { document: EditorDocumentId },
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorWorkspaceIntentPhase {
    AwaitingDecision,
    Saving,
    RetiringRuntime,
    /// Every document decision and runtime retirement completed without destroying the workspace.
    Prepared,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorWorkspaceIntentRejection {
    Busy,
    ResultPending,
    NoActiveDocument,
    MissingDocument,
    DecisionNotRequired,
    StalePrompt,
    PersistenceRejected(EditorPersistenceRejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorWorkspaceIntentResult {
    Applied {
        intent: EditorWorkspaceIntent,
    },
    Cancelled {
        intent: EditorWorkspaceIntent,
    },
    Rejected {
        intent: EditorWorkspaceIntent,
        reason: EditorWorkspaceIntentRejection,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorWorkspaceIntentRequestResult {
    Accepted,
    Rejected(EditorWorkspaceIntentRejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EditorWorkspaceIntentView {
    intent: Option<EditorWorkspaceIntent>,
    phase: Option<EditorWorkspaceIntentPhase>,
    decision_document: Option<EditorDocumentId>,
    decision_prompt: Option<EditorClosePromptToken>,
    result: Option<EditorWorkspaceIntentResult>,
}

impl EditorWorkspaceIntentView {
    #[must_use]
    pub const fn new(
        intent: Option<EditorWorkspaceIntent>,
        phase: Option<EditorWorkspaceIntentPhase>,
        decision_document: Option<EditorDocumentId>,
        decision_prompt: Option<EditorClosePromptToken>,
        result: Option<EditorWorkspaceIntentResult>,
    ) -> Self {
        Self {
            intent,
            phase,
            decision_document,
            decision_prompt,
            result,
        }
    }

    #[must_use]
    pub const fn intent(self) -> Option<EditorWorkspaceIntent> {
        self.intent
    }

    #[must_use]
    pub const fn phase(self) -> Option<EditorWorkspaceIntentPhase> {
        self.phase
    }

    /// Returns the exact document whose close decision is currently requested.
    #[must_use]
    pub const fn decision_document(self) -> Option<EditorDocumentId> {
        self.decision_document
    }

    #[must_use]
    pub const fn decision_prompt(self) -> Option<EditorClosePromptToken> {
        self.decision_prompt
    }

    #[must_use]
    pub const fn result(self) -> Option<EditorWorkspaceIntentResult> {
        self.result
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorProjectView {
    readiness: EditorProjectReadinessView,
    project_browser: crate::EditorProjectBrowserView,
    play: EditorPlayView,
    persistence: EditorPersistenceView,
    project_content: EditorProjectContentView,
    workspace_intent: EditorWorkspaceIntentView,
    runtime_edit_result: Option<EditorRuntimeEditResult>,
    apply_changes_result: Option<EditorApplyChangesResult>,
}

impl EditorProjectView {
    #[must_use]
    pub const fn new(
        readiness: EditorProjectReadinessView,
        project_browser: crate::EditorProjectBrowserView,
        play: EditorPlayView,
        persistence: EditorPersistenceView,
        project_content: EditorProjectContentView,
        workspace_intent: EditorWorkspaceIntentView,
    ) -> Self {
        Self {
            readiness,
            project_browser,
            play,
            persistence,
            project_content,
            workspace_intent,
            runtime_edit_result: None,
            apply_changes_result: None,
        }
    }

    #[must_use]
    pub const fn readiness(&self) -> EditorProjectReadinessView {
        self.readiness
    }

    #[must_use]
    pub const fn project_browser(&self) -> &crate::EditorProjectBrowserView {
        &self.project_browser
    }

    #[must_use]
    pub const fn play(&self) -> &EditorPlayView {
        &self.play
    }

    #[must_use]
    pub const fn persistence(&self) -> EditorPersistenceView {
        self.persistence
    }

    #[must_use]
    pub const fn project_content(&self) -> EditorProjectContentView {
        self.project_content
    }

    #[must_use]
    pub const fn workspace_intent(&self) -> EditorWorkspaceIntentView {
        self.workspace_intent
    }

    #[must_use]
    pub fn with_inspector_results(
        mut self,
        runtime_edit_result: Option<EditorRuntimeEditResult>,
        apply_changes_result: Option<EditorApplyChangesResult>,
    ) -> Self {
        self.runtime_edit_result = runtime_edit_result;
        self.apply_changes_result = apply_changes_result;
        self
    }

    #[must_use]
    pub const fn runtime_edit_result(&self) -> Option<&EditorRuntimeEditResult> {
        self.runtime_edit_result.as_ref()
    }

    #[must_use]
    pub const fn apply_changes_result(&self) -> Option<&EditorApplyChangesResult> {
        self.apply_changes_result.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use crate::{EditorPlayState, EditorRuntimeUnavailableReason, EditorWorkspaceIntentView};

    use super::*;

    #[test]
    fn ordinary_degraded_authoring_does_not_fabricate_runtime_project_content() {
        let runtime_reason = EditorRuntimeUnavailableReason::SupervisedPlayContractMissing;
        let project = EditorProjectView::new(
            EditorProjectReadinessView::new(
                EditorAuthoringReadiness::Degraded,
                EditorRuntimeAvailability::Unavailable(runtime_reason),
            ),
            crate::EditorProjectBrowserView::default(),
            EditorPlayView::new(EditorPlayState::Empty, None, None, None, false, None),
            EditorPersistenceView::default(),
            EditorProjectContentView::NotInstalled,
            EditorWorkspaceIntentView::default(),
        );

        assert_eq!(
            project.readiness().authoring(),
            EditorAuthoringReadiness::Degraded
        );
        assert_eq!(
            project.readiness().runtime(),
            EditorRuntimeAvailability::Unavailable(runtime_reason)
        );
        assert_eq!(project.project_content().current_generation(), None);
        assert!(!project.project_content().is_current());
        assert!(!project.project_content().retryable());
    }
}
