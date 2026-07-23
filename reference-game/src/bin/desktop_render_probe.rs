use std::{
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use nara::ecs as bevy_ecs;
use nara::{
    app::{
        AppExit, AppExitRequests, PluginCategory, PluginDeclaration, PluginDefinition,
        PluginDefinitionId, PluginError, PluginId, RuntimeGeneration,
    },
    ecs::{Entity, Query, With, schedule::IntoScheduleConfigs, system::SystemParam},
    gameplay::{GameplayCommandQueue, GameplayCommandSet},
    image::PreparedImageResource,
    prelude::{App, CoreStage, FixedTime, Plugin, Res, ResMut, Resource},
    project_host::{DesktopRun, DesktopRunOutcome},
    render::{PreparedRenderResources, RenderFrame, RenderFrameState},
    render_wgpu::WgpuRenderBackend,
    sprite_render::SpriteBatches,
    window::{
        Window, WindowId,
        backend::{BackendWindowHandles, WindowTargetSnapshot},
    },
};
use nara_reference_game::{
    MovementDirection, REFERENCE_DESKTOP_PLUGIN_ID, ReferenceDesktopPlugin, WaveRunGeneration,
    WaveSnapshot, movement_command, retry_command, wave_desktop_intent,
};

#[path = "support/startup_marker.rs"]
mod startup_marker;
mod support;

use startup_marker::StartupMarker;
use support::project_root::open_project_root;

const PRODUCT_RENDER_PROBE_PLUGIN_ID: PluginId =
    PluginId::new("reference-game.product-render-probe");
const PRODUCT_RENDER_PROBE_DEFINITION_ID: PluginDefinitionId =
    PluginDefinitionId::new("reference-game.product-render-probe", 1);
const PRODUCT_RENDER_EVIDENCE_TIMEOUT: Duration = Duration::from_secs(30);
const PRODUCT_RENDER_PROBE_REQUIREMENTS: &[PluginId] = &[REFERENCE_DESKTOP_PLUGIN_ID];
const PRODUCT_RENDER_PROBE_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(PRODUCT_RENDER_PROBE_PLUGIN_ID, PluginCategory::Tooling)
        .requires_plugins(PRODUCT_RENDER_PROBE_REQUIREMENTS);

fn main() -> ExitCode {
    let root = match open_project_root() {
        Ok(root) => root,
        Err(_) => return fail("desktop_render_probe.root_failed"),
    };
    let marker = match StartupMarker::from_environment("desktop_first_playable_present") {
        Ok(marker) => Arc::new(marker),
        Err(error) => return fail(error.code()),
    };
    let evidence = Arc::new(ProductRenderEvidence::default());
    let plugin_evidence = Arc::clone(&evidence);
    let plugin_marker = Arc::clone(&marker);
    let probe = PluginDefinition::infallible::<ProductRenderProbePlugin, _>(
        PRODUCT_RENDER_PROBE_DEFINITION_ID,
        b"reference-game-product-render-probe-v1",
        move || ProductRenderProbePlugin {
            evidence: Arc::clone(&plugin_evidence),
            marker: Arc::clone(&plugin_marker),
        },
    );
    let intent = wave_desktop_intent().insert_after::<ReferenceDesktopPlugin>(probe);
    let mut run = DesktopRun::new(root, intent);
    let mut cleanup_deadline = None;

    let report = loop {
        let report = run.execute();
        if report.outcome() != DesktopRunOutcome::CleanupIncomplete {
            break report;
        }
        let now = Instant::now();
        let deadline = *cleanup_deadline
            .get_or_insert_with(|| now.checked_add(Duration::from_secs(20)).unwrap_or(now));
        if now >= deadline {
            return fail("desktop_render_probe.cleanup_timeout");
        }
        std::thread::park_timeout(Duration::from_millis(1));
    };
    if report.outcome() != DesktopRunOutcome::Completed(AppExit::Requested)
        || report.diagnostics().has_errors()
        || evidence.timed_out.load(Ordering::SeqCst)
        || !evidence.prepared_image.load(Ordering::SeqCst)
        || !evidence.textured_batch.load(Ordering::SeqCst)
        || !evidence.submitted_frame.load(Ordering::SeqCst)
        || !evidence.retry_continuity.load(Ordering::SeqCst)
    {
        return fail("desktop_render_probe.product_path_failed");
    }
    if let Err(error) = marker.verify_success() {
        return fail(error.code());
    }

    println!("desktop_render_probe: ok");
    ExitCode::SUCCESS
}

fn fail(code: &'static str) -> ExitCode {
    eprintln!("{code}");
    ExitCode::FAILURE
}

#[derive(Debug, Default)]
struct ProductRenderEvidence {
    prepared_image: AtomicBool,
    textured_batch: AtomicBool,
    submitted_frame: AtomicBool,
    retry_continuity: AtomicBool,
    timed_out: AtomicBool,
}

#[derive(Debug)]
struct ProductRenderProbePlugin {
    evidence: Arc<ProductRenderEvidence>,
    marker: Arc<StartupMarker>,
}

impl Plugin for ProductRenderProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &PRODUCT_RENDER_PROBE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(ProductRenderProbe {
            evidence: Arc::clone(&self.evidence),
            marker: Arc::clone(&self.marker),
            deadline: Instant::now() + PRODUCT_RENDER_EVIDENCE_TIMEOUT,
            phase: ProductProbePhase::InitialFrame,
            baseline: None,
            reset_frame_floor: None,
        })?
        .insert_resource(ProductServiceSentinel {
            identity: Arc::new(()),
            value: 0x004e_4152_4155_3133,
        })?
        .add_systems(
            CoreStage::FixedUpdate,
            observe_product_render.in_set(GameplayCommandSet::Capture),
        )?
        .add_systems(CoreStage::Cleanup, observe_startup_present)?;
        Ok(())
    }
}

#[derive(Debug, Resource)]
struct ProductRenderProbe {
    evidence: Arc<ProductRenderEvidence>,
    marker: Arc<StartupMarker>,
    deadline: Instant,
    phase: ProductProbePhase,
    baseline: Option<ProductContinuity>,
    reset_frame_floor: Option<u64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProductProbePhase {
    InitialFrame,
    Terminal,
    ResetFrame,
}

#[derive(Debug, Resource)]
struct ProductServiceSentinel {
    identity: Arc<()>,
    value: u64,
}

#[derive(Debug, Clone)]
struct ProductContinuity {
    runtime_generation: u64,
    window_entity: Entity,
    handles: BackendWindowHandles,
    target: WindowTargetSnapshot,
    backend_instance_id: u64,
    device_epoch: u64,
    service_identity: Arc<()>,
    service_value: u64,
}

impl ProductContinuity {
    fn matches(&self, current: &Self) -> bool {
        self.runtime_generation == current.runtime_generation
            && self.window_entity == current.window_entity
            && self.handles.shares_authority_with(&current.handles)
            && self.target == current.target
            && self.backend_instance_id == current.backend_instance_id
            && self.device_epoch != 0
            && self.device_epoch == current.device_epoch
            && Arc::ptr_eq(&self.service_identity, &current.service_identity)
            && self.service_value == current.service_value
    }
}

#[derive(SystemParam)]
struct ProductRenderInputs<'w, 's> {
    prepared: Res<'w, PreparedRenderResources<PreparedImageResource>>,
    batches: Res<'w, SpriteBatches>,
    frame: Res<'w, RenderFrame>,
    backend: Res<'w, WgpuRenderBackend>,
    runtime_generation: Res<'w, RuntimeGeneration>,
    wave_generation: Res<'w, WaveRunGeneration>,
    snapshot: Res<'w, WaveSnapshot>,
    fixed_time: Res<'w, FixedTime>,
    handles: Res<'w, BackendWindowHandles>,
    service: Res<'w, ProductServiceSentinel>,
    windows: Query<'w, 's, Entity, With<Window>>,
}

#[derive(SystemParam)]
struct StartupPresentInputs<'w> {
    prepared: Res<'w, PreparedRenderResources<PreparedImageResource>>,
    batches: Res<'w, SpriteBatches>,
    frame: Res<'w, RenderFrame>,
    backend: Res<'w, WgpuRenderBackend>,
}

fn observe_product_render(
    inputs: ProductRenderInputs<'_, '_>,
    mut probe: ResMut<ProductRenderProbe>,
    mut commands: ResMut<GameplayCommandQueue>,
    mut exit: ResMut<AppExitRequests>,
) {
    let prepared_image = !inputs.prepared.is_empty();
    let textured_batch = inputs
        .batches
        .as_slice()
        .iter()
        .any(|batch| batch.material.image.is_some());
    let submitted_frame = submitted_product_frame(&inputs.frame, &inputs.backend);
    let product_frame_ready = prepared_image && textured_batch && submitted_frame;
    match probe.phase {
        ProductProbePhase::InitialFrame if product_frame_ready => {
            if let Some(continuity) = capture_continuity(&inputs) {
                let Some(tick) = inputs.fixed_time.tick().checked_add(1) else {
                    probe.evidence.timed_out.store(true, Ordering::SeqCst);
                    exit.request_exit();
                    return;
                };
                let submission = movement_command(tick, 9_000, MovementDirection::Left)
                    .expect("the engine-owned product movement command is valid");
                if commands.submit(submission).is_err() {
                    probe.evidence.timed_out.store(true, Ordering::SeqCst);
                    exit.request_exit();
                    return;
                }
                probe.baseline = Some(continuity);
                probe.phase = ProductProbePhase::Terminal;
            }
        }
        ProductProbePhase::Terminal if inputs.snapshot.is_terminal() => {
            let Some(tick) = inputs.fixed_time.tick().checked_add(1) else {
                probe.evidence.timed_out.store(true, Ordering::SeqCst);
                exit.request_exit();
                return;
            };
            let submission = retry_command(tick, 9_001)
                .expect("the engine-owned product Retry command is valid");
            if commands.submit(submission).is_err() {
                probe.evidence.timed_out.store(true, Ordering::SeqCst);
                exit.request_exit();
                return;
            }
            probe.phase = ProductProbePhase::ResetFrame;
        }
        ProductProbePhase::ResetFrame if inputs.wave_generation.get() >= 2 => {
            let floor = *probe.reset_frame_floor.get_or_insert(inputs.frame.index);
            if inputs.frame.index > floor && product_frame_ready {
                let continuity = capture_continuity(&inputs);
                let preserved = probe
                    .baseline
                    .as_ref()
                    .zip(continuity.as_ref())
                    .is_some_and(|(baseline, current)| baseline.matches(current));
                probe
                    .evidence
                    .prepared_image
                    .store(prepared_image, Ordering::SeqCst);
                probe
                    .evidence
                    .textured_batch
                    .store(textured_batch, Ordering::SeqCst);
                probe
                    .evidence
                    .submitted_frame
                    .store(submitted_frame, Ordering::SeqCst);
                probe
                    .evidence
                    .retry_continuity
                    .store(preserved, Ordering::SeqCst);
                exit.request_exit();
            }
        }
        _ => {}
    }
    if Instant::now() >= probe.deadline {
        probe.evidence.timed_out.store(true, Ordering::SeqCst);
        exit.request_exit();
    }
}

/// Emits the measurement marker after the complete Render stage has submitted and presented the
/// same frame. The gameplay probe remains in FixedUpdate because it drives command semantics;
/// startup timing must not wait for the next fixed tick merely to observe a prior present.
fn observe_startup_present(
    inputs: StartupPresentInputs<'_>,
    probe: Res<ProductRenderProbe>,
    mut exit: ResMut<AppExitRequests>,
) {
    let prepared_image = !inputs.prepared.is_empty();
    let textured_batch = inputs
        .batches
        .as_slice()
        .iter()
        .any(|batch| batch.material.image.is_some());
    if prepared_image
        && textured_batch
        && submitted_product_frame(&inputs.frame, &inputs.backend)
        && probe.marker.emit().is_err()
    {
        probe.evidence.timed_out.store(true, Ordering::SeqCst);
        exit.request_exit();
    }
}

fn submitted_product_frame(frame: &RenderFrame, backend: &WgpuRenderBackend) -> bool {
    let transaction = backend.frame_transaction_stats();
    frame.state == RenderFrameState::Submitted
        && transaction.frame_index() == Some(frame.index)
        && transaction.packet_admissions() == 1
        && transaction.packet_rejections() == 0
        && transaction.surface_acquire_attempts() == 1
        && transaction.surface_acquires() == 1
        && transaction.queue_submissions() == 1
        && transaction.presents() == 1
}

fn capture_continuity(inputs: &ProductRenderInputs<'_, '_>) -> Option<ProductContinuity> {
    let mut windows = inputs.windows.iter();
    let window_entity = windows.next()?;
    if windows.next().is_some() {
        return None;
    }
    Some(ProductContinuity {
        runtime_generation: inputs.runtime_generation.get(),
        window_entity,
        handles: (*inputs.handles).clone(),
        target: inputs.handles.snapshot(WindowId::PRIMARY).ok()?,
        backend_instance_id: inputs.backend.instance_id(),
        device_epoch: inputs.backend.device_epoch(),
        service_identity: Arc::clone(&inputs.service.identity),
        service_value: inputs.service.value,
    })
}
