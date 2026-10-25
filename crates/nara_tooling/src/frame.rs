//! Immutable Editor frames and the UI-neutral ordered action ingress.

use std::sync::atomic::{AtomicU64, Ordering};

use nara_diagnostic::DiagnosticReport;
use nara_reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValueKind};
use nara_scene::{AuthoringFieldEdit, SceneAuthoringRevision, SceneEntityId, ScenePatchDocument};

use crate::{
    EditorApplyChangesRejection, EditorCloseDecision, EditorDocumentId, EditorGestureAction,
    EditorGestureOutcome, EditorImageImportAction, EditorImageImportActionOutcome,
    EditorImageImportView, EditorPersistenceRequestResult, EditorPlayRequestResult,
    EditorProjectBrowserAction, EditorProjectBrowserActionOutcome, EditorProjectView,
    EditorRuntimeEditRejection, EditorRuntimeEditRequest, EditorWorkspaceCommandReport,
    EditorWorkspaceIntent, EditorWorkspaceIntentRequestResult, EditorWorkspaceModel,
    PrefabCompositionCommand, SceneApplyChangesRequest, SceneCompositionCommand,
    SceneInspectorCommand,
};

static NEXT_EDITOR_FRAME_SOURCE: AtomicU64 = AtomicU64::new(0);

/// Exact identity of one immutable Editor frame published by one Host session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EditorFrameRevision {
    source_id: u64,
    generation: u64,
}

/// Identity of one document decision issued by the workspace-intent owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorClosePromptToken {
    source_id: u64,
    generation: u64,
    document: EditorDocumentId,
}

impl EditorClosePromptToken {
    #[must_use]
    pub const fn document(self) -> EditorDocumentId {
        self.document
    }

    #[must_use]
    pub const fn resolve(self, decision: EditorCloseDecision) -> EditorWorkspaceIntentAction {
        EditorWorkspaceIntentAction::Resolve {
            prompt: self,
            decision,
        }
    }
}

/// Private publication clock used by the concrete Editor Host.
#[doc(hidden)]
#[derive(Debug)]
pub struct EditorFrameClock {
    source_id: u64,
    generation: u64,
    close_prompt_generation: u64,
}

impl EditorFrameClock {
    #[must_use]
    pub fn new() -> Self {
        let previous = NEXT_EDITOR_FRAME_SOURCE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .expect("editor frame source identity space is exhausted");
        Self {
            source_id: previous + 1,
            generation: 0,
            close_prompt_generation: 0,
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn issue_close_prompt(&mut self, document: EditorDocumentId) -> EditorClosePromptToken {
        self.close_prompt_generation = self
            .close_prompt_generation
            .checked_add(1)
            .expect("editor close prompt generation space is exhausted");
        EditorClosePromptToken {
            source_id: self.source_id,
            generation: self.close_prompt_generation,
            document,
        }
    }

    #[must_use]
    pub fn publish(
        &mut self,
        workspace: EditorWorkspaceModel,
        project: EditorProjectView,
        diagnostics: DiagnosticReport,
        reopen_target: EditorReopenTarget,
        field_feedback: Option<EditorFieldFeedback>,
    ) -> EditorFrameModel {
        let revision = EditorFrameRevision {
            source_id: self.source_id,
            generation: self.generation,
        };
        self.generation = self
            .generation
            .checked_add(1)
            .expect("editor frame generation space is exhausted");
        EditorFrameModel {
            revision,
            workspace,
            project,
            diagnostics,
            reopen_target,
            field_feedback,
            image_import: None,
            image_import_work_pending: false,
            source_asset_import: None,
            source_asset_import_work_pending: false,
            document_open: Default::default(),
            scene_preview_document: None,
        }
    }
}

impl Default for EditorFrameClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Exact source target exposed for the next Reopen action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorReopenTarget {
    Unavailable,
    OpenDocument {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    },
}

/// Stable field identity used by UI-neutral validation and action feedback.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EditorFieldTarget {
    document: EditorDocumentId,
    base_revision: SceneAuthoringRevision,
    entity: SceneEntityId,
    component: ComponentTypeId,
    component_version: ComponentSchemaVersion,
    field: ComponentFieldId,
}

impl EditorFieldTarget {
    #[must_use]
    pub fn new(
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        field: ComponentFieldId,
    ) -> Self {
        Self {
            document,
            base_revision,
            entity,
            component,
            component_version,
            field,
        }
    }

    #[must_use]
    pub const fn document(&self) -> EditorDocumentId {
        self.document
    }

    #[must_use]
    pub const fn base_revision(&self) -> SceneAuthoringRevision {
        self.base_revision
    }

    #[must_use]
    pub const fn entity(&self) -> &SceneEntityId {
        &self.entity
    }

    #[must_use]
    pub const fn component(&self) -> &ComponentTypeId {
        &self.component
    }

    #[must_use]
    pub const fn component_version(&self) -> ComponentSchemaVersion {
        self.component_version
    }

    #[must_use]
    pub const fn field(&self) -> &ComponentFieldId {
        &self.field
    }

    /// Compares durable field identity while intentionally ignoring the source revision.
    ///
    /// This lets a stale-action message attach to the corresponding field in the successor frame;
    /// the exact revision remains available through [`Self::base_revision`] for dispatch checks.
    #[must_use]
    pub fn same_field(&self, other: &Self) -> bool {
        self.document == other.document
            && self.entity == other.entity
            && self.component == other.component
            && self.component_version == other.component_version
            && self.field == other.field
    }
}

/// Adapter-originated validation failure that must become durable Host feedback.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorFieldValidationFailure {
    target: EditorFieldTarget,
    expected: ComponentValueKind,
}

impl EditorFieldValidationFailure {
    #[must_use]
    pub const fn new(target: EditorFieldTarget, expected: ComponentValueKind) -> Self {
        Self { target, expected }
    }

    #[must_use]
    pub const fn target(&self) -> &EditorFieldTarget {
        &self.target
    }

    #[must_use]
    pub const fn expected(&self) -> ComponentValueKind {
        self.expected
    }
}

/// Reason shown inline for the latest exact field failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorFieldFeedbackReason {
    InvalidValue(ComponentValueKind),
    StaleAction,
    MutationRejected,
}

/// Latest Host-owned inline feedback for one stable field target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorFieldFeedback {
    target: EditorFieldTarget,
    reason: EditorFieldFeedbackReason,
}

impl EditorFieldFeedback {
    #[must_use]
    pub const fn new(target: EditorFieldTarget, reason: EditorFieldFeedbackReason) -> Self {
        Self { target, reason }
    }

    #[must_use]
    pub const fn target(&self) -> &EditorFieldTarget {
        &self.target
    }

    #[must_use]
    pub const fn reason(&self) -> EditorFieldFeedbackReason {
        self.reason
    }
}

/// One coherent, post-drive Editor snapshot consumed by every UI Adapter.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorFrameModel {
    revision: EditorFrameRevision,
    workspace: EditorWorkspaceModel,
    project: EditorProjectView,
    diagnostics: DiagnosticReport,
    reopen_target: EditorReopenTarget,
    field_feedback: Option<EditorFieldFeedback>,
    image_import: Option<EditorImageImportView>,
    image_import_work_pending: bool,
    source_asset_import: Option<crate::EditorSourceAssetImportView>,
    source_asset_import_work_pending: bool,
    document_open: crate::EditorDocumentOpenView,
    scene_preview_document: Option<EditorDocumentId>,
}

impl EditorFrameModel {
    /// Returns the explicitly bound Scene source, independently of the active Inspector document.
    /// Its revision belongs to this frame's Workspace Scene tab; this is not picking authority.
    #[must_use]
    pub const fn scene_preview_document(&self) -> Option<EditorDocumentId> {
        self.scene_preview_document
    }

    #[doc(hidden)]
    #[must_use]
    pub fn with_scene_preview_document(mut self, document: Option<EditorDocumentId>) -> Self {
        self.scene_preview_document = document;
        self
    }

    #[doc(hidden)]
    #[must_use]
    pub fn with_source_asset_import(
        mut self,
        view: Option<crate::EditorSourceAssetImportView>,
        work_pending: bool,
    ) -> Self {
        self.source_asset_import = view;
        self.source_asset_import_work_pending = work_pending;
        self
    }

    #[must_use]
    pub const fn source_asset_import(&self) -> Option<&crate::EditorSourceAssetImportView> {
        self.source_asset_import.as_ref()
    }

    #[must_use]
    pub const fn source_asset_import_work_pending(&self) -> bool {
        self.source_asset_import_work_pending
    }

    /// Installs the exact project-global pending Open and terminal result.
    #[doc(hidden)]
    #[must_use]
    pub fn with_document_open(mut self, view: crate::EditorDocumentOpenView) -> Self {
        self.document_open = view;
        self
    }

    #[must_use]
    pub const fn document_open(&self) -> &crate::EditorDocumentOpenView {
        &self.document_open
    }
    /// Installs the Host-owned selected-image projection and project-global pending-work fact.
    #[doc(hidden)]
    #[must_use]
    pub fn with_image_import(
        mut self,
        image_import: Option<EditorImageImportView>,
        image_import_work_pending: bool,
    ) -> Self {
        self.image_import = image_import;
        self.image_import_work_pending = image_import_work_pending;
        self
    }

    #[must_use]
    pub const fn revision(&self) -> EditorFrameRevision {
        self.revision
    }

    #[must_use]
    pub const fn workspace(&self) -> &EditorWorkspaceModel {
        &self.workspace
    }

    #[must_use]
    pub const fn project(&self) -> &EditorProjectView {
        &self.project
    }

    #[must_use]
    pub const fn project_readiness(&self) -> crate::EditorProjectReadinessView {
        self.project.readiness()
    }

    #[must_use]
    pub const fn project_browser(&self) -> &crate::EditorProjectBrowserView {
        self.project.project_browser()
    }

    /// Returns image import details only when the current Browser selection admits them.
    #[must_use]
    pub const fn image_import(&self) -> Option<&EditorImageImportView> {
        self.image_import.as_ref()
    }

    /// Reports project-global image work independently of the current Browser selection.
    #[must_use]
    pub const fn image_import_work_pending(&self) -> bool {
        self.image_import_work_pending
    }

    #[must_use]
    pub const fn authoring_readiness(&self) -> crate::EditorAuthoringReadiness {
        self.project.readiness().authoring()
    }

    #[must_use]
    pub const fn runtime_availability(&self) -> crate::EditorRuntimeAvailability {
        self.project.readiness().runtime()
    }

    #[must_use]
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    #[must_use]
    pub const fn reopen_target(&self) -> EditorReopenTarget {
        self.reopen_target
    }

    #[must_use]
    pub const fn field_feedback(&self) -> Option<&EditorFieldFeedback> {
        self.field_feedback.as_ref()
    }

    #[must_use]
    pub const fn play(&self) -> &crate::EditorPlayView {
        self.project.play()
    }

    #[must_use]
    pub const fn persistence(&self) -> crate::EditorPersistenceView {
        self.project.persistence()
    }

    #[must_use]
    pub const fn project_content(&self) -> crate::EditorProjectContentView {
        self.project.project_content()
    }

    #[must_use]
    pub const fn workspace_intent(&self) -> crate::EditorWorkspaceIntentView {
        self.project.workspace_intent()
    }

    #[must_use]
    pub const fn runtime_edit_result(&self) -> Option<&crate::EditorRuntimeEditResult> {
        self.project.runtime_edit_result()
    }

    #[must_use]
    pub const fn apply_changes_result(&self) -> Option<&crate::EditorApplyChangesResult> {
        self.project.apply_changes_result()
    }
}

/// Workspace navigation emitted from one exact Editor frame.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorNavigationAction {
    OpenPrefabSource {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        entity: SceneEntityId,
    },
    SetActiveDocument {
        document: EditorDocumentId,
    },
    SelectEntity {
        document: EditorDocumentId,
        entity: Option<SceneEntityId>,
    },
    MarkExternalChanged {
        document: EditorDocumentId,
    },
}

impl EditorNavigationAction {
    #[must_use]
    pub const fn document(&self) -> EditorDocumentId {
        match self {
            Self::OpenPrefabSource { document, .. }
            | Self::SetActiveDocument { document }
            | Self::SelectEntity { document, .. }
            | Self::MarkExternalChanged { document } => *document,
        }
    }
}

/// One revision-bound document mutation.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorDocumentMutation {
    ApplyInspectorCommand {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        command: SceneInspectorCommand,
    },
    ApplyScenePatch {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        patch: ScenePatchDocument,
    },
    ApplyPrefabPatch {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        patch: ScenePatchDocument,
    },
    ApplyAuthoringFieldEdit {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        edit: AuthoringFieldEdit,
    },
    ApplyPrefabFieldEdit {
        edit: crate::EditorAtomicFieldEdit,
    },
    ComposeScene {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        command: SceneCompositionCommand,
    },
    ComposePrefab {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        command: PrefabCompositionCommand,
    },
    Undo {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    },
    Redo {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    },
}

impl EditorDocumentMutation {
    #[must_use]
    pub const fn document(&self) -> EditorDocumentId {
        match self {
            Self::ApplyInspectorCommand { document, .. }
            | Self::ApplyScenePatch { document, .. }
            | Self::ApplyPrefabPatch { document, .. }
            | Self::ApplyAuthoringFieldEdit { document, .. }
            | Self::ComposeScene { document, .. }
            | Self::ComposePrefab { document, .. }
            | Self::Undo { document, .. }
            | Self::Redo { document, .. } => *document,
            Self::ApplyPrefabFieldEdit { edit } => edit.target().document(),
        }
    }

    #[must_use]
    pub const fn base_revision(&self) -> SceneAuthoringRevision {
        match self {
            Self::ApplyInspectorCommand { base_revision, .. }
            | Self::ApplyScenePatch { base_revision, .. }
            | Self::ApplyPrefabPatch { base_revision, .. }
            | Self::ApplyAuthoringFieldEdit { base_revision, .. }
            | Self::ComposeScene { base_revision, .. }
            | Self::ComposePrefab { base_revision, .. }
            | Self::Undo { base_revision, .. }
            | Self::Redo { base_revision, .. } => *base_revision,
            Self::ApplyPrefabFieldEdit { edit } => edit.target().base_revision(),
        }
    }
}

/// Revision-bound persistence commands and result acknowledgement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPersistenceAction {
    Save {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    },
    Reopen {
        target: EditorReopenTarget,
    },
    AcknowledgeResult,
}

/// Dirty-close and exit arbitration actions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorWorkspaceIntentAction {
    Request(EditorWorkspaceIntent),
    Resolve {
        prompt: EditorClosePromptToken,
        decision: EditorCloseDecision,
    },
    /// Commits one fully prepared non-destructive workspace exit.
    CommitPrepared,
    /// Aborts one fully prepared non-destructive workspace exit.
    AbortPrepared,
    AcknowledgeResult,
}

/// Play-owner actions emitted from one exact Editor frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPlayAction {
    Play {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    },
    Cancel,
    Pause,
    Resume,
    StepFixedTick,
    Stop,
    Restart,
    RunLastGood,
    RetryRetirement,
    RetryClose,
    AcknowledgeResult,
}

/// UI-neutral action applied by the concrete Editor Host in list order.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorAction {
    Navigate(EditorNavigationAction),
    /// Reads direct source entity identities at an explicit open-document basis.
    QueryDocumentEntities(crate::EditorEntityQuery),
    /// Discovers actual component/root metadata at an explicit source basis.
    QueryDocumentAuthoring(crate::EditorAuthoringQuery),
    /// Copies one selected complete root after source and logical-cost admission.
    ReadDocumentRoot(crate::EditorRootRead),
    Mutate(EditorDocumentMutation),
    Extension(crate::EditorExtensionCommand),
    Persistence(EditorPersistenceAction),
    ProjectBrowser(EditorProjectBrowserAction),
    ImageImport(EditorImageImportAction),
    SourceAssetImport(crate::EditorSourceAssetImportAction),
    ProjectContent(crate::EditorProjectContentAction),
    WorkspaceIntent(EditorWorkspaceIntentAction),
    Play(EditorPlayAction),
    ReportFieldValidation(EditorFieldValidationFailure),
    Gesture(EditorGestureAction),
    RuntimeEdit {
        document: EditorDocumentId,
        request: EditorRuntimeEditRequest,
    },
    ApplyChanges {
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        generation: u64,
        request: SceneApplyChangesRequest,
    },
    AcknowledgeRuntimeEditResult,
    AcknowledgeApplyChangesResult,
}

impl EditorAction {
    #[doc(hidden)]
    #[must_use]
    pub const fn requires_current_frame(&self) -> bool {
        match self {
            Self::Navigate(_)
            | Self::ImageImport(_)
            | Self::SourceAssetImport(_)
            | Self::RuntimeEdit { .. }
            | Self::ApplyChanges { .. }
            | Self::Gesture(_) => true,
            Self::ProjectBrowser(action) => matches!(
                action,
                EditorProjectBrowserAction::Refresh | EditorProjectBrowserAction::Open { .. }
            ),
            Self::Play(action) => matches!(action, EditorPlayAction::Play { .. }),
            Self::Mutate(_)
            | Self::QueryDocumentEntities(_)
            | Self::QueryDocumentAuthoring(_)
            | Self::ReadDocumentRoot(_)
            | Self::Extension(_)
            | Self::Persistence(_)
            | Self::ProjectContent(_)
            | Self::WorkspaceIntent(_)
            | Self::ReportFieldValidation(_)
            | Self::AcknowledgeRuntimeEditResult
            | Self::AcknowledgeApplyChangesResult => false,
        }
    }

    #[doc(hidden)]
    #[must_use]
    pub fn field_target(&self) -> Option<EditorFieldTarget> {
        match self {
            Self::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
                document,
                base_revision,
                command,
            }) => inspector_field_target(*document, *base_revision, command),
            Self::ReportFieldValidation(failure) => Some(failure.target().clone()),
            Self::Mutate(EditorDocumentMutation::ApplyPrefabFieldEdit { edit }) => {
                Some(edit.target().clone())
            }
            Self::Gesture(action) => action.field_target().cloned(),
            _ => None,
        }
    }
}

/// Ordered commands with their originating frame for geometry-sensitive admission.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorActionBatch {
    frame_revision: EditorFrameRevision,
    actions: Vec<EditorAction>,
}

impl EditorActionBatch {
    #[must_use]
    pub fn new(
        frame_revision: EditorFrameRevision,
        actions: impl IntoIterator<Item = EditorAction>,
    ) -> Self {
        Self {
            frame_revision,
            actions: actions.into_iter().collect(),
        }
    }

    #[must_use]
    pub const fn frame_revision(&self) -> EditorFrameRevision {
        self.frame_revision
    }

    #[doc(hidden)]
    pub fn into_actions(self) -> Vec<EditorAction> {
        self.actions
    }

    #[doc(hidden)]
    #[must_use]
    pub fn first_field_target(&self) -> Option<EditorFieldTarget> {
        self.actions.iter().find_map(EditorAction::field_target)
    }

    #[doc(hidden)]
    #[must_use]
    pub fn requires_current_frame(&self) -> bool {
        self.actions
            .iter()
            .any(EditorAction::requires_current_frame)
    }
}

/// Host-level rejection that does not silently rebase an Editor action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorActionRejection {
    NoPublishedFrame,
    StaleFrame {
        expected: EditorFrameRevision,
        received: EditorFrameRevision,
    },
    DocumentNotInFrame {
        document: EditorDocumentId,
    },
    StaleDocumentFact {
        document: EditorDocumentId,
        expected: SceneAuthoringRevision,
        actual: Option<SceneAuthoringRevision>,
    },
    ActiveDocumentChanged {
        expected: EditorDocumentId,
        actual: Option<EditorDocumentId>,
    },
    FieldContractMismatch {
        document: EditorDocumentId,
    },
    StructureContractMismatch {
        document: EditorDocumentId,
    },
    RuntimeGenerationChanged {
        expected: u64,
        actual: Option<u64>,
    },
    RuntimeDocumentChanged {
        expected: EditorDocumentId,
        actual: Option<EditorDocumentId>,
    },
    InvalidReopenTarget,
}

/// Typed outcome for one action in an ordered Editor batch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorActionAcknowledgement {
    WorkspaceIntent(bool),
    RuntimeEdit(bool),
    ApplyChanges(bool),
    FieldValidation,
}

fn inspector_field_target(
    document: EditorDocumentId,
    base_revision: SceneAuthoringRevision,
    command: &SceneInspectorCommand,
) -> Option<EditorFieldTarget> {
    let (entity, component, component_version, field) = match command {
        SceneInspectorCommand::SetField {
            entity,
            component,
            component_version,
            field,
            ..
        }
        | SceneInspectorCommand::RemoveField {
            entity,
            component,
            component_version,
            field,
            ..
        }
        | SceneInspectorCommand::SetAssetRefField {
            entity,
            component,
            component_version,
            field,
            ..
        } => (entity, component, *component_version, field),
        SceneInspectorCommand::MoveEntity { .. } => return None,
    };
    Some(EditorFieldTarget::new(
        document,
        base_revision,
        entity.clone(),
        component.clone(),
        component_version,
        field.clone(),
    ))
}

/// Typed outcome for one action in an ordered Editor batch.
#[derive(Debug, Clone, PartialEq)]
pub enum EditorActionOutcome {
    Workspace(EditorWorkspaceCommandReport),
    /// Owned read-only page or exact query rejection; not a document mutation result.
    DocumentEntities(Result<crate::EditorEntityPage, crate::EditorDocumentQueryRejection>),
    DocumentAuthoring(Result<crate::EditorAuthoringPage, crate::EditorDocumentQueryRejection>),
    DocumentRoot(Result<crate::EditorRootObservation, crate::EditorDocumentQueryRejection>),
    Persistence(EditorPersistenceRequestResult),
    ProjectBrowser(EditorProjectBrowserActionOutcome),
    ImageImport(EditorImageImportActionOutcome),
    SourceAssetImport(crate::EditorSourceAssetImportActionOutcome),
    ProjectContent(crate::EditorProjectContentRefreshRequestResult),
    WorkspaceIntent(EditorWorkspaceIntentRequestResult),
    Play(EditorPlayRequestResult),
    Gesture(EditorGestureOutcome),
    RuntimeEdit(Result<(), EditorRuntimeEditRejection>),
    ApplyChanges(Result<(), EditorApplyChangesRejection>),
    Acknowledged(EditorActionAcknowledgement),
    Rejected(EditorActionRejection),
}

#[cfg(test)]
mod entity_query_tests {
    use super::*;
    use crate::{
        EditorDocumentQueryBasis, EditorEntityQuery, EditorEntityQueryLimits,
        EditorEntityQueryPosition,
    };

    #[test]
    fn query_uses_document_basis_but_mixed_navigation_batch_keeps_frame_admission() {
        let document = EditorDocumentId::from_raw(1);
        let basis = EditorDocumentQueryBasis {
            document,
            revision: SceneAuthoringRevision::initial(),
            schema: None,
        };
        let query = EditorAction::QueryDocumentEntities(EditorEntityQuery {
            position: EditorEntityQueryPosition::Start(basis.clone()),
            limits: EditorEntityQueryLimits::new(1, 1).unwrap(),
        });
        let queries = [
            query,
            EditorAction::QueryDocumentAuthoring(crate::EditorAuthoringQuery {
                position: crate::EditorAuthoringQueryPosition::Start(
                    crate::EditorAuthoringQueryTarget {
                        basis: basis.clone(),
                        entity: SceneEntityId::new("item").unwrap(),
                        projection: crate::EditorAuthoringProjection::Components,
                    },
                ),
                limits: crate::EditorAuthoringQueryLimits::new(1, 1, 1024).unwrap(),
            }),
            EditorAction::ReadDocumentRoot(crate::EditorRootRead {
                target: crate::EditorRootTarget {
                    basis,
                    entity: SceneEntityId::new("item").unwrap(),
                    component: ComponentTypeId::new("game.Data"),
                    field: ComponentFieldId::new("amount"),
                },
                limits: crate::EditorRootReadLimits::new(1024, 1, 8).unwrap(),
            }),
        ];
        for query in queries {
            assert!(!query.requires_current_frame());
            assert!(query.field_target().is_none());
            let frame_revision = EditorFrameRevision {
                source_id: 1,
                generation: 0,
            };
            assert!(
                !EditorActionBatch::new(frame_revision, [query.clone()]).requires_current_frame()
            );
            assert!(
                EditorActionBatch::new(
                    frame_revision,
                    [
                        query,
                        EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
                            document
                        }),
                    ],
                )
                .requires_current_frame()
            );
        }
    }
}

/// Outcomes for an ordered action batch, retaining its originating frame context.
#[derive(Debug, Clone, PartialEq)]
pub struct EditorActionBatchReport {
    frame_revision: EditorFrameRevision,
    outcomes: Vec<EditorActionOutcome>,
}

impl EditorActionBatchReport {
    #[doc(hidden)]
    #[must_use]
    pub fn new(
        frame_revision: EditorFrameRevision,
        outcomes: impl IntoIterator<Item = EditorActionOutcome>,
    ) -> Self {
        Self {
            frame_revision,
            outcomes: outcomes.into_iter().collect(),
        }
    }

    #[must_use]
    pub const fn frame_revision(&self) -> EditorFrameRevision {
        self.frame_revision
    }

    #[must_use]
    pub fn outcomes(&self) -> &[EditorActionOutcome] {
        &self.outcomes
    }

    #[must_use]
    pub fn into_outcomes(self) -> Vec<EditorActionOutcome> {
        self.outcomes
    }
}
