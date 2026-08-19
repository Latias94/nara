use std::{collections::BTreeMap, fmt, sync::Arc, time::Duration};

mod actions;
#[cfg(any(feature = "presentation-2d", feature = "runtime-ui"))]
mod authoring_preview;
mod content_refresh;
#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
mod game_view;
mod model;
mod open;
mod persistence;
mod play;
#[cfg(feature = "presentation-2d")]
mod preview;
#[cfg(feature = "runtime-ui")]
mod ui_authoring;
#[cfg(feature = "presentation-ui")]
mod ui_preview;

#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
pub use game_view::{
    EditorGameInputBatch, EditorGameInputEnvelope, EditorGameInputEvent, EditorGameStartActivation,
    EditorGameStartRequest, EditorGameViewBridgeFailure, EditorGameViewCandidate,
    EditorGameViewCandidateIdentity, EditorGameViewCandidateParts, EditorGameViewCompletion,
    EditorGameViewCompletionDisposition,
};
pub use open::{
    EditorProjectOpenAttempt, EditorProjectOpenFailure, EditorProjectOpenFailureImpact,
    EditorProjectOpenFailureSource, EditorProjectOpenHost, EditorProjectOpenPhase,
    EditorProjectOpenProgress,
};
#[cfg(feature = "presentation-2d")]
pub use preview::{EditorPreviewCandidate, EditorPreviewHit};
#[cfg(feature = "runtime-ui")]
pub use ui_authoring::{EditorUiAuthoringEditError, EditorUiAuthoringFrame};
#[cfg(feature = "runtime-ui")]
use ui_authoring::{EditorUiAuthoringProjectionCache, EditorUiImageChoices};
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
use nara_reflect::{
    ComponentCapability, ComponentRegistry, component_registry,
    report_component_registry_authority_fault,
};
#[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
use nara_render::Extent2d;
use nara_scene::{SceneEntitySource, UnavailableRecordManifest};
use nara_tooling::{
    __export_apply_changes_from_world, EditorAuthoringReadiness, EditorCloseDecision,
    EditorDocumentId, EditorDocumentKind, EditorFieldFeedback, EditorFrameClock, EditorFrameModel,
    EditorGestureCancelReason, EditorGestureController, EditorPersistenceAuthority,
    EditorPersistenceCommand, EditorPersistenceFailureStage, EditorPersistenceOperation,
    EditorPersistenceRejection, EditorPersistenceRequestKind, EditorPersistenceRequestResult,
    EditorPersistenceResult, EditorPersistenceView, EditorPlayCommand, EditorPlayFailure,
    EditorPlayOperation, EditorPlayOperationResult, EditorPlayRejection, EditorPlayRequestResult,
    EditorPlayState, EditorPlayView, EditorProjectContentGeneration, EditorProjectContentView,
    EditorProjectReadinessView, EditorRuntimeAvailability, EditorRuntimeEditRejection,
    EditorRuntimeEditRequest, EditorRuntimeEditResult, EditorRuntimeUnavailableReason,
    EditorWorkspace, EditorWorkspaceCommand, EditorWorkspaceCommandReport, EditorWorkspaceIntent,
    EditorWorkspaceIntentPhase, EditorWorkspaceIntentRejection, EditorWorkspaceIntentRequestResult,
    EditorWorkspaceIntentResult, EditorWorkspaceIntentView, SceneApplyChangesComponentStatus,
    SceneApplyChangesRequest,
};
use nara_tooling::{EditorApplyChangesRejection, EditorApplyChangesResult};

use crate::ProductRecipe;
use crate::project_host::authoring::{
    AuthoringProjectReadiness, AuthoringProjectReceipt, AuthoringProjectSessionSnapshot,
    first_party_authoring_preview_registry,
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
            .and_then(|registry| registry.snapshot().map_err(|_| ()))
            .map(ComponentRegistry::from_snapshot);
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
    registry: &ComponentRegistry,
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
    let Some(Ok(Some(mut value))) = registry.encode_component(&request.component, world, entity)
    else {
        return EditorRuntimeEditResult::Rejected {
            request: request.clone(),
            reason: EditorRuntimeEditRejection::MissingComponent,
        };
    };
    if value.set_path(field.path(), request.value.clone()).is_err() {
        return EditorRuntimeEditResult::Rejected {
            request: request.clone(),
            reason: EditorRuntimeEditRejection::InvalidValue,
        };
    }
    let Some(Ok(prepared)) = registry.preflight_component(&request.component, &value) else {
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
pub struct EditorProjectOpenError {
    diagnostics: DiagnosticReport,
}

impl EditorProjectOpenError {
    #[must_use]
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    #[must_use]
    pub fn into_diagnostics(self) -> DiagnosticReport {
        self.diagnostics
    }
}

impl fmt::Display for EditorProjectOpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("editor project session could not be opened")
    }
}

impl std::error::Error for EditorProjectOpenError {}

enum PendingPersistence {
    Save(PendingSave),
    Reopen {
        document: EditorDocumentId,
        base_revision: Option<nara_scene::SceneAuthoringRevision>,
    },
}

enum PendingSave {
    Scene(SceneSaveCandidate),
    Prefab(crate::project_host::persistence::PrefabSaveCandidate),
}

#[derive(Clone)]
struct PendingProjectContentRefresh {
    document: EditorDocumentId,
    revision: nara_scene::SceneAuthoringRevision,
    prefab: nara_asset::AssetPath,
    saved_digest: nara_tooling::EditorDocumentDigest,
    expected_digest: ContentDigest,
}

#[derive(Clone, Copy)]
enum PersistenceLane {
    Scene,
    Prefab,
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

enum EditorRuntimeOwnerState {
    Empty,
    Preparing {
        document: EditorDocumentId,
        revision: nara_scene::SceneAuthoringRevision,
        operation: EditorPlayOperation,
    },
    Starting {
        document: EditorDocumentId,
        revision: nara_scene::SceneAuthoringRevision,
        operation: EditorPlayOperation,
        attempt: super::RuntimeStartAttempt,
    },
    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    AwaitingGameTarget {
        document: EditorDocumentId,
        revision: nara_scene::SceneAuthoringRevision,
        operation: EditorPlayOperation,
        request: EditorGameStartRequest,
        prepared: super::PreparedRuntimeStart,
    },
    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    CommittingGameTarget {
        document: EditorDocumentId,
        revision: nara_scene::SceneAuthoringRevision,
        operation: EditorPlayOperation,
        request: EditorGameStartRequest,
        prepared: super::PreparedRuntimeStart,
    },
    CancellingStart {
        terminal: EditorPlayOperationResult,
        attempt: super::RuntimeStartAttempt,
    },
    RetiringPlay {
        terminal: EditorPlayOperationResult,
    },
    Active {
        document: EditorDocumentId,
        source_revision: nara_scene::SceneAuthoringRevision,
        #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
        game: Option<ActiveEditorGameView>,
    },
    RetirementIncomplete {
        terminal: EditorPlayOperationResult,
    },
    Transitioning,
}

impl EditorRuntimeOwnerState {
    fn document(&self) -> Option<EditorDocumentId> {
        match self {
            Self::Preparing { document, .. }
            | Self::Starting { document, .. }
            | Self::Active { document, .. } => Some(*document),
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            Self::AwaitingGameTarget { document, .. }
            | Self::CommittingGameTarget { document, .. } => Some(*document),
            Self::Empty
            | Self::CancellingStart { .. }
            | Self::RetiringPlay { .. }
            | Self::RetirementIncomplete { .. }
            | Self::Transitioning => None,
        }
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
struct PendingDocumentClose {
    document: EditorDocumentId,
    phase: EditorWorkspaceIntentPhase,
}

#[derive(Debug)]
struct PendingExitPreparation {
    documents: Box<[EditorDocumentId]>,
    cursor: usize,
    phase: EditorWorkspaceIntentPhase,
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
            Self::CloseDocument(pending) => pending.phase,
            Self::Exit(pending) => pending.phase,
        }
    }

    fn decision_document(&self) -> Option<EditorDocumentId> {
        match self {
            Self::CloseDocument(pending) => Some(pending.document),
            Self::Exit(pending) => pending.current_document(),
        }
    }

    fn set_phase(&mut self, phase: EditorWorkspaceIntentPhase) {
        match self {
            Self::CloseDocument(pending) => pending.phase = phase,
            Self::Exit(pending) => pending.phase = phase,
        }
    }
}

impl PendingExitPreparation {
    fn new(documents: Vec<EditorDocumentId>) -> Self {
        Self {
            documents: documents.into_boxed_slice(),
            cursor: 0,
            phase: EditorWorkspaceIntentPhase::AwaitingDecision,
        }
    }

    fn current_document(&self) -> Option<EditorDocumentId> {
        self.documents.get(self.cursor).copied()
    }
}

struct PublishedEditorFrame {
    model: Arc<EditorFrameModel>,
    dispatched: bool,
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

struct TrustedCompiledEditorLane {
    plan: RuntimePlan,
    content_candidate: ProjectSettingsCandidate,
    content_loader: ProjectContentLoader,
    snapshot: crate::project_host::ProjectContentSnapshot,
    prefab_root: DirectoryCapability,
    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    game_view_config: Option<game_view::EditorGameViewCaptureConfig>,
    runtime_host: ProjectHost,
    runtime_owner: EditorRuntimeOwnerState,
    pending_control: Option<PendingEditorControl>,
    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
    pending_step_presentation: Option<EditorStepPresentationState>,
    runtime_edit_result: Option<EditorRuntimeEditResult>,
    pending_apply_changes: Option<PendingApplyChanges>,
    apply_changes_result: Option<EditorApplyChangesResult>,
}

/// Concrete product Host for one Editor workspace and its future single Play owner.
///
/// Product Adapters consume [`Self::current_frame`], return one ordered action batch through
/// [`Self::dispatch_editor_actions`], and publish the next frame through
/// [`Self::drive_editor_frame`]. Crate-private helpers remain owner-conformance seams only.
pub struct EditorProjectSession {
    workspace: EditorWorkspace,
    persistence_authority: EditorPersistenceAuthority,
    scene_title: String,
    source_document: EditorDocumentId,
    execution: EditorExecutionLane,
    project_content: EditorProjectContentView,
    next_project_content_generation: u64,
    pending_content_refresh: Option<PendingProjectContentRefresh>,
    failed_content_refresh: Option<PendingProjectContentRefresh>,
    persistence: ScenePersistenceHost,
    prefab_persistence: Option<PrefabPersistenceHost>,
    authoring_prefab_source_observations:
        BTreeMap<nara_asset::AssetPath, AuthoringPrefabSourceObservation>,
    #[cfg(test)]
    reject_next_prefab_source_binding: bool,
    persistence_operation: EditorPersistenceOperation,
    pending_persistence: Option<PendingPersistence>,
    persistence_result: Option<EditorPersistenceResult>,
    last_persistence_receipt: Option<EditorPersistenceReceipt>,
    diagnostics: DiagnosticReport,
    field_feedback: Option<EditorFieldFeedback>,
    play_result: Option<EditorPlayOperationResult>,
    pending_workspace_intent: Option<PendingWorkspaceIntent>,
    workspace_intent_result: Option<EditorWorkspaceIntentResult>,
    frame_clock: EditorFrameClock,
    published_frame: Option<PublishedEditorFrame>,
    gestures: EditorGestureController,
    last_gesture_cancellation: Option<EditorGestureCancelReason>,
    #[cfg(feature = "presentation-2d")]
    preview: preview::EditorPreviewSession,
    #[cfg(feature = "runtime-ui")]
    ui_authoring_image_choices: EditorUiImageChoices,
    #[cfg(feature = "presentation-ui")]
    ui_preview: ui_preview::EditorUiPreviewSession,
    #[cfg(feature = "runtime-ui")]
    ui_authoring_frame: Option<Arc<EditorUiAuthoringFrame>>,
    #[cfg(feature = "runtime-ui")]
    ui_authoring_projection_cache: Option<EditorUiAuthoringProjectionCache>,
}

impl fmt::Debug for EditorProjectSession {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EditorProjectSession")
            .field("workspace_documents", &self.workspace.len())
            .field("persistence_operation", &self.persistence_operation)
            .field("persistence_result", &self.persistence_result)
            .field("play_state", &self.play_view().state())
            .finish_non_exhaustive()
    }
}

impl EditorProjectSession {
    #[must_use]
    pub const fn project_readiness(&self) -> EditorProjectReadinessView {
        match &self.execution {
            EditorExecutionLane::Authoring(snapshot) => EditorProjectReadinessView::new(
                match snapshot.readiness() {
                    AuthoringProjectReadiness::Complete => EditorAuthoringReadiness::Complete,
                    AuthoringProjectReadiness::Degraded => EditorAuthoringReadiness::Degraded,
                },
                EditorRuntimeAvailability::Unavailable(
                    EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
                ),
            ),
            EditorExecutionLane::TrustedCompiled(_) => EditorProjectReadinessView::new(
                EditorAuthoringReadiness::Complete,
                EditorRuntimeAvailability::TrustedInProcess,
            ),
        }
    }

    fn authoring_project(&self) -> Option<&AuthoringProjectSessionSnapshot> {
        match &self.execution {
            EditorExecutionLane::Authoring(snapshot) => Some(snapshot),
            EditorExecutionLane::TrustedCompiled(_) => None,
        }
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
        Some(std::mem::replace(
            &mut lane.runtime_owner,
            EditorRuntimeOwnerState::Transitioning,
        ))
    }

    fn install_trusted_runtime_owner(&mut self, state: EditorRuntimeOwnerState) -> bool {
        let Some(lane) = self.trusted_compiled_mut() else {
            return false;
        };
        lane.runtime_owner = state;
        true
    }

    pub(crate) fn open(receipt: AuthoringProjectReceipt) -> Result<Self, EditorProjectOpenError> {
        let parts = receipt.into_session_parts().map_err(|_| {
            editor_open_error("project.editor.authoring-receipt-consumption-failed")
        })?;
        #[cfg(feature = "presentation-2d")]
        let preview_registry = first_party_authoring_preview_registry(built_in_schema_providers())
            .map_err(|_| editor_open_error("project.editor.authoring-preview-registry-invalid"))?;
        #[cfg(feature = "presentation-2d")]
        let preview =
            preview::EditorPreviewSession::new_authoring(&parts.snapshot, &preview_registry)
                .map_err(|diagnostics| EditorProjectOpenError { diagnostics })?;
        #[cfg(feature = "runtime-ui")]
        let ui_authoring_image_choices =
            EditorUiImageChoices::from_authoring_snapshot(&parts.snapshot);
        #[cfg(feature = "presentation-ui")]
        let ui_preview = ui_preview::EditorUiPreviewSession::new_authoring().map_err(|_error| {
            editor_open_error("project.editor.authoring-ui-preview-init-failed")
        })?;
        let project_root = parts.snapshot.project_root();
        let settings = parts.snapshot.settings();
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
        let startup_source = parts.snapshot.startup_source();
        let (persistence, opened_digest) = ScenePersistenceHost::bind_authoring_source(
            &scenes,
            startup.as_str(),
            startup_source.identity(),
            startup_source.digest(),
        )
        .map_err(editor_persistence_open_error)?;
        let diagnostics = parts.startup_scene.diagnostics().clone();
        let (mut workspace, mut persistence_authority) = EditorWorkspace::new_hosted();
        let report = workspace
            .open_authoring_scene_document(
                startup_title.clone(),
                nara_tooling::EditorDocumentSourceBinding::new(startup_title.clone()),
                parts.startup_scene,
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
            workspace,
            persistence_authority,
            scene_title: startup_title,
            source_document: document,
            execution: EditorExecutionLane::Authoring(parts.snapshot),
            project_content: EditorProjectContentView::NotInstalled,
            next_project_content_generation: 1,
            pending_content_refresh: None,
            failed_content_refresh: None,
            persistence,
            prefab_persistence: None,
            authoring_prefab_source_observations: BTreeMap::new(),
            #[cfg(test)]
            reject_next_prefab_source_binding: false,
            persistence_operation: EditorPersistenceOperation::Idle,
            pending_persistence: None,
            persistence_result: None,
            last_persistence_receipt: None,
            diagnostics,
            field_feedback: None,
            play_result: None,
            pending_workspace_intent: None,
            workspace_intent_result: None,
            frame_clock: EditorFrameClock::new(),
            published_frame: None,
            gestures: EditorGestureController::default(),
            last_gesture_cancellation: None,
            #[cfg(feature = "presentation-2d")]
            preview,
            #[cfg(feature = "runtime-ui")]
            ui_authoring_image_choices,
            #[cfg(feature = "presentation-ui")]
            ui_preview,
            #[cfg(feature = "runtime-ui")]
            ui_authoring_frame: None,
            #[cfg(feature = "runtime-ui")]
            ui_authoring_projection_cache: None,
        };
        session.publish_editor_frame();
        Ok(session)
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
        let candidate =
            ingest_project_manifest(&manifest, profile.as_deref()).map_err(|error| {
                EditorProjectOpenError {
                    diagnostics: error.diagnostics().clone(),
                }
            })?;
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
        let plan = resolve_project_runtime_request(&candidate, request).map_err(|error| {
            EditorProjectOpenError {
                diagnostics: runtime_plan_failure_report(&error),
            }
        })?;
        let runtime_agreement = prepare_runtime_schema_agreement(&project_root, &candidate, &plan)
            .map_err(|error| EditorProjectOpenError {
                diagnostics: error.diagnostics().clone(),
            })?;

        let prefabs_path = RelativePath::new(candidate.settings().paths.prefabs.as_str())
            .map_err(|_| editor_open_error("project.editor.prefabs-path-invalid"))?;
        let prefab_root = project_root
            .open_directory(&prefabs_path)
            .map_err(|_| editor_open_error("project.editor.prefabs-open-failed"))?;

        let loader = ProjectContentLoader::new(project_root.share()).map_err(|error| {
            EditorProjectOpenError {
                diagnostics: error.diagnostics().clone(),
            }
        })?;
        let snapshot = loader
            .load_with_runtime_agreement(&candidate, &plan, runtime_agreement)
            .map_err(|error| EditorProjectOpenError {
                diagnostics: error.diagnostics().clone(),
            })?;
        let initial_content_generation = EditorProjectContentGeneration::new(1);
        #[cfg(feature = "runtime-ui")]
        let ui_authoring_image_choices =
            EditorUiImageChoices::from_snapshot(&snapshot, initial_content_generation);
        #[cfg(feature = "presentation-ui")]
        let ui_preview = ui_preview::EditorUiPreviewSession::new(&snapshot).map_err(|_error| {
            EditorProjectOpenError {
                diagnostics: single_error(
                    "project.editor.ui-preview-init-failed",
                    "the Editor UI preview owner could not be initialized",
                ),
            }
        })?;
        #[cfg(feature = "presentation-2d")]
        let preview = preview::EditorPreviewSession::new(
            snapshot.clone(),
            plan.schema_validation().snapshot(),
        )
        .map_err(|diagnostics| EditorProjectOpenError { diagnostics })?;
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
        let opened = ScenePersistenceHost::open(
            &scenes,
            startup.as_str(),
            plan.schema_validation().registry(),
        )
        .map_err(editor_persistence_open_error)?;

        let (mut workspace, mut persistence_authority) = EditorWorkspace::new_hosted();
        let report = workspace
            .open_scene_session(startup.as_str(), opened.session)
            .map_err(|error| EditorProjectOpenError {
                diagnostics: error.report().diagnostics.clone(),
            })?;
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
            workspace,
            persistence_authority,
            scene_title: startup.as_str().to_owned(),
            source_document: document,
            execution: EditorExecutionLane::TrustedCompiled(Box::new(TrustedCompiledEditorLane {
                plan,
                content_candidate: candidate,
                content_loader: loader,
                snapshot,
                prefab_root,
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                game_view_config,
                runtime_host: ProjectHost::new(cleanup_policy),
                runtime_owner: EditorRuntimeOwnerState::Empty,
                pending_control: None,
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                pending_step_presentation: None,
                runtime_edit_result: None,
                pending_apply_changes: None,
                apply_changes_result: None,
            })),
            project_content: EditorProjectContentView::Current {
                generation: initial_content_generation,
            },
            next_project_content_generation: 2,
            pending_content_refresh: None,
            failed_content_refresh: None,
            persistence: opened.host,
            prefab_persistence: None,
            authoring_prefab_source_observations: BTreeMap::new(),
            #[cfg(test)]
            reject_next_prefab_source_binding: false,
            persistence_operation: EditorPersistenceOperation::Idle,
            pending_persistence: None,
            persistence_result: None,
            last_persistence_receipt: None,
            diagnostics,
            field_feedback: None,
            play_result: None,
            pending_workspace_intent: None,
            workspace_intent_result: None,
            frame_clock: EditorFrameClock::new(),
            published_frame: None,
            gestures: EditorGestureController::default(),
            last_gesture_cancellation: None,
            #[cfg(feature = "presentation-2d")]
            preview,
            #[cfg(feature = "runtime-ui")]
            ui_authoring_image_choices,
            #[cfg(feature = "presentation-ui")]
            ui_preview,
            #[cfg(feature = "runtime-ui")]
            ui_authoring_frame: None,
            #[cfg(feature = "runtime-ui")]
            ui_authoring_projection_cache: None,
        };
        session.publish_editor_frame();
        Ok(session)
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
        self.preview.set_extent(extent);
    }

    #[cfg(feature = "presentation-2d")]
    #[doc(hidden)]
    #[must_use]
    pub fn take_editor_preview_candidate(&mut self) -> Option<EditorPreviewCandidate> {
        self.preview.take_candidate()
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
        let EditorExecutionLane::TrustedCompiled(lane) = &mut self.execution else {
            return Err(EditorGameViewBridgeFailure::RuntimeUnavailable);
        };
        let state = std::mem::replace(
            &mut lane.runtime_owner,
            EditorRuntimeOwnerState::Transitioning,
        );
        match state {
            EditorRuntimeOwnerState::AwaitingGameTarget {
                document,
                revision,
                operation,
                request,
                prepared,
            } if request == expected => {
                lane.runtime_owner = EditorRuntimeOwnerState::CommittingGameTarget {
                    document,
                    revision,
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
        let state = std::mem::replace(
            &mut lane.runtime_owner,
            EditorRuntimeOwnerState::Transitioning,
        );
        let EditorRuntimeOwnerState::CommittingGameTarget {
            document,
            revision,
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
            document,
            source_revision: revision,
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
        let state = std::mem::replace(
            &mut lane.runtime_owner,
            EditorRuntimeOwnerState::Transitioning,
        );
        match state {
            EditorRuntimeOwnerState::AwaitingGameTarget {
                operation,
                request,
                prepared,
                ..
            } if request == expected => {
                lane.runtime_host.abort_prepared_start(prepared);
                lane.pending_step_presentation = None;
                self.play_result = Some(EditorPlayOperationResult::Pending { operation });
                lane.runtime_owner = EditorRuntimeOwnerState::RetiringPlay {
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

        if let Some(candidate) = self.take_game_view_candidate()? {
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
        let Some(lane) = self.trusted_compiled_mut() else {
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
        let authoring = self.authoring_project().is_some();
        let mut projection_rejected = false;
        let mut projection_omitted = false;
        let source_identity = self
            .workspace
            .scene(self.source_document)
            .map(|slot| (self.source_document, slot.revision()));
        let requires_projection = source_identity.is_some_and(|(document, revision)| {
            self.preview.requires_source_projection(document, revision)
        });
        let source = self.workspace.scene(self.source_document).and_then(|slot| {
            let scene = if requires_projection {
                let (scene, omissions) = if authoring {
                    match authoring_preview::project_scene(
                        self.source_document,
                        slot.revision(),
                        slot.session(),
                    ) {
                        Ok((scene, omissions)) => (scene, omissions),
                        Err(_) => {
                            projection_rejected = true;
                            return None;
                        }
                    }
                } else {
                    (
                        slot.session().document().clone(),
                        authoring_preview::EditorAuthoringPreviewOmissions::empty(
                            self.source_document,
                            slot.revision(),
                        ),
                    )
                };
                projection_omitted = !omissions.is_empty();
                Some((scene, omissions))
            } else {
                None
            };
            Some(preview::EditorPreviewSource {
                document: self.source_document,
                revision: slot.revision(),
                projection: scene.map(|(scene, omissions)| {
                    preview::EditorPreviewProjection::new(scene, omissions)
                }),
            })
        });
        if projection_rejected {
            let _ = self.diagnostics.extend(single_error(
                "project.editor.authoring-scene-preview-projection-rejected",
                "the authoring Scene could not publish a safe preview projection",
            ));
        } else if projection_omitted {
            let _ = self.diagnostics.extend(single_warning(
                "project.editor.authoring-scene-preview-source-omitted",
                "the Scene preview omitted source values that are unavailable to generic authoring",
            ));
        }
        let report = self.preview.drive(source);
        if let Some(report) = report {
            let _ = self.diagnostics.extend(report);
        }
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
        if matches!(command, EditorWorkspaceCommand::CloseDocument { .. }) {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.close-requires-workspace-intent",
            );
        }
        if self.pending_workspace_intent.is_some() {
            return editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.workspace-intent-pending",
            );
        }
        match &mut self.execution {
            EditorExecutionLane::Authoring(_) => self.workspace.apply_authoring_command(command),
            EditorExecutionLane::TrustedCompiled(lane) => self
                .workspace
                .apply_command(lane.plan.schema_validation().registry(), command),
        }
    }

    pub(crate) fn request_workspace_intent(
        &mut self,
        intent: EditorWorkspaceIntent,
    ) -> EditorWorkspaceIntentRequestResult {
        if self.pending_workspace_intent.is_some() || self.pending_persistence.is_some() {
            return EditorWorkspaceIntentRequestResult::Rejected(
                EditorWorkspaceIntentRejection::Busy,
            );
        }
        if self.workspace_intent_result.is_some() {
            return EditorWorkspaceIntentRequestResult::Rejected(
                EditorWorkspaceIntentRejection::ResultPending,
            );
        }

        let pending = match intent {
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
                    phase: if dirty {
                        EditorWorkspaceIntentPhase::AwaitingDecision
                    } else {
                        EditorWorkspaceIntentPhase::RetiringRuntime
                    },
                })
            }
            EditorWorkspaceIntent::Exit => {
                let documents = self.workspace.__document_ids();
                if documents.is_empty() {
                    self.workspace_intent_result =
                        Some(EditorWorkspaceIntentResult::Applied { intent });
                    return EditorWorkspaceIntentRequestResult::Accepted;
                }
                let mut pending = PendingExitPreparation::new(documents);
                self.prepare_current_exit_document(&mut pending);
                PendingWorkspaceIntent::Exit(pending)
            }
        };
        let should_retire = pending.phase() == EditorWorkspaceIntentPhase::RetiringRuntime;
        self.pending_workspace_intent = Some(pending);
        if should_retire {
            self.advance_workspace_retirement();
        }
        EditorWorkspaceIntentRequestResult::Accepted
    }

    pub(crate) fn resolve_workspace_intent(
        &mut self,
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
        let intent = pending.intent();
        let Some(document) = pending.decision_document() else {
            self.pending_workspace_intent = Some(pending);
            return EditorWorkspaceIntentRequestResult::Rejected(
                EditorWorkspaceIntentRejection::DecisionNotRequired,
            );
        };

        match decision {
            EditorCloseDecision::Cancel => {
                self.workspace_intent_result =
                    Some(EditorWorkspaceIntentResult::Cancelled { intent });
            }
            EditorCloseDecision::Discard => {
                match &mut pending {
                    PendingWorkspaceIntent::CloseDocument(close) => {
                        close.phase = EditorWorkspaceIntentPhase::RetiringRuntime;
                    }
                    PendingWorkspaceIntent::Exit(exit) => {
                        exit.cursor = exit.cursor.saturating_add(1);
                        self.prepare_current_exit_document(exit);
                    }
                }
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
                let EditorPersistenceRequestResult::Accepted = requested else {
                    let EditorPersistenceRequestResult::Rejected(reason) = requested else {
                        unreachable!()
                    };
                    self.pending_workspace_intent = Some(pending);
                    return EditorWorkspaceIntentRequestResult::Rejected(
                        EditorWorkspaceIntentRejection::PersistenceRejected(reason),
                    );
                };
                pending.set_phase(EditorWorkspaceIntentPhase::Saving);
                self.pending_workspace_intent = Some(pending);
            }
        }
        EditorWorkspaceIntentRequestResult::Accepted
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
                if exit.phase == EditorWorkspaceIntentPhase::Prepared
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
                if exit.phase == EditorWorkspaceIntentPhase::Prepared
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
        if self.pending_workspace_intent.is_some() {
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
                let EditorRuntimeOwnerState::Active { document, .. } = &lane.runtime_owner else {
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
                (*document, generation)
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
        let runtime = lane
            .runtime_host
            .running_runtime_mut()
            .ok_or(EditorRuntimeEditRejection::InvalidRuntimeState)?;
        let submitted = runtime
            .with_driver_scope(|scope| {
                scope.__apply_port::<EditorRuntimeBridge>(EditorRuntimeBridgeInput::Submit(
                    request.clone(),
                ))
            })
            .map_err(|_| EditorRuntimeEditRejection::InvalidRuntimeState)?
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
        if self.pending_workspace_intent.is_some() {
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
                let EditorRuntimeOwnerState::Active {
                    document,
                    source_revision,
                    ..
                } = &lane.runtime_owner
                else {
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
                (*document, *source_revision, generation)
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
                .document()
                .is_some_and(|document| document != pending_document)
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
                operation, attempt, ..
            } => {
                self.cancel_pending_runtime_edit();
                self.cancel_pending_apply_changes();
                let _ =
                    self.install_trusted_runtime_owner(EditorRuntimeOwnerState::CancellingStart {
                        terminal: EditorPlayOperationResult::Cancelled { operation },
                        attempt,
                    });
            }
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            EditorRuntimeOwnerState::AwaitingGameTarget {
                operation,
                prepared,
                ..
            } => {
                if let Some(lane) = self.trusted_compiled_mut() {
                    lane.runtime_host.abort_prepared_start(prepared);
                }
                let _ = self.install_trusted_runtime_owner(EditorRuntimeOwnerState::RetiringPlay {
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
            | EditorRuntimeOwnerState::RetirementIncomplete { .. }
            | EditorRuntimeOwnerState::Transitioning) => {
                let _ = self.install_trusted_runtime_owner(state);
            }
        }
    }

    fn finish_workspace_intent_document(&mut self) {
        let Some(pending) = self.pending_workspace_intent.take() else {
            return;
        };
        match pending {
            PendingWorkspaceIntent::CloseDocument(pending) => {
                let closing_prefab = self.workspace.document_kind(pending.document)
                    == Some(EditorDocumentKind::Prefab);
                let report = self.workspace.__discard_document(pending.document);
                if report.applied && closing_prefab {
                    self.prefab_persistence = None;
                }
                let _ = self.diagnostics.extend(report.diagnostics);
                self.workspace_intent_result = Some(if report.applied {
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
                pending.phase = EditorWorkspaceIntentPhase::Prepared;
                self.pending_workspace_intent = Some(PendingWorkspaceIntent::Exit(pending));
            }
        }
    }

    fn prepare_current_exit_document(&self, pending: &mut PendingExitPreparation) {
        while let Some(document) = pending.current_document() {
            if self.workspace.document_is_dirty(document).unwrap_or(false) {
                pending.phase = EditorWorkspaceIntentPhase::AwaitingDecision;
                return;
            }
            pending.cursor = pending.cursor.saturating_add(1);
        }
        pending.phase = EditorWorkspaceIntentPhase::RetiringRuntime;
    }

    fn request_start(&mut self, operation: EditorPlayOperation) -> EditorPlayRequestResult {
        let Some(lane) = self.trusted_compiled() else {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeUnavailable(
                EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
            ));
        };
        if !matches!(&lane.runtime_owner, EditorRuntimeOwnerState::Empty)
            || lane.pending_control.is_some()
        {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
        }
        let Some(document) = self.workspace.active_document() else {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation,
                reason: EditorPlayRejection::NoActiveDocument,
            });
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::NoActiveDocument);
        };
        let Some(slot) = self.workspace.scene(document) else {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation,
                reason: EditorPlayRejection::NoActiveDocument,
            });
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::NoActiveDocument);
        };
        let revision = slot.revision();
        let Some(lane) = self.trusted_compiled_mut() else {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeUnavailable(
                EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
            ));
        };
        lane.runtime_owner = EditorRuntimeOwnerState::Preparing {
            document,
            revision,
            operation,
        };
        self.play_result = Some(EditorPlayOperationResult::Pending { operation });
        EditorPlayRequestResult::Accepted
    }

    fn request_start_cancel(&mut self) -> EditorPlayRequestResult {
        let Some(state) = self.take_trusted_runtime_owner() else {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeUnavailable(
                EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
            ));
        };
        match state {
            EditorRuntimeOwnerState::Preparing { operation, .. } => {
                let _ = self.install_trusted_runtime_owner(EditorRuntimeOwnerState::Empty);
                self.play_result = Some(EditorPlayOperationResult::Cancelled { operation });
                EditorPlayRequestResult::Accepted
            }
            EditorRuntimeOwnerState::Starting {
                operation, attempt, ..
            } => {
                let _ =
                    self.install_trusted_runtime_owner(EditorRuntimeOwnerState::CancellingStart {
                        terminal: EditorPlayOperationResult::Cancelled { operation },
                        attempt,
                    });
                self.play_result = Some(EditorPlayOperationResult::Pending {
                    operation: EditorPlayOperation::Cancel,
                });
                EditorPlayRequestResult::Accepted
            }
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            EditorRuntimeOwnerState::AwaitingGameTarget {
                operation,
                prepared,
                ..
            } => {
                if let Some(lane) = self.trusted_compiled_mut() {
                    lane.runtime_host.abort_prepared_start(prepared);
                }
                let _ = self.install_trusted_runtime_owner(EditorRuntimeOwnerState::RetiringPlay {
                    terminal: EditorPlayOperationResult::Cancelled { operation },
                });
                self.play_result = Some(EditorPlayOperationResult::Pending {
                    operation: EditorPlayOperation::Cancel,
                });
                EditorPlayRequestResult::Accepted
            }
            other => {
                let _ = self.install_trusted_runtime_owner(other);
                EditorPlayRequestResult::Rejected(EditorPlayRejection::InvalidState)
            }
        }
    }

    fn request_active_control(
        &mut self,
        operation: EditorPlayOperation,
        control: RuntimeControl,
    ) -> EditorPlayRequestResult {
        let Some(lane) = self.trusted_compiled() else {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeUnavailable(
                EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
            ));
        };
        if !matches!(&lane.runtime_owner, EditorRuntimeOwnerState::Active { .. }) {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::InvalidState);
        }
        let retires_runtime = matches!(
            operation,
            EditorPlayOperation::Stop | EditorPlayOperation::Restart
        );
        if lane.pending_apply_changes.is_some() && !retires_runtime {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
        }
        if lane.pending_control.is_some() {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
        }
        #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
        let mut game_capture_suspended = false;
        #[cfg(not(any(feature = "presentation-2d", feature = "presentation-ui")))]
        let game_capture_suspended = false;
        if retires_runtime {
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            {
                if let Some(lane) = self.trusted_compiled_mut() {
                    lane.pending_step_presentation = None;
                }
            }
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            if matches!(
                self.trusted_compiled().map(|lane| &lane.runtime_owner),
                Some(EditorRuntimeOwnerState::Active { game: Some(_), .. })
            ) {
                if self
                    .set_game_view_source_active(false, "the Play Runtime is retiring")
                    .is_err()
                {
                    return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
                }
                game_capture_suspended = true;
            }
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            if self.game_view_retirement_blocked() {
                if game_capture_suspended {
                    let _ = self.set_game_view_source_active(
                        true,
                        "the blocked Play retirement did not supersede Game View",
                    );
                }
                return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
            }
            self.cancel_pending_runtime_edit();
            self.cancel_pending_apply_changes();
        }
        let requested = match self.trusted_compiled_mut() {
            Some(lane) => lane.runtime_host.request_runtime_control(control),
            None => {
                return EditorPlayRequestResult::Rejected(EditorPlayRejection::RuntimeUnavailable(
                    EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
                ));
            }
        };
        match requested {
            Ok(ticket) => {
                if let Some(lane) = self.trusted_compiled_mut() {
                    lane.pending_control = Some(PendingEditorControl { operation, ticket });
                }
                self.play_result = Some(EditorPlayOperationResult::Pending { operation });
                EditorPlayRequestResult::Accepted
            }
            Err(fault) => {
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                if game_capture_suspended {
                    let _ = self.set_game_view_source_active(
                        true,
                        "the rejected Play retirement did not supersede Game View",
                    );
                }
                #[cfg(not(any(feature = "presentation-2d", feature = "presentation-ui")))]
                let _ = game_capture_suspended;
                let _ = self.diagnostics.extend(fault.diagnostics);
                self.play_result = Some(EditorPlayOperationResult::Rejected {
                    operation,
                    reason: EditorPlayRejection::InvalidState,
                });
                EditorPlayRequestResult::Rejected(EditorPlayRejection::InvalidState)
            }
        }
    }

    fn drive_play_owner(&mut self, real_delta: Duration) {
        let Some(state) = self.take_trusted_runtime_owner() else {
            return;
        };
        let next = match state {
            EditorRuntimeOwnerState::Empty => EditorRuntimeOwnerState::Empty,
            EditorRuntimeOwnerState::Preparing {
                document,
                revision,
                operation,
            } => self.drive_preparing(document, revision, operation),
            EditorRuntimeOwnerState::Starting {
                document,
                revision,
                operation,
                attempt,
            } => self.drive_starting(document, revision, operation, attempt),
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            EditorRuntimeOwnerState::AwaitingGameTarget {
                document,
                revision,
                operation,
                request,
                prepared,
            } => EditorRuntimeOwnerState::AwaitingGameTarget {
                document,
                revision,
                operation,
                request,
                prepared,
            },
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            EditorRuntimeOwnerState::CommittingGameTarget {
                document,
                revision,
                operation,
                request,
                prepared,
            } => EditorRuntimeOwnerState::CommittingGameTarget {
                document,
                revision,
                operation,
                request,
                prepared,
            },
            EditorRuntimeOwnerState::CancellingStart { terminal, attempt } => {
                self.drive_cancelled_start(terminal, attempt)
            }
            EditorRuntimeOwnerState::RetiringPlay { terminal } => {
                self.drive_start_retirement(terminal)
            }
            EditorRuntimeOwnerState::RetirementIncomplete { terminal } => {
                EditorRuntimeOwnerState::RetirementIncomplete { terminal }
            }
            EditorRuntimeOwnerState::Active {
                document,
                source_revision,
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                game,
            } => self.drive_active(
                document,
                source_revision,
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                game,
                real_delta,
            ),
            EditorRuntimeOwnerState::Transitioning => EditorRuntimeOwnerState::Transitioning,
        };
        let _ = self.install_trusted_runtime_owner(next);
    }

    fn drive_preparing(
        &mut self,
        document: EditorDocumentId,
        requested_revision: nara_scene::SceneAuthoringRevision,
        operation: EditorPlayOperation,
    ) -> EditorRuntimeOwnerState {
        if !self.play_source_is_current() {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation,
                reason: EditorPlayRejection::RuntimeOutOfDate,
            });
            return EditorRuntimeOwnerState::Empty;
        }
        let Some(slot) = self.workspace.scene(document) else {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation,
                reason: EditorPlayRejection::NoActiveDocument,
            });
            return EditorRuntimeOwnerState::Empty;
        };
        let revision = slot.revision();
        if revision != requested_revision {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation,
                reason: EditorPlayRejection::StaleDocument,
            });
            return EditorRuntimeOwnerState::Empty;
        }
        let expanded = match self.trusted_compiled().map(|lane| {
            lane.snapshot.prepare_editor_startup_scene(
                slot.session().document(),
                lane.plan.schema_validation().registry(),
            )
        }) {
            None => {
                self.play_result = Some(EditorPlayOperationResult::Rejected {
                    operation,
                    reason: EditorPlayRejection::RuntimeUnavailable(
                        EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
                    ),
                });
                return EditorRuntimeOwnerState::Empty;
            }
            Some(Ok(expanded)) => expanded,
            Some(Err(error)) => {
                let _ = self.diagnostics.extend(error.diagnostics().clone());
                self.play_result = Some(EditorPlayOperationResult::Failed {
                    operation,
                    failure: EditorPlayFailure::Preparation,
                });
                return EditorRuntimeOwnerState::Empty;
            }
        };
        let started = match self.trusted_compiled_mut() {
            Some(lane) => lane.runtime_host.begin_editor_start(
                lane.snapshot.clone(),
                lane.plan.clone(),
                expanded,
                Vec::new(),
            ),
            None => {
                self.play_result = Some(EditorPlayOperationResult::Rejected {
                    operation,
                    reason: EditorPlayRejection::RuntimeUnavailable(
                        EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
                    ),
                });
                return EditorRuntimeOwnerState::Empty;
            }
        };
        match started {
            Ok(attempt) => EditorRuntimeOwnerState::Starting {
                document,
                revision,
                operation,
                attempt,
            },
            Err(fault) => {
                let _ = self.diagnostics.extend(fault.diagnostics);
                self.play_result = Some(EditorPlayOperationResult::Failed {
                    operation,
                    failure: EditorPlayFailure::Preparation,
                });
                EditorRuntimeOwnerState::Empty
            }
        }
    }

    fn drive_starting(
        &mut self,
        document: EditorDocumentId,
        revision: nara_scene::SceneAuthoringRevision,
        operation: EditorPlayOperation,
        mut attempt: super::RuntimeStartAttempt,
    ) -> EditorRuntimeOwnerState {
        let prepared = match self.trusted_compiled_mut() {
            Some(lane) => lane.runtime_host.prepare_start(&mut attempt),
            None => {
                self.play_result = Some(EditorPlayOperationResult::Rejected {
                    operation,
                    reason: EditorPlayRejection::RuntimeUnavailable(
                        EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
                    ),
                });
                return EditorRuntimeOwnerState::Empty;
            }
        };
        match prepared {
            Ok(prepared) => {
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                if let Some(config) = self
                    .trusted_compiled()
                    .and_then(|lane| lane.game_view_config)
                {
                    let request = game_view::start_request(config, document, revision, &prepared);
                    return EditorRuntimeOwnerState::AwaitingGameTarget {
                        document,
                        revision,
                        operation,
                        request,
                        prepared,
                    };
                }

                let (diagnostics, generation) = match self.trusted_compiled_mut() {
                    Some(lane) => {
                        let diagnostics = lane.runtime_host.commit_prepared_start(prepared);
                        let generation = lane.runtime_host.running_runtime_generation();
                        (diagnostics, generation)
                    }
                    None => {
                        self.play_result = Some(EditorPlayOperationResult::Rejected {
                            operation,
                            reason: EditorPlayRejection::RuntimeUnavailable(
                                EditorRuntimeUnavailableReason::ExternalExecutionHostNotImplemented,
                            ),
                        });
                        return EditorRuntimeOwnerState::Empty;
                    }
                };
                let _ = self.diagnostics.extend(diagnostics);
                self.play_result = Some(EditorPlayOperationResult::Applied {
                    operation,
                    generation,
                });
                EditorRuntimeOwnerState::Active {
                    document,
                    source_revision: revision,
                    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                    game: None,
                }
            }
            Err(fault) => {
                let _ = self.diagnostics.extend(fault.diagnostics);
                let terminal = EditorPlayOperationResult::Failed {
                    operation,
                    failure: EditorPlayFailure::Start,
                };
                if self
                    .trusted_compiled()
                    .is_some_and(|lane| lane.runtime_host.has_cleanup_owner())
                {
                    self.play_result = Some(EditorPlayOperationResult::Pending { operation });
                    EditorRuntimeOwnerState::RetiringPlay { terminal }
                } else {
                    self.play_result = Some(terminal);
                    EditorRuntimeOwnerState::Empty
                }
            }
        }
    }

    fn drive_start_retirement(
        &mut self,
        terminal: EditorPlayOperationResult,
    ) -> EditorRuntimeOwnerState {
        let Some(outcome) = self
            .trusted_compiled_mut()
            .map(|lane| lane.runtime_host.drive_cleanup_once())
        else {
            return EditorRuntimeOwnerState::Empty;
        };
        match outcome {
            super::CleanupDriveOutcome::Complete {
                failed,
                diagnostics,
            } => {
                let _ = self.diagnostics.extend(diagnostics);
                let cleanup_replaces_terminal =
                    failed && !matches!(terminal, EditorPlayOperationResult::Failed { .. });
                self.play_result = Some(if cleanup_replaces_terminal {
                    EditorPlayOperationResult::Failed {
                        operation: EditorPlayOperation::RetryRetirement,
                        failure: EditorPlayFailure::Retirement,
                    }
                } else {
                    terminal
                });
                EditorRuntimeOwnerState::Empty
            }
            super::CleanupDriveOutcome::Retiring => {
                EditorRuntimeOwnerState::RetiringPlay { terminal }
            }
            super::CleanupDriveOutcome::RetirementIncomplete => {
                EditorRuntimeOwnerState::RetirementIncomplete { terminal }
            }
        }
    }

    fn drive_cancelled_start(
        &mut self,
        terminal: EditorPlayOperationResult,
        attempt: super::RuntimeStartAttempt,
    ) -> EditorRuntimeOwnerState {
        drop(attempt);
        self.play_result = Some(terminal);
        EditorRuntimeOwnerState::Empty
    }

    fn drive_active(
        &mut self,
        document: EditorDocumentId,
        source_revision: nara_scene::SceneAuthoringRevision,
        #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))] game: Option<
            ActiveEditorGameView,
        >,
        real_delta: Duration,
    ) -> EditorRuntimeOwnerState {
        let driven = match self.trusted_compiled_mut() {
            Some(lane) => lane.runtime_host.drive_running_runtime(real_delta),
            None => return EditorRuntimeOwnerState::Empty,
        };
        if let Err(fault) = driven {
            let _ = self.diagnostics.extend(fault.diagnostics);
            self.cancel_pending_runtime_edit_without_port();
            let pending = self
                .trusted_compiled_mut()
                .and_then(|lane| lane.pending_control.take());
            if let Some(pending) = pending {
                self.play_result = Some(EditorPlayOperationResult::Failed {
                    operation: pending.operation,
                    failure: EditorPlayFailure::Runtime,
                });
            }
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            {
                if let Some(lane) = self.trusted_compiled_mut() {
                    lane.pending_step_presentation = None;
                }
            }
            return EditorRuntimeOwnerState::Active {
                document,
                source_revision,
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                game,
            };
        }

        self.poll_runtime_edit();
        self.complete_pending_apply_changes(document, source_revision);

        let runtime_state = self
            .trusted_compiled()
            .and_then(|lane| lane.runtime_host.running_runtime_state());
        if runtime_state == Some(RuntimeState::Stopped) {
            let (pending, control_failed, close_failure, generation) = {
                let Some(lane) = self.trusted_compiled_mut() else {
                    return EditorRuntimeOwnerState::Empty;
                };
                let pending = lane.pending_control.take();
                let control_failed = pending.is_some_and(|pending| {
                    matches!(
                        lane.runtime_host.runtime_control_status(pending.ticket),
                        Some(RuntimeControlStatus::Failed(_))
                    )
                });
                let close_failure = lane
                    .runtime_host
                    .running_runtime_close_evidence()
                    .filter(super::close_evidence_has_terminal_failure);
                let generation = lane.runtime_host.running_runtime_generation();
                let released = lane.runtime_host.release_stopped_runtime();
                debug_assert!(released);
                (pending, control_failed, close_failure, generation)
            };
            let operation = pending.map_or(EditorPlayOperation::Stop, |pending| pending.operation);
            if control_failed || close_failure.is_some() {
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                {
                    if let Some(lane) = self.trusted_compiled_mut() {
                        lane.pending_step_presentation = None;
                    }
                }
                if let Some(evidence) = close_failure {
                    let _ = self
                        .diagnostics
                        .extend(super::cleanup_failed_report(&evidence));
                }
                self.play_result = Some(EditorPlayOperationResult::Failed {
                    operation,
                    failure: EditorPlayFailure::Close,
                });
                return EditorRuntimeOwnerState::Empty;
            }
            if operation == EditorPlayOperation::Restart {
                let Some(slot) = self.workspace.scene(document) else {
                    self.play_result = Some(EditorPlayOperationResult::Rejected {
                        operation,
                        reason: EditorPlayRejection::NoActiveDocument,
                    });
                    return EditorRuntimeOwnerState::Empty;
                };
                let revision = slot.revision();
                self.play_result = Some(EditorPlayOperationResult::Pending { operation });
                return EditorRuntimeOwnerState::Preparing {
                    document,
                    revision,
                    operation,
                };
            }
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            {
                if let Some(lane) = self.trusted_compiled_mut() {
                    lane.pending_step_presentation = None;
                }
            }
            self.play_result = Some(EditorPlayOperationResult::Applied {
                operation,
                generation,
            });
            return EditorRuntimeOwnerState::Empty;
        }

        if runtime_state == Some(RuntimeState::CloseIncomplete) {
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            {
                if let Some(lane) = self.trusted_compiled_mut() {
                    lane.pending_step_presentation = None;
                }
            }
            let pending = self
                .trusted_compiled_mut()
                .and_then(|lane| lane.pending_control.take());
            if let Some(pending) = pending {
                self.play_result = Some(EditorPlayOperationResult::Failed {
                    operation: pending.operation,
                    failure: EditorPlayFailure::Close,
                });
            }
            return EditorRuntimeOwnerState::Active {
                document,
                source_revision,
                #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                game,
            };
        }

        let pending = self
            .trusted_compiled()
            .and_then(|lane| lane.pending_control);
        if let Some(pending) = pending {
            let (status, generation) = self.trusted_compiled().map_or((None, None), |lane| {
                (
                    lane.runtime_host.runtime_control_status(pending.ticket),
                    lane.runtime_host.running_runtime_generation(),
                )
            });
            match status {
                Some(RuntimeControlStatus::Applied) => {
                    if let Some(lane) = self.trusted_compiled_mut() {
                        lane.pending_control = None;
                    }
                    if pending.operation == EditorPlayOperation::StepFixedTick {
                        #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                        {
                            if let Some(lane) = self.trusted_compiled_mut() {
                                lane.pending_step_presentation = generation.map(|generation| {
                                    EditorStepPresentationState::AwaitingCandidate { generation }
                                });
                            }
                        }
                        #[cfg(not(any(feature = "presentation-2d", feature = "presentation-ui")))]
                        {
                            self.play_result = Some(EditorPlayOperationResult::Applied {
                                operation: pending.operation,
                                generation,
                            });
                        }
                    } else {
                        self.play_result = Some(EditorPlayOperationResult::Applied {
                            operation: pending.operation,
                            generation,
                        });
                    }
                }
                Some(RuntimeControlStatus::Failed(_)) | None => {
                    if let Some(lane) = self.trusted_compiled_mut() {
                        lane.pending_control = None;
                    }
                    #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
                    {
                        if let Some(lane) = self.trusted_compiled_mut() {
                            lane.pending_step_presentation = None;
                        }
                    }
                    self.play_result = Some(EditorPlayOperationResult::Failed {
                        operation: pending.operation,
                        failure: EditorPlayFailure::Runtime,
                    });
                }
                Some(RuntimeControlStatus::Pending) => {}
            }
        }
        EditorRuntimeOwnerState::Active {
            document,
            source_revision,
            #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
            game,
        }
    }

    fn poll_runtime_edit(&mut self) {
        let Some(lane) = self.trusted_compiled_mut() else {
            return;
        };
        if !matches!(
            lane.runtime_edit_result,
            Some(EditorRuntimeEditResult::Pending(_))
        ) || !matches!(
            lane.runtime_host.running_runtime_state(),
            Some(RuntimeState::Running | RuntimeState::Paused)
        ) {
            return;
        }
        let Some(runtime) = lane.runtime_host.running_runtime_mut() else {
            return;
        };
        let Ok(Ok(EditorRuntimeBridgeOutput::Result(Some(result)))) =
            runtime.with_driver_scope(|scope| {
                scope.__apply_port::<EditorRuntimeBridge>(EditorRuntimeBridgeInput::TakeResult)
            })
        else {
            return;
        };
        lane.runtime_edit_result = Some(result);
    }

    fn cancel_pending_runtime_edit(&mut self) {
        let Some(lane) = self.trusted_compiled_mut() else {
            return;
        };
        if !matches!(
            lane.runtime_edit_result,
            Some(EditorRuntimeEditResult::Pending(_))
        ) {
            return;
        }
        let result = lane.runtime_host.running_runtime_mut().and_then(|runtime| {
            runtime
                .with_driver_scope(|scope| {
                    scope.__apply_port::<EditorRuntimeBridge>(EditorRuntimeBridgeInput::Cancel)
                })
                .ok()
                .and_then(Result::ok)
        });
        match result {
            Some(EditorRuntimeBridgeOutput::Result(Some(result))) => {
                lane.runtime_edit_result = Some(result);
            }
            _ => self.cancel_pending_runtime_edit_without_port(),
        }
    }

    fn cancel_pending_runtime_edit_without_port(&mut self) {
        let Some(lane) = self.trusted_compiled_mut() else {
            return;
        };
        let Some(EditorRuntimeEditResult::Pending(request)) = lane.runtime_edit_result.take()
        else {
            return;
        };
        lane.runtime_edit_result = Some(EditorRuntimeEditResult::Cancelled(request));
    }

    fn complete_pending_apply_changes(
        &mut self,
        document: EditorDocumentId,
        source_revision: nara_scene::SceneAuthoringRevision,
    ) {
        let Some(pending) = self
            .trusted_compiled_mut()
            .and_then(|lane| lane.pending_apply_changes.take())
        else {
            return;
        };
        let running_generation = self
            .trusted_compiled()
            .and_then(|lane| lane.runtime_host.running_runtime_generation());
        if pending.document != document
            || pending.source_revision != source_revision
            || running_generation != Some(pending.generation)
        {
            if let Some(lane) = self.trusted_compiled_mut() {
                lane.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                    request: pending.request,
                    reason: EditorApplyChangesRejection::StaleGeneration,
                    report: None,
                });
            }
            return;
        }
        let Some(slot) = self.workspace.scene(document) else {
            if let Some(lane) = self.trusted_compiled_mut() {
                lane.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                    request: pending.request,
                    reason: EditorApplyChangesRejection::StaleDocument,
                    report: None,
                });
            }
            return;
        };
        if slot.revision() != pending.document_revision {
            if let Some(lane) = self.trusted_compiled_mut() {
                lane.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                    request: pending.request,
                    reason: EditorApplyChangesRejection::StaleDocument,
                    report: None,
                });
            }
            return;
        }
        let request = pending.request;
        let exported = self.trusted_compiled().map(|lane| {
            lane.runtime_host.with_running_world(|world| {
                __export_apply_changes_from_world(
                    world,
                    slot.session(),
                    lane.plan.schema_validation().registry(),
                    source_revision,
                    request.clone(),
                )
            })
        });
        let Some(exported) = exported else {
            return;
        };
        let Ok(mut report) = exported else {
            if let Some(lane) = self.trusted_compiled_mut() {
                lane.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                    request,
                    reason: EditorApplyChangesRejection::InvalidRuntimeState,
                    report: None,
                });
            }
            return;
        };
        let Some(patch) = report.patch.clone() else {
            if let Some(lane) = self.trusted_compiled_mut() {
                lane.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                    request,
                    reason: EditorApplyChangesRejection::Unsupported,
                    report: Some(report),
                });
            }
            return;
        };
        let patch_report = match &mut self.execution {
            EditorExecutionLane::Authoring(_) => return,
            EditorExecutionLane::TrustedCompiled(lane) => self.workspace.apply_command(
                lane.plan.schema_validation().registry(),
                EditorWorkspaceCommand::ApplyScenePatch {
                    document,
                    base_revision: pending.document_revision,
                    patch,
                },
            ),
        };
        let Some(scene_patch_report) = patch_report.patch_report else {
            if let Some(lane) = self.trusted_compiled_mut() {
                lane.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                    request,
                    reason: EditorApplyChangesRejection::StaleDocument,
                    report: Some(report),
                });
            }
            return;
        };
        report.applied = scene_patch_report.applied;
        report.current_revision = patch_report.revision.unwrap_or(report.current_revision);
        report.patch_report = Some(scene_patch_report.clone());
        let _ = report.diagnostics.extend(scene_patch_report.diagnostics);
        for component in &mut report.components {
            if component.status == SceneApplyChangesComponentStatus::Pending {
                component.status = if report.applied {
                    SceneApplyChangesComponentStatus::Applied
                } else {
                    SceneApplyChangesComponentStatus::Rejected
                };
            }
        }
        if let Some(lane) = self.trusted_compiled_mut() {
            lane.apply_changes_result = Some(if report.applied {
                EditorApplyChangesResult::Applied(report)
            } else {
                EditorApplyChangesResult::Rejected {
                    request,
                    reason: EditorApplyChangesRejection::StaleDocument,
                    report: Some(report),
                }
            });
        }
    }

    fn cancel_pending_apply_changes(&mut self) {
        let Some(lane) = self.trusted_compiled_mut() else {
            return;
        };
        let Some(pending) = lane.pending_apply_changes.take() else {
            return;
        };
        lane.apply_changes_result = Some(EditorApplyChangesResult::Cancelled(pending.request));
    }
}

fn editor_open_error(code: &'static str) -> EditorProjectOpenError {
    EditorProjectOpenError {
        diagnostics: single_error(code, "Editor project could not be opened"),
    }
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
    use nara_identity::SceneInstanceId;
    use nara_reflect::{
        ComponentFieldId, ComponentFieldPath, ComponentFieldSchema, ComponentSchema,
        ComponentSchemaVersion, ComponentTypeId, ComponentValue, ComponentValueKind,
    };
    use nara_scene::SceneEntityId;

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
        let revision =
            nara_scene::SceneAuthoringSession::new(nara_scene::SceneDocument::default()).revision();

        let component_entity_id = SceneEntityId::new("component-read-only").unwrap();
        let field_entity_id = SceneEntityId::new("field-read-only").unwrap();
        let mut world = World::new();
        let component_entity = world
            .spawn((
                scene_source(1, component_entity_id.clone()),
                ComponentReadOnlyProbe { value: 7 },
            ))
            .id();
        let field_entity = world
            .spawn((
                scene_source(2, field_entity_id.clone()),
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
                apply_runtime_edit(&mut world, &registry, &request),
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

    fn scene_source(instance: u64, entity_id: SceneEntityId) -> SceneEntitySource {
        SceneEntitySource {
            instance_id: SceneInstanceId::new(instance).unwrap(),
            source_entity_id: entity_id.clone(),
            entity_id,
            projection_path: None,
        }
    }
}
