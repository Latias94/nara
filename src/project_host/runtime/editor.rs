use std::{collections::BTreeMap, fmt, sync::Arc, time::Duration};

mod actions;
mod asset_metadata;
#[cfg(feature = "runtime-ui")]
mod authoring_images;
#[cfg(any(feature = "presentation-2d", feature = "runtime-ui"))]
mod authoring_preview;
mod authoring_runtime;
mod authoring_sources;
mod content_refresh;
mod document_open;
mod extension_documents;
mod external_play;
#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
mod game_view;
mod image_import;
mod model;
mod open;
mod persistence;
mod play;
mod prefab_conversion;
mod prepared_edit;
#[cfg(feature = "presentation-2d")]
mod presentation;
#[cfg(feature = "presentation-2d")]
mod preview;
mod project_browser;
#[cfg(feature = "presentation-2d")]
mod scene_capture;
mod schema_owner;
mod source_admission;
mod source_asset_import;
mod source_tools;
#[cfg(test)]
mod test_fixtures;
mod trusted_play;
#[cfg(feature = "runtime-ui")]
mod ui_authoring;
#[cfg(feature = "presentation-ui")]
mod ui_preview;

#[cfg(test)]
pub(crate) use authoring_runtime::EditorImageRuntimeFailure;
pub(crate) use document_open::EditorDocumentOpenRequestResult;
#[cfg(test)]
pub(crate) use document_open::preparation::OpenTestPhase;
pub use extension_documents::EditorExtensionDocumentHostError;
#[doc(hidden)]
pub use external_play::{
    EditorExternalPlayReceiptError, SupervisedDesktopSourceAdmissionError,
    SupervisedDesktopSourceAuthority, admit_supervised_desktop_source_receipt,
};
#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
pub use game_view::{
    EditorGameInputBatch, EditorGameInputEnvelope, EditorGameInputEvent, EditorGameStartActivation,
    EditorGameStartRequest, EditorGameViewBridgeFailure, EditorGameViewCandidate,
    EditorGameViewCandidateIdentity, EditorGameViewCandidateParts, EditorGameViewCompletion,
    EditorGameViewCompletionDisposition,
};
#[cfg(test)]
pub(crate) use image_import::{
    EditorImageImportFailure, EditorImageImportRequestResult, EditorImageImportState,
    EditorImageLastGood,
};
pub use open::{
    EditorProjectOpenAttempt, EditorProjectOpenFailure, EditorProjectOpenFailureImpact,
    EditorProjectOpenFailureSource, EditorProjectOpenHost, EditorProjectOpenPhase,
    EditorProjectOpenProgress, EditorProjectOpenTasksCloseReport,
};
#[cfg(test)]
pub(crate) use persistence::{ReopenTestPhase, SaveTestPhase};
pub use prepared_edit::EditorEditPreparationError;
#[cfg(feature = "presentation-2d")]
pub use presentation::{
    EditorAcceptedPreview, EditorPresentationError, EditorPresentationImage, EditorPresentedToken,
    EditorPreviewPresentation,
};
#[cfg(feature = "presentation-2d")]
pub use preview::{EditorPreviewCandidate, EditorPreviewContentStamp, EditorPreviewHit};
pub use schema_owner::{
    EditorSchemaOwnerAdoptionError, EditorSchemaOwnerAdoptionFailure, EditorSchemaOwnerAdoptionId,
    EditorSchemaOwnerAdoptionOutcome, EditorSchemaOwnerAdoptionPhase,
    EditorSchemaOwnerAdoptionView,
};
pub use source_asset_import::{EditorSourceAssetDomain, EditorSourceAssetImporters};
pub use source_tools::{
    EditorSourceToolCommand, EditorSourceToolDefinition, EditorSourceToolInstallation,
    EditorSourceToolInstallationError, EditorSourceToolInstallationInspection,
    EditorSourceToolInstallationOrphan, EditorSourceToolInstallationOutcome,
    EditorSourceToolInstallationPlan, EditorSourceToolInstallationReceipt,
    EditorSourceToolInstalledMember, EditorSourceToolManagedError, EditorSourceToolManagedHost,
    EditorSourceToolManagedHostDigests, EditorSourceToolReleaseError,
    EditorSourceToolReleaseMember, EditorSourceToolReleaseMemberKind,
    EditorSourceToolReleaseRequirement, PreparedEditorSourceToolContent,
    PreparedEditorSourceToolInstallations, PreparedEditorSourceToolRelease,
    PreparedEditorSourceToolSelection, PreparedEditorSourceToolWithdrawal,
};
#[cfg(feature = "runtime-ui")]
use ui_authoring::EditorUiAuthoringProjectionCache;
#[cfg(feature = "runtime-ui")]
pub use ui_authoring::{EditorUiAuthoringEditError, EditorUiAuthoringFrame};
#[cfg(feature = "presentation-ui")]
pub use ui_preview::EditorUiPreviewCandidate;

use nara_app::{
    __RuntimeDriverPort, App, CoreStage, Plugin, PluginCategory, PluginDeclaration,
    PluginDefinition, PluginDefinitionId, PluginError, PluginId, RuntimeClosePolicy,
    RuntimeControl, RuntimeControlStatus, RuntimeControlTicket, RuntimeState,
};
use nara_diagnostic::DiagnosticReport;
use nara_ecs::{Entity, Mut, Resource, World};
use nara_fs::{ContentDigest, DirectoryCapability, FileIdentity, RelativePath};
use nara_image::ImageImportSettings;
#[cfg(test)]
use nara_reflect::ComponentRegistry;
use nara_reflect::{
    ComponentCapability, ComponentRegistrySnapshot, component_registry,
    report_component_registry_authority_fault,
};
#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
use nara_render::Extent2d;
use nara_scene::{SceneEntitySource, UnavailableRecordManifest};
use nara_tasks::TaskShutdownReport;
#[cfg(test)]
use nara_tooling::EditorFrameRevision;
use nara_tooling::{
    __export_apply_changes_from_world, EditorAuthoringReadiness, EditorCloseDecision,
    EditorClosePromptToken, EditorDocumentId, EditorDocumentKind, EditorExternalPlayRequest,
    EditorExternalPlayView, EditorFieldFeedback, EditorFrameClock, EditorFrameModel,
    EditorGestureCancelReason, EditorGestureController, EditorPersistenceAttemptId,
    EditorPersistenceAuthority, EditorPersistenceCommand, EditorPersistenceFailureStage,
    EditorPersistenceOperation, EditorPersistenceRejection, EditorPersistenceRequestKind,
    EditorPersistenceRequestResult, EditorPersistenceResult, EditorPersistenceView,
    EditorPlayCommand, EditorPlayFailure, EditorPlayOperation, EditorPlayOperationResult,
    EditorPlayRejection, EditorPlayRequestResult, EditorPlayState, EditorPlayView,
    EditorProjectContentGeneration, EditorProjectContentView, EditorProjectReadinessView,
    EditorRuntimeAvailability, EditorRuntimeEditRejection, EditorRuntimeEditRequest,
    EditorRuntimeEditResult, EditorRuntimeUnavailableReason, EditorWorkspace,
    EditorWorkspaceCommand, EditorWorkspaceCommandReport, EditorWorkspaceIntent,
    EditorWorkspaceIntentPhase, EditorWorkspaceIntentRejection, EditorWorkspaceIntentRequestResult,
    EditorWorkspaceIntentResult, EditorWorkspaceIntentView, SceneApplyChangesComponentStatus,
    SceneApplyChangesRequest,
};
use nara_tooling::{EditorApplyChangesRejection, EditorApplyChangesResult};

use crate::ProductRecipe;
use crate::project_host::authoring::{
    AuthoringDocumentAdmissionError, AuthoringProjectReadiness, AuthoringProjectReceipt,
    AuthoringProjectSessionSnapshot, ProjectSourceIndexFailure, ProjectSourceIndexSnapshot,
    ProjectSourceIndexStateKind, first_party_authoring_preview_registry,
};
use crate::project_host::composition::{
    built_in_schema_providers, editor_play_runtime_plugins_with_recipe,
    resolve_project_runtime_request, runtime_host_policy_contribution,
};
use crate::project_host::runtime_agreement::prepare_runtime_schema_agreement;
use crate::project_host::{
    ProjectContentLoader, ProjectSettingsCandidate, ingest_project_manifest,
};

use super::{
    PROJECT_MANIFEST, ProjectHost, RuntimePlan, RuntimePluginOverlay, runtime_plan_failure_report,
    runtime_plan_selected_report, single_error, single_warning,
    single_warning_with_project_relative,
};
use crate::project_host::persistence::{
    EditorPersistenceReceipt, OpenedSceneReopen, PrefabPersistenceHost, ScenePersistenceHost,
    SceneReopenOutcome, SceneSaveCandidate,
};

const EDITOR_RUNTIME_BRIDGE_PLUGIN_ID: PluginId = PluginId::new("nara.editor-runtime-bridge");
const EDITOR_RUNTIME_BRIDGE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.editor-runtime-bridge", 1);
const EDITOR_RUNTIME_BRIDGE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(EDITOR_RUNTIME_BRIDGE_PLUGIN_ID, PluginCategory::Tooling);

#[derive(Debug, Default)]
struct EditorRuntimeBridgePlugin;

impl Plugin for EditorRuntimeBridgePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &EDITOR_RUNTIME_BRIDGE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<EditorRuntimeBridge>()?
            .add_systems(CoreStage::Last, apply_editor_runtime_edit)?;
        Ok(())
    }
}

fn editor_runtime_bridge_definition() -> PluginDefinition {
    PluginDefinition::infallible::<EditorRuntimeBridgePlugin, _>(
        EDITOR_RUNTIME_BRIDGE_DEFINITION_ID,
        b"editor-runtime-bridge-v1",
        EditorRuntimeBridgePlugin::default,
    )
}

#[derive(Debug, Default, Resource)]
struct EditorRuntimeBridge {
    pending: Option<EditorRuntimeEditRequest>,
    result: Option<EditorRuntimeEditResult>,
}

struct PendingEditorRuntimeEdit<'bridge> {
    bridge: &'bridge mut EditorRuntimeBridge,
    request: Option<EditorRuntimeEditRequest>,
}

impl<'bridge> PendingEditorRuntimeEdit<'bridge> {
    fn take(bridge: &'bridge mut EditorRuntimeBridge) -> Option<Self> {
        bridge.pending.take().map(|request| Self {
            bridge,
            request: Some(request),
        })
    }

    fn request(&self) -> &EditorRuntimeEditRequest {
        self.request
            .as_ref()
            .expect("pending editor runtime edit always retains its request until completion")
    }

    fn complete(mut self, result: EditorRuntimeEditResult) {
        self.request.take();
        self.bridge.result = Some(result);
    }
}

impl Drop for PendingEditorRuntimeEdit<'_> {
    fn drop(&mut self) {
        if let Some(request) = self.request.take() {
            self.bridge.pending = Some(request);
        }
    }
}

enum EditorRuntimeBridgeInput {
    Submit(EditorRuntimeEditRequest),
    TakeResult,
    Cancel,
}

enum EditorRuntimeBridgeOutput {
    Submitted,
    Busy,
    Result(Option<EditorRuntimeEditResult>),
}

impl __RuntimeDriverPort for EditorRuntimeBridge {
    type Input = EditorRuntimeBridgeInput;
    type Output = EditorRuntimeBridgeOutput;

    fn apply_driver_input(&mut self, input: Self::Input) -> Self::Output {
        match input {
            EditorRuntimeBridgeInput::Submit(request) => {
                if self.pending.is_some() || self.result.is_some() {
                    EditorRuntimeBridgeOutput::Busy
                } else {
                    self.pending = Some(request);
                    EditorRuntimeBridgeOutput::Submitted
                }
            }
            EditorRuntimeBridgeInput::TakeResult => {
                EditorRuntimeBridgeOutput::Result(self.result.take())
            }
            EditorRuntimeBridgeInput::Cancel => {
                let result = self
                    .pending
                    .take()
                    .map(EditorRuntimeEditResult::Cancelled)
                    .or_else(|| self.result.take());
                EditorRuntimeBridgeOutput::Result(result)
            }
        }
    }
}

fn apply_editor_runtime_edit(world: &mut World) {
    if !world.contains_resource::<EditorRuntimeBridge>() {
        return;
    }
    world.resource_scope(|world, mut bridge: Mut<EditorRuntimeBridge>| {
        let Some(pending) = PendingEditorRuntimeEdit::take(&mut bridge) else {
            return;
        };
        if report_component_registry_authority_fault(world).is_err() {
            let request = pending.request().clone();
            pending.complete(EditorRuntimeEditResult::Rejected {
                request,
                reason: EditorRuntimeEditRejection::ApplyRejected,
            });
            return;
        }
        let registry = component_registry(world)
            .ok_or(())
            .and_then(|registry| registry.snapshot().map_err(|_| ()));
        let mut result = match registry {
            Ok(registry) => apply_runtime_edit(world, &registry, pending.request()),
            Err(_) => EditorRuntimeEditResult::Rejected {
                request: pending.request().clone(),
                reason: EditorRuntimeEditRejection::ApplyRejected,
            },
        };
        if report_component_registry_authority_fault(world).is_err() {
            result = EditorRuntimeEditResult::Rejected {
                request: pending.request().clone(),
                reason: EditorRuntimeEditRejection::ApplyRejected,
            };
        }
        pending.complete(result);
    });
}

fn apply_runtime_edit(
    world: &mut World,
    registry: &ComponentRegistrySnapshot,
    request: &EditorRuntimeEditRequest,
) -> EditorRuntimeEditResult {
    let mut target = None;
    let mut matches = 0_usize;
    let mut query = world.query::<(Entity, &SceneEntitySource)>();
    for (entity, source) in query.iter(world) {
        if source.entity_id == request.entity {
            matches = matches.saturating_add(1);
            target = Some(entity);
            if matches > 1 {
                break;
            }
        }
    }
    let Some(entity) = target else {
        return EditorRuntimeEditResult::Rejected {
            request: request.clone(),
            reason: EditorRuntimeEditRejection::MissingEntity,
        };
    };
    if matches > 1 {
        return EditorRuntimeEditResult::Rejected {
            request: request.clone(),
            reason: EditorRuntimeEditRejection::AmbiguousEntity,
        };
    }
    let Some(schema) = registry.schema(&request.component) else {
        return EditorRuntimeEditResult::Rejected {
            request: request.clone(),
            reason: EditorRuntimeEditRejection::UnknownComponent,
        };
    };
    if schema.version != request.component_version {
        return EditorRuntimeEditResult::Rejected {
            request: request.clone(),
            reason: EditorRuntimeEditRejection::SchemaVersionMismatch,
        };
    }
    if !schema.has_capability(ComponentCapability::Edit) {
        return EditorRuntimeEditResult::Rejected {
            request: request.clone(),
            reason: EditorRuntimeEditRejection::NotEditable,
        };
    }
    let Some(field) = registry.resolve_field(&request.component, &request.field) else {
        return EditorRuntimeEditResult::Rejected {
            request: request.clone(),
            reason: EditorRuntimeEditRejection::UnknownField,
        };
    };
    if !field.has_capability(ComponentCapability::Edit) {
        return EditorRuntimeEditResult::Rejected {
            request: request.clone(),
            reason: EditorRuntimeEditRejection::NotEditable,
        };
    }
    let mut value = match registry.encode_component(&request.component, world, entity) {
        Ok(Some(value)) => value,
        Ok(None) => {
            return EditorRuntimeEditResult::Rejected {
                request: request.clone(),
                reason: EditorRuntimeEditRejection::MissingComponent,
            };
        }
        Err(_) => {
            return EditorRuntimeEditResult::Rejected {
                request: request.clone(),
                reason: EditorRuntimeEditRejection::InvalidValue,
            };
        }
    };
    if value.set_path(field.path(), request.value.clone()).is_err() {
        return EditorRuntimeEditResult::Rejected {
            request: request.clone(),
            reason: EditorRuntimeEditRejection::InvalidValue,
        };
    }
    let Ok(prepared) = registry.preflight_component(&request.component, &value) else {
        return EditorRuntimeEditResult::Rejected {
            request: request.clone(),
            reason: EditorRuntimeEditRejection::InvalidValue,
        };
    };
    if prepared.apply(world, entity).is_err() {
        return EditorRuntimeEditResult::Rejected {
            request: request.clone(),
            reason: EditorRuntimeEditRejection::ApplyRejected,
        };
    }
    EditorRuntimeEditResult::Applied(request.clone())
}

/// Advanced Rust configuration for one trusted compiled Editor project session.
#[doc(hidden)]
pub struct TrustedCompiledEditorProjectIntent {
    profile: Option<String>,
    cleanup_policy: RuntimeClosePolicy,
    recipe: ProductRecipe,
}

impl fmt::Debug for TrustedCompiledEditorProjectIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TrustedCompiledEditorProjectIntent")
            .field("profile_present", &self.profile.is_some())
            .field("cleanup_policy", &self.cleanup_policy)
            .field("recipe", &self.recipe)
            .finish_non_exhaustive()
    }
}

impl Default for TrustedCompiledEditorProjectIntent {
    fn default() -> Self {
        Self::new()
    }
}

impl TrustedCompiledEditorProjectIntent {
    #[must_use]
    pub fn new() -> Self {
        Self {
            profile: None,
            cleanup_policy: RuntimeClosePolicy::default(),
            recipe: ProductRecipe::new(),
        }
    }

    #[must_use]
    pub fn with_profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }

    #[must_use]
    pub fn with_cleanup_timeout(mut self, timeout: Duration) -> Self {
        self.cleanup_policy = RuntimeClosePolicy::new(timeout);
        self
    }

    /// Uses one pure product recipe for this Editor project session.
    #[must_use]
    pub fn with_recipe(mut self, recipe: ProductRecipe) -> Self {
        self.recipe = recipe;
        self
    }
}

#[derive(Debug)]
#[must_use = "a failed Open can retain authoring cleanup until physical retirement"]
pub struct EditorProjectOpenError {
    diagnostics: DiagnosticReport,
    retirement: Option<Box<authoring_runtime::EditorAuthoringRetirement>>,
}

impl EditorProjectOpenError {
    fn new(diagnostics: DiagnosticReport) -> Self {
        Self {
            diagnostics,
            retirement: None,
        }
    }

    fn with_runtime(mut self, runtime: authoring_runtime::EditorAuthoringRuntime) -> Self {
        assert!(
            self.retirement.is_none(),
            "one unpublished authoring owner belongs to this failure"
        );
        self.retirement = Some(Box::new(runtime.into()));
        self
    }

    #[must_use]
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    /// Candidate refusal does not establish physical cleanup completion.
    #[must_use]
    pub fn retirement_state(&self) -> nara_app::RuntimeCandidateRetirementState {
        self.retirement.as_ref().map_or(
            nara_app::RuntimeCandidateRetirementState::Retired,
            |owner| owner.state(),
        )
    }

    #[must_use]
    pub fn close_evidence(&self) -> Option<&nara_app::RuntimeCloseEvidence> {
        self.retirement
            .as_ref()
            .and_then(|owner| owner.close_evidence())
    }

    /// Advances retained cleanup once on the App's construction thread.
    pub fn advance_retirement(&mut self) -> nara_app::RuntimeCandidateRetirementState {
        if let Some(owner) = self.retirement.as_mut() {
            let _ = owner.poll();
        }
        self.retirement_state()
    }
}

impl From<authoring_runtime::EditorAuthoringConstructionFailure> for EditorProjectOpenError {
    fn from(failure: authoring_runtime::EditorAuthoringConstructionFailure) -> Self {
        Self {
            diagnostics: failure.diagnostics,
            retirement: Some(Box::new(failure.retirement)),
        }
    }
}

impl fmt::Display for EditorProjectOpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("editor project session could not be opened")
    }
}

impl std::error::Error for EditorProjectOpenError {}

enum PendingPersistence {
    Save {
        lane: persistence::PersistenceLane,
        candidate: PendingSave,
        authoring: Option<nara_scene::AuthoringDocumentReadmission>,
    },
    Saving(persistence::BackgroundSave),
    Reopen(persistence::PendingReopen),
    Reopening(persistence::BackgroundReopen),
}

enum PendingSave {
    Scene(SceneSaveCandidate),
    Prefab(crate::project_host::persistence::PrefabSaveCandidate),
    Extension(nara_tooling::EditorPersistenceCheckpoint),
}

#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
#[derive(Clone, Copy)]
struct ActiveEditorGameView {
    start: EditorGameStartRequest,
    in_flight: Option<EditorGameViewCandidateIdentity>,
}

#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
impl ActiveEditorGameView {
    const fn new(start: EditorGameStartRequest) -> Self {
        Self {
            start,
            in_flight: None,
        }
    }
}

#[derive(Clone, Copy)]
struct EditorPlaySource {
    document: EditorDocumentId,
    revision: nara_scene::SceneAuthoringRevision,
}

enum EditorRuntimeOwnerState {
    Empty,
    Preparing {
        source: EditorPlaySource,
        operation: EditorPlayOperation,
    },
    Starting {
        source: EditorPlaySource,
        operation: EditorPlayOperation,
        attempt: super::RuntimeStartAttempt,
    },
    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    AwaitingGameTarget {
        source: EditorPlaySource,
        operation: EditorPlayOperation,
        request: EditorGameStartRequest,
        prepared: super::PreparedRuntimeStart,
    },
    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    CommittingGameTarget {
        source: EditorPlaySource,
        operation: EditorPlayOperation,
        request: EditorGameStartRequest,
        prepared: super::PreparedRuntimeStart,
    },
    CancellingStart {
        source: EditorPlaySource,
        terminal: EditorPlayOperationResult,
        attempt: super::RuntimeStartAttempt,
    },
    RetiringPlay {
        source: EditorPlaySource,
        terminal: EditorPlayOperationResult,
    },
    Active {
        source: EditorPlaySource,
        #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
        game: Option<ActiveEditorGameView>,
    },
    RetirementIncomplete {
        source: EditorPlaySource,
        terminal: EditorPlayOperationResult,
    },
    Transitioning,
}

impl EditorRuntimeOwnerState {
    fn source(&self) -> Option<EditorPlaySource> {
        match self {
            Self::Preparing { source, .. }
            | Self::Starting { source, .. }
            | Self::CancellingStart { source, .. }
            | Self::RetiringPlay { source, .. }
            | Self::RetirementIncomplete { source, .. }
            | Self::Active { source, .. } => Some(*source),
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            Self::AwaitingGameTarget { source, .. } | Self::CommittingGameTarget { source, .. } => {
                Some(*source)
            }
            Self::Empty => None,
            Self::Transitioning => {
                unreachable!("an Editor Runtime source is not observed during a transition")
            }
        }
    }

    fn take(&mut self) -> Self {
        assert!(
            !matches!(self, Self::Transitioning),
            "an Editor Runtime owner cannot be taken during a transition"
        );
        std::mem::replace(self, Self::Transitioning)
    }
}

#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
#[derive(Clone, Copy, Debug, PartialEq)]
enum EditorStepPresentationState {
    AwaitingCandidate {
        generation: u64,
    },
    AwaitingPresentation {
        identity: EditorGameViewCandidateIdentity,
    },
}

#[derive(Clone, Copy)]
struct PendingEditorControl {
    operation: EditorPlayOperation,
    ticket: RuntimeControlTicket,
}

struct PendingApplyChanges {
    generation: u64,
    document: EditorDocumentId,
    document_revision: nara_scene::SceneAuthoringRevision,
    source_revision: nara_scene::SceneAuthoringRevision,
    request: SceneApplyChangesRequest,
}

#[derive(Debug)]
enum PendingWorkspaceIntent {
    CloseDocument(PendingDocumentClose),
    Exit(PendingExitPreparation),
}

#[derive(Debug, Clone, Copy)]
enum WorkspaceIntentProgress {
    AwaitingDecision,
    Saving(EditorPersistenceAttemptId),
    RetiringRuntime,
    Prepared,
}

impl WorkspaceIntentProgress {
    const fn phase(self) -> EditorWorkspaceIntentPhase {
        match self {
            Self::AwaitingDecision => EditorWorkspaceIntentPhase::AwaitingDecision,
            Self::Saving(_) => EditorWorkspaceIntentPhase::Saving,
            Self::RetiringRuntime => EditorWorkspaceIntentPhase::RetiringRuntime,
            Self::Prepared => EditorWorkspaceIntentPhase::Prepared,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct PendingDocumentClose {
    document: EditorDocumentId,
    phase: WorkspaceIntentProgress,
    prompt: Option<EditorClosePromptToken>,
}

#[derive(Debug)]
struct PendingExitPreparation {
    documents: Box<[EditorDocumentId]>,
    cursor: usize,
    phase: WorkspaceIntentProgress,
    prompt: Option<EditorClosePromptToken>,
}

impl PendingWorkspaceIntent {
    const fn intent(&self) -> EditorWorkspaceIntent {
        match self {
            Self::CloseDocument(pending) => EditorWorkspaceIntent::CloseDocument {
                document: pending.document,
            },
            Self::Exit(_) => EditorWorkspaceIntent::Exit,
        }
    }

    const fn phase(&self) -> EditorWorkspaceIntentPhase {
        match self {
            Self::CloseDocument(pending) => pending.phase.phase(),
            Self::Exit(pending) => pending.phase.phase(),
        }
    }

    fn decision_document(&self) -> Option<EditorDocumentId> {
        match self {
            Self::CloseDocument(pending) => Some(pending.document),
            Self::Exit(pending) => pending.current_document(),
        }
    }

    fn set_progress(&mut self, phase: WorkspaceIntentProgress) {
        match self {
            Self::CloseDocument(pending) => pending.phase = phase,
            Self::Exit(pending) => pending.phase = phase,
        }
    }

    fn save_attempt(&self) -> Option<EditorPersistenceAttemptId> {
        let progress = match self {
            Self::CloseDocument(pending) => pending.phase,
            Self::Exit(pending) => pending.phase,
        };
        match progress {
            WorkspaceIntentProgress::Saving(attempt) => Some(attempt),
            _ => None,
        }
    }

    fn prompt(&self) -> Option<EditorClosePromptToken> {
        match self {
            Self::CloseDocument(pending) => pending.prompt,
            Self::Exit(pending) => pending.prompt,
        }
    }

    fn set_prompt(&mut self, prompt: Option<EditorClosePromptToken>) {
        match self {
            Self::CloseDocument(pending) => pending.prompt = prompt,
            Self::Exit(pending) => pending.prompt = prompt,
        }
    }
}

impl PendingExitPreparation {
    fn new(documents: Vec<EditorDocumentId>) -> Self {
        Self {
            documents: documents.into_boxed_slice(),
            cursor: 0,
            phase: WorkspaceIntentProgress::AwaitingDecision,
            prompt: None,
        }
    }

    fn current_document(&self) -> Option<EditorDocumentId> {
        self.documents.get(self.cursor).copied()
    }
}

struct PublishedEditorFrame {
    model: Arc<EditorFrameModel>,
    source_index: Option<Arc<ProjectSourceIndexSnapshot>>,
    image_import: Option<image_import::EditorImageImportFrame>,
}

enum EditorExecutionLane {
    Authoring(AuthoringProjectSessionSnapshot),
    TrustedCompiled(Box<TrustedCompiledEditorLane>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AuthoringPrefabSourceObservation {
    identity: FileIdentity,
    digest: ContentDigest,
}

impl AuthoringPrefabSourceObservation {
    const fn new(identity: FileIdentity, digest: ContentDigest) -> Self {
        Self { identity, digest }
    }
}

#[derive(Clone)]
struct EditorScenePersistenceLane {
    project_path: Arc<str>,
    source_key: nara_scene::SourceDocumentKey,
    document: EditorDocumentId,
    host: persistence::RetainedHost<ScenePersistenceHost>,
    authoring_source: Option<crate::project_host::authoring::document::AuthoringDocumentSourceFact>,
}

#[derive(Clone)]
struct EditorPrefabPersistenceLane {
    project_path: Arc<str>,
    source_key: nara_scene::SourceDocumentKey,
    document: EditorDocumentId,
    host: persistence::RetainedHost<PrefabPersistenceHost>,
    authoring_source: Option<crate::project_host::authoring::document::AuthoringDocumentSourceFact>,
}

struct TrustedCompiledEditorLane {
    plan: RuntimePlan,
    content_candidate: ProjectSettingsCandidate,
    content_loader: Arc<ProjectContentLoader>,
    snapshot: crate::project_host::ProjectContentSnapshot,
    prefab_root: DirectoryCapability,
    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    game_view_config: Option<game_view::EditorGameViewCaptureConfig>,
    runtime_host: ProjectHost,
    close_summary: trusted_play::EditorPlayCloseSummary,
    runtime_owner: EditorRuntimeOwnerState,
    pending_control: Option<PendingEditorControl>,
    reported_close_failure_generation: Option<u64>,
    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    pending_step_presentation: Option<EditorStepPresentationState>,
    runtime_edit_result: Option<EditorRuntimeEditResult>,
    #[cfg(test)]
    fault_after_next_runtime_edit: bool,
    pending_apply_changes: Option<PendingApplyChanges>,
    apply_changes_result: Option<EditorApplyChangesResult>,
}

/// Concrete product Host for one Editor workspace and its future single Play owner.
///
/// Product Adapters consume [`Self::current_frame`], return one ordered action batch through
/// [`Self::dispatch_editor_actions`], and publish the next frame through
/// [`Self::drive_editor_frame`]. Crate-private helpers remain owner-conformance seams only.
pub struct EditorProjectSession {
    closing: bool,
    workspace: EditorWorkspace,
    persistence_authority: EditorPersistenceAuthority,
    scene_lanes: BTreeMap<EditorDocumentId, EditorScenePersistenceLane>,
    scene_preview_document: Option<EditorDocumentId>,
    #[cfg(feature = "presentation-2d")]
    scene_capture: scene_capture::EditorSceneCapture,
    execution: EditorExecutionLane,
    project_content: EditorProjectContentView,
    next_project_content_generation: u64,
    prefab_content_refresh:
        BTreeMap<nara_asset::AssetPath, content_refresh::PrefabContentRefreshObligation>,
    prefab_lanes: BTreeMap<EditorDocumentId, EditorPrefabPersistenceLane>,
    extension_documents: extension_documents::EditorExtensionDocumentsHost,
    pending_document_open: Option<document_open::PendingDocumentOpen>,
    next_document_open_request: u64,
    document_open_result: Option<nara_tooling::EditorDocumentOpenResult>,
    #[cfg(test)]
    next_document_open_gate: Option<document_open::preparation::OpenTestGate>,
    project_browser: project_browser::EditorProjectBrowserHost,
    image_import: image_import::EditorImageImportHost,
    source_asset_import: source_asset_import::EditorSourceAssetImportHost,
    prefab_conversion: prefab_conversion::EditorPrefabConversionHost,
    source_tools: source_tools::EditorSourceTools,
    schema_owner_adoption: schema_owner::EditorSchemaOwnerAdoptionHost,
    #[cfg(test)]
    next_close_poll_gate: Option<Box<dyn FnOnce(&mut Self) + Send>>,
    authoring_prefab_source_observations:
        BTreeMap<nara_asset::AssetPath, AuthoringPrefabSourceObservation>,
    #[cfg(test)]
    reject_next_prefab_source_binding: bool,
    persistence: persistence::PersistenceState,
    next_persistence_attempt: u64,
    #[cfg(test)]
    next_save_gate: Option<persistence::SaveTestGate>,
    #[cfg(test)]
    next_reopen_gate: Option<persistence::ReopenTestGate>,
    diagnostics: DiagnosticReport,
    field_feedback: Option<EditorFieldFeedback>,
    play_result: Option<EditorPlayOperationResult>,
    external_play_request: Option<EditorExternalPlayRequest>,
    external_play_attempt: Option<u64>,
    external_play_view: Option<EditorExternalPlayView>,
    external_play_bound_source_digest: Option<ContentDigest>,
    current_saved_source_digest: Option<ContentDigest>,
    next_external_play_attempt: u64,
    pending_workspace_intent: Option<PendingWorkspaceIntent>,
    workspace_intent_result: Option<EditorWorkspaceIntentResult>,
    frame_clock: EditorFrameClock,
    published_frame: Option<PublishedEditorFrame>,
    gestures: EditorGestureController,
    last_gesture_cancellation: Option<EditorGestureCancelReason>,
    authoring_runtime: authoring_runtime::EditorAuthoringRuntime,
    authoring_retirements: authoring_runtime::EditorAuthoringRetirements,
    #[cfg(test)]
    next_authoring_refresh_hook:
        Option<Box<dyn FnOnce(&authoring_runtime::EditorAuthoringRuntime)>>,
    #[cfg(feature = "runtime-ui")]
    authoring_images: authoring_images::EditorAuthoringImageHost,
    #[cfg(feature = "presentation-ui")]
    ui_preview: ui_preview::EditorUiPreviewSession,
    #[cfg(feature = "runtime-ui")]
    ui_authoring_frame: Option<Arc<EditorUiAuthoringFrame>>,
    #[cfg(feature = "runtime-ui")]
    ui_authoring_projection_cache: Option<EditorUiAuthoringProjectionCache>,
}

/// Evidence that authoring, trusted Play and pending file publication completed physical cleanup.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorProjectCloseReceipt {
    task_shutdown: TaskShutdownReport,
    authoring_cleanup_failed: bool,
    play_cleanup_failure: Option<super::ProjectRuntimeCloseObservation>,
}

impl EditorProjectCloseReceipt {
    fn new(
        task_shutdown: TaskShutdownReport,
        authoring_cleanup_failed: bool,
        play_cleanup_failure: Option<super::ProjectRuntimeCloseObservation>,
    ) -> Self {
        debug_assert!(task_shutdown.is_complete());
        Self {
            task_shutdown,
            authoring_cleanup_failed,
            play_cleanup_failure,
        }
    }

    #[must_use]
    pub const fn task_shutdown(&self) -> &TaskShutdownReport {
        &self.task_shutdown
    }

    /// Cleanup failure is independent of physical completion, including replaced authoring owners.
    #[must_use]
    pub const fn has_authoring_cleanup_failure(&self) -> bool {
        self.authoring_cleanup_failed
    }

    /// Sticky failure from a physically completed trusted Play owner, independent of completion.
    #[must_use]
    pub const fn has_play_cleanup_failure(&self) -> bool {
        self.play_cleanup_failure.is_some()
    }

    #[must_use]
    pub fn play_close_evidence(&self) -> Option<&nara_app::RuntimeCloseEvidence> {
        self.play_cleanup_failure
            .as_ref()
            .and_then(|failure| failure.close_evidence.as_ref())
    }

    #[must_use]
    pub fn has_cleanup_failure(&self) -> bool {
        self.authoring_cleanup_failed
            || self.has_play_cleanup_failure()
            || self.task_shutdown.has_cleanup_failure()
    }
}

/// One bounded attempt to retire every physical owner of an Editor project session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorProjectCloseProgress {
    /// Authoring, trusted Play or file cleanup remains incomplete and owned by this session.
    Incomplete {
        task_shutdown: TaskShutdownReport,
        file_cleanup_pending: bool,
        play_cleanup_pending: bool,
    },
    /// Every session-local owner completed physical cleanup; failure evidence remains observable.
    Complete(EditorProjectCloseReceipt),
}

impl EditorProjectCloseProgress {
    #[must_use]
    pub const fn task_shutdown(&self) -> &TaskShutdownReport {
        match self {
            Self::Incomplete { task_shutdown, .. } => task_shutdown,
            Self::Complete(receipt) => receipt.task_shutdown(),
        }
    }
}

impl fmt::Debug for EditorProjectSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EditorProjectSession")
            .field("workspace_documents", &self.workspace.len())
            .field("persistence", &self.persistence.view())
            .field("play_state", &self.play_view().state())
            .finish_non_exhaustive()
    }
}

impl Drop for EditorProjectSession {
    fn drop(&mut self) {
        self.schema_owner_adoption.cancel();
        if let Some(project) = self.authoring_project_mut() {
            project.cancel_source_index();
        }
        self.image_import.cancel_pending();
        self.source_asset_import.cancel();
        self.prefab_conversion.cancel();
        self.source_tools.cancel();
        self.cancel_pending_document_open();
    }
}

impl EditorProjectSession {
    /// Advances admitted persistence before cancelling session-local work and polling cleanup.
    ///
    /// The caller must retain this session and retry after `Incomplete`. Dropping an incomplete
    /// session transfers retained runtime ownership to process quarantine.
    /// Filesystem calls borrowed by persistence must return before completion can be observed;
    /// this poll does not claim to preempt an OS call or roll back published bytes.
    #[doc(hidden)]
    pub fn advance_close(&mut self) -> EditorProjectCloseProgress {
        self.closing = true;
        self.schema_owner_adoption.cancel();
        self.advance_schema_owner_adoption();
        self.source_tools.cancel();
        self.advance_source_tools();
        self.prefab_conversion.cancel();
        self.advance_prefab_conversion();
        #[cfg(test)]
        if let Some(gate) = self.next_close_poll_gate.take() {
            gate(self);
        }
        #[cfg(feature = "presentation-2d")]
        self.scene_capture.cancel();
        let play_closed = self.advance_session_play_close();
        self.advance_authoring_retirement();
        self.cancel_pending_document_open();
        if let Some(project) = self.authoring_project_mut() {
            project.cancel_source_index();
        }
        self.authoring_runtime.host_custody().close_admission();
        self.advance_persistence();
        if self.persistence.has_pending_work() {
            return EditorProjectCloseProgress::Incomplete {
                task_shutdown: self.session_task_shutdown(&TaskShutdownReport::default()),
                file_cleanup_pending: true,
                play_cleanup_pending: !play_closed,
            };
        }
        self.image_import.cancel_pending();
        self.source_asset_import.cancel();
        self.drain_schema_adoption_source_asset_import();
        let mut files_closed = self.image_import.retry_publication_cleanup();
        files_closed &= self.authoring_runtime.host_custody().retry_cleanup();
        match self.authoring_runtime.shutdown_tasks() {
            authoring_runtime::EditorAuthoringRuntimeCloseProgress::Incomplete(task_shutdown) => {
                EditorProjectCloseProgress::Incomplete {
                    task_shutdown: self.session_task_shutdown(&task_shutdown),
                    file_cleanup_pending: !files_closed,
                    play_cleanup_pending: !play_closed,
                }
            }
            authoring_runtime::EditorAuthoringRuntimeCloseProgress::Complete(task_shutdown) => {
                self.advance_schema_owner_adoption();
                self.drain_schema_adoption_source_asset_import();
                self.advance_prefab_conversion();
                self.advance_source_tools();
                files_closed &= !self.schema_owner_adoption.pending();
                files_closed &= !self.source_asset_import.schema_adoption_pending();
                files_closed &= self.authoring_runtime.host_custody().retry_cleanup();
                let task_shutdown = self.session_task_shutdown(&task_shutdown);
                if files_closed && !self.authoring_retirements.is_pending() && play_closed {
                    EditorProjectCloseProgress::Complete(EditorProjectCloseReceipt::new(
                        task_shutdown,
                        self.authoring_retirements.failed()
                            || self.authoring_runtime.has_close_failure(),
                        self.trusted_compiled()
                            .and_then(|lane| lane.close_summary.failure().cloned()),
                    ))
                } else {
                    EditorProjectCloseProgress::Incomplete {
                        task_shutdown,
                        file_cleanup_pending: !files_closed,
                        play_cleanup_pending: !play_closed,
                    }
                }
            }
        }
    }

    fn session_task_shutdown(&self, live_authoring: &TaskShutdownReport) -> TaskShutdownReport {
        let authoring = self.authoring_retirements.report_with(live_authoring);
        let Some(lane) = self.trusted_compiled() else {
            return authoring;
        };
        let current = lane.runtime_host.current_task_shutdown();
        let play = lane.close_summary.report_with(current.as_ref());
        TaskShutdownReport::aggregate([&authoring, &play])
    }

    fn accepts_new_work(&self) -> bool {
        !self.closing && self.authoring_runtime.host_custody().is_accepting()
    }

    #[must_use]
    pub fn project_readiness(&self) -> EditorProjectReadinessView {
        match &self.execution {
            EditorExecutionLane::Authoring(snapshot) => EditorProjectReadinessView::new(
                match snapshot.readiness() {
                    AuthoringProjectReadiness::Complete => EditorAuthoringReadiness::Complete,
                    AuthoringProjectReadiness::Degraded => EditorAuthoringReadiness::Degraded,
                },
                external_play::authoring_runtime_availability(snapshot),
            ),
            EditorExecutionLane::TrustedCompiled(_) => EditorProjectReadinessView::new(
                EditorAuthoringReadiness::Complete,
                EditorRuntimeAvailability::TrustedInProcess,
            ),
        }
    }

    /// Builds the capability-free source receipt consumed by the private Editor Play owner.
    #[doc(hidden)]
    pub fn supervised_desktop_source_receipt(
        &self,
    ) -> Result<Vec<u8>, EditorExternalPlayReceiptError> {
        self.authoring_project()
            .ok_or(EditorExternalPlayReceiptError::NotAuthoringProject)
            .and_then(external_play::build_supervised_desktop_source_receipt)
    }

    fn authoring_project(&self) -> Option<&AuthoringProjectSessionSnapshot> {
        match &self.execution {
            EditorExecutionLane::Authoring(snapshot) => Some(snapshot),
            EditorExecutionLane::TrustedCompiled(_) => None,
        }
    }

    fn authoring_project_mut(&mut self) -> Option<&mut AuthoringProjectSessionSnapshot> {
        match &mut self.execution {
            EditorExecutionLane::Authoring(snapshot) => Some(snapshot),
            EditorExecutionLane::TrustedCompiled(_) => None,
        }
    }

    #[must_use]
    pub(crate) fn project_source_index_kind(&self) -> ProjectSourceIndexStateKind {
        self.authoring_project().map_or(
            ProjectSourceIndexStateKind::NotRequested,
            AuthoringProjectSessionSnapshot::source_index_kind,
        )
    }

    #[must_use]
    pub(crate) fn project_source_index_snapshot(&self) -> Option<Arc<ProjectSourceIndexSnapshot>> {
        self.authoring_project()
            .and_then(AuthoringProjectSessionSnapshot::source_index_snapshot)
    }

    #[must_use]
    pub(crate) fn project_source_index_failure(&self) -> Option<&ProjectSourceIndexFailure> {
        self.authoring_project()
            .and_then(AuthoringProjectSessionSnapshot::source_index_failure)
    }

    #[must_use]
    pub(crate) fn project_source_index_work_pending(&self) -> bool {
        self.authoring_project()
            .is_some_and(AuthoringProjectSessionSnapshot::source_index_has_pending_work)
    }

    pub(crate) fn project_source_index_progress(
        &self,
    ) -> Option<crate::project_host::authoring::ProjectSourceIndexProgress> {
        self.authoring_project()
            .and_then(AuthoringProjectSessionSnapshot::source_index_progress)
    }

    pub(crate) fn request_project_source_index_refresh(&mut self) -> bool {
        if !self.accepts_new_work() {
            return false;
        }
        self.authoring_project_mut()
            .and_then(AuthoringProjectSessionSnapshot::request_source_index_refresh)
            .is_some()
    }

    #[cfg(test)]
    pub(crate) fn hold_next_source_index_scan(
        &mut self,
        phase: crate::project_host::authoring::ScanTestPhase,
    ) -> crate::project_host::authoring::ScanTestControl {
        self.authoring_project_mut()
            .expect("source-index gate requires authoring project")
            .hold_next_source_index_scan(phase)
    }

    pub(crate) fn editor_image_import_work_pending(&self) -> bool {
        self.image_import.has_pending_work()
    }

    #[cfg(test)]
    pub(crate) fn editor_image_import_state(&self) -> image_import::EditorImageImportState {
        self.image_import.state()
    }

    #[cfg(test)]
    pub(crate) fn editor_image_last_good(
        &self,
        asset_path: &nara_asset::AssetPath,
    ) -> Option<&image_import::EditorImageLastGood> {
        self.image_import.last_good(asset_path)
    }

    #[cfg(all(test, feature = "runtime-ui"))]
    pub(crate) fn editor_authoring_image_catalog_facts(
        &self,
    ) -> (u64, Vec<(String, ImageImportSettings)>) {
        let receipt = self.authoring_images.receipt();
        let products = receipt
            .products()
            .iter()
            .map(|product| (product.path().as_str().to_owned(), product.settings()))
            .collect();
        (receipt.generation().get(), products)
    }

    #[cfg(test)]
    pub(crate) fn request_editor_image_import(
        &mut self,
        frame_revision: EditorFrameRevision,
        project_relative_path: &str,
        settings: ImageImportSettings,
    ) -> image_import::EditorImageImportRequestResult {
        if !self.accepts_new_work() {
            return image_import::EditorImageImportRequestResult::Rejected(
                image_import::EditorImageImportFailure::Busy,
            );
        }
        if let Some(failure) = self.image_import.request_blocker() {
            return image_import::EditorImageImportRequestResult::Rejected(failure);
        }
        let source_index = match self.published_frame.as_ref() {
            Some(frame) if frame.model.revision() == frame_revision => {
                frame.source_index.as_ref().map(Arc::clone)
            }
            _ => {
                return image_import::EditorImageImportRequestResult::Rejected(
                    image_import::EditorImageImportFailure::StaleFrame,
                );
            }
        };
        let Some(image_frame) = source_index.as_ref().and_then(|source_index| {
            self.image_import
                .frame_for(source_index, project_relative_path)
        }) else {
            return image_import::EditorImageImportRequestResult::Rejected(
                image_import::EditorImageImportFailure::CatalogUnavailable,
            );
        };
        let expected_detail_revision = image_frame.detail_revision();
        self.submit_editor_image_import(
            image_frame,
            project_relative_path,
            expected_detail_revision,
            settings,
        )
    }

    fn submit_editor_image_import(
        &mut self,
        image_frame: image_import::EditorImageImportFrame,
        project_relative_path: &str,
        expected_detail_revision: u64,
        settings: ImageImportSettings,
    ) -> image_import::EditorImageImportRequestResult {
        if !self.accepts_new_work() {
            return image_import::EditorImageImportRequestResult::Rejected(
                image_import::EditorImageImportFailure::Busy,
            );
        }
        let result = {
            let EditorProjectSession {
                execution,
                authoring_runtime,
                image_import,
                ..
            } = self;
            let EditorExecutionLane::Authoring(project) = execution else {
                return image_import::EditorImageImportRequestResult::Rejected(
                    image_import::EditorImageImportFailure::CatalogUnavailable,
                );
            };
            image_import.request(
                project,
                authoring_runtime,
                image_frame,
                project_relative_path,
                expected_detail_revision,
                settings,
            )
        };
        if !matches!(
            result,
            image_import::EditorImageImportRequestResult::Rejected(_)
        ) {
            self.refresh_current_saved_source_digest();
        }
        result
    }

    fn advance_editor_image_import(&mut self) {
        #[cfg(feature = "presentation-2d")]
        let images_before = self.authoring_runtime.image_publication_revision();
        let revocation_failure = {
            let EditorProjectSession {
                execution,
                authoring_runtime,
                image_import,
                ..
            } = self;
            if let EditorExecutionLane::Authoring(project) = execution {
                image_import.advance(project, authoring_runtime)
            } else {
                None
            }
        };
        #[cfg(feature = "presentation-2d")]
        if self.authoring_runtime.image_publication_revision() != images_before {
            self.scene_capture.invalidate();
            self.authoring_runtime.invalidate_preview_content();
        }
        if revocation_failure.is_some() {
            let _ = self.diagnostics.extend(single_warning(
                "project.editor.authoring-image-revocation-failed",
                "authoring image revocation failed; reopen the project before importing images or generating Scene previews",
            ));
        }

        #[cfg(feature = "runtime-ui")]
        self.publish_authoring_image_successor_if_changed();

        if let Some(failure) = self.image_import.take_hydration_failure() {
            let code = match failure.kind() {
                image_import::EditorImageHydrationFailureKind::ClosureInvalid => {
                    "project.editor.authoring-image-closure-invalid"
                }
                image_import::EditorImageHydrationFailureKind::CapacityExceeded => {
                    "project.editor.authoring-image-capacity-exceeded"
                }
                image_import::EditorImageHydrationFailureKind::SourceUnavailable => {
                    "project.editor.authoring-image-source-unavailable"
                }
                image_import::EditorImageHydrationFailureKind::SourceChanged => {
                    "project.editor.authoring-image-source-changed"
                }
                image_import::EditorImageHydrationFailureKind::MetadataUnavailable => {
                    "project.editor.authoring-image-metadata-unavailable"
                }
                image_import::EditorImageHydrationFailureKind::MetadataChanged => {
                    "project.editor.authoring-image-metadata-changed"
                }
                image_import::EditorImageHydrationFailureKind::Runtime(_) => {
                    "project.editor.authoring-image-runtime-rejected"
                }
                image_import::EditorImageHydrationFailureKind::ResultMismatch => {
                    "project.editor.authoring-image-result-mismatch"
                }
            };
            let _ = self
                .diagnostics
                .extend(single_warning_with_project_relative(
                    code,
                    "a referenced authoring image could not be restored; the last complete image catalog remains active",
                    "asset",
                    failure.asset_path().as_str(),
                ));
        }
    }

    #[cfg(feature = "runtime-ui")]
    fn publish_authoring_image_successor_if_changed(&mut self) {
        if !self.image_import.last_good_store_changed() {
            return;
        }
        let selected = self.image_import.authoring_image_asset_ids();
        let successor = match self
            .authoring_images
            .prepare_selected_successor(&self.authoring_runtime, &selected)
        {
            Ok(Some(successor)) => successor,
            Ok(None) => {
                self.image_import.acknowledge_last_good_store_changed();
                return;
            }
            Err(_) => {
                let _ = self.diagnostics.extend(single_warning(
                    "project.editor.authoring-image-catalog-rejected",
                    "the updated authoring image facts were rejected; the last complete image catalog remains active",
                ));
                return;
            }
        };

        if self.authoring_images.publish_successor(successor).is_err() {
            let _ = self.diagnostics.extend(single_warning(
                "project.editor.authoring-image-catalog-rejected",
                "the updated authoring image catalog became stale before publication",
            ));
            return;
        }
        self.image_import.acknowledge_last_good_store_changed();
        self.ui_authoring_frame = None;
        #[cfg(feature = "presentation-ui")]
        self.ui_preview.retire();
    }

    /// Returns the complete unavailable-record manifest for the active generic authoring
    /// document. Trusted compiled sessions do not publish this preservation contract.
    #[doc(hidden)]
    #[must_use]
    pub fn active_authoring_unavailable_record_manifest(
        &self,
    ) -> Option<&UnavailableRecordManifest> {
        self.authoring_project()?;
        let document = self.workspace.active_document()?;
        self.workspace
            .scene(document)
            .and_then(|slot| slot.unavailable_record_manifest())
            .or_else(|| {
                self.workspace
                    .prefab(document)
                    .and_then(|slot| slot.unavailable_record_manifest())
            })
    }

    fn trusted_compiled(&self) -> Option<&TrustedCompiledEditorLane> {
        match &self.execution {
            EditorExecutionLane::Authoring(_) => None,
            EditorExecutionLane::TrustedCompiled(lane) => Some(lane),
        }
    }

    fn trusted_compiled_mut(&mut self) -> Option<&mut TrustedCompiledEditorLane> {
        match &mut self.execution {
            EditorExecutionLane::Authoring(_) => None,
            EditorExecutionLane::TrustedCompiled(lane) => Some(lane),
        }
    }

    fn take_trusted_runtime_owner(&mut self) -> Option<EditorRuntimeOwnerState> {
        let lane = self.trusted_compiled_mut()?;
        Some(lane.runtime_owner.take())
    }

    fn install_trusted_runtime_owner(&mut self, state: EditorRuntimeOwnerState) -> bool {
        let Some(lane) = self.trusted_compiled_mut() else {
            return false;
        };
        lane.runtime_owner = state;
        true
    }

    fn open(
        receipt: AuthoringProjectReceipt,
        #[cfg(test)] publication_hook: Option<
            Box<dyn FnOnce(&authoring_runtime::EditorAuthoringRuntime)>,
        >,
    ) -> Result<Self, EditorProjectOpenError> {
        let snapshot = receipt.into_session_snapshot().map_err(|_| {
            editor_open_error("project.editor.authoring-receipt-consumption-failed")
        })?;
        let preview_registry = first_party_authoring_preview_registry(built_in_schema_providers())
            .map_err(|_| editor_open_error("project.editor.authoring-preview-registry-invalid"))?;
        let authoring_runtime =
            authoring_runtime::EditorAuthoringRuntime::new_authoring(&snapshot, &preview_registry)
                .map_err(EditorProjectOpenError::from)?;
        #[cfg(test)]
        if let Some(hook) = publication_hook {
            hook(&authoring_runtime);
        }
        let mut unpublished = Some(authoring_runtime);
        let publication: Result<Self, EditorProjectOpenError> = (|| {
            let authoring_runtime = unpublished
                .as_ref()
                .expect("the unpublished authoring owner is retained");
            let mut image_import = image_import::EditorImageImportHost::default();
            image_import.initialize_authoring_hydration(&snapshot);
            #[cfg(feature = "runtime-ui")]
            let authoring_images = authoring_images::EditorAuthoringImageHost::capture_selected(
                &authoring_runtime,
                &image_import.authoring_image_asset_ids(),
            )
            .map_err(|_| editor_open_error("project.editor.authoring-image-init-failed"))?;
            #[cfg(feature = "presentation-ui")]
            let ui_preview = ui_preview::EditorUiPreviewSession::new().map_err(|_error| {
                editor_open_error("project.editor.authoring-ui-preview-init-failed")
            })?;
            let project_root = snapshot.project_root();
            let settings = snapshot.settings();
            let scenes_path = RelativePath::new(settings.paths.scenes.as_str())
                .map_err(|_| editor_open_error("project.editor.scenes-path-invalid"))?;
            let scenes = project_root
                .open_directory(&scenes_path)
                .map_err(|_| editor_open_error("project.editor.scenes-open-failed"))?;
            let startup = settings
                .startup
                .default_scene
                .as_ref()
                .ok_or_else(|| editor_open_error("project.editor.startup-scene-missing"))?;
            let startup_title = startup.as_str().to_owned();
            let startup_project_path: Arc<str> = Arc::from(format!(
                "{}/{}",
                settings.paths.scenes.as_str(),
                startup.as_str()
            ));
            let startup_source_key = snapshot.startup_native_source().key().clone();
            let crate::project_host::authoring::AdmittedAuthoringDocument::Scene {
                source: startup_authoring_source,
                file: startup_file,
                document: startup_scene,
            } = snapshot
                .admit_startup_document()
                .map_err(|error| match error {
                    AuthoringDocumentAdmissionError::SourceIdentityChanged => {
                        editor_persistence_open_error(EditorPersistenceFailureStage::Validate)
                    }
                    error => editor_open_error(error.code()),
                })?
            else {
                unreachable!("startup admission retains its Scene kind");
            };
            let (persistence, opened_digest) = ScenePersistenceHost::bind_authoring_source(
                &scenes,
                startup.as_str(),
                startup_file,
                startup_authoring_source.digest(),
            )
            .map_err(editor_persistence_open_error)?;
            let mut diagnostics = startup_scene.diagnostics().clone();
            diagnostics.extend(snapshot.startup_compilation_diagnostics().clone());
            let (mut workspace, mut persistence_authority) = EditorWorkspace::new_hosted();
            let report = workspace
                .open_authoring_scene_document(
                    startup_title.clone(),
                    nara_tooling::EditorDocumentSourceBinding::new(startup_title.clone()),
                    startup_scene,
                )
                .map_err(|_| editor_open_error("project.editor.scene-publication-failed"))?;
            let document = report
                .opened_document
                .ok_or_else(|| editor_open_error("project.editor.scene-publication-failed"))?;
            let binding = persistence_authority.bind_opened_source_digest(
                &mut workspace,
                document,
                opened_digest,
            );
            if !binding.applied {
                return Err(editor_open_error(
                    "project.editor.persistence-authority-invalid",
                ));
            }

            let mut session = Self {
                closing: false,
                workspace,
                persistence_authority,
                scene_lanes: BTreeMap::from([(
                    document,
                    EditorScenePersistenceLane {
                        project_path: startup_project_path,
                        source_key: startup_source_key,
                        authoring_source: Some(startup_authoring_source),
                        document,
                        host: authoring_runtime
                            .host_custody()
                            .retain_scene(persistence)
                            .ok_or_else(|| {
                                editor_open_error("project.editor.persistence-owner-closed")
                            })?,
                    },
                )]),
                scene_preview_document: Some(document),
                #[cfg(feature = "presentation-2d")]
                scene_capture: Default::default(),
                execution: EditorExecutionLane::Authoring(snapshot),
                project_content: EditorProjectContentView::NotInstalled,
                next_project_content_generation: 1,
                prefab_content_refresh: BTreeMap::new(),
                prefab_lanes: BTreeMap::new(),
                extension_documents: extension_documents::EditorExtensionDocumentsHost::default(),
                pending_document_open: None,
                next_document_open_request: 0,
                document_open_result: None,
                #[cfg(test)]
                next_document_open_gate: None,
                project_browser: project_browser::EditorProjectBrowserHost::new(),
                image_import,
                source_asset_import: source_asset_import::EditorSourceAssetImportHost::default(),
                prefab_conversion: Default::default(),
                source_tools: Default::default(),
                schema_owner_adoption: Default::default(),
                #[cfg(test)]
                next_close_poll_gate: None,
                authoring_prefab_source_observations: BTreeMap::new(),
                #[cfg(test)]
                reject_next_prefab_source_binding: false,
                persistence: persistence::PersistenceState::Idle,
                next_persistence_attempt: 0,
                #[cfg(test)]
                next_save_gate: None,
                #[cfg(test)]
                next_reopen_gate: None,
                diagnostics,
                field_feedback: None,
                play_result: None,
                external_play_request: None,
                external_play_attempt: None,
                external_play_view: None,
                external_play_bound_source_digest: None,
                current_saved_source_digest: None,
                next_external_play_attempt: 1,
                pending_workspace_intent: None,
                workspace_intent_result: None,
                frame_clock: EditorFrameClock::new(),
                published_frame: None,
                gestures: EditorGestureController::default(),
                last_gesture_cancellation: None,
                authoring_runtime: unpublished
                    .take()
                    .expect("publication transfers the authoring owner once"),
                authoring_retirements: Default::default(),
                #[cfg(test)]
                next_authoring_refresh_hook: None,
                #[cfg(feature = "runtime-ui")]
                authoring_images,
                #[cfg(feature = "presentation-ui")]
                ui_preview,
                #[cfg(feature = "runtime-ui")]
                ui_authoring_frame: None,
                #[cfg(feature = "runtime-ui")]
                ui_authoring_projection_cache: None,
            };
            #[cfg(feature = "presentation-2d")]
            session.initialize_authoring_scene_capture();
            session.initialize_source_tool_selection();
            session.publish_editor_frame(false);
            Ok(session)
        })();
        publication.map_err(|error| {
            error.with_runtime(
                unpublished
                    .take()
                    .expect("a rejected publication retains the authoring owner"),
            )
        })
    }

    /// Opens the explicitly advanced in-process Editor lane compiled with project Runtime code.
    #[doc(hidden)]
    pub fn open_trusted_compiled(
        project_root: DirectoryCapability,
        intent: TrustedCompiledEditorProjectIntent,
    ) -> Result<Self, EditorProjectOpenError> {
        Self::open_trusted_compiled_impl(project_root, intent, None)
    }

    pub(crate) fn open_trusted_compiled_with_overlay(
        project_root: DirectoryCapability,
        intent: TrustedCompiledEditorProjectIntent,
        overlay: RuntimePluginOverlay,
    ) -> Result<Self, EditorProjectOpenError> {
        Self::open_trusted_compiled_impl(project_root, intent, Some(overlay))
    }

    fn open_trusted_compiled_impl(
        project_root: DirectoryCapability,
        intent: TrustedCompiledEditorProjectIntent,
        overlay: Option<RuntimePluginOverlay>,
    ) -> Result<Self, EditorProjectOpenError> {
        let TrustedCompiledEditorProjectIntent {
            profile,
            cleanup_policy,
            recipe,
        } = intent;
        let manifest_path = RelativePath::new(PROJECT_MANIFEST)
            .expect("the engine-owned manifest name is a valid relative path");
        let manifest = project_root
            .open_file(&manifest_path)
            .map_err(|_| editor_open_error("project.editor.manifest-open-failed"))?;
        let candidate = ingest_project_manifest(&manifest, profile.as_deref())
            .map_err(|error| EditorProjectOpenError::new(error.diagnostics().clone()))?;
        drop(manifest);

        let host_policy = runtime_host_policy_contribution(editor_runtime_bridge_definition());
        #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
        let (game_view_config, host_policy) = match game_view::editor_game_view_bridge_definition(
            candidate.normalized_capabilities(),
            candidate.settings().window.to_window(),
        ) {
            Some((config, definition)) => (Some(config), host_policy.with_definition(definition)),
            None => (None, host_policy),
        };
        #[cfg(not(any(feature = "presentation-2d", feature = "presentation-ui")))]
        let host_policy = host_policy;
        let request =
            editor_play_runtime_plugins_with_recipe(&candidate, recipe, overlay, Some(host_policy));
        let plan = resolve_project_runtime_request(&candidate, request)
            .map_err(|error| EditorProjectOpenError::new(runtime_plan_failure_report(&error)))?;
        let runtime_agreement = prepare_runtime_schema_agreement(&project_root, &candidate, &plan)
            .map_err(|error| EditorProjectOpenError::new(error.diagnostics().clone()))?;

        let prefabs_path = RelativePath::new(candidate.settings().paths.prefabs.as_str())
            .map_err(|_| editor_open_error("project.editor.prefabs-path-invalid"))?;
        let prefab_root = project_root
            .open_directory(&prefabs_path)
            .map_err(|_| editor_open_error("project.editor.prefabs-open-failed"))?;

        let loader = ProjectContentLoader::new(project_root.share())
            .map_err(|error| EditorProjectOpenError::new(error.diagnostics().clone()))?;
        let snapshot = loader
            .load_with_runtime_agreement(&candidate, &plan, runtime_agreement)
            .map_err(|error| EditorProjectOpenError::new(error.diagnostics().clone()))?;
        let authoring_runtime = authoring_runtime::EditorAuthoringRuntime::new(
            snapshot.clone(),
            plan.schema_validation().snapshot(),
        )
        .map_err(EditorProjectOpenError::from)?;
        let mut unpublished = Some(authoring_runtime);
        let publication: Result<Self, EditorProjectOpenError> = (|| {
            let authoring_runtime = unpublished
                .as_ref()
                .expect("the unpublished authoring owner is retained");
            let initial_content_generation = EditorProjectContentGeneration::new(1);
            #[cfg(feature = "runtime-ui")]
            let authoring_images = authoring_images::EditorAuthoringImageHost::capture(
                &authoring_runtime,
            )
            .map_err(|_| {
                EditorProjectOpenError::new(single_error(
                    "project.editor.authoring-image-init-failed",
                    "the Editor authoring image catalog could not be initialized",
                ))
            })?;
            #[cfg(feature = "presentation-ui")]
            let ui_preview = ui_preview::EditorUiPreviewSession::new().map_err(|_error| {
                EditorProjectOpenError::new(single_error(
                    "project.editor.ui-preview-init-failed",
                    "the Editor UI preview owner could not be initialized",
                ))
            })?;
            let scenes_path = RelativePath::new(candidate.settings().paths.scenes.as_str())
                .map_err(|_| editor_open_error("project.editor.scenes-path-invalid"))?;
            let scenes = project_root
                .open_directory(&scenes_path)
                .map_err(|_| editor_open_error("project.editor.scenes-open-failed"))?;
            let startup = candidate
                .settings()
                .startup
                .default_scene
                .as_ref()
                .ok_or_else(|| editor_open_error("project.editor.startup-scene-missing"))?;
            let startup_project_path: Arc<str> = Arc::from(format!(
                "{}/{}",
                candidate.settings().paths.scenes.as_str(),
                startup.as_str()
            ));
            let opened = ScenePersistenceHost::open(
                &scenes,
                startup.as_str(),
                plan.schema_validation().registry(),
            )
            .map_err(editor_persistence_open_error)?;

            let (mut workspace, mut persistence_authority) = EditorWorkspace::new_hosted();
            let report = workspace
                .open_scene_session(startup.as_str(), opened.session)
                .map_err(|error| EditorProjectOpenError::new(error.report().diagnostics.clone()))?;
            let document = report
                .opened_document
                .expect("opening one editor scene publishes one document identity");
            let binding = persistence_authority.bind_opened_source_digest(
                &mut workspace,
                document,
                opened.digest,
            );
            if !binding.applied {
                return Err(editor_open_error(
                    "project.editor.persistence-authority-invalid",
                ));
            }

            let mut diagnostics = runtime_plan_selected_report(&plan);
            let _ = diagnostics.extend(report.diagnostics);
            let mut session = Self {
                closing: false,
                workspace,
                persistence_authority,
                scene_lanes: BTreeMap::from([(
                    document,
                    EditorScenePersistenceLane {
                        project_path: startup_project_path,
                        source_key: snapshot.selected_scene_source_key().clone(),
                        authoring_source: None,
                        document,
                        host: authoring_runtime
                            .host_custody()
                            .retain_scene(opened.host)
                            .ok_or_else(|| {
                                editor_open_error("project.editor.persistence-owner-closed")
                            })?,
                    },
                )]),
                scene_preview_document: Some(document),
                #[cfg(feature = "presentation-2d")]
                scene_capture: Default::default(),
                execution: EditorExecutionLane::TrustedCompiled(Box::new(
                    TrustedCompiledEditorLane {
                        plan,
                        content_candidate: candidate,
                        content_loader: Arc::new(loader),
                        snapshot,
                        prefab_root,
                        #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                        game_view_config,
                        runtime_host: ProjectHost::new(cleanup_policy),
                        close_summary: trusted_play::EditorPlayCloseSummary::default(),
                        runtime_owner: EditorRuntimeOwnerState::Empty,
                        pending_control: None,
                        reported_close_failure_generation: None,
                        #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                        pending_step_presentation: None,
                        runtime_edit_result: None,
                        #[cfg(test)]
                        fault_after_next_runtime_edit: false,
                        pending_apply_changes: None,
                        apply_changes_result: None,
                    },
                )),
                project_content: EditorProjectContentView::Current {
                    generation: initial_content_generation,
                },
                next_project_content_generation: 2,
                prefab_content_refresh: BTreeMap::new(),
                prefab_lanes: BTreeMap::new(),
                extension_documents: extension_documents::EditorExtensionDocumentsHost::default(),
                pending_document_open: None,
                next_document_open_request: 0,
                document_open_result: None,
                #[cfg(test)]
                next_document_open_gate: None,
                project_browser: project_browser::EditorProjectBrowserHost::new(),
                image_import: image_import::EditorImageImportHost::default(),
                source_asset_import: source_asset_import::EditorSourceAssetImportHost::default(),
                prefab_conversion: Default::default(),
                source_tools: Default::default(),
                schema_owner_adoption: Default::default(),
                #[cfg(test)]
                next_close_poll_gate: None,
                authoring_prefab_source_observations: BTreeMap::new(),
                #[cfg(test)]
                reject_next_prefab_source_binding: false,
                persistence: persistence::PersistenceState::Idle,
                next_persistence_attempt: 0,
                #[cfg(test)]
                next_save_gate: None,
                #[cfg(test)]
                next_reopen_gate: None,
                diagnostics,
                field_feedback: None,
                play_result: None,
                external_play_request: None,
                external_play_attempt: None,
                external_play_view: None,
                external_play_bound_source_digest: None,
                current_saved_source_digest: None,
                next_external_play_attempt: 1,
                pending_workspace_intent: None,
                workspace_intent_result: None,
                frame_clock: EditorFrameClock::new(),
                published_frame: None,
                gestures: EditorGestureController::default(),
                last_gesture_cancellation: None,
                authoring_runtime: unpublished
                    .take()
                    .expect("publication transfers the authoring owner once"),
                authoring_retirements: Default::default(),
                #[cfg(test)]
                next_authoring_refresh_hook: None,
                #[cfg(feature = "runtime-ui")]
                authoring_images,
                #[cfg(feature = "presentation-ui")]
                ui_preview,
                #[cfg(feature = "runtime-ui")]
                ui_authoring_frame: None,
                #[cfg(feature = "runtime-ui")]
                ui_authoring_projection_cache: None,
            };
            session.initialize_source_tool_selection();
            session.publish_editor_frame(false);
            Ok(session)
        })();
        publication.map_err(|error| {
            error.with_runtime(
                unpublished
                    .take()
                    .expect("a rejected publication retains the authoring owner"),
            )
        })
    }

    #[cfg(feature = "presentation-2d")]
    #[doc(hidden)]
    pub fn configure_editor_preview(&mut self, extent: Option<Extent2d>) {
        self.set_editor_preview_extent(extent);
        self.drive_editor_preview();
    }

    /// Changes the extent used by the next ordinary Editor drive without publishing an extra frame.
    #[cfg(feature = "presentation-2d")]
    #[doc(hidden)]
    pub fn set_editor_preview_extent(&mut self, extent: Option<Extent2d>) {
        self.authoring_runtime.set_preview_extent(extent);
    }

    #[cfg(feature = "presentation-2d")]
    #[doc(hidden)]
    #[must_use]
    pub fn take_editor_preview_candidate(&mut self) -> Option<EditorPreviewCandidate> {
        if self.current_editor_preview_content_stamp().is_none() {
            self.authoring_runtime.invalidate_preview_content();
            return None;
        }
        self.authoring_runtime.take_preview_candidate()
    }

    #[cfg(feature = "presentation-ui")]
    #[doc(hidden)]
    #[must_use]
    pub fn take_editor_ui_preview_candidate(&mut self) -> Option<EditorUiPreviewCandidate> {
        self.ui_preview.take_candidate()
    }

    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    #[doc(hidden)]
    #[must_use]
    pub fn pending_game_start_request(&self) -> Option<EditorGameStartRequest> {
        match self.trusted_compiled().map(|lane| &lane.runtime_owner) {
            Some(EditorRuntimeOwnerState::AwaitingGameTarget { request, .. }) => Some(*request),
            _ => None,
        }
    }

    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    #[doc(hidden)]
    #[must_use]
    pub fn active_game_start_request(&self) -> Option<EditorGameStartRequest> {
        match self.trusted_compiled().map(|lane| &lane.runtime_owner) {
            Some(EditorRuntimeOwnerState::Active {
                game: Some(game), ..
            }) => Some(game.start),
            _ => None,
        }
    }

    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    #[doc(hidden)]
    pub fn prepare_game_start_activation(
        &mut self,
        expected: EditorGameStartRequest,
    ) -> Result<EditorGameStartActivation, EditorGameViewBridgeFailure> {
        if self.closing {
            return Err(EditorGameViewBridgeFailure::RuntimeUnavailable);
        }
        let EditorExecutionLane::TrustedCompiled(lane) = &mut self.execution else {
            return Err(EditorGameViewBridgeFailure::RuntimeUnavailable);
        };
        let state = lane.runtime_owner.take();
        match state {
            EditorRuntimeOwnerState::AwaitingGameTarget {
                source,
                operation,
                request,
                prepared,
            } if request == expected => {
                lane.runtime_owner = EditorRuntimeOwnerState::CommittingGameTarget {
                    source,
                    operation,
                    request,
                    prepared,
                };
                Ok(EditorGameStartActivation::new(request))
            }
            other @ EditorRuntimeOwnerState::AwaitingGameTarget { .. } => {
                lane.runtime_owner = other;
                Err(EditorGameViewBridgeFailure::GameStartRequestMismatch)
            }
            other => {
                lane.runtime_owner = other;
                Err(EditorGameViewBridgeFailure::GameStartNotPending)
            }
        }
    }

    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    #[doc(hidden)]
    pub fn commit_game_start_activation(&mut self, activation: EditorGameStartActivation) {
        let EditorExecutionLane::TrustedCompiled(lane) = &mut self.execution else {
            return;
        };
        let expected = activation.into_request();
        let state = lane.runtime_owner.take();
        let EditorRuntimeOwnerState::CommittingGameTarget {
            source,
            operation,
            request,
            prepared,
        } = state
        else {
            lane.runtime_owner = state;
            panic!("a Game start activation commits only its prepared Runtime");
        };
        assert_eq!(
            request, expected,
            "a Game start activation cannot commit a foreign Runtime reservation"
        );
        let diagnostics = lane.runtime_host.commit_prepared_start(prepared);
        let _ = self.diagnostics.extend(diagnostics);
        let generation = lane.runtime_host.running_runtime_generation();
        self.play_result = Some(EditorPlayOperationResult::Applied {
            operation,
            generation,
        });
        lane.pending_step_presentation = None;
        lane.runtime_owner = EditorRuntimeOwnerState::Active {
            source,
            game: Some(ActiveEditorGameView::new(request)),
        };
    }

    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    #[doc(hidden)]
    pub fn abort_pending_game_start(
        &mut self,
        expected: EditorGameStartRequest,
    ) -> Result<(), EditorGameViewBridgeFailure> {
        let EditorExecutionLane::TrustedCompiled(lane) = &mut self.execution else {
            return Err(EditorGameViewBridgeFailure::RuntimeUnavailable);
        };
        let state = lane.runtime_owner.take();
        match state {
            EditorRuntimeOwnerState::AwaitingGameTarget {
                source,
                operation,
                request,
                prepared,
            } if request == expected => {
                lane.runtime_host.abort_prepared_start(prepared);
                lane.pending_step_presentation = None;
                self.play_result = Some(EditorPlayOperationResult::Pending { operation });
                lane.runtime_owner = EditorRuntimeOwnerState::RetiringPlay {
                    source,
                    terminal: EditorPlayOperationResult::Failed {
                        operation,
                        failure: EditorPlayFailure::Start,
                    },
                };
                Ok(())
            }
            other @ EditorRuntimeOwnerState::AwaitingGameTarget { .. } => {
                lane.runtime_owner = other;
                Err(EditorGameViewBridgeFailure::GameStartRequestMismatch)
            }
            other => {
                lane.runtime_owner = other;
                Err(EditorGameViewBridgeFailure::GameStartNotPending)
            }
        }
    }

    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    #[doc(hidden)]
    pub fn take_game_view_candidate(
        &mut self,
    ) -> Result<Option<EditorGameViewCandidate>, EditorGameViewBridgeFailure> {
        if self.closing {
            return Err(EditorGameViewBridgeFailure::RuntimeUnavailable);
        }
        self.take_existing_game_view_candidate()
    }

    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    fn take_existing_game_view_candidate(
        &mut self,
    ) -> Result<Option<EditorGameViewCandidate>, EditorGameViewBridgeFailure> {
        let EditorExecutionLane::TrustedCompiled(lane) = &mut self.execution else {
            return Err(EditorGameViewBridgeFailure::RuntimeUnavailable);
        };
        let start = match &lane.runtime_owner {
            EditorRuntimeOwnerState::Active {
                game: Some(game), ..
            } if game.in_flight.is_none() => game.start,
            EditorRuntimeOwnerState::Active { game: Some(_), .. } => {
                return Err(EditorGameViewBridgeFailure::CandidateBusy);
            }
            _ => return Err(EditorGameViewBridgeFailure::RuntimeUnavailable),
        };
        let runtime = lane
            .runtime_host
            .running_runtime_mut()
            .ok_or(EditorGameViewBridgeFailure::RuntimeUnavailable)?;
        let Some(candidate) = game_view::take_candidate(runtime)? else {
            return Ok(None);
        };
        let candidate = candidate.bind_start(start);
        let identity = candidate.identity();
        {
            let EditorRuntimeOwnerState::Active {
                game: Some(game), ..
            } = &mut lane.runtime_owner
            else {
                unreachable!("a captured Game candidate retains its active Runtime owner")
            };
            game.in_flight = Some(identity);
        }
        if let Some(EditorStepPresentationState::AwaitingCandidate { generation }) =
            lane.pending_step_presentation
        {
            if generation == identity.runtime_generation().get() {
                lane.pending_step_presentation =
                    Some(EditorStepPresentationState::AwaitingPresentation { identity });
            }
        }
        Ok(Some(candidate))
    }

    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    #[doc(hidden)]
    pub fn complete_game_view_candidate(
        &mut self,
        completion: EditorGameViewCompletion,
    ) -> Result<(), EditorGameViewCompletion> {
        let EditorExecutionLane::TrustedCompiled(lane) = &mut self.execution else {
            return Err(completion);
        };
        if !matches!(
            &lane.runtime_owner,
            EditorRuntimeOwnerState::Active {
                game: Some(ActiveEditorGameView {
                    in_flight: Some(identity),
                    ..
                }),
                ..
            } if *identity == completion.identity()
        ) {
            return Err(completion);
        }
        let identity = completion.identity();
        let disposition = completion.disposition();
        let Some(runtime) = lane.runtime_host.running_runtime_mut() else {
            return Err(completion);
        };
        match game_view::complete_candidate(runtime, completion) {
            Ok(()) => {
                let EditorRuntimeOwnerState::Active {
                    game: Some(game), ..
                } = &mut lane.runtime_owner
                else {
                    unreachable!("a completed Game candidate retains its active Runtime owner")
                };
                game.in_flight = None;
                if matches!(
                    lane.pending_step_presentation,
                    Some(EditorStepPresentationState::AwaitingPresentation {
                        identity: expected
                    }) if expected == identity
                ) {
                    match disposition {
                        EditorGameViewCompletionDisposition::Presented => {
                            lane.pending_step_presentation = None;
                            self.play_result = Some(EditorPlayOperationResult::Applied {
                                operation: EditorPlayOperation::StepFixedTick,
                                generation: Some(identity.runtime_generation().get()),
                            });
                        }
                        EditorGameViewCompletionDisposition::Deferred => {
                            lane.pending_step_presentation =
                                Some(EditorStepPresentationState::AwaitingCandidate {
                                    generation: identity.runtime_generation().get(),
                                });
                        }
                        EditorGameViewCompletionDisposition::ContentRejected
                        | EditorGameViewCompletionDisposition::Failed => {
                            lane.pending_step_presentation = None;
                            self.play_result = Some(EditorPlayOperationResult::Failed {
                                operation: EditorPlayOperation::StepFixedTick,
                                failure: EditorPlayFailure::Presentation,
                            });
                        }
                    }
                }
                Ok(())
            }
            Err(completion) => Err(completion),
        }
    }

    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    #[doc(hidden)]
    pub fn set_game_view_source_active(
        &mut self,
        active: bool,
        retirement_message: &'static str,
    ) -> Result<(), EditorGameViewBridgeFailure> {
        if self.closing && active {
            return Err(EditorGameViewBridgeFailure::RuntimeUnavailable);
        }
        let source_is_idle = self.trusted_compiled().is_some_and(|lane| {
            matches!(
                lane.runtime_owner,
                EditorRuntimeOwnerState::Active {
                    game: Some(ActiveEditorGameView {
                        in_flight: None,
                        ..
                    }),
                    ..
                }
            )
        });
        if !source_is_idle {
            return Err(EditorGameViewBridgeFailure::CandidateBusy);
        }
        if !active {
            self.cancel_game_step_presentation();
        }
        {
            let runtime = self
                .trusted_compiled_mut()
                .and_then(|lane| lane.runtime_host.running_runtime_mut())
                .ok_or(EditorGameViewBridgeFailure::RuntimeUnavailable)?;
            game_view::set_capture_enabled(runtime, false)?;
        }

        // Retirement must return an already-produced candidate after new capture admission closes.
        if let Some(candidate) = self.take_existing_game_view_candidate()? {
            let completion = candidate.content_rejected(retirement_message);
            self.complete_game_view_candidate(completion)
                .map_err(|_| EditorGameViewBridgeFailure::CompletionAuthorityMismatch)?;
        }

        if active {
            let runtime = self
                .trusted_compiled_mut()
                .and_then(|lane| lane.runtime_host.running_runtime_mut())
                .ok_or(EditorGameViewBridgeFailure::RuntimeUnavailable)?;
            game_view::set_capture_enabled(runtime, true)?;
        }
        Ok(())
    }

    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    #[doc(hidden)]
    pub fn resize_game_view(
        &mut self,
        extent: Extent2d,
    ) -> Result<bool, EditorGameViewBridgeFailure> {
        if self.closing {
            return Ok(false);
        }
        let Some(lane) = self.trusted_compiled() else {
            return Err(EditorGameViewBridgeFailure::RuntimeUnavailable);
        };
        if !matches!(
            lane.runtime_host.running_runtime_state(),
            Some(RuntimeState::Running | RuntimeState::Paused)
        ) {
            return Ok(false);
        }
        let start = match &lane.runtime_owner {
            EditorRuntimeOwnerState::Active {
                game: Some(game), ..
            } if game.in_flight.is_none() => game.start,
            EditorRuntimeOwnerState::Active { game: Some(_), .. } => {
                return Err(EditorGameViewBridgeFailure::CandidateBusy);
            }
            _ => return Err(EditorGameViewBridgeFailure::RuntimeUnavailable),
        };
        if !start.resizable() {
            return Ok(false);
        }
        let logical = start.logical_window().resolution;
        if logical.physical_width == extent.width && logical.physical_height == extent.height {
            return Ok(false);
        }

        self.cancel_game_step_presentation();
        self.set_game_view_source_active(false, "the Game View geometry was superseded")?;
        let resized = start.resized(extent)?;
        {
            let runtime = self
                .trusted_compiled_mut()
                .and_then(|lane| lane.runtime_host.running_runtime_mut())
                .ok_or(EditorGameViewBridgeFailure::RuntimeUnavailable)?;
            game_view::reconfigure(runtime, resized.capture_config())?;
        }
        let Some(lane) = self.trusted_compiled_mut() else {
            return Err(EditorGameViewBridgeFailure::RuntimeUnavailable);
        };
        let EditorRuntimeOwnerState::Active {
            game: Some(game), ..
        } = &mut lane.runtime_owner
        else {
            return Err(EditorGameViewBridgeFailure::RuntimeUnavailable);
        };
        game.start = resized;
        let runtime = lane
            .runtime_host
            .running_runtime_mut()
            .ok_or(EditorGameViewBridgeFailure::RuntimeUnavailable)?;
        game_view::set_capture_enabled(runtime, true)?;
        Ok(true)
    }

    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    #[doc(hidden)]
    pub fn cancel_game_step_presentation(&mut self) -> bool {
        let Some(lane) = self.trusted_compiled_mut() else {
            return false;
        };
        if lane.pending_step_presentation.take().is_some() {
            self.play_result = Some(EditorPlayOperationResult::Cancelled {
                operation: EditorPlayOperation::StepFixedTick,
            });
            true
        } else {
            false
        }
    }

    /// Requests a Host-owned recovery pause without resolving a pending document decision.
    ///
    /// The Host must first retire the Game source, then keep driving and retry after `Busy`.
    /// Existing controls retain their exact tickets. A queued Step may create a presentation
    /// wait during that drive, so retry once more before deciding that recovery is ready.
    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    #[doc(hidden)]
    pub fn request_runtime_pause_for_gpu_recovery(&mut self) -> EditorPlayRequestResult {
        self.cancel_game_step_presentation();
        let Some(lane) = self.trusted_compiled() else {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeUnavailable(
                EditorRuntimeUnavailableReason::SupervisedPlayUnavailable,
            ));
        };
        if !matches!(lane.runtime_owner, EditorRuntimeOwnerState::Active { .. }) {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::InvalidState);
        }
        if lane.pending_control.is_some() || lane.pending_apply_changes.is_some() {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
        }
        match lane.runtime_host.running_runtime_state() {
            Some(RuntimeState::Paused) => EditorPlayRequestResult::Accepted,
            Some(RuntimeState::Running) => {
                self.request_active_control(EditorPlayOperation::Pause, RuntimeControl::Pause)
            }
            _ => EditorPlayRequestResult::Rejected(EditorPlayRejection::InvalidState),
        }
    }

    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    #[doc(hidden)]
    #[must_use]
    pub fn game_step_requires_runtime_drive(&self) -> bool {
        self.trusted_compiled().is_some_and(|lane| {
            matches!(
                lane.pending_step_presentation,
                Some(EditorStepPresentationState::AwaitingCandidate { .. })
            )
        })
    }

    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    #[doc(hidden)]
    pub fn apply_game_view_input_batch(
        &mut self,
        batch: EditorGameInputBatch,
    ) -> Result<(), EditorGameViewBridgeFailure> {
        let events = batch.into_events();
        if events.is_empty() {
            return Ok(());
        }
        if events.len() > EditorGameInputBatch::MAX_EVENTS {
            return Err(EditorGameViewBridgeFailure::InputBudgetExceeded);
        }
        let Some(lane) = self.trusted_compiled() else {
            return Err(EditorGameViewBridgeFailure::RuntimeUnavailable);
        };
        let expected_start = match &lane.runtime_owner {
            EditorRuntimeOwnerState::Active {
                game: Some(game), ..
            } => game.start,
            _ => return Err(EditorGameViewBridgeFailure::RuntimeUnavailable),
        };
        if events
            .iter()
            .any(|event| event.identity().start() != expected_start)
        {
            return Err(EditorGameViewBridgeFailure::GameStartRequestMismatch);
        }
        let expected_route = events[0].route_generation();
        if expected_route == 0
            || events
                .iter()
                .any(|event| event.route_generation() != expected_route)
        {
            return Err(EditorGameViewBridgeFailure::InputRouteMismatch);
        }
        if events.iter().any(|event| {
            matches!(
                event.event(),
                EditorGameInputEvent::Focused(false) | EditorGameInputEvent::SurfaceRetired
            )
        }) {
            if matches!(
                &lane.runtime_owner,
                EditorRuntimeOwnerState::Active { game: Some(game), .. }
                    if game.in_flight.is_some()
            ) {
                return Err(EditorGameViewBridgeFailure::CandidateBusy);
            }
            if let Some(candidate) = self.take_game_view_candidate()? {
                self.complete_game_view_candidate(candidate.interrupted())
                    .map_err(|_| EditorGameViewBridgeFailure::CompletionAuthorityMismatch)?;
            }
        }
        let lane = self
            .trusted_compiled_mut()
            .ok_or(EditorGameViewBridgeFailure::RuntimeUnavailable)?;
        let runtime = lane
            .runtime_host
            .running_runtime_mut()
            .ok_or(EditorGameViewBridgeFailure::RuntimeUnavailable)?;
        game_view::apply_input_batch(runtime, EditorGameInputBatch::new(events))
    }

    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    fn game_view_retirement_blocked(&mut self) -> bool {
        let Some(lane) = self.trusted_compiled_mut() else {
            return false;
        };
        match &lane.runtime_owner {
            EditorRuntimeOwnerState::Active {
                game: Some(game), ..
            } if game.in_flight.is_some() => return true,
            EditorRuntimeOwnerState::Active { game: Some(_), .. } => {}
            _ => return false,
        }
        lane.runtime_host
            .running_runtime_mut()
            .is_some_and(game_view::retirement_blocked)
    }

    #[cfg(feature = "presentation-2d")]
    fn drive_editor_preview(&mut self) {
        if self.closing
            || self.schema_adoption_fenced()
            || self.authoring_runtime.preview_extent().is_none()
        {
            return;
        }
        let source = if self.authoring_project().is_some() {
            self.drive_authoring_scene_capture()
        } else {
            self.scene_preview_document.and_then(|document| {
                let slot = self.workspace.scene(document)?;
                let dependencies = preview::EditorPreviewDependencies::empty();
                let projection = self
                    .authoring_runtime
                    .preview_requires_source_projection(document, slot.revision(), &dependencies)
                    .then(|| {
                        preview::EditorPreviewProjection::trusted_compiled(
                            self.scene_lanes
                                .get(&document)
                                .expect("an open Scene retains its admitted source")
                                .source_key
                                .clone(),
                            slot.session().document().clone(),
                            authoring_preview::EditorAuthoringPreviewOmissions::empty(
                                document,
                                slot.revision(),
                            ),
                        )
                    });
                Some(preview::EditorPreviewSource {
                    document,
                    revision: slot.revision(),
                    dependencies,
                    projection,
                })
            })
        };
        if let Some(report) = self.authoring_runtime.drive_preview(source) {
            let _ = self.diagnostics.extend(report);
        }
        self.release_published_scene_capture();
    }

    #[cfg(test)]
    #[must_use]
    pub(super) const fn workspace(&self) -> &EditorWorkspace {
        &self.workspace
    }

    #[cfg(test)]
    #[must_use]
    pub(super) fn workspace_model(&self) -> nara_tooling::EditorWorkspaceModel {
        match self.trusted_compiled() {
            Some(lane) => self
                .workspace
                .model(lane.plan.schema_validation().registry(), None),
            None => self.workspace.authoring_model(),
        }
    }

    #[cfg(test)]
    pub(super) fn test_schema_snapshot(&self) -> nara_reflect::ComponentRegistrySnapshot {
        self.trusted_compiled()
            .expect("the test schema snapshot is available only in the trusted compiled lane")
            .plan
            .schema_validation()
            .registry()
            .snapshot()
            .expect("the Editor plan registry is frozen")
    }

    #[cfg(test)]
    pub(super) fn test_runtime_schema_snapshot(
        &self,
    ) -> Option<nara_reflect::ComponentRegistrySnapshot> {
        self.trusted_compiled()?
            .runtime_host
            .with_running_world(|world| component_registry(world)?.snapshot().ok())
            .ok()
            .flatten()
    }

    #[cfg(test)]
    #[must_use]
    pub(super) fn test_project_content_revision(
        &self,
    ) -> crate::project_host::ProjectContentRevision {
        self.trusted_compiled()
            .expect("project content is available only in the trusted compiled lane")
            .snapshot
            .revision()
    }

    #[cfg(test)]
    #[must_use]
    pub(super) fn test_project_content_budget_snapshot(
        &self,
    ) -> crate::project_host::ProjectContentBudgetSnapshot {
        self.trusted_compiled()
            .expect("project content is available only in the trusted compiled lane")
            .content_loader
            .budget_snapshot()
    }

    #[cfg(test)]
    pub(super) fn test_project_content_budget_host(
        &self,
    ) -> crate::project_host::ProjectContentBudgetHost {
        self.trusted_compiled()
            .expect("project content is available only in the trusted compiled lane")
            .content_loader
            .budget_host()
    }

    #[cfg(test)]
    #[cfg(feature = "presentation-2d")]
    pub(super) fn test_exhaust_image_publication_revision(&mut self) {
        self.authoring_runtime
            .test_exhaust_image_publication_revision();
    }

    #[cfg(test)]
    pub(super) fn test_hold_next_image_import(
        &mut self,
    ) -> authoring_runtime::EditorImageImportTestControl {
        self.authoring_runtime.hold_next_image_import()
    }

    #[cfg(test)]
    pub(super) fn test_fault_after_next_image_publication(
        &mut self,
    ) -> nara_app::RuntimeFaultReporter {
        self.authoring_runtime
            .test_fault_after_next_image_publication()
    }

    #[cfg(test)]
    pub(super) fn test_image_import_asset(
        &self,
        handle: nara_asset::Handle<nara_image::ImageAsset>,
        version: nara_asset::AssetVersion,
    ) -> Option<&nara_image::ImageAsset> {
        self.authoring_runtime.image_import_asset(handle, version)
    }

    #[cfg(test)]
    pub(super) fn test_hold_next_image_import_through_shutdown(
        &mut self,
    ) -> authoring_runtime::EditorImageImportTestControl {
        self.authoring_runtime
            .hold_next_image_import_through_shutdown()
    }

    #[cfg(test)]
    pub(super) fn test_authoring_task_stats(&self) -> nara_tasks::TaskStats {
        self.authoring_runtime.persistence_tasks().stats()
    }

    #[cfg(test)]
    pub(super) fn test_image_import_budget_host(&self) -> nara_image::ImageImportBudgetHost {
        self.authoring_runtime.image_import_budget_host()
    }

    #[cfg(test)]
    pub(super) fn test_tile_set(
        &self,
        path: &nara_asset::AssetPath,
    ) -> Option<nara_tilemap::TileSet> {
        self.authoring_runtime.test_tile_set(path)
    }

    #[cfg(test)]
    pub(super) fn test_reject_next_valid_image_metadata_receipt(&mut self) {
        self.image_import.test_reject_next_valid_metadata_receipt();
    }

    #[cfg(test)]
    pub(super) fn test_fail_next_image_metadata_after_write(
        &mut self,
    ) -> crate::project_host::file_publication::CleanupTestControl {
        self.image_import.test_fail_next_metadata_after_write()
    }

    #[cfg(test)]
    pub(super) fn test_panic_after_image_metadata_publication(&mut self) {
        self.image_import.test_panic_after_metadata_publication();
    }

    #[cfg(test)]
    pub(super) fn test_allow_image_publication_cleanup(&mut self) {
        self.image_import.test_allow_publication_cleanup();
    }

    #[must_use]
    #[doc(hidden)]
    pub(crate) fn workspace_intent_view(&self) -> EditorWorkspaceIntentView {
        let (intent, phase, decision_document) = match self.pending_workspace_intent.as_ref() {
            Some(pending) => (
                Some(pending.intent()),
                Some(pending.phase()),
                pending.decision_document(),
            ),
            None => (None, None, None),
        };
        EditorWorkspaceIntentView::new(
            intent,
            phase,
            decision_document,
            self.pending_workspace_intent
                .as_ref()
                .and_then(PendingWorkspaceIntent::prompt),
            self.workspace_intent_result,
        )
    }

    #[cfg(test)]
    #[must_use]
    pub(super) const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    pub(crate) fn apply_workspace_command(
        &mut self,
        command: EditorWorkspaceCommand,
    ) -> EditorWorkspaceCommandReport {
        if self.schema_adoption_fenced()
            && !matches!(
                command,
                EditorWorkspaceCommand::SetActiveDocument { .. }
                    | EditorWorkspaceCommand::SelectEntity { .. }
                    | EditorWorkspaceCommand::MarkExternalChanged { .. }
            )
        {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.schema-adoption-busy",
            );
        }
        if matches!(command, EditorWorkspaceCommand::CloseDocument { .. }) {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.close-requires-workspace-intent",
            );
        }
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.workspace-intent-pending",
            );
        }
        let selected_document = match &command {
            EditorWorkspaceCommand::SetActiveDocument { document } => Some(*document),
            _ => None,
        };
        let report = match &mut self.execution {
            EditorExecutionLane::Authoring(_) => self.workspace.apply_authoring_command(command),
            EditorExecutionLane::TrustedCompiled(lane) => self
                .workspace
                .apply_command(lane.plan.schema_validation().registry(), command),
        };
        if report.applied
            && selected_document.is_some_and(|document| {
                self.workspace.document_kind(document) == Some(EditorDocumentKind::Scene)
            })
        {
            self.scene_preview_document = selected_document;
        }
        report
    }

    pub(crate) fn request_workspace_intent(
        &mut self,
        intent: EditorWorkspaceIntent,
    ) -> EditorWorkspaceIntentRequestResult {
        if self.schema_adoption_fenced() {
            self.schema_owner_adoption.cancel();
            if self.schema_owner_adoption.pending() {
                return EditorWorkspaceIntentRequestResult::Rejected(
                    EditorWorkspaceIntentRejection::Busy,
                );
            }
        }
        if !self.accepts_new_work()
            || self.pending_workspace_intent.is_some()
            || self.persistence.has_pending_work()
        {
            return EditorWorkspaceIntentRequestResult::Rejected(
                EditorWorkspaceIntentRejection::Busy,
            );
        }
        if self.workspace_intent_result.is_some() {
            return EditorWorkspaceIntentRequestResult::Rejected(
                EditorWorkspaceIntentRejection::ResultPending,
            );
        }

        let mut pending = match intent {
            EditorWorkspaceIntent::CloseDocument { document } => {
                let Some(dirty) = self.workspace.document_is_dirty(document) else {
                    let reason = if self.workspace.active_document().is_some() {
                        EditorWorkspaceIntentRejection::MissingDocument
                    } else {
                        EditorWorkspaceIntentRejection::NoActiveDocument
                    };
                    self.workspace_intent_result =
                        Some(EditorWorkspaceIntentResult::Rejected { intent, reason });
                    return EditorWorkspaceIntentRequestResult::Rejected(reason);
                };
                PendingWorkspaceIntent::CloseDocument(PendingDocumentClose {
                    document,
                    prompt: None,
                    phase: if dirty {
                        WorkspaceIntentProgress::AwaitingDecision
                    } else {
                        WorkspaceIntentProgress::RetiringRuntime
                    },
                })
            }
            EditorWorkspaceIntent::Exit => {
                let documents = self.workspace.__document_ids();
                if documents.is_empty() {
                    self.cancel_pending_document_open();
                    self.workspace_intent_result =
                        Some(EditorWorkspaceIntentResult::Applied { intent });
                    return EditorWorkspaceIntentRequestResult::Accepted;
                }
                let mut pending = PendingExitPreparation::new(documents);
                self.prepare_current_exit_document(&mut pending);
                PendingWorkspaceIntent::Exit(pending)
            }
        };
        self.cancel_pending_document_open();
        self.refresh_workspace_close_prompt(&mut pending);
        let should_retire = pending.phase() == EditorWorkspaceIntentPhase::RetiringRuntime;
        self.pending_workspace_intent = Some(pending);
        if should_retire {
            self.advance_workspace_retirement();
        }
        EditorWorkspaceIntentRequestResult::Accepted
    }

    pub(crate) fn resolve_workspace_intent(
        &mut self,
        prompt: EditorClosePromptToken,
        decision: EditorCloseDecision,
    ) -> EditorWorkspaceIntentRequestResult {
        let Some(mut pending) = self.pending_workspace_intent.take() else {
            return EditorWorkspaceIntentRequestResult::Rejected(
                EditorWorkspaceIntentRejection::DecisionNotRequired,
            );
        };
        if pending.phase() != EditorWorkspaceIntentPhase::AwaitingDecision {
            self.pending_workspace_intent = Some(pending);
            return EditorWorkspaceIntentRequestResult::Rejected(
                EditorWorkspaceIntentRejection::Busy,
            );
        }
        if pending.prompt() != Some(prompt) {
            self.pending_workspace_intent = Some(pending);
            return EditorWorkspaceIntentRequestResult::Rejected(
                EditorWorkspaceIntentRejection::StalePrompt,
            );
        }
        pending.set_prompt(None);
        let intent = pending.intent();
        let Some(document) = pending.decision_document() else {
            self.pending_workspace_intent = Some(pending);
            return EditorWorkspaceIntentRequestResult::Rejected(
                EditorWorkspaceIntentRejection::DecisionNotRequired,
            );
        };

        match decision {
            EditorCloseDecision::Cancel => {
                self.cancel_pending_document_open();
                self.workspace_intent_result =
                    Some(EditorWorkspaceIntentResult::Cancelled { intent });
            }
            EditorCloseDecision::Discard => {
                match &mut pending {
                    PendingWorkspaceIntent::CloseDocument(close) => {
                        close.phase = WorkspaceIntentProgress::RetiringRuntime;
                    }
                    PendingWorkspaceIntent::Exit(exit) => {
                        exit.cursor = exit.cursor.saturating_add(1);
                        self.prepare_current_exit_document(exit);
                    }
                }
                self.refresh_workspace_close_prompt(&mut pending);
                let should_retire = pending.phase() == EditorWorkspaceIntentPhase::RetiringRuntime;
                self.pending_workspace_intent = Some(pending);
                if should_retire {
                    self.advance_workspace_retirement();
                }
            }
            EditorCloseDecision::Save => {
                let requested =
                    self.request_persistence_for_workspace_intent(EditorPersistenceCommand::Save {
                        document: Some(document),
                    });
                let attempt = match requested {
                    EditorPersistenceRequestResult::Accepted { attempt } => attempt,
                    EditorPersistenceRequestResult::Rejected { reason, .. }
                    | EditorPersistenceRequestResult::Refused(reason) => {
                        self.refresh_workspace_close_prompt(&mut pending);
                        self.pending_workspace_intent = Some(pending);
                        return EditorWorkspaceIntentRequestResult::Rejected(
                            EditorWorkspaceIntentRejection::PersistenceRejected(reason),
                        );
                    }
                    EditorPersistenceRequestResult::Acknowledged { .. } => unreachable!(),
                };
                pending.set_progress(WorkspaceIntentProgress::Saving(attempt));
                self.pending_workspace_intent = Some(pending);
            }
        }
        EditorWorkspaceIntentRequestResult::Accepted
    }

    fn refresh_workspace_close_prompt(&mut self, pending: &mut PendingWorkspaceIntent) {
        let prompt = if pending.phase() == EditorWorkspaceIntentPhase::AwaitingDecision {
            pending
                .decision_document()
                .map(|document| self.frame_clock.issue_close_prompt(document))
        } else {
            None
        };
        pending.set_prompt(prompt);
    }

    pub(crate) fn commit_prepared_workspace_intent(
        &mut self,
    ) -> EditorWorkspaceIntentRequestResult {
        let Some(pending) = self.pending_workspace_intent.take() else {
            return EditorWorkspaceIntentRequestResult::Rejected(
                EditorWorkspaceIntentRejection::DecisionNotRequired,
            );
        };
        if !matches!(
            &pending,
            PendingWorkspaceIntent::Exit(exit)
                if exit.phase.phase() == EditorWorkspaceIntentPhase::Prepared
        ) {
            self.pending_workspace_intent = Some(pending);
            return EditorWorkspaceIntentRequestResult::Rejected(
                EditorWorkspaceIntentRejection::Busy,
            );
        }
        self.workspace_intent_result = Some(EditorWorkspaceIntentResult::Applied {
            intent: EditorWorkspaceIntent::Exit,
        });
        EditorWorkspaceIntentRequestResult::Accepted
    }

    pub(crate) fn abort_prepared_workspace_intent(&mut self) -> EditorWorkspaceIntentRequestResult {
        let Some(pending) = self.pending_workspace_intent.take() else {
            return EditorWorkspaceIntentRequestResult::Rejected(
                EditorWorkspaceIntentRejection::DecisionNotRequired,
            );
        };
        if !matches!(
            &pending,
            PendingWorkspaceIntent::Exit(exit)
                if exit.phase.phase() == EditorWorkspaceIntentPhase::Prepared
        ) {
            self.pending_workspace_intent = Some(pending);
            return EditorWorkspaceIntentRequestResult::Rejected(
                EditorWorkspaceIntentRejection::Busy,
            );
        }
        self.workspace_intent_result = Some(EditorWorkspaceIntentResult::Cancelled {
            intent: EditorWorkspaceIntent::Exit,
        });
        EditorWorkspaceIntentRequestResult::Accepted
    }

    pub(crate) fn acknowledge_workspace_intent_result(&mut self) -> bool {
        self.workspace_intent_result.take().is_some()
    }

    pub(crate) fn request_runtime_edit_request(
        &mut self,
        request: EditorRuntimeEditRequest,
    ) -> Result<(), EditorRuntimeEditRejection> {
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return Err(EditorRuntimeEditRejection::Busy);
        }
        let (document, generation) = match &self.execution {
            EditorExecutionLane::Authoring(_) => {
                return Err(EditorRuntimeEditRejection::InvalidRuntimeState);
            }
            EditorExecutionLane::TrustedCompiled(lane) => {
                if lane.pending_control.is_some()
                    || lane.pending_apply_changes.is_some()
                    || lane.runtime_edit_result.is_some()
                {
                    return Err(EditorRuntimeEditRejection::Busy);
                }
                let EditorRuntimeOwnerState::Active { source, .. } = &lane.runtime_owner else {
                    return Err(EditorRuntimeEditRejection::InvalidRuntimeState);
                };
                if !matches!(
                    lane.runtime_host.running_runtime_state(),
                    Some(RuntimeState::Running | RuntimeState::Paused)
                ) {
                    return Err(EditorRuntimeEditRejection::InvalidRuntimeState);
                }
                let generation = lane
                    .runtime_host
                    .running_runtime_generation()
                    .ok_or(EditorRuntimeEditRejection::InvalidRuntimeState)?;
                (source.document, generation)
            }
        };
        if request.generation != generation {
            return Err(EditorRuntimeEditRejection::StaleGeneration);
        }
        let revision = self
            .workspace
            .scene(document)
            .ok_or(EditorRuntimeEditRejection::StaleDocument)?
            .revision();
        if request.document_revision != revision {
            return Err(EditorRuntimeEditRejection::StaleDocument);
        }
        let lane = self
            .trusted_compiled_mut()
            .ok_or(EditorRuntimeEditRejection::InvalidRuntimeState)?;
        #[cfg(test)]
        let inject_fault = std::mem::take(&mut lane.fault_after_next_runtime_edit);
        let runtime = lane
            .runtime_host
            .running_runtime_mut()
            .ok_or(EditorRuntimeEditRejection::InvalidRuntimeState)?;
        #[cfg(test)]
        let fault_reporter = runtime.fault_reporter();
        let submitted = runtime
            .with_driver_scope(|scope| {
                let output = scope.__apply_port::<EditorRuntimeBridge>(
                    EditorRuntimeBridgeInput::Submit(request.clone()),
                );
                #[cfg(test)]
                if inject_fault && matches!(&output, Ok(EditorRuntimeBridgeOutput::Submitted)) {
                    fault_reporter.report(nara_app::RuntimeFault::engine(
                        nara_app::RuntimeFaultKind::RequiredService,
                        "nara.test.editor-runtime-edit-post-operation",
                    ));
                }
                output
            })
            .map_err(|_| EditorRuntimeEditRejection::InvalidRuntimeState)?
            // Register an accepted request before the normal Host drive observes the sticky fault.
            .output
            .map_err(|_| EditorRuntimeEditRejection::InvalidRuntimeState)?;
        match submitted {
            EditorRuntimeBridgeOutput::Submitted => {
                lane.runtime_edit_result = Some(EditorRuntimeEditResult::Pending(request));
                Ok(())
            }
            EditorRuntimeBridgeOutput::Busy | EditorRuntimeBridgeOutput::Result(_) => {
                Err(EditorRuntimeEditRejection::Busy)
            }
        }
    }

    #[cfg(test)]
    pub(super) fn test_fault_after_next_runtime_edit(&mut self) -> nara_app::RuntimeFaultReporter {
        let lane = self
            .trusted_compiled_mut()
            .expect("the runtime-edit fault gate requires trusted compiled Play");
        let reporter = lane
            .runtime_host
            .running_runtime_mut()
            .expect("the runtime-edit fault gate requires a running Runtime")
            .fault_reporter();
        lane.fault_after_next_runtime_edit = true;
        reporter
    }

    #[cfg(test)]
    #[must_use]
    pub(super) fn runtime_edit_result(&self) -> Option<&EditorRuntimeEditResult> {
        self.trusted_compiled()
            .and_then(|lane| lane.runtime_edit_result.as_ref())
    }

    pub(crate) fn acknowledge_runtime_edit_result(&mut self) -> bool {
        let Some(lane) = self.trusted_compiled_mut() else {
            return false;
        };
        if matches!(
            lane.runtime_edit_result,
            Some(EditorRuntimeEditResult::Pending(_))
        ) {
            return false;
        }
        lane.runtime_edit_result.take().is_some()
    }

    pub(crate) fn request_apply_changes(
        &mut self,
        request: SceneApplyChangesRequest,
    ) -> Result<(), EditorApplyChangesRejection> {
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return Err(EditorApplyChangesRejection::Busy);
        }
        let (document, source_revision, generation) = match &self.execution {
            EditorExecutionLane::Authoring(_) => {
                return Err(EditorApplyChangesRejection::InvalidRuntimeState);
            }
            EditorExecutionLane::TrustedCompiled(lane) => {
                if lane.pending_control.is_some()
                    || lane.pending_apply_changes.is_some()
                    || lane.apply_changes_result.is_some()
                    || matches!(
                        lane.runtime_edit_result,
                        Some(EditorRuntimeEditResult::Pending(_))
                    )
                {
                    return Err(EditorApplyChangesRejection::Busy);
                }
                let EditorRuntimeOwnerState::Active { source, .. } = &lane.runtime_owner else {
                    return Err(EditorApplyChangesRejection::InvalidRuntimeState);
                };
                if !matches!(
                    lane.runtime_host.running_runtime_state(),
                    Some(RuntimeState::Running | RuntimeState::Paused)
                ) {
                    return Err(EditorApplyChangesRejection::InvalidRuntimeState);
                }
                let generation = lane
                    .runtime_host
                    .running_runtime_generation()
                    .ok_or(EditorApplyChangesRejection::InvalidRuntimeState)?;
                (source.document, source.revision, generation)
            }
        };
        let document_revision = self
            .workspace
            .scene(document)
            .ok_or(EditorApplyChangesRejection::StaleDocument)?
            .revision();
        if document_revision != source_revision {
            return Err(EditorApplyChangesRejection::RuntimeOutOfDate);
        }
        let Some(lane) = self.trusted_compiled_mut() else {
            return Err(EditorApplyChangesRejection::InvalidRuntimeState);
        };
        lane.pending_apply_changes = Some(PendingApplyChanges {
            generation,
            document,
            document_revision,
            source_revision,
            request: request.clone(),
        });
        lane.apply_changes_result = Some(EditorApplyChangesResult::Pending {
            generation,
            document_revision,
            request,
        });
        Ok(())
    }

    #[cfg(test)]
    #[must_use]
    pub(super) fn apply_changes_result(&self) -> Option<&EditorApplyChangesResult> {
        self.trusted_compiled()
            .and_then(|lane| lane.apply_changes_result.as_ref())
    }

    pub(crate) fn acknowledge_apply_changes_result(&mut self) -> bool {
        let Some(lane) = self.trusted_compiled_mut() else {
            return false;
        };
        if lane.pending_apply_changes.is_some() {
            return false;
        }
        lane.apply_changes_result.take().is_some()
    }

    fn advance_workspace_retirement(&mut self) {
        let Some(pending) = self.pending_workspace_intent.as_ref() else {
            return;
        };
        if pending.phase() != EditorWorkspaceIntentPhase::RetiringRuntime {
            return;
        }
        let close_document = match pending {
            PendingWorkspaceIntent::CloseDocument(pending) => Some(pending.document),
            PendingWorkspaceIntent::Exit(_) => None,
        };
        let Some(state) = self.take_trusted_runtime_owner() else {
            self.finish_workspace_intent_document();
            return;
        };
        if close_document.is_some_and(|pending_document| {
            state
                .source()
                .is_some_and(|source| source.document != pending_document)
        }) {
            let _ = self.install_trusted_runtime_owner(state);
            self.finish_workspace_intent_document();
            return;
        }

        match state {
            EditorRuntimeOwnerState::Empty => {
                let _ = self.install_trusted_runtime_owner(EditorRuntimeOwnerState::Empty);
                self.finish_workspace_intent_document();
            }
            EditorRuntimeOwnerState::Preparing { operation, .. } => {
                self.play_result = Some(EditorPlayOperationResult::Cancelled { operation });
                let _ = self.install_trusted_runtime_owner(EditorRuntimeOwnerState::Empty);
                self.finish_workspace_intent_document();
            }
            EditorRuntimeOwnerState::Starting {
                source,
                operation,
                attempt,
            } => {
                self.cancel_pending_runtime_edit();
                self.cancel_pending_apply_changes();
                let _ =
                    self.install_trusted_runtime_owner(EditorRuntimeOwnerState::CancellingStart {
                        source,
                        terminal: EditorPlayOperationResult::Cancelled { operation },
                        attempt,
                    });
            }
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            EditorRuntimeOwnerState::AwaitingGameTarget {
                source,
                operation,
                prepared,
                ..
            } => {
                if let Some(lane) = self.trusted_compiled_mut() {
                    lane.runtime_host.abort_prepared_start(prepared);
                }
                let _ = self.install_trusted_runtime_owner(EditorRuntimeOwnerState::RetiringPlay {
                    source,
                    terminal: EditorPlayOperationResult::Cancelled { operation },
                });
            }
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            state @ EditorRuntimeOwnerState::CommittingGameTarget { .. } => {
                let _ = self.install_trusted_runtime_owner(state);
            }
            state @ EditorRuntimeOwnerState::Active { .. } => {
                let _ = self.install_trusted_runtime_owner(state);
                let (runtime_state, control_pending) =
                    self.trusted_compiled().map_or((None, false), |lane| {
                        (
                            lane.runtime_host.running_runtime_state(),
                            lane.pending_control.is_some(),
                        )
                    });
                if !control_pending
                    && !matches!(
                        runtime_state,
                        Some(
                            RuntimeState::Stopping
                                | RuntimeState::Stopped
                                | RuntimeState::CloseIncomplete
                        )
                    )
                {
                    let _ = self
                        .request_active_control(EditorPlayOperation::Stop, RuntimeControl::Stop);
                }
            }
            state @ (EditorRuntimeOwnerState::CancellingStart { .. }
            | EditorRuntimeOwnerState::RetiringPlay { .. }
            | EditorRuntimeOwnerState::RetirementIncomplete { .. }) => {
                let _ = self.install_trusted_runtime_owner(state);
            }
            EditorRuntimeOwnerState::Transitioning => {
                unreachable!("an Editor Runtime transition cannot drive document Close")
            }
        }
    }

    fn finish_workspace_intent_document(&mut self) {
        let Some(pending) = self.pending_workspace_intent.take() else {
            return;
        };
        match pending {
            PendingWorkspaceIntent::CloseDocument(pending) => {
                let scene_clean = self.scene_lanes.get(&pending.document).is_none_or(|lane| {
                    lane.host
                        .try_with(|host| host.retry_cleanup())
                        .unwrap_or(false)
                });
                let prefab_clean = self.prefab_lanes.get(&pending.document).is_none_or(|lane| {
                    lane.host
                        .try_with(|host| host.retry_cleanup())
                        .unwrap_or(false)
                });
                let extension_clean = self.extension_documents.retire(pending.document);
                if !scene_clean || !prefab_clean || !extension_clean {
                    self.workspace_intent_result = Some(EditorWorkspaceIntentResult::Rejected {
                        intent: EditorWorkspaceIntent::CloseDocument {
                            document: pending.document,
                        },
                        reason: EditorWorkspaceIntentRejection::PersistenceRejected(
                            EditorPersistenceRejection::CleanupPending,
                        ),
                    });
                    return;
                }
                let closing_kind = self.workspace.document_kind(pending.document);
                let report = self.workspace.__discard_document(pending.document);
                let applied = report.applied;
                if report.applied {
                    match closing_kind {
                        Some(EditorDocumentKind::Scene) => {
                            if let Some(lane) = self.scene_lanes.remove(&pending.document) {
                                assert!(lane.host.retire());
                            }
                            if self.scene_preview_document == Some(pending.document) {
                                self.scene_preview_document = None;
                            }
                        }
                        Some(EditorDocumentKind::Prefab) => {
                            if let Some(lane) = self.prefab_lanes.remove(&pending.document) {
                                assert!(lane.host.retire());
                            }
                        }
                        _ => {}
                    }
                }
                let _ = self.diagnostics.extend(report.diagnostics);
                self.workspace_intent_result = Some(if applied {
                    EditorWorkspaceIntentResult::Applied {
                        intent: EditorWorkspaceIntent::CloseDocument {
                            document: pending.document,
                        },
                    }
                } else {
                    EditorWorkspaceIntentResult::Rejected {
                        intent: EditorWorkspaceIntent::CloseDocument {
                            document: pending.document,
                        },
                        reason: EditorWorkspaceIntentRejection::MissingDocument,
                    }
                });
            }
            PendingWorkspaceIntent::Exit(mut pending) => {
                pending.phase = WorkspaceIntentProgress::Prepared;
                self.pending_workspace_intent = Some(PendingWorkspaceIntent::Exit(pending));
            }
        }
    }

    fn prepare_current_exit_document(&self, pending: &mut PendingExitPreparation) {
        while let Some(document) = pending.current_document() {
            if self.workspace.document_is_dirty(document).unwrap_or(false) {
                pending.phase = WorkspaceIntentProgress::AwaitingDecision;
                return;
            }
            pending.cursor = pending.cursor.saturating_add(1);
        }
        pending.phase = WorkspaceIntentProgress::RetiringRuntime;
    }
}

fn editor_open_error(code: &'static str) -> EditorProjectOpenError {
    EditorProjectOpenError::new(single_error(code, "Editor project could not be opened"))
}

fn editor_persistence_open_error(stage: EditorPersistenceFailureStage) -> EditorProjectOpenError {
    let code = match stage {
        EditorPersistenceFailureStage::OpenTarget => "project.editor.scene-open-failed",
        EditorPersistenceFailureStage::ReadTarget => "project.editor.scene-read-failed",
        EditorPersistenceFailureStage::Decode => "project.editor.scene-decode-failed",
        EditorPersistenceFailureStage::Validate => "project.editor.scene-validation-failed",
        EditorPersistenceFailureStage::Encode
        | EditorPersistenceFailureStage::CreateTemporary
        | EditorPersistenceFailureStage::WriteTemporary
        | EditorPersistenceFailureStage::SyncTemporary
        | EditorPersistenceFailureStage::ReplaceTarget => {
            "project.editor.scene-open-internal-failed"
        }
    };
    editor_open_error(code)
}

fn editor_workspace_command_rejected(
    active_document: Option<EditorDocumentId>,
    code: &'static str,
) -> EditorWorkspaceCommandReport {
    EditorWorkspaceCommandReport {
        active_document,
        diagnostics: single_error(code, "Editor workspace command is unavailable"),
        ..EditorWorkspaceCommandReport::default()
    }
}

#[cfg(test)]
mod tests {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    use super::*;

    use nara_ecs::Component;
    use nara_reflect::{
        ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentSchema,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    };
    use nara_scene::SceneEntityId;

    #[cfg(feature = "runtime-ui")]
    #[test]
    fn native_prefab_patch_cannot_cross_into_ui_domain_without_ui_admission() {
        use nara_scene::{
            PrefabAuthoringSession, PrefabDocument, SceneComponentRecord, SceneDocument,
            SceneEntityRecord, ScenePatchDocument, ScenePatchOperation,
        };
        use nara_tooling::{
            EditorAction, EditorActionBatch, EditorActionOutcome, EditorDocumentMutation,
            EditorDocumentSourceBinding,
        };
        use std::{
            fs,
            sync::atomic::{AtomicU64, Ordering},
            time::Instant,
        };

        static NEXT_ROOT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "nara-ui-transition-{}-{}",
            std::process::id(),
            NEXT_ROOT.fetch_add(1, Ordering::Relaxed)
        ));
        for directory in ["assets", "scenes", "prefabs", "schema"] {
            fs::create_dir_all(root.join(directory)).unwrap();
        }
        fs::write(root.join("nara.toml"), "schema_version = 1\n[project]\nsource_universe = '75a0a1c0-daf6-5671-b219-c2e2137d5517'\nnative_document_identity = 'project_location'\nname = \"UI transition\"\n[paths]\nassets = \"assets\"\nscenes = \"scenes\"\nprefabs = \"prefabs\"\n[startup]\ndefault_scene = \"startup.scene.json\"\n[authoring]\nschema_bundle = \"schema/authoring.bundle.json\"\n[runtime]\npreset = \"local-headless\"\n[capabilities]\nrequested = [\"runtime-2d\", \"runtime-ui\"]\n").unwrap();
        fs::write(
            root.join("scenes/startup.scene.json"),
            SceneDocument::default().to_json_string().unwrap(),
        )
        .unwrap();
        let bundle = nara_reflect::AuthoringSchemaBundle::from_trusted_provider_definitions(
            built_in_schema_providers(),
        )
        .unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            bundle.to_json_string().unwrap(),
        )
        .unwrap();
        #[cfg(windows)]
        let directory = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&root)
                .unwrap()
        };
        #[cfg(unix)]
        let directory = fs::File::open(&root).unwrap();
        let capability = DirectoryCapability::from_host_handle(
            directory,
            nara_fs::HostCapabilityOptions::new(
                nara_fs::CapabilityRights::ReadWrite,
                nara_fs::TrustMode::TrustedLocal,
            ),
        )
        .unwrap();
        let mut editor = EditorProjectSession::open_trusted_compiled(
            capability,
            TrustedCompiledEditorProjectIntent::new(),
        )
        .unwrap();
        let entity = SceneEntityId::new("ordinary").unwrap();
        let original = PrefabDocument::from_ordered_roots([SceneEntityRecord::new(entity.clone())]);
        let document = editor
            .workspace
            .open_prefab_session(
                "Ordinary",
                EditorDocumentSourceBinding::new("ordinary.prefab.json"),
                PrefabAuthoringSession::new(original.clone()),
            )
            .unwrap()
            .opened_document
            .unwrap();
        let frame = editor.drive_editor_frame(Duration::ZERO);
        let revision = frame.workspace().document(document).unwrap().revision;
        let report = editor
            .dispatch_editor_actions(EditorActionBatch::new(
                frame.revision(),
                [EditorAction::Mutate(
                    EditorDocumentMutation::ApplyPrefabPatch {
                        document,
                        base_revision: revision,
                        patch: ScenePatchDocument::new([ScenePatchOperation::AddComponent {
                            entity,
                            component: ComponentTypeId::new("nara.ui.UiRoot"),
                            value: SceneComponentRecord::new(
                                ComponentSchemaVersion::ONE,
                                ComponentValue::map([(
                                    "layer",
                                    ComponentValue::String("base".into()),
                                )]),
                            ),
                        }]),
                    },
                )],
            ))
            .unwrap();
        let accepted = matches!(&report.outcomes()[0], EditorActionOutcome::Workspace(report) if report.applied);
        let slot = editor.workspace.prefab(document).unwrap();
        let unchanged = slot.session().document() == &original
            && slot.revision() == revision
            && slot.session().history_status().undo_depth == 0;

        // Data-authoring admission cannot replace the integrated UI structural contract.
        let ui_component = ComponentTypeId::new("nara.ui.UiRoot");
        let ui_template = ComponentValue::map([("layer", ComponentValue::String("base".into()))]);
        let data_bundle = nara_reflect::AuthoringSchemaBundle::new([
            nara_reflect::AuthoringSchemaBundleOwner::new(
                nara_reflect::PersistentComponentSchemaOwnerId::try_new("test.retained-ui")
                    .unwrap(),
                nara_reflect::ComponentSchemaCatalog {
                    components: vec![
                        ComponentSchema::new(
                            ui_component.clone(),
                            "Root",
                            ComponentSchemaVersion::ONE,
                        )
                        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
                        .with_fields([ComponentFieldSchema::required(
                            ComponentFieldId::new("layer"),
                            "Layer",
                            ComponentFieldPath::from_fields(["layer"]),
                            ComponentValueKind::String,
                        )
                        .with_capabilities(ComponentCapability::SCENE_AUTHORING)])
                        .with_authoring_initial_value(ui_template.clone()),
                    ],
                    ..Default::default()
                },
                None,
            ),
        ])
        .unwrap();
        let adapter = nara_scene::__private::AuthoringAdapterReceipt::from_schema_owner(
            &data_bundle.owners()[0],
            &Default::default(),
        )
        .unwrap();
        for contains_ui in [false, true] {
            let mut anchor = SceneEntityRecord::new(SceneEntityId::new("anchor").unwrap());
            if contains_ui {
                anchor.components.insert(
                    ui_component.clone(),
                    SceneComponentRecord::new(ComponentSchemaVersion::ONE, ui_template.clone())
                        .with_owner(data_bundle.owners()[0].owner_id().clone()),
                );
            }
            let source = PrefabDocument::from_ordered_roots([anchor]);
            let path = format!("guard-{contains_ui}.prefab.json");
            let published = nara_scene::PrefabDocumentCandidate::decode_json_bytes(
                source.to_json_string().unwrap().as_bytes(),
            )
            .unwrap()
            .admit_authoring(
                nara_scene::AuthoringDocumentSource::new(
                    nara_asset::AssetRef::path(path.clone()).unwrap(),
                ),
                &data_bundle,
                std::slice::from_ref(&adapter),
            )
            .unwrap()
            .publish();
            let target = editor
                .workspace
                .open_authoring_prefab_document(
                    "Guard",
                    EditorDocumentSourceBinding::new(path),
                    published,
                )
                .unwrap()
                .opened_document
                .unwrap();
            let basis = editor.document_query_basis(target).unwrap();
            let mut draft = editor.begin_authoring_construction(&basis).unwrap();
            let local = draft
                .create_entity(nara_scene::ScenePlacement::last(
                    nara_scene::ScenePlacementOwner::Root,
                ))
                .unwrap();
            if !contains_ui {
                draft.add_component(&local, ui_component.clone()).unwrap();
            }
            assert!(
                matches!(editor.prepare_authoring_construction(&basis, &draft),
                Err(EditorEditPreparationError::StructureContractMismatch { document }) if document == target)
            );
            let bypass = editor
                .workspace
                .prepare_authoring_construction(&basis, &draft)
                .unwrap();
            assert!(matches!(editor.commit_authoring_construction(bypass),
                Err(EditorActionOutcome::Rejected(nara_tooling::EditorActionRejection::StructureContractMismatch { document })) if document == target));
            let slot = editor.workspace.prefab(target).unwrap();
            assert_eq!(slot.session().document(), &source);
            assert_eq!(slot.revision(), basis.revision);
            assert_eq!(slot.session().history_status().undo_depth, 0);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if matches!(
                editor.advance_close(),
                EditorProjectCloseProgress::Complete(_)
            ) {
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::yield_now();
        }
        drop(editor);
        let resolved = root.canonicalize().unwrap();
        assert!(resolved.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            resolved
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("nara-ui-transition-")
        );
        fs::remove_dir_all(resolved).unwrap();
        assert!(
            !accepted,
            "an ordinary Prefab acquired a UI root without its required UiNode"
        );
        assert!(
            unchanged,
            "a rejected UI domain transition must preserve the document and history"
        );
    }

    #[derive(Debug, Component)]
    struct ComponentReadOnlyProbe {
        value: i64,
    }

    #[derive(Debug, Component)]
    struct FieldReadOnlyProbe {
        value: i64,
    }

    #[test]
    fn editor_runtime_bridge_restores_a_pending_edit_after_unwind() {
        let request = EditorRuntimeEditRequest {
            generation: 1,
            document_revision: nara_scene::SceneAuthoringSession::new(
                nara_scene::SceneDocument::default(),
            )
            .revision(),
            entity: SceneEntityId::new("panic-recovery").unwrap(),
            component: ComponentTypeId::new("nara.test.PanicRecovery"),
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("value"),
            value: ComponentValue::I64(1),
        };
        let mut world = World::new();
        world.insert_resource(EditorRuntimeBridge {
            pending: Some(request.clone()),
            result: None,
        });

        let unwind = catch_unwind(AssertUnwindSafe(|| {
            world.resource_scope(|_world, mut bridge: Mut<EditorRuntimeBridge>| {
                let _pending = PendingEditorRuntimeEdit::take(&mut bridge)
                    .expect("test bridge starts with a pending edit");
                panic!("injected runtime edit codec panic");
            });
        }));

        assert!(unwind.is_err());
        let bridge = world.resource::<EditorRuntimeBridge>();
        assert_eq!(bridge.pending.as_ref(), Some(&request));
        assert!(bridge.result.is_none());
    }

    #[test]
    fn runtime_edit_distinguishes_failed_encoding_from_a_missing_component() {
        let mut registry = ComponentRegistry::new();
        let component = ComponentTypeId::new("nara.test.EncodingFailure");
        let schema = ComponentSchema::new(
            component.clone(),
            "Encoding failure",
            ComponentSchemaVersion::ONE,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)
        .with_fields([ComponentFieldSchema::required(
            ComponentFieldId::new("value"),
            "Value",
            ComponentFieldPath::from_fields(["value"]),
            ComponentValueKind::I64,
        )
        .with_capabilities(ComponentCapability::SCENE_AUTHORING)]);
        registry
            .register_persistent_component_with_codec::<FieldReadOnlyProbe, _, _>(
                schema,
                |value| {
                    Ok(FieldReadOnlyProbe {
                        value: value.field_i64("value")?,
                    })
                },
                |_| {
                    Err(nara_reflect::ComponentCodecError::invalid_field(
                        "value",
                        "encodable",
                    ))
                },
            )
            .unwrap();
        registry.freeze().unwrap();
        let execution = registry.snapshot().unwrap();
        let entity_id = SceneEntityId::new("encoding-failure").unwrap();
        let mut world = World::new();
        let entity = world
            .spawn((
                scene_source(entity_id.clone()),
                FieldReadOnlyProbe { value: 7 },
            ))
            .id();
        let request = EditorRuntimeEditRequest {
            generation: 1,
            document_revision: nara_scene::SceneAuthoringSession::new(
                nara_scene::SceneDocument::default(),
            )
            .revision(),
            entity: entity_id,
            component,
            component_version: ComponentSchemaVersion::ONE,
            field: ComponentFieldId::new("value"),
            value: ComponentValue::I64(99),
        };
        assert!(matches!(
            apply_runtime_edit(&mut world, &execution, &request),
            EditorRuntimeEditResult::Rejected {
                reason: EditorRuntimeEditRejection::InvalidValue,
                ..
            }
        ));
        assert_eq!(world.get::<FieldReadOnlyProbe>(entity).unwrap().value, 7);

        world.entity_mut(entity).remove::<FieldReadOnlyProbe>();
        assert!(matches!(
            apply_runtime_edit(&mut world, &execution, &request),
            EditorRuntimeEditResult::Rejected {
                reason: EditorRuntimeEditRejection::MissingComponent,
                ..
            }
        ));
    }

    #[test]
    fn runtime_edit_rejects_components_and_fields_without_edit_capability() {
        let component_read_only = ComponentTypeId::new("nara.test.ComponentReadOnlyProbe");
        let field_read_only = ComponentTypeId::new("nara.test.FieldReadOnlyProbe");
        let mut registry = ComponentRegistry::new();
        register_probe::<ComponentReadOnlyProbe>(
            &mut registry,
            component_read_only.clone(),
            [ComponentCapability::Scene, ComponentCapability::Inspect],
            [ComponentCapability::Scene, ComponentCapability::Inspect],
            |probe| probe.value,
            |value| ComponentReadOnlyProbe { value },
        );
        register_probe::<FieldReadOnlyProbe>(
            &mut registry,
            field_read_only.clone(),
            ComponentCapability::SCENE_AUTHORING,
            [ComponentCapability::Scene, ComponentCapability::Inspect],
            |probe| probe.value,
            |value| FieldReadOnlyProbe { value },
        );
        registry.freeze().unwrap();
        let execution = registry.snapshot().unwrap();
        let revision =
            nara_scene::SceneAuthoringSession::new(nara_scene::SceneDocument::default()).revision();

        let component_entity_id = SceneEntityId::new("component-read-only").unwrap();
        let field_entity_id = SceneEntityId::new("field-read-only").unwrap();
        let mut world = World::new();
        let component_entity = world
            .spawn((
                scene_source(component_entity_id.clone()),
                ComponentReadOnlyProbe { value: 7 },
            ))
            .id();
        let field_entity = world
            .spawn((
                scene_source(field_entity_id.clone()),
                FieldReadOnlyProbe { value: 11 },
            ))
            .id();

        for (entity_id, component) in [
            (component_entity_id, component_read_only),
            (field_entity_id, field_read_only),
        ] {
            let request = EditorRuntimeEditRequest {
                generation: 1,
                document_revision: revision,
                entity: entity_id,
                component,
                component_version: ComponentSchemaVersion::ONE,
                field: ComponentFieldId::new("value"),
                value: ComponentValue::I64(99),
            };
            assert!(matches!(
                apply_runtime_edit(&mut world, &execution, &request),
                EditorRuntimeEditResult::Rejected {
                    reason: EditorRuntimeEditRejection::NotEditable,
                    ..
                }
            ));
        }
        assert_eq!(
            world
                .get::<ComponentReadOnlyProbe>(component_entity)
                .unwrap()
                .value,
            7
        );
        assert_eq!(
            world.get::<FieldReadOnlyProbe>(field_entity).unwrap().value,
            11
        );
    }

    fn register_probe<T: Component>(
        registry: &mut ComponentRegistry,
        id: ComponentTypeId,
        component_capabilities: impl IntoIterator<Item = ComponentCapability>,
        field_capabilities: impl IntoIterator<Item = ComponentCapability>,
        encode: impl Fn(&T) -> i64 + Send + Sync + 'static,
        decode: impl Fn(i64) -> T + Send + Sync + 'static,
    ) {
        let schema = ComponentSchema::new(id, "Runtime edit probe", ComponentSchemaVersion::ONE)
            .with_capabilities(component_capabilities)
            .with_fields([ComponentFieldSchema::required(
                ComponentFieldId::new("value"),
                "Value",
                ComponentFieldPath::from_fields(["value"]),
                ComponentValueKind::I64,
            )
            .with_capabilities(field_capabilities)]);
        registry
            .register_persistent_component_with_codec::<T, _, _>(
                schema,
                move |value| Ok(decode(value.field_i64("value")?)),
                move |component| {
                    Ok(ComponentValue::map([(
                        "value",
                        ComponentValue::I64(encode(component)),
                    )]))
                },
            )
            .unwrap();
    }

    fn scene_source(entity_id: SceneEntityId) -> SceneEntitySource {
        let mut fixture = World::new();
        let mut registry = ComponentRegistry::new();
        registry.freeze().unwrap();
        let candidate = nara_scene::ExpandedSceneCandidate::plain(
            nara_scene::SourceDocumentKey::named(
                "840ad54a-4b14-4f32-954a-e873ff0d6e41".parse().unwrap(),
                "runtime-edit-probe",
                nara_scene::SourceDocumentKind::Scene,
            )
            .unwrap(),
            Arc::new(nara_scene::SceneDocument::from_ordered_roots([
                nara_scene::SceneEntityRecord::new(entity_id.clone()),
            ])),
        );
        let report = nara_scene::spawn_scene(
            &mut fixture,
            &registry,
            &candidate,
            &nara_scene::SceneOccurrenceKey::new("editor-fixture", "runtime-edit-probe").unwrap(),
            None,
        );
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        let nara_identity::EntityLookup::Resolved(entity) =
            report.instance.unwrap().resolve(&fixture, &entity_id)
        else {
            panic!("the fixture publishes its bound source entity");
        };
        fixture.get::<SceneEntitySource>(entity).unwrap().clone()
    }
}
