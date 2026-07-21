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
    time::Duration,
};

#[cfg(windows)]
use std::fs::OpenOptions;

use nara_app::{
    App, Plugin, PluginCategory, PluginDeclaration, PluginDefinition, PluginDefinitionId,
    PluginError, PluginId, PluginPrepareFailure, PluginShutdownContext, PluginShutdownObligationId,
    RuntimeCloseContext, RuntimeCloseParticipant, RuntimeCloseParticipantError,
    RuntimeCloseParticipantId, RuntimeCloseProgress, RuntimeFault, RuntimeFaultKind,
    RuntimeFaultReporter, StartupStage, drive_runtime_quarantine, runtime_quarantine_status,
};
#[cfg(all(feature = "desktop-winit", feature = "render-wgpu"))]
use nara_app::{RuntimeControl, RuntimeControlRequestResult};
use nara_asset::{AssetMeta, AssetPath, AssetRef, StableAssetId};
use nara_diagnostic::DiagnosticValueRef;
use nara_ecs::{Resource, error::BevyError};
use nara_fs::{
    CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode,
};
use nara_gameplay::{GameplayCommandPlugin, GameplayCommandQueue};
use nara_image::PreparedImageResource;
use nara_reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue};
use nara_render::PreparedRenderResources;
use nara_scene::{SceneComponentRecord, SceneDocument, SceneEntityRecord};
use nara_tasks::{
    TASK_PLUGIN_ID, TaskDomainKey, TaskHandle, TaskPoolKind, TaskPools, TaskSpawnRequest,
};

use crate::project_content::ProjectContentLoader;
use crate::project_host::{
    built_in_schema_providers, ingest_project_manifest, project_runtime_plugins,
    resolve_runtime_plan,
};

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
const REGISTRY_FAILURE_PLUGIN_ID: PluginId = PluginId::new("nara.test.project-registry-failure");
const REGISTRY_FAILURE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("nara.test.project-registry-failure", 1);
const REGISTRY_FAILURE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(REGISTRY_FAILURE_PLUGIN_ID, PluginCategory::Runtime)
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
struct RegistryFailurePlugin;

impl Plugin for RegistryFailurePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &REGISTRY_FAILURE_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Ok(())
    }

    fn finish(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?.remove_resource::<ComponentRegistry>();
        Ok(())
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
fn project_host_rejects_a_different_schema_plan_from_the_same_lineage() {
    let project = TestProject::new("schema-lineage");
    let (snapshot, default_plan, reduced_plan) = project.snapshot_and_schema_plans();
    assert_eq!(snapshot.lineage(), reduced_plan.lineage());
    assert_ne!(
        default_plan.schema_validation().fingerprint(),
        reduced_plan.schema_validation().fingerprint()
    );
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());

    let Err(error) = host.begin_start(snapshot, reduced_plan, Vec::new()) else {
        panic!("cross-schema start unexpectedly succeeded");
    };

    assert_eq!(
        first_code(&error.diagnostics),
        "project.run.schema-mismatch"
    );
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
}

#[test]
fn publication_is_one_visibility_cut_and_repeated_starts_use_fresh_generations() {
    let project = TestProject::new("publication-cut");
    let (snapshot, plan) = project.snapshot_and_plan(false);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());

    let mut first_attempt = host
        .begin_start(snapshot.clone(), plan.clone(), Vec::new())
        .unwrap();
    assert!(host.start_claim.is_active(first_attempt.epoch));
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
    let diagnostics = host.complete_start(&mut first_attempt).unwrap();
    assert!(!diagnostics.has_errors());
    let first_generation = running_generation(&host);
    close_host(&mut host);

    let mut second_attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    assert!(host.start_claim.is_active(second_attempt.epoch));
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
    let diagnostics = host.complete_start(&mut second_attempt).unwrap();
    assert!(!diagnostics.has_errors());
    let second_generation = running_generation(&host);

    assert_ne!(first_generation, second_generation);
    close_host(&mut host);
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

    let ProjectHostSlot::Running(published) = &mut host.slot else {
        panic!("runtime is not visible in the Host slot");
    };
    published.runtime_mut().drive(Duration::ZERO).unwrap();
    let prepared = published
        .runtime()
        .world()
        .resource::<PreparedRenderResources<PreparedImageResource>>();
    assert_eq!(
        prepared.len(),
        1,
        "the production Host publication must reach render preparation",
    );

    close_host(&mut host);
}

#[test]
fn concurrent_starts_share_immutable_content_but_isolate_every_runtime_owner() {
    let project = TestProject::new("runtime-owner-isolation");
    let (snapshot, plan) = project.snapshot_and_plan(false);
    let shared_scene = snapshot.expanded_startup_scene() as *const SceneDocument as usize;
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
    assert_eq!(first.snapshot_scene, shared_scene);
    assert_eq!(second.snapshot_scene, shared_scene);
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
fn publication_reservation_unwind_restores_the_empty_host_slot() {
    let project = TestProject::new("publication-reservation-unwind");
    let (snapshot, plan) = project.snapshot_and_plan(false);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());

    let unwound = catch_unwind(AssertUnwindSafe(|| {
        let _reservation = HostPublicationReservation::new(&mut host, 1, snapshot, plan);
        panic!("injected pre-publication unwind");
    }));

    assert!(unwound.is_err());
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
}

#[test]
fn close_time_runtime_fault_cannot_complete_the_product_run() {
    let project = TestProject::new("close-time-runtime-fault");
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

    let failure = host.stop_running().unwrap_err();

    assert!(report_has_code(
        &failure.diagnostics,
        "project.run.runtime-faulted"
    ));
    let had_cleanup = host.has_cleanup_owner();
    let mut cleanup_failed = false;
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while host.has_cleanup_owner() && std::time::Instant::now() < deadline {
        match host.drive_cleanup_once() {
            CleanupDriveOutcome::Complete { failed, .. } => cleanup_failed = failed,
            CleanupDriveOutcome::Retiring | CleanupDriveOutcome::RetirementIncomplete => {
                std::thread::yield_now();
            }
        }
    }
    if had_cleanup {
        assert!(cleanup_failed);
    }
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
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
    assert!(matches!(
        host.drive_cleanup_once(),
        CleanupDriveOutcome::Complete { failed: true, .. }
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
    let intent = DesktopRunIntent::new()
        .insert_after::<GameplayCommandPlugin>(close_probe_definition(&builds, &closes))
        .insert_after::<CloseProbePlugin>(startup_failure_definition());
    let mut run = DesktopRun::new(project.capability(), intent);

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
        "project.run.plugin-prepare-failed",
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
        PluginDefinition::infallible::<RegistryFailurePlugin, _>(
            REGISTRY_FAILURE_DEFINITION_ID,
            b"project-registry-failure-v1",
            RegistryFailurePlugin::default,
        ),
        "project.run.registry-missing",
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

    let preparation = runtime_construction_failure_report(&RuntimeConstructionError::Plugin(
        PluginInstantiationError::Prepare(PluginPrepareError::Failed {
            plugin: PREPARE_FAILURE_PLUGIN_ID,
            code: "nara.test.prepare-rejected",
        }),
    ));
    assert_eq!(
        identifier_field(&preparation, "project.run.plugin-prepare-failed", "plugin"),
        Some(PREPARE_FAILURE_PLUGIN_ID.as_str())
    );
    assert_eq!(
        identifier_field(
            &preparation,
            "project.run.plugin-prepare-failed",
            "failure-code"
        ),
        Some("nara.test.prepare-rejected")
    );

    let hook = runtime_construction_failure_report(&RuntimeConstructionError::Plugin(
        PluginInstantiationError::Plugin(PluginError::HookPanicked {
            plugin: FINISH_FAILURE_PLUGIN_ID,
            hook: nara_app::PluginHook::Finish,
        }),
    ));
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
    let mut host = ProjectHost::new(RuntimeClosePolicy::new(Duration::ZERO));
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();
    let epoch = attempt.epoch;
    host.complete_start(&mut attempt).unwrap();
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

    control.release();
    drain_host_cleanup(&mut host);

    assert_eq!(control.instances.load(Ordering::SeqCst), 1);
    assert!(!host.start_claim.any_active());
}

fn assert_start_phase_failure(
    project: &TestProject,
    failure: PluginDefinition,
    expected_code: &str,
    expected_probe_builds: usize,
) {
    let builds = Arc::new(AtomicUsize::new(0));
    let closes = Arc::new(AtomicUsize::new(0));
    let (snapshot, plan) =
        project.snapshot_and_fault_plan(close_probe_definition(&builds, &closes), failure);
    let mut host = ProjectHost::new(RuntimeClosePolicy::default());
    let mut attempt = host.begin_start(snapshot, plan, Vec::new()).unwrap();

    let error = host.complete_start(&mut attempt).unwrap_err();

    assert_eq!(first_code(&error.diagnostics), expected_code);
    assert!(!host.start_claim.any_active());
    assert!(!matches!(host.slot, ProjectHostSlot::Running(_)));
    assert_eq!(builds.load(Ordering::SeqCst), expected_probe_builds);
    drain_host_cleanup(&mut host);
    assert_eq!(closes.load(Ordering::SeqCst), expected_probe_builds);
    assert!(matches!(
        host.drive_cleanup_once(),
        CleanupDriveOutcome::Complete { failed: false, .. }
    ));
    assert_eq!(closes.load(Ordering::SeqCst), expected_probe_builds);
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
            Err(PluginPrepareFailure::new(
                "nara.test.project-prepare-failure",
            ))
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

fn startup_failure_definition() -> PluginDefinition {
    PluginDefinition::infallible::<StartupFailurePlugin, _>(
        STARTUP_FAILURE_DEFINITION_ID,
        b"project-startup-failure-v1",
        StartupFailurePlugin::default,
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
    snapshot_scene: usize,
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
        snapshot_scene: published._snapshot.expanded_startup_scene() as *const SceneDocument
            as usize,
        plan_fingerprint: published._plan.plugin_plan().fingerprint(),
    }
}

fn close_host(host: &mut ProjectHost) {
    let _ = host.stop_running();
    drain_host_cleanup(host);
}

fn drain_host_cleanup(host: &mut ProjectHost) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while host.has_cleanup_owner() {
        assert!(
            std::time::Instant::now() < deadline,
            "Host cleanup did not finish"
        );
        host.drive_cleanup_once();
        std::thread::yield_now();
    }
    assert!(matches!(host.slot, ProjectHostSlot::Empty));
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
        fs::write(
            root.join("nara.toml"),
            format!(
                r#"schema_version = 1

[project]
name = "{name}"

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
                ComponentTypeId::new("nara.sprite.Sprite"),
                SceneComponentRecord::new(ComponentSchemaVersion::ONE, sprite_value(&image)),
            );
        fs::write(
            project.root.join("scenes/startup.scene.json"),
            SceneDocument::new([entity]).to_json_string().unwrap(),
        )
        .unwrap();
        fs::copy(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/images/valid-rgba-1x1.png"),
            project.root.join("assets/textures/player.png"),
        )
        .unwrap();
        let meta = AssetMeta::new(
            StableAssetId::parse_str("3c7c5be4-fd4e-4b65-b8d4-c671f5982186").unwrap(),
            AssetPath::new("textures/player.png").unwrap(),
            AssetSourceKind::Image,
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
            request.disable::<nara_tilemap::TilemapPlugin>()
        } else {
            request
        };
        let plan = resolve_runtime_plan(&candidate, request, built_in_schema_providers()).unwrap();
        let loader = ProjectContentLoader::new(root).unwrap();
        let snapshot = loader.load(&candidate, &plan).unwrap();
        (snapshot, plan)
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
        let snapshot = loader.load(&candidate, &plan).unwrap();
        (snapshot, plan)
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
        let snapshot = loader.load(&candidate, &plan).unwrap();
        (snapshot, plan)
    }

    fn snapshot_and_schema_plans(&self) -> (ProjectContentSnapshot, RuntimePlan, RuntimePlan) {
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
        let reduced_plan = resolve_runtime_plan(
            &candidate,
            project_runtime_plugins(&candidate).disable::<nara_tilemap::TilemapPlugin>(),
            built_in_schema_providers(),
        )
        .unwrap();
        let loader = ProjectContentLoader::new(root).unwrap();
        let snapshot = loader.load(&candidate, &default_plan).unwrap();
        (snapshot, default_plan, reduced_plan)
    }

    fn capability(&self) -> DirectoryCapability {
        DirectoryCapability::from_host_handle(
            host_directory(&self.root).unwrap(),
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, portable_trust()),
        )
        .unwrap()
    }
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
