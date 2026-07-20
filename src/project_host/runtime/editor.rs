use std::{fmt, time::Duration};

use nara_app::{
    __RuntimeDriverPort, App, CoreStage, Plugin, PluginCategory, PluginDeclaration,
    PluginDefinition, PluginDefinitionId, PluginError, PluginId, RuntimeClosePolicy,
    RuntimeControl, RuntimeControlStatus, RuntimeControlTicket, RuntimeState,
};
use nara_diagnostic::DiagnosticReport;
use nara_ecs::{Entity, Mut, Resource, World};
use nara_fs::{DirectoryCapability, RelativePath};
use nara_gameplay::{GAMEPLAY_COMMAND_PLUGIN_ID, GameplayCommandPlugin};
use nara_reflect::{
    ComponentFieldId, ComponentRegistry, ComponentSchemaProviderDefinition, ComponentSchemaVersion,
    ComponentTypeId, ComponentValue,
};
use nara_scene::{SceneEntityId, SceneEntitySource};
use nara_tooling::{
    __export_apply_changes_from_world, EditorCloseDecision, EditorDocumentId,
    EditorPersistenceCommand, EditorPersistenceFailureStage, EditorPersistenceOperation,
    EditorPersistenceRejection, EditorPersistenceRequestResult, EditorPersistenceResult,
    EditorPersistenceView, EditorPlayCommand, EditorPlayFailure, EditorPlayOperation,
    EditorPlayOperationResult, EditorPlayRejection, EditorPlayRequestResult, EditorPlayState,
    EditorPlayView, EditorProjectView, EditorRuntimeEditRejection, EditorRuntimeEditRequest,
    EditorRuntimeEditResult, EditorWorkspace, EditorWorkspaceCommand, EditorWorkspaceCommandReport,
    EditorWorkspaceIntent, EditorWorkspaceIntentPhase, EditorWorkspaceIntentRejection,
    EditorWorkspaceIntentRequestResult, EditorWorkspaceIntentResult, EditorWorkspaceIntentView,
    SceneApplyChangesComponentStatus, SceneApplyChangesRequest,
};
use nara_tooling::{EditorApplyChangesRejection, EditorApplyChangesResult};

use crate::project_host::{
    ProjectContentLoader, ProjectRuntimePlugins, built_in_schema_providers,
    ingest_project_manifest, project_runtime_plugins, resolve_runtime_plan,
};

use super::{
    PROJECT_MANIFEST, ProjectHost, RuntimePlan, runtime_plan_failure_report,
    runtime_plan_selected_report, single_error,
};
use crate::project_host::persistence::{
    EditorPersistenceReceipt, ScenePersistenceHost, SceneReopenOutcome, SceneSaveCandidate,
    SceneSaveOutcome,
};

type EditorPluginEdit =
    Box<dyn FnOnce(ProjectRuntimePlugins) -> ProjectRuntimePlugins + Send + 'static>;

const EDITOR_RUNTIME_BRIDGE_PLUGIN_ID: PluginId = PluginId::new("nara.editor-runtime-bridge");
const EDITOR_RUNTIME_BRIDGE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.editor-runtime-bridge", 1);
const EDITOR_RUNTIME_BRIDGE_REQUIREMENTS: &[PluginId] = &[GAMEPLAY_COMMAND_PLUGIN_ID];
const EDITOR_RUNTIME_BRIDGE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(EDITOR_RUNTIME_BRIDGE_PLUGIN_ID, PluginCategory::Tooling)
        .requires_plugins(EDITOR_RUNTIME_BRIDGE_REQUIREMENTS);

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
    let Some(mut bridge) = world.remove_resource::<EditorRuntimeBridge>() else {
        return;
    };
    let Some(request) = bridge.pending.take() else {
        world.insert_resource(bridge);
        return;
    };
    let result = world.resource_scope(|world, registry: Mut<'_, ComponentRegistry>| {
        apply_runtime_edit(world, &registry, &request)
    });
    bridge.result = Some(result);
    world.insert_resource(bridge);
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
    if registry
        .resolve_field(&request.component, &request.field)
        .is_none()
    {
        return EditorRuntimeEditResult::Rejected {
            request: request.clone(),
            reason: EditorRuntimeEditRejection::UnknownField,
        };
    }
    let Some(Ok(Some(mut value))) = registry.encode_component(&request.component, world, entity)
    else {
        return EditorRuntimeEditResult::Rejected {
            request: request.clone(),
            reason: EditorRuntimeEditRejection::MissingComponent,
        };
    };
    let Some(field) = registry.resolve_field(&request.component, &request.field) else {
        return EditorRuntimeEditResult::Rejected {
            request: request.clone(),
            reason: EditorRuntimeEditRejection::UnknownField,
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

/// Rust authoring configuration for one concrete Editor project session.
pub struct EditorProjectIntent {
    profile: Option<String>,
    cleanup_policy: RuntimeClosePolicy,
    plugin_edits: Vec<EditorPluginEdit>,
    schema_providers: Vec<ComponentSchemaProviderDefinition>,
}

impl fmt::Debug for EditorProjectIntent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EditorProjectIntent")
            .field("profile_present", &self.profile.is_some())
            .field("cleanup_policy", &self.cleanup_policy)
            .field("plugin_edit_count", &self.plugin_edits.len())
            .field("schema_provider_count", &self.schema_providers.len())
            .finish_non_exhaustive()
    }
}

impl Default for EditorProjectIntent {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorProjectIntent {
    #[must_use]
    pub fn new() -> Self {
        Self {
            profile: None,
            cleanup_policy: RuntimeClosePolicy::default(),
            plugin_edits: Vec::new(),
            schema_providers: Vec::new(),
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

    #[must_use]
    pub fn configure(mut self, definition: PluginDefinition) -> Self {
        self.plugin_edits
            .push(Box::new(move |request| request.configure(definition)));
        self
    }

    #[must_use]
    pub fn disable<P: Plugin>(mut self) -> Self {
        self.plugin_edits
            .push(Box::new(|request| request.disable::<P>()));
        self
    }

    #[must_use]
    pub fn insert_after<P: Plugin>(mut self, definition: PluginDefinition) -> Self {
        self.plugin_edits.push(Box::new(move |request| {
            request.insert_after::<P>(definition)
        }));
        self
    }

    #[must_use]
    pub fn insert_before<P: Plugin>(mut self, definition: PluginDefinition) -> Self {
        self.plugin_edits.push(Box::new(move |request| {
            request.insert_before::<P>(definition)
        }));
        self
    }

    #[must_use]
    pub fn with_schema_provider(mut self, provider: ComponentSchemaProviderDefinition) -> Self {
        self.schema_providers.push(provider);
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
}

impl fmt::Display for EditorProjectOpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("editor project session could not be opened")
    }
}

impl std::error::Error for EditorProjectOpenError {}

struct PendingSave {
    document: EditorDocumentId,
    revision: nara_scene::SceneAuthoringRevision,
    scene: nara_scene::SceneDocument,
}

enum PendingPersistence {
    Save(PendingSave),
    Reopen { document: EditorDocumentId },
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
    },
    RetiringPlay {
        terminal: EditorPlayOperationResult,
    },
    Active {
        document: EditorDocumentId,
        source_revision: nara_scene::SceneAuthoringRevision,
    },
    RetirementIncomplete {
        terminal: EditorPlayOperationResult,
    },
    Transitioning,
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

#[derive(Debug, Clone, Copy)]
struct PendingWorkspaceIntent {
    intent: EditorWorkspaceIntent,
    document: EditorDocumentId,
    phase: EditorWorkspaceIntentPhase,
}

/// Concrete product Host for one Editor workspace and its future single Play owner.
pub struct EditorProjectSession {
    workspace: EditorWorkspace,
    scene_title: String,
    source_document: EditorDocumentId,
    plan: RuntimePlan,
    snapshot: crate::project_host::ProjectContentSnapshot,
    persistence: ScenePersistenceHost,
    persistence_operation: EditorPersistenceOperation,
    pending_persistence: Option<PendingPersistence>,
    persistence_result: Option<EditorPersistenceResult>,
    last_persistence_receipt: Option<EditorPersistenceReceipt>,
    diagnostics: DiagnosticReport,
    runtime_host: ProjectHost,
    runtime_owner: EditorRuntimeOwnerState,
    start_attempt: Option<super::RuntimeStartAttempt>,
    pending_control: Option<PendingEditorControl>,
    play_result: Option<EditorPlayOperationResult>,
    runtime_edit_result: Option<EditorRuntimeEditResult>,
    pending_apply_changes: Option<PendingApplyChanges>,
    apply_changes_result: Option<EditorApplyChangesResult>,
    pending_workspace_intent: Option<PendingWorkspaceIntent>,
    workspace_intent_result: Option<EditorWorkspaceIntentResult>,
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
    pub fn open(
        project_root: DirectoryCapability,
        intent: EditorProjectIntent,
    ) -> Result<Self, EditorProjectOpenError> {
        let EditorProjectIntent {
            profile,
            cleanup_policy,
            plugin_edits,
            schema_providers,
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

        let mut request = project_runtime_plugins(&candidate);
        for edit in plugin_edits {
            request = edit(request);
        }
        request = request.insert_after::<GameplayCommandPlugin>(editor_runtime_bridge_definition());
        let mut providers = built_in_schema_providers();
        providers.extend(schema_providers);
        let plan = resolve_runtime_plan(&candidate, request, providers).map_err(|error| {
            EditorProjectOpenError {
                diagnostics: runtime_plan_failure_report(&error),
            }
        })?;

        let loader = ProjectContentLoader::new(project_root.share()).map_err(|error| {
            EditorProjectOpenError {
                diagnostics: error.diagnostics().clone(),
            }
        })?;
        let snapshot = loader
            .load(&candidate, &plan)
            .map_err(|error| EditorProjectOpenError {
                diagnostics: error.diagnostics().clone(),
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
        let opened = ScenePersistenceHost::open(
            &scenes,
            startup.as_str(),
            plan.schema_validation().registry(),
        )
        .map_err(editor_persistence_open_error)?;

        let mut workspace = EditorWorkspace::new();
        let report = workspace
            .open_scene_session(startup.as_str(), opened.session)
            .map_err(|error| EditorProjectOpenError {
                diagnostics: error.report().diagnostics.clone(),
            })?;
        let document = report
            .opened_document
            .expect("opening one editor scene publishes one document identity");
        let binding = workspace.__bind_opened_source_digest(document, opened.digest);
        debug_assert!(binding.applied);

        let mut diagnostics = runtime_plan_selected_report(&plan);
        let _ = diagnostics.extend(report.diagnostics);
        Ok(Self {
            workspace,
            scene_title: startup.as_str().to_owned(),
            source_document: document,
            plan,
            snapshot,
            persistence: opened.host,
            persistence_operation: EditorPersistenceOperation::Idle,
            pending_persistence: None,
            persistence_result: None,
            last_persistence_receipt: None,
            diagnostics,
            runtime_host: ProjectHost::new(cleanup_policy),
            runtime_owner: EditorRuntimeOwnerState::Empty,
            start_attempt: None,
            pending_control: None,
            play_result: None,
            runtime_edit_result: None,
            pending_apply_changes: None,
            apply_changes_result: None,
            pending_workspace_intent: None,
            workspace_intent_result: None,
        })
    }

    #[must_use]
    pub const fn workspace(&self) -> &EditorWorkspace {
        &self.workspace
    }

    #[must_use]
    pub fn workspace_model(&mut self) -> nara_tooling::EditorWorkspaceModel {
        self.workspace
            .model(self.plan.schema_validation().registry(), None)
    }

    #[must_use]
    pub const fn persistence_view(&self) -> EditorPersistenceView {
        EditorPersistenceView::new(self.persistence_operation, self.persistence_result)
    }

    #[must_use]
    pub const fn workspace_intent_view(&self) -> EditorWorkspaceIntentView {
        let (intent, phase) = match self.pending_workspace_intent {
            Some(pending) => (Some(pending.intent), Some(pending.phase)),
            None => (None, None),
        };
        EditorWorkspaceIntentView::new(intent, phase, self.workspace_intent_result)
    }

    #[must_use]
    pub const fn last_persistence_receipt(&self) -> Option<EditorPersistenceReceipt> {
        self.last_persistence_receipt
    }

    #[must_use]
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    #[must_use]
    pub fn play_view(&self) -> EditorPlayView {
        let current_revision = self
            .workspace
            .active_scene()
            .map(nara_tooling::EditorSceneSlot::revision);
        let (state, source_revision) = match &self.runtime_owner {
            EditorRuntimeOwnerState::Empty => (EditorPlayState::Empty, None),
            EditorRuntimeOwnerState::Preparing { revision, .. } => {
                (EditorPlayState::PreparingPlay, Some(*revision))
            }
            EditorRuntimeOwnerState::Starting { revision, .. } => {
                (EditorPlayState::Starting, Some(*revision))
            }
            EditorRuntimeOwnerState::RetiringPlay { .. } => (EditorPlayState::RetiringPlay, None),
            EditorRuntimeOwnerState::RetirementIncomplete { .. } => {
                (EditorPlayState::RetirementIncomplete, None)
            }
            EditorRuntimeOwnerState::Active {
                source_revision, ..
            } => {
                let state = match self.runtime_host.running_runtime_state() {
                    Some(RuntimeState::Running) => EditorPlayState::Running,
                    Some(RuntimeState::Paused) => EditorPlayState::Paused,
                    Some(RuntimeState::Stepping) => EditorPlayState::Stepping,
                    Some(RuntimeState::Faulted) => EditorPlayState::Faulted,
                    Some(RuntimeState::Stopping) => EditorPlayState::Stopping,
                    Some(RuntimeState::CloseIncomplete) => EditorPlayState::CloseIncomplete,
                    Some(RuntimeState::Stopped) | None => EditorPlayState::Empty,
                };
                let projected =
                    self.pending_control
                        .map_or(state, |pending| match pending.operation {
                            EditorPlayOperation::StepFixedTick => EditorPlayState::Stepping,
                            EditorPlayOperation::Stop
                            | EditorPlayOperation::Restart
                            | EditorPlayOperation::RetryClose => EditorPlayState::Stopping,
                            EditorPlayOperation::Play
                            | EditorPlayOperation::Cancel
                            | EditorPlayOperation::Pause
                            | EditorPlayOperation::Resume
                            | EditorPlayOperation::RetryRetirement => state,
                        });
                (projected, Some(*source_revision))
            }
            EditorRuntimeOwnerState::Transitioning => {
                unreachable!("editor runtime state is not observed during a transition")
            }
        };
        let out_of_date = source_revision
            .zip(current_revision)
            .is_some_and(|(source, current)| source != current);
        EditorPlayView::new(
            state,
            self.runtime_host.running_runtime_generation(),
            source_revision,
            current_revision,
            out_of_date,
            self.play_result,
        )
    }

    pub fn apply_workspace_command(
        &mut self,
        command: EditorWorkspaceCommand,
    ) -> EditorWorkspaceCommandReport {
        if matches!(command, EditorWorkspaceCommand::CloseScene { .. }) {
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
        self.workspace
            .apply_command(self.plan.schema_validation().registry(), command)
    }

    pub fn request_workspace_intent(
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

        let document = match intent {
            EditorWorkspaceIntent::CloseScene { document } => document,
            EditorWorkspaceIntent::Exit => {
                let Some(document) = self.workspace.active_document() else {
                    self.workspace_intent_result =
                        Some(EditorWorkspaceIntentResult::Applied { intent });
                    return EditorWorkspaceIntentRequestResult::Accepted;
                };
                document
            }
        };
        let Some(slot) = self.workspace.scene(document) else {
            let reason = if self.workspace.active_document().is_some() {
                EditorWorkspaceIntentRejection::MissingDocument
            } else {
                EditorWorkspaceIntentRejection::NoActiveDocument
            };
            self.workspace_intent_result =
                Some(EditorWorkspaceIntentResult::Rejected { intent, reason });
            return EditorWorkspaceIntentRequestResult::Rejected(reason);
        };
        let phase = if slot.is_dirty() {
            EditorWorkspaceIntentPhase::AwaitingDecision
        } else {
            EditorWorkspaceIntentPhase::RetiringRuntime
        };
        self.pending_workspace_intent = Some(PendingWorkspaceIntent {
            intent,
            document,
            phase,
        });
        if phase == EditorWorkspaceIntentPhase::RetiringRuntime {
            self.advance_workspace_retirement();
        }
        EditorWorkspaceIntentRequestResult::Accepted
    }

    pub fn resolve_workspace_intent(
        &mut self,
        decision: EditorCloseDecision,
    ) -> EditorWorkspaceIntentRequestResult {
        let Some(pending) = self.pending_workspace_intent else {
            return EditorWorkspaceIntentRequestResult::Rejected(
                EditorWorkspaceIntentRejection::DecisionNotRequired,
            );
        };
        if pending.phase != EditorWorkspaceIntentPhase::AwaitingDecision {
            return EditorWorkspaceIntentRequestResult::Rejected(
                EditorWorkspaceIntentRejection::Busy,
            );
        }

        match decision {
            EditorCloseDecision::Cancel => {
                self.pending_workspace_intent = None;
                self.workspace_intent_result = Some(EditorWorkspaceIntentResult::Cancelled {
                    intent: pending.intent,
                });
            }
            EditorCloseDecision::Discard => {
                self.pending_workspace_intent = Some(PendingWorkspaceIntent {
                    phase: EditorWorkspaceIntentPhase::RetiringRuntime,
                    ..pending
                });
                self.advance_workspace_retirement();
            }
            EditorCloseDecision::Save => {
                let requested = self.request_persistence(EditorPersistenceCommand::Save {
                    document: Some(pending.document),
                });
                let EditorPersistenceRequestResult::Accepted = requested else {
                    let EditorPersistenceRequestResult::Rejected(reason) = requested else {
                        unreachable!()
                    };
                    return EditorWorkspaceIntentRequestResult::Rejected(
                        EditorWorkspaceIntentRejection::PersistenceRejected(reason),
                    );
                };
                self.pending_workspace_intent = Some(PendingWorkspaceIntent {
                    phase: EditorWorkspaceIntentPhase::Saving,
                    ..pending
                });
            }
        }
        EditorWorkspaceIntentRequestResult::Accepted
    }

    pub fn acknowledge_workspace_intent_result(&mut self) -> bool {
        self.workspace_intent_result.take().is_some()
    }

    pub fn request_persistence(
        &mut self,
        command: EditorPersistenceCommand,
    ) -> EditorPersistenceRequestResult {
        if command == EditorPersistenceCommand::AcknowledgeResult {
            return if self.pending_persistence.is_some() {
                EditorPersistenceRequestResult::Rejected(EditorPersistenceRejection::Busy)
            } else if self.persistence_result.take().is_some() {
                EditorPersistenceRequestResult::Accepted
            } else {
                EditorPersistenceRequestResult::Rejected(EditorPersistenceRejection::ResultPending)
            };
        }
        if self.pending_persistence.is_some() {
            return EditorPersistenceRequestResult::Rejected(EditorPersistenceRejection::Busy);
        }
        if self.persistence_result.is_some()
            && !matches!(
                (self.persistence_result, command),
                (
                    Some(EditorPersistenceResult::PersistenceUncertain { .. }),
                    EditorPersistenceCommand::Reopen { .. }
                )
            )
        {
            return EditorPersistenceRequestResult::Rejected(
                EditorPersistenceRejection::ResultPending,
            );
        }

        let requested_document = match command {
            EditorPersistenceCommand::Save { document } => {
                let Some(document) = document.or(self.workspace.active_document()) else {
                    self.persistence_result = Some(EditorPersistenceResult::Rejected {
                        document: None,
                        reason: EditorPersistenceRejection::NoActiveDocument,
                    });
                    return EditorPersistenceRequestResult::Rejected(
                        EditorPersistenceRejection::NoActiveDocument,
                    );
                };
                document
            }
            EditorPersistenceCommand::Reopen { document } => document
                .or(self.workspace.active_document())
                .unwrap_or(self.source_document),
            EditorPersistenceCommand::AcknowledgeResult => unreachable!(),
        };

        if matches!(command, EditorPersistenceCommand::Reopen { .. })
            && (!matches!(self.runtime_owner, EditorRuntimeOwnerState::Empty)
                || self.start_attempt.is_some()
                || self.runtime_host.has_cleanup_owner())
        {
            self.persistence_result = Some(EditorPersistenceResult::Rejected {
                document: Some(requested_document),
                reason: EditorPersistenceRejection::RuntimeActive,
            });
            return EditorPersistenceRequestResult::Rejected(
                EditorPersistenceRejection::RuntimeActive,
            );
        }

        match command {
            EditorPersistenceCommand::Save { .. } => {
                let Some(slot) = self.workspace.scene(requested_document) else {
                    self.persistence_result = Some(EditorPersistenceResult::Rejected {
                        document: Some(requested_document),
                        reason: EditorPersistenceRejection::MissingDocument,
                    });
                    return EditorPersistenceRequestResult::Rejected(
                        EditorPersistenceRejection::MissingDocument,
                    );
                };
                if !slot.is_dirty() {
                    self.persistence_result = Some(EditorPersistenceResult::Rejected {
                        document: Some(requested_document),
                        reason: EditorPersistenceRejection::NotDirty,
                    });
                    return EditorPersistenceRequestResult::Rejected(
                        EditorPersistenceRejection::NotDirty,
                    );
                }
                let revision = slot.revision();
                self.pending_persistence = Some(PendingPersistence::Save(PendingSave {
                    document: requested_document,
                    revision,
                    scene: slot.session().document().clone(),
                }));
                self.persistence_operation = EditorPersistenceOperation::Saving {
                    document: requested_document,
                    captured_revision: revision,
                };
            }
            EditorPersistenceCommand::Reopen { .. } => {
                self.persistence_result = None;
                self.pending_persistence = Some(PendingPersistence::Reopen {
                    document: requested_document,
                });
                self.persistence_operation = EditorPersistenceOperation::Opening {
                    document: requested_document,
                };
            }
            EditorPersistenceCommand::AcknowledgeResult => unreachable!(),
        }
        EditorPersistenceRequestResult::Accepted
    }

    pub fn request_play(&mut self, command: EditorPlayCommand) -> EditorPlayRequestResult {
        if command == EditorPlayCommand::AcknowledgeResult {
            return if self.play_result.take().is_some() {
                EditorPlayRequestResult::Accepted
            } else {
                EditorPlayRequestResult::Rejected(EditorPlayRejection::ResultPending)
            };
        }
        if let Some(pending) = self.pending_workspace_intent {
            let retry_matches_owner = matches!(
                (pending.phase, command, self.play_view().state()),
                (
                    EditorWorkspaceIntentPhase::RetiringRuntime,
                    EditorPlayCommand::RetryRetirement,
                    EditorPlayState::RetirementIncomplete,
                ) | (
                    EditorWorkspaceIntentPhase::RetiringRuntime,
                    EditorPlayCommand::RetryClose,
                    EditorPlayState::CloseIncomplete,
                )
            );
            if !retry_matches_owner {
                return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
            }
        }

        match command {
            EditorPlayCommand::Play => self.request_start(EditorPlayOperation::Play),
            EditorPlayCommand::Cancel => self.request_start_cancel(),
            EditorPlayCommand::Pause => {
                self.request_active_control(EditorPlayOperation::Pause, RuntimeControl::Pause)
            }
            EditorPlayCommand::Resume => {
                self.request_active_control(EditorPlayOperation::Resume, RuntimeControl::Resume)
            }
            EditorPlayCommand::StepFixedTick => self.request_active_control(
                EditorPlayOperation::StepFixedTick,
                RuntimeControl::StepFixedTick,
            ),
            EditorPlayCommand::Stop => {
                self.request_active_control(EditorPlayOperation::Stop, RuntimeControl::Stop)
            }
            EditorPlayCommand::Restart => {
                self.request_active_control(EditorPlayOperation::Restart, RuntimeControl::Stop)
            }
            EditorPlayCommand::RetryClose => self.request_active_control(
                EditorPlayOperation::RetryClose,
                RuntimeControl::RetryClose,
            ),
            EditorPlayCommand::RetryRetirement => {
                let state = std::mem::replace(
                    &mut self.runtime_owner,
                    EditorRuntimeOwnerState::Transitioning,
                );
                match state {
                    EditorRuntimeOwnerState::RetirementIncomplete { terminal } => {
                        self.runtime_owner = EditorRuntimeOwnerState::RetiringPlay { terminal };
                        self.play_result = Some(EditorPlayOperationResult::Pending {
                            operation: EditorPlayOperation::RetryRetirement,
                        });
                        EditorPlayRequestResult::Accepted
                    }
                    other => {
                        self.runtime_owner = other;
                        EditorPlayRequestResult::Rejected(EditorPlayRejection::InvalidState)
                    }
                }
            }
            EditorPlayCommand::AcknowledgeResult => unreachable!(),
        }
    }

    pub fn request_runtime_edit(
        &mut self,
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        field: ComponentFieldId,
        value: ComponentValue,
    ) -> Result<(), EditorRuntimeEditRejection> {
        if self.pending_workspace_intent.is_some() {
            return Err(EditorRuntimeEditRejection::Busy);
        }
        if self.runtime_edit_result.is_some() {
            return Err(EditorRuntimeEditRejection::Busy);
        }
        let EditorRuntimeOwnerState::Active { document, .. } = self.runtime_owner else {
            return Err(EditorRuntimeEditRejection::InvalidRuntimeState);
        };
        if !matches!(
            self.runtime_host.running_runtime_state(),
            Some(RuntimeState::Running | RuntimeState::Paused)
        ) {
            return Err(EditorRuntimeEditRejection::InvalidRuntimeState);
        }
        let generation = self
            .runtime_host
            .running_runtime_generation()
            .ok_or(EditorRuntimeEditRejection::InvalidRuntimeState)?;
        let revision = self
            .workspace
            .scene(document)
            .ok_or(EditorRuntimeEditRejection::StaleDocument)?
            .revision();
        let request = EditorRuntimeEditRequest {
            generation,
            document_revision: revision,
            entity,
            component,
            component_version,
            field,
            value,
        };
        self.request_runtime_edit_request(request)
    }

    pub fn request_runtime_edit_request(
        &mut self,
        request: EditorRuntimeEditRequest,
    ) -> Result<(), EditorRuntimeEditRejection> {
        if self.pending_workspace_intent.is_some() || self.runtime_edit_result.is_some() {
            return Err(EditorRuntimeEditRejection::Busy);
        }
        let EditorRuntimeOwnerState::Active { document, .. } = self.runtime_owner else {
            return Err(EditorRuntimeEditRejection::InvalidRuntimeState);
        };
        if !matches!(
            self.runtime_host.running_runtime_state(),
            Some(RuntimeState::Running | RuntimeState::Paused)
        ) {
            return Err(EditorRuntimeEditRejection::InvalidRuntimeState);
        }
        let generation = self
            .runtime_host
            .running_runtime_generation()
            .ok_or(EditorRuntimeEditRejection::InvalidRuntimeState)?;
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
        let runtime = self
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
                self.runtime_edit_result = Some(EditorRuntimeEditResult::Pending(request));
                Ok(())
            }
            EditorRuntimeBridgeOutput::Busy | EditorRuntimeBridgeOutput::Result(_) => {
                Err(EditorRuntimeEditRejection::Busy)
            }
        }
    }

    #[must_use]
    pub const fn runtime_edit_result(&self) -> Option<&EditorRuntimeEditResult> {
        self.runtime_edit_result.as_ref()
    }

    pub fn acknowledge_runtime_edit_result(&mut self) -> bool {
        self.runtime_edit_result.take().is_some()
    }

    pub fn request_apply_changes(
        &mut self,
        request: SceneApplyChangesRequest,
    ) -> Result<(), EditorApplyChangesRejection> {
        if self.pending_workspace_intent.is_some() {
            return Err(EditorApplyChangesRejection::Busy);
        }
        if self.pending_apply_changes.is_some() || self.apply_changes_result.is_some() {
            return Err(EditorApplyChangesRejection::Busy);
        }
        let EditorRuntimeOwnerState::Active {
            document,
            source_revision,
        } = self.runtime_owner
        else {
            return Err(EditorApplyChangesRejection::InvalidRuntimeState);
        };
        if !matches!(
            self.runtime_host.running_runtime_state(),
            Some(RuntimeState::Running | RuntimeState::Paused)
        ) {
            return Err(EditorApplyChangesRejection::InvalidRuntimeState);
        }
        let generation = self
            .runtime_host
            .running_runtime_generation()
            .ok_or(EditorApplyChangesRejection::InvalidRuntimeState)?;
        let document_revision = self
            .workspace
            .scene(document)
            .ok_or(EditorApplyChangesRejection::StaleDocument)?
            .revision();
        if document_revision != source_revision {
            return Err(EditorApplyChangesRejection::RuntimeOutOfDate);
        }
        self.pending_apply_changes = Some(PendingApplyChanges {
            generation,
            document,
            document_revision,
            source_revision,
            request: request.clone(),
        });
        self.apply_changes_result = Some(EditorApplyChangesResult::Pending {
            generation,
            document_revision,
            request,
        });
        Ok(())
    }

    #[must_use]
    pub const fn apply_changes_result(&self) -> Option<&EditorApplyChangesResult> {
        self.apply_changes_result.as_ref()
    }

    pub fn acknowledge_apply_changes_result(&mut self) -> bool {
        if self.pending_apply_changes.is_some() {
            return false;
        }
        self.apply_changes_result.take().is_some()
    }

    /// Drives one Editor frame across persistence and the single retained runtime owner.
    pub fn drive_editor_frame(&mut self, real_delta: Duration) -> EditorProjectView {
        if let Some(operation) = self.pending_persistence.take() {
            self.persistence_result = Some(match operation {
                PendingPersistence::Save(save) => self.complete_save(save),
                PendingPersistence::Reopen { document } => self.complete_reopen(document),
            });
            self.persistence_operation = EditorPersistenceOperation::Idle;
        }
        self.advance_workspace_persistence();
        self.advance_workspace_retirement();
        self.drive_play_owner(real_delta);
        self.advance_workspace_retirement();
        EditorProjectView::new(
            self.play_view(),
            self.persistence_view(),
            self.workspace_intent_view(),
        )
        .with_inspector_results(
            self.runtime_edit_result.clone(),
            self.apply_changes_result.clone(),
        )
    }

    fn advance_workspace_persistence(&mut self) {
        let Some(pending) = self.pending_workspace_intent else {
            return;
        };
        if pending.phase != EditorWorkspaceIntentPhase::Saving {
            return;
        }
        match self.persistence_result {
            Some(EditorPersistenceResult::Saved { document, .. })
                if document == pending.document =>
            {
                self.pending_workspace_intent = Some(PendingWorkspaceIntent {
                    phase: EditorWorkspaceIntentPhase::RetiringRuntime,
                    ..pending
                });
            }
            Some(
                EditorPersistenceResult::Rejected { .. }
                | EditorPersistenceResult::Failed { .. }
                | EditorPersistenceResult::PersistenceUncertain { .. },
            ) => {
                self.pending_workspace_intent = Some(PendingWorkspaceIntent {
                    phase: EditorWorkspaceIntentPhase::AwaitingDecision,
                    ..pending
                });
            }
            Some(EditorPersistenceResult::Opened { .. })
            | Some(EditorPersistenceResult::Saved { .. }) => {
                self.pending_workspace_intent = Some(PendingWorkspaceIntent {
                    phase: EditorWorkspaceIntentPhase::AwaitingDecision,
                    ..pending
                });
            }
            None => {}
        }
    }

    fn advance_workspace_retirement(&mut self) {
        let Some(pending) = self.pending_workspace_intent else {
            return;
        };
        if pending.phase != EditorWorkspaceIntentPhase::RetiringRuntime {
            return;
        }

        match &self.runtime_owner {
            EditorRuntimeOwnerState::Empty => {
                self.finish_workspace_intent(pending);
            }
            EditorRuntimeOwnerState::Preparing { operation, .. } => {
                let operation = *operation;
                self.runtime_owner = EditorRuntimeOwnerState::Empty;
                self.play_result = Some(EditorPlayOperationResult::Cancelled { operation });
                self.finish_workspace_intent(pending);
            }
            EditorRuntimeOwnerState::Starting { operation, .. } => {
                let operation = *operation;
                self.cancel_pending_runtime_edit();
                self.cancel_pending_apply_changes();
                self.runtime_owner = EditorRuntimeOwnerState::RetiringPlay {
                    terminal: EditorPlayOperationResult::Cancelled { operation },
                };
            }
            EditorRuntimeOwnerState::Active { .. } => {
                let runtime_state = self.runtime_host.running_runtime_state();
                if self.pending_control.is_none()
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
            EditorRuntimeOwnerState::RetiringPlay { .. }
            | EditorRuntimeOwnerState::RetirementIncomplete { .. } => {}
            EditorRuntimeOwnerState::Transitioning => {
                unreachable!("workspace retirement is advanced outside runtime transitions")
            }
        }
    }

    fn finish_workspace_intent(&mut self, pending: PendingWorkspaceIntent) {
        let report = self.workspace.__discard_scene(pending.document);
        let _ = self.diagnostics.extend(report.diagnostics);
        self.pending_workspace_intent = None;
        self.workspace_intent_result = Some(if report.applied {
            EditorWorkspaceIntentResult::Applied {
                intent: pending.intent,
            }
        } else {
            EditorWorkspaceIntentResult::Rejected {
                intent: pending.intent,
                reason: EditorWorkspaceIntentRejection::MissingDocument,
            }
        });
    }

    fn request_start(&mut self, operation: EditorPlayOperation) -> EditorPlayRequestResult {
        if !matches!(self.runtime_owner, EditorRuntimeOwnerState::Empty)
            || self.start_attempt.is_some()
            || self.pending_control.is_some()
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
        self.runtime_owner = EditorRuntimeOwnerState::Preparing {
            document,
            revision,
            operation,
        };
        self.play_result = Some(EditorPlayOperationResult::Pending { operation });
        EditorPlayRequestResult::Accepted
    }

    fn request_start_cancel(&mut self) -> EditorPlayRequestResult {
        let state = std::mem::replace(
            &mut self.runtime_owner,
            EditorRuntimeOwnerState::Transitioning,
        );
        match state {
            EditorRuntimeOwnerState::Preparing { operation, .. } => {
                self.runtime_owner = EditorRuntimeOwnerState::Empty;
                self.play_result = Some(EditorPlayOperationResult::Cancelled { operation });
                EditorPlayRequestResult::Accepted
            }
            EditorRuntimeOwnerState::Starting { operation, .. } => {
                self.runtime_owner = EditorRuntimeOwnerState::RetiringPlay {
                    terminal: EditorPlayOperationResult::Cancelled { operation },
                };
                self.play_result = Some(EditorPlayOperationResult::Pending {
                    operation: EditorPlayOperation::Cancel,
                });
                EditorPlayRequestResult::Accepted
            }
            other => {
                self.runtime_owner = other;
                EditorPlayRequestResult::Rejected(EditorPlayRejection::InvalidState)
            }
        }
    }

    fn request_active_control(
        &mut self,
        operation: EditorPlayOperation,
        control: RuntimeControl,
    ) -> EditorPlayRequestResult {
        if !matches!(self.runtime_owner, EditorRuntimeOwnerState::Active { .. }) {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::InvalidState);
        }
        if self.pending_control.is_some() {
            return EditorPlayRequestResult::Rejected(EditorPlayRejection::Busy);
        }
        if matches!(
            operation,
            EditorPlayOperation::Stop | EditorPlayOperation::Restart
        ) {
            self.cancel_pending_runtime_edit();
            self.cancel_pending_apply_changes();
        }
        match self.runtime_host.request_runtime_control(control) {
            Ok(ticket) => {
                self.pending_control = Some(PendingEditorControl { operation, ticket });
                self.play_result = Some(EditorPlayOperationResult::Pending { operation });
                EditorPlayRequestResult::Accepted
            }
            Err(fault) => {
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
        let state = std::mem::replace(
            &mut self.runtime_owner,
            EditorRuntimeOwnerState::Transitioning,
        );
        self.runtime_owner = match state {
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
            } => self.drive_starting(document, revision, operation),
            EditorRuntimeOwnerState::RetiringPlay { terminal } => {
                self.drive_start_retirement(terminal)
            }
            EditorRuntimeOwnerState::RetirementIncomplete { terminal } => {
                EditorRuntimeOwnerState::RetirementIncomplete { terminal }
            }
            EditorRuntimeOwnerState::Active {
                document,
                source_revision,
            } => self.drive_active(document, source_revision, real_delta),
            EditorRuntimeOwnerState::Transitioning => {
                unreachable!("one Editor frame owns one runtime state transition")
            }
        };
    }

    fn drive_preparing(
        &mut self,
        document: EditorDocumentId,
        requested_revision: nara_scene::SceneAuthoringRevision,
        operation: EditorPlayOperation,
    ) -> EditorRuntimeOwnerState {
        let Some(slot) = self.workspace.scene(document) else {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation,
                reason: EditorPlayRejection::NoActiveDocument,
            });
            return EditorRuntimeOwnerState::Empty;
        };
        let revision = slot.revision();
        if revision.source_id() != requested_revision.source_id() {
            self.play_result = Some(EditorPlayOperationResult::Rejected {
                operation,
                reason: EditorPlayRejection::StaleDocument,
            });
            return EditorRuntimeOwnerState::Empty;
        }
        let expanded = match self.snapshot.prepare_editor_startup_scene(
            slot.session().document(),
            self.plan.schema_validation().registry(),
        ) {
            Ok(expanded) => expanded,
            Err(error) => {
                let _ = self.diagnostics.extend(error.diagnostics().clone());
                self.play_result = Some(EditorPlayOperationResult::Failed {
                    operation,
                    failure: EditorPlayFailure::Preparation,
                });
                return EditorRuntimeOwnerState::Empty;
            }
        };
        match self.runtime_host.begin_editor_start(
            self.snapshot.clone(),
            self.plan.clone(),
            expanded,
            Vec::new(),
        ) {
            Ok(attempt) => {
                self.start_attempt = Some(attempt);
                EditorRuntimeOwnerState::Starting {
                    document,
                    revision,
                    operation,
                }
            }
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
    ) -> EditorRuntimeOwnerState {
        let Some(mut attempt) = self.start_attempt.take() else {
            self.play_result = Some(EditorPlayOperationResult::Failed {
                operation,
                failure: EditorPlayFailure::Start,
            });
            return EditorRuntimeOwnerState::Empty;
        };
        match self.runtime_host.complete_start(&mut attempt) {
            Ok(diagnostics) => {
                let _ = self.diagnostics.extend(diagnostics);
                let generation = self.runtime_host.running_runtime_generation();
                self.play_result = Some(EditorPlayOperationResult::Applied {
                    operation,
                    generation,
                });
                EditorRuntimeOwnerState::Active {
                    document,
                    source_revision: revision,
                }
            }
            Err(fault) => {
                let _ = self.diagnostics.extend(fault.diagnostics);
                let terminal = EditorPlayOperationResult::Failed {
                    operation,
                    failure: EditorPlayFailure::Start,
                };
                if self.runtime_host.has_cleanup_owner() {
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
        if let Some(attempt) = self.start_attempt.take() {
            drop(attempt);
            self.play_result = Some(terminal);
            return EditorRuntimeOwnerState::Empty;
        }
        match self.runtime_host.drive_cleanup_once() {
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
            super::CleanupDriveOutcome::Incomplete => {
                EditorRuntimeOwnerState::RetirementIncomplete { terminal }
            }
        }
    }

    fn drive_active(
        &mut self,
        document: EditorDocumentId,
        source_revision: nara_scene::SceneAuthoringRevision,
        real_delta: Duration,
    ) -> EditorRuntimeOwnerState {
        if let Err(fault) = self.runtime_host.drive_running_runtime(real_delta) {
            let _ = self.diagnostics.extend(fault.diagnostics);
            self.cancel_pending_runtime_edit_without_port();
            if let Some(pending) = self.pending_control.take() {
                self.play_result = Some(EditorPlayOperationResult::Failed {
                    operation: pending.operation,
                    failure: EditorPlayFailure::Runtime,
                });
            }
            return EditorRuntimeOwnerState::Active {
                document,
                source_revision,
            };
        }

        self.poll_runtime_edit();
        self.complete_pending_apply_changes(document, source_revision);

        let runtime_state = self.runtime_host.running_runtime_state();
        if runtime_state == Some(RuntimeState::Stopped) {
            let operation = self
                .pending_control
                .take()
                .map_or(EditorPlayOperation::Stop, |pending| pending.operation);
            let generation = self.runtime_host.running_runtime_generation();
            let released = self.runtime_host.release_stopped_runtime();
            debug_assert!(released);
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
            self.play_result = Some(EditorPlayOperationResult::Applied {
                operation,
                generation,
            });
            return EditorRuntimeOwnerState::Empty;
        }

        if runtime_state == Some(RuntimeState::CloseIncomplete) {
            if let Some(pending) = self.pending_control.take() {
                self.play_result = Some(EditorPlayOperationResult::Failed {
                    operation: pending.operation,
                    failure: EditorPlayFailure::Close,
                });
            }
            return EditorRuntimeOwnerState::Active {
                document,
                source_revision,
            };
        }

        if let Some(pending) = self.pending_control {
            match self.runtime_host.runtime_control_status(pending.ticket) {
                Some(RuntimeControlStatus::Applied) => {
                    self.pending_control = None;
                    self.play_result = Some(EditorPlayOperationResult::Applied {
                        operation: pending.operation,
                        generation: self.runtime_host.running_runtime_generation(),
                    });
                }
                Some(RuntimeControlStatus::Failed(_)) | None => {
                    self.pending_control = None;
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
        }
    }

    fn poll_runtime_edit(&mut self) {
        if !matches!(
            self.runtime_edit_result,
            Some(EditorRuntimeEditResult::Pending(_))
        ) || !matches!(
            self.runtime_host.running_runtime_state(),
            Some(RuntimeState::Running | RuntimeState::Paused)
        ) {
            return;
        }
        let Some(runtime) = self.runtime_host.running_runtime_mut() else {
            return;
        };
        let Ok(Ok(EditorRuntimeBridgeOutput::Result(Some(result)))) =
            runtime.with_driver_scope(|scope| {
                scope.__apply_port::<EditorRuntimeBridge>(EditorRuntimeBridgeInput::TakeResult)
            })
        else {
            return;
        };
        self.runtime_edit_result = Some(result);
    }

    fn cancel_pending_runtime_edit(&mut self) {
        if !matches!(
            self.runtime_edit_result,
            Some(EditorRuntimeEditResult::Pending(_))
        ) {
            return;
        }
        let result = self.runtime_host.running_runtime_mut().and_then(|runtime| {
            runtime
                .with_driver_scope(|scope| {
                    scope.__apply_port::<EditorRuntimeBridge>(EditorRuntimeBridgeInput::Cancel)
                })
                .ok()
                .and_then(Result::ok)
        });
        match result {
            Some(EditorRuntimeBridgeOutput::Result(Some(result))) => {
                self.runtime_edit_result = Some(result);
            }
            _ => self.cancel_pending_runtime_edit_without_port(),
        }
    }

    fn cancel_pending_runtime_edit_without_port(&mut self) {
        let Some(EditorRuntimeEditResult::Pending(request)) = self.runtime_edit_result.take()
        else {
            return;
        };
        self.runtime_edit_result = Some(EditorRuntimeEditResult::Cancelled(request));
    }

    fn complete_pending_apply_changes(
        &mut self,
        document: EditorDocumentId,
        source_revision: nara_scene::SceneAuthoringRevision,
    ) {
        let Some(pending) = self.pending_apply_changes.take() else {
            return;
        };
        if pending.document != document
            || pending.source_revision != source_revision
            || self.runtime_host.running_runtime_generation() != Some(pending.generation)
        {
            self.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                request: pending.request,
                reason: EditorApplyChangesRejection::StaleGeneration,
                report: None,
            });
            return;
        }
        let Some(slot) = self.workspace.scene(document) else {
            self.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                request: pending.request,
                reason: EditorApplyChangesRejection::StaleDocument,
                report: None,
            });
            return;
        };
        if slot.revision() != pending.document_revision {
            self.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                request: pending.request,
                reason: EditorApplyChangesRejection::StaleDocument,
                report: None,
            });
            return;
        }
        let request = pending.request;
        let exported = self.runtime_host.with_running_world(|world| {
            __export_apply_changes_from_world(
                world,
                slot.session(),
                self.plan.schema_validation().registry(),
                source_revision,
                request.clone(),
            )
        });
        let Ok(mut report) = exported else {
            self.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                request,
                reason: EditorApplyChangesRejection::InvalidRuntimeState,
                report: None,
            });
            return;
        };
        let Some(patch) = report.patch.clone() else {
            self.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                request,
                reason: EditorApplyChangesRejection::Unsupported,
                report: Some(report),
            });
            return;
        };
        let patch_report = self.workspace.apply_command(
            self.plan.schema_validation().registry(),
            EditorWorkspaceCommand::ApplyScenePatch {
                document: Some(document),
                patch,
            },
        );
        let Some(scene_patch_report) = patch_report.patch_report else {
            self.apply_changes_result = Some(EditorApplyChangesResult::Rejected {
                request,
                reason: EditorApplyChangesRejection::StaleDocument,
                report: Some(report),
            });
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
        self.apply_changes_result = Some(if report.applied {
            EditorApplyChangesResult::Applied(report)
        } else {
            EditorApplyChangesResult::Rejected {
                request,
                reason: EditorApplyChangesRejection::StaleDocument,
                report: Some(report),
            }
        });
    }

    fn cancel_pending_apply_changes(&mut self) {
        let Some(pending) = self.pending_apply_changes.take() else {
            return;
        };
        self.apply_changes_result = Some(EditorApplyChangesResult::Cancelled(pending.request));
    }

    fn complete_save(&mut self, save: PendingSave) -> EditorPersistenceResult {
        match self.persistence.save(SceneSaveCandidate {
            document: save.document,
            revision: save.revision,
            scene: save.scene,
        }) {
            SceneSaveOutcome::Saved(receipt) => {
                let report = self
                    .workspace
                    .__apply_persistence_checkpoint(receipt.checkpoint());
                if !report.applied {
                    return EditorPersistenceResult::Rejected {
                        document: Some(save.document),
                        reason: EditorPersistenceRejection::StaleRevision,
                    };
                }
                self.last_persistence_receipt = Some(receipt);
                EditorPersistenceResult::Saved {
                    document: receipt.document(),
                    revision: receipt.revision(),
                    digest: receipt.digest(),
                }
            }
            SceneSaveOutcome::Rejected(reason) => EditorPersistenceResult::Rejected {
                document: Some(save.document),
                reason,
            },
            SceneSaveOutcome::Failed(stage) => EditorPersistenceResult::Failed {
                document: Some(save.document),
                stage,
            },
            SceneSaveOutcome::PersistenceUncertain {
                checkpoint,
                evidence,
            } => {
                self.last_persistence_receipt = Some(evidence);
                EditorPersistenceResult::PersistenceUncertain {
                    document: checkpoint.document,
                    revision: checkpoint.revision,
                    digest: checkpoint.digest,
                }
            }
        }
    }

    fn complete_reopen(&mut self, document: EditorDocumentId) -> EditorPersistenceResult {
        match self
            .persistence
            .reopen(self.plan.schema_validation().registry())
        {
            SceneReopenOutcome::Opened { session, digest } => {
                let report = if self.workspace.scene(document).is_some() {
                    match self
                        .workspace
                        .reload_external_session(Some(document), session)
                    {
                        Ok(report) => report,
                        Err(_) => {
                            return EditorPersistenceResult::Rejected {
                                document: Some(document),
                                reason: EditorPersistenceRejection::StaleRevision,
                            };
                        }
                    }
                } else if self.workspace.is_empty() {
                    match self
                        .workspace
                        .open_scene_session(self.scene_title.clone(), session)
                    {
                        Ok(report) => report,
                        Err(_) => {
                            return EditorPersistenceResult::Failed {
                                document: Some(document),
                                stage: EditorPersistenceFailureStage::Validate,
                            };
                        }
                    }
                } else {
                    return EditorPersistenceResult::Rejected {
                        document: Some(document),
                        reason: EditorPersistenceRejection::MissingDocument,
                    };
                };
                let opened_document = report.opened_document.unwrap_or(document);
                let revision = report
                    .revision
                    .expect("a reopened document reports its authoring revision");
                let binding = self
                    .workspace
                    .__bind_opened_source_digest(opened_document, digest);
                debug_assert!(binding.applied);
                self.source_document = opened_document;
                EditorPersistenceResult::Opened {
                    document: opened_document,
                    revision,
                    digest,
                }
            }
            SceneReopenOutcome::Rejected(reason) => EditorPersistenceResult::Rejected {
                document: Some(document),
                reason,
            },
            SceneReopenOutcome::Failed(stage) => EditorPersistenceResult::Failed {
                document: Some(document),
                stage,
            },
        }
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
