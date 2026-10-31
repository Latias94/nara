//! Four real temporal accumulators through managed Runtime capture and native presentation.

#![forbid(unsafe_code)]

use std::{
    error::Error,
    io,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

use nara::{
    MinimalPlugins,
    core::Color,
    project_host::{DesktopPresentationControl, DesktopRun, DesktopRunOptions, DesktopRunOutcome},
    transform::Transform2d,
    window::{PrimaryWindowId, Window, WindowId, WindowPlugin, WindowResolution},
};
use nara_app::{App, AppExit, CoreStage, RuntimeGeneration, RuntimeInstance, StartupStage};
use nara_ecs::{
    Commands, Query, Res, ResMut, Resource, SystemParam, schedule::IntoScheduleConfigs,
};
use nara_render::{
    Camera2d, Extent2d, ExtractedViews, FrameStats, RenderBackendStatus, RenderComponentsPlugin,
    RenderFrame, RenderFrameState, RenderFrameTarget, RenderPlugin, ViewportRect,
    build_render_frame_packet,
};
use nara_render_wgpu::{
    WgpuCapturePlugin, WgpuEmptyHistoryReason, WgpuHistoryLineage, WgpuHistoryView,
    WgpuPolicyCaptureSet, WgpuPolicyDefinition, WgpuPolicyFrameWriter, WgpuPolicyInstallation,
    WgpuRenderInput, WgpuRenderProgramError, WgpuWindowPresentationPlugin, install_wgpu_policy,
};

#[path = "../tests/support/accumulation.rs"]
mod accumulation;

use accumulation::{
    Effect, HEIGHT, HistoryCapture, HistoryPolicy, Predecessor, Topology, VIEW_WIDTH, ViewTopology,
    WIDTH,
};

const PRESENTATIONS: usize = 3;
const UNINITIALIZED: Predecessor = Predecessor::Empty(WgpuEmptyHistoryReason::Uninitialized);

/// Created once from the first actual camera extraction and managed Runtime packet.
struct OriginalSource {
    topology: Topology,
    lineage: WgpuHistoryLineage,
}

#[derive(Default, Resource)]
struct HistorySource(Option<OriginalSource>);

#[derive(Clone)]
struct CapturedFrame {
    input: WgpuRenderInput,
    capture: Arc<HistoryCapture>,
    predecessor: Predecessor,
}

#[derive(Default, Resource)]
struct CapturedFrameSlot(Option<CapturedFrame>);

/// The post-Host observer acknowledges actual publications for the next ECS capture.
/// No App-frame or Host-frame counter supplies an assumed predecessor.
#[derive(Clone, Default, Resource)]
struct PresentedFrames(Arc<Mutex<Vec<CapturedFrame>>>);

#[derive(SystemParam)]
struct RuntimeSource<'w, 's> {
    generation: Res<'w, RuntimeGeneration>,
    frame: Res<'w, RenderFrame>,
    primary: Res<'w, PrimaryWindowId>,
    windows: Query<'w, 's, &'static Window>,
    views: Res<'w, ExtractedViews>,
}

fn capture_history(
    source: RuntimeSource<'_, '_>,
    installation: Res<'_, WgpuPolicyInstallation<HistoryPolicy>>,
    mut writer: WgpuPolicyFrameWriter<'_>,
    mut original: ResMut<'_, HistorySource>,
    presented: Res<'_, PresentedFrames>,
    mut slot: ResMut<'_, CapturedFrameSlot>,
) -> nara_ecs::bevy_ecs::error::Result {
    if !writer.can_capture(&installation) {
        return Ok(());
    }
    let Some(packet) = build_render_frame_packet(
        *source.generation,
        source.frame.index,
        Some(source.primary.0),
        source.windows.iter(),
        &source.views,
    )
    .map_err(|error| WgpuRenderProgramError::feature(error.to_string()))?
    else {
        return Ok(());
    };
    assert_eq!(
        packet.views().len(),
        2,
        "two ordinary cameras must be extracted",
    );
    assert_eq!(
        packet.target().extent(),
        Extent2d::new(WIDTH, HEIGHT).unwrap()
    );
    assert_eq!(
        packet.source_generation().runtime_generation(),
        Some(*source.generation),
    );
    assert!(matches!(packet.target(), RenderFrameTarget::Window(_)));

    if original.0.is_none() {
        // Use the camera owners' actual IDs; Topology::new's independent View IDs are not used.
        let topology = Topology {
            views: std::array::from_fn(|index| {
                let id = packet.views()[index].id;
                ViewTopology {
                    index,
                    id,
                    history_view: WgpuHistoryView::new(id),
                    effects: std::array::from_fn(|effect| {
                        Effect::new(effect, (2 * index + effect) as u32)
                    }),
                }
            }),
        };
        let lineage = WgpuHistoryLineage::new(packet.source_generation())?;
        original.0 = Some(OriginalSource { topology, lineage });
    }
    let original = original
        .0
        .as_ref()
        .expect("the original source was initialized once");
    let (phase, predecessor) = {
        let frames = presented
            .0
            .lock()
            .expect("presentation acknowledgment is available");
        if frames.len() == PRESENTATIONS {
            return Ok(());
        }
        (
            frames.len() as u32,
            frames.last().map_or(UNINITIALIZED, |previous| {
                Predecessor::Frame(previous.input.packet().frame_index())
            }),
        )
    };
    // HistoryPolicy::build checks all four selected predecessors before encoding each frame.
    let capture = original
        .topology
        .capture(&original.lineage, phase, false, [predecessor; 4]);
    // Retain a sibling input for inspection, using the same Arc and actual window packet.
    // Only the normal writer/seal/Runtime/window producer path executes this policy.
    let input = installation.capture(packet, Arc::clone(&capture))?;
    if writer.capture(&installation, Arc::clone(&capture))? {
        slot.0 = Some(CapturedFrame {
            input,
            capture,
            predecessor,
        });
    }
    Ok(())
}

fn setup_cameras(mut commands: Commands) {
    for index in 0..2 {
        commands.spawn((
            Camera2d {
                viewport: Some(
                    ViewportRect::new(index * VIEW_WIDTH, 0, VIEW_WIDTH, HEIGHT).unwrap(),
                ),
                clear_color: (index == 0).then_some(Color::BLACK),
                viewport_height: HEIGHT as f32,
                order: index as i32,
                ..Camera2d::default()
            },
            Transform2d::default(),
        ));
    }
}

fn require_native_selection() -> String {
    let backend = std::env::var("WGPU_BACKEND").expect("explicitly select WGPU_BACKEND");
    assert!(
        matches!(backend.as_str(), "dx12" | "vulkan" | "metal"),
        "select exactly one native backend: dx12, vulkan or metal",
    );
    assert_eq!(
        std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref(),
        Ok("0"),
        "native qualification requires NARA_WGPU_FORCE_FALLBACK=0",
    );
    backend
}

fn main() -> Result<(), Box<dyn Error>> {
    let selected_backend = require_native_selection();
    let installation = WgpuPolicyDefinition::new(HistoryPolicy::new()).install()?;
    let installation_id = installation.id();
    let mut window = Window::new(
        "Independent temporal history policy",
        WindowResolution::new(WIDTH, HEIGHT),
    );
    window.resizable = false;
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        WindowPlugin {
            primary_window: Some(window),
        },
        RenderComponentsPlugin,
        RenderPlugin::default(),
        WgpuCapturePlugin,
        WgpuWindowPresentationPlugin,
    ))?;
    install_wgpu_policy(&mut app, installation)?;
    let presented = PresentedFrames::default();
    app.insert_resource(presented.clone())?;
    app.insert_resource(HistorySource::default())?;
    app.insert_resource(CapturedFrameSlot::default())?;
    app.add_systems(StartupStage::Scene, setup_cameras)?;
    app.add_systems(
        CoreStage::Render,
        capture_history.in_set(WgpuPolicyCaptureSet::Capture),
    )?;
    let runtime = RuntimeInstance::start(app)?;
    let deadline = Instant::now() + Duration::from_secs(20);
    let options = DesktopRunOptions::new()
        .with_native_retirement_budget(Duration::from_secs(2))
        .with_presentation_observer(move |observation| {
            let Some(world) = observation.world() else {
                return DesktopPresentationControl::Continue;
            };
            if observation.now() >= deadline {
                eprintln!(
                    "history-window: presentation deadline exceeded; backend={:?}",
                    world.get_resource::<RenderBackendStatus>(),
                );
                return DesktopPresentationControl::Exit(AppExit::Requested);
            }
            let Some(receipt) = observation.presented_frame() else {
                return DesktopPresentationControl::Continue;
            };
            let captured = world
                .resource::<CapturedFrameSlot>()
                .0
                .as_ref()
                .expect("the normal ECS collector retained this frame's exact capture");
            let packet = captured.input.packet();
            assert_eq!(captured.input.installation(), installation_id);
            assert!(std::ptr::eq(
                captured.input.captured::<HistoryPolicy>().unwrap(),
                captured.capture.as_ref(),
            ));
            assert_eq!(
                packet.source_generation().runtime_generation(),
                Some(observation.runtime_generation()),
            );
            assert_eq!(
                captured.capture.lineage.source_generation(),
                packet.source_generation(),
            );
            assert_eq!(receipt.runtime_generation(), observation.runtime_generation());
            assert_eq!(receipt.render_transaction_frame_index(), packet.frame_index());
            assert_eq!(packet.views(), world.resource::<ExtractedViews>().as_slice());
            assert_eq!(world.resource::<RenderFrame>().index, packet.frame_index());
            assert_eq!(
                world.resource::<RenderFrame>().state,
                RenderFrameState::Submitted,
            );
            let RenderFrameTarget::Window(window) = packet.target() else {
                panic!("the normal Runtime producer must present the original window target");
            };
            assert_eq!(receipt.window_id(), window.id);
            assert_eq!(receipt.window_id(), WindowId::PRIMARY);
            assert!(receipt.surface_generation() > 0 && receipt.device_epoch() > 0);
            assert_eq!(
                observation.wgpu().instance_id(),
                receipt.backend_instance_id(),
            );
            assert_eq!(observation.wgpu().device_epoch(), receipt.device_epoch());
            assert_eq!(
                observation.wgpu().configured_surface_extent(window.id),
                Extent2d::new(WIDTH, HEIGHT),
            );
            let transaction = observation.wgpu().frame_transaction_stats();
            assert_eq!(
                transaction.render_transaction_frame_index(),
                Some(packet.frame_index()),
            );
            assert_eq!(
                transaction.app_frame_index(),
                Some(receipt.app_frame_index()),
            );
            assert_eq!(transaction.packet_admissions(), 1);
            assert_eq!(transaction.packet_rejections(), 0);
            assert_eq!(transaction.surface_acquire_attempts(), 1);
            assert_eq!(transaction.surface_acquires(), 1);
            assert_eq!(transaction.presents(), 1);
            assert!(
                transaction.queue_submissions() > 0,
                "actual native submission is required",
            );
            // Four accumulators, two View composites and the Host's final surface draw.
            assert_eq!(world.resource::<FrameStats>().draw_calls, 7);
            assert_eq!(world.resource::<FrameStats>().sprites, 0);
            let adapter = observation
                .adapter_summary()
                .expect("native adapter facts are required");
            assert_eq!(adapter.backend(), selected_backend);
            assert_ne!(adapter.device_type().to_ascii_lowercase(), "cpu");

            let mut frames = world
                .resource::<PresentedFrames>()
                .0
                .lock()
                .expect("presentation acknowledgment is available");
            assert!(frames.len() < PRESENTATIONS);
            assert_eq!(captured.capture.phase, frames.len() as u32);
            let expected = frames.last().map_or(UNINITIALIZED, |previous| {
                assert!(packet.frame_index() > previous.input.packet().frame_index());
                assert_eq!(
                    packet.source_generation(),
                    previous.input.packet().source_generation(),
                );
                assert_eq!(packet.views(), previous.input.packet().views());
                Predecessor::Frame(previous.input.packet().frame_index())
            });
            assert_eq!(captured.predecessor, expected);
            println!(
                "history-window: presented={} runtime={} frame={} predecessor={expected:?} backend={} adapter={} type={}",
                frames.len() + 1,
                receipt.runtime_generation().get(),
                packet.frame_index(),
                adapter.backend(),
                adapter.name(),
                adapter.device_type(),
            );
            frames.push(captured.clone());
            if frames.len() == PRESENTATIONS {
                DesktopPresentationControl::Exit(AppExit::Success)
            } else {
                DesktopPresentationControl::Continue
            }
        });
    let mut desktop = DesktopRun::from_runtime(runtime, options);
    let mut report = desktop.execute();
    let cleanup_deadline = Instant::now() + Duration::from_secs(10);
    while report.outcome() == DesktopRunOutcome::CleanupIncomplete
        && Instant::now() < cleanup_deadline
    {
        thread::sleep(Duration::from_millis(1));
        report = desktop.execute();
    }
    if report.outcome() != DesktopRunOutcome::Completed(AppExit::Success) {
        return Err(io::Error::other(format!(
            "temporal window presentation or native retirement failed: {:?}; {:?}",
            report.outcome(),
            report.diagnostics(),
        ))
        .into());
    }

    // Runtime close has retired the actual native producer and dropped the original ECS owners.
    // Retained inputs still name the original installation, captures, Views and Runtime lineage.
    let frames = presented
        .0
        .lock()
        .expect("retained presentation evidence is available");
    assert_eq!(frames.len(), PRESENTATIONS);
    for (phase, captured) in frames.iter().enumerate() {
        assert_eq!(captured.input.installation(), installation_id);
        assert_eq!(captured.capture.phase, phase as u32);
        assert!(std::ptr::eq(
            captured.input.captured::<HistoryPolicy>().unwrap(),
            captured.capture.as_ref(),
        ));
        assert_eq!(
            captured.capture.lineage.source_generation(),
            captured.input.packet().source_generation(),
        );
        assert_eq!(
            captured.predecessor,
            if phase == 0 {
                UNINITIALIZED
            } else {
                Predecessor::Frame(frames[phase - 1].input.packet().frame_index())
            },
        );
        assert!(matches!(
            captured.input.packet().target(),
            RenderFrameTarget::Window(_),
        ));
    }
    println!("render-policy-history-window: three temporal publications presented and retired");
    Ok(())
}
