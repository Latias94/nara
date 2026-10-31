//! Actual managed Runtime capture, surface presentation and independent image reconstruction.

use std::{
    cell::RefCell,
    error::Error,
    io,
    rc::Rc,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

use nara::{
    MinimalPlugins,
    project_host::{DesktopPresentationControl, DesktopRun, DesktopRunOptions, DesktopRunOutcome},
    transform::Transform2d,
    window::{PrimaryWindowId, Window, WindowId, WindowPlugin, WindowResolution},
};
use nara_app::{App, AppExit, CoreStage, RuntimeGeneration, RuntimeInstance, StartupStage};
use nara_ecs::{
    Commands, Query, Res, ResMut, Resource, SystemParam, schedule::IntoScheduleConfigs,
};
use nara_render::{
    Camera2d, ExtractedViews, FrameStats, RenderComponentsPlugin, RenderFrame, RenderFrameTarget,
    RenderPlugin, build_render_frame_packet,
};
use nara_render_wgpu::{
    WgpuCapturePlugin, WgpuPolicyCaptureSet, WgpuPolicyDefinition, WgpuPolicyFrameWriter,
    WgpuPolicyInstallation, WgpuRenderHost, WgpuRenderInput, WgpuRenderProgramError,
    WgpuWindowPresentationPlugin, install_wgpu_policy,
};

#[path = "../tests/support/mod.rs"]
pub mod support;

use support::{
    heatmap::{HeatmapCapture, HeatmapPolicy, ScalarField},
    image,
};

#[derive(Clone)]
struct Replay {
    input: WgpuRenderInput,
    capture: Arc<HeatmapCapture>,
}

/// One immutable sibling input from the same installation, capture Arc and extracted topology.
#[derive(Default, Resource)]
struct ReplaySlot(Option<Replay>);

#[derive(SystemParam)]
struct HeatmapSource<'w, 's> {
    field: Res<'w, ScalarField>,
    generation: Res<'w, RuntimeGeneration>,
    frame: Res<'w, RenderFrame>,
    primary: Res<'w, PrimaryWindowId>,
    windows: Query<'w, 's, &'static Window>,
    views: Res<'w, ExtractedViews>,
}

fn capture_heatmap(
    source: HeatmapSource<'_, '_>,
    installation: Res<'_, WgpuPolicyInstallation<HeatmapPolicy>>,
    mut writer: WgpuPolicyFrameWriter<'_>,
    mut replay: ResMut<'_, ReplaySlot>,
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
    let capture = source.field.capture();
    let input = installation.capture(packet, Arc::clone(&capture))?;
    if writer.capture(&installation, Arc::clone(&capture))? {
        replay.0 = Some(Replay { input, capture });
    }
    Ok(())
}

fn setup_camera(mut commands: Commands) {
    commands.spawn((Camera2d::default(), Transform2d::default()));
}

fn main() -> Result<(), Box<dyn Error>> {
    image::require_native_selection();
    let definition = WgpuPolicyDefinition::new(HeatmapPolicy::new(0));
    let installation = definition.install()?;
    let installation_id = installation.id();
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        WindowPlugin {
            primary_window: Some(Window::new(
                "Independent scalar-field renderer policy",
                WindowResolution::new(320, 192),
            )),
        },
        RenderComponentsPlugin,
        RenderPlugin::default(),
        WgpuCapturePlugin,
        WgpuWindowPresentationPlugin,
    ))?;
    install_wgpu_policy(&mut app, installation)?;
    app.insert_resource(ScalarField::default())?;
    app.insert_resource(ReplaySlot::default())?;
    app.add_systems(StartupStage::Scene, setup_camera)?;
    app.add_systems(
        CoreStage::Render,
        capture_heatmap.in_set(WgpuPolicyCaptureSet::Capture),
    )?;
    let runtime = RuntimeInstance::start(app)?;
    let retained = Rc::new(RefCell::new(None));
    let observed = Rc::clone(&retained);
    let deadline = Instant::now() + Duration::from_secs(20);
    let options = DesktopRunOptions::new()
        .with_native_retirement_budget(Duration::from_secs(2))
        .with_presentation_observer(move |observation| {
            if let Some(receipt) = observation.presented_frame() {
                let world = observation
                    .world()
                    .expect("presentation belongs to a live Runtime");
                let replay = world
                    .resource::<ReplaySlot>()
                    .0
                    .as_ref()
                    .expect("the policy system retained this frame's original capture");
                let packet = replay.input.packet();
                assert_eq!(replay.input.installation(), installation_id);
                assert_eq!(
                    packet.source_generation().runtime_generation(),
                    Some(observation.runtime_generation()),
                );
                assert_eq!(
                    receipt.runtime_generation(),
                    observation.runtime_generation()
                );
                assert_eq!(
                    receipt.render_transaction_frame_index(),
                    packet.frame_index()
                );
                let RenderFrameTarget::Window(window) = packet.target() else {
                    panic!("normal Runtime capture must still name its original window");
                };
                assert_eq!(receipt.window_id(), window.id);
                assert_eq!(receipt.window_id(), WindowId::PRIMARY);
                assert!(receipt.surface_generation() > 0 && receipt.device_epoch() > 0);
                // One independently encoded heatmap draw and the Host's final surface draw.
                assert_eq!(world.resource::<FrameStats>().draw_calls, 2);
                assert_eq!(world.resource::<FrameStats>().sprites, 0);
                let adapter = observation
                    .adapter_summary()
                    .expect("native adapter facts are required");
                assert_eq!(adapter.backend(), std::env::var("WGPU_BACKEND").unwrap());
                assert_ne!(
                    adapter.device_type().to_ascii_lowercase(),
                    "cpu",
                    "hardware qualification requires a hardware adapter",
                );
                println!(
                    "presented: runtime={} frame={} backend={} adapter={} type={}",
                    observation.runtime_generation().get(),
                    packet.frame_index(),
                    adapter.backend(),
                    adapter.name(),
                    adapter.device_type(),
                );
                *observed.borrow_mut() = Some((replay.clone(), receipt.backend_instance_id()));
                return DesktopPresentationControl::Exit(AppExit::Success);
            }
            if observation.now() >= deadline {
                return DesktopPresentationControl::Exit(AppExit::Requested);
            }
            DesktopPresentationControl::Continue
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
            "window presentation or retained cleanup failed: {:?}; {:?}",
            report.outcome(),
            report.diagnostics(),
        ))
        .into());
    }
    let (replay, original_backend) = retained
        .borrow_mut()
        .take()
        .expect("success requires one exact presented receipt");
    // Runtime/Host close has now dropped the original installation. The accepted input retains it.
    let extent = replay.input.packet().target().extent();
    let image_target = image::target(extent.width, extent.height);
    let input = replay.input.with_image_target(image_target)?;
    assert_eq!(input.installation(), replay.input.installation());
    assert_eq!(
        input.packet().source_generation(),
        replay.input.packet().source_generation(),
    );
    assert_eq!(
        input.packet().frame_index(),
        replay.input.packet().frame_index()
    );
    assert_eq!(input.packet().views(), replay.input.packet().views());
    assert_eq!(
        input.packet().world_canvas(),
        replay.input.packet().world_canvas(),
    );

    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer()?;
    host.prepare_policy(&producer, &definition)?;
    host.begin_frame(1)?;
    image::admit(
        &mut host,
        &producer,
        input.packet().source_generation(),
        image_target,
    );
    let output = host.submit(&producer, &input)?;
    assert_ne!(output.receipt().backend_instance_id(), original_backend);
    assert_eq!(
        output.receipt().render_transaction_frame_index(),
        input.packet().frame_index(),
    );
    image::assert_heatmap(&mut host, &output, &replay.capture, 0);
    image::settle(
        &host
            .retire_producer(&producer)
            .expect("the image producer submitted work"),
    );
    drop((output, producer));
    image::finish(&mut host);
    assert_eq!(host.retained_resource_bytes(), 0);
    println!("render-policy-window: presented, reconstructed and retired");
    Ok(())
}
