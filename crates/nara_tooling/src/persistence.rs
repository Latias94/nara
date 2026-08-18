//! UI-neutral editor persistence and dirty-close models.

use nara_scene::SceneAuthoringRevision;

use crate::{EditorApplyChangesResult, EditorPlayView, EditorRuntimeEditResult};
use crate::{EditorDocumentDigest, EditorDocumentId};

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
    Current {
        generation: EditorProjectContentGeneration,
    },
    RefreshPending {
        current_generation: EditorProjectContentGeneration,
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        saved_digest: EditorDocumentDigest,
    },
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
    pub const fn current_generation(self) -> EditorProjectContentGeneration {
        match self {
            Self::Current { generation } => generation,
            Self::RefreshPending {
                current_generation, ..
            }
            | Self::Stale {
                current_generation, ..
            } => current_generation,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorWorkspaceIntentRejection {
    Busy,
    ResultPending,
    NoActiveDocument,
    MissingDocument,
    DecisionNotRequired,
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
    result: Option<EditorWorkspaceIntentResult>,
}

impl EditorWorkspaceIntentView {
    #[must_use]
    pub const fn new(
        intent: Option<EditorWorkspaceIntent>,
        phase: Option<EditorWorkspaceIntentPhase>,
        result: Option<EditorWorkspaceIntentResult>,
    ) -> Self {
        Self {
            intent,
            phase,
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

    #[must_use]
    pub const fn result(self) -> Option<EditorWorkspaceIntentResult> {
        self.result
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditorProjectView {
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
        play: EditorPlayView,
        persistence: EditorPersistenceView,
        project_content: EditorProjectContentView,
        workspace_intent: EditorWorkspaceIntentView,
    ) -> Self {
        Self {
            play,
            persistence,
            project_content,
            workspace_intent,
            runtime_edit_result: None,
            apply_changes_result: None,
        }
    }

    #[must_use]
    pub const fn play(&self) -> EditorPlayView {
        self.play
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
