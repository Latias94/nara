//! UI-agnostic Editor domain models and command seams.

mod composition;
mod diagnostic;
mod entity_query;
mod frame;
mod gesture;
mod image_import;
mod inspector;
mod persistence;
mod play;
mod project_browser;
mod snapshot;
mod source_asset_import;
mod workspace;

pub use composition::{
    PrefabCompositionCommand, PrefabCompositionDraftError, PrefabCompositionEntityDraft,
    PrefabCompositionOutcome, SceneCompositionCommand, SceneCompositionEntityKind,
};
pub use entity_query::{
    EditorEntityCursor, EditorEntityPage, EditorEntityQuery, EditorEntityQueryBasis,
    EditorEntityQueryLimits, EditorEntityQueryPosition, EditorEntityQueryRejection,
    MAX_EDITOR_ENTITY_QUERY_ROWS, MAX_EDITOR_ENTITY_QUERY_SCAN_RECORDS,
};
#[doc(hidden)]
pub use frame::EditorFrameClock;
pub use frame::{
    EditorAction, EditorActionAcknowledgement, EditorActionBatch, EditorActionBatchReport,
    EditorActionOutcome, EditorActionRejection, EditorClosePromptToken, EditorDocumentMutation,
    EditorFieldFeedback, EditorFieldFeedbackReason, EditorFieldTarget,
    EditorFieldValidationFailure, EditorFrameModel, EditorFrameRevision, EditorNavigationAction,
    EditorPersistenceAction, EditorPlayAction, EditorReopenTarget, EditorWorkspaceIntentAction,
};
#[doc(hidden)]
pub use gesture::EditorGestureController;
pub use gesture::{
    EditorAtomicFieldEdit, EditorAtomicFieldEditError, EditorFieldWrite, EditorGestureAction,
    EditorGestureCancelReason, EditorGestureId, EditorGestureOutcome, EditorGestureRejection,
    EditorGestureStamp, EditorGestureView, MAX_EDITOR_ATOMIC_FIELD_WRITES,
};
pub use image_import::{
    EditorImageColorSpace, EditorImageDimensions, EditorImageImportAction,
    EditorImageImportActionOutcome, EditorImageImportActionRejection,
    EditorImageImportAttemptPhase, EditorImageImportAttemptView, EditorImageImportAvailabilityView,
    EditorImageImportDetailRevision, EditorImageImportFailureReason, EditorImageImportFreshness,
    EditorImageImportMetadataIssue, EditorImageImportMetadataView, EditorImageImportSettings,
    EditorImageImportView,
};
pub use inspector::{
    SceneComponentCompositionCapability, SceneCompositionCapabilities, SceneInspectorCommand,
    SceneInspectorCommandReport, SceneInspectorComponentAccess, SceneInspectorComponentView,
    SceneInspectorEntityRow, SceneInspectorEntityView, SceneInspectorFieldState,
    SceneInspectorFieldView, SceneInspectorModel, SceneInspectorReadOnlyReason,
    SceneInspectorState,
};
pub use persistence::{
    EditorAuthoringReadiness, EditorCloseDecision, EditorPersistenceCommand,
    EditorPersistenceFailureStage, EditorPersistenceOperation, EditorPersistenceRejection,
    EditorPersistenceRequestKind, EditorPersistenceRequestResult, EditorPersistenceResult,
    EditorPersistenceView, EditorProjectContentAction, EditorProjectContentGeneration,
    EditorProjectContentRefreshFailure, EditorProjectContentRefreshRejection,
    EditorProjectContentRefreshRequestResult, EditorProjectContentView, EditorProjectReadinessView,
    EditorProjectView, EditorWorkspaceIntent, EditorWorkspaceIntentPhase,
    EditorWorkspaceIntentRejection, EditorWorkspaceIntentRequestResult,
    EditorWorkspaceIntentResult, EditorWorkspaceIntentView,
};
#[doc(hidden)]
pub use play::__export_apply_changes_from_world;
pub use play::{
    EditorApplyChangesRejection, EditorApplyChangesResult, EditorExternalPlayCompilerLevel,
    EditorExternalPlayDiagnostics, EditorExternalPlayMessage, EditorExternalPlayMessageKind,
    EditorExternalPlayObservation, EditorExternalPlayOutput, EditorExternalPlayRequest,
    EditorExternalPlayState, EditorExternalPlayView, EditorPlayCommand, EditorPlayControl,
    EditorPlayFailure, EditorPlayOperation, EditorPlayOperationResult, EditorPlayRejection,
    EditorPlayRequestResult, EditorPlayState, EditorPlayView, EditorRuntimeAvailability,
    EditorRuntimeEditRejection, EditorRuntimeEditRequest, EditorRuntimeEditResult,
    EditorRuntimeUnavailableReason, SceneApplyChangesComponentReport,
    SceneApplyChangesComponentStatus, SceneApplyChangesReport, SceneApplyChangesRequest,
    SceneEditorModel, SceneEditorState,
};
pub use project_browser::{
    EditorDocumentOpenId, EditorDocumentOpenPhase, EditorDocumentOpenResult,
    EditorDocumentOpenView, EditorProjectBrowserAction, EditorProjectBrowserActionOutcome,
    EditorProjectBrowserBudgetUsage, EditorProjectBrowserDiagnosticPhase,
    EditorProjectBrowserDiagnosticView, EditorProjectBrowserEntry, EditorProjectBrowserKindFilter,
    EditorProjectBrowserLocalAction, EditorProjectBrowserPendingWork,
    EditorProjectBrowserQueryAdvance, EditorProjectBrowserRow, EditorProjectBrowserScanProgress,
    EditorProjectBrowserSnapshot, EditorProjectBrowserState, EditorProjectBrowserStatus,
    EditorProjectBrowserView, EditorProjectSourceInspectReason, EditorProjectSourceKind,
    EditorProjectSourceOpenCapability,
};
pub use snapshot::{DEFAULT_WORLD_IDENTITY_SNAPSHOT_LOCATOR_LIMIT, WorldIdentitySnapshot};
pub use source_asset_import::{
    EditorSourceAssetDomainView, EditorSourceAssetImportAction,
    EditorSourceAssetImportActionOutcome, EditorSourceAssetImportAttempt,
    EditorSourceAssetImportRejection, EditorSourceAssetImportView, EditorSourceAssetMetadataView,
};
pub use workspace::{
    EditorDocumentDigest, EditorDocumentId, EditorDocumentKind, EditorDocumentSourceBinding,
    EditorDocumentTabModel, EditorExtensionCommand, EditorExtensionDocument,
    EditorExtensionDocumentError, EditorExtensionDocumentView, EditorExternalReloadState,
    EditorPrefabSessionPublicationError, EditorPrefabSlot, EditorSceneModel,
    EditorSceneSessionPublicationError, EditorSceneSlot, EditorSceneTabModel, EditorSelectionSet,
    EditorWorkspace, EditorWorkspaceCommand, EditorWorkspaceCommandReport, EditorWorkspaceModel,
    PreparedEditorDocumentEdit,
};
#[doc(hidden)]
pub use workspace::{
    EditorExtensionDocumentCandidate, EditorPersistenceAuthority, EditorPersistenceCheckpoint,
    EditorPersistenceCommit,
};
