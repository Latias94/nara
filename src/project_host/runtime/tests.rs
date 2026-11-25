mod content_residency;
mod current_scene;
mod processed_input;
mod project_scene_requests;

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

use crate::fs::{
    CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode,
};
use nara_app::tasks::{
    TASK_PLUGIN_ID, TaskDomainKey, TaskHandle, TaskPoolKind, TaskPools, TaskSpawnRequest,
};
use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
    PluginDomainError, PluginError, PluginId, PluginInstantiationError, PluginPrepareError,
    PluginShutdownContext, PluginShutdownObligationId, RuntimeCloseContext,
    RuntimeCloseParticipant, RuntimeCloseParticipantError, RuntimeCloseParticipantId,
    RuntimeCloseProgress, RuntimeConstructionError, RuntimeFault, RuntimeFaultDetail,
    RuntimeFaultKind, RuntimeFaultReporter, StartupStage,
};
#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
use nara_app::{RuntimeControl, RuntimeControlRequestResult};
use nara_asset::{
    AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, Assets, StableAssetId,
};
use nara_core::diagnostic::DiagnosticValueRef;
use nara_ecs::identity::{EntityLookup, WorldIdentityDomain};
use nara_ecs::{Res, Resource, bevy_ecs::error::BevyError, schedule::IntoScheduleConfigs};
use nara_reflect::{
    AuthoringSchemaBundle, ComponentRegistrySnapshot, ComponentSchemaVersion, ComponentTypeId,
    ComponentValue,
};
use nara_render::image::{ImageAsset, ImageImportSettings};
use nara_scene::{CompiledSceneDocument, SceneComponentRecord, SceneDocument, SceneEntityRecord};

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
        activation.source().entities().len(),
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

    let Err(error) = host.begin_start(first_snapshot, second_plan) else {
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

    let Err(error) = host.begin_start(snapshot, plan) else {
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

    let Err(error) = host.begin_start(snapshot.clone(), reduced_plan) else {
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
    let Err(error) = host.begin_start(snapshot, equivalent_plan) else {
        panic!("equal but independently constructed schema authority unexpectedly succeeded");
    };
    assert_eq!(
        first_code(&error.diagnostics),
        "project.run.schema-mismatch"
    );
}

#[test]
fn sequential_starts_share_immutable_authority_and_rebuild_runtime_sessions() {
    let project = TestProject::new("publication-cut");
    let (snapshot, plan) = project.snapshot_and_session_plan();
    let shared_scene = snapshot.compiled_scene() as *const CompiledSceneDocument as usize;
    let shared_registry = plan.schema_validation().snapshot().clone();
    let stable_plan = plan.plugin_plan().fingerprint();
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());

    let mut first_attempt = host.begin_start(snapshot.clone(), plan.clone()).unwrap();
    assert!(host.start_claim.is_active(first_attempt.epoch));
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
    let diagnostics = host.complete_start(&mut first_attempt).unwrap();
    assert!(!diagnostics.has_errors());
    let first = running_owner_identity(&host);
    close_host(&mut host);

    let mut second_attempt = host.begin_start(snapshot, plan).unwrap();
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
    let source_pointer = snapshot.compiled_scene() as *const CompiledSceneDocument;
    let source_entity_ids = snapshot
        .compiled_scene()
        .entities()
        .iter()
        .map(|entity| entity.id().clone())
        .collect::<Vec<_>>();
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());

    let mut attempt = host.begin_start(snapshot, plan).unwrap();
    host.complete_start(&mut attempt).unwrap();

    let ProjectHostSlot::Running(published) = &host.slot else {
        panic!("runtime is not visible in the Host slot");
    };
    let world = published.runtime().world();
    let activation = world.resource::<CurrentSceneOwner>();
    assert_eq!(
        activation.source() as *const CompiledSceneDocument,
        source_pointer
    );
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

    let mut attempt = host.begin_start(snapshot, plan).unwrap();
    host.complete_start(&mut attempt).unwrap();

    assert_eq!(observations.load(Ordering::SeqCst), 1);
    close_host(&mut host);
}

#[test]
fn classified_product_startup_failure_preserves_detail_and_prevents_publication() {
    let project = TestProject::new("classified-startup-failure");
    let (snapshot, plan) =
        project.snapshot_and_plan_with_plugin(classified_startup_failure_definition());
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan).unwrap();

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
fn project_host_publishes_snapshot_images_without_copying_the_retained_pixels() {
    let project = TestProject::with_image("image-publication");
    let (snapshot, plan) = project.snapshot_and_plan(false);
    assert_eq!(snapshot.images().len(), 1);
    let snapshot_pixels = snapshot.images()[0].image().pixels().as_ptr();
    let snapshot_source = snapshot.images()[0].source().clone();
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());

    let mut attempt = host.begin_start(snapshot, plan).unwrap();
    host.complete_start(&mut attempt).unwrap();

    {
        let ProjectHostSlot::Running(published) = &host.slot else {
            panic!("runtime is not visible in the Host slot");
        };
        let runtime_images = published.runtime().world().resource::<Assets<ImageAsset>>();
        let runtime_images = runtime_images.iter().collect::<Vec<_>>();
        assert_eq!(runtime_images.len(), 1);
        let runtime_image = runtime_images[0].1;
        assert_eq!(runtime_image.source(), Some(&snapshot_source));
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
    let mut attempt = host.begin_start(snapshot, plan).unwrap();

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
    let shared_scene = snapshot.compiled_scene() as *const CompiledSceneDocument as usize;
    let shared_registry = plan.schema_validation().snapshot().clone();
    let stable_plan = plan.plugin_plan().fingerprint();
    let mut first_host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut second_host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut first_attempt = first_host
        .begin_start(snapshot.clone(), plan.clone())
        .unwrap();
    let mut second_attempt = second_host.begin_start(snapshot, plan).unwrap();
    first_host.complete_start(&mut first_attempt).unwrap();
    second_host.complete_start(&mut second_attempt).unwrap();

    let first = running_owner_identity(&first_host);
    let second = running_owner_identity(&second_host);

    assert_ne!(first.generation, second.generation);
    assert_ne!(first.world, second.world);
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
        .begin_start(snapshot.clone(), plan.clone())
        .unwrap();
    let mut second_attempt = second_host.begin_start(snapshot, plan).unwrap();

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
    let attempt = host.begin_start(snapshot.clone(), plan.clone()).unwrap();
    let cancelled_epoch = attempt.epoch;

    drop(attempt);

    assert!(!host.start_claim.is_active(cancelled_epoch));
    let mut replacement = host.begin_start(snapshot, plan).unwrap();
    assert_ne!(replacement.epoch, cancelled_epoch);
    host.complete_start(&mut replacement).unwrap();
    close_host(&mut host);
}

#[test]
fn product_tick_uses_one_exact_step() {
    let project = TestProject::new("exact-step");
    let (snapshot, plan) = project.snapshot_and_plan(false);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan).unwrap();
    host.complete_start(&mut attempt).unwrap();
    let ProjectHostSlot::Running(published) = &mut host.slot else {
        panic!("runtime was not published");
    };
    let tick_before = published
        .runtime()
        .world()
        .resource::<nara_app::FixedTime>()
        .tick();

    assert_eq!(host.prepare_exact_tick_run().unwrap(), None);
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
    let mut attempt = host.begin_start(snapshot, plan).unwrap();

    let unwound = catch_unwind(AssertUnwindSafe(|| {
        let _prepared = host.prepare_start(&mut attempt).unwrap();
        panic!("injected pre-publication unwind");
    }));

    assert!(unwound.is_err());
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
    assert!(!host.start_claim.any_active());
}

#[test]
fn reported_execution_fault_is_not_reemitted_by_runtime_retirement() {
    let drives: [fn(&mut ProjectHost) -> Result<(), HostFault>; 4] = [
        |host| host.drive_one_frame(Duration::ZERO).map(|_| ()),
        |host| host.prepare_exact_tick_run().map(|_| ()),
        |host| host.drive_one_fixed_tick().map(|_| ()),
        |host| host.drive_running_runtime(Duration::ZERO),
    ];
    for drive in drives {
        let project = TestProject::new("reported-execution-fault-retirement");
        let (snapshot, plan) = project.snapshot_and_plan(false);
        let mut host = ProjectHost::new(RuntimeClosePolicy::default());
        let mut attempt = host.begin_start(snapshot.clone(), plan.clone()).unwrap();
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
        let mut attempt = host.begin_start(snapshot, plan).unwrap();
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
    let mut attempt = host.begin_start(snapshot, plan).unwrap();
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
    let mut attempt = host.begin_start(snapshot, plan).unwrap();
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
    let mut attempt = host.begin_start(snapshot, plan).unwrap();
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
            let mut attempt = host.begin_start(snapshot, plan).unwrap();
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
    let mut attempt = host.begin_start(snapshot, plan).unwrap();
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
    let mut attempt = host.begin_start(snapshot, plan).unwrap();
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
fn startup_panic_releases_the_host_claim() {
    let project = TestProject::new("startup-panic-claim");
    let (snapshot, plan) = project.snapshot_and_fault_plan(
        close_probe_definition(
            &Arc::new(AtomicUsize::new(0)),
            &Arc::new(AtomicUsize::new(0)),
        ),
        panic_startup_definition(),
    );
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan).unwrap();

    let unwound = catch_unwind(AssertUnwindSafe(|| {
        let _ = host.complete_start(&mut attempt);
    }));

    assert!(unwound.is_err());
    assert!(!host.start_claim.any_active());
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
}

#[test]
fn unavailable_execution_profile_refuses_project_publication_and_retires_original_owners() {
    struct ParallelSelection;

    impl Plugin for ParallelSelection {
        fn declaration() -> &'static PluginDeclaration {
            const DECLARATION: PluginDeclaration = PluginDeclaration::new(
                PluginId::new("nara.test.project-parallel-selection"),
                PluginCategory::Runtime,
            );
            &DECLARATION
        }

        fn build(&self, app: &mut App) -> Result<(), PluginError> {
            app.set_execution_profile(nara_app::ExecutionProfile::Parallel)?;
            Ok(())
        }
    }

    let project = TestProject::new("unavailable-execution-profile");
    let report = assert_start_phase_failure(
        &project,
        PluginDefinition::infallible::<ParallelSelection, _>(
            PluginDefinitionId::new("nara.test.project-parallel-selection", 1),
            b"project-parallel-selection-v1",
            || ParallelSelection,
        ),
        "project.run.execution-profile-unavailable",
        1,
    );
    let code = "project.run.execution-profile-unavailable";
    assert_eq!(
        identifier_field(&report, code, "reason"),
        Some("execution-profile")
    );
    assert_eq!(
        identifier_field(&report, code, "execution-profile"),
        Some("parallel")
    );
    assert!(matches!(
        identifier_field(&report, code, "execution-profile-reason"),
        Some("parallel-not-built" | "compute-pool-unavailable")
    ));
    assert_eq!(identifier_field(&report, code, "plugin"), None);
    assert_eq!(identifier_field(&report, code, "hook"), None);
}

#[test]
fn plugin_reported_execution_failure_retains_its_build_and_shutdown_invocation_identity() {
    struct ReportedFailure {
        fail_build: bool,
    }

    const PLUGIN: PluginId = PluginId::new("nara.test.reported-execution-failure");
    impl Plugin for ReportedFailure {
        fn declaration() -> &'static PluginDeclaration {
            const DECLARATION: PluginDeclaration =
                PluginDeclaration::new(PLUGIN, PluginCategory::Runtime);
            &DECLARATION
        }

        fn build(&self, _app: &mut App) -> Result<(), PluginError> {
            if self.fail_build {
                Err(nara_app::ExecutionProfileError::ComputePoolUnavailable.into())
            } else {
                Ok(())
            }
        }

        fn shutdown(&self, _context: &mut PluginShutdownContext<'_>) -> Result<(), PluginError> {
            if self.fail_build {
                Ok(())
            } else {
                Err(nara_app::ExecutionProfileError::ComputePoolUnavailable.into())
            }
        }
    }

    let project = TestProject::new("reported-execution-failure");
    let build = assert_start_phase_failure(
        &project,
        PluginDefinition::infallible::<ReportedFailure, _>(
            PluginDefinitionId::new("nara.test.reported-execution-failure", 1),
            b"reported-execution-failure-v1",
            || ReportedFailure { fail_build: true },
        ),
        "project.run.plugin-hook-failed",
        1,
    );
    let mut app = App::new();
    app.add_plugins(ReportedFailure { fail_build: false })
        .unwrap();
    let retired = nara_app::RuntimeInstance::start(app)
        .unwrap()
        .finish()
        .unwrap_err();
    assert_eq!(
        retired.retirement_state(),
        nara_app::RuntimeCandidateRetirementState::Retired
    );
    let mut shutdown = DiagnosticReport::default();
    append_plugin_shutdown_diagnostics(&mut shutdown, retired.plugin_failure_report());

    for (report, code, hook) in [
        (&build, "project.run.plugin-hook-failed", "build"),
        (&shutdown, "project.run.cleanup-plugin-shutdown", "shutdown"),
    ] {
        assert_eq!(
            identifier_field(report, code, "plugin"),
            Some(PLUGIN.as_str())
        );
        assert_eq!(identifier_field(report, code, "hook"), Some(hook));
        assert_eq!(
            identifier_field(report, code, "execution-profile"),
            Some("parallel")
        );
        assert_eq!(
            identifier_field(report, code, "execution-profile-reason"),
            Some("compute-pool-unavailable")
        );
        assert!(
            !report.iter().any(|diagnostic| diagnostic.code().as_str()
                == "project.run.execution-profile-unavailable")
        );
    }
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
    let mut attempt = host.begin_start(snapshot, plan).unwrap();
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
    let mut attempt = host.begin_start(snapshot, plan).unwrap();

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
    let mut attempt = host.begin_start(snapshot, plan).unwrap();
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
    let mut attempt = host.begin_start(snapshot, plan).unwrap();
    let epoch = attempt.epoch;
    host.complete_start(&mut attempt).unwrap();
    let first_generation = running_generation(&host);
    assert_eq!(host.prepare_exact_tick_run().unwrap(), None);
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

    let replacement_error =
        match host.begin_start(replacement_snapshot.clone(), replacement_plan.clone()) {
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
        .begin_start(replacement_snapshot, replacement_plan)
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
) -> DiagnosticReport {
    let builds = Arc::new(AtomicUsize::new(0));
    let closes = Arc::new(AtomicUsize::new(0));
    let (snapshot, plan) =
        project.snapshot_and_fault_plan(close_probe_definition(&builds, &closes), failure);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan).unwrap();

    let error = host.complete_start(&mut attempt).unwrap_err();
    let actual_code = first_code(&error.diagnostics).to_owned();
    let claim_remained_active = host.start_claim.any_active();
    let runtime_became_visible = matches!(host.slot, ProjectHostSlot::Running(_));
    let actual_builds = builds.load(Ordering::SeqCst);
    drain_host_cleanup(&mut host);
    let actual_closes = closes.load(Ordering::SeqCst);
    let redundant_cleanup = host.drive_cleanup_once();
    let closes_after_redundant_cleanup = closes.load(Ordering::SeqCst);

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
    error.diagnostics.clone()
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
        fixed_time: world.resource::<nara_app::FixedTime>() as *const nara_app::FixedTime as usize,
        task_pools: world.resource::<TaskPools>() as *const TaskPools as usize,
        service_session: world.resource::<RuntimeServiceSessionId>().0,
        identity_domain: world.resource::<WorldIdentityDomain>().id().get(),
        snapshot_scene: world.resource::<CurrentSceneOwner>().source()
            as *const CompiledSceneDocument as usize,
        registry_snapshot: nara_reflect::component_registry(world)
            .expect("the published runtime registry is installed")
            .clone(),
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
state = "ready"
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
        let entity =
            SceneEntityRecord::new(nara_ecs::identity::SceneEntityId::new("player").unwrap())
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

    fn snapshot_and_plan(&self, disable_tilemap: bool) -> (ProjectContentSnapshot, RuntimePlan) {
        let root = self.capability();
        let manifest = root
            .open_file(&RelativePath::new(PROJECT_MANIFEST).unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        drop(manifest);
        let request = project_runtime_plugins(&candidate);
        let request = if disable_tilemap {
            request.disable::<nara_render::tilemap::TilemapPlugin>()
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
            project_runtime_plugins(&candidate).insert_after::<nara_asset::AssetPlugin>(definition);
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
            .insert_after::<nara_asset::AssetPlugin>(probe)
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
            project_runtime_plugins(&candidate).disable::<nara_render::tilemap::TilemapPlugin>(),
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
