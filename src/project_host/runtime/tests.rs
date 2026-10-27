#[cfg(all(
    feature = "tooling",
    feature = "runtime-ui",
    feature = "presentation-ui"
))]
mod authoring_images;

#[cfg(feature = "tooling")]
mod browser_preparation;
#[cfg(feature = "tooling")]
mod closing_session;
mod content_residency;
mod current_scene;
#[cfg(feature = "tooling")]
mod document_sources;
mod editor_scene_travel;
#[cfg(all(
    feature = "tooling",
    feature = "presentation-2d",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
mod game_view_outcomes;
#[cfg(feature = "tooling")]
mod host_custody;
#[cfg(feature = "tooling")]
mod image_publication_custody;
#[cfg(feature = "tooling")]
mod owned_reopen;
#[cfg(feature = "tooling")]
mod owned_save;
#[cfg(all(feature = "tooling", feature = "runtime-ui"))]
mod prefab_revert;
mod project_scene_requests;
#[cfg(feature = "tooling")]
mod source_import_refresh;
#[cfg(feature = "tooling")]
mod task_facilities;

use std::{
    error::Error,
    fmt,
    fs::{self, File},
    io,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
    PluginDomainError, PluginError, PluginId, PluginInstantiationError, PluginPrepareError,
    PluginShutdownContext, PluginShutdownObligationId, RuntimeAdmissionReservation,
    RuntimeCloseContext, RuntimeCloseParticipant, RuntimeCloseParticipantError,
    RuntimeCloseParticipantId, RuntimeCloseProgress, RuntimeConstructionError, RuntimeFault,
    RuntimeFaultDetail, RuntimeFaultKind, RuntimeFaultReporter, StartupStage,
    drive_runtime_quarantine, runtime_quarantine_status,
};
#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
use nara_app::{RuntimeControl, RuntimeControlRequestResult};
use nara_asset::{
    AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId,
};
use nara_diagnostic::DiagnosticValueRef;
use nara_ecs::{Res, Resource, bevy_ecs::error::BevyError, schedule::IntoScheduleConfigs};
use nara_fs::{
    CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode,
};
use nara_gameplay::{GameplayCommandPlugin, GameplayCommandQueue};
use nara_identity::{EntityLookup, WorldIdentityDomain};
use nara_image::{ImageColorSpace, ImageImportSettings};
#[cfg(all(feature = "tooling", feature = "runtime-ui"))]
use nara_reflect::AuthoringSchemaBundleOwner;
#[cfg(feature = "tooling")]
use nara_reflect::ComponentFieldId;
use nara_reflect::{
    AuthoringSchemaBundle, ComponentRegistrySnapshot, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue, ComponentValueKind, PersistentComponentSchemaOwnerId,
};
#[cfg(feature = "tooling")]
use nara_scene::{AuthoringFieldEdit, SCENE_COMPONENTS_SCHEMA_OWNER_ID};
#[cfg(all(feature = "tooling", feature = "runtime-ui"))]
use nara_scene::{PrefabDocument, PrefabInstance, SCENE_COMPONENTS_SCHEMA_PROVIDER};
use nara_scene::{SceneComponentRecord, SceneDocument, SceneEntityRecord};
#[cfg(feature = "tooling")]
use nara_scene::{ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementOwner};
use nara_tasks::{
    TASK_PLUGIN_ID, TaskDomainKey, TaskHandle, TaskPoolKind, TaskPools, TaskSpawnRequest,
};
#[cfg(feature = "tooling")]
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorActionOutcome, EditorActionRejection,
    EditorAtomicFieldEdit, EditorDocumentMutation, EditorFieldWrite, EditorGestureAction,
    EditorGestureCancelReason, EditorGestureOutcome, EditorImageColorSpace,
    EditorImageImportAction, EditorImageImportActionOutcome, EditorImageImportActionRejection,
    EditorImageImportAttemptView, EditorImageImportAvailabilityView,
    EditorImageImportDetailRevision, EditorImageImportMetadataView,
    EditorImageImportSettings as ToolingImageImportSettings, EditorNavigationAction,
    EditorPersistenceAction, EditorProjectBrowserAction, EditorProjectBrowserActionOutcome,
    EditorProjectBrowserLocalAction, EditorProjectContentRefreshFailure, EditorProjectContentView,
    EditorWorkspaceIntent, EditorWorkspaceIntentAction,
};
#[cfg(feature = "tooling")]
use nara_tooling::{EditorPersistenceCommand, EditorPersistenceResult, EditorWorkspaceCommand};

use crate::project_content::{
    ProjectContentBudgetKind, ProjectContentLimits, ProjectContentLoader,
};
use crate::project_host::ProjectSettingsCandidate;
#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
use crate::project_host::advanced::{RuntimeProbe, desktop_run_with_probe};
use crate::project_host::advanced::{
    built_in_schema_providers, project_runtime_plugins, resolve_runtime_plan,
};
use crate::project_host::ingest_project_manifest;
use crate::project_host::runtime_agreement::prepare_runtime_schema_agreement;
use crate::startup_scene::{CurrentScene, CurrentSceneOwner, StartupSceneActivationSet};

use super::*;

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

const CLOSE_PROBE_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-close-probe");
const CLOSE_PROBE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.project-close-probe", 1);
const CLOSE_PROBE_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.test.project-close-probe");
const CLOSE_PROBE_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.test.project-close-probe");
const CLOSE_PROBE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CLOSE_PROBE_PLUGIN_ID, PluginCategory::Service)
        .shutdown_obligations(&[CLOSE_PROBE_OBLIGATION]);

const PREPARE_FAILURE_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-prepare-failure");
const PREPARE_FAILURE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.project-prepare-failure", 1);
const PREPARE_FAILURE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(PREPARE_FAILURE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[CLOSE_PROBE_PLUGIN_ID]);
const BUILD_FAILURE_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-build-failure");
const BUILD_FAILURE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.project-build-failure", 1);
const BUILD_FAILURE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(BUILD_FAILURE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[CLOSE_PROBE_PLUGIN_ID]);
const FINISH_FAILURE_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-finish-failure");
const FINISH_FAILURE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.project-finish-failure", 1);
const FINISH_FAILURE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(FINISH_FAILURE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[CLOSE_PROBE_PLUGIN_ID]);
const COMMAND_FAILURE_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-command-failure");
const COMMAND_FAILURE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.project-command-failure", 1);
const COMMAND_FAILURE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(COMMAND_FAILURE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[CLOSE_PROBE_PLUGIN_ID]);
const ADMISSION_SCOPE_FAILURE_PLUGIN_ID: PluginId =
    PluginId::new("nara.test.project-admission-scope-failure");
const ADMISSION_SCOPE_FAILURE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.project-admission-scope-failure", 1);
const ADMISSION_SCOPE_FAILURE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(ADMISSION_SCOPE_FAILURE_PLUGIN_ID, PluginCategory::Runtime);
const STARTUP_FAILURE_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-startup-failure");
const STARTUP_FAILURE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.project-startup-failure", 1);
const STARTUP_FAILURE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(STARTUP_FAILURE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[CLOSE_PROBE_PLUGIN_ID]);
const ACTIVATION_PROBE_PLUGIN_ID: PluginId = PluginId::new("nara.test.startup-activation-probe");
const ACTIVATION_PROBE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.startup-activation-probe", 1);
const ACTIVATION_PROBE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(ACTIVATION_PROBE_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[crate::startup_scene::STARTUP_SCENE_ACTIVATION_PLUGIN_ID]);
const CLASSIFIED_STARTUP_FAILURE_PLUGIN_ID: PluginId =
    PluginId::new("nara.test.classified-startup-failure");
const CLASSIFIED_STARTUP_FAILURE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.classified-startup-failure", 1);
const CLASSIFIED_STARTUP_FAILURE_DECLARATION: PluginDeclaration = PluginDeclaration::new(
    CLASSIFIED_STARTUP_FAILURE_PLUGIN_ID,
    PluginCategory::Runtime,
)
.requires_plugins(&[crate::startup_scene::STARTUP_SCENE_ACTIVATION_PLUGIN_ID]);
const RECIPE_STARTUP_FAILURE_PLUGIN_ID: PluginId =
    PluginId::new("nara.test.product-recipe-startup-failure");
const RECIPE_STARTUP_FAILURE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(RECIPE_STARTUP_FAILURE_PLUGIN_ID, PluginCategory::Runtime);
const BLOCKING_TASK_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-blocking-task");
const BLOCKING_TASK_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.project-blocking-task", 1);
const BLOCKING_TASK_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(BLOCKING_TASK_PLUGIN_ID, PluginCategory::Service)
        .requires_plugins(&[TASK_PLUGIN_ID]);
const CLOSE_FAILURE_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-close-failure");
const CLOSE_FAILURE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.project-close-failure", 1);
const CLOSE_FAILURE_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.test.project-close-failure");
const CLOSE_FAILURE_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.test.project-close-failure");
const CLOSE_FAILURE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(CLOSE_FAILURE_PLUGIN_ID, PluginCategory::Service)
        .requires_plugins(&[CLOSE_PROBE_PLUGIN_ID])
        .shutdown_obligations(&[CLOSE_FAILURE_OBLIGATION]);
const PANIC_STARTUP_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-startup-panic");
const PANIC_STARTUP_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.project-startup-panic", 1);
const PANIC_STARTUP_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(PANIC_STARTUP_PLUGIN_ID, PluginCategory::Runtime)
        .requires_plugins(&[CLOSE_PROBE_PLUGIN_ID]);
const RUNTIME_SERVICE_SESSION_PLUGIN_ID: PluginId =
    PluginId::new("nara.test.runtime-service-session");
const RUNTIME_SERVICE_SESSION_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.runtime-service-session", 1);
const RUNTIME_SERVICE_SESSION_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(RUNTIME_SERVICE_SESSION_PLUGIN_ID, PluginCategory::Service);

static NEXT_RUNTIME_SERVICE_SESSION_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Default)]
struct RuntimeServiceSessionPlugin;

impl Plugin for RuntimeServiceSessionPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &RUNTIME_SERVICE_SESSION_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(StartupStage::Runtime, publish_runtime_service_session_id)?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Resource)]
struct RuntimeServiceSessionId(u64);

fn publish_runtime_service_session_id(world: &mut nara_ecs::World) {
    let id = NEXT_RUNTIME_SERVICE_SESSION_ID
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .expect("test runtime service-session identities are exhausted");
    world.insert_resource(RuntimeServiceSessionId(id));
}

fn runtime_service_session_definition() -> PluginDefinition {
    PluginDefinition::infallible::<RuntimeServiceSessionPlugin, _>(
        RUNTIME_SERVICE_SESSION_DEFINITION_ID,
        b"runtime-service-session-v1",
        RuntimeServiceSessionPlugin::default,
    )
}

#[derive(Debug)]
struct CloseProbePlugin {
    builds: Arc<AtomicUsize>,
    closes: Arc<AtomicUsize>,
}

impl Plugin for CloseProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &CLOSE_PROBE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        self.builds.fetch_add(1, Ordering::SeqCst);
        app.register_plugin_runtime_close_participant(
            CLOSE_PROBE_OBLIGATION,
            CLOSE_PROBE_PARTICIPANT,
            CloseProbeParticipant {
                closes: Arc::clone(&self.closes),
            },
        )?;
        Ok(())
    }
}

struct CloseProbeParticipant {
    closes: Arc<AtomicUsize>,
}

impl RuntimeCloseParticipant for CloseProbeParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.closes.fetch_add(1, Ordering::SeqCst);
        Ok(RuntimeCloseProgress::Complete)
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        panic!("a close probe that completed during begin must not be polled")
    }
}

#[derive(Debug, Default)]
struct PrepareFailurePlugin;

impl Plugin for PrepareFailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &PREPARE_FAILURE_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        unreachable!("the prepare failure plugin is never constructed")
    }
}

#[derive(Debug, Default)]
struct BuildFailurePlugin;

impl Plugin for BuildFailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &BUILD_FAILURE_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Err(plugin_probe_error(BUILD_FAILURE_PLUGIN_ID))
    }
}

#[derive(Debug, Default)]
struct FinishFailurePlugin;

impl Plugin for FinishFailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &FINISH_FAILURE_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn finish(&self, _app: &mut App) -> Result<(), PluginError> {
        Err(plugin_probe_error(FINISH_FAILURE_PLUGIN_ID))
    }
}

#[derive(Debug, Default)]
struct CommandFailurePlugin;

impl Plugin for CommandFailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &COMMAND_FAILURE_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?.remove_resource::<GameplayCommandQueue>();
        Ok(())
    }
}

#[derive(Debug, Default)]
struct AdmissionScopeFailurePlugin;

impl Plugin for AdmissionScopeFailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &ADMISSION_SCOPE_FAILURE_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?.remove_resource::<RuntimeFaultReporter>();
        Ok(())
    }
}

#[derive(Debug, Default)]
struct StartupFailurePlugin;

impl Plugin for StartupFailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &STARTUP_FAILURE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(StartupStage::Runtime, fail_project_startup)?;
        Ok(())
    }
}

#[derive(Resource)]
struct ActivationProbe(Arc<AtomicUsize>);

#[derive(Debug)]
struct ActivationProbePlugin {
    observations: Arc<AtomicUsize>,
}

impl Plugin for ActivationProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &ACTIVATION_PROBE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(ActivationProbe(Arc::clone(&self.observations)))?
            .add_systems(
                StartupStage::Runtime,
                observe_startup_activation.in_set(StartupSceneActivationSet),
            )?;
        Ok(())
    }
}

fn observe_startup_activation(activation: CurrentScene<'_>, probe: Res<ActivationProbe>) {
    assert_eq!(
        activation.source().entities.len(),
        activation.receipt().len()
    );
    probe.0.fetch_add(1, Ordering::SeqCst);
}

#[derive(Debug, Default)]
struct ClassifiedStartupFailurePlugin;

impl Plugin for ClassifiedStartupFailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &CLASSIFIED_STARTUP_FAILURE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(
            StartupStage::Runtime,
            fail_classified_startup.in_set(StartupSceneActivationSet),
        )?;
        Ok(())
    }
}

fn fail_classified_startup(_activation: CurrentScene<'_>) -> Result<(), BevyError> {
    let detail = RuntimeFaultDetail::new(
        "nara.test.startup-configuration-invalid",
        "Startup configuration is invalid",
        "nara.test.product-startup",
    )
    .expect("test diagnostic metadata is valid");
    Err(detail.into_bevy_error())
}

#[derive(Debug, Default)]
struct RecipeStartupFailurePlugin;

impl Plugin for RecipeStartupFailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &RECIPE_STARTUP_FAILURE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(StartupStage::Runtime, fail_project_startup)?;
        Ok(())
    }
}

#[derive(Debug)]
struct ProjectStartupFailure;

impl fmt::Display for ProjectStartupFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("project startup failure probe")
    }
}

impl Error for ProjectStartupFailure {}

fn fail_project_startup() -> Result<(), BevyError> {
    Err(BevyError::error(ProjectStartupFailure))
}

#[derive(Debug)]
struct CloseFailurePlugin {
    control: Arc<CloseFailureControl>,
    report_fault: bool,
    fail_shutdown: bool,
}

impl Plugin for CloseFailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &CLOSE_FAILURE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.register_plugin_runtime_close_participant(
            CLOSE_FAILURE_OBLIGATION,
            CLOSE_FAILURE_PARTICIPANT,
            DelayedCloseParticipant {
                control: Arc::clone(&self.control),
            },
        )?;
        Ok(())
    }

    fn shutdown(&self, context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
        if self.report_fault {
            context
                .world()
                .resource::<RuntimeFaultReporter>()
                .report(RuntimeFault::engine(
                    RuntimeFaultKind::RequiredService,
                    "nara.test.project-close-fault",
                ));
        }
        if self.fail_shutdown {
            return Err(plugin_probe_error(CLOSE_FAILURE_PLUGIN_ID));
        }
        Ok(())
    }
}

#[derive(Debug)]
struct CloseFailureControl {
    released: std::sync::atomic::AtomicBool,
}

impl CloseFailureControl {
    fn new(released: bool) -> Self {
        Self {
            released: std::sync::atomic::AtomicBool::new(released),
        }
    }

    fn release(&self) {
        self.released.store(true, Ordering::Release);
    }
}

struct DelayedCloseParticipant {
    control: Arc<CloseFailureControl>,
}

impl RuntimeCloseParticipant for DelayedCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(if self.control.released.load(Ordering::Acquire) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }

    fn poll_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(if self.control.released.load(Ordering::Acquire) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }
}

#[derive(Debug, Default)]
struct PanicStartupPlugin;

impl Plugin for PanicStartupPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &PANIC_STARTUP_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(StartupStage::Runtime, panic_project_startup)?;
        Ok(())
    }
}

fn panic_project_startup() {
    panic!("injected project startup panic");
}

fn plugin_probe_error(plugin: PluginId) -> PluginError {
    PluginError::SetupFailed {
        plugin,
        message: "project Host phase failure probe".to_owned(),
    }
}

#[derive(Debug)]
struct BlockingTaskPlugin {
    control: Arc<BlockingTaskControl>,
}

impl Plugin for BlockingTaskPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &BLOCKING_TASK_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        self.control.instances.fetch_add(1, Ordering::SeqCst);
        let (started_sender, started_receiver) = mpsc::sync_channel(1);
        let (release_sender, release_receiver) = mpsc::channel();
        *self
            .control
            .release
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(release_sender);
        let handle = app
            .world()
            .get_resource::<TaskPools>()
            .ok_or_else(blocking_task_error)?
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(1, TaskDomainKey::new(24)),
                move |_| {
                    let _ = started_sender.send(());
                    let _ = release_receiver.recv();
                },
            )
            .into_handle()
            .map_err(|_| blocking_task_error())?;
        started_receiver
            .recv_timeout(Duration::from_secs(10))
            .map_err(|_| blocking_task_error())?;
        app.insert_resource(BlockingTaskHandle(handle))?;
        Ok(())
    }
}

#[derive(Debug, Default)]
struct BlockingTaskControl {
    instances: AtomicUsize,
    release: Mutex<Option<mpsc::Sender<()>>>,
}

impl BlockingTaskControl {
    fn release(&self) {
        self.release
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
            .expect("the task fault probe retains a release sender")
            .send(())
            .unwrap();
    }
}

#[derive(Resource)]
struct BlockingTaskHandle(#[allow(dead_code)] TaskHandle<()>);

fn blocking_task_error() -> PluginError {
    plugin_probe_error(BLOCKING_TASK_PLUGIN_ID)
}

#[test]
fn project_host_rejects_a_runtime_plan_from_another_project_lineage() {
    let first = TestProject::new("first-lineage");
    let second = TestProject::new("second-lineage");
    let (first_snapshot, _first_plan) = first.snapshot_and_plan(false);
    let (_second_snapshot, second_plan) = second.snapshot_and_plan(false);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());

    let Err(error) = host.begin_start(first_snapshot, second_plan, Vec::new()) else {
        panic!("cross-lineage start unexpectedly succeeded");
    };

    assert_eq!(
        first_code(&error.diagnostics),
        "project.run.lineage-mismatch"
    );
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
}

#[test]
fn project_host_rejects_content_without_runtime_schema_agreement() {
    let project = TestProject::new("missing-runtime-agreement");
    let root = project.capability();
    let manifest = root
        .open_file(&RelativePath::new(PROJECT_MANIFEST).unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    drop(manifest);
    let plan = resolve_runtime_plan(
        &candidate,
        project_runtime_plugins(&candidate),
        built_in_schema_providers(),
    )
    .unwrap();
    let snapshot = ProjectContentLoader::new(root)
        .unwrap()
        .load(&candidate, &plan)
        .unwrap();
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());

    let Err(error) = host.begin_start(snapshot, plan, Vec::new()) else {
        panic!("runtime content without schema agreement unexpectedly started");
    };

    assert_eq!(
        first_code(&error.diagnostics),
        "project.run.runtime-schema-agreement-missing"
    );
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
}

#[test]
fn project_host_rejects_a_different_schema_plan_from_the_same_lineage() {
    let project = TestProject::new("schema-lineage");
    let (snapshot, default_plan, equivalent_plan, reduced_plan) =
        project.snapshot_and_schema_plans();
    assert_eq!(snapshot.lineage(), reduced_plan.lineage());
    assert_ne!(
        default_plan.schema_validation().composition_fingerprint(),
        reduced_plan.schema_validation().composition_fingerprint()
    );
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());

    let Err(error) = host.begin_start(snapshot.clone(), reduced_plan, Vec::new()) else {
        panic!("cross-schema start unexpectedly succeeded");
    };

    assert_eq!(
        first_code(&error.diagnostics),
        "project.run.schema-mismatch"
    );
    assert!(matches!(host.slot, ProjectHostSlot::Empty));

    assert_eq!(
        default_plan.schema_validation().composition_fingerprint(),
        equivalent_plan
            .schema_validation()
            .composition_fingerprint()
    );
    assert!(
        !default_plan
            .schema_validation()
            .snapshot()
            .ptr_eq(equivalent_plan.schema_validation().snapshot())
    );
    let Err(error) = host.begin_start(snapshot, equivalent_plan, Vec::new()) else {
        panic!("equal but independently constructed schema authority unexpectedly succeeded");
    };
    assert_eq!(
        first_code(&error.diagnostics),
        "project.run.schema-mismatch"
    );
}

#[cfg(feature = "tooling")]
#[test]
fn editor_no_play_workflow_keeps_one_plan_owned_schema_snapshot() {
    let project = TestProject::new("editor-schema-snapshot");
    let mut editor = super::editor::EditorProjectSession::open_trusted_compiled(
        project.capability(),
        super::editor::TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let expected = editor.test_schema_snapshot();
    assert_eq!(
        editor.play_view().state(),
        nara_tooling::EditorPlayState::Empty
    );

    let _ = editor.workspace_model();
    assert!(expected.ptr_eq(&editor.test_schema_snapshot()));
    let document = editor.workspace().active_document().unwrap();
    let base_revision = editor.workspace().scene(document).unwrap().revision();
    let report = editor.apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
        document,
        base_revision,
        patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
            entity: SceneEntityRecord::new(nara_identity::SceneEntityId::new("edited").unwrap()),
            placement: ScenePlacement::last(ScenePlacementOwner::Root),
        }]),
    });
    assert!(report.applied);
    assert!(expected.ptr_eq(&editor.test_schema_snapshot()));

    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Reopen {
            document: Some(document),
        }),
        nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
    ));
    drive_persistence_to_terminal(&mut editor);
    assert!(matches!(
        editor.persistence_view().result(),
        Some(EditorPersistenceResult::Opened { .. })
    ));
    assert!(expected.ptr_eq(&editor.test_schema_snapshot()));
    assert_eq!(
        editor.play_view().state(),
        nara_tooling::EditorPlayState::Empty
    );
    assert_eq!(editor.play_view().generation(), None);
}

#[cfg(feature = "tooling")]
#[test]
fn trusted_compiled_editor_rejects_a_bundle_missing_selected_native_owners() {
    let project = TestProject::new("editor-runtime-agreement");
    fs::write(
        project.root.join("schema/authoring.bundle.json"),
        AuthoringSchemaBundle::new([])
            .unwrap()
            .to_json_string()
            .unwrap(),
    )
    .unwrap();

    let Err(error) = super::editor::EditorProjectSession::open_trusted_compiled(
        project.capability(),
        super::editor::TrustedCompiledEditorProjectIntent::new(),
    ) else {
        panic!("trusted compiled Editor accepted a bundle missing selected native owners");
    };

    assert_eq!(
        first_code(error.diagnostics()),
        "project.runtime-agreement.selected-owner-missing"
    );
}

#[cfg(all(feature = "tooling", feature = "runtime-ui"))]
#[test]
fn editor_prefab_lane_saves_and_reopens_the_exact_source_document() {
    let project = TestProject::new("editor-prefab-persistence");
    fs::write(
        project.root.join("nara.toml"),
        r#"schema_version = 1

[project]
source_universe = 'c60a6da4-b6a7-5028-bc53-39111af1a2ba'
native_document_identity = 'project_location'
name = "Editor Prefab Persistence"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[authoring]
schema_bundle = "schema/authoring.bundle.json"

[runtime]
preset = "local-headless"

[capabilities]
requested = ["runtime-2d", "runtime-ui"]

[ui.primary_surface]
logical_width = 960
logical_height = 540
"#,
    )
    .unwrap();
    let prefab_entity = nara_identity::SceneEntityId::new("retry").unwrap();
    let prefab_anchor = nara_identity::SceneEntityId::new("ui-anchor").unwrap();
    let plain_anchor = nara_identity::SceneEntityId::new("plain-anchor").unwrap();
    let image_settings = ImageImportSettings::new(ImageColorSpace::Linear);
    let image_path = "textures/strict-refresh.png";
    let image_bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
    )
    .unwrap();
    project.write_canonical_image(
        image_path,
        "5c5773fd-a572-4555-8d90-f9fd60c7f676",
        image_settings,
        &image_bytes,
    );
    fs::write(
        project.root.join("prefabs/reference-ui.prefab.json"),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(prefab_entity.clone())
            .with_component(
                ComponentTypeId::new("nara.ui.UiRoot"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    ComponentValue::map([("layer", ComponentValue::String("base".to_owned()))]),
                ),
            )
            .with_component(
                ComponentTypeId::new("nara.transform.Transform2d"),
                identity_transform_record(),
            )])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    fs::write(
        project.root.join("prefabs/plain.prefab.json"),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
            nara_identity::SceneEntityId::new("plain").unwrap(),
        )])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let mut anchor = SceneEntityRecord::new(prefab_anchor.clone());
    anchor.prefab = Some(PrefabInstance {
        source: AssetRef::path("reference-ui.prefab.json").unwrap(),
        overrides: None,
    });
    let mut plain = SceneEntityRecord::new(plain_anchor.clone());
    plain.prefab = Some(PrefabInstance {
        source: AssetRef::path("plain.prefab.json").unwrap(),
        overrides: None,
    });
    let image = SceneEntityRecord::new(nara_identity::SceneEntityId::new("strict-image").unwrap())
        .with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            identity_transform_record(),
        )
        .with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                sprite_value(&AssetRef::path(image_path).unwrap()),
            ),
        );
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        SceneDocument::from_ordered_roots([anchor, plain, image])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut editor = super::editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        super::editor::TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    assert_eq!(
        editor.editor_authoring_image_catalog_facts(),
        (1, vec![(image_path.to_owned(), image_settings)])
    );
    let scene_document = editor.workspace().active_document().unwrap();
    let frame = editor.current_frame();
    let scene_revision = frame.workspace().document(scene_document).unwrap().revision;
    let non_ui_report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: scene_document,
                    entity: Some(plain_anchor.clone()),
                }),
                EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                    document: scene_document,
                    base_revision: scene_revision,
                    entity: plain_anchor,
                }),
            ],
        ))
        .unwrap();
    let EditorActionOutcome::Workspace(non_ui) = &non_ui_report.outcomes()[1] else {
        panic!("ordinary Prefab navigation must publish a workspace outcome");
    };
    assert!(non_ui.applied, "{:?}", non_ui.diagnostics);
    assert_eq!(editor.workspace().len(), 2);
    let ordinary_document = non_ui.opened_document.unwrap();
    assert_eq!(
        editor.workspace().document_kind(ordinary_document),
        Some(nara_tooling::EditorDocumentKind::Prefab)
    );
    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument {
            document: ordinary_document
        }),
        nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
    );
    assert!(editor.workspace().prefab(ordinary_document).is_none());
    let _ = editor.acknowledge_workspace_intent_result();
    assert_eq!(editor.workspace().len(), 1);

    let frame = editor.drive_editor_frame(Duration::ZERO);
    let open_report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: scene_document,
                    entity: Some(prefab_anchor.clone()),
                }),
                EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                    document: scene_document,
                    base_revision: scene_revision,
                    entity: prefab_anchor.clone(),
                }),
            ],
        ))
        .unwrap();
    let EditorActionOutcome::Workspace(opened) = &open_report.outcomes()[1] else {
        panic!("opening a Prefab source must return a workspace publication outcome");
    };
    assert!(opened.applied, "{:?}", opened.diagnostics);
    let prefab_document = opened.opened_document.unwrap();

    assert!(editor.workspace().scene(scene_document).is_some());
    assert_eq!(
        editor.workspace().document_kind(prefab_document),
        Some(nara_tooling::EditorDocumentKind::Prefab)
    );
    assert_eq!(editor.workspace().active_document(), Some(prefab_document));
    assert_eq!(
        editor
            .workspace()
            .prefab(prefab_document)
            .unwrap()
            .source_binding()
            .locator(),
        "reference-ui.prefab.json"
    );

    let authored_frame = editor.drive_editor_frame(Duration::ZERO);
    let opened_revision = authored_frame
        .workspace()
        .document(prefab_document)
        .unwrap()
        .revision;
    let edit_report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            authored_frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: prefab_document,
                    entity: Some(prefab_entity),
                }),
                EditorAction::Mutate(EditorDocumentMutation::ApplyPrefabPatch {
                    document: prefab_document,
                    base_revision: opened_revision,
                    patch: transform_x_patch("retry", 12.0),
                }),
            ],
        ))
        .unwrap();
    assert!(edit_report.outcomes().iter().all(|outcome| {
        matches!(outcome, EditorActionOutcome::Workspace(report) if report.applied)
    }));

    let edited_frame = editor.drive_editor_frame(Duration::ZERO);
    let edited_revision = edited_frame
        .workspace()
        .document(prefab_document)
        .unwrap()
        .revision;
    let save_report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            edited_frame.revision(),
            [EditorAction::Persistence(EditorPersistenceAction::Save {
                document: prefab_document,
                base_revision: edited_revision,
            })],
        ))
        .unwrap();
    assert!(matches!(
        save_report.outcomes()[0],
        EditorActionOutcome::Persistence(
            nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
        )
    ));
    let saved_frame = drive_persistence_to_terminal(&mut editor);
    assert!(matches!(
        editor.persistence_view().result(),
        Some(EditorPersistenceResult::Saved { document, .. }) if document == prefab_document
    ));
    assert!(
        !editor
            .workspace()
            .prefab(prefab_document)
            .unwrap()
            .is_dirty()
    );

    let last_good_content = editor.test_project_content_revision();
    assert!(matches!(
        saved_frame.project_content(),
        EditorProjectContentView::RefreshPending {
            document,
            revision,
            ..
        } if document == prefab_document && revision == edited_revision
    ));
    let accepted_source = fs::read(project.root.join("prefabs/reference-ui.prefab.json")).unwrap();
    let mut externally_changed = accepted_source.clone();
    externally_changed.extend_from_slice(b"\n");
    fs::write(
        project.root.join("prefabs/reference-ui.prefab.json"),
        externally_changed,
    )
    .unwrap();

    let stale_frame = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        stale_frame.project_content(),
        EditorProjectContentView::Stale {
            failure: EditorProjectContentRefreshFailure::SavedSourceChanged,
            ..
        }
    ));
    assert_eq!(editor.test_project_content_revision(), last_good_content);
    assert_eq!(
        editor.editor_authoring_image_catalog_facts(),
        (1, vec![(image_path.to_owned(), image_settings)])
    );
    assert_eq!(
        editor
            .test_project_content_budget_snapshot()
            .active_generations(),
        1
    );
    assert_eq!(
        editor.request_play(nara_tooling::EditorPlayCommand::Play),
        nara_tooling::EditorPlayRequestResult::Rejected(
            nara_tooling::EditorPlayRejection::RuntimeOutOfDate,
        )
    );

    fs::write(
        project.root.join("prefabs/reference-ui.prefab.json"),
        accepted_source,
    )
    .unwrap();
    let reconcile = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            stale_frame.revision(),
            [
                EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
                    attempt: stale_frame
                        .persistence()
                        .attempt()
                        .expect("the observed persistence result is terminal"),
                }),
                EditorAction::Persistence(EditorPersistenceAction::Reopen {
                    target: stale_frame.reopen_target(),
                }),
            ],
        ))
        .unwrap();
    assert!(matches!(
        reconcile.outcomes()[0],
        EditorActionOutcome::Persistence(
            nara_tooling::EditorPersistenceRequestResult::Acknowledged { .. }
        )
    ));
    assert!(matches!(
        reconcile.outcomes()[1],
        EditorActionOutcome::Persistence(
            nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
        )
    ));
    let reconciled_frame = drive_persistence_to_terminal(&mut editor);
    assert!(matches!(
        reconciled_frame.project_content(),
        EditorProjectContentView::RefreshPending { .. }
    ));
    let acknowledged = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            reconciled_frame.revision(),
            [EditorAction::Persistence(
                EditorPersistenceAction::AcknowledgeResult {
                    attempt: reconciled_frame
                        .persistence()
                        .attempt()
                        .expect("the observed persistence result is terminal"),
                },
            )],
        ))
        .unwrap();
    assert!(matches!(
        acknowledged.outcomes()[0],
        EditorActionOutcome::Persistence(
            nara_tooling::EditorPersistenceRequestResult::Acknowledged { .. }
        )
    ));
    let mut refreshed_frame = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        refreshed_frame.project_content(),
        EditorProjectContentView::Current { generation } if generation.get() == 2
    ));
    assert_eq!(
        editor.editor_authoring_image_catalog_facts(),
        (2, vec![(image_path.to_owned(), image_settings)])
    );
    let retirement_deadline = std::time::Instant::now() + Duration::from_secs(5);
    while refreshed_frame.authoring_retirement_pending() {
        assert!(
            std::time::Instant::now() < retirement_deadline,
            "the authoring predecessor did not retire"
        );
        refreshed_frame = editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let budget = editor.test_project_content_budget_snapshot();
    assert_eq!(budget.active_generations(), 1);
    assert_eq!(budget.generation_high_water(), 2);

    let saved_revision = refreshed_frame
        .workspace()
        .document(prefab_document)
        .unwrap()
        .revision;
    let successor_edit = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            refreshed_frame.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyPrefabPatch {
                    document: prefab_document,
                    base_revision: saved_revision,
                    patch: transform_x_patch("retry", 24.0),
                },
            )],
        ))
        .unwrap();
    assert!(matches!(
        &successor_edit.outcomes()[0],
        EditorActionOutcome::Workspace(report) if report.applied
    ));

    let dirty_frame = editor.drive_editor_frame(Duration::ZERO);
    let reopen_report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            dirty_frame.revision(),
            [EditorAction::Persistence(EditorPersistenceAction::Reopen {
                target: dirty_frame.reopen_target(),
            })],
        ))
        .unwrap();
    assert!(matches!(
        reopen_report.outcomes()[0],
        EditorActionOutcome::Persistence(
            nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
        )
    ));
    let reopened_frame = drive_persistence_to_terminal(&mut editor);
    assert!(matches!(
        editor.persistence_view().result(),
        Some(EditorPersistenceResult::Opened { document, .. }) if document == prefab_document
    ));
    let reopened = editor.workspace().prefab(prefab_document).unwrap();
    assert!(!reopened.is_dirty());
    assert!(reopened.selection().is_empty());
    assert_eq!(reopened.session().history_status().undo_depth, 0);
    assert_eq!(reopened.session().history_status().redo_depth, 0);
    assert_ne!(reopened.revision().source_id(), opened_revision.source_id());

    let close_report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            reopened_frame.revision(),
            [EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseDocument {
                    document: prefab_document,
                }),
            )],
        ))
        .unwrap();
    assert!(matches!(
        close_report.outcomes()[0],
        EditorActionOutcome::WorkspaceIntent(
            nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
        )
    ));
    assert!(editor.workspace().prefab(prefab_document).is_none());
    assert!(editor.workspace().scene(scene_document).is_some());

    let closed_frame = editor.drive_editor_frame(Duration::ZERO);
    let scene_revision = closed_frame
        .workspace()
        .document(scene_document)
        .unwrap()
        .revision;
    let reopen_source = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            closed_frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::OpenPrefabSource {
                    document: scene_document,
                    base_revision: scene_revision,
                    entity: prefab_anchor,
                },
            )],
        ))
        .unwrap();
    assert!(matches!(
        &reopen_source.outcomes()[0],
        EditorActionOutcome::Workspace(report) if report.applied
    ));

    let exit_frame = editor.drive_editor_frame(Duration::ZERO);
    let retained_document_count = editor.workspace().len();
    let exit = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            exit_frame.revision(),
            [
                EditorAction::Persistence(EditorPersistenceAction::AcknowledgeResult {
                    attempt: exit_frame
                        .persistence()
                        .attempt()
                        .expect("the observed persistence result is terminal"),
                }),
                EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::AcknowledgeResult),
                EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(
                    EditorWorkspaceIntent::Exit,
                )),
            ],
        ))
        .unwrap();
    assert!(matches!(
        exit.outcomes()[2],
        EditorActionOutcome::WorkspaceIntent(
            nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
        )
    ));
    for _ in 0..8 {
        if editor.workspace_intent_view().phase()
            == Some(nara_tooling::EditorWorkspaceIntentPhase::Prepared)
        {
            break;
        }
        editor.drive_editor_frame(Duration::ZERO);
    }
    assert_eq!(
        editor.workspace_intent_view().phase(),
        Some(nara_tooling::EditorWorkspaceIntentPhase::Prepared)
    );
    assert_eq!(editor.workspace().len(), retained_document_count);
    let commit_frame = editor.drive_editor_frame(Duration::ZERO);
    let committed = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            commit_frame.revision(),
            [EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::CommitPrepared,
            )],
        ))
        .unwrap();
    assert!(matches!(
        committed.outcomes()[0],
        EditorActionOutcome::WorkspaceIntent(
            nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
        )
    ));
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(editor.workspace().len(), retained_document_count);
    assert!(matches!(
        editor.workspace_intent_view().result(),
        Some(nara_tooling::EditorWorkspaceIntentResult::Applied {
            intent: EditorWorkspaceIntent::Exit,
        })
    ));
}

#[cfg(feature = "tooling")]
#[test]
fn generic_editor_drives_the_initial_source_index_and_retains_it_during_refresh() {
    let project = TestProject::new("generic-source-index");
    let baseline_runtime_digest = project.snapshot_and_plan(false).0.content_digest();
    fs::create_dir_all(project.root.join("scripts")).unwrap();
    fs::write(
        project.root.join("scenes/external.scene.json"),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(project.root.join("scripts/player.rs"), "fn player() {}\n").unwrap();
    let browser_only_runtime_digest = project.snapshot_and_plan(false).0.content_digest();
    assert_eq!(browser_only_runtime_digest, baseline_runtime_digest);

    let mut host = super::editor::EditorProjectOpenHost::new();
    let mut attempt = host.begin(project.writable_capability(), None);
    let open_deadline = Instant::now() + Duration::from_secs(5);
    let mut editor = loop {
        match host.advance(attempt) {
            super::editor::EditorProjectOpenProgress::Pending(next) => {
                assert!(
                    Instant::now() < open_deadline,
                    "project Open did not settle"
                );
                std::thread::sleep(Duration::from_millis(1));
                attempt = next;
            }
            super::editor::EditorProjectOpenProgress::Opened(session) => break session,
            super::editor::EditorProjectOpenProgress::Rejected(failure) => panic!(
                "the generic project should open before indexing: {:?}",
                failure.diagnostics()
            ),
        }
    };

    assert_eq!(
        editor.project_source_index_kind(),
        crate::project_host::authoring::ProjectSourceIndexStateKind::Scanning
    );
    assert!(editor.project_source_index_work_pending());
    assert!(editor.project_source_index_snapshot().is_none());
    let index_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < index_deadline {
        if !editor.project_source_index_work_pending() {
            break;
        }
        editor.drive_editor_frame(Duration::ZERO);
        if editor.project_source_index_work_pending() {
            assert!(editor.project_source_index_snapshot().is_none());
        }
        std::thread::sleep(Duration::from_millis(1));
    }

    assert_eq!(
        editor.project_source_index_kind(),
        crate::project_host::authoring::ProjectSourceIndexStateKind::Ready
    );
    assert!(!editor.project_source_index_work_pending());
    assert!(editor.project_source_index_failure().is_none());
    let first = editor.project_source_index_snapshot().unwrap();
    assert_eq!(first.generation().get(), 1);
    assert!(first.entry("scenes/external.scene.json").is_some());
    assert!(first.entry("scripts/player.rs").is_some());

    assert!(editor.request_project_source_index_refresh());
    assert!(editor.project_source_index_work_pending());
    assert!(Arc::ptr_eq(
        &first,
        &editor.project_source_index_snapshot().unwrap()
    ));
    let refresh_gate = editor
        .hold_next_source_index_scan(crate::project_host::authoring::ScanTestPhase::BeforeBegin);
    editor.drive_editor_frame(Duration::ZERO);
    assert!(refresh_gate.wait_until_started());
    assert!(Arc::ptr_eq(
        &first,
        &editor.project_source_index_snapshot().unwrap()
    ));
    refresh_gate.release();
    let index_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < index_deadline {
        if !editor.project_source_index_work_pending() {
            break;
        }
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let second = editor.project_source_index_snapshot().unwrap();
    assert_eq!(second.generation().get(), 2);
    assert!(!Arc::ptr_eq(&first, &second));
    assert_eq!(second.root_identity(), first.root_identity());
    assert_eq!(second.manifest_identity(), first.manifest_identity());
    assert_eq!(second.manifest_digest(), first.manifest_digest());
    assert_eq!(second.bundle_identity(), first.bundle_identity());
    assert_eq!(second.bundle_digest(), first.bundle_digest());
    assert_eq!(second.effective_roots(), first.effective_roots());
    assert_eq!(second.entries(), first.entries());
    assert_eq!(second.diagnostics(), first.diagnostics());
    assert_eq!(second.usage(), first.usage());
    assert_eq!(second.completion(), first.completion());
}

#[cfg(feature = "tooling")]
#[test]
fn generic_editor_image_import_publishes_metadata_and_preserves_last_good_on_failure() {
    let project = TestProject::new("generic-image-import");
    fs::create_dir_all(project.root.join("assets/textures")).unwrap();
    let source_bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
    )
    .unwrap();
    fs::write(
        project.root.join("assets/textures/imported.png"),
        &source_bytes,
    )
    .unwrap();

    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    #[cfg(feature = "runtime-ui")]
    assert_eq!(
        editor.editor_authoring_image_catalog_facts(),
        (1, Vec::new())
    );
    let linear = ImageImportSettings::new(ImageColorSpace::Linear);
    let stale_frame = editor.current_frame();
    assert!(editor.request_project_source_index_refresh());
    drive_image_import_host_until_idle(&mut editor);
    assert!(matches!(
        editor.request_editor_image_import(
            stale_frame.revision(),
            "assets/textures/imported.png",
            linear,
        ),
        super::editor::EditorImageImportRequestResult::Rejected(
            super::editor::EditorImageImportFailure::StaleFrame
        )
    ));
    assert!(
        !project
            .root
            .join("assets/textures/imported.png.meta")
            .exists()
    );

    let frame = editor.current_frame();
    let import_basis_generation = editor.project_source_index_snapshot().unwrap().generation();
    let tasks_before_import = editor
        .test_authoring_task_stats()
        .for_kind(TaskPoolKind::Io);
    let started = editor.request_editor_image_import(
        frame.revision(),
        "assets/textures/imported.png",
        linear,
    );
    assert!(matches!(
        started,
        super::editor::EditorImageImportRequestResult::Started(_)
    ));
    assert!(editor.project_source_index_work_pending());
    let concurrent = editor.request_editor_image_import(
        frame.revision(),
        "assets/textures/imported.png",
        linear,
    );
    assert!(
        matches!(
            concurrent,
            super::editor::EditorImageImportRequestResult::Rejected(
                super::editor::EditorImageImportFailure::Busy
            )
        ),
        "a concurrent request returned {concurrent:?}"
    );
    let tasks_after_admission = editor
        .test_authoring_task_stats()
        .for_kind(TaskPoolKind::Io);
    assert_eq!(
        tasks_after_admission.admitted,
        tasks_before_import.admitted + 1
    );

    let first_state = drive_image_import_to_terminal(&mut editor);
    let tasks_after_completion = editor
        .test_authoring_task_stats()
        .for_kind(TaskPoolKind::Io);
    assert!(tasks_after_completion.completed > tasks_after_admission.completed);
    assert!(tasks_after_completion.taken > tasks_after_admission.taken);
    let (stable_id, first_version) = match first_state {
        super::editor::EditorImageImportState::Succeeded {
            stable_id, version, ..
        } => (stable_id, version),
        other => panic!("the initial image import did not succeed: {other:?}"),
    };
    drive_image_import_host_until_idle(&mut editor);
    #[cfg(feature = "runtime-ui")]
    assert_eq!(
        editor.editor_authoring_image_catalog_facts(),
        (2, vec![("textures/imported.png".to_owned(), linear)])
    );
    let refreshed_index = editor.project_source_index_snapshot().unwrap();
    assert!(refreshed_index.generation() > import_basis_generation);
    assert!(
        refreshed_index
            .entry("assets/textures/imported.png.meta")
            .is_some()
    );
    let asset_path = AssetPath::new("textures/imported.png").unwrap();
    let first_last_good = editor
        .editor_image_last_good(&asset_path)
        .cloned()
        .expect("a successful import publishes concrete last-good image facts");
    assert_eq!(first_last_good.stable_id(), stable_id);
    assert_eq!(first_last_good.version(), first_version);
    assert_eq!(
        first_last_good.source_digest(),
        nara_fs::ContentDigest::of_bytes(&source_bytes)
    );
    assert_eq!(first_last_good.settings_hash(), linear.settings_hash());

    let metadata_path = project.root.join("assets/textures/imported.png.meta");
    let metadata = nara_asset::AssetMetaCandidate::<ImageImportSettings>::decode_json_bytes(
        &fs::read(&metadata_path).unwrap(),
    )
    .unwrap();
    assert_eq!(metadata.identity().stable_id(), stable_id);
    assert_eq!(metadata.identity().path(), &asset_path);
    assert_eq!(metadata.settings(), &linear);

    editor.test_reject_next_valid_image_metadata_receipt();
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let srgb = ImageImportSettings::default();
    assert!(matches!(
        editor.request_editor_image_import(frame.revision(), "assets/textures/imported.png", srgb),
        super::editor::EditorImageImportRequestResult::Completed(
            super::editor::EditorImageImportState::ReconcileRequired { .. }
        )
    ));
    assert!(matches!(
        editor.request_editor_image_import(frame.revision(), "assets/textures/imported.png", srgb),
        super::editor::EditorImageImportRequestResult::Rejected(
            super::editor::EditorImageImportFailure::ReconcileRequired
        )
    ));
    assert_same_image_product(
        editor.editor_image_last_good(&asset_path).unwrap(),
        &first_last_good,
    );
    drive_image_import_host_until_idle(&mut editor);
    assert_same_image_product(
        editor.editor_image_last_good(&asset_path).unwrap(),
        &first_last_good,
    );
    #[cfg(feature = "runtime-ui")]
    assert_eq!(
        editor.editor_authoring_image_catalog_facts(),
        (2, vec![("textures/imported.png".to_owned(), linear)])
    );
    assert!(
        editor.apply_project_browser_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/textures/imported.png")),
        })
    );
    let settings_changed = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        settings_changed
            .image_import()
            .expect("settings drift retains the accepted last-good image")
            .availability(),
        EditorImageImportAvailabilityView::LastGood {
            stable_id: projected_id,
            version: projected_version,
            active_settings,
            freshness: nara_tooling::EditorImageImportFreshness::SettingsChanged,
            ..
        } if projected_id == stable_id
            && projected_version == first_version
            && active_settings
                == ToolingImageImportSettings::new(EditorImageColorSpace::Linear)
    ));

    fs::write(
        project.root.join("assets/textures/imported.png"),
        b"not a png",
    )
    .unwrap();
    let frame = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.request_editor_image_import(frame.revision(), "assets/textures/imported.png", srgb,),
        super::editor::EditorImageImportRequestResult::Started(_)
    ));
    let failed = drive_image_import_to_terminal(&mut editor);
    assert!(matches!(
        failed,
        super::editor::EditorImageImportState::Failed {
            failure: super::editor::EditorImageImportFailure::Runtime(
                super::editor::EditorImageRuntimeFailure::Import
            ),
            ..
        }
    ));
    assert_same_image_product(
        editor.editor_image_last_good(&asset_path).unwrap(),
        &first_last_good,
    );

    let desired = nara_asset::AssetMetaCandidate::<ImageImportSettings>::decode_json_bytes(
        &fs::read(&metadata_path).unwrap(),
    )
    .unwrap();
    assert_eq!(desired.identity().stable_id(), stable_id);
    assert_eq!(desired.settings(), &srgb);

    fs::write(
        project.root.join("assets/textures/imported.png"),
        &source_bytes,
    )
    .unwrap();
    drive_image_import_host_until_idle(&mut editor);
    let frame = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.request_editor_image_import(
            frame.revision(),
            "assets/textures/imported.png",
            linear,
        ),
        super::editor::EditorImageImportRequestResult::Started(_)
    ));
    let reimported = drive_image_import_to_terminal(&mut editor);
    let second_version = match reimported {
        super::editor::EditorImageImportState::Succeeded {
            stable_id: reimported_id,
            version,
            ..
        } => {
            assert_eq!(reimported_id, stable_id);
            assert!(version > first_version);
            version
        }
        other => panic!("the image reimport did not succeed: {other:?}"),
    };
    let second_last_good = editor
        .editor_image_last_good(&asset_path)
        .cloned()
        .expect("a successful reimport replaces the concrete last-good image facts");
    assert_eq!(second_last_good.stable_id(), stable_id);
    assert_eq!(second_last_good.version(), second_version);
    assert_eq!(second_last_good.settings_hash(), linear.settings_hash());
    drive_image_import_host_until_idle(&mut editor);

    let metadata_replacement = editor.test_hold_next_image_import();
    let frame = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.request_editor_image_import(frame.revision(), "assets/textures/imported.png", srgb,),
        super::editor::EditorImageImportRequestResult::Started(_)
    ));
    assert!(metadata_replacement.wait_until_started());
    let external_metadata = AssetMeta::new(
        AssetMetaIdentity::new(stable_id, asset_path.clone(), AssetSourceKind::Image),
        linear,
    )
    .to_json_string()
    .unwrap();
    fs::write(&metadata_path, external_metadata).unwrap();
    assert!(metadata_replacement.release());
    assert!(matches!(
        drive_image_import_to_terminal(&mut editor),
        super::editor::EditorImageImportState::Failed {
            failure: super::editor::EditorImageImportFailure::MetadataPublicationRejected,
            ..
        }
    ));
    assert_same_image_product(
        editor.editor_image_last_good(&asset_path).unwrap(),
        &second_last_good,
    );
    drive_image_import_host_until_idle(&mut editor);

    let source_replacement = editor.test_hold_next_image_import();
    let frame = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.request_editor_image_import(frame.revision(), "assets/textures/imported.png", srgb,),
        super::editor::EditorImageImportRequestResult::Started(_)
    ));
    assert!(source_replacement.wait_until_started());
    fs::write(
        project.root.join("assets/textures/imported.png"),
        b"source changed after the accepted metadata receipt",
    )
    .unwrap();
    assert!(source_replacement.release());
    assert!(matches!(
        drive_image_import_to_terminal(&mut editor),
        super::editor::EditorImageImportState::Failed {
            failure: super::editor::EditorImageImportFailure::Runtime(
                super::editor::EditorImageRuntimeFailure::SourceChanged
            ),
            ..
        }
    ));
    assert_same_image_product(
        editor.editor_image_last_good(&asset_path).unwrap(),
        &second_last_good,
    );

    fs::write(
        project.root.join("assets/textures/imported.png"),
        &source_bytes,
    )
    .unwrap();
    drive_image_import_host_until_idle(&mut editor);
    let cancellation = editor.test_hold_next_image_import();
    let budget = editor.test_image_import_budget_host();
    let retained_reservations = budget.snapshot().active_reservations();
    assert_eq!(retained_reservations, 1);
    let frame = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        editor.request_editor_image_import(
            frame.revision(),
            "assets/textures/imported.png",
            linear,
        ),
        super::editor::EditorImageImportRequestResult::Started(_)
    ));
    assert!(cancellation.wait_until_started());
    assert!(budget.snapshot().active_reservations() > retained_reservations);
    drop(editor);
    assert_eq!(budget.snapshot().active_reservations(), 0);

    fs::write(project.root.join("assets/non-image.bin"), b"asset payload").unwrap();
    let conflicting_metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            stable_id,
            AssetPath::new("non-image.bin").unwrap(),
            AssetSourceKind::Other("binary".to_owned()),
        ),
        (),
    )
    .to_json_string()
    .unwrap();
    fs::write(
        project.root.join("assets/non-image.bin.meta"),
        conflicting_metadata,
    )
    .unwrap();
    let mut successor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut successor);
    assert_eq!(
        successor.editor_image_import_state(),
        super::editor::EditorImageImportState::Idle
    );
    assert!(successor.editor_image_last_good(&asset_path).is_none());
    let frame = successor.drive_editor_frame(Duration::ZERO);
    let conflict = successor.request_editor_image_import(
        frame.revision(),
        "assets/textures/imported.png",
        linear,
    );
    assert!(
        matches!(
            conflict,
            super::editor::EditorImageImportRequestResult::Completed(
                super::editor::EditorImageImportState::Failed {
                    failure: super::editor::EditorImageImportFailure::MetadataInvalid,
                    ..
                }
            )
        ),
        "global Stable Asset ID conflict returned {conflict:?}"
    );
}

#[cfg(feature = "tooling")]
#[test]
fn generic_editor_image_import_projection_remains_available_in_authoring_degraded() {
    let project = TestProject::new("generic-image-import-degraded");
    let fixture =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/generic-editor/known-unbound");
    fs::copy(
        fixture.join("catalogs/authoring.bundle.json"),
        project.root.join("schema/authoring.bundle.json"),
    )
    .unwrap();
    fs::copy(
        fixture.join("worlds/entry.scene.json"),
        project.root.join("scenes/startup.scene.json"),
    )
    .unwrap();
    fs::create_dir_all(project.root.join("assets/textures")).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
        project.root.join("assets/textures/imported.png"),
    )
    .unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    assert_eq!(
        editor.project_readiness().authoring(),
        nara_tooling::EditorAuthoringReadiness::Degraded
    );
    assert!(
        editor.apply_project_browser_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/textures/imported.png")),
        })
    );

    let frame = editor.drive_editor_frame(Duration::ZERO);
    let view = frame
        .image_import()
        .expect("a selected PNG remains importable without Runtime authority");
    assert_eq!(view.relative_path(), "assets/textures/imported.png");
    assert_eq!(view.metadata(), EditorImageImportMetadataView::Missing);
    assert_eq!(
        view.availability(),
        EditorImageImportAvailabilityView::Unavailable
    );
    assert_eq!(view.attempt(), EditorImageImportAttemptView::Idle);
    assert!(view.can_submit());
    assert!(!frame.image_import_work_pending());
}

#[cfg(feature = "tooling")]
#[test]
fn generic_editor_image_import_action_submits_the_exact_selected_detail() {
    let project = TestProject::new("generic-image-import-exact-action");
    fs::create_dir_all(project.root.join("assets/textures")).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
        project.root.join("assets/textures/imported.png"),
    )
    .unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
        project.root.join("assets/textures/other.png"),
    )
    .unwrap();

    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    assert!(
        editor.apply_project_browser_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/textures/imported.png")),
        })
    );
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let settings = ToolingImageImportSettings::new(EditorImageColorSpace::Linear);
    let detail = frame
        .image_import()
        .expect("the selected PNG publishes an exact image detail");
    let original_detail_revision = detail.detail_revision();
    let action = detail.submit_action(settings);
    let held = editor.test_hold_next_image_import();

    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::ImageImport(action)],
        ))
        .unwrap();
    assert!(matches!(
        report.outcomes(),
        [EditorActionOutcome::ImageImport(
            EditorImageImportActionOutcome::Started { .. }
        )]
    ));
    assert!(matches!(
        editor.editor_image_import_state(),
        super::editor::EditorImageImportState::Pending { .. }
    ));
    assert!(held.wait_until_started());
    let metadata = nara_asset::AssetMetaCandidate::<ImageImportSettings>::decode_json_bytes(
        &fs::read(project.root.join("assets/textures/imported.png.meta")).unwrap(),
    )
    .unwrap();
    assert_eq!(
        metadata.settings(),
        &ImageImportSettings::new(ImageColorSpace::Linear)
    );
    let pending = editor.drive_editor_frame(Duration::ZERO);
    let pending_detail = pending
        .image_import()
        .expect("accepted metadata retains a safe detail while its catalog refresh is pending");
    assert_ne!(pending_detail.detail_revision(), original_detail_revision);
    assert!(matches!(
        pending_detail.metadata(),
        EditorImageImportMetadataView::Ready {
            committed_settings,
            ..
        } if committed_settings == settings
    ));
    assert!(matches!(
        pending_detail.attempt(),
        EditorImageImportAttemptView::Pending { .. }
    ));
    assert!(pending.image_import_work_pending());
    assert!(held.release());

    let (generation, stable_id, version) = match drive_image_import_to_terminal(&mut editor) {
        super::editor::EditorImageImportState::Succeeded {
            generation,
            stable_id,
            version,
        } => (generation.get(), stable_id, version),
        other => panic!("the exact image action did not succeed: {other:?}"),
    };
    drive_image_import_host_until_idle(&mut editor);
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let view = frame
        .image_import()
        .expect("the selected image retains its successful detail projection");
    assert_eq!(
        view.metadata(),
        EditorImageImportMetadataView::Ready {
            stable_id,
            committed_settings: settings,
        }
    );
    assert_eq!(
        view.attempt(),
        EditorImageImportAttemptView::Succeeded { generation }
    );
    assert!(matches!(
        view.availability(),
        EditorImageImportAvailabilityView::LastGood {
            stable_id: projected_id,
            version: projected_version,
            active_settings,
            dimensions,
            freshness: nara_tooling::EditorImageImportFreshness::Fresh,
            ..
        } if projected_id == stable_id
            && projected_version == version
            && active_settings == settings
            && dimensions.width() == 1
            && dimensions.height() == 1
    ));
    assert!(!frame.image_import_work_pending());

    assert!(
        editor.apply_project_browser_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/textures/other.png")),
        })
    );
    let other = editor.drive_editor_frame(Duration::ZERO);
    let other = other
        .image_import()
        .expect("another selected PNG publishes its own detail projection");
    assert_eq!(other.metadata(), EditorImageImportMetadataView::Missing);
    assert_eq!(other.attempt(), EditorImageImportAttemptView::Idle);
    assert_eq!(
        other.availability(),
        EditorImageImportAvailabilityView::Unavailable
    );

    fs::write(
        project.root.join("assets/textures/imported.png.meta"),
        b"{invalid-json",
    )
    .unwrap();
    assert!(editor.request_project_source_index_refresh());
    drive_image_import_host_until_idle(&mut editor);
    assert!(
        editor.apply_project_browser_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/textures/imported.png")),
        })
    );
    let invalid_metadata = editor.drive_editor_frame(Duration::ZERO);
    let invalid_metadata = invalid_metadata
        .image_import()
        .expect("invalid desired metadata remains visible as a safe read-only detail");
    assert!(matches!(
        invalid_metadata.metadata(),
        EditorImageImportMetadataView::Invalid {
            issue: nara_tooling::EditorImageImportMetadataIssue::InvalidFormat,
        }
    ));
    assert_eq!(
        invalid_metadata.availability(),
        EditorImageImportAvailabilityView::Unavailable
    );
    assert!(
        editor
            .editor_image_last_good(&AssetPath::new("textures/imported.png").unwrap())
            .is_none()
    );
}

#[cfg(feature = "tooling")]
#[test]
fn generic_editor_image_import_refresh_failure_retains_last_good_without_pending_work() {
    let project = TestProject::new("generic-image-import-refresh-failure");
    fs::create_dir_all(project.root.join("assets/textures")).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
        project.root.join("assets/textures/imported.png"),
    )
    .unwrap();

    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    assert!(
        editor.apply_project_browser_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/textures/imported.png")),
        })
    );
    let first_frame = editor.drive_editor_frame(Duration::ZERO);
    let first_action = first_frame
        .image_import()
        .expect("the selected PNG publishes an import detail")
        .submit_action(ToolingImageImportSettings::default());
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            first_frame.revision(),
            [EditorAction::ImageImport(first_action)],
        ))
        .unwrap();
    assert!(matches!(
        report.outcomes(),
        [EditorActionOutcome::ImageImport(
            EditorImageImportActionOutcome::Started { .. }
        )]
    ));
    let (stable_id, version) = match drive_image_import_to_terminal(&mut editor) {
        super::editor::EditorImageImportState::Succeeded {
            stable_id, version, ..
        } => (stable_id, version),
        other => panic!("the initial image import did not succeed: {other:?}"),
    };
    drive_image_import_host_until_idle(&mut editor);

    editor.test_reject_next_valid_image_metadata_receipt();
    let refresh_gate = editor.hold_next_source_index_scan(
        crate::project_host::authoring::ScanTestPhase::BeforeRevalidation,
    );
    let update_frame = editor.drive_editor_frame(Duration::ZERO);
    let update_action = update_frame
        .image_import()
        .expect("the accepted image remains reimportable")
        .submit_action(ToolingImageImportSettings::new(
            EditorImageColorSpace::Linear,
        ));
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            update_frame.revision(),
            [EditorAction::ImageImport(update_action)],
        ))
        .unwrap();
    assert!(matches!(
        report.outcomes(),
        [EditorActionOutcome::ImageImport(
            EditorImageImportActionOutcome::Completed {
                attempt: EditorImageImportAttemptView::ReconcileRequired { .. },
            }
        )]
    ));

    editor.drive_editor_frame(Duration::ZERO);
    assert!(refresh_gate.wait_until_started());
    fs::rename(
        project.root.join("assets"),
        project.root.join("assets-before-refresh"),
    )
    .unwrap();
    fs::create_dir(project.root.join("assets")).unwrap();
    refresh_gate.release();
    drive_image_import_host_until_idle(&mut editor);

    assert!(editor.project_source_index_failure().is_some());
    assert!(!editor.project_source_index_work_pending());
    assert!(!editor.editor_image_import_work_pending());
    let failed_refresh = editor.drive_editor_frame(Duration::ZERO);
    let detail = failed_refresh
        .image_import()
        .expect("failed refresh retains a safe last-good image detail");
    assert!(matches!(
        detail.metadata(),
        EditorImageImportMetadataView::Invalid {
            issue: nara_tooling::EditorImageImportMetadataIssue::MetadataUnavailable,
        }
    ));
    assert!(matches!(
        detail.availability(),
        EditorImageImportAvailabilityView::LastGood {
            stable_id: projected_id,
            version: projected_version,
            freshness: nara_tooling::EditorImageImportFreshness::DesiredStateUnavailable,
            ..
        } if projected_id == stable_id && projected_version == version
    ));
    assert!(matches!(
        detail.attempt(),
        EditorImageImportAttemptView::ReconcileRequired { .. }
    ));
    assert!(!detail.can_submit());
    assert!(!failed_refresh.image_import_work_pending());
    for _ in 0..3 {
        assert!(
            !editor
                .drive_editor_frame(Duration::ZERO)
                .image_import_work_pending()
        );
    }
}

#[cfg(feature = "tooling")]
#[test]
fn generic_editor_image_import_fallback_binds_freshness_and_attempt_to_the_selected_target() {
    let project = TestProject::new("generic-image-import-target-bound-fallback");
    fs::create_dir_all(project.root.join("assets/textures")).unwrap();
    for file_name in ["first.png", "second.png"] {
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
            project.root.join("assets/textures").join(file_name),
        )
        .unwrap();
    }

    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    assert!(
        editor.apply_project_browser_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/textures/first.png")),
        })
    );
    let first_frame = editor.drive_editor_frame(Duration::ZERO);
    let first_action = first_frame
        .image_import()
        .expect("the first PNG publishes an import detail")
        .submit_action(ToolingImageImportSettings::default());
    editor
        .dispatch_editor_actions(EditorActionBatch::new(
            first_frame.revision(),
            [EditorAction::ImageImport(first_action)],
        ))
        .unwrap();
    let (stable_id, version) = match drive_image_import_to_terminal(&mut editor) {
        super::editor::EditorImageImportState::Succeeded {
            stable_id, version, ..
        } => (stable_id, version),
        other => panic!("the first image import did not succeed: {other:?}"),
    };
    drive_image_import_host_until_idle(&mut editor);
    assert!(editor.request_project_source_index_refresh());
    drive_image_import_host_until_idle(&mut editor);
    let unverified = editor.drive_editor_frame(Duration::ZERO);
    assert!(matches!(
        unverified
            .image_import()
            .expect("a newer catalog cannot authenticate the previous source digest")
            .availability(),
        EditorImageImportAvailabilityView::LastGood {
            freshness: nara_tooling::EditorImageImportFreshness::SourceUnverified,
            ..
        }
    ));

    assert!(
        editor.apply_project_browser_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/textures/second.png")),
        })
    );
    let second_frame = editor.drive_editor_frame(Duration::ZERO);
    let second_action = second_frame
        .image_import()
        .expect("the second PNG publishes an import detail")
        .submit_action(ToolingImageImportSettings::default());
    fs::remove_file(project.root.join("assets/textures/second.png")).unwrap();
    let second_report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            second_frame.revision(),
            [EditorAction::ImageImport(second_action)],
        ))
        .unwrap();
    assert!(matches!(
        second_report.outcomes(),
        [EditorActionOutcome::ImageImport(
            EditorImageImportActionOutcome::Completed {
                attempt: EditorImageImportAttemptView::Failed { .. },
            }
        )]
    ));

    let refresh_gate = editor.hold_next_source_index_scan(
        crate::project_host::authoring::ScanTestPhase::BeforeRevalidation,
    );
    assert!(editor.request_project_source_index_refresh());
    editor.drive_editor_frame(Duration::ZERO);
    assert!(refresh_gate.wait_until_started());
    fs::rename(
        project.root.join("assets"),
        project.root.join("assets-before-target-bound-refresh"),
    )
    .unwrap();
    fs::create_dir(project.root.join("assets")).unwrap();
    refresh_gate.release();
    drive_image_import_host_until_idle(&mut editor);
    assert!(editor.project_source_index_failure().is_some());

    assert!(
        editor.apply_project_browser_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/textures/first.png")),
        })
    );
    let fallback = editor.drive_editor_frame(Duration::ZERO);
    let detail = fallback
        .image_import()
        .expect("the first image retains one bounded fallback detail");
    assert_eq!(detail.attempt(), EditorImageImportAttemptView::Idle);
    assert!(matches!(
        detail.availability(),
        EditorImageImportAvailabilityView::LastGood {
            stable_id: projected_id,
            version: projected_version,
            freshness: nara_tooling::EditorImageImportFreshness::DesiredStateUnavailable,
            ..
        } if projected_id == stable_id && projected_version == version
    ));
    assert!(!detail.can_submit());
}

#[cfg(feature = "tooling")]
#[test]
fn generic_editor_image_import_exact_source_conflict_overrides_stale_catalog_freshness() {
    let project = TestProject::new("generic-image-import-exact-source-conflict");
    fs::create_dir_all(project.root.join("assets/textures")).unwrap();
    let source = project.root.join("assets/textures/imported.png");
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
        &source,
    )
    .unwrap();

    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    assert!(
        editor.apply_project_browser_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/textures/imported.png")),
        })
    );
    let initial_frame = editor.drive_editor_frame(Duration::ZERO);
    let initial_action = initial_frame
        .image_import()
        .expect("the selected PNG publishes an import detail")
        .submit_action(ToolingImageImportSettings::default());
    editor
        .dispatch_editor_actions(EditorActionBatch::new(
            initial_frame.revision(),
            [EditorAction::ImageImport(initial_action)],
        ))
        .unwrap();
    let (stable_id, version) = match drive_image_import_to_terminal(&mut editor) {
        super::editor::EditorImageImportState::Succeeded {
            stable_id, version, ..
        } => (stable_id, version),
        other => panic!("the initial image import did not succeed: {other:?}"),
    };
    drive_image_import_host_until_idle(&mut editor);

    let catalog_generation = editor.project_source_index_snapshot().unwrap().generation();
    let reimport_frame = editor.drive_editor_frame(Duration::ZERO);
    let reimport_action = reimport_frame
        .image_import()
        .expect("the accepted image remains reimportable")
        .submit_action(ToolingImageImportSettings::default());
    fs::rename(
        &source,
        project.root.join("imported-before-exact-conflict.png"),
    )
    .unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
        &source,
    )
    .unwrap();

    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            reimport_frame.revision(),
            [EditorAction::ImageImport(reimport_action)],
        ))
        .unwrap();
    assert!(matches!(
        report.outcomes(),
        [EditorActionOutcome::ImageImport(
            EditorImageImportActionOutcome::Completed {
                attempt: EditorImageImportAttemptView::Stale { .. },
            }
        )]
    ));
    assert_eq!(
        editor.project_source_index_snapshot().unwrap().generation(),
        catalog_generation
    );

    let conflicted = editor.drive_editor_frame(Duration::ZERO);
    let detail = conflicted
        .image_import()
        .expect("the stale catalog still projects the selected last-good image");
    assert!(matches!(
        detail.attempt(),
        EditorImageImportAttemptView::Stale { .. }
    ));
    assert!(matches!(
        detail.availability(),
        EditorImageImportAvailabilityView::LastGood {
            stable_id: projected_id,
            version: projected_version,
            freshness: nara_tooling::EditorImageImportFreshness::SourceChanged,
            ..
        } if projected_id == stable_id && projected_version == version
    ));
}

#[cfg(feature = "tooling")]
#[test]
fn generic_editor_image_import_action_rejects_stale_detail_failure_atomically() {
    let project = TestProject::new("generic-image-import-stale-action");
    fs::create_dir_all(project.root.join("assets/textures")).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
        project.root.join("assets/textures/imported.png"),
    )
    .unwrap();

    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    assert!(
        editor.apply_project_browser_local_action(EditorProjectBrowserLocalAction::Select {
            relative_path: Some(Arc::from("assets/textures/imported.png")),
        })
    );
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let view = frame
        .image_import()
        .expect("the selected PNG publishes an exact image detail");
    let stale_revision =
        EditorImageImportDetailRevision::new(view.detail_revision().get().checked_add(1).unwrap());
    let action = EditorImageImportAction::Submit {
        relative_path: Arc::from(view.relative_path()),
        expected_detail_revision: stale_revision,
        settings: ToolingImageImportSettings::default(),
    };

    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::ImageImport(action)],
        ))
        .unwrap();
    assert_eq!(
        report.outcomes(),
        &[EditorActionOutcome::ImageImport(
            EditorImageImportActionOutcome::Rejected(EditorImageImportActionRejection::Stale)
        )]
    );
    assert_eq!(
        editor.editor_image_import_state(),
        super::editor::EditorImageImportState::Idle
    );
    assert!(
        !project
            .root
            .join("assets/textures/imported.png.meta")
            .exists()
    );
    assert!(
        editor
            .editor_image_last_good(&AssetPath::new("textures/imported.png").unwrap())
            .is_none()
    );
}

#[cfg(feature = "tooling")]
#[test]
fn idle_generic_editor_project_close_yields_progress_before_polling_workers() {
    let project = TestProject::new("editor-idle-close");
    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);

    let first = editor.advance_close();
    assert!(
        matches!(
            first,
            super::editor::EditorProjectCloseProgress::Incomplete { .. }
        ),
        "the first close advance must return before polling task retirement: {first:#?}"
    );
    let close = drive_editor_close_to_completion(&mut editor);

    assert!(
        matches!(
            &close,
            super::editor::EditorProjectCloseProgress::Complete(_)
        ),
        "an idle generic Editor session must close completely: {close:#?}"
    );
    assert!(close.task_shutdown().is_complete());
}

#[cfg(feature = "tooling")]
fn drive_persistence_to_terminal(
    editor: &mut super::editor::EditorProjectSession,
) -> Arc<nara_tooling::EditorFrameModel> {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        if frame.persistence().result().is_some() {
            return frame;
        }
        assert!(
            Instant::now() < deadline,
            "Editor persistence retained an unfinished operation"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[cfg(feature = "tooling")]
fn drive_editor_close_to_completion(
    editor: &mut super::editor::EditorProjectSession,
) -> super::editor::EditorProjectCloseProgress {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let progress = editor.advance_close();
        if matches!(
            progress,
            super::editor::EditorProjectCloseProgress::Complete(_)
        ) {
            return progress;
        }
        assert!(
            Instant::now() < deadline,
            "Editor close retained an unfinished owner: {progress:#?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[cfg(all(feature = "tooling", windows))]
#[test]
fn image_metadata_cleanup_blocks_new_publication_and_session_close_until_retry() {
    let project = TestProject::new("editor-image-cleanup-close");
    let textures = project.root.join("assets/textures");
    fs::create_dir_all(&textures).unwrap();
    fs::copy(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
        textures.join("imported.png"),
    )
    .unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    let entries = fs::read_dir(&textures).unwrap().count();
    let frame = editor.current_frame();
    editor.test_fail_next_image_metadata_after_write();
    assert!(matches!(
        editor.request_editor_image_import(
            frame.revision(),
            "assets/textures/imported.png",
            ImageImportSettings::default(),
        ),
        super::editor::EditorImageImportRequestResult::Completed(
            super::editor::EditorImageImportState::Failed {
                failure: super::editor::EditorImageImportFailure::CleanupPending,
                ..
            }
        )
    ));
    assert!(!textures.join("imported.png.meta").exists());
    assert_eq!(fs::read_dir(&textures).unwrap().count(), entries + 1);
    assert!(matches!(
        editor.request_editor_image_import(
            frame.revision(),
            "assets/textures/imported.png",
            ImageImportSettings::default(),
        ),
        super::editor::EditorImageImportRequestResult::Rejected(
            super::editor::EditorImageImportFailure::CleanupPending
        )
    ));
    assert_eq!(fs::read_dir(&textures).unwrap().count(), entries + 1);
    let deadline = Instant::now() + Duration::from_secs(5);
    let close = loop {
        let close = editor.advance_close();
        if close.task_shutdown().is_complete() {
            break close;
        }
        assert!(
            Instant::now() < deadline,
            "task retirement did not finish: {close:#?}"
        );
        std::thread::sleep(Duration::from_millis(1));
    };
    assert!(close.task_shutdown().is_complete());
    assert!(matches!(
        close,
        super::editor::EditorProjectCloseProgress::Incomplete {
            file_cleanup_pending: true,
            ..
        }
    ));
    editor.test_allow_image_publication_cleanup();
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        super::editor::EditorProjectCloseProgress::Complete(_)
    ));
    assert_eq!(fs::read_dir(&textures).unwrap().count(), entries);
}

#[cfg(all(feature = "tooling", windows))]
#[test]
fn scene_save_cleanup_blocks_session_close_until_retry() {
    let project = TestProject::new("editor-save-cleanup-close");
    let mut editor = super::editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        super::editor::TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let document = editor.workspace().active_document().unwrap();
    let base_revision = editor.workspace().scene(document).unwrap().revision();
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
                document,
                base_revision,
                patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                    entity: SceneEntityRecord::new(
                        nara_identity::SceneEntityId::new("unsaved").unwrap()
                    ),
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                }]),
            })
            .applied
    );
    editor.test_fail_next_save_after_write(document);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document),
        }),
        nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
    ));
    drive_persistence_to_terminal(&mut editor);
    assert!(matches!(
        editor.persistence_view().result(),
        Some(EditorPersistenceResult::Failed {
            stage: nara_tooling::EditorPersistenceFailureStage::WriteTemporary,
            ..
        })
    ));
    assert!(matches!(
        editor.advance_close(),
        super::editor::EditorProjectCloseProgress::Incomplete {
            file_cleanup_pending: true,
            ..
        }
    ));
    editor.test_allow_save_cleanup(document);
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        super::editor::EditorProjectCloseProgress::Complete(_)
    ));
}

#[cfg(feature = "tooling")]
#[test]
fn editor_project_close_retries_timed_out_image_task_owner_before_releasing_session() {
    let project = TestProject::new("editor-image-close-retry");
    fs::create_dir_all(project.root.join("assets/textures")).unwrap();
    let source_bytes = fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
    )
    .unwrap();
    fs::write(
        project.root.join("assets/textures/imported.png"),
        source_bytes,
    )
    .unwrap();

    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    let linear = ImageImportSettings::new(ImageColorSpace::Linear);
    let frame = editor.current_frame();
    assert!(matches!(
        editor.request_editor_image_import(
            frame.revision(),
            "assets/textures/imported.png",
            linear,
        ),
        super::editor::EditorImageImportRequestResult::Started(_)
    ));
    assert!(matches!(
        drive_image_import_to_terminal(&mut editor),
        super::editor::EditorImageImportState::Succeeded { .. }
    ));
    drive_image_import_host_until_idle(&mut editor);

    let budget = editor.test_image_import_budget_host();
    assert_eq!(budget.snapshot().active_reservations(), 1);
    let blocked = editor.test_hold_next_image_import_through_shutdown();
    let frame = editor.current_frame();
    assert!(matches!(
        editor.request_editor_image_import(
            frame.revision(),
            "assets/textures/imported.png",
            ImageImportSettings::default(),
        ),
        super::editor::EditorImageImportRequestResult::Started(_)
    ));
    assert!(blocked.wait_until_started());
    assert!(budget.snapshot().active_reservations() > 1);

    let incomplete = editor.advance_close();
    assert!(matches!(
        &incomplete,
        super::editor::EditorProjectCloseProgress::Incomplete { .. }
    ));
    assert!(!incomplete.task_shutdown().is_complete());
    assert!(!incomplete.task_shutdown().timed_out());
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let progress = editor.advance_close();
        assert!(matches!(
            progress,
            super::editor::EditorProjectCloseProgress::Incomplete { .. }
        ));
        if progress.task_shutdown().timed_out() {
            break;
        }
        assert!(
            Instant::now() < deadline,
            "the held task did not reach its close deadline"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(budget.snapshot().active_reservations() > 1);

    assert!(blocked.release());
    let complete = drive_editor_close_to_completion(&mut editor);
    assert!(matches!(
        &complete,
        super::editor::EditorProjectCloseProgress::Complete(_)
    ));
    assert!(complete.task_shutdown().is_complete());
    assert!(complete.task_shutdown().timed_out());
    assert!(
        complete
            .task_shutdown()
            .for_kind(TaskPoolKind::Io)
            .joined_workers
            > 0
    );
    assert!(
        complete
            .task_shutdown()
            .for_kind(TaskPoolKind::Io)
            .cancellation_requests
            > 0
    );
    assert_eq!(budget.snapshot().active_reservations(), 1);

    drop(editor);
    assert_eq!(budget.snapshot().active_reservations(), 0);
}

#[cfg(feature = "tooling")]
fn drive_image_import_to_terminal(
    editor: &mut super::editor::EditorProjectSession,
) -> super::editor::EditorImageImportState {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        let state = editor.editor_image_import_state();
        if !matches!(
            state,
            super::editor::EditorImageImportState::Idle
                | super::editor::EditorImageImportState::Pending { .. }
        ) {
            return state;
        }
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("the bounded image import did not reach a terminal state");
}

#[cfg(feature = "tooling")]
fn assert_same_image_product(
    actual: &super::editor::EditorImageLastGood,
    expected: &super::editor::EditorImageLastGood,
) {
    assert_eq!(actual.stable_id(), expected.stable_id());
    assert_eq!(actual.source_identity(), expected.source_identity());
    assert_eq!(actual.source_digest(), expected.source_digest());
    assert_eq!(actual.settings_hash(), expected.settings_hash());
    assert_eq!(actual.version(), expected.version());
}

#[cfg(feature = "tooling")]
fn drive_image_import_host_until_idle(editor: &mut super::editor::EditorProjectSession) {
    let deadline = Instant::now() + Duration::from_secs(2);
    while Instant::now() < deadline {
        if !editor.project_source_index_work_pending() && !editor.editor_image_import_work_pending()
        {
            return;
        }
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("the bounded image catalog or refresh did not settle");
}

#[cfg(feature = "tooling")]
fn open_generic_editor_with_ready_index(
    project: &TestProject,
) -> super::editor::EditorProjectSession {
    open_generic_editor_with_host(project, super::editor::EditorProjectOpenHost::new())
}

#[cfg(feature = "tooling")]
fn open_generic_editor_with_host(
    project: &TestProject,
    mut host: super::editor::EditorProjectOpenHost,
) -> super::editor::EditorProjectSession {
    let mut attempt = host.begin(project.writable_capability(), None);
    let open_deadline = Instant::now() + Duration::from_secs(5);
    let mut editor = loop {
        match host.advance(attempt) {
            super::editor::EditorProjectOpenProgress::Pending(next) => {
                assert!(
                    Instant::now() < open_deadline,
                    "project Open did not settle"
                );
                std::thread::sleep(Duration::from_millis(1));
                attempt = next;
            }
            super::editor::EditorProjectOpenProgress::Opened(session) => break session,
            super::editor::EditorProjectOpenProgress::Rejected(failure) => panic!(
                "the generic project should open before document admission: {:?}",
                failure.diagnostics()
            ),
        }
    };
    let index_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < index_deadline {
        if !editor.project_source_index_work_pending() {
            break;
        }
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!editor.project_source_index_work_pending());
    assert!(matches!(
        editor.project_source_index_kind(),
        crate::project_host::authoring::ProjectSourceIndexStateKind::Ready
            | crate::project_host::authoring::ProjectSourceIndexStateKind::ReadyWithDiagnostics
    ));
    assert!(editor.project_source_index_snapshot().is_some());
    editor
}

#[cfg(all(feature = "tooling", feature = "presentation-2d"))]
#[test]
fn generic_editor_preview_resolves_stable_image_after_source_rename() {
    let project = TestProject::with_image("generic-stable-image-preview");
    let stable_id = StableAssetId::parse_str("3c7c5be4-fd4e-4b65-b8d4-c671f5982186").unwrap();
    let mut value = sprite_value(&AssetRef::path("textures/player.png").unwrap());
    let ComponentValue::Map(fields) = &mut value else {
        unreachable!()
    };
    let ComponentValue::Map(material) = fields.get_mut("material").unwrap() else {
        unreachable!()
    };
    material.insert(
        "image".to_owned(),
        ComponentValue::map([
            ("kind", ComponentValue::String("stable_id".to_owned())),
            ("value", ComponentValue::String(stable_id.to_string())),
        ]),
    );
    let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        nara_identity::SceneEntityId::new("player").unwrap(),
    )
    .with_component(
        ComponentTypeId::new("nara.transform.Transform2d"),
        identity_transform_record(),
    )
    .with_component(
        ComponentTypeId::new("nara.sprite.Sprite"),
        SceneComponentRecord::new(ComponentSchemaVersion::ONE, value),
    )]);
    let authored_bytes = scene.to_json_string().unwrap();
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        &authored_bytes,
    )
    .unwrap();

    for path in ["textures/player.png", "textures/renamed-player.png"] {
        if path != "textures/player.png" {
            fs::rename(
                project.root.join("assets/textures/player.png"),
                project.root.join("assets").join(path),
            )
            .unwrap();
            fs::remove_file(project.root.join("assets/textures/player.png.meta")).unwrap();
            let metadata = AssetMeta::new(
                AssetMetaIdentity::new(
                    stable_id,
                    AssetPath::new(path).unwrap(),
                    AssetSourceKind::Image,
                ),
                ImageImportSettings::default(),
            );
            fs::write(
                project.root.join("assets").join(format!("{path}.meta")),
                metadata.to_json_string().unwrap(),
            )
            .unwrap();
        }
        let mut editor = open_generic_editor_with_ready_index(&project);
        drive_image_import_host_until_idle(&mut editor);
        editor.set_editor_preview_extent(Some(nara_render::Extent2d::new(640, 360).unwrap()));
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut previewed_player = false;
        let mut omitted = true;
        while Instant::now() < deadline {
            editor.drive_editor_frame(Duration::ZERO);
            if let Some(candidate) = editor.take_editor_preview_candidate() {
                previewed_player = candidate
                    .hits()
                    .iter()
                    .any(|hit| hit.entity().as_str() == "player");
                omitted = !candidate.preview_omissions().is_empty();
                if previewed_player {
                    break;
                }
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        let diagnostics = editor.current_frame().diagnostics().clone();
        let hydrated = editor
            .editor_image_last_good(&AssetPath::new(path).unwrap())
            .is_some();
        let close = drive_editor_close_to_completion(&mut editor);
        drop(editor);
        assert!(matches!(
            close,
            super::editor::EditorProjectCloseProgress::Complete(_)
        ));
        assert!(
            hydrated,
            "stable image did not hydrate at '{path}': {diagnostics:?}"
        );
        assert!(
            previewed_player && !omitted,
            "stable image did not materialize in generic preview at '{path}': {diagnostics:?}"
        );
        assert_eq!(
            fs::read_to_string(project.root.join("scenes/startup.scene.json")).unwrap(),
            authored_bytes
        );
    }
}

#[cfg(all(feature = "tooling", feature = "presentation-2d"))]
#[test]
fn generic_editor_imported_image_can_be_selected_by_path_and_stable_id_in_scene_preview() {
    let project = TestProject::with_image("generic-imported-image-scene-preview");
    let imported_path = "textures/imported.png";
    fs::copy(
        project.root.join("assets/textures/player.png"),
        project.root.join("assets").join(imported_path),
    )
    .unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    drive_image_import_host_until_idle(&mut editor);
    let requested = editor.request_editor_image_import(
        editor.current_frame().revision(),
        "assets/textures/imported.png",
        ImageImportSettings::default(),
    );
    let imported = if matches!(
        requested,
        super::editor::EditorImageImportRequestResult::Started(_)
    ) {
        drive_image_import_to_terminal(&mut editor)
    } else {
        editor.editor_image_import_state()
    };
    drive_image_import_host_until_idle(&mut editor);
    let accepted_image = editor
        .editor_image_last_good(&AssetPath::new(imported_path).unwrap())
        .cloned();
    let mut selections = Vec::new();
    if let Some(image) = accepted_image {
        let document = editor.workspace().active_document().unwrap();
        editor.set_editor_preview_extent(Some(nara_render::Extent2d::new(640, 360).unwrap()));
        for (kind, value) in [
            ("path", imported_path.to_owned()),
            ("stable_id", image.stable_id().to_string()),
        ] {
            let base_revision = editor.workspace().scene(document).unwrap().revision();
            let report =
                editor.apply_workspace_command(EditorWorkspaceCommand::ApplyAuthoringFieldEdit {
                    document,
                    base_revision,
                    edit: AuthoringFieldEdit::single(
                        nara_identity::SceneEntityId::new("player").unwrap(),
                        ComponentTypeId::new("nara.sprite.Sprite"),
                        ComponentSchemaVersion::ONE,
                        ComponentFieldId::new("material.image"),
                        ComponentValue::map([
                            ("kind", ComponentValue::String(kind.to_owned())),
                            ("value", ComponentValue::String(value)),
                        ]),
                    )
                    .unwrap(),
                });
            let expected_revision = editor.workspace().scene(document).unwrap().revision();
            let expected_key = nara_image::image_resource_key(image.handle());
            let deadline = Instant::now() + Duration::from_secs(5);
            let mut selected = false;
            while Instant::now() < deadline {
                editor.drive_editor_frame(Duration::ZERO);
                if let Some(candidate) = editor.take_editor_preview_candidate() {
                    let current = candidate.revision() == expected_revision
                        && candidate
                            .hits()
                            .iter()
                            .any(|hit| hit.entity().as_str() == "player");
                    let (_, _, _, _, batches, _) = candidate.into_parts();
                    selected = current
                        && batches.as_slice().iter().any(|batch| {
                            batch
                                .image
                                .as_ref()
                                .is_some_and(|image| image.identity().key() == expected_key)
                        });
                    if selected {
                        break;
                    }
                }
                std::thread::sleep(Duration::from_millis(1));
            }
            selections.push((report.applied, selected));
        }
    }
    let diagnostics = editor.current_frame().diagnostics().clone();
    let close = drive_editor_close_to_completion(&mut editor);
    drop(editor);
    assert!(matches!(
        close,
        super::editor::EditorProjectCloseProgress::Complete(_)
    ));
    assert!(
        matches!(
            imported,
            super::editor::EditorImageImportState::Succeeded { .. }
        ),
        "{requested:?}: {imported:?}"
    );
    assert_eq!(
        selections,
        vec![(true, true), (true, true)],
        "{diagnostics:?}"
    );
}

#[cfg(feature = "tooling")]
#[test]
fn external_play_restart_rechecks_saved_files_before_requesting_child_retirement() {
    for (name, relative_path, remove) in [
        ("scene", "scenes/startup.scene.json", false),
        ("prefab", "prefabs/actor.prefab.json", false),
        ("cargo", "Cargo.toml", false),
        ("unavailable", "scenes/startup.scene.json", true),
    ] {
        let project = TestProject::new(&format!("external-restart-{name}"));
        project.configure_supervised_play(&format!("external-restart-{name}"));
        if name == "prefab" {
            fs::write(
                project.root.join(relative_path),
                editable_authoring_prefab("prefab member")
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap();
            let mut anchor =
                SceneEntityRecord::new(nara_identity::SceneEntityId::new("prefab-anchor").unwrap());
            anchor.prefab = Some(nara_scene::PrefabInstance {
                source: AssetRef::path("actor.prefab.json").unwrap(),
                overrides: None,
            });
            fs::write(
                project.root.join("scenes/startup.scene.json"),
                SceneDocument::from_ordered_roots([editable_name_entity("initial"), anchor])
                    .to_json_string()
                    .unwrap(),
            )
            .unwrap();
        }
        let mut host = super::editor::EditorProjectOpenHost::new();
        let mut attempt = host.begin(project.writable_capability(), Some("desktop".to_owned()));
        let open_deadline = Instant::now() + Duration::from_secs(5);
        let mut editor = loop {
            match host.advance(attempt) {
                super::editor::EditorProjectOpenProgress::Pending(next) => {
                    assert!(
                        Instant::now() < open_deadline,
                        "project Open did not settle"
                    );
                    std::thread::sleep(Duration::from_millis(1));
                    attempt = next;
                }
                super::editor::EditorProjectOpenProgress::Opened(session) => break session,
                super::editor::EditorProjectOpenProgress::Rejected(failure) => {
                    panic!(
                        "supervised project did not open: {:?}",
                        failure.diagnostics()
                    )
                }
            }
        };
        assert_eq!(
            editor.request_play(nara_tooling::EditorPlayCommand::Play),
            nara_tooling::EditorPlayRequestResult::Accepted,
        );
        let play = editor.take_external_play_request().unwrap();
        let source_digest = nara_fs::ContentDigest::of_bytes(play.source_receipt().unwrap());
        assert!(
            editor.apply_external_play_observation(
                nara_tooling::EditorExternalPlayObservation::new(
                    play.attempt(),
                    nara_tooling::EditorExternalPlayState::Running,
                    Some(1),
                    true,
                    Some(source_digest),
                    nara_tooling::EditorExternalPlayOutput::default(),
                    Some(nara_tooling::EditorPlayOperationResult::Applied {
                        operation: nara_tooling::EditorPlayOperation::Play,
                        generation: Some(1),
                    }),
                )
                .unwrap(),
            )
        );
        assert!(
            !editor
                .drive_editor_frame(Duration::ZERO)
                .play()
                .is_out_of_date()
        );

        let path = project.root.join(relative_path);
        if remove {
            fs::remove_file(path).unwrap();
        } else {
            let mut bytes = fs::read(&path).unwrap();
            bytes.push(b'\n');
            fs::write(path, bytes).unwrap();
        }
        assert!(
            !editor.current_frame().play().is_out_of_date(),
            "external disk changes deliberately leave the published source flag stale"
        );
        assert_eq!(
            editor.request_play(nara_tooling::EditorPlayCommand::Restart),
            nara_tooling::EditorPlayRequestResult::Rejected(
                nara_tooling::EditorPlayRejection::RuntimeOutOfDate,
            ),
            "{name}: admission must inspect saved files before issuing retirement"
        );
        assert!(editor.take_external_play_request().is_none(), "{name}");
        let frame = editor.drive_editor_frame(Duration::ZERO);
        assert!(frame.play().is_out_of_date(), "{name}");
        assert_eq!(frame.play().state(), nara_tooling::EditorPlayState::Running);
        assert_eq!(frame.play().external().unwrap().generation(), Some(1));
        assert_eq!(
            editor.request_play(nara_tooling::EditorPlayCommand::Stop),
            nara_tooling::EditorPlayRequestResult::Accepted,
            "{name}: source admission failure must not block Stop"
        );
        assert_eq!(
            editor.take_external_play_request().unwrap().operation(),
            nara_tooling::EditorPlayOperation::Stop,
        );
    }
}

#[cfg(feature = "tooling")]
#[test]
fn external_play_tracks_exact_saved_source_across_edit_save_restart_and_successor_play() {
    let project = TestProject::new("external-play-saved-source-receipt");
    project.configure_supervised_play("external-play-saved-source-receipt");

    let mut host = super::editor::EditorProjectOpenHost::new();
    let mut attempt = host.begin(project.writable_capability(), Some("desktop".to_owned()));
    let open_deadline = Instant::now() + Duration::from_secs(5);
    let mut editor = loop {
        match host.advance(attempt) {
            super::editor::EditorProjectOpenProgress::Pending(next) => {
                assert!(
                    Instant::now() < open_deadline,
                    "project Open did not settle"
                );
                std::thread::sleep(Duration::from_millis(1));
                attempt = next;
            }
            super::editor::EditorProjectOpenProgress::Opened(session) => break session,
            super::editor::EditorProjectOpenProgress::Rejected(failure) => panic!(
                "the supervised authoring project should open: {:?}",
                failure.diagnostics()
            ),
        }
    };
    assert_eq!(
        editor.project_readiness().runtime(),
        nara_tooling::EditorRuntimeAvailability::ExternalSupervised
    );

    let document = editor.workspace().active_document().unwrap();
    assert_eq!(
        editor.request_play(nara_tooling::EditorPlayCommand::Play),
        nara_tooling::EditorPlayRequestResult::Accepted
    );
    let play = editor.take_external_play_request().unwrap();
    let first_source = play.source_receipt().unwrap().to_vec();
    let first_digest = nara_fs::ContentDigest::of_bytes(&first_source);
    let output = |sequence: u64, text: &str| {
        let message = nara_tooling::EditorExternalPlayMessage::new(
            sequence,
            nara_tooling::EditorExternalPlayMessageKind::ProductStdout,
            None,
            text.to_owned(),
            false,
        )
        .unwrap();
        nara_tooling::EditorExternalPlayOutput::new(
            sequence,
            nara_tooling::EditorExternalPlayDiagnostics::default(),
            vec![message],
            0,
            0,
            0,
            false,
        )
        .unwrap()
    };
    let stale_output = output(1, "stale output");
    assert!(
        !editor.apply_external_play_observation(
            nara_tooling::EditorExternalPlayObservation::new(
                play.attempt() + 1,
                nara_tooling::EditorExternalPlayState::Faulted,
                None,
                false,
                Some(first_digest),
                stale_output,
                None,
            )
            .unwrap(),
        )
    );
    let first_output = output(2, "first product output");
    assert!(
        editor.apply_external_play_observation(
            nara_tooling::EditorExternalPlayObservation::new(
                play.attempt(),
                nara_tooling::EditorExternalPlayState::Running,
                Some(1),
                true,
                Some(first_digest),
                first_output,
                Some(nara_tooling::EditorPlayOperationResult::Applied {
                    operation: nara_tooling::EditorPlayOperation::Play,
                    generation: Some(1),
                }),
            )
            .unwrap(),
        )
    );
    let running = editor.drive_editor_frame(Duration::ZERO);
    assert!(!running.play().is_out_of_date());
    assert_eq!(
        running.play().external().unwrap().output().messages()[0].text(),
        "first product output"
    );

    let unchanged = nara_tooling::EditorExternalPlayObservation::new(
        play.attempt(),
        nara_tooling::EditorExternalPlayState::Running,
        Some(1),
        true,
        Some(first_digest),
        output(2, "first product output"),
        None,
    )
    .unwrap();
    assert!(
        !editor.apply_external_play_observation(unchanged),
        "a quiet child must not report a frame-changing observation"
    );
    assert_eq!(
        editor.drive_editor_frame(Duration::ZERO).play().result(),
        Some(nara_tooling::EditorPlayOperationResult::Applied {
            operation: nara_tooling::EditorPlayOperation::Play,
            generation: Some(1),
        }),
        "state-only observations preserve the last operation result"
    );

    make_authoring_name_dirty(&mut editor, document, "saved-successor");
    assert!(editor.workspace().scene(document).unwrap().is_dirty());
    assert!(
        !editor
            .drive_editor_frame(Duration::ZERO)
            .play()
            .is_out_of_date()
    );

    assert_eq!(
        editor.request_play(nara_tooling::EditorPlayCommand::Restart),
        nara_tooling::EditorPlayRequestResult::Accepted
    );
    let restart = editor.take_external_play_request().unwrap();
    assert!(restart.source_receipt().is_none());
    assert!(
        editor
            .external_play_view()
            .unwrap()
            .output()
            .messages()
            .is_empty()
    );
    let restart_output = output(1, "restarted product output");
    assert!(
        editor.apply_external_play_observation(
            nara_tooling::EditorExternalPlayObservation::new(
                restart.attempt(),
                nara_tooling::EditorExternalPlayState::Running,
                Some(1),
                true,
                Some(first_digest),
                restart_output.clone(),
                Some(nara_tooling::EditorPlayOperationResult::Applied {
                    operation: nara_tooling::EditorPlayOperation::Restart,
                    generation: Some(1),
                }),
            )
            .unwrap(),
        )
    );

    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document),
        }),
        nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
    ));
    let saved = drive_persistence_to_terminal(&mut editor);
    assert!(matches!(
        editor.persistence_view().result(),
        Some(EditorPersistenceResult::Saved { document: saved_document, .. })
            if saved_document == document
    ));
    assert!(saved.play().is_out_of_date());
    assert!(!editor.external_play_view().unwrap().last_good_available());

    assert_eq!(
        editor.request_play(nara_tooling::EditorPlayCommand::Restart),
        nara_tooling::EditorPlayRequestResult::Rejected(
            nara_tooling::EditorPlayRejection::RuntimeOutOfDate,
        ),
        "changed saved source must reject Restart before retiring the running child"
    );
    assert!(editor.take_external_play_request().is_none());
    let rejected = editor.drive_editor_frame(Duration::ZERO);
    let external = rejected.play().external().unwrap();
    assert_eq!(
        external.state(),
        nara_tooling::EditorExternalPlayState::Running
    );
    assert_eq!(external.generation(), Some(1));
    assert_eq!(external.output(), &restart_output);
    assert_eq!(
        rejected.play().result(),
        Some(nara_tooling::EditorPlayOperationResult::Rejected {
            operation: nara_tooling::EditorPlayOperation::Restart,
            reason: nara_tooling::EditorPlayRejection::RuntimeOutOfDate,
        }),
    );

    assert!(
        editor.apply_external_play_observation(
            nara_tooling::EditorExternalPlayObservation::new(
                restart.attempt(),
                nara_tooling::EditorExternalPlayState::Faulted,
                Some(1),
                true,
                Some(first_digest),
                restart_output.clone(),
                Some(nara_tooling::EditorPlayOperationResult::Failed {
                    operation: nara_tooling::EditorPlayOperation::Restart,
                    failure: nara_tooling::EditorPlayFailure::Runtime,
                }),
            )
            .unwrap(),
        )
    );
    assert!(!editor.external_play_view().unwrap().last_good_available());
    assert_eq!(
        editor.request_play(nara_tooling::EditorPlayCommand::RunLastGood),
        nara_tooling::EditorPlayRequestResult::Rejected(
            nara_tooling::EditorPlayRejection::InvalidState,
        )
    );

    assert_eq!(
        editor.request_play(nara_tooling::EditorPlayCommand::Stop),
        nara_tooling::EditorPlayRequestResult::Accepted
    );
    let stop = editor.take_external_play_request().unwrap();
    assert!(
        editor.apply_external_play_observation(
            nara_tooling::EditorExternalPlayObservation::new(
                stop.attempt(),
                nara_tooling::EditorExternalPlayState::Idle,
                None,
                true,
                Some(first_digest),
                restart_output,
                Some(nara_tooling::EditorPlayOperationResult::Applied {
                    operation: nara_tooling::EditorPlayOperation::Stop,
                    generation: None,
                }),
            )
            .unwrap(),
        )
    );
    assert_eq!(
        editor.external_play_view().unwrap().output().messages()[0].text(),
        "restarted product output"
    );

    assert_eq!(
        editor.request_play(nara_tooling::EditorPlayCommand::Play),
        nara_tooling::EditorPlayRequestResult::Accepted
    );
    let successor = editor.take_external_play_request().unwrap();
    assert!(
        editor
            .external_play_view()
            .unwrap()
            .output()
            .messages()
            .is_empty()
    );
    let successor_source = successor.source_receipt().unwrap();
    assert_ne!(
        nara_fs::ContentDigest::of_bytes(successor_source),
        first_digest
    );
    let successor_digest = nara_fs::ContentDigest::of_bytes(successor_source);
    assert!(
        editor.apply_external_play_observation(
            nara_tooling::EditorExternalPlayObservation::new(
                successor.attempt(),
                nara_tooling::EditorExternalPlayState::CleanupIncomplete,
                Some(2),
                false,
                Some(successor_digest),
                nara_tooling::EditorExternalPlayOutput::default(),
                Some(nara_tooling::EditorPlayOperationResult::Failed {
                    operation: nara_tooling::EditorPlayOperation::RetryClose,
                    failure: nara_tooling::EditorPlayFailure::Close,
                }),
            )
            .unwrap(),
        )
    );
    assert_eq!(
        editor.drive_editor_frame(Duration::ZERO).play().state(),
        nara_tooling::EditorPlayState::CloseIncomplete,
    );
    assert_eq!(
        editor.request_play(nara_tooling::EditorPlayCommand::RetryClose),
        nara_tooling::EditorPlayRequestResult::Accepted,
    );
    assert_eq!(
        editor.take_external_play_request().unwrap().operation(),
        nara_tooling::EditorPlayOperation::RetryClose,
    );
}

#[cfg(feature = "tooling")]
fn opened_document(
    result: super::editor::EditorDocumentOpenRequestResult,
) -> nara_tooling::EditorDocumentId {
    match result {
        super::editor::EditorDocumentOpenRequestResult::Activated { document }
        | super::editor::EditorDocumentOpenRequestResult::Opened { document } => document,
        other => panic!("the indexed document was not opened: {other:?}"),
    }
}

#[cfg(feature = "tooling")]
fn make_authoring_name_dirty(
    editor: &mut super::editor::EditorProjectSession,
    document: nara_tooling::EditorDocumentId,
    value: &str,
) {
    let base_revision = editor
        .workspace()
        .scene(document)
        .map(|slot| slot.revision())
        .or_else(|| {
            editor
                .workspace()
                .prefab(document)
                .map(|slot| slot.revision())
        })
        .expect("the editable authoring document is open");
    let report = editor.apply_workspace_command(EditorWorkspaceCommand::ApplyAuthoringFieldEdit {
        document,
        base_revision,
        edit: AuthoringFieldEdit::single(
            nara_identity::SceneEntityId::new("editable-name").unwrap(),
            ComponentTypeId::new("nara.scene.Name"),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new("value"),
            ComponentValue::String(value.to_owned()),
        )
        .unwrap(),
    });
    assert!(report.applied, "{report:?}");
}

#[cfg(feature = "tooling")]
fn editable_name_entity(initial_name: &str) -> SceneEntityRecord {
    let owner =
        PersistentComponentSchemaOwnerId::try_new(SCENE_COMPONENTS_SCHEMA_OWNER_ID.as_str())
            .unwrap();
    SceneEntityRecord::new(nara_identity::SceneEntityId::new("editable-name").unwrap())
        .with_component(
            ComponentTypeId::new("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String(initial_name.to_owned()),
            )
            .with_owner(owner),
        )
}

#[cfg(feature = "tooling")]
fn editable_authoring_scene(initial_name: &str) -> SceneDocument {
    SceneDocument::from_ordered_roots([editable_name_entity(initial_name)])
}

#[cfg(feature = "tooling")]
fn editable_authoring_prefab(initial_name: &str) -> nara_scene::PrefabDocument {
    nara_scene::PrefabDocument::from_ordered_roots([editable_name_entity(initial_name)])
}

#[cfg(feature = "tooling")]
fn scene_name_value(
    editor: &super::editor::EditorProjectSession,
    document: nara_tooling::EditorDocumentId,
) -> String {
    let component = editor
        .workspace()
        .scene(document)
        .unwrap()
        .session()
        .document()
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "editable-name")
        .unwrap()
        .components
        .get(&ComponentTypeId::new("nara.scene.Name"))
        .unwrap();
    match &component.value {
        ComponentValue::String(value) => value.clone(),
        value => panic!("the editable Name component has an unexpected value: {value:?}"),
    }
}

#[cfg(feature = "tooling")]
fn refresh_project_source_index(
    editor: &mut super::editor::EditorProjectSession,
) -> Arc<crate::project_host::authoring::ProjectSourceIndexSnapshot> {
    assert!(editor.request_project_source_index_refresh());
    let index_deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < index_deadline {
        if !editor.project_source_index_work_pending() {
            break;
        }
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert!(!editor.project_source_index_work_pending());
    assert!(matches!(
        editor.project_source_index_kind(),
        crate::project_host::authoring::ProjectSourceIndexStateKind::Ready
            | crate::project_host::authoring::ProjectSourceIndexStateKind::ReadyWithDiagnostics
    ));
    editor.project_source_index_snapshot().unwrap()
}

#[cfg(feature = "tooling")]
#[test]
fn generic_editor_opens_closure_external_scene_and_prefab_with_independent_lanes() {
    let project = TestProject::new("generic-document-admission");
    fs::create_dir_all(project.root.join("scripts")).unwrap();
    fs::write(
        project.root.join("scenes/external.scene.json"),
        SceneDocument::from_ordered_roots([SceneEntityRecord::new(
            nara_identity::SceneEntityId::new("external-scene").unwrap(),
        )])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let unknown_owner =
        PersistentComponentSchemaOwnerId::try_new("example.generic-editor.unknown").unwrap();
    fs::write(
        project.root.join("prefabs/external.prefab.json"),
        nara_scene::PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
            nara_identity::SceneEntityId::new("external-prefab").unwrap(),
        )
        .with_component(
            ComponentTypeId::new("example.Unknown"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, ComponentValue::Bool(true))
                .with_owner(unknown_owner),
        )])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();

    let mut editor = open_generic_editor_with_ready_index(&project);
    let startup = editor.workspace().active_document().unwrap();
    let index = editor.project_source_index_snapshot().unwrap();
    let opened_scene = editor.open_indexed_authoring_document(
        Arc::clone(&index),
        "scenes/external.scene.json",
        crate::project_host::authoring::AuthoringDocumentKind::Scene,
    );
    let scene_document = match opened_scene {
        super::editor::EditorDocumentOpenRequestResult::Opened { document } => document,
        other => panic!("closure-external Scene was not opened: {other:?}"),
    };
    assert_eq!(editor.workspace().len(), 2);
    assert!(editor.workspace().scene(startup).is_some());
    assert!(editor.workspace_intent_view().result().is_none());
    assert_eq!(editor.workspace().active_document(), Some(scene_document));
    assert_eq!(
        editor
            .workspace()
            .scene(scene_document)
            .unwrap()
            .source_binding()
            .unwrap()
            .locator(),
        "scenes/external.scene.json"
    );
    assert_eq!(
        editor
            .workspace()
            .scene(scene_document)
            .unwrap()
            .authoring_readiness(),
        Some(nara_scene::AuthoringDocumentReadiness::Complete)
    );

    let opened_prefab = editor.open_indexed_authoring_document(
        index,
        "prefabs/external.prefab.json",
        crate::project_host::authoring::AuthoringDocumentKind::Prefab,
    );
    let prefab_document = match opened_prefab {
        super::editor::EditorDocumentOpenRequestResult::Opened { document } => document,
        other => panic!("closure-external Prefab was not opened: {other:?}"),
    };
    assert_eq!(editor.workspace().len(), 3);
    assert!(editor.workspace().scene(startup).is_some());
    assert!(editor.workspace().scene(scene_document).is_some());
    assert_eq!(editor.workspace().active_document(), Some(prefab_document));
    assert_eq!(
        editor
            .workspace()
            .prefab(prefab_document)
            .unwrap()
            .source_binding()
            .locator(),
        "prefabs/external.prefab.json"
    );
    assert_eq!(
        editor
            .workspace()
            .prefab(prefab_document)
            .unwrap()
            .authoring_readiness(),
        Some(nara_scene::AuthoringDocumentReadiness::Degraded)
    );
}

#[cfg(feature = "tooling")]
#[test]
fn indexed_document_admission_rejections_leave_the_workspace_unchanged() {
    let project = TestProject::new("generic-document-rejection");
    fs::create_dir_all(project.root.join("scripts")).unwrap();
    fs::write(
        project.root.join("scenes/external.scene.json"),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(project.root.join("scenes/malformed.scene.json"), b"{").unwrap();
    let oversized = File::create(project.root.join("scenes/oversized.scene.json")).unwrap();
    oversized
        .set_len((nara_scene::SceneFileLimits::default().encoded_bytes().get() + 1) as u64)
        .unwrap();
    drop(oversized);
    let mut editor = open_generic_editor_with_ready_index(&project);
    let index = editor.project_source_index_snapshot().unwrap();
    let original_document = editor.workspace().active_document().unwrap();
    let original_revision = editor
        .workspace()
        .scene(original_document)
        .unwrap()
        .revision();

    let rejections = [
        editor.open_indexed_authoring_document(
            Arc::clone(&index),
            "scenes/missing.scene.json",
            crate::project_host::authoring::AuthoringDocumentKind::Scene,
        ),
        editor.open_indexed_authoring_document(
            Arc::clone(&index),
            "scenes/external.scene.json",
            crate::project_host::authoring::AuthoringDocumentKind::Prefab,
        ),
        editor.open_indexed_authoring_document(
            Arc::clone(&index),
            "scenes/malformed.scene.json",
            crate::project_host::authoring::AuthoringDocumentKind::Scene,
        ),
        editor.open_indexed_authoring_document(
            Arc::clone(&index),
            "scenes/oversized.scene.json",
            crate::project_host::authoring::AuthoringDocumentKind::Scene,
        ),
    ];
    assert_eq!(
        rejections,
        [
            super::editor::EditorDocumentOpenRequestResult::Rejected {
                code: "project.editor.document-index-member-missing"
            },
            super::editor::EditorDocumentOpenRequestResult::Rejected {
                code: "project.editor.document-kind-mismatch"
            },
            super::editor::EditorDocumentOpenRequestResult::Rejected {
                code: "project.editor.document-source-format-invalid"
            },
            super::editor::EditorDocumentOpenRequestResult::Rejected {
                code: "project.editor.document-source-too-large"
            },
        ]
    );

    // Keep the old object alive so the replacement cannot reuse its native identity.
    fs::rename(
        project.root.join("scenes/external.scene.json"),
        project.root.join("scenes/previous.scene.json"),
    )
    .unwrap();
    fs::write(
        project.root.join("scenes/external.scene.json"),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    assert_eq!(
        editor.open_indexed_authoring_document(
            Arc::clone(&index),
            "scenes/external.scene.json",
            crate::project_host::authoring::AuthoringDocumentKind::Scene,
        ),
        super::editor::EditorDocumentOpenRequestResult::Rejected {
            code: "project.editor.document-source-identity-changed"
        }
    );
    assert_eq!(editor.workspace().len(), 1);
    assert_eq!(
        editor.workspace().active_document(),
        Some(original_document)
    );
    assert_eq!(
        editor
            .workspace()
            .scene(original_document)
            .unwrap()
            .revision(),
        original_revision
    );

    let refreshed = refresh_project_source_index(&mut editor);
    assert!(!Arc::ptr_eq(&index, &refreshed));
    assert_eq!(
        editor.open_indexed_authoring_document(
            index,
            "scenes/external.scene.json",
            crate::project_host::authoring::AuthoringDocumentKind::Scene,
        ),
        super::editor::EditorDocumentOpenRequestResult::Rejected {
            code: "project.editor.document-index-stale"
        }
    );
}

#[cfg(feature = "tooling")]
#[test]
fn project_browser_rejects_stale_frames_without_rebasing_the_open_request() {
    let project = TestProject::new("project-browser-stale-frame");
    fs::create_dir_all(project.root.join("scripts")).unwrap();
    fs::write(
        project.root.join("scenes/external.scene.json"),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let stale = editor.current_frame();
    assert!(
        stale
            .project_browser()
            .entry("scenes/external.scene.json")
            .is_some()
    );
    let original_document = editor.workspace().active_document();
    let current = editor.drive_editor_frame(Duration::ZERO);

    assert_eq!(
        editor.dispatch_editor_actions(EditorActionBatch::new(
            stale.revision(),
            [EditorAction::ProjectBrowser(
                EditorProjectBrowserAction::Open {
                    relative_path: Arc::from("scenes/external.scene.json"),
                }
            )],
        )),
        Err(EditorActionRejection::StaleFrame {
            expected: current.revision(),
            received: stale.revision(),
        })
    );
    assert_eq!(editor.workspace().active_document(), original_document);
}

#[cfg(feature = "tooling")]
#[test]
fn project_browser_rejects_unindexed_and_identity_replaced_sources_before_open() {
    let project = TestProject::new("project-browser-open-admission");
    fs::create_dir_all(project.root.join("scripts")).unwrap();
    fs::write(
        project.root.join("scenes/external.scene.json"),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let original_document = editor.workspace().active_document();

    let frame = editor.current_frame();
    let forged = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::ProjectBrowser(
                EditorProjectBrowserAction::Open {
                    relative_path: Arc::from("target/private.scene.json"),
                },
            )],
        ))
        .unwrap();
    assert_eq!(
        forged.outcomes(),
        [EditorActionOutcome::ProjectBrowser(
            EditorProjectBrowserActionOutcome::Rejected {
                code: "project.editor.document-not-indexed",
            }
        )]
    );

    let current = editor.drive_editor_frame(Duration::ZERO);
    // Keep the old object alive so the replacement cannot reuse its native identity.
    fs::rename(
        project.root.join("scenes/external.scene.json"),
        project.root.join("scenes/previous.scene.json"),
    )
    .unwrap();
    fs::write(
        project.root.join("scenes/external.scene.json"),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    let replaced = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            current.revision(),
            [EditorAction::ProjectBrowser(
                EditorProjectBrowserAction::Open {
                    relative_path: Arc::from("scenes/external.scene.json"),
                },
            )],
        ))
        .unwrap();
    let [
        EditorActionOutcome::ProjectBrowser(EditorProjectBrowserActionOutcome::Preparing {
            request,
        }),
    ] = replaced.outcomes()
    else {
        panic!("{replaced:?}");
    };
    let request = *request;
    let deadline = Instant::now() + Duration::from_secs(5);
    let result = loop {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        if let Some(result) = frame.document_open().result() {
            break result;
        }
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(1));
    };
    assert_eq!(
        result,
        nara_tooling::EditorDocumentOpenResult::Rejected {
            request,
            code: "project.editor.document-source-identity-changed"
        }
    );
    assert_eq!(editor.workspace().active_document(), original_document);
}

#[cfg(feature = "tooling")]
#[test]
fn duplicate_close_decision_cannot_approve_the_next_dirty_document() {
    let project = TestProject::new("duplicate-close-decision");
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        editable_authoring_scene("scene").to_json_string().unwrap(),
    )
    .unwrap();
    fs::write(
        project.root.join("prefabs/close.prefab.json"),
        editable_authoring_prefab("prefab")
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let scene = editor.workspace().active_document().unwrap();
    let index = editor.project_source_index_snapshot().unwrap();
    let prefab = opened_document(editor.open_indexed_authoring_document(
        index,
        "prefabs/close.prefab.json",
        crate::project_host::authoring::AuthoringDocumentKind::Prefab,
    ));
    assert!(editor.workspace_intent_view().result().is_none());
    make_authoring_name_dirty(&mut editor, scene, "dirty-scene");
    make_authoring_name_dirty(&mut editor, prefab, "dirty-prefab");
    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::Exit),
        nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
    );
    let frame = editor.drive_editor_frame(Duration::ZERO);
    let first = frame.workspace_intent().decision_document().unwrap();
    let decision = frame
        .workspace_intent()
        .decision_prompt()
        .unwrap()
        .resolve(nara_tooling::EditorCloseDecision::Discard);
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [
                EditorAction::WorkspaceIntent(decision.clone()),
                EditorAction::WorkspaceIntent(decision),
            ],
        ))
        .unwrap();
    assert!(matches!(
        report.outcomes()[0],
        EditorActionOutcome::WorkspaceIntent(
            nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
        )
    ));
    assert!(matches!(
        report.outcomes()[1],
        EditorActionOutcome::WorkspaceIntent(
            nara_tooling::EditorWorkspaceIntentRequestResult::Rejected(_)
        )
    ));
    let remaining = editor.workspace_intent_view();
    assert_eq!(
        remaining.phase(),
        Some(nara_tooling::EditorWorkspaceIntentPhase::AwaitingDecision)
    );
    assert_ne!(remaining.decision_document(), Some(first));
    assert!(editor.workspace().document_is_dirty(scene).unwrap());
    assert!(editor.workspace().document_is_dirty(prefab).unwrap());
}

#[cfg(feature = "tooling")]
#[test]
fn close_prompt_replay_rejects_after_cancel_new_intent_and_failed_save() {
    use nara_tooling::{
        EditorCloseDecision, EditorPersistenceFailureStage, EditorWorkspaceIntentPhase,
        EditorWorkspaceIntentRejection, EditorWorkspaceIntentRequestResult,
    };

    let project = TestProject::new("close-prompt-lifetime");
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        editable_authoring_scene("scene").to_json_string().unwrap(),
    )
    .unwrap();
    let mut editor = open_generic_editor_with_ready_index(&project);
    let document = editor.workspace().active_document().unwrap();
    make_authoring_name_dirty(&mut editor, document, "retained-dirty");
    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::Exit),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    let old = editor.workspace_intent_view().decision_prompt().unwrap();
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(editor.workspace_intent_view().decision_prompt(), Some(old));
    assert_eq!(
        editor.resolve_workspace_intent(old, EditorCloseDecision::Cancel),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert!(editor.acknowledge_workspace_intent_result());
    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::Exit),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    let current = editor.workspace_intent_view().decision_prompt().unwrap();
    assert_ne!(old, current);
    let before = editor.workspace_intent_view();
    assert_eq!(
        editor.resolve_workspace_intent(old, EditorCloseDecision::Discard),
        EditorWorkspaceIntentRequestResult::Rejected(EditorWorkspaceIntentRejection::StalePrompt)
    );
    assert_eq!(editor.workspace_intent_view(), before);
    let disk_before = fs::read(project.root.join("scenes/startup.scene.json")).unwrap();
    editor.test_fail_next_save(document, EditorPersistenceFailureStage::Encode);
    assert_eq!(
        editor.resolve_workspace_intent(current, EditorCloseDecision::Save),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    drive_persistence_to_terminal(&mut editor);
    let retry = editor.workspace_intent_view();
    assert_eq!(
        retry.phase(),
        Some(EditorWorkspaceIntentPhase::AwaitingDecision)
    );
    assert_ne!(retry.decision_prompt(), Some(current));
    assert_eq!(
        editor.resolve_workspace_intent(current, EditorCloseDecision::Discard),
        EditorWorkspaceIntentRequestResult::Rejected(EditorWorkspaceIntentRejection::StalePrompt)
    );
    assert_eq!(editor.workspace_intent_view(), retry);
    assert!(editor.workspace().document_is_dirty(document).unwrap());
    assert_eq!(
        fs::read(project.root.join("scenes/startup.scene.json")).unwrap(),
        disk_before
    );
    let retry_prompt = retry.decision_prompt().unwrap();
    let control = editor.hold_next_save(super::editor::SaveTestPhase::BeforePublication);
    assert_eq!(
        editor.resolve_workspace_intent(retry_prompt, EditorCloseDecision::Save),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert!(editor.persistence_view().result().is_none());
    editor.drive_editor_frame(Duration::ZERO);
    assert!(control.wait_until_started());
    for _ in 0..3 {
        let frame = editor.drive_editor_frame(Duration::ZERO);
        assert!(frame.persistence().result().is_none());
        assert_eq!(
            frame.workspace_intent().phase(),
            Some(EditorWorkspaceIntentPhase::Saving)
        );
        assert!(frame.workspace_intent().decision_prompt().is_none());
    }
    assert_eq!(
        editor.resolve_workspace_intent(current, EditorCloseDecision::Discard),
        EditorWorkspaceIntentRequestResult::Rejected(EditorWorkspaceIntentRejection::Busy)
    );
    assert_eq!(
        editor.resolve_workspace_intent(retry_prompt, EditorCloseDecision::Cancel),
        EditorWorkspaceIntentRequestResult::Rejected(EditorWorkspaceIntentRejection::Busy)
    );
    assert_eq!(
        fs::read(project.root.join("scenes/startup.scene.json")).unwrap(),
        disk_before
    );
    control.release();
    let deadline = Instant::now() + Duration::from_secs(5);
    while editor.workspace_intent_view().phase() == Some(EditorWorkspaceIntentPhase::Saving) {
        editor.drive_editor_frame(Duration::ZERO);
        assert!(
            Instant::now() < deadline,
            "the retried exit Save did not complete"
        );
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        editor.workspace_intent_view().phase(),
        Some(EditorWorkspaceIntentPhase::Prepared)
    );
    assert!(!editor.workspace().document_is_dirty(document).unwrap());
    assert!(
        fs::read_to_string(project.root.join("scenes/startup.scene.json"))
            .unwrap()
            .contains("retained-dirty")
    );
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        super::editor::EditorProjectCloseProgress::Complete(_)
    ));
}

#[cfg(feature = "tooling")]
#[test]
fn additive_scene_open_preserves_dirty_documents_and_close_targets_cancel_discard_or_save() {
    use nara_tooling::{
        EditorCloseDecision, EditorWorkspaceIntentRequestResult, EditorWorkspaceIntentResult,
    };

    let project = TestProject::new("generic-scene-additive-open");
    fs::create_dir_all(project.root.join("scripts")).unwrap();
    for name in ["first", "second", "third"] {
        fs::write(
            project.root.join(format!("scenes/{name}.scene.json")),
            editable_authoring_scene(name).to_json_string().unwrap(),
        )
        .unwrap();
    }
    let mut editor = open_generic_editor_with_ready_index(&project);
    let startup = editor.workspace().active_document().unwrap();
    let index = editor.project_source_index_snapshot().unwrap();
    let first = opened_document(editor.open_indexed_authoring_document(
        Arc::clone(&index),
        "scenes/first.scene.json",
        crate::project_host::authoring::AuthoringDocumentKind::Scene,
    ));
    make_authoring_name_dirty(&mut editor, first, "cancel-retained");
    let first_before = editor.workspace_model().document(first).unwrap().clone();

    let second = opened_document(editor.open_indexed_authoring_document(
        Arc::clone(&index),
        "scenes/second.scene.json",
        crate::project_host::authoring::AuthoringDocumentKind::Scene,
    ));
    assert_ne!(first, second);
    assert_eq!(editor.workspace().len(), 3);
    assert!(editor.workspace_intent_view().decision_prompt().is_none());
    assert!(editor.workspace_intent_view().result().is_none());
    let mut retained = editor.workspace_model().document(first).unwrap().clone();
    retained.active = first_before.active;
    assert_eq!(retained, first_before);
    assert_eq!(scene_name_value(&editor, first), "cancel-retained");
    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: first }),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(
        editor.resolve_workspace_intent(
            editor.workspace_intent_view().decision_prompt().unwrap(),
            EditorCloseDecision::Cancel
        ),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(
        editor.workspace_intent_view().result(),
        Some(EditorWorkspaceIntentResult::Cancelled {
            intent: EditorWorkspaceIntent::CloseDocument { document: first }
        })
    );
    assert_eq!(editor.workspace().active_document(), Some(second));
    assert!(editor.workspace().scene(first).unwrap().is_dirty());
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::Undo {
                document: first,
                base_revision: editor.workspace().document_revision(first).unwrap(),
            })
            .applied
    );
    assert_eq!(scene_name_value(&editor, first), "first");
    make_authoring_name_dirty(&mut editor, first, "discarded-first");
    assert!(editor.acknowledge_workspace_intent_result());

    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: first }),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(
        editor.resolve_workspace_intent(
            editor.workspace_intent_view().decision_prompt().unwrap(),
            EditorCloseDecision::Discard
        ),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.workspace_intent_view().result(),
        Some(EditorWorkspaceIntentResult::Applied {
            intent: EditorWorkspaceIntent::CloseDocument { document: first }
        })
    );
    assert!(editor.workspace().scene(first).is_none());
    assert!(editor.workspace().scene(startup).is_some());
    assert!(editor.workspace().scene(second).is_some());
    assert!(
        !fs::read_to_string(project.root.join("scenes/first.scene.json"))
            .unwrap()
            .contains("discarded-first")
    );
    assert!(editor.acknowledge_workspace_intent_result());
    make_authoring_name_dirty(&mut editor, second, "saved-before-close");
    let saved_revision = editor.workspace().document_revision(second).unwrap();

    let third = opened_document(editor.open_indexed_authoring_document(
        index,
        "scenes/third.scene.json",
        crate::project_host::authoring::AuthoringDocumentKind::Scene,
    ));
    assert_ne!(third, second);
    assert_eq!(scene_name_value(&editor, second), "saved-before-close");
    assert!(editor.workspace().scene(second).unwrap().is_dirty());
    assert!(editor.workspace_intent_view().decision_prompt().is_none());
    make_authoring_name_dirty(&mut editor, third, "retained-third");
    let third_before = editor.workspace_model().document(third).unwrap().clone();
    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: second }),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(
        editor.resolve_workspace_intent(
            editor.workspace_intent_view().decision_prompt().unwrap(),
            EditorCloseDecision::Save
        ),
        EditorWorkspaceIntentRequestResult::Accepted
    );
    drive_persistence_to_terminal(&mut editor);
    assert!(matches!(
        editor.persistence_view().result(),
        Some(EditorPersistenceResult::Saved { document, revision, .. })
            if document == second && revision == saved_revision
    ));
    assert_eq!(
        editor.workspace_intent_view().result(),
        Some(EditorWorkspaceIntentResult::Applied {
            intent: EditorWorkspaceIntent::CloseDocument { document: second }
        })
    );
    assert!(editor.workspace().scene(second).is_none());
    assert_eq!(
        editor.workspace_model().document(third),
        Some(&third_before)
    );
    assert!(editor.workspace().scene(startup).is_some());
    assert!(
        fs::read_to_string(project.root.join("scenes/second.scene.json"))
            .unwrap()
            .contains("saved-before-close")
    );
    assert!(editor.acknowledge_workspace_intent_result());
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult {
            attempt: editor
                .persistence_view()
                .attempt()
                .expect("the test observed a terminal persistence result")
        }),
        nara_tooling::EditorPersistenceRequestResult::Acknowledged { .. }
    ));

    let refreshed = refresh_project_source_index(&mut editor);
    let reopened = opened_document(editor.open_indexed_authoring_document(
        Arc::clone(&refreshed),
        "scenes/second.scene.json",
        crate::project_host::authoring::AuthoringDocumentKind::Scene,
    ));
    assert_ne!(reopened, second);
    assert_eq!(scene_name_value(&editor, reopened), "saved-before-close");
    assert!(editor.workspace_intent_view().result().is_none());

    assert_eq!(
        editor
            .request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: reopened }),
        nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert!(editor.workspace().scene(reopened).is_none());
    assert!(editor.workspace().scene(startup).is_some());
    assert_eq!(scene_name_value(&editor, third), "retained-third");
    let active = editor.workspace().active_document().unwrap();
    assert_ne!(active, reopened);
    assert_eq!(
        editor.current_frame().reopen_target(),
        nara_tooling::EditorReopenTarget::OpenDocument {
            document: active,
            base_revision: editor.workspace().document_revision(active).unwrap(),
        }
    );
    assert!(editor.acknowledge_workspace_intent_result());

    let reopened_after_release = opened_document(editor.open_indexed_authoring_document(
        refreshed,
        "scenes/second.scene.json",
        crate::project_host::authoring::AuthoringDocumentKind::Scene,
    ));
    assert_ne!(reopened_after_release, reopened);
    assert_eq!(
        scene_name_value(&editor, reopened_after_release),
        "saved-before-close"
    );
    assert_eq!(scene_name_value(&editor, third), "retained-third");
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        super::editor::EditorProjectCloseProgress::Complete(_)
    ));
}

#[cfg(feature = "tooling")]
#[test]
fn additive_prefab_open_preserves_dirty_sources_and_close_targets_cancel_discard_or_save() {
    let project = TestProject::new("generic-prefab-additive-open");
    fs::create_dir_all(project.root.join("scripts")).unwrap();
    for name in ["first", "second"] {
        fs::write(
            project.root.join(format!("prefabs/{name}.prefab.json")),
            editable_authoring_prefab(name).to_json_string().unwrap(),
        )
        .unwrap();
    }
    let mut editor = open_generic_editor_with_ready_index(&project);
    let scene = editor.workspace().active_document().unwrap();
    let index = editor.project_source_index_snapshot().unwrap();
    let first = opened_document(editor.open_indexed_authoring_document(
        Arc::clone(&index),
        "prefabs/first.prefab.json",
        crate::project_host::authoring::AuthoringDocumentKind::Prefab,
    ));
    make_authoring_name_dirty(&mut editor, first, "cancel-retained");
    let first_before = editor.workspace_model().document(first).unwrap().clone();

    let second = opened_document(editor.open_indexed_authoring_document(
        index,
        "prefabs/second.prefab.json",
        crate::project_host::authoring::AuthoringDocumentKind::Prefab,
    ));
    assert_ne!(first, second);
    assert_eq!(editor.workspace().len(), 3);
    assert!(editor.workspace_intent_view().decision_prompt().is_none());
    assert!(editor.workspace_intent_view().result().is_none());
    let mut retained = editor.workspace_model().document(first).unwrap().clone();
    retained.active = first_before.active;
    assert_eq!(retained, first_before);
    make_authoring_name_dirty(&mut editor, second, "retained-second");
    let second_before = editor.workspace_model().document(second).unwrap().clone();
    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: first }),
        nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(
        editor.resolve_workspace_intent(
            editor.workspace_intent_view().decision_prompt().unwrap(),
            nara_tooling::EditorCloseDecision::Cancel
        ),
        nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
    );
    assert!(editor.workspace().scene(scene).is_some());
    assert!(editor.workspace().prefab(first).unwrap().is_dirty());
    assert_eq!(editor.workspace().active_document(), Some(second));
    assert_eq!(
        editor.workspace_model().document(second),
        Some(&second_before)
    );
    assert_eq!(
        editor.workspace_intent_view().result(),
        Some(nara_tooling::EditorWorkspaceIntentResult::Cancelled {
            intent: EditorWorkspaceIntent::CloseDocument { document: first }
        })
    );
    assert!(editor.acknowledge_workspace_intent_result());

    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: first }),
        nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(
        editor.resolve_workspace_intent(
            editor.workspace_intent_view().decision_prompt().unwrap(),
            nara_tooling::EditorCloseDecision::Discard
        ),
        nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
    );
    editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor.workspace_intent_view().result(),
        Some(nara_tooling::EditorWorkspaceIntentResult::Applied {
            intent: EditorWorkspaceIntent::CloseDocument { document: first }
        })
    );
    assert!(editor.workspace().prefab(first).is_none());
    assert!(editor.workspace().scene(scene).is_some());
    assert_eq!(
        editor.workspace_model().document(second),
        Some(&second_before)
    );
    assert_eq!(
        editor
            .workspace()
            .prefab(second)
            .unwrap()
            .source_binding()
            .locator(),
        "prefabs/second.prefab.json"
    );
    assert!(
        !fs::read_to_string(project.root.join("prefabs/first.prefab.json"))
            .unwrap()
            .contains("cancel-retained")
    );
    assert!(editor.acknowledge_workspace_intent_result());
    assert_eq!(
        editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument { document: second }),
        nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
    );
    assert_eq!(
        editor.workspace_intent_view().decision_document(),
        Some(second)
    );
    assert_eq!(
        editor.resolve_workspace_intent(
            editor.workspace_intent_view().decision_prompt().unwrap(),
            nara_tooling::EditorCloseDecision::Save,
        ),
        nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
    );
    assert!(matches!(
        drive_persistence_to_terminal(&mut editor).persistence().result(),
        Some(EditorPersistenceResult::Saved { document, revision, .. })
            if document == second && revision == second_before.revision
    ));
    assert_eq!(
        editor.workspace_intent_view().result(),
        Some(nara_tooling::EditorWorkspaceIntentResult::Applied {
            intent: EditorWorkspaceIntent::CloseDocument { document: second }
        })
    );
    assert!(editor.workspace().prefab(second).is_none());
    assert!(editor.workspace().scene(scene).is_some());
    assert!(
        fs::read_to_string(project.root.join("prefabs/second.prefab.json"))
            .unwrap()
            .contains("retained-second")
    );
    assert!(matches!(
        drive_editor_close_to_completion(&mut editor),
        super::editor::EditorProjectCloseProgress::Complete(_)
    ));
}

#[cfg(all(feature = "tooling", feature = "runtime-ui"))]
#[test]
fn authoring_prefab_source_survives_failed_publication_close_and_reopen() {
    let project = TestProject::new("authoring-prefab-reopen");
    fs::create_dir_all(project.root.join("schemas")).unwrap();
    let manifest = fs::read_to_string(project.root.join("nara.toml"))
        .unwrap()
        .replace(
            "schema/authoring.bundle.json",
            "schemas/authoring.bundle.json",
        );
    fs::write(project.root.join("nara.toml"), manifest).unwrap();
    let resolved =
        nara_reflect::__private::resolve_schema_provider(SCENE_COMPONENTS_SCHEMA_PROVIDER).unwrap();
    let schema_owner =
        PersistentComponentSchemaOwnerId::try_new(SCENE_COMPONENTS_SCHEMA_OWNER_ID.as_str())
            .unwrap();
    let bundle = AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
        schema_owner.clone(),
        resolved.current().catalog().clone(),
        resolved
            .predecessor()
            .map(|record| record.catalog().clone()),
    )])
    .unwrap();
    fs::write(
        project.root.join("schemas/authoring.bundle.json"),
        bundle.to_json_string().unwrap(),
    )
    .unwrap();

    let prefab = AssetPath::new("panel.prefab.json").unwrap();
    fs::write(
        project.root.join("prefabs/panel.prefab.json"),
        PrefabDocument::from_ordered_roots([SceneEntityRecord::new(
            nara_identity::SceneEntityId::new("panel").unwrap(),
        )
        .with_component(
            ComponentTypeId::new("nara.scene.Name"),
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                ComponentValue::String("Panel".to_owned()),
            )
            .with_owner(schema_owner),
        )])
        .to_json_string()
        .unwrap(),
    )
    .unwrap();
    let anchor = nara_identity::SceneEntityId::new("panel-anchor").unwrap();
    let mut anchor_record = SceneEntityRecord::new(anchor.clone());
    anchor_record.prefab = Some(PrefabInstance {
        source: AssetRef::Path(prefab),
        overrides: None,
    });
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        SceneDocument::from_ordered_roots([anchor_record])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();

    let mut host = super::editor::EditorProjectOpenHost::new();
    let mut attempt = host.begin(project.writable_capability(), None);
    let open_deadline = Instant::now() + Duration::from_secs(5);
    let mut editor = loop {
        match host.advance(attempt) {
            super::editor::EditorProjectOpenProgress::Pending(next) => {
                assert!(
                    Instant::now() < open_deadline,
                    "project Open did not settle"
                );
                std::thread::sleep(Duration::from_millis(1));
                attempt = next;
            }
            super::editor::EditorProjectOpenProgress::Opened(session) => break session,
            super::editor::EditorProjectOpenProgress::Rejected(failure) => panic!(
                "the authoring project should open before the Prefab lifecycle test: {:?}",
                failure.diagnostics()
            ),
        }
    };
    let scene_document = editor.workspace().active_document().unwrap();

    editor.test_reject_next_prefab_source_binding();
    let failed = open_prefab_from_scene(&mut editor, scene_document, &anchor);
    assert!(!failed.applied);
    assert_eq!(editor.workspace().len(), 1);

    let opened = open_prefab_from_scene(&mut editor, scene_document, &anchor);
    assert!(opened.applied, "{:?}", opened.diagnostics);
    let prefab_document = opened.opened_document.unwrap();

    let frame = editor.current_frame();
    let prefab_revision = frame
        .workspace()
        .document(prefab_document)
        .unwrap()
        .revision;
    let edited = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyAuthoringFieldEdit {
                    document: prefab_document,
                    base_revision: prefab_revision,
                    edit: AuthoringFieldEdit::single(
                        nara_identity::SceneEntityId::new("panel").unwrap(),
                        ComponentTypeId::new("nara.scene.Name"),
                        ComponentSchemaVersion::ONE,
                        ComponentFieldId::new("value"),
                        ComponentValue::String("Saved Panel".to_owned()),
                    )
                    .unwrap(),
                },
            )],
        ))
        .unwrap();
    assert!(matches!(
        &edited.outcomes()[0],
        EditorActionOutcome::Workspace(report) if report.applied
    ));
    let edited_frame = editor.drive_editor_frame(Duration::ZERO);
    let edited_revision = edited_frame
        .workspace()
        .document(prefab_document)
        .unwrap()
        .revision;
    let save = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            edited_frame.revision(),
            [EditorAction::Persistence(EditorPersistenceAction::Save {
                document: prefab_document,
                base_revision: edited_revision,
            })],
        ))
        .unwrap();
    assert!(matches!(
        save.outcomes()[0],
        EditorActionOutcome::Persistence(
            nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
        )
    ));
    drive_persistence_to_terminal(&mut editor);
    assert!(matches!(
        editor.persistence_view().result(),
        Some(EditorPersistenceResult::Saved { document, .. }) if document == prefab_document
    ));

    let closed = editor.request_workspace_intent(EditorWorkspaceIntent::CloseDocument {
        document: prefab_document,
    });
    assert_eq!(
        closed,
        nara_tooling::EditorWorkspaceIntentRequestResult::Accepted
    );
    assert!(editor.workspace().prefab(prefab_document).is_none());

    let reopened = open_prefab_from_scene(&mut editor, scene_document, &anchor);
    assert!(reopened.applied, "{:?}", reopened.diagnostics);
    let reopened_document = reopened.opened_document.unwrap();
    let reopened_prefab = editor.workspace().prefab(reopened_document).unwrap();
    let reopened_panel = reopened_prefab
        .session()
        .document()
        .entities
        .iter()
        .find(|entity| entity.id.as_str() == "panel")
        .expect("the saved Prefab source should retain its panel entity");
    assert_eq!(
        reopened_panel
            .components
            .get(&ComponentTypeId::new("nara.scene.Name"))
            .and_then(|record| record.value.as_str()),
        Some("Saved Panel")
    );
}

#[cfg(feature = "tooling")]
fn open_prefab_from_scene(
    editor: &mut super::editor::EditorProjectSession,
    scene_document: nara_tooling::EditorDocumentId,
    anchor: &nara_identity::SceneEntityId,
) -> nara_tooling::EditorWorkspaceCommandReport {
    let frame = editor.current_frame();
    let scene_revision = frame.workspace().document(scene_document).unwrap().revision;
    let report = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: scene_document,
                    entity: Some(anchor.clone()),
                }),
                EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                    document: scene_document,
                    base_revision: scene_revision,
                    entity: anchor.clone(),
                }),
            ],
        ))
        .unwrap();
    let EditorActionOutcome::Workspace(opened) = &report.outcomes()[1] else {
        panic!("opening a Prefab source must return a workspace outcome");
    };
    let opened = opened.clone();
    let _ = editor.drive_editor_frame(Duration::ZERO);
    opened
}

#[cfg(all(feature = "tooling", feature = "runtime-ui"))]
#[test]
fn editor_ui_authoring_frame_and_continuous_gesture_share_one_exact_prefab_revision() {
    use nara_ui::__private::{
        UiAuthoringDimensionKind, UiAuthoringDirectEdit, UiAuthoringFieldKind,
        UiAuthoringFieldValue, UiDirectManipulationCapability,
    };

    let project = TestProject::with_image("editor-ui-authoring-frame");
    fs::write(
        project.root.join("nara.toml"),
        r#"schema_version = 1

[project]
source_universe = 'c60a6da4-b6a7-5028-bc53-39111af1a2ba'
native_document_identity = 'project_location'
name = "Editor UI Authoring Frame"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[authoring]
schema_bundle = "schema/authoring.bundle.json"

[runtime]
preset = "local-headless"

[capabilities]
requested = ["runtime-2d", "runtime-ui"]

[ui.primary_surface]
logical_width = 960
logical_height = 540
"#,
    )
    .unwrap();

    let root = nara_identity::SceneEntityId::new("ui-root").unwrap();
    let button = nara_identity::SceneEntityId::new("retry").unwrap();
    let anchor = nara_identity::SceneEntityId::new("ui-anchor").unwrap();
    let prefab = PrefabDocument::new(
        [root.clone()],
        [
            SceneEntityRecord::new(root.clone())
                .with_children([button.clone()])
                .with_component(
                    ComponentTypeId::new("nara.ui.UiRoot"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::map([("layer", ComponentValue::String("base".to_owned()))]),
                    ),
                )
                .with_component(
                    ComponentTypeId::new("nara.ui.UiNode"),
                    ui_node_record("flow", "percent", 1.0, "percent", 1.0),
                ),
            SceneEntityRecord::new(button.clone())
                .with_component(
                    ComponentTypeId::new("nara.ui.UiNode"),
                    ui_node_record("absolute", "px", 180.0, "px", 48.0),
                )
                .with_component(
                    ComponentTypeId::new("nara.ui.UiVisual"),
                    ui_visual_record(Some("textures/player.png")),
                )
                .with_component(
                    ComponentTypeId::new("nara.ui.UiButton"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::map([
                            ("label", ComponentValue::String("Retry".to_owned())),
                            (
                                "action",
                                ComponentValue::String("reference-game.retry".to_owned()),
                            ),
                            ("enabled", ComponentValue::Bool(true)),
                            ("logical_text_size", ComponentValue::f64(20.0).unwrap()),
                        ]),
                    ),
                ),
        ],
    );
    fs::write(
        project.root.join("prefabs/reference-ui.prefab.json"),
        prefab.to_json_string().unwrap(),
    )
    .unwrap();
    let mut anchor_record = SceneEntityRecord::new(anchor.clone());
    anchor_record.prefab = Some(PrefabInstance {
        source: AssetRef::path("reference-ui.prefab.json").unwrap(),
        overrides: None,
    });
    let image = AssetRef::path("textures/player.png").unwrap();
    let player = SceneEntityRecord::new(nara_identity::SceneEntityId::new("player").unwrap())
        .with_component(
            ComponentTypeId::new("nara.transform.Transform2d"),
            identity_transform_record(),
        )
        .with_component(
            ComponentTypeId::new("nara.sprite.Sprite"),
            SceneComponentRecord::new(ComponentSchemaVersion::ONE, sprite_value(&image)),
        );
    fs::write(
        project.root.join("scenes/startup.scene.json"),
        SceneDocument::from_ordered_roots([anchor_record, player])
            .to_json_string()
            .unwrap(),
    )
    .unwrap();

    let mut editor = super::editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        super::editor::TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let scene_document = editor.workspace().active_document().unwrap();
    let scene_frame = editor.current_frame();
    let scene_revision = scene_frame
        .workspace()
        .document(scene_document)
        .unwrap()
        .revision;
    let opened = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            scene_frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: scene_document,
                    entity: Some(anchor.clone()),
                }),
                EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                    document: scene_document,
                    base_revision: scene_revision,
                    entity: anchor,
                }),
            ],
        ))
        .unwrap();
    let EditorActionOutcome::Workspace(opened) = &opened.outcomes()[1] else {
        panic!("opening the UI Prefab must publish a workspace document");
    };
    assert!(opened.applied, "{:?}", opened.diagnostics);
    let prefab_document = opened.opened_document.unwrap();

    let mut frame = editor.drive_editor_frame(Duration::ZERO);
    let ui_frame = editor.current_ui_authoring_frame().unwrap();
    assert_eq!(ui_frame.editor_frame(), frame.revision());
    assert_eq!(ui_frame.document(), prefab_document);
    assert_eq!(
        ui_frame.revision(),
        frame
            .workspace()
            .document(prefab_document)
            .unwrap()
            .revision
    );
    assert_eq!(ui_frame.image_choice_generation().get(), 1);
    let image_choices = ui_frame.image_choices().unwrap().choices();
    assert_eq!(image_choices.len(), 1);
    assert_eq!(
        image_choices[0].source_identity(),
        "3c7c5be4-fd4e-4b65-b8d4-c671f5982186"
    );
    assert_eq!(image_choices[0].project_path(), "textures/player.png");
    let projection = ui_frame.projection().unwrap();
    assert_eq!(projection.hierarchy().len(), 2);
    assert_eq!(projection.source_map().len(), 2);
    #[cfg(feature = "presentation-ui")]
    {
        let nara_ui::__private::UiAuthoringCanvasProjection::Ready(canvas) = projection.canvas()
        else {
            panic!(
                "a valid primary surface must compile one UI Canvas candidate: {:?}",
                projection.canvas()
            );
        };
        let initial_canvas_generation = ui_frame.canvas_generation().unwrap();
        assert_eq!(canvas.logical_size().width(), 960.0);
        assert_eq!(canvas.logical_size().height(), 540.0);
        assert!(canvas.facts().iter().any(|fact| fact.entity() == &button));
        let candidate = editor.take_editor_ui_preview_candidate().unwrap();
        assert_eq!(candidate.document(), prefab_document);
        assert_eq!(candidate.revision(), ui_frame.revision());
        assert_eq!(candidate.geometry_generation(), initial_canvas_generation);
        assert_eq!(candidate.canvas(), canvas);
        let (_, _, _, _, initial_render, _, consumed_canvas) = candidate.into_parts();

        let idle_frame = editor.drive_editor_frame(Duration::ZERO);
        let idle_ui = editor.current_ui_authoring_frame().unwrap();
        assert_eq!(
            idle_ui.canvas_generation(),
            Some(initial_canvas_generation),
            "consuming a candidate must not invalidate unchanged Canvas geometry"
        );
        assert_eq!(
            idle_ui.projection().unwrap().canvas(),
            &nara_ui::__private::UiAuthoringCanvasProjection::Ready(consumed_canvas.clone())
        );
        assert_eq!(idle_ui.editor_frame(), idle_frame.revision());

        assert!(editor.take_editor_ui_preview_candidate().is_none());
        let _ = editor.drive_editor_frame(Duration::ZERO);
        let recovered_ui = editor.current_ui_authoring_frame().unwrap();
        assert_eq!(
            recovered_ui.canvas_generation(),
            Some(initial_canvas_generation),
            "GPU recovery must not change unchanged Canvas geometry"
        );
        let recovered_candidate = editor.take_editor_ui_preview_candidate().unwrap();
        assert_eq!(recovered_candidate.document(), prefab_document);
        assert_eq!(recovered_candidate.revision(), ui_frame.revision());
        assert_eq!(
            recovered_candidate.geometry_generation(),
            initial_canvas_generation
        );
        assert_eq!(recovered_candidate.canvas(), &consumed_canvas);
        let (_, _, _, _, recovered_render, _, _) = recovered_candidate.into_parts();
        assert_eq!(
            recovered_render.source_generation(),
            initial_render.source_generation()
        );
        assert!(recovered_render.frame_index() > initial_render.frame_index());
        assert_eq!(recovered_render.target(), initial_render.target());

        let settled_frame = editor.drive_editor_frame(Duration::ZERO);
        assert_eq!(
            editor
                .current_ui_authoring_frame()
                .unwrap()
                .canvas_generation(),
            Some(initial_canvas_generation)
        );
        frame = settled_frame;
    }
    let button_node = projection
        .hierarchy()
        .iter()
        .find(|node| node.entity() == &button)
        .unwrap();
    assert_eq!(
        button_node.gizmos().move_node(),
        UiDirectManipulationCapability::Editable
    );
    assert_eq!(
        button_node.gizmos().resize_node(),
        UiDirectManipulationCapability::Editable
    );
    let width_key = projection
        .fields()
        .iter()
        .find(|field| {
            field.key().entity() == &button
                && field.key().primary_field().as_str() == "style.width.kind"
                && matches!(field.kind(), UiAuthoringFieldKind::Dimension)
        })
        .unwrap()
        .key()
        .clone();
    let label = projection
        .fields()
        .iter()
        .find(|field| {
            field.key().entity() == &button && field.key().primary_field().as_str() == "label"
        })
        .unwrap();
    let accepted_label = ui_frame
        .lower_field_edit(label.key(), label.value().clone())
        .unwrap();
    let forged_label = EditorAtomicFieldEdit::new(
        accepted_label.target().clone(),
        [EditorFieldWrite::new(
            ComponentFieldId::new("label"),
            ComponentValueKind::String,
            ComponentValue::String(String::new()),
        )
        .unwrap()],
    )
    .unwrap();
    let rejected_revision = editor
        .workspace()
        .prefab(prefab_document)
        .unwrap()
        .revision();
    let rejected_history = editor
        .workspace()
        .prefab(prefab_document)
        .unwrap()
        .session()
        .history_status();
    let rejected = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [EditorAction::Mutate(
                EditorDocumentMutation::ApplyPrefabFieldEdit { edit: forged_label },
            )],
        ))
        .unwrap();
    assert!(matches!(
        rejected.outcomes()[0],
        EditorActionOutcome::Rejected(EditorActionRejection::FieldContractMismatch {
            document
        }) if document == prefab_document
    ));
    assert_eq!(
        editor
            .workspace()
            .prefab(prefab_document)
            .unwrap()
            .revision(),
        rejected_revision
    );
    assert_eq!(
        editor
            .workspace()
            .prefab(prefab_document)
            .unwrap()
            .session()
            .history_status(),
        rejected_history
    );
    frame = editor.drive_editor_frame(Duration::ZERO);

    let baseline = ui_frame
        .lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
            entity: button.clone(),
            delta: [0.0, 0.0],
        })
        .unwrap();
    let begin = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: prefab_document,
                    entity: Some(button.clone()),
                }),
                EditorAction::Gesture(EditorGestureAction::Begin { baseline }),
            ],
        ))
        .unwrap();
    let EditorActionOutcome::Gesture(EditorGestureOutcome::Began(gesture)) = &begin.outcomes()[1]
    else {
        panic!(
            "the exact accepted baseline must begin one gesture: {:?}",
            begin.outcomes()[1]
        );
    };
    let gesture_id = gesture.stamp().id();
    let revision_before = editor
        .workspace()
        .prefab(prefab_document)
        .unwrap()
        .revision();
    let history_before = editor
        .workspace()
        .prefab(prefab_document)
        .unwrap()
        .session()
        .history_status();

    let first_preview_frame = editor.drive_editor_frame(Duration::ZERO);
    let first_ui = editor.current_ui_authoring_frame().unwrap();
    assert_eq!(first_ui.selection().top_entity(), Some(&button));
    let first_preview = first_ui
        .lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
            entity: button.clone(),
            delta: [40.0, 0.0],
        })
        .unwrap();
    let previewed = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            first_preview_frame.revision(),
            [EditorAction::Gesture(EditorGestureAction::Preview {
                gesture: gesture_id,
                edit: first_preview,
            })],
        ))
        .unwrap();
    assert!(matches!(
        previewed.outcomes()[0],
        EditorActionOutcome::Gesture(EditorGestureOutcome::Previewed(_))
    ));
    assert_eq!(
        editor
            .workspace()
            .prefab(prefab_document)
            .unwrap()
            .revision(),
        revision_before
    );
    assert_eq!(
        editor
            .workspace()
            .prefab(prefab_document)
            .unwrap()
            .session()
            .history_status(),
        history_before
    );

    let second_preview_frame = editor.drive_editor_frame(Duration::ZERO);
    let second_ui = editor.current_ui_authoring_frame().unwrap();
    #[cfg(feature = "presentation-ui")]
    assert!(second_ui.canvas_generation().unwrap() > first_ui.canvas_generation().unwrap());
    assert!(matches!(
        second_ui
            .projection()
            .unwrap()
            .field(&width_key)
            .unwrap()
            .value(),
        UiAuthoringFieldValue::Dimension { value, .. } if *value == 220.0
    ));
    let second_preview = second_ui
        .lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
            entity: button.clone(),
            delta: [10.0, 0.0],
        })
        .unwrap();
    let _ = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            second_preview_frame.revision(),
            [EditorAction::Gesture(EditorGestureAction::Preview {
                gesture: gesture_id,
                edit: second_preview,
            })],
        ))
        .unwrap();

    let commit_frame = editor.drive_editor_frame(Duration::ZERO);
    let commit_ui = editor.current_ui_authoring_frame().unwrap();
    let committed = commit_ui
        .lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
            entity: button.clone(),
            delta: [10.0, 0.0],
        })
        .unwrap();
    let commit = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            commit_frame.revision(),
            [EditorAction::Gesture(EditorGestureAction::Commit {
                gesture: gesture_id,
                edit: committed,
            })],
        ))
        .unwrap();
    assert!(matches!(
        &commit.outcomes()[0],
        EditorActionOutcome::Workspace(report) if report.applied
    ));
    let slot = editor.workspace().prefab(prefab_document).unwrap();
    assert_eq!(
        slot.revision().generation(),
        revision_before.generation() + 1
    );
    assert_eq!(slot.session().history_status().undo_depth, 1);

    let accepted_frame = editor.drive_editor_frame(Duration::ZERO);
    let accepted_ui = editor.current_ui_authoring_frame().unwrap();
    let cancel_baseline = accepted_ui
        .lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
            entity: button.clone(),
            delta: [0.0, 0.0],
        })
        .unwrap();
    let began = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            accepted_frame.revision(),
            [EditorAction::Gesture(EditorGestureAction::Begin {
                baseline: cancel_baseline,
            })],
        ))
        .unwrap();
    let EditorActionOutcome::Gesture(EditorGestureOutcome::Began(cancel_gesture)) =
        &began.outcomes()[0]
    else {
        panic!("a successor gesture must begin");
    };
    let cancel_id = cancel_gesture.stamp().id();
    let cancel_preview_frame = editor.drive_editor_frame(Duration::ZERO);
    let cancel_ui = editor.current_ui_authoring_frame().unwrap();
    let cancel_preview = cancel_ui
        .lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
            entity: button.clone(),
            delta: [20.0, 0.0],
        })
        .unwrap();
    let _ = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            cancel_preview_frame.revision(),
            [EditorAction::Gesture(EditorGestureAction::Preview {
                gesture: cancel_id,
                edit: cancel_preview,
            })],
        ))
        .unwrap();
    let cancel_frame = editor.drive_editor_frame(Duration::ZERO);
    let cancelled = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            cancel_frame.revision(),
            [EditorAction::Gesture(EditorGestureAction::Cancel {
                gesture: cancel_id,
                reason: EditorGestureCancelReason::Escape,
            })],
        ))
        .unwrap();
    assert!(matches!(
        cancelled.outcomes()[0],
        EditorActionOutcome::Gesture(EditorGestureOutcome::Cancelled(
            EditorGestureCancelReason::Escape
        ))
    ));
    let restored_frame = editor.drive_editor_frame(Duration::ZERO);
    let restored = editor.current_ui_authoring_frame().unwrap();
    assert_eq!(restored.editor_frame(), restored_frame.revision());
    assert_eq!(
        restored.last_gesture_cancellation(),
        Some(EditorGestureCancelReason::Escape)
    );
    assert!(matches!(
        restored
            .projection()
            .unwrap()
            .field(&width_key)
            .unwrap()
            .value(),
        UiAuthoringFieldValue::Dimension { value, .. } if *value == 240.0
    ));
    assert_eq!(
        editor
            .workspace()
            .prefab(prefab_document)
            .unwrap()
            .session()
            .history_status()
            .undo_depth,
        1
    );

    let stale_baseline = restored
        .lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
            entity: button.clone(),
            delta: [0.0, 0.0],
        })
        .unwrap();
    let successor_edit = restored
        .lower_field_edit(
            &width_key,
            UiAuthoringFieldValue::Dimension {
                kind: UiAuthoringDimensionKind::Pixels,
                value: 250.0,
            },
        )
        .unwrap();
    let stale = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            restored_frame.revision(),
            [
                EditorAction::Gesture(EditorGestureAction::Begin {
                    baseline: stale_baseline.clone(),
                }),
                EditorAction::Mutate(EditorDocumentMutation::ApplyPrefabFieldEdit {
                    edit: successor_edit,
                }),
            ],
        ))
        .unwrap();
    let EditorActionOutcome::Gesture(EditorGestureOutcome::Began(stale_gesture)) =
        &stale.outcomes()[0]
    else {
        panic!("the stale-gesture probe must begin from an accepted frame");
    };
    assert!(matches!(
        &stale.outcomes()[1],
        EditorActionOutcome::Workspace(report) if report.applied
    ));
    let stale_gesture_id = stale_gesture.stamp().id();
    let stale_frame = editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor
            .current_ui_authoring_frame()
            .unwrap()
            .last_gesture_cancellation(),
        Some(EditorGestureCancelReason::StaleRevision)
    );
    let stale_retry = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            stale_frame.revision(),
            [EditorAction::Gesture(EditorGestureAction::Preview {
                gesture: stale_gesture_id,
                edit: stale_baseline,
            })],
        ))
        .unwrap();
    assert!(matches!(
        stale_retry.outcomes()[0],
        EditorActionOutcome::Rejected(_)
    ));

    let handoff_frame = editor.drive_editor_frame(Duration::ZERO);
    let handoff_ui = editor.current_ui_authoring_frame().unwrap();
    let handoff_baseline = handoff_ui
        .lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
            entity: button.clone(),
            delta: [0.0, 0.0],
        })
        .unwrap();
    let began = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            handoff_frame.revision(),
            [EditorAction::Gesture(EditorGestureAction::Begin {
                baseline: handoff_baseline,
            })],
        ))
        .unwrap();
    let EditorActionOutcome::Gesture(EditorGestureOutcome::Began(handoff_gesture)) =
        &began.outcomes()[0]
    else {
        panic!("handoff probe must begin from the current Prefab");
    };
    let handoff_id = handoff_gesture.stamp().id();
    let handoff_commit_frame = editor.drive_editor_frame(Duration::ZERO);
    let handoff_commit = editor
        .current_ui_authoring_frame()
        .unwrap()
        .lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
            entity: button.clone(),
            delta: [25.0, 0.0],
        })
        .unwrap();
    let handoff_revision = editor
        .workspace()
        .prefab(prefab_document)
        .unwrap()
        .revision();
    let handoff_history = editor
        .workspace()
        .prefab(prefab_document)
        .unwrap()
        .session()
        .history_status();
    let handoff = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            handoff_commit_frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
                    document: scene_document,
                }),
                EditorAction::Gesture(EditorGestureAction::Commit {
                    gesture: handoff_id,
                    edit: handoff_commit,
                }),
            ],
        ))
        .unwrap();
    assert!(matches!(
        &handoff.outcomes()[0],
        EditorActionOutcome::Workspace(report) if report.applied
    ));
    assert!(matches!(
        handoff.outcomes()[1],
        EditorActionOutcome::Gesture(EditorGestureOutcome::Rejected(_))
    ));
    assert_eq!(
        editor
            .workspace()
            .prefab(prefab_document)
            .unwrap()
            .revision(),
        handoff_revision
    );
    assert_eq!(
        editor
            .workspace()
            .prefab(prefab_document)
            .unwrap()
            .session()
            .history_status(),
        handoff_history
    );

    let scene_frame = editor.drive_editor_frame(Duration::ZERO);
    let returned = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            scene_frame.revision(),
            [EditorAction::Navigate(
                EditorNavigationAction::SetActiveDocument {
                    document: prefab_document,
                },
            )],
        ))
        .unwrap();
    assert!(matches!(
        &returned.outcomes()[0],
        EditorActionOutcome::Workspace(report) if report.applied
    ));
    let _ = editor.drive_editor_frame(Duration::ZERO);
    assert_eq!(
        editor
            .current_ui_authoring_frame()
            .unwrap()
            .last_gesture_cancellation(),
        Some(EditorGestureCancelReason::SourceHandoff)
    );

    let structure_frame = editor.current_frame();
    let structure_ui = editor.current_ui_authoring_frame().unwrap();
    let retained_canvas_generation = structure_ui.canvas_generation();
    let stale_delete = structure_ui
        .lower_structure_intent(&nara_ui::__private::UiStructureIntent::Delete {
            target: button.clone(),
        })
        .unwrap();
    let structure_baseline = structure_ui
        .lower_direct_edit(&UiAuthoringDirectEdit::ResizeBy {
            entity: button.clone(),
            delta: [0.0, 0.0],
        })
        .unwrap();
    let forged_draft =
        nara_tooling::PrefabCompositionEntityDraft::new(std::collections::BTreeMap::new()).unwrap();
    let cross_document_create = structure_ui
        .lower_structure_intent(&nara_ui::__private::UiStructureIntent::Create {
            parameters: nara_ui::__private::UiCreateParameters::Container,
            placement: ScenePlacement::last(ScenePlacementOwner::Parent {
                parent: root.clone(),
            }),
        })
        .unwrap();
    let EditorDocumentMutation::ComposePrefab {
        command: cross_document_command,
        ..
    } = cross_document_create
    else {
        panic!("a catalog create intent must lower to Prefab composition");
    };
    let scene_revision = structure_frame
        .workspace()
        .document(scene_document)
        .unwrap()
        .revision;
    let rejected_structure = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            structure_frame.revision(),
            [
                EditorAction::Gesture(EditorGestureAction::Begin {
                    baseline: structure_baseline,
                }),
                EditorAction::Mutate(EditorDocumentMutation::ComposePrefab {
                    document: prefab_document,
                    base_revision: structure_ui.revision(),
                    command: nara_tooling::PrefabCompositionCommand::CreateEntity {
                        draft: forged_draft,
                        placement: ScenePlacement::last(ScenePlacementOwner::Parent {
                            parent: root.clone(),
                        }),
                    },
                }),
                EditorAction::Mutate(EditorDocumentMutation::ComposePrefab {
                    document: scene_document,
                    base_revision: scene_revision,
                    command: cross_document_command,
                }),
            ],
        ))
        .unwrap();
    assert!(matches!(
        rejected_structure.outcomes()[0],
        EditorActionOutcome::Gesture(EditorGestureOutcome::Began(_))
    ));
    assert!(matches!(
        rejected_structure.outcomes()[1],
        EditorActionOutcome::Rejected(EditorActionRejection::StructureContractMismatch {
            document
        }) if document == prefab_document
    ));
    assert!(matches!(
        rejected_structure.outcomes()[2],
        EditorActionOutcome::Rejected(EditorActionRejection::StructureContractMismatch {
            document
        }) if document == scene_document
    ));
    let retained_ui = editor.current_ui_authoring_frame().unwrap();
    assert!(Arc::ptr_eq(&structure_ui, &retained_ui));
    assert_eq!(retained_ui.canvas_generation(), retained_canvas_generation);

    let retained_frame = editor.drive_editor_frame(Duration::ZERO);
    let retained_ui = editor.current_ui_authoring_frame().unwrap();
    assert!(retained_ui.gesture().is_some());
    let create = retained_ui
        .lower_structure_intent(&nara_ui::__private::UiStructureIntent::Create {
            parameters: nara_ui::__private::UiCreateParameters::Container,
            placement: ScenePlacement::last(ScenePlacementOwner::Parent { parent: root }),
        })
        .unwrap();
    let created_batch = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            retained_frame.revision(),
            [
                EditorAction::Mutate(create),
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: prefab_document,
                    entity: Some(button),
                }),
            ],
        ))
        .unwrap();
    let EditorActionOutcome::Workspace(created) = &created_batch.outcomes()[0] else {
        panic!("a frame-lowered structure intent must reach the workspace");
    };
    assert!(created.applied, "{:?}", created.diagnostics);
    let Some(nara_tooling::PrefabCompositionOutcome::Created {
        entity: created_entity,
    }) = &created.prefab_composition_outcome
    else {
        panic!("successful structure creation must return its exact source identity");
    };
    assert!(matches!(
        &created_batch.outcomes()[1],
        EditorActionOutcome::Rejected(EditorActionRejection::StaleDocumentFact {
            document,
            ..
        }) if *document == prefab_document
    ));
    assert!(editor.current_ui_authoring_frame().is_none());
    #[cfg(feature = "presentation-ui")]
    assert!(editor.take_editor_ui_preview_candidate().is_none());

    let successor_frame = editor.drive_editor_frame(Duration::ZERO);
    let successor_ui = editor.current_ui_authoring_frame().unwrap();
    assert_eq!(
        successor_ui.last_gesture_cancellation(),
        Some(EditorGestureCancelReason::StaleRevision)
    );
    assert_eq!(successor_ui.selection().top_entity(), Some(created_entity));
    let stale = editor
        .dispatch_editor_actions(EditorActionBatch::new(
            successor_frame.revision(),
            [
                EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
                    document: scene_document,
                }),
                EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                    document: prefab_document,
                    entity: successor_ui.selection().top_entity().cloned(),
                }),
                EditorAction::Mutate(stale_delete),
            ],
        ))
        .unwrap();
    assert!(matches!(
        stale.outcomes()[0],
        EditorActionOutcome::Workspace(ref report) if report.applied
    ));
    assert!(matches!(
        stale.outcomes()[1],
        EditorActionOutcome::Rejected(EditorActionRejection::ActiveDocumentChanged {
            expected,
            actual: Some(actual),
        }) if expected == prefab_document && actual == scene_document
    ));
    assert!(matches!(
        stale.outcomes()[2],
        EditorActionOutcome::Rejected(EditorActionRejection::StaleDocumentFact {
            document,
            ..
        }) if document == prefab_document
    ));
}

#[cfg(feature = "tooling")]
#[test]
fn editor_play_runtime_uses_the_plan_owned_schema_snapshot() {
    let project = TestProject::new("editor-play-schema-snapshot");
    let mut editor = super::editor::EditorProjectSession::open_trusted_compiled(
        project.capability(),
        super::editor::TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let expected = editor.test_schema_snapshot();
    let play_request = editor.request_play(nara_tooling::EditorPlayCommand::Play);
    for _ in 0..32 {
        if editor.play_view().state() == nara_tooling::EditorPlayState::Running {
            break;
        }
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::yield_now();
    }
    let running_state = editor.play_view().state();
    let runtime_snapshot = editor.test_runtime_schema_snapshot();
    let stop_request = editor.request_play(nara_tooling::EditorPlayCommand::Stop);
    for _ in 0..32 {
        if editor.play_view().state() == nara_tooling::EditorPlayState::Empty {
            break;
        }
        editor.drive_editor_frame(Duration::ZERO);
        std::thread::yield_now();
    }
    let stopped_state = editor.play_view().state();

    assert_eq!(
        play_request,
        nara_tooling::EditorPlayRequestResult::Accepted
    );
    assert_eq!(running_state, nara_tooling::EditorPlayState::Running);
    assert!(runtime_snapshot.is_some_and(|actual| expected.ptr_eq(&actual)));
    assert_eq!(
        stop_request,
        nara_tooling::EditorPlayRequestResult::Accepted
    );
    assert_eq!(stopped_state, nara_tooling::EditorPlayState::Empty);
}

#[cfg(all(
    feature = "tooling",
    feature = "runtime-ui",
    feature = "presentation-2d",
    feature = "presentation-ui",
    feature = "desktop-winit",
    feature = "render-wgpu"
))]
#[test]
fn editor_play_profile_keeps_logical_producers_without_native_gpu_contributions() {
    let project = TestProject::new("editor-play-placement");
    fs::write(
        project.root.join("nara.toml"),
        r#"schema_version = 1

[project]
source_universe = 'c60a6da4-b6a7-5028-bc53-39111af1a2ba'
native_document_identity = 'project_location'
name = "Editor Play Placement"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[runtime]
preset = "local-headless"

[capabilities]
requested = ["runtime-2d"]

[profiles.desktop.capabilities]
requested = [
    "runtime-2d",
    "runtime-ui",
    "presentation-2d",
    "presentation-ui",
    "desktop-winit",
    "render-wgpu",
]

[profiles.desktop.runtime]
preset = "local-headless"

[profiles.desktop.window]
enabled = true
width = 960
height = 540
resizable = true
"#,
    )
    .unwrap();
    let root = project.capability();
    let manifest = root
        .open_file(&RelativePath::new(PROJECT_MANIFEST).unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, Some("desktop")).unwrap();
    drop(manifest);

    let shipping = crate::project_host::composition::resolve_project_runtime_request(
        &candidate,
        crate::project_host::composition::project_runtime_plugins_with_recipe(
            &candidate,
            crate::ProductRecipe::new(),
            None,
        ),
    )
    .unwrap();
    let editor = crate::project_host::composition::resolve_project_runtime_request(
        &candidate,
        crate::project_host::composition::editor_play_runtime_plugins_with_recipe(
            &candidate,
            crate::ProductRecipe::new(),
            None,
            None,
        ),
    )
    .unwrap();
    let contains = |plan: &RuntimePlan, plugin: PluginId| {
        plan.plugin_plan()
            .entries()
            .iter()
            .any(|entry| entry.plugin_id() == plugin)
    };

    for plugin in [
        nara_render_wgpu::WGPU_RENDER_PLUGIN_ID,
        nara_render_wgpu::WGPU_SPRITE_SUBMITTER_PLUGIN_ID,
        nara_render_wgpu::WGPU_UI_SUBMITTER_PLUGIN_ID,
        crate::WINIT_UI_ACCESSIBILITY_CONTRIBUTION_PLUGIN_ID,
    ] {
        assert!(contains(&shipping, plugin));
        assert!(!contains(&editor, plugin));
    }
    for plugin in [
        nara_window::WINDOW_PLUGIN_ID,
        nara_render::RENDER_PLUGIN_ID,
        nara_sprite_render::SPRITE_RENDER_PLUGIN_ID,
        nara_ui_render::UI_RENDER_PLUGIN_ID,
    ] {
        assert!(contains(&editor, plugin));
    }
    assert_eq!(
        editor.normalized_capabilities(),
        candidate.normalized_capabilities()
    );
}

#[cfg(feature = "tooling")]
#[test]
fn failed_and_uncertain_saves_do_not_advance_workspace_persistence_state() {
    let project = TestProject::new("editor-save-failure-state");
    let mut editor = super::editor::EditorProjectSession::open_trusted_compiled(
        project.writable_capability(),
        super::editor::TrustedCompiledEditorProjectIntent::new(),
    )
    .unwrap();
    let document = editor.workspace().active_document().unwrap();
    let base_revision = editor.workspace().scene(document).unwrap().revision();
    assert!(
        editor
            .apply_workspace_command(EditorWorkspaceCommand::ApplyScenePatch {
                document,
                base_revision,
                patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                    entity: SceneEntityRecord::new(
                        nara_identity::SceneEntityId::new("unsaved").unwrap(),
                    ),
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                }]),
            })
            .applied
    );
    let saved_revision = editor.workspace().scene(document).unwrap().saved_revision();
    let saved_digest = editor.workspace().scene(document).unwrap().saved_digest();

    editor.test_fail_next_save(
        document,
        nara_tooling::EditorPersistenceFailureStage::Encode,
    );
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document),
        }),
        nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
    ));
    drive_persistence_to_terminal(&mut editor);
    assert!(matches!(
        editor.persistence_view().result(),
        Some(EditorPersistenceResult::Failed {
            request: nara_tooling::EditorPersistenceRequestKind::Save,
            document: Some(failed_document),
            stage: nara_tooling::EditorPersistenceFailureStage::Encode,
        }) if failed_document == document
    ));
    let slot = editor.workspace().scene(document).unwrap();
    assert_eq!(slot.saved_revision(), saved_revision);
    assert_eq!(slot.saved_digest(), saved_digest);
    assert!(slot.is_dirty());

    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::AcknowledgeResult {
            attempt: editor
                .persistence_view()
                .attempt()
                .expect("the test observed a terminal persistence result")
        }),
        nara_tooling::EditorPersistenceRequestResult::Acknowledged { .. }
    ));
    editor.test_reject_next_valid_persistence_receipt(document);
    assert!(matches!(
        editor.request_persistence(EditorPersistenceCommand::Save {
            document: Some(document),
        }),
        nara_tooling::EditorPersistenceRequestResult::Accepted { .. }
    ));
    drive_persistence_to_terminal(&mut editor);
    let uncertain_result = editor.persistence_view().result();
    assert!(
        matches!(
            uncertain_result,
            Some(EditorPersistenceResult::PersistenceUncertain {
                document: uncertain_document,
                ..
            }) if uncertain_document == document
        ),
        "expected an uncertain save, got {uncertain_result:?}"
    );
    let slot = editor.workspace().scene(document).unwrap();
    assert_eq!(slot.saved_revision(), saved_revision);
    assert_eq!(slot.saved_digest(), saved_digest);
    assert!(slot.is_dirty());
}

#[test]
fn sequential_starts_share_immutable_authority_and_rebuild_runtime_sessions() {
    let project = TestProject::new("publication-cut");
    let (snapshot, plan) = project.snapshot_and_session_plan();
    let shared_scene = snapshot.expanded_scene() as *const SceneDocument as usize;
    let shared_registry = plan.schema_validation().snapshot().clone();
    let stable_plan = plan.plugin_plan().fingerprint();
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());

    let mut first_attempt = host
        .begin_start(snapshot.clone(), plan.clone(), Vec::new())
        .unwrap();
    assert!(host.start_claim.is_active(first_attempt.epoch));
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
    let diagnostics = host.complete_start(&mut first_attempt).unwrap();
    assert!(!diagnostics.has_errors());
    let first = running_owner_identity(&host);
    close_host(&mut host);

    let mut second_attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    assert!(host.start_claim.is_active(second_attempt.epoch));
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
    let diagnostics = host.complete_start(&mut second_attempt).unwrap();
    assert!(!diagnostics.has_errors());
    let second = running_owner_identity(&host);

    assert_ne!(first.generation, second.generation);
    assert_ne!(first.service_session, second.service_session);
    assert_ne!(first.identity_domain, second.identity_domain);
    assert_eq!(first.snapshot_scene, shared_scene);
    assert_eq!(second.snapshot_scene, shared_scene);
    assert!(first.registry_snapshot.ptr_eq(&shared_registry));
    assert!(second.registry_snapshot.ptr_eq(&shared_registry));
    assert_eq!(first.plan_fingerprint, stable_plan);
    assert_eq!(second.plan_fingerprint, stable_plan);
    close_host(&mut host);
}

#[test]
fn bundled_startup_activation_shares_the_spawned_source_and_resolves_its_receipt() {
    let project = TestProject::new("startup-activation-source");
    let (snapshot, plan) = project.snapshot_and_plan(false);
    let source_pointer = snapshot.expanded_scene() as *const SceneDocument;
    let source_entity_ids = snapshot
        .expanded_scene()
        .entities
        .iter()
        .map(|entity| entity.id.clone())
        .collect::<Vec<_>>();
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());

    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    host.complete_start(&mut attempt).unwrap();

    let ProjectHostSlot::Running(published) = &host.slot else {
        panic!("runtime is not visible in the Host slot");
    };
    let world = published.runtime().world();
    let activation = world.resource::<CurrentSceneOwner>();
    assert_eq!(activation.source() as *const SceneDocument, source_pointer);
    assert_eq!(activation.receipt().entity_ids(), source_entity_ids);
    for entity_id in &source_entity_ids {
        assert!(matches!(
            activation.receipt().resolve(world, entity_id),
            EntityLookup::Resolved(entity) if world.get_entity(entity).is_ok()
        ));
    }

    close_host(&mut host);
}

#[test]
fn product_dependents_observe_the_promoted_startup_activation() {
    let project = TestProject::new("startup-activation-order");
    let observations = Arc::new(AtomicUsize::new(0));
    let (snapshot, plan) =
        project.snapshot_and_plan_with_plugin(activation_probe_definition(&observations));
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());

    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    host.complete_start(&mut attempt).unwrap();

    assert_eq!(observations.load(Ordering::SeqCst), 1);
    close_host(&mut host);
}

#[test]
fn editor_startup_source_has_an_exact_independent_retained_budget_lease() {
    let project = TestProject::new("editor-startup-source-budget");
    let (snapshot, plan, loader) =
        project.snapshot_plan_and_loader_with_limits(ProjectContentLimits::default());
    let budget_host = loader.budget_host();
    let baseline_snapshot = budget_host.snapshot();
    let baseline = baseline_snapshot.active(ProjectContentBudgetKind::RetainedBytes);
    let baseline_reservations = baseline_snapshot.active_reservations();

    let source = snapshot
        .prepare_editor_startup_scene(
            snapshot.selected_scene_source_key(),
            snapshot.expanded_scene(),
            plan.schema_validation().bindings(),
        )
        .unwrap();
    let source_charge = source.retained_bytes();
    assert_eq!(
        budget_host
            .snapshot()
            .active(ProjectContentBudgetKind::RetainedBytes),
        baseline + source_charge
    );
    assert_eq!(
        budget_host.snapshot().active_reservations(),
        baseline_reservations + 1
    );
    drop(source);
    assert_eq!(
        budget_host
            .snapshot()
            .active(ProjectContentBudgetKind::RetainedBytes),
        baseline
    );
    assert_eq!(
        budget_host.snapshot().active_reservations(),
        baseline_reservations
    );
    drop(snapshot);
    assert_eq!(
        budget_host
            .snapshot()
            .active(ProjectContentBudgetKind::RetainedBytes),
        0
    );
    drop((plan, loader));

    let exact_total = baseline.checked_add(source_charge).unwrap();
    let exact_limits = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::RetainedBytes, exact_total)
        .unwrap();
    let (snapshot, plan, loader) = project.snapshot_plan_and_loader_with_limits(exact_limits);
    let exact_budget = loader.budget_host();
    let source = snapshot
        .prepare_editor_startup_scene(
            snapshot.selected_scene_source_key(),
            snapshot.expanded_scene(),
            plan.schema_validation().bindings(),
        )
        .unwrap();
    assert_eq!(
        exact_budget
            .snapshot()
            .active(ProjectContentBudgetKind::RetainedBytes),
        exact_total
    );
    drop((source, snapshot));
    assert_eq!(
        exact_budget
            .snapshot()
            .active(ProjectContentBudgetKind::RetainedBytes),
        0
    );
    drop((plan, loader));

    let rejected_limits = ProjectContentLimits::default()
        .with_limit(
            ProjectContentBudgetKind::RetainedBytes,
            exact_total.checked_sub(1).unwrap(),
        )
        .unwrap();
    let (snapshot, plan, loader) = project.snapshot_plan_and_loader_with_limits(rejected_limits);
    let rejected_budget = loader.budget_host();
    let rejected_baseline_reservations = rejected_budget.snapshot().active_reservations();
    let result = snapshot.prepare_editor_startup_scene(
        snapshot.selected_scene_source_key(),
        snapshot.expanded_scene(),
        plan.schema_validation().bindings(),
    );
    assert!(result.is_err());
    assert_eq!(
        rejected_budget
            .snapshot()
            .active(ProjectContentBudgetKind::RetainedBytes),
        baseline
    );
    assert_eq!(
        rejected_budget.snapshot().active_reservations(),
        rejected_baseline_reservations
    );
    drop(snapshot);
    assert_eq!(
        rejected_budget
            .snapshot()
            .active(ProjectContentBudgetKind::RetainedBytes),
        0
    );
}

#[test]
fn classified_product_startup_failure_preserves_detail_and_prevents_publication() {
    let project = TestProject::new("classified-startup-failure");
    let (snapshot, plan) =
        project.snapshot_and_plan_with_plugin(classified_startup_failure_definition());
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();

    let error = host.complete_start(&mut attempt).unwrap_err();

    let diagnostic = error
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == "nara.test.startup-configuration-invalid")
        .expect("the classified startup diagnostic reaches Project Host");
    assert_eq!(
        diagnostic.summary().as_str(),
        "Startup configuration is invalid"
    );
    assert_eq!(
        identifier_field(
            &error.diagnostics,
            "nara.test.startup-configuration-invalid",
            "fault-origin",
        ),
        Some("nara.test.product-startup")
    );
    assert!(!matches!(host.slot, ProjectHostSlot::Running(_)));
    drain_host_cleanup(&mut host);
}

#[test]
fn editor_activation_lease_survives_failed_startup_until_delayed_cleanup_finishes() {
    let project = TestProject::new("editor-startup-cleanup-lease");
    let root = project.capability();
    let manifest = root
        .open_file(&RelativePath::new(PROJECT_MANIFEST).unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    drop(manifest);
    let control = Arc::new(CloseFailureControl::new(false));
    let builds = Arc::new(AtomicUsize::new(0));
    let closes = Arc::new(AtomicUsize::new(0));
    let request = project_runtime_plugins(&candidate)
        .insert_after::<GameplayCommandPlugin>(close_probe_definition(&builds, &closes))
        .insert_after::<CloseProbePlugin>(close_failure_definition(&control, false, false))
        .insert_after::<CloseFailurePlugin>(classified_startup_failure_definition());
    let plan = resolve_runtime_plan(&candidate, request, built_in_schema_providers()).unwrap();
    let loader = ProjectContentLoader::new(root).unwrap();
    let budget_host = loader.budget_host();
    let snapshot = load_runtime_snapshot(&loader, &candidate, &plan);
    let baseline_reservations = budget_host.snapshot().active_reservations();
    let source = snapshot
        .prepare_editor_startup_scene(
            snapshot.selected_scene_source_key(),
            snapshot.expanded_scene(),
            plan.schema_validation().bindings(),
        )
        .unwrap();
    let editor_source_charge = source.retained_bytes();
    assert_eq!(
        budget_host.snapshot().active_reservations(),
        baseline_reservations + 1
    );
    drop(loader);

    let mut host = ProjectHost::new(RuntimeClosePolicy::new(Duration::ZERO));
    let mut attempt = host
        .begin_editor_start(snapshot, plan, source, Vec::new())
        .unwrap();
    let error = host.complete_start(&mut attempt).unwrap_err();
    let failure_code = first_code(&error.diagnostics).to_owned();
    let runtime_was_visible = matches!(host.slot, ProjectHostSlot::Running(_));
    let retained_after_failure = budget_host
        .snapshot()
        .active(ProjectContentBudgetKind::RetainedBytes);
    let reservations_after_failure = budget_host.snapshot().active_reservations();
    let cleanup_outcome = host.drive_cleanup_once();
    let retained_during_incomplete_cleanup = budget_host
        .snapshot()
        .active(ProjectContentBudgetKind::RetainedBytes);
    let reservations_during_incomplete_cleanup = budget_host.snapshot().active_reservations();

    control.release();
    drain_host_cleanup(&mut host);
    let released = budget_host.snapshot();

    assert_eq!(failure_code, "nara.test.startup-configuration-invalid");
    assert!(!runtime_was_visible);
    assert!(retained_after_failure >= editor_source_charge);
    assert!(reservations_after_failure > 0);
    assert!(matches!(
        cleanup_outcome,
        CleanupDriveOutcome::Retiring | CleanupDriveOutcome::RetirementIncomplete
    ));
    assert_eq!(retained_during_incomplete_cleanup, retained_after_failure);
    assert_eq!(
        reservations_during_incomplete_cleanup,
        reservations_after_failure
    );
    assert_eq!(released.active(ProjectContentBudgetKind::RetainedBytes), 0);
    assert_eq!(released.active_reservations(), 0);
}

#[test]
fn route_capacity_rejection_keeps_the_same_project_start_attempt_retryable() {
    let mut reservations = Vec::new();
    while let Ok(reservation) = RuntimeAdmissionReservation::try_acquire() {
        reservations.push(reservation);
    }
    assert!(reservations.len() >= 131);

    let project = TestProject::new("runtime-route-capacity");
    let (snapshot, plan) = project.snapshot_and_plan(false);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();

    let error = host.complete_start(&mut attempt).unwrap_err();
    assert_eq!(
        first_code(&error.diagnostics),
        "project.run.runtime-capacity-exhausted"
    );
    assert!(attempt.inputs.is_some());
    assert!(host.start_claim.is_active(attempt.epoch));
    assert!(matches!(host.slot, ProjectHostSlot::Empty));

    drop(
        reservations
            .pop()
            .expect("saturated route capacity has one releasable reservation"),
    );
    let diagnostics = host.complete_start(&mut attempt).unwrap();
    assert!(!diagnostics.has_errors());
    close_host(&mut host);
    drop(reservations);
}

#[test]
fn project_host_publishes_snapshot_images_without_copying_the_retained_pixels() {
    let project = TestProject::with_image("image-publication");
    let (snapshot, plan) = project.snapshot_and_plan(false);
    assert_eq!(snapshot.images().len(), 1);
    let snapshot_pixels = snapshot.images()[0].image().pixels().as_ptr();
    let snapshot_source = snapshot.images()[0].image().source().clone();
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());

    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    host.complete_start(&mut attempt).unwrap();

    {
        let ProjectHostSlot::Running(published) = &host.slot else {
            panic!("runtime is not visible in the Host slot");
        };
        let runtime_images = published.runtime().world().resource::<Assets<ImageAsset>>();
        let runtime_images = runtime_images.iter().collect::<Vec<_>>();
        assert_eq!(runtime_images.len(), 1);
        let runtime_image = runtime_images[0].1;
        assert_eq!(runtime_image.source(), &snapshot_source);
        assert_eq!(
            runtime_image.pixels().as_ptr(),
            snapshot_pixels,
            "runtime publication must share the snapshot-owned pixel allocation and its lease",
        );
    }

    close_host(&mut host);
}

#[test]
fn published_host_keeps_only_live_content_and_residency_owners() {
    let project = TestProject::with_image("host-content-custody");
    let (snapshot, plan, loader) =
        project.snapshot_plan_and_loader_with_limits(ProjectContentLimits::default());
    let startup_revision = snapshot.revision();
    let budget = loader.budget_host();
    assert_eq!(budget.snapshot().active_reservations(), 2);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();

    host.complete_start(&mut attempt).unwrap();

    assert_eq!(host.running_content_revision(), Some(startup_revision));
    assert_eq!(budget.snapshot().active_reservations(), 2);
    assert_eq!(budget.snapshot().active_generations(), 1);
    let escaped_image = host
        .with_running_world(|world| {
            world
                .resource::<Assets<ImageAsset>>()
                .iter()
                .next()
                .unwrap()
                .1
                .share_retained()
                .unwrap()
        })
        .unwrap();

    close_host(&mut host);

    assert_eq!(host.running_content_revision(), None);
    assert_eq!(budget.snapshot().active_reservations(), 1);
    assert_eq!(budget.snapshot().active_generations(), 1);
    assert!(!escaped_image.pixels().is_empty());
    drop(escaped_image);
    assert_eq!(budget.snapshot().active_reservations(), 0);
    assert_eq!(budget.snapshot().active_generations(), 0);
}

#[test]
fn concurrent_starts_share_immutable_content_but_isolate_every_runtime_owner() {
    let project = TestProject::new("runtime-owner-isolation");
    let (snapshot, plan) = project.snapshot_and_session_plan();
    let shared_scene = snapshot.expanded_scene() as *const SceneDocument as usize;
    let shared_registry = plan.schema_validation().snapshot().clone();
    let stable_plan = plan.plugin_plan().fingerprint();
    let mut first_host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut second_host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut first_attempt = first_host
        .begin_start(snapshot.clone(), plan.clone(), Vec::new())
        .unwrap();
    let mut second_attempt = second_host.begin_start(snapshot, plan, Vec::new()).unwrap();
    first_host.complete_start(&mut first_attempt).unwrap();
    second_host.complete_start(&mut second_attempt).unwrap();

    let first = running_owner_identity(&first_host);
    let second = running_owner_identity(&second_host);

    assert_ne!(first.generation, second.generation);
    assert_ne!(first.world, second.world);
    assert_ne!(first.command_queue, second.command_queue);
    assert_ne!(first.fixed_time, second.fixed_time);
    assert_ne!(first.task_pools, second.task_pools);
    assert_ne!(first.service_session, second.service_session);
    assert_ne!(first.identity_domain, second.identity_domain);
    assert_eq!(first.snapshot_scene, shared_scene);
    assert_eq!(second.snapshot_scene, shared_scene);
    assert!(first.registry_snapshot.ptr_eq(&shared_registry));
    assert!(second.registry_snapshot.ptr_eq(&shared_registry));
    assert_eq!(first.plan_fingerprint, stable_plan);
    assert_eq!(second.plan_fingerprint, stable_plan);
    close_host(&mut first_host);
    close_host(&mut second_host);
}

#[test]
fn start_attempts_are_bound_to_one_host_and_remain_usable_after_foreign_rejection() {
    let project = TestProject::new("host-bound-attempt");
    let (snapshot, plan) = project.snapshot_and_plan(false);
    let mut first_host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut second_host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut first_attempt = first_host
        .begin_start(snapshot.clone(), plan.clone(), Vec::new())
        .unwrap();
    let mut second_attempt = second_host.begin_start(snapshot, plan, Vec::new()).unwrap();

    let error = second_host.complete_start(&mut first_attempt).unwrap_err();

    assert_eq!(first_code(&error.diagnostics), "project.run.stale-start");
    assert!(first_host.start_claim.is_active(first_attempt.epoch));
    assert!(second_host.start_claim.is_active(second_attempt.epoch));
    assert!(matches!(first_host.slot, ProjectHostSlot::Empty));
    assert!(matches!(second_host.slot, ProjectHostSlot::Empty));

    first_host.complete_start(&mut first_attempt).unwrap();
    second_host.complete_start(&mut second_attempt).unwrap();
    close_host(&mut first_host);
    close_host(&mut second_host);
}

#[test]
fn dropping_an_unclaimed_attempt_releases_the_host_start_slot() {
    let project = TestProject::new("cancelled-attempt");
    let (snapshot, plan) = project.snapshot_and_plan(false);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let attempt = host
        .begin_start(snapshot.clone(), plan.clone(), Vec::new())
        .unwrap();
    let cancelled_epoch = attempt.epoch;

    drop(attempt);

    assert!(!host.start_claim.is_active(cancelled_epoch));
    let mut replacement = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    assert_ne!(replacement.epoch, cancelled_epoch);
    host.complete_start(&mut replacement).unwrap();
    close_host(&mut host);
}

#[test]
fn product_tick_uses_one_exact_step() {
    let project = TestProject::new("exact-step");
    let (snapshot, plan) = project.snapshot_and_plan(false);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    host.complete_start(&mut attempt).unwrap();
    let ProjectHostSlot::Running(published) = &mut host.slot else {
        panic!("runtime was not published");
    };
    let tick_before = published
        .runtime()
        .world()
        .resource::<nara_app::FixedTime>()
        .tick();

    assert_eq!(host.drive_one_fixed_tick().unwrap(), None);

    let ProjectHostSlot::Running(published) = &host.slot else {
        panic!("runtime was not retained after exact stepping");
    };
    assert_eq!(
        published
            .runtime()
            .world()
            .resource::<nara_app::FixedTime>()
            .tick(),
        tick_before + 1
    );
    assert_eq!(published.runtime().state(), RuntimeState::Paused);
    close_host(&mut host);
}

#[test]
fn prepared_publication_unwind_keeps_the_host_slot_empty() {
    let project = TestProject::new("publication-reservation-unwind");
    let (snapshot, plan) = project.snapshot_and_plan(false);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    let retained_before = runtime_quarantine_status().current_thread_retained();

    let unwound = catch_unwind(AssertUnwindSafe(|| {
        let _prepared = host.prepare_start(&mut attempt).unwrap();
        panic!("injected pre-publication unwind");
    }));

    assert!(unwound.is_err());
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
    assert!(!host.start_claim.any_active());
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while runtime_quarantine_status().current_thread_retained() > retained_before
        && std::time::Instant::now() < deadline
    {
        let _ = drive_runtime_quarantine();
        std::thread::yield_now();
    }
    assert_eq!(
        runtime_quarantine_status().current_thread_retained(),
        retained_before
    );
}

#[test]
fn reported_execution_fault_is_not_reemitted_by_runtime_retirement() {
    let drives: [fn(&mut ProjectHost) -> Result<(), HostFault>; 3] = [
        |host| host.drive_one_frame(Duration::ZERO).map(|_| ()),
        |host| host.drive_one_fixed_tick().map(|_| ()),
        |host| host.drive_running_runtime(Duration::ZERO),
    ];
    for drive in drives {
        let project = TestProject::new("reported-execution-fault-retirement");
        let (snapshot, plan) = project.snapshot_and_plan(false);
        let mut host = ProjectHost::new(RuntimeClosePolicy::default());
        let mut attempt = host
            .begin_start(snapshot.clone(), plan.clone(), Vec::new())
            .unwrap();
        host.complete_start(&mut attempt).unwrap();
        let fault = RuntimeFault::engine(
            RuntimeFaultKind::System,
            "nara.test.reported-execution-fault",
        );
        assert!(
            host.running_runtime_mut()
                .unwrap()
                .fault_reporter()
                .report(fault.clone())
        );

        let mut primary = drive(&mut host).unwrap_err().diagnostics;
        assert_eq!(
            identifier_field(&primary, "project.run.runtime-faulted", "fault-source"),
            Some("nara.test.reported-execution-fault")
        );
        host.retire_running();
        let (failed, diagnostics) = drive_host_cleanup_to_completion(&mut host);
        assert!(!failed);
        assert!(!diagnostics.has_errors());
        primary.extend(diagnostics);
        assert_eq!(
            primary
                .iter()
                .filter(|diagnostic| diagnostic.code().as_str() == "project.run.runtime-faulted")
                .count(),
            1
        );

        // Identical fault metadata in a successor is still a new, undelivered execution fault.
        let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
        host.complete_start(&mut attempt).unwrap();
        assert!(
            host.running_runtime_mut()
                .unwrap()
                .fault_reporter()
                .report(fault)
        );
        host.retire_running();
        let (failed, diagnostics) = drive_host_cleanup_to_completion(&mut host);
        assert!(failed);
        assert_eq!(
            identifier_field(&diagnostics, "project.run.runtime-faulted", "fault-source"),
            Some("nara.test.reported-execution-fault")
        );
        assert!(matches!(host.slot, ProjectHostSlot::Empty));
    }
}

#[test]
fn reported_execution_fault_does_not_suppress_terminal_cleanup_failures() {
    struct FailedClose;
    impl RuntimeCloseParticipant for FailedClose {
        fn begin_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            Ok(RuntimeCloseProgress::CompleteWithError {
                code: "nara.test.retirement-participant-failed",
            })
        }

        fn poll_close(
            &mut self,
            context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            self.begin_close(context)
        }
    }

    let project = TestProject::new("reported-fault-with-cleanup-failures");
    let control = Arc::new(CloseFailureControl::new(false));
    let (snapshot, plan) = project.snapshot_and_fault_plan(
        close_probe_definition(
            &Arc::new(AtomicUsize::new(0)),
            &Arc::new(AtomicUsize::new(0)),
        ),
        close_failure_definition(&control, false, true),
    );
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    attempt
        .inputs
        .as_mut()
        .unwrap()
        .obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.retirement-participant"),
            FailedClose,
        )
        .unwrap();
    host.complete_start(&mut attempt).unwrap();
    assert!(
        host.running_runtime_mut()
            .unwrap()
            .fault_reporter()
            .report(RuntimeFault::engine(
                RuntimeFaultKind::System,
                "nara.test.primary-before-cleanup"
            ))
    );
    let primary = host.drive_one_frame(Duration::ZERO).unwrap_err();
    assert!(report_has_code(
        &primary.diagnostics,
        "project.run.runtime-faulted"
    ));

    host.retire_running();
    assert!(matches!(
        host.drive_cleanup_once(),
        CleanupDriveOutcome::Retiring | CleanupDriveOutcome::RetirementIncomplete
    ));
    control.release();
    let (failed, diagnostics) = drive_host_cleanup_to_completion(&mut host);

    assert!(failed);
    assert!(!report_has_code(
        &diagnostics,
        "project.run.runtime-faulted"
    ));
    assert!(report_has_code(&diagnostics, "project.run.cleanup-failed"));
    assert!(report_has_code(
        &diagnostics,
        "project.run.cleanup-plugin-shutdown"
    ));
    assert_eq!(
        identifier_field(
            &diagnostics,
            "project.run.cleanup-participant-error",
            "failure-code"
        ),
        Some("nara.test.retirement-participant-failed")
    );
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
}

#[test]
fn runtime_retirement_reports_a_fault_first_raised_during_cleanup() {
    let project = TestProject::new("retirement-first-runtime-fault");
    let control = Arc::new(CloseFailureControl::new(true));
    let (snapshot, plan) = project.snapshot_and_fault_plan(
        close_probe_definition(
            &Arc::new(AtomicUsize::new(0)),
            &Arc::new(AtomicUsize::new(0)),
        ),
        close_failure_definition(&control, true, false),
    );
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    host.complete_start(&mut attempt).unwrap();
    assert!(host.running_runtime_mut().unwrap().fault().is_none());

    host.retire_running();
    let (failed, diagnostics) = drive_host_cleanup_to_completion(&mut host);

    assert!(failed);
    assert_eq!(
        identifier_field(&diagnostics, "project.run.runtime-faulted", "fault-source"),
        Some("nara.test.project-close-fault")
    );
    assert!(!report_has_code(&diagnostics, "project.run.cleanup-failed"));
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
}

#[test]
fn outcome_capture_delivers_typed_fault_metadata_before_retirement() {
    let project = TestProject::new("outcome-fault-delivery");
    let (snapshot, plan) = project.snapshot_and_plan(false);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    host.complete_start(&mut attempt).unwrap();
    host.drive_one_frame(Duration::ZERO).unwrap();
    let detail = nara_app::RuntimeFaultDetail::new(
        "game.outcome-fault",
        "Outcome capture observed a runtime fault",
        "game.outcome",
    )
    .unwrap();
    host.running_runtime_mut()
        .unwrap()
        .fault_reporter()
        .report(RuntimeFault::engine_with_detail(
            RuntimeFaultKind::RequiredService,
            "nara.test.outcome-fault",
            detail,
        ));
    let failure = host
        .capture_outcome::<RuntimeServiceSessionId>()
        .unwrap_err();
    host.retire_running();
    let (failed, diagnostics) = drive_host_cleanup_to_completion(&mut host);
    assert_eq!(
        identifier_field(&failure.diagnostics, "game.outcome-fault", "fault-source"),
        Some("nara.test.outcome-fault")
    );
    assert_eq!(
        identifier_field(&failure.diagnostics, "game.outcome-fault", "fault-origin"),
        Some("game.outcome")
    );
    assert!(!failed);
    assert!(!diagnostics.has_errors());
}

#[test]
fn close_time_runtime_fault_cannot_complete_the_product_run() {
    for fail_shutdown in [false, true] {
        for released in [false, true] {
            let project = TestProject::new("close-time-runtime-fault");
            let control = Arc::new(CloseFailureControl::new(released));
            let (snapshot, plan) = project.snapshot_and_fault_plan(
                close_probe_definition(
                    &Arc::new(AtomicUsize::new(0)),
                    &Arc::new(AtomicUsize::new(0)),
                ),
                close_failure_definition(&control, true, fail_shutdown),
            );
            let mut host = ProjectHost::new(RuntimeClosePolicy::default());
            let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
            host.complete_start(&mut attempt).unwrap();

            let failure = host.stop_running().unwrap_err();
            control.release();
            let cleanup = host
                .has_cleanup_owner()
                .then(|| drive_host_cleanup_to_completion(&mut host));

            assert!(report_has_code(
                &failure.diagnostics,
                "project.run.runtime-faulted"
            ));
            assert_eq!(
                identifier_field(
                    &failure.diagnostics,
                    "project.run.runtime-faulted",
                    "fault-kind"
                ),
                Some("required-service")
            );
            assert_eq!(
                identifier_field(
                    &failure.diagnostics,
                    "project.run.runtime-faulted",
                    "fault-source"
                ),
                Some("nara.test.project-close-fault")
            );
            assert_eq!(
                report_has_code(&failure.diagnostics, "project.run.cleanup-failed"),
                fail_shutdown
            );
            if let Some((failed, diagnostics)) = cleanup {
                assert_eq!(failed, fail_shutdown);
                assert!(!report_has_code(
                    &diagnostics,
                    "project.run.runtime-faulted"
                ));
                assert_eq!(
                    report_has_code(&diagnostics, "project.run.cleanup-plugin-shutdown"),
                    fail_shutdown
                );
            }
            assert!(matches!(host.slot, ProjectHostSlot::Empty));
        }
    }
}

#[test]
fn delayed_cleanup_preserves_plugin_shutdown_failure_until_terminal_result() {
    let project = TestProject::new("delayed-plugin-shutdown-failure");
    let control = Arc::new(CloseFailureControl::new(false));
    let (snapshot, plan) = project.snapshot_and_fault_plan(
        close_probe_definition(
            &Arc::new(AtomicUsize::new(0)),
            &Arc::new(AtomicUsize::new(0)),
        ),
        close_failure_definition(&control, false, true),
    );
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    host.complete_start(&mut attempt).unwrap();

    let failure = host.stop_running().unwrap_err();
    assert!(report_has_code(
        &failure.diagnostics,
        "project.run.cleanup-failed"
    ));
    assert!(host.has_cleanup_owner());

    control.release();
    let (failed, diagnostics) = drive_host_cleanup_to_completion(&mut host);
    assert!(failed);
    assert!(report_has_code(
        &diagnostics,
        "project.run.cleanup-plugin-shutdown"
    ));
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
#[test]
fn desktop_host_preserves_real_close_evidence_until_retry_finishes() {
    let project = TestProject::new("desktop-close-evidence");
    let control = Arc::new(CloseFailureControl::new(false));
    let (snapshot, plan) = project.snapshot_and_fault_plan(
        close_probe_definition(
            &Arc::new(AtomicUsize::new(0)),
            &Arc::new(AtomicUsize::new(0)),
        ),
        close_failure_definition(&control, false, false),
    );
    let close_timeout = Duration::from_millis(50);
    let mut host = ProjectHost::new(RuntimeClosePolicy::new(close_timeout));
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    host.complete_start(&mut attempt).unwrap();

    {
        let runtime = host.running_runtime_mut().unwrap();
        assert!(matches!(
            runtime.request_control(RuntimeControl::Stop),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        std::thread::sleep(close_timeout + Duration::from_millis(10));
        for _ in 0..8 {
            if runtime.state() == RuntimeState::CloseIncomplete {
                break;
            }
            runtime.drive(Duration::ZERO).unwrap();
        }
        assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
    }
    assert!(
        host.running_runtime_close_evidence()
            .unwrap()
            .causes()
            .contains(&RuntimeCloseCause::DeadlineExceeded)
    );

    control.release();
    {
        let runtime = host.running_runtime_mut().unwrap();
        assert!(matches!(
            runtime.request_control(RuntimeControl::RetryClose),
            RuntimeControlRequestResult::Accepted(_)
        ));
        for _ in 0..8 {
            if runtime.state() == RuntimeState::Stopped {
                break;
            }
            runtime.drive(Duration::ZERO).unwrap();
        }
        assert_eq!(runtime.state(), RuntimeState::Stopped);
    }
    assert!(
        host.running_runtime_close_evidence()
            .unwrap()
            .causes()
            .contains(&RuntimeCloseCause::DeadlineExceeded)
    );
    host.retire_running();
    drain_host_cleanup(&mut host);
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
#[test]
fn desktop_product_report_preserves_a_real_candidate_startup_failure() {
    let project = TestProject::new("desktop-startup-failure");
    let builds = Arc::new(AtomicUsize::new(0));
    let closes = Arc::new(AtomicUsize::new(0));
    let recipe = crate::ProductRecipe::new()
        .add_plugin::<StartupFailurePlugin>()
        .unwrap();
    let intent = DesktopRunIntent::new().with_recipe(recipe);
    let mut run = desktop_run_with_probe(
        project.capability(),
        intent,
        RuntimeProbe::new(close_probe_definition(&builds, &closes)),
    );

    let first = run.execute();
    assert_eq!(first.outcome(), DesktopRunOutcome::CleanupIncomplete);
    let report = (0..8)
        .find_map(|_| {
            let report = run.execute();
            (report.outcome() != DesktopRunOutcome::CleanupIncomplete).then_some(report)
        })
        .expect("desktop startup-failure cleanup should reach a bounded terminal report");

    assert_eq!(report.outcome(), DesktopRunOutcome::Failed);
    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.run.startup-failed"
            && diagnostic.summary().as_str() == "Project runtime startup failed"
    }));
    assert_eq!(builds.load(Ordering::SeqCst), 1);
    assert_eq!(closes.load(Ordering::SeqCst), 1);
}

#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
#[test]
fn desktop_recipe_facade_reports_startup_failure_before_opening_a_window() {
    let project = TestProject::new("desktop-recipe-startup-failure");
    let recipe = crate::ProductRecipe::new()
        .add_plugin::<RecipeStartupFailurePlugin>()
        .unwrap();
    let mut run = DesktopRun::from_recipe(project.capability(), recipe);

    let report = (0..8)
        .find_map(|_| {
            let report = run.execute();
            (report.outcome() != DesktopRunOutcome::CleanupIncomplete).then_some(report)
        })
        .expect("desktop recipe startup failure should reach a bounded terminal report");

    assert_eq!(report.outcome(), DesktopRunOutcome::Failed);
    assert!(report.diagnostics().iter().any(|diagnostic| {
        diagnostic.code().as_str() == "project.run.startup-failed"
            && diagnostic.summary().as_str() == "Project runtime startup failed"
    }));
}

#[test]
fn startup_panic_releases_the_host_claim_and_quarantines_the_runtime_owner() {
    let project = TestProject::new("startup-panic-claim");
    let (snapshot, plan) = project.snapshot_and_fault_plan(
        close_probe_definition(
            &Arc::new(AtomicUsize::new(0)),
            &Arc::new(AtomicUsize::new(0)),
        ),
        panic_startup_definition(),
    );
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    let retained_before = runtime_quarantine_status().process_retained();

    let unwound = catch_unwind(AssertUnwindSafe(|| {
        let _ = host.complete_start(&mut attempt);
    }));

    assert!(unwound.is_err());
    assert!(!host.start_claim.any_active());
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while runtime_quarantine_status().process_retained() > retained_before
        && std::time::Instant::now() < deadline
    {
        let _ = drive_runtime_quarantine();
        std::thread::yield_now();
    }
    assert_eq!(
        runtime_quarantine_status().process_retained(),
        retained_before
    );
}

#[test]
fn every_start_phase_failure_keeps_runtime_invisible_and_closes_acquired_owners_once() {
    let project = TestProject::new("start-phase-failures");
    assert_start_phase_failure(
        &project,
        prepare_failure_definition(),
        "nara.test.project-prepare-failure",
        0,
    );
    assert_start_phase_failure(
        &project,
        PluginDefinition::infallible::<BuildFailurePlugin, _>(
            BUILD_FAILURE_DEFINITION_ID,
            b"project-build-failure-v1",
            BuildFailurePlugin::default,
        ),
        "project.run.plugin-hook-failed",
        1,
    );
    assert_start_phase_failure(
        &project,
        PluginDefinition::infallible::<FinishFailurePlugin, _>(
            FINISH_FAILURE_DEFINITION_ID,
            b"project-finish-failure-v1",
            FinishFailurePlugin::default,
        ),
        "project.run.plugin-hook-failed",
        1,
    );
    assert_start_phase_failure(
        &project,
        PluginDefinition::infallible::<CommandFailurePlugin, _>(
            COMMAND_FAILURE_DEFINITION_ID,
            b"project-command-failure-v1",
            CommandFailurePlugin::default,
        ),
        "project.run.command-queue-missing",
        1,
    );
    assert_start_phase_failure(
        &project,
        PluginDefinition::infallible::<StartupFailurePlugin, _>(
            STARTUP_FAILURE_DEFINITION_ID,
            b"project-startup-failure-v1",
            StartupFailurePlugin::default,
        ),
        "project.run.startup-failed",
        1,
    );
}

#[test]
fn runtime_scope_failure_report_preserves_fault_identity() {
    let report = runtime_scope_failure_report(&RuntimeFault::engine(
        RuntimeFaultKind::System,
        "nara.ecs.fallible-execution",
    ));

    assert_eq!(first_code(&report), "project.run.runtime-faulted");
    assert_eq!(
        identifier_field(&report, "project.run.runtime-faulted", "fault-kind"),
        Some("system")
    );
    assert_eq!(
        identifier_field(&report, "project.run.runtime-faulted", "fault-source"),
        Some("nara.ecs.fallible-execution")
    );
}

#[test]
fn public_failure_diagnostics_preserve_safe_phase_and_owner_identity() {
    let composition = runtime_plan_failure_report(&RuntimePlanError::Composition(
        CompositionError::UnrequestedProductCapability {
            plugin: BUILD_FAILURE_PLUGIN_ID,
            capability: nara_project::ProductCapability::Runtime2d,
        },
    ));
    assert_eq!(
        identifier_field(&composition, "project.run.composition-invalid", "reason"),
        Some("unrequested-product-capability")
    );
    assert_eq!(
        identifier_field(&composition, "project.run.composition-invalid", "plugin"),
        Some(BUILD_FAILURE_PLUGIN_ID.as_str())
    );
    assert_eq!(
        identifier_field(
            &composition,
            "project.run.composition-invalid",
            "capability"
        ),
        Some("runtime-2d")
    );

    let alternatives = runtime_plan_failure_report(&RuntimePlanError::Composition(
        CompositionError::UnrequestedProductCapabilityAlternatives {
            plugin: BUILD_FAILURE_PLUGIN_ID,
            alternatives: nara_project::ProductCapabilitySet::from_capabilities([
                nara_project::ProductCapability::RuntimeUi,
                nara_project::ProductCapability::Runtime2d,
            ]),
        },
    ));
    assert_eq!(
        identifier_field(&alternatives, "project.run.composition-invalid", "reason"),
        Some("unrequested-product-capability-alternatives")
    );
    assert_eq!(
        identifier_field(&alternatives, "project.run.composition-invalid", "plugin"),
        Some(BUILD_FAILURE_PLUGIN_ID.as_str())
    );
    assert_eq!(
        identifier_field(
            &alternatives,
            "project.run.composition-invalid",
            "alternatives"
        ),
        Some("runtime-2d/runtime-ui")
    );

    let plan = runtime_plan_failure_report(&RuntimePlanError::PluginPlan(
        nara_app::PluginPlanError::MissingPlugin {
            plugin: BUILD_FAILURE_PLUGIN_ID,
            required: CLOSE_PROBE_PLUGIN_ID,
        },
    ));
    assert_eq!(
        identifier_field(&plan, "project.run.plugin-plan-invalid", "reason"),
        Some("missing-plugin")
    );
    assert_eq!(
        identifier_field(&plan, "project.run.plugin-plan-invalid", "plugin"),
        Some(BUILD_FAILURE_PLUGIN_ID.as_str())
    );
    assert_eq!(
        identifier_field(&plan, "project.run.plugin-plan-invalid", "related-plugin"),
        Some(CLOSE_PROBE_PLUGIN_ID.as_str())
    );

    let preparation = runtime_construction_failure_report(
        &RuntimeConstructionError::Plugin(PluginInstantiationError::Prepare(
            PluginPrepareError::Failed {
                plugin: PREPARE_FAILURE_PLUGIN_ID,
                source: PluginDomainError::new(
                    "nara.test.prepare-rejected",
                    "Plugin preparation was rejected",
                )
                .unwrap(),
            },
        )),
        None,
    );
    assert_eq!(
        identifier_field(&preparation, "nara.test.prepare-rejected", "plugin"),
        Some(PREPARE_FAILURE_PLUGIN_ID.as_str())
    );
    assert_eq!(
        identifier_field(&preparation, "nara.test.prepare-rejected", "failure-code"),
        Some("nara.test.prepare-rejected")
    );

    let hook = runtime_construction_failure_report(
        &RuntimeConstructionError::Plugin(PluginInstantiationError::Plugin(
            PluginError::HookPanicked {
                plugin: FINISH_FAILURE_PLUGIN_ID,
                hook: nara_app::PluginHook::Finish,
            },
        )),
        None,
    );
    assert_eq!(
        identifier_field(&hook, "project.run.plugin-hook-failed", "plugin"),
        Some(FINISH_FAILURE_PLUGIN_ID.as_str())
    );
    assert_eq!(
        identifier_field(&hook, "project.run.plugin-hook-failed", "hook"),
        Some("finish")
    );
}

#[test]
fn a_fault_winning_the_publish_lock_is_retired_without_becoming_visible() {
    let project = TestProject::new("publication-fault");
    let builds = Arc::new(AtomicUsize::new(0));
    let closes = Arc::new(AtomicUsize::new(0));
    let (snapshot, plan) =
        project.snapshot_and_plan_with_plugin(close_probe_definition(&builds, &closes));
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    arm_publication_fault_for_test();

    let error = host.complete_start(&mut attempt).unwrap_err();

    assert_eq!(
        first_code(&error.diagnostics),
        "project.run.publication-faulted"
    );
    assert!(!host.start_claim.any_active());
    assert!(!matches!(host.slot, ProjectHostSlot::Running(_)));
    assert_eq!(builds.load(Ordering::SeqCst), 1);
    drain_host_cleanup(&mut host);
    assert_eq!(closes.load(Ordering::SeqCst), 1);
}

#[test]
fn admission_scope_failure_before_materialization_is_reported_without_panicking() {
    let project = TestProject::new("admission-scope-failure");
    let (snapshot, plan) =
        project.snapshot_and_plan_with_plugin(admission_scope_failure_definition());
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();

    let error = host.complete_start(&mut attempt).unwrap_err();

    assert_eq!(
        first_code(&error.diagnostics),
        "project.run.runtime-faulted"
    );
    assert!(!host.start_claim.any_active());
    assert!(!matches!(host.slot, ProjectHostSlot::Running(_)));
    drain_host_cleanup(&mut host);
}

#[test]
fn duplicate_completion_rejects_without_disturbing_the_published_runtime() {
    let project = TestProject::new("duplicate-publication");
    let (snapshot, plan) = project.snapshot_and_plan(false);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    host.complete_start(&mut attempt).unwrap();
    let generation = running_generation(&host);

    let error = host.complete_start(&mut attempt).unwrap_err();

    assert_eq!(first_code(&error.diagnostics), "project.run.stale-start");
    assert_eq!(running_generation(&host), generation);
    close_host(&mut host);
}

#[test]
fn real_task_shutdown_retains_the_host_epoch_and_owner_until_later_drives_complete() {
    let project = TestProject::new("task-cleanup-custody");
    let control = Arc::new(BlockingTaskControl::default());
    let definition_control = Arc::clone(&control);
    let blocking_task = PluginDefinition::infallible::<BlockingTaskPlugin, _>(
        BLOCKING_TASK_DEFINITION_ID,
        b"project-blocking-task-v1",
        move || BlockingTaskPlugin {
            control: Arc::clone(&definition_control),
        },
    );
    let (snapshot, plan) = project.snapshot_and_plan_with_plugin(blocking_task);
    let replacement_snapshot = snapshot.clone();
    let replacement_plan = plan.clone();
    let mut host = ProjectHost::new(RuntimeClosePolicy::new(Duration::ZERO));
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    let epoch = attempt.epoch;
    host.complete_start(&mut attempt).unwrap();
    let first_generation = running_generation(&host);
    assert_eq!(host.drive_one_fixed_tick().unwrap(), None);

    let stop_started = std::time::Instant::now();
    let error = host.stop_running().unwrap_err();

    assert!(stop_started.elapsed() < Duration::from_secs(1));
    assert_eq!(
        first_code(&error.diagnostics),
        "project.run.cleanup-incomplete"
    );
    assert!(report_has_code(
        &error.diagnostics,
        "project.run.cleanup-deadline-exceeded"
    ));
    let ProjectHostSlot::Cleaning {
        epoch: retained_epoch,
        owner,
        ..
    } = &host.slot
    else {
        panic!("unfinished task cleanup did not retain Host custody");
    };
    assert_eq!(*retained_epoch, epoch);
    assert_eq!(
        owner.state(),
        RuntimeCandidateRetirementState::RetirementIncomplete
    );
    assert_eq!(control.instances.load(Ordering::SeqCst), 1);

    let replacement_error = match host.begin_start(
        replacement_snapshot.clone(),
        replacement_plan.clone(),
        Vec::new(),
    ) {
        Ok(_) => panic!("a Host with an incomplete retirement must reject replacement"),
        Err(error) => error,
    };
    assert_eq!(
        first_code(&replacement_error.diagnostics),
        "project.run.busy"
    );
    assert_eq!(control.instances.load(Ordering::SeqCst), 1);

    control.release();
    drain_host_cleanup(&mut host);

    assert_eq!(control.instances.load(Ordering::SeqCst), 1);
    assert!(!host.start_claim.any_active());

    let mut replacement = host
        .begin_start(replacement_snapshot, replacement_plan, Vec::new())
        .unwrap();
    host.complete_start(&mut replacement).unwrap();
    assert_ne!(running_generation(&host), first_generation);
    assert_eq!(control.instances.load(Ordering::SeqCst), 2);
    control.release();
    close_host(&mut host);
    assert_eq!(control.instances.load(Ordering::SeqCst), 2);
}

fn assert_start_phase_failure(
    project: &TestProject,
    failure: PluginDefinition,
    expected_code: &str,
    expected_probe_builds: usize,
) {
    let retained_before = runtime_quarantine_status().process_retained();
    let builds = Arc::new(AtomicUsize::new(0));
    let closes = Arc::new(AtomicUsize::new(0));
    let (snapshot, plan) =
        project.snapshot_and_fault_plan(close_probe_definition(&builds, &closes), failure);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();

    let error = host.complete_start(&mut attempt).unwrap_err();
    let actual_code = first_code(&error.diagnostics).to_owned();
    let claim_remained_active = host.start_claim.any_active();
    let runtime_became_visible = matches!(host.slot, ProjectHostSlot::Running(_));
    let actual_builds = builds.load(Ordering::SeqCst);
    drain_host_cleanup(&mut host);
    let actual_closes = closes.load(Ordering::SeqCst);
    let redundant_cleanup = host.drive_cleanup_once();
    let closes_after_redundant_cleanup = closes.load(Ordering::SeqCst);
    let retained_after = runtime_quarantine_status().process_retained();

    assert_eq!(actual_code, expected_code);
    assert!(!claim_remained_active);
    assert!(!runtime_became_visible);
    assert_eq!(actual_builds, expected_probe_builds);
    assert_eq!(actual_closes, expected_probe_builds);
    assert!(matches!(
        redundant_cleanup,
        CleanupDriveOutcome::Complete { failed: false, .. }
    ));
    assert_eq!(closes_after_redundant_cleanup, expected_probe_builds);
    assert_eq!(
        retained_after, retained_before,
        "a start-phase failure retained a runtime owner after Host cleanup"
    );
}

fn close_probe_definition(
    builds: &Arc<AtomicUsize>,
    closes: &Arc<AtomicUsize>,
) -> PluginDefinition {
    let builds = Arc::clone(builds);
    let closes = Arc::clone(closes);
    PluginDefinition::infallible::<CloseProbePlugin, _>(
        CLOSE_PROBE_DEFINITION_ID,
        b"project-close-probe-v1",
        move || CloseProbePlugin {
            builds: Arc::clone(&builds),
            closes: Arc::clone(&closes),
        },
    )
}

fn prepare_failure_definition() -> PluginDefinition {
    PluginDefinition::fallible::<PrepareFailurePlugin, _>(
        PREPARE_FAILURE_DEFINITION_ID,
        b"project-prepare-failure-v1",
        || {
            Err(PluginDomainError::new(
                "nara.test.project-prepare-failure",
                "Project plugin preparation failed",
            )
            .unwrap())
        },
    )
}

fn admission_scope_failure_definition() -> PluginDefinition {
    PluginDefinition::infallible::<AdmissionScopeFailurePlugin, _>(
        ADMISSION_SCOPE_FAILURE_DEFINITION_ID,
        b"project-admission-scope-failure-v1",
        AdmissionScopeFailurePlugin::default,
    )
}

fn activation_probe_definition(observations: &Arc<AtomicUsize>) -> PluginDefinition {
    let observations = Arc::clone(observations);
    PluginDefinition::infallible::<ActivationProbePlugin, _>(
        ACTIVATION_PROBE_DEFINITION_ID,
        b"startup-activation-probe-v1",
        move || ActivationProbePlugin {
            observations: Arc::clone(&observations),
        },
    )
}

fn classified_startup_failure_definition() -> PluginDefinition {
    PluginDefinition::infallible::<ClassifiedStartupFailurePlugin, _>(
        CLASSIFIED_STARTUP_FAILURE_DEFINITION_ID,
        b"classified-startup-failure-v1",
        ClassifiedStartupFailurePlugin::default,
    )
}

fn close_failure_definition(
    control: &Arc<CloseFailureControl>,
    report_fault: bool,
    fail_shutdown: bool,
) -> PluginDefinition {
    let control = Arc::clone(control);
    let configuration = [u8::from(report_fault), u8::from(fail_shutdown)];
    PluginDefinition::infallible::<CloseFailurePlugin, _>(
        CLOSE_FAILURE_DEFINITION_ID,
        configuration,
        move || CloseFailurePlugin {
            control: Arc::clone(&control),
            report_fault,
            fail_shutdown,
        },
    )
}

fn panic_startup_definition() -> PluginDefinition {
    PluginDefinition::infallible::<PanicStartupPlugin, _>(
        PANIC_STARTUP_DEFINITION_ID,
        b"project-startup-panic-v1",
        PanicStartupPlugin::default,
    )
}

fn running_generation(host: &ProjectHost) -> u64 {
    let ProjectHostSlot::Running(published) = &host.slot else {
        panic!("runtime is not visible in the Host slot");
    };
    published.runtime().generation().get()
}

struct RunningOwnerIdentity {
    generation: u64,
    world: usize,
    command_queue: usize,
    fixed_time: usize,
    task_pools: usize,
    service_session: u64,
    identity_domain: u64,
    snapshot_scene: usize,
    registry_snapshot: ComponentRegistrySnapshot,
    plan_fingerprint: nara_app::PluginPlanFingerprint,
}

fn running_owner_identity(host: &ProjectHost) -> RunningOwnerIdentity {
    let ProjectHostSlot::Running(published) = &host.slot else {
        panic!("runtime is not visible in the Host slot");
    };
    let world = published.runtime().world();
    RunningOwnerIdentity {
        generation: published.runtime().generation().get(),
        world: world as *const World as usize,
        command_queue: world.resource::<GameplayCommandQueue>() as *const GameplayCommandQueue
            as usize,
        fixed_time: world.resource::<nara_app::FixedTime>() as *const nara_app::FixedTime as usize,
        task_pools: world.resource::<TaskPools>() as *const TaskPools as usize,
        service_session: world.resource::<RuntimeServiceSessionId>().0,
        identity_domain: world.resource::<WorldIdentityDomain>().id().get(),
        snapshot_scene: world.resource::<CurrentSceneOwner>().source() as *const SceneDocument
            as usize,
        registry_snapshot: nara_reflect::component_registry(world)
            .expect("the published runtime registry is installed")
            .snapshot()
            .expect("the published runtime registry remains frozen"),
        plan_fingerprint: published._plan.plugin_plan().fingerprint(),
    }
}

fn close_host(host: &mut ProjectHost) {
    let _ = host.stop_running();
    drain_host_cleanup(host);
}

fn drain_host_cleanup(host: &mut ProjectHost) {
    if host.has_cleanup_owner() {
        let _ = drive_host_cleanup_to_completion(host);
    }
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
}

fn drive_host_cleanup_to_completion(host: &mut ProjectHost) -> (bool, DiagnosticReport) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    loop {
        if let CleanupDriveOutcome::Complete {
            failed,
            diagnostics,
            ..
        } = host.drive_cleanup_once()
        {
            return (failed, diagnostics);
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Host cleanup did not finish"
        );
        std::thread::yield_now();
    }
}

fn first_code(report: &DiagnosticReport) -> &str {
    report
        .iter()
        .next()
        .expect("Host rejection publishes one diagnostic")
        .code()
        .as_str()
}

fn report_has_code(report: &DiagnosticReport, code: &str) -> bool {
    report
        .iter()
        .any(|diagnostic| diagnostic.code().as_str() == code)
}

fn identifier_field<'a>(report: &'a DiagnosticReport, code: &str, key: &str) -> Option<&'a str> {
    let field = report
        .iter()
        .find(|diagnostic| diagnostic.code().as_str() == code)?
        .fields()
        .iter()
        .find(|field| field.key().as_str() == key)?;
    match field.value() {
        DiagnosticValueRef::Identifier(value) => Some(value),
        DiagnosticValueRef::Unsigned(_)
        | DiagnosticValueRef::Signed(_)
        | DiagnosticValueRef::Bool(_)
        | DiagnosticValueRef::Display(_)
        | DiagnosticValueRef::ProjectRelative(_)
        | DiagnosticValueRef::Redacted => None,
    }
}

struct TestProject {
    root: PathBuf,
}

impl TestProject {
    fn new(name: &str) -> Self {
        let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "nara_project_host_unit_{}_{}",
            std::process::id(),
            sequence
        ));
        fs::create_dir_all(root.join("assets")).unwrap();
        fs::create_dir_all(root.join("scenes")).unwrap();
        fs::create_dir_all(root.join("prefabs")).unwrap();
        fs::create_dir_all(root.join("schema")).unwrap();
        let authoring_schema_bundle =
            AuthoringSchemaBundle::from_trusted_provider_definitions(built_in_schema_providers())
                .unwrap();
        fs::write(
            root.join("schema/authoring.bundle.json"),
            authoring_schema_bundle.to_json_string().unwrap(),
        )
        .unwrap();
        fs::write(
            root.join("nara.toml"),
            format!(
                r#"schema_version = 1

[project]
source_universe = 'c60a6da4-b6a7-5028-bc53-39111af1a2ba'
native_document_identity = 'project_location'
name = "{name}"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[authoring]
schema_bundle = "schema/authoring.bundle.json"

[runtime]
preset = "local-headless"

[capabilities]
requested = ["runtime-2d"]
"#
            ),
        )
        .unwrap();
        fs::write(
            root.join("scenes/startup.scene.json"),
            SceneDocument::default().to_json_string().unwrap(),
        )
        .unwrap();
        Self { root }
    }

    fn with_image(name: &str) -> Self {
        let project = Self::new(name);
        fs::create_dir_all(project.root.join("assets/textures")).unwrap();

        let image = AssetRef::path("textures/player.png").unwrap();
        let entity = SceneEntityRecord::new(nara_identity::SceneEntityId::new("player").unwrap())
            .with_component(
                ComponentTypeId::new("nara.transform.Transform2d"),
                identity_transform_record(),
            )
            .with_component(
                ComponentTypeId::new("nara.sprite.Sprite"),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, sprite_value(&image)),
            );
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            SceneDocument::from_ordered_roots([entity])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
            project.root.join("assets/textures/player.png"),
        )
        .unwrap();
        let meta = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str("3c7c5be4-fd4e-4b65-b8d4-c671f5982186").unwrap(),
                AssetPath::new("textures/player.png").unwrap(),
                AssetSourceKind::Image,
            ),
            ImageImportSettings::default(),
        );
        fs::write(
            project.root.join("assets/textures/player.png.meta"),
            meta.to_json_string().unwrap(),
        )
        .unwrap();

        project
    }

    #[cfg(feature = "tooling")]
    fn configure_supervised_play(&self, name: &str) {
        fs::write(
            self.root.join("nara.toml"),
            format!(
                r#"schema_version = 1

[project]
source_universe = 'c60a6da4-b6a7-5028-bc53-39111af1a2ba'
native_document_identity = 'project_location'
name = "{name}"
stable_id = "{name}"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[authoring]
schema_bundle = "schema/authoring.bundle.json"

[runtime]
preset = "local-headless"

[capabilities]
requested = ["runtime-2d"]

[profiles.desktop.capabilities]
requested = ["runtime-2d"]

[profiles.desktop.execution]
contract = "desktop-supervised-play/v1"

[profiles.desktop.runtime]
preset = "local-headless"
"#
            ),
        )
        .unwrap();
        fs::write(
            self.root.join("scenes/startup.scene.json"),
            SceneDocument::from_ordered_roots([editable_name_entity("initial")])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(
            self.root.join("Cargo.toml"),
            format!(
                r#"[package]
name = "{name}"
version = "0.1.0"
edition = "2024"

[[bin]]
name = "desktop"
path = "src/main.rs"
required-features = ["desktop"]

[features]
desktop = []
"#
            ),
        )
        .unwrap();
        fs::write(self.root.join("Cargo.lock"), "version = 4\n").unwrap();
    }

    #[cfg(all(feature = "tooling", feature = "runtime-ui"))]
    fn configure_authoring_ui(&self, name: &str) {
        fs::write(
            self.root.join("nara.toml"),
            format!(
                r#"schema_version = 1

[project]
source_universe = 'c60a6da4-b6a7-5028-bc53-39111af1a2ba'
native_document_identity = 'project_location'
name = "{name}"

[paths]
assets = "assets"
scenes = "scenes"
prefabs = "prefabs"

[startup]
default_scene = "startup.scene.json"

[authoring]
schema_bundle = "schema/authoring.bundle.json"

[runtime]
preset = "local-headless"

[capabilities]
requested = ["runtime-2d", "runtime-ui"]

[ui.primary_surface]
logical_width = 960
logical_height = 540
"#,
            ),
        )
        .unwrap();
    }

    #[cfg(all(feature = "tooling", feature = "runtime-ui"))]
    fn write_canonical_image(
        &self,
        path: &str,
        stable_id: &str,
        settings: ImageImportSettings,
        bytes: &[u8],
    ) {
        let asset_path = AssetPath::new(path).unwrap();
        let source = self.root.join("assets").join(path);
        fs::create_dir_all(source.parent().unwrap()).unwrap();
        fs::write(&source, bytes).unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str(stable_id).unwrap(),
                asset_path,
                AssetSourceKind::Image,
            ),
            settings,
        );
        fs::write(
            self.root.join("assets").join(format!("{path}.meta")),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
    }

    #[cfg(all(feature = "tooling", feature = "runtime-ui"))]
    fn write_ui_authoring_documents(
        &self,
        brush_image: Option<&str>,
        additional_scene_image: Option<&str>,
    ) {
        let root = nara_identity::SceneEntityId::new("ui-root").unwrap();
        let image = nara_identity::SceneEntityId::new("image-node").unwrap();
        let prefab = PrefabDocument::new(
            [root.clone()],
            [
                SceneEntityRecord::new(root.clone())
                    .with_children([image.clone()])
                    .with_component(
                        ComponentTypeId::new("nara.ui.UiRoot"),
                        SceneComponentRecord::new(
                            ComponentSchemaVersion::ONE,
                            ComponentValue::map([(
                                "layer",
                                ComponentValue::String("base".to_owned()),
                            )]),
                        ),
                    )
                    .with_component(
                        ComponentTypeId::new("nara.ui.UiNode"),
                        ui_node_record("flow", "percent", 1.0, "percent", 1.0),
                    ),
                SceneEntityRecord::new(image)
                    .with_component(
                        ComponentTypeId::new("nara.ui.UiNode"),
                        ui_node_record("absolute", "px", 128.0, "px", 128.0),
                    )
                    .with_component(
                        ComponentTypeId::new("nara.ui.UiVisual"),
                        ui_visual_record(brush_image),
                    ),
            ],
        );
        fs::write(
            self.root.join("prefabs/ui.prefab.json"),
            prefab.to_json_string().unwrap(),
        )
        .unwrap();

        let mut anchor =
            SceneEntityRecord::new(nara_identity::SceneEntityId::new("ui-anchor").unwrap());
        anchor.prefab = Some(PrefabInstance {
            source: AssetRef::path("ui.prefab.json").unwrap(),
            overrides: None,
        });
        let mut entities = vec![anchor];
        if let Some(path) = additional_scene_image {
            entities.push(
                SceneEntityRecord::new(
                    nara_identity::SceneEntityId::new("additional-sprite").unwrap(),
                )
                .with_component(
                    ComponentTypeId::new("nara.transform.Transform2d"),
                    identity_transform_record(),
                )
                .with_component(
                    ComponentTypeId::new("nara.sprite.Sprite"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        sprite_value(&AssetRef::path(path).unwrap()),
                    ),
                ),
            );
        }
        fs::write(
            self.root.join("scenes/startup.scene.json"),
            SceneDocument::from_ordered_roots(entities)
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
    }

    fn snapshot_and_plan(&self, disable_tilemap: bool) -> (ProjectContentSnapshot, RuntimePlan) {
        let root = self.capability();
        let manifest = root
            .open_file(&RelativePath::new(PROJECT_MANIFEST).unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        drop(manifest);
        let request = project_runtime_plugins(&candidate);
        let request = if disable_tilemap {
            request.disable::<nara_tilemap::TilemapPlugin>()
        } else {
            request
        };
        let plan = resolve_runtime_plan(&candidate, request, built_in_schema_providers()).unwrap();
        let loader = ProjectContentLoader::new(root).unwrap();
        let snapshot = load_runtime_snapshot(&loader, &candidate, &plan);
        (snapshot, plan)
    }

    fn snapshot_plan_and_loader_with_limits(
        &self,
        limits: ProjectContentLimits,
    ) -> (ProjectContentSnapshot, RuntimePlan, ProjectContentLoader) {
        let root = self.capability();
        let manifest = root
            .open_file(&RelativePath::new(PROJECT_MANIFEST).unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        drop(manifest);
        let plan = resolve_runtime_plan(
            &candidate,
            project_runtime_plugins(&candidate),
            built_in_schema_providers(),
        )
        .unwrap();
        let loader = ProjectContentLoader::with_limits(root, limits).unwrap();
        let snapshot = load_runtime_snapshot(&loader, &candidate, &plan);
        (snapshot, plan, loader)
    }

    fn snapshot_and_plan_with_plugin(
        &self,
        definition: PluginDefinition,
    ) -> (ProjectContentSnapshot, RuntimePlan) {
        let root = self.capability();
        let manifest = root
            .open_file(&RelativePath::new(PROJECT_MANIFEST).unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        drop(manifest);
        let request =
            project_runtime_plugins(&candidate).insert_after::<GameplayCommandPlugin>(definition);
        let plan = resolve_runtime_plan(&candidate, request, built_in_schema_providers()).unwrap();
        let loader = ProjectContentLoader::new(root).unwrap();
        let snapshot = load_runtime_snapshot(&loader, &candidate, &plan);
        (snapshot, plan)
    }

    fn snapshot_and_session_plan(&self) -> (ProjectContentSnapshot, RuntimePlan) {
        self.snapshot_and_plan_with_plugin(runtime_service_session_definition())
    }

    fn snapshot_and_fault_plan(
        &self,
        probe: PluginDefinition,
        failure: PluginDefinition,
    ) -> (ProjectContentSnapshot, RuntimePlan) {
        let root = self.capability();
        let manifest = root
            .open_file(&RelativePath::new(PROJECT_MANIFEST).unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        drop(manifest);
        let request = project_runtime_plugins(&candidate)
            .insert_after::<GameplayCommandPlugin>(probe)
            .insert_after::<CloseProbePlugin>(failure);
        let plan = resolve_runtime_plan(&candidate, request, built_in_schema_providers()).unwrap();
        let loader = ProjectContentLoader::new(root).unwrap();
        let snapshot = load_runtime_snapshot(&loader, &candidate, &plan);
        (snapshot, plan)
    }

    fn snapshot_and_schema_plans(
        &self,
    ) -> (
        ProjectContentSnapshot,
        RuntimePlan,
        RuntimePlan,
        RuntimePlan,
    ) {
        let root = self.capability();
        let manifest = root
            .open_file(&RelativePath::new(PROJECT_MANIFEST).unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        drop(manifest);
        let default_plan = resolve_runtime_plan(
            &candidate,
            project_runtime_plugins(&candidate),
            built_in_schema_providers(),
        )
        .unwrap();
        let equivalent_plan = resolve_runtime_plan(
            &candidate,
            project_runtime_plugins(&candidate),
            built_in_schema_providers(),
        )
        .unwrap();
        let reduced_plan = resolve_runtime_plan(
            &candidate,
            project_runtime_plugins(&candidate).disable::<nara_tilemap::TilemapPlugin>(),
            built_in_schema_providers(),
        )
        .unwrap();
        let loader = ProjectContentLoader::new(root).unwrap();
        let snapshot = load_runtime_snapshot(&loader, &candidate, &default_plan);
        (snapshot, default_plan, equivalent_plan, reduced_plan)
    }

    fn capability(&self) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            host_directory(&self.root).unwrap(),
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
        )
        .unwrap()
    }

    fn writable_capability(&self) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            host_directory(&self.root).unwrap(),
            HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
        )
        .unwrap()
    }
}

fn load_runtime_snapshot(
    loader: &ProjectContentLoader,
    candidate: &ProjectSettingsCandidate,
    plan: &RuntimePlan,
) -> ProjectContentSnapshot {
    let project_root = loader.project_root();
    let agreement = prepare_runtime_schema_agreement(&project_root, candidate, plan).unwrap();
    loader
        .load_with_runtime_agreement(candidate, plan, agreement)
        .unwrap()
}

fn sprite_value(image: &AssetRef) -> ComponentValue {
    ComponentValue::map([
        (
            "size",
            ComponentValue::map([
                ("x", ComponentValue::f64(16.0).unwrap()),
                ("y", ComponentValue::f64(16.0).unwrap()),
            ]),
        ),
        (
            "material",
            ComponentValue::map([
                (
                    "image",
                    ComponentValue::map([
                        ("kind", ComponentValue::String("path".to_owned())),
                        (
                            "value",
                            ComponentValue::String(image.as_path().unwrap().as_str().to_owned()),
                        ),
                    ]),
                ),
                (
                    "tint",
                    ComponentValue::map([
                        ("r", ComponentValue::f64(1.0).unwrap()),
                        ("g", ComponentValue::f64(1.0).unwrap()),
                        ("b", ComponentValue::f64(1.0).unwrap()),
                        ("a", ComponentValue::f64(1.0).unwrap()),
                    ]),
                ),
            ]),
        ),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
    ])
}

fn identity_transform_record() -> SceneComponentRecord {
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([
            (
                "translation",
                ComponentValue::map([
                    ("x", ComponentValue::f64(0.0).unwrap()),
                    ("y", ComponentValue::f64(0.0).unwrap()),
                ]),
            ),
            ("rotation", ComponentValue::f64(0.0).unwrap()),
            (
                "scale",
                ComponentValue::map([
                    ("x", ComponentValue::f64(1.0).unwrap()),
                    ("y", ComponentValue::f64(1.0).unwrap()),
                ]),
            ),
        ]),
    )
}

#[cfg(all(feature = "tooling", feature = "runtime-ui"))]
fn ui_node_record(
    position_mode: &str,
    width_kind: &str,
    width_value: f64,
    height_kind: &str,
    height_value: f64,
) -> SceneComponentRecord {
    let dimension = |kind: &str, value: f64| {
        ComponentValue::map([
            ("kind", ComponentValue::String(kind.to_owned())),
            ("value", ComponentValue::f64(value).unwrap()),
        ])
    };
    SceneComponentRecord::new(
        ComponentSchemaVersion::ONE,
        ComponentValue::map([(
            "style",
            ComponentValue::map([
                (
                    "position_mode",
                    ComponentValue::String(position_mode.to_owned()),
                ),
                (
                    "flex_direction",
                    ComponentValue::String("column".to_owned()),
                ),
                ("align_items", ComponentValue::String("stretch".to_owned())),
                (
                    "justify_content",
                    ComponentValue::String("start".to_owned()),
                ),
                ("gap", ComponentValue::f64(0.0).unwrap()),
                (
                    "padding",
                    ComponentValue::map([
                        ("left", ComponentValue::f64(0.0).unwrap()),
                        ("right", ComponentValue::f64(0.0).unwrap()),
                        ("top", ComponentValue::f64(0.0).unwrap()),
                        ("bottom", ComponentValue::f64(0.0).unwrap()),
                    ]),
                ),
                ("flex_grow", ComponentValue::f64(0.0).unwrap()),
                ("flex_shrink", ComponentValue::f64(1.0).unwrap()),
                ("width", dimension(width_kind, width_value)),
                ("height", dimension(height_kind, height_value)),
                ("left", dimension("px", 10.0)),
                ("top", dimension("px", 20.0)),
                ("z_index", ComponentValue::I64(0)),
                ("visible", ComponentValue::Bool(true)),
                ("overflow", ComponentValue::String("visible".to_owned())),
                ("stacking_context", ComponentValue::Bool(false)),
            ]),
        )]),
    )
}

#[cfg(all(feature = "tooling", feature = "runtime-ui"))]
fn ui_visual_record(image: Option<&str>) -> SceneComponentRecord {
    let brush = |red: f64, green: f64, blue: f64, alpha: f64, image: Option<&str>| {
        ComponentValue::map([
            (
                "kind",
                ComponentValue::String(if image.is_some() { "image" } else { "solid" }.to_owned()),
            ),
            ("red", ComponentValue::f64(red).unwrap()),
            ("green", ComponentValue::f64(green).unwrap()),
            ("blue", ComponentValue::f64(blue).unwrap()),
            ("alpha", ComponentValue::f64(alpha).unwrap()),
            ("role", ComponentValue::String(String::new())),
            (
                "image",
                image.map_or(ComponentValue::Null, |path| {
                    ComponentValue::map([
                        ("kind", ComponentValue::String("path".to_owned())),
                        ("value", ComponentValue::String(path.to_owned())),
                    ])
                }),
            ),
        ])
    };
    SceneComponentRecord::new(
        ComponentSchemaVersion::new(2).unwrap(),
        ComponentValue::map([
            ("background", brush(0.2, 0.4, 0.8, 1.0, image)),
            ("border_width", ComponentValue::f64(0.0).unwrap()),
            ("border_brush", brush(0.0, 0.0, 0.0, 0.0, None)),
            (
                "radii",
                ComponentValue::map([
                    ("top_left", ComponentValue::f64(0.0).unwrap()),
                    ("top_right", ComponentValue::f64(0.0).unwrap()),
                    ("bottom_right", ComponentValue::f64(0.0).unwrap()),
                    ("bottom_left", ComponentValue::f64(0.0).unwrap()),
                ]),
            ),
        ]),
    )
}

#[cfg(all(feature = "tooling", feature = "runtime-ui"))]
fn ui_visual_background_image_path<'a>(
    document: &'a PrefabDocument,
    entity: &str,
) -> Option<&'a str> {
    let record = document
        .entities
        .iter()
        .find(|candidate| candidate.id.as_str() == entity)?
        .components
        .get(&ComponentTypeId::new("nara.ui.UiVisual"))?;
    let ComponentValue::Map(visual) = &record.value else {
        return None;
    };
    let ComponentValue::Map(background) = visual.get("background")? else {
        return None;
    };
    let ComponentValue::Map(image) = background.get("image")? else {
        return None;
    };
    (image.get("kind")?.as_str()? == "path")
        .then(|| image.get("value")?.as_str())
        .flatten()
}

#[cfg(all(feature = "tooling", feature = "runtime-ui"))]
fn transform_x_patch(entity: &str, value: f64) -> ScenePatchDocument {
    ScenePatchDocument::new([ScenePatchOperation::SetField {
        entity: nara_identity::SceneEntityId::new(entity).unwrap(),
        component: ComponentTypeId::new("nara.transform.Transform2d"),
        component_version: ComponentSchemaVersion::ONE,
        field: ComponentFieldId::new("translation.x"),
        value: ComponentValue::f64(value).unwrap(),
    }])
}

impl Drop for TestProject {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.root).unwrap();
    }
}

fn portable_trust() -> TrustMode {
    if cfg!(any(windows, target_os = "linux")) {
        TrustMode::Untrusted
    } else {
        TrustMode::TrustedLocal
    }
}

fn host_directory(path: &Path) -> io::Result<File> {
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    }

    #[cfg(unix)]
    {
        File::open(path)
    }
}
