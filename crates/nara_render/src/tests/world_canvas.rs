use super::*;
use nara_ecs::bevy_ecs::system::SystemState;

use crate::__private::prepare_world_presentation;

fn publish(
    world: &mut World,
    packet: &RenderFramePacket,
) -> crate::__private::WorldPresentationGuard {
    let (presented, guard) = prepare_world_presentation(packet).unwrap();
    guard.mark_presented();
    world.insert_resource(presented);
    guard
}

fn canvas_packet(window: &Window, camera: Entity, canvas: WorldCanvas) -> RenderFramePacket {
    let viewport = ViewportRect::from_extent(canvas.extent()).unwrap();
    canvas_packet_with_viewport(window, camera, canvas, viewport)
}

fn canvas_packet_with_viewport(
    window: &Window,
    camera: Entity,
    canvas: WorldCanvas,
    viewport: ViewportRect,
) -> RenderFramePacket {
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        camera_entity: camera,
        target: RenderTarget::PrimaryWindow,
        viewport,
        mapping: ViewMapping2d::orthographic(
            nara_core::Mat3::IDENTITY,
            canvas.extent().height as f32,
            viewport,
        )
        .unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(Color::BLACK),
    });
    build_render_frame_packet_with_world_canvas(
        test_runtime_generation(),
        1,
        Some(window.id),
        [window],
        &views,
        Some(canvas),
    )
    .unwrap()
    .unwrap()
}

#[test]
fn world_canvas_packet_keeps_native_geometry_and_maps_in_physical_integer_pixels() {
    let canvas = WorldCanvas::new(320, 180).unwrap();
    let window = Window {
        resolution: nara_window::WindowResolution::new(1_001, 701).with_scale_factor(1.5),
        ..Default::default()
    };
    let packet = canvas_packet(&window, Entity::from_raw_u32(1).unwrap(), canvas);
    assert_eq!(packet.target().extent(), Extent2d::new(1_001, 701).unwrap());
    let captured = packet.world_canvas().unwrap();
    assert_eq!(captured.extent(), canvas.extent());
    assert_eq!(packet.views()[0].viewport.extent(), canvas.extent());
    let mapping = captured.mapping();
    let rect = mapping.content_rect();
    assert_eq!(mapping.pixels_per_point(), 1.5);
    let width = (rect.max[0] - rect.min[0]) * mapping.pixels_per_point();
    let height = (rect.max[1] - rect.min[1]) * mapping.pixels_per_point();
    assert!((width - 960.0).abs() < 0.001);
    assert!((height - 540.0).abs() < 0.001);
    assert!((rect.min[0] * 1.5 - 21.0).abs() < 0.001);
    assert!((rect.min[1] * 1.5 - 81.0).abs() < 0.001);
    assert_eq!(captured.bar_color(), Color::BLACK);
}

#[test]
fn undersized_canvas_packet_downscales_without_clipping_the_world() {
    let window = Window {
        resolution: nara_window::WindowResolution::new(80, 60),
        ..Default::default()
    };
    let packet = canvas_packet(
        &window,
        Entity::from_raw_u32(1).unwrap(),
        WorldCanvas::new(320, 180).unwrap(),
    );
    let mapping = packet.world_canvas().unwrap().mapping();
    assert_eq!(
        mapping.content_rect(),
        DisplayRect {
            min: [0.0, 7.5],
            max: [80.0, 52.5]
        }
    );
    assert_eq!(mapping.panel_to_target([40.0, 30.0]), Some([160.0, 90.0]));
    assert_eq!(mapping.panel_to_target([40.0, 0.0]), None);
}

#[test]
fn presented_world_pointer_requires_present_current_geometry_and_live_owner() {
    let mut world = World::new();
    let window = Window {
        resolution: nara_window::WindowResolution::new(1_000, 700).with_scale_factor(1.5),
        ..Default::default()
    };
    let window_entity = world.spawn(window.clone()).id();
    let camera = world.spawn_empty().id();
    let canvas = WorldCanvas::new(320, 180).unwrap();
    let packet = canvas_packet(&window, camera, canvas);
    world.insert_resource(packet.source_generation().runtime_generation().unwrap());
    world.insert_resource(canvas);
    let (presented, guard) = prepare_world_presentation(&packet).unwrap();
    let rect = packet.world_canvas().unwrap().mapping().content_rect();
    let center = Vec2::new(
        (rect.min[0] + rect.max[0]) * 0.5,
        (rect.min[1] + rect.max[1]) * 0.5,
    );
    world.insert_resource(presented);
    let mut parameter = SystemState::<WorldPointer>::new(&mut world);
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, center)
            .is_none()
    );
    guard.mark_presented();
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, center)
            .unwrap()
            .abs_diff_eq(Vec2::ZERO, 0.001)
    );
    for point in [
        Vec2::new(rect.max[0], center.y),
        Vec2::new(center.x, rect.max[1]),
        Vec2::new(center.x, 0.0),
        Vec2::new(f32::NAN, center.y),
    ] {
        assert!(
            parameter
                .get(&world)
                .unwrap()
                .window_to_world(camera, point)
                .is_none()
        );
    }
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(Entity::from_raw_u32(99).unwrap(), center)
            .is_none()
    );

    // No extraction or render drive occurs between the real geometry change and the query.
    world
        .get_mut::<Window>(window_entity)
        .unwrap()
        .resolution
        .physical_width = 1_001;
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, center)
            .is_none()
    );
    world.get_mut::<Window>(window_entity).unwrap().resolution = window.resolution;
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, center)
            .is_none()
    );
    guard.mark_presented();
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, center)
            .is_none()
    );
    drop(guard);
    let guard = publish(&mut world, &packet);
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, center)
            .is_some()
    );
    world
        .get_mut::<Window>(window_entity)
        .unwrap()
        .resolution
        .scale_factor = 2.0;
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, center)
            .is_none()
    );
    world.get_mut::<Window>(window_entity).unwrap().resolution = window.resolution;
    drop(guard);
    let guard = publish(&mut world, &packet);
    world.insert_resource(WorldCanvas::new(160, 90).unwrap());
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, center)
            .is_none()
    );
    world.insert_resource(canvas);
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, center)
            .is_none()
    );
    drop(guard);
    let guard = publish(&mut world, &packet);
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, center)
            .is_some()
    );
    drop(guard);
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, center)
            .is_none()
    );
}

#[test]
fn presented_pointer_rejects_other_viewports_and_runtime_generations() {
    let mut world = World::new();
    let window = Window::default();
    world.spawn(window.clone());
    let camera = world.spawn_empty().id();
    let canvas = WorldCanvas::new(320, 180).unwrap();
    let packet = canvas_packet_with_viewport(
        &window,
        camera,
        canvas,
        ViewportRect::new(0, 0, 160, 180).unwrap(),
    );
    let generation = packet.source_generation().runtime_generation().unwrap();
    world.insert_resource(generation);
    world.insert_resource(canvas);
    let (presented, guard) = prepare_world_presentation(&packet).unwrap();
    guard.mark_presented();
    world.insert_resource(presented);
    let mut parameter = SystemState::<WorldPointer>::new(&mut world);
    let rect = packet.world_canvas().unwrap().mapping().content_rect();
    let y = (rect.min[1] + rect.max[1]) * 0.5;
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, Vec2::new(rect.min[0] + 1.0, y))
            .is_some()
    );
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, Vec2::new((rect.min[0] + rect.max[0]) * 0.5, y))
            .is_none()
    );
    world.insert_resource(test_runtime_generation());
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, Vec2::new(rect.min[0] + 1.0, y))
            .is_none()
    );
}

#[test]
fn extraction_uses_canvas_extent_without_changing_native_window() {
    let mut app = render_app(true);
    let canvas = WorldCanvas::new(320, 180).unwrap();
    app.insert_resource(canvas).unwrap();
    app.world_mut().unwrap().spawn((
        Camera2d {
            viewport_height: 180.0,
            ..Default::default()
        },
        Transform2d::default(),
    ));
    let mut query = app.world_mut().unwrap().query::<&Window>();
    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    let view = runtime.world().resource::<ExtractedViews>().as_slice()[0];
    assert_eq!(view.viewport.extent(), canvas.extent());
    assert!(
        view.viewport_to_world(Vec2::new(160.0, 90.0))
            .unwrap()
            .abs_diff_eq(Vec2::ZERO, 0.001)
    );
    assert_eq!(
        query.single(runtime.world()).unwrap().resolution,
        nara_window::WindowResolution::default()
    );
    retire_runtime(runtime);
}

#[test]
fn canvas_constructor_rejects_empty_extents_and_nonfinite_bars() {
    assert!(WorldCanvas::new(0, 180).is_none());
    assert!(WorldCanvas::new(320, 0).is_none());
    assert!(
        WorldCanvas::new(320, 180)
            .unwrap()
            .with_bar_color(Color::rgb(f32::NAN, 0.0, 0.0))
            .is_none()
    );
    assert!(
        WorldCanvas::new(320, 180)
            .unwrap()
            .with_bar_color(Color::rgba(0.0, 0.0, 0.0, f32::INFINITY))
            .is_none()
    );
}

#[test]
fn native_world_pointer_uses_the_presented_camera_without_canvas_configuration() {
    let mut world = World::new();
    let window = Window {
        resolution: nara_window::WindowResolution::new(800, 600).with_scale_factor(2.0),
        ..Default::default()
    };
    world.spawn(window.clone());
    let camera = world.spawn_empty().id();
    let viewport = ViewportRect::from_window_resolution(window.resolution).unwrap();
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        camera_entity: camera,
        target: RenderTarget::PrimaryWindow,
        viewport,
        mapping: ViewMapping2d::orthographic(
            nara_core::Mat3::from_translation(Vec2::new(12.0, 24.0)),
            600.0,
            viewport,
        )
        .unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(Color::BLACK),
    });
    let packet = build_render_frame_packet(
        test_runtime_generation(),
        1,
        Some(window.id),
        [&window],
        &views,
    )
    .unwrap()
    .unwrap();
    assert!(packet.world_canvas().is_none());
    world.insert_resource(packet.source_generation().runtime_generation().unwrap());
    let (presented, guard) = prepare_world_presentation(&packet).unwrap();
    guard.mark_presented();
    world.insert_resource(presented);
    let mut parameter = SystemState::<WorldPointer>::new(&mut world);
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, Vec2::new(200.0, 150.0))
            .unwrap()
            .abs_diff_eq(Vec2::new(12.0, 24.0), 0.001)
    );
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, Vec2::new(400.0, 150.0))
            .is_none()
    );
    guard.invalidate();
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, Vec2::new(200.0, 150.0))
            .is_none()
    );
}

#[test]
fn default_render_plugin_preserves_a_directly_configured_world_canvas() {
    let mut app = App::new();
    app.add_plugins((
        ComponentRegistryPlugin,
        HierarchyPlugin,
        TransformPlugin,
        RenderComponentsPlugin,
    ))
    .unwrap();
    let canvas = WorldCanvas::new(320, 180).unwrap();
    app.insert_resource(canvas).unwrap();
    app.add_plugin(RenderPlugin::default()).unwrap();
    assert_eq!(
        app.world().get_resource::<WorldCanvas>().copied(),
        Some(canvas)
    );
}

#[test]
fn capture_retains_the_configuration_that_produced_the_views() {
    fn change_canvas_after_extraction(mut canvas: ResMut<WorldCanvas>) {
        *canvas = WorldCanvas::new(640, 360).unwrap();
    }

    let mut app = render_app(true);
    let original = WorldCanvas::new(320, 180).unwrap();
    app.insert_resource(original).unwrap();
    app.world_mut()
        .unwrap()
        .spawn((Camera2d::default(), Transform2d::default()));
    app.add_systems(
        CoreStage::Extract,
        change_canvas_after_extraction.after(__private::RenderExtractSet::Views),
    )
    .unwrap();
    let mut windows = app.world_mut().unwrap().query::<&Window>();
    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    let views = runtime.world().resource::<ExtractedViews>();
    assert_eq!(
        runtime.world().resource::<WorldCanvas>().extent(),
        Extent2d::new(640, 360).unwrap()
    );
    assert_eq!(views.world_canvas(), Some(original));
    let packet = build_render_frame_packet_with_world_canvas(
        runtime.generation(),
        1,
        Some(WindowId::PRIMARY),
        windows.iter(runtime.world()),
        views,
        views.world_canvas(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(packet.world_canvas().unwrap().extent(), original.extent());
    assert_eq!(packet.views()[0].viewport.extent(), original.extent());
    let mut cleared = views.clone();
    cleared.clear();
    assert!(cleared.world_canvas().is_none());
    assert!(cleared.is_empty());
    retire_runtime(runtime);
}

#[test]
fn window_event_batches_revoke_even_when_final_geometry_matches_the_presented_frame() {
    use nara_ecs::bevy_ecs::system::RunSystemOnce;
    use nara_window::{WindowEvent, WindowEvents};

    let mut world = World::new();
    let window = Window::default();
    world.spawn(window.clone());
    let camera = world.spawn_empty().id();
    let canvas = WorldCanvas::new(320, 180).unwrap();
    let packet = canvas_packet(&window, camera, canvas);
    world.insert_resource(packet.source_generation().runtime_generation().unwrap());
    world.insert_resource(canvas);
    let guard = publish(&mut world, &packet);
    let mut events = WindowEvents::default();
    events.push(WindowEvent::Resized {
        window_id: window.id,
        resolution: nara_window::WindowResolution::new(0, 0),
    });
    events.push(WindowEvent::Resized {
        window_id: window.id,
        resolution: window.resolution,
    });
    world.insert_resource(events);
    world
        .run_system_once(crate::world_pointer::invalidate_window_presentation)
        .unwrap();
    let mut parameter = SystemState::<WorldPointer>::new(&mut world);
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, Vec2::new(640.0, 360.0))
            .is_none()
    );
    guard.mark_presented();
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, Vec2::new(640.0, 360.0))
            .is_none()
    );
}

#[test]
fn callback_revocation_before_activation_cannot_be_undone_by_late_present() {
    let mut world = World::new();
    let window = Window::default();
    world.spawn(window.clone());
    let camera = world.spawn_empty().id();
    let canvas = WorldCanvas::new(320, 180).unwrap();
    let packet = canvas_packet(&window, camera, canvas);
    world.insert_resource(packet.source_generation().runtime_generation().unwrap());
    world.insert_resource(canvas);
    let (presented, guard) = prepare_world_presentation(&packet).unwrap();
    let revoker = guard.revoker();
    std::thread::spawn(move || revoker.invalidate())
        .join()
        .unwrap();
    guard.mark_presented();
    world.insert_resource(presented);
    let mut parameter = SystemState::<WorldPointer>::new(&mut world);
    assert!(
        parameter
            .get(&world)
            .unwrap()
            .window_to_world(camera, Vec2::new(640.0, 360.0))
            .is_none()
    );
}

#[test]
fn zero_window_skips_explicit_viewports_and_requires_fresh_presentation_after_restore() {
    let mut world = World::new();
    let window = Window::default();
    let window_entity = world.spawn(window.clone()).id();
    let camera = world.spawn_empty().id();
    for canvas in [None, WorldCanvas::new(320, 180)] {
        let viewport = ViewportRect::new(0, 0, 320, 180).unwrap();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            camera_entity: camera,
            target: RenderTarget::PrimaryWindow,
            viewport,
            mapping: ViewMapping2d::orthographic(nara_core::Mat3::IDENTITY, 180.0, viewport)
                .unwrap(),
            order: 0,
            load: ViewLoadOp::Clear(Color::BLACK),
        });
        let generation = test_runtime_generation();
        world.insert_resource(generation);
        if let Some(canvas) = canvas {
            world.insert_resource(canvas);
        }
        let packet = build_render_frame_packet_with_world_canvas(
            generation,
            1,
            Some(window.id),
            [&window],
            &views,
            canvas,
        )
        .unwrap()
        .unwrap();
        let guard = publish(&mut world, &packet);
        let point = if canvas.is_some() {
            Vec2::new(640.0, 360.0)
        } else {
            Vec2::new(160.0, 90.0)
        };
        let mut parameter = SystemState::<WorldPointer>::new(&mut world);
        assert!(
            parameter
                .get(&world)
                .unwrap()
                .window_to_world(camera, point)
                .is_some()
        );
        let suspended = Window {
            resolution: nara_window::WindowResolution::new(0, 0),
            ..window.clone()
        };
        world.get_mut::<Window>(window_entity).unwrap().resolution = suspended.resolution;
        assert!(
            build_render_frame_packet_with_world_canvas(
                generation,
                2,
                Some(window.id),
                [&suspended],
                &views,
                canvas,
            )
            .unwrap()
            .is_none(),
            "a zero native target is a skip, even with an explicit viewport"
        );
        assert!(
            parameter
                .get(&world)
                .unwrap()
                .window_to_world(camera, point)
                .is_none()
        );
        world.get_mut::<Window>(window_entity).unwrap().resolution = window.resolution;
        assert!(
            parameter
                .get(&world)
                .unwrap()
                .window_to_world(camera, point)
                .is_none()
        );
        let restored = build_render_frame_packet_with_world_canvas(
            generation,
            3,
            Some(window.id),
            [&window],
            &views,
            canvas,
        )
        .unwrap()
        .unwrap();
        drop(guard);
        let _restored_guard = publish(&mut world, &restored);
        assert!(
            parameter
                .get(&world)
                .unwrap()
                .window_to_world(camera, point)
                .is_some()
        );
    }
}
