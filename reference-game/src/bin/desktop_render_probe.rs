use std::{
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use nara::{
    app::{
        AppExit, AppExitRequests, PluginCategory, PluginDeclaration, PluginDefinition,
        PluginDefinitionId, PluginError, PluginId, RuntimeGeneration, StartupStage,
    },
    asset::{AssetEvents, AssetStates, Assets},
    ecs::{Entity, With, schedule::IntoScheduleConfigs},
    gameplay::{GameplayCommandQueue, GameplayCommandSet},
    image::{ImageAsset, PreparedImages},
    prelude::{App, CoreStage, FixedTime, Plugin, ResMut, Resource, World},
    project_host::{
        DesktopPresentationControl, DesktopPresentationObservation, DesktopRunOptions,
        DesktopRunOutcome,
        advanced::{RuntimeProbe, desktop_run_with_probe},
    },
    render::RenderFrame,
    render_wgpu::{__private::presented_frame_this_transaction, WgpuRenderObservation},
    sprite_render::__private::SpriteBatches,
    ui_render::advanced::latest_presented_ui_observation,
    window::{
        Window, WindowId,
        backend::{BackendWindowHandles, WindowTargetSnapshot},
    },
};
use nara_reference_game::{
    MovementDirection, REFERENCE_DESKTOP_PLUGIN_ID, WaveRunGeneration, WaveSnapshot,
    movement_command, retry_command, wave_desktop_intent,
};

mod desktop_support;
mod startup_marker;
mod support;

use desktop_support::{resident_product_glyphs, submitted_product_frame};
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
    let probe = PluginDefinition::infallible::<ProductRenderProbePlugin, _>(
        PRODUCT_RENDER_PROBE_DEFINITION_ID,
        b"reference-game-product-render-probe-v1",
        move || ProductRenderProbePlugin {
            evidence: Arc::clone(&plugin_evidence),
        },
    );
    let observer_evidence = Arc::clone(&evidence);
    let observer_marker = Arc::clone(&marker);
    let execution = DesktopRunOptions::new().with_presentation_observer(move |observation| {
        observe_startup_present(observation, &observer_marker, &observer_evidence)
    });
    let mut run = desktop_run_with_probe(
        root,
        wave_desktop_intent().with_execution(execution),
        RuntimeProbe::new(probe),
    );
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
    let timed_out = evidence.timed_out.load(Ordering::SeqCst);
    let prepared_image = evidence.prepared_image.load(Ordering::SeqCst);
    let textured_batch = evidence.textured_batch.load(Ordering::SeqCst);
    let ui_glyph_texture = evidence.ui_glyph_texture.load(Ordering::SeqCst);
    let submitted_frame = evidence.submitted_frame.load(Ordering::SeqCst);
    let retry_continuity = evidence.retry_continuity.load(Ordering::SeqCst);
    if report.outcome() != DesktopRunOutcome::Completed(AppExit::Requested)
        || report.diagnostics().has_errors()
        || timed_out
        || !prepared_image
        || !textured_batch
        || !ui_glyph_texture
        || !submitted_frame
        || !retry_continuity
    {
        let diagnostics = report
            .diagnostics()
            .iter()
            .map(|diagnostic| (diagnostic.code().as_str(), diagnostic.fields()))
            .collect::<Vec<_>>();
        eprintln!(
            "desktop_render_probe.details outcome={:?} timed_out={} prepared_image={} textured_batch={} ui_glyph_texture={} submitted_frame={} retry_continuity={} diagnostics={diagnostics:?}",
            report.outcome(),
            timed_out,
            prepared_image,
            textured_batch,
            ui_glyph_texture,
            submitted_frame,
            retry_continuity,
        );
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
    ui_glyph_texture: AtomicBool,
    submitted_frame: AtomicBool,
    retry_continuity: AtomicBool,
    timed_out: AtomicBool,
}

#[derive(Debug)]
struct ProductRenderProbePlugin {
    evidence: Arc<ProductRenderEvidence>,
}

impl Plugin for ProductRenderProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &PRODUCT_RENDER_PROBE_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(ProductRenderProbe {
            evidence: Arc::clone(&self.evidence),
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
        .add_systems(StartupStage::Tooling, fail_retry_before_first_image_prepare)?;
        Ok(())
    }
}

fn fail_retry_before_first_image_prepare(
    mut images: ResMut<Assets<ImageAsset>>,
    mut states: ResMut<AssetStates>,
    mut events: ResMut<AssetEvents>,
) {
    let handles = images.iter().map(|(handle, _)| handle).collect::<Vec<_>>();
    assert!(
        !handles.is_empty(),
        "the product must publish an image before startup"
    );
    for handle in handles {
        let version = states.set_loading(handle.id());
        images
            .record_reload_failure(handle, &mut states, &mut events, "probe retry failed")
            .unwrap();
        assert_eq!(
            states.state(handle.id()).unwrap().usable_version(),
            Some(version)
        );
    }
}

#[derive(Debug, Resource)]
struct ProductRenderProbe {
    evidence: Arc<ProductRenderEvidence>,
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

fn observe_product_render(world: &mut World) {
    let prepared_image = !world.resource::<PreparedImages>().is_empty();
    let textured_batch = world
        .resource::<SpriteBatches>()
        .as_slice()
        .iter()
        .any(|batch| batch.image.is_some());
    let observation = world.resource::<WgpuRenderObservation>();
    let submitted_frame = submitted_product_frame(observation);
    let completed_frame = observation
        .frame_transaction_stats()
        .render_transaction_frame_index();
    let observed_ui_glyph_texture = latest_presented_ui_observation(world)
        .is_some_and(|presented| resident_product_glyphs(observation, &presented));
    let product_frame_ready = prepared_image && textured_batch && submitted_frame;
    let phase = world.resource::<ProductRenderProbe>().phase;
    match phase {
        ProductProbePhase::InitialFrame if product_frame_ready => {
            if let Some(continuity) = capture_continuity(world) {
                let Some(tick) = world.resource::<FixedTime>().tick().checked_add(1) else {
                    fail_product_probe(world);
                    return;
                };
                let submission = movement_command(tick, 9_000, MovementDirection::Left)
                    .expect("the engine-owned product movement command is valid");
                if world
                    .resource_mut::<GameplayCommandQueue>()
                    .submit(submission)
                    .is_err()
                {
                    fail_product_probe(world);
                    return;
                }
                let mut probe = world.resource_mut::<ProductRenderProbe>();
                probe.baseline = Some(continuity);
                probe.phase = ProductProbePhase::Terminal;
            }
        }
        ProductProbePhase::Terminal if world.resource::<WaveSnapshot>().is_terminal() => {
            let Some(tick) = world.resource::<FixedTime>().tick().checked_add(1) else {
                fail_product_probe(world);
                return;
            };
            let submission = retry_command(tick, 9_001)
                .expect("the engine-owned product Retry command is valid");
            if world
                .resource_mut::<GameplayCommandQueue>()
                .submit(submission)
                .is_err()
            {
                fail_product_probe(world);
                return;
            }
            world.resource_mut::<ProductRenderProbe>().phase = ProductProbePhase::ResetFrame;
        }
        ProductProbePhase::ResetFrame if world.resource::<WaveRunGeneration>().get() >= 2 => {
            // The current simulation capture clock is only a lower bound here. Submission and
            // glyph evidence below must belong to a later, actually completed transaction.
            let capture_floor = world.resource::<RenderFrame>().index;
            let floor = *world
                .resource_mut::<ProductRenderProbe>()
                .reset_frame_floor
                .get_or_insert(capture_floor);
            if completed_frame.is_some_and(|frame| frame > floor)
                && product_frame_ready
                && observed_ui_glyph_texture
            {
                let continuity = capture_continuity(world);
                let probe = world.resource::<ProductRenderProbe>();
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
                    .ui_glyph_texture
                    .store(observed_ui_glyph_texture, Ordering::SeqCst);
                probe
                    .evidence
                    .submitted_frame
                    .store(submitted_frame, Ordering::SeqCst);
                probe
                    .evidence
                    .retry_continuity
                    .store(preserved, Ordering::SeqCst);
                world.resource_mut::<AppExitRequests>().request_exit();
            }
        }
        _ => {}
    }
    if Instant::now() >= world.resource::<ProductRenderProbe>().deadline {
        fail_product_probe(world);
    }
}

fn fail_product_probe(world: &mut World) {
    world
        .resource::<ProductRenderProbe>()
        .evidence
        .timed_out
        .store(true, Ordering::SeqCst);
    world.resource_mut::<AppExitRequests>().request_exit();
}

/// Emits the marker at the actual native present callback. Maintenance advances the same finite
/// timeout without another gameplay tick or an artificial invocation of Last.
fn observe_startup_present(
    observation: DesktopPresentationObservation<'_>,
    marker: &StartupMarker,
    evidence: &ProductRenderEvidence,
) -> DesktopPresentationControl {
    let Some(world) = observation.world() else {
        return DesktopPresentationControl::Continue;
    };
    if observation.now() >= world.resource::<ProductRenderProbe>().deadline {
        evidence.timed_out.store(true, Ordering::SeqCst);
        return DesktopPresentationControl::Exit(AppExit::Requested);
    }
    let Some(presented_frame) = observation.presented_frame() else {
        return DesktopPresentationControl::Continue;
    };
    if presented_frame.runtime_generation() != observation.runtime_generation()
        || presented_frame_this_transaction(observation.wgpu()) != Some(presented_frame)
    {
        return DesktopPresentationControl::Continue;
    }
    let prepared_image = !world.resource::<PreparedImages>().is_empty();
    let textured_batch = world
        .resource::<SpriteBatches>()
        .as_slice()
        .iter()
        .any(|batch| batch.image.is_some());
    let ui_glyph_texture = latest_presented_ui_observation(world)
        .is_some_and(|presented| resident_product_glyphs(observation.wgpu(), &presented));
    let submitted_frame = submitted_product_frame(observation.wgpu());
    evidence
        .prepared_image
        .fetch_or(prepared_image, Ordering::SeqCst);
    evidence
        .textured_batch
        .fetch_or(textured_batch, Ordering::SeqCst);
    evidence
        .ui_glyph_texture
        .fetch_or(ui_glyph_texture, Ordering::SeqCst);
    evidence
        .submitted_frame
        .fetch_or(submitted_frame, Ordering::SeqCst);
    if prepared_image
        && textured_batch
        && ui_glyph_texture
        && submitted_frame
        && marker.emit().is_err()
    {
        evidence.timed_out.store(true, Ordering::SeqCst);
        return DesktopPresentationControl::Exit(AppExit::Requested);
    }
    DesktopPresentationControl::Continue
}

fn capture_continuity(world: &mut World) -> Option<ProductContinuity> {
    let mut query = world.query_filtered::<Entity, With<Window>>();
    let mut windows = query.iter(world);
    let window_entity = windows.next()?;
    if windows.next().is_some() {
        return None;
    }
    Some(ProductContinuity {
        runtime_generation: world.resource::<RuntimeGeneration>().get(),
        window_entity,
        handles: world.resource::<BackendWindowHandles>().clone(),
        target: world
            .resource::<BackendWindowHandles>()
            .snapshot(WindowId::PRIMARY)
            .ok()?,
        backend_instance_id: world.resource::<WgpuRenderObservation>().instance_id(),
        device_epoch: world.resource::<WgpuRenderObservation>().device_epoch(),
        service_identity: Arc::clone(&world.resource::<ProductServiceSentinel>().identity),
        service_value: world.resource::<ProductServiceSentinel>().value,
    })
}
