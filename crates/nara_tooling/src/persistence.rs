//! UI-neutral editor persistence and dirty-close models.

use nara_scene::SceneAuthoringRevision;

use crate::{EditorApplyChangesResult, EditorPlayView, EditorRuntimeEditResult};
use crate::{EditorDocumentDigest, EditorDocumentId};

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
pub enum EditorPersistenceRejection {
    Busy,
    ResultPending,
    NoActiveDocument,
    MissingDocument,
    NotDirty,
    StaleRevision,
    TargetChanged,
    TargetDeleted,
    LockUnavailable,
    UnsupportedGuarantee,
    RequiresReconcile,
    RuntimeActive,
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
        document: Option<EditorDocumentId>,
        reason: EditorPersistenceRejection,
    },
    Failed {
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
    CloseScene { document: EditorDocumentId },
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
    workspace_intent: EditorWorkspaceIntentView,
    runtime_edit_result: Option<EditorRuntimeEditResult>,
    apply_changes_result: Option<EditorApplyChangesResult>,
}

impl EditorProjectView {
    #[must_use]
    pub const fn new(
        play: EditorPlayView,
        persistence: EditorPersistenceView,
        workspace_intent: EditorWorkspaceIntentView,
    ) -> Self {
        Self {
            play,
            persistence,
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
