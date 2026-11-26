use nara_scene::spatial::{SpatialPlugin, Transform2d};
use nara_window::{WindowPlugin, WindowResolution};

use crate::{Camera2d, RenderFrameTarget, RenderPlugin, WorldCanvas};

use super::*;

fn desktop_runtime(world_canvas: Option<WorldCanvas>, camera: bool) -> RuntimeInstance {
    let mut app = App::new();
    app.add_plugins((
        SpatialPlugin,
        WindowPlugin {
            primary_window: Some(Window::new(
                "Desktop test",
                WindowResolution::new(1152, 768),
            )),
        },
        RenderPlugin { world_canvas },
        DesktopPresentationPlugin,
    ))
    .unwrap();
    if camera {
        app.world_mut()
            .unwrap()
            .spawn((Camera2d::default(), Transform2d::default()));
    }
    RuntimeInstance::start(app).unwrap()
}

#[test]
fn capture_keeps_world_canvas_and_surface_at_distinct_extents() {
    let canvas = WorldCanvas::new(320, 180).unwrap();
    let mut runtime = desktop_runtime(Some(canvas), true);
    runtime.drive(Duration::from_millis(16)).unwrap();
    let world = runtime.world();
    let window = world
        .iter_entities()
        .find_map(|entity| entity.get::<Window>())
        .unwrap();
    let packet = capture_window_packet(world, runtime.generation(), 1, window)
        .unwrap()
        .unwrap();

    assert!(matches!(packet.target(), RenderFrameTarget::Window(target) if target.id == window.id));
    assert_eq!(packet.target().extent(), Extent2d::new(1152, 768).unwrap());
    assert_eq!(packet.world_canvas().unwrap().extent(), canvas.extent());
    assert_eq!(packet.views()[0].viewport.extent(), canvas.extent());
}

#[test]
fn camera_free_capture_uses_window_space_only_for_hud() {
    let mut runtime = desktop_runtime(None, false);
    runtime.drive(Duration::from_millis(16)).unwrap();
    let world = runtime.world();
    let window = world
        .iter_entities()
        .find_map(|entity| entity.get::<Window>())
        .unwrap();
    let packet = capture_window_packet(world, runtime.generation(), 1, window).unwrap();

    assert_eq!(packet.is_some(), cfg!(feature = "hud-runtime"));
    if let Some(packet) = packet {
        assert!(packet.views().is_empty());
        assert_eq!(packet.target().extent(), Extent2d::new(1152, 768).unwrap());
    }
}

#[test]
fn maintenance_without_a_native_surface_cannot_publish_a_frame() {
    let mut runtime = desktop_runtime(None, true);
    let mut driver = DesktopPresentationDriver::default();
    let handles = BackendWindowHandles::default();
    runtime.drive(Duration::from_millis(16)).unwrap();

    let now = Instant::now();
    assert_eq!(
        driver.maintain(&mut runtime, &handles, now).unwrap(),
        Default::default()
    );
    assert_eq!(
        runtime.world().resource::<DesktopPresentation>().latest(),
        None
    );
    assert!(
        driver
            .retire(&mut runtime, &handles, &[WindowId::PRIMARY], now)
            .unwrap()
    );
}

#[test]
fn surface_retry_waits_through_earlier_callbacks_and_new_frames() {
    let mut runtime = desktop_runtime(None, true);
    let now = Instant::now();
    let retry_at = now + SURFACE_RETRY_INTERVAL;
    let mut driver = DesktopPresentationDriver {
        retry_at: Some(retry_at),
        ..Default::default()
    };
    let handles = BackendWindowHandles::default();

    for check_at in [now, retry_at - Duration::from_millis(1)] {
        runtime.drive(Duration::from_millis(16)).unwrap();
        let progress = driver.maintain(&mut runtime, &handles, check_at).unwrap();
        assert_eq!(progress.next_wake, Some(retry_at));
        assert_eq!(driver.last_frame, None);
    }
    let progress = driver.maintain(&mut runtime, &handles, retry_at).unwrap();
    assert_eq!(progress, Default::default());
    assert!(driver.last_frame.is_some());
    assert!(driver.retry_at.is_none());
}

#[test]
#[ignore = "requires a native WGPU adapter"]
fn a_lost_device_reports_once_and_releases_the_renderer_on_close() {
    let mut runtime = desktop_runtime(None, true);
    let context = GpuContext::request(wgpu::Instance::default(), None).unwrap();
    context.device().destroy();
    let mut driver = DesktopPresentationDriver {
        renderer: Some(Renderer::new(context)),
        ..Default::default()
    };
    let handles = BackendWindowHandles::default();
    let now = Instant::now();

    let error = driver.maintain(&mut runtime, &handles, now).unwrap_err();
    assert!(error.to_string().contains("device was lost"));
    assert!(
        driver
            .retire(&mut runtime, &handles, &[WindowId::PRIMARY], now)
            .unwrap()
    );
    assert!(driver.renderer.is_none());
}

#[cfg(feature = "hud-runtime")]
#[test]
fn clearing_a_display_withdraws_hud_hits_and_keeps_the_candidate_for_retry() {
    use nara_ui::{
        UiNode, UiPrimarySurfaceMetrics, UiRoot, UiSurfaceEndpoint, UiSurfaceId, UiSurfaceSlot,
        UiText, install_ui_surface, latest_presented_ui_frame, ui_candidate_frame,
    };

    let surface = UiSurfaceId::new(1).unwrap();
    let endpoint = UiSurfaceEndpoint::new(
        nara_input::InputWindowId::PRIMARY,
        nara_input::InputSeatId::PRIMARY,
        nara_input::InputPointerId::PRIMARY,
    );
    let transform = UiPrimarySurfaceMetrics::new(surface, 320, 180, [0; 4])
        .direct_transform()
        .unwrap();
    let mut app = App::new();
    app.add_plugins((nara_input::InputPlugin, nara_ui::UiPlugin::default()))
        .unwrap();
    let world = app.world_mut().unwrap();
    install_ui_surface(
        world,
        UiSurfaceSlot::new(surface, endpoint, transform).unwrap(),
    )
    .unwrap();
    world.spawn((
        UiRoot::default(),
        UiNode::default(),
        UiText::new("Retry", 18.0).unwrap(),
    ));
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::from_millis(16)).unwrap();

    runtime
        .with_driver_scope(|scope| {
            scope.with_world_mut(|world| {
                let candidate = ui_candidate_frame(world, surface).unwrap().unwrap();
                let hud = crate::hud::HudPresentation {
                    frame: candidate.clone(),
                    glyph_count: 5,
                };
                let mut driver = DesktopPresentationDriver::default();

                assert_eq!(driver.publish_hud(world, Some(&hud), 1).unwrap(), 5);
                assert!(latest_presented_ui_frame(world, surface).is_some());
                driver.clear_display(world);
                assert!(latest_presented_ui_frame(world, surface).is_none());
                assert!(std::sync::Arc::ptr_eq(
                    &candidate,
                    &ui_candidate_frame(world, surface).unwrap().unwrap()
                ));
                assert_eq!(driver.publish_hud(world, Some(&hud), 2).unwrap(), 5);
                assert_eq!(driver.publish_hud(world, None, 3).unwrap(), 0);
                assert!(latest_presented_ui_frame(world, surface).is_none());
            })
        })
        .unwrap();
}
