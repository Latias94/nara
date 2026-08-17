//! UI-agnostic Editor domain models and command seams.

mod composition;
mod diagnostic;
mod frame;
mod inspector;
mod persistence;
mod play;
mod snapshot;
mod workspace;

pub use composition::{
    SceneCompositionCommand, SceneCompositionComponentKind, SceneCompositionEntityKind,
};
#[doc(hidden)]
pub use frame::EditorFrameClock;
pub use frame::{
    EditorAction, EditorActionAcknowledgement, EditorActionBatch, EditorActionBatchReport,
    EditorActionOutcome, EditorActionRejection, EditorDocumentMutation, EditorFieldFeedback,
    EditorFieldFeedbackReason, EditorFieldTarget, EditorFieldValidationFailure, EditorFrameModel,
    EditorFrameRevision, EditorNavigationAction, EditorPersistenceAction, EditorPlayAction,
    EditorReopenTarget, EditorWorkspaceIntentAction,
};
pub use inspector::{
    SceneInspectorCommand, SceneInspectorCommandReport, SceneInspectorComponentView,
    SceneInspectorEntityRow, SceneInspectorEntityView, SceneInspectorFieldState,
    SceneInspectorFieldView, SceneInspectorModel, SceneInspectorState,
};
pub use persistence::{
    EditorCloseDecision, EditorPersistenceCommand, EditorPersistenceFailureStage,
    EditorPersistenceOperation, EditorPersistenceRejection, EditorPersistenceRequestKind,
    EditorPersistenceRequestResult, EditorPersistenceResult, EditorPersistenceView,
    EditorProjectView, EditorWorkspaceIntent, EditorWorkspaceIntentPhase,
    EditorWorkspaceIntentRejection, EditorWorkspaceIntentRequestResult,
    EditorWorkspaceIntentResult, EditorWorkspaceIntentView,
};
#[doc(hidden)]
pub use play::__export_apply_changes_from_world;
pub use play::{
    EditorApplyChangesRejection, EditorApplyChangesResult, EditorPlayCommand, EditorPlayFailure,
    EditorPlayOperation, EditorPlayOperationResult, EditorPlayRejection, EditorPlayRequestResult,
    EditorPlayState, EditorPlayView, EditorRuntimeEditRejection, EditorRuntimeEditRequest,
    EditorRuntimeEditResult, SceneApplyChangesComponentReport, SceneApplyChangesComponentStatus,
    SceneApplyChangesReport, SceneApplyChangesRequest, SceneEditorModel, SceneEditorState,
};
pub use snapshot::{DEFAULT_WORLD_IDENTITY_SNAPSHOT_LOCATOR_LIMIT, WorldIdentitySnapshot};
pub use workspace::{
    EditorDocumentDigest, EditorDocumentId, EditorDocumentKind, EditorDocumentSourceBinding,
    EditorDocumentTabModel, EditorExternalReloadState, EditorPrefabSessionPublicationError,
    EditorPrefabSlot, EditorSceneModel, EditorSceneSessionPublicationError, EditorSceneSlot,
    EditorSceneTabModel, EditorSelectionSet, EditorWorkspace, EditorWorkspaceCommand,
    EditorWorkspaceCommandReport, EditorWorkspaceModel,
};
#[doc(hidden)]
pub use workspace::{
    EditorPersistenceAuthority, EditorPersistenceCheckpoint, EditorPersistenceCommit,
};
