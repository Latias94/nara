use std::time::Duration;

use nara_app::{
    FixedTime, FixedUpdateSet, RuntimeControl, RuntimeControlRequestResult, RuntimeInstance,
};
use nara_core::Vec2;
use nara_ecs::{Commands, bevy_ecs::entity_disabling::Disabled};
use nara_scene::spatial::{ChildOf, SpatialPlugin, Transform2d, Visibility};
use nara_window::{WindowPlugin, WindowResolution};

use super::*;
use crate::{
    MAX_VIEWS_PER_TARGET, RenderFramePacket, RenderFramePacketError, RenderFrameTarget,
    RenderImage2d, RenderImagePacket, RenderTarget, build_image_render_frame_packet,
    build_render_frame_packet, build_render_frame_packet_with_world_canvas,
};

fn app(window: bool) -> App {
    let mut app = App::new();
    app.add_plugin(SpatialPlugin).unwrap();
    if window {
        app.add_plugin(WindowPlugin::default()).unwrap();
    }
    app.add_plugin(RenderPlugin::default()).unwrap();
    app
}

fn views(runtime: &RuntimeInstance) -> &ExtractedViews {
    runtime.world().resource::<ExtractedViews>()
}

fn edit(runtime: &mut RuntimeInstance, edit: impl FnOnce(&mut World)) {
    let outcome = runtime
        .with_driver_scope(|scope| scope.with_world_mut(edit))
        .unwrap();
    assert!(outcome.post_operation_fault.is_none());
}

fn extract(runtime: &mut RuntimeInstance) {
    runtime.drive(Duration::ZERO).unwrap();
}

fn packet(runtime: &RuntimeInstance) -> RenderFramePacket {
    let views = views(runtime);
    let time = views.captured_time().unwrap();
    build_render_frame_packet_with_world_canvas(
        runtime.generation(),
        time.real().frame,
        None,
        [&Window::default()],
        views,
        views.world_canvas(),
    )
    .unwrap()
    .unwrap()
}

#[test]
fn camera_extraction_orders_views_and_clears_each_target_once() {
    let mut app = app(true);
    app.insert_resource(ClearColor(Color::WHITE)).unwrap();
    let world = app.world_mut().unwrap();
    let right = world
        .spawn((
            Camera2d {
                target: RenderTarget::Window(WindowId::PRIMARY),
                viewport: ViewportRect::new(640, 0, 640, 720),
                order: 4,
                ..Camera2d::default()
            },
            Transform2d::IDENTITY,
        ))
        .id();
    let left = world
        .spawn((
            Camera2d {
                viewport: ViewportRect::new(0, 0, 640, 720),
                ..Camera2d::default()
            },
            Transform2d::IDENTITY,
        ))
        .id();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    extract(&mut runtime);
    let frame = packet(&runtime);
    let [first, second] = frame.views() else {
        panic!("expected two views")
    };
    assert_eq!((first.camera_entity, second.camera_entity), (left, right));
    assert_eq!(
        (first.load, second.load),
        (ViewLoadOp::Clear(Color::WHITE), ViewLoadOp::Load)
    );
    assert_eq!(second.viewport.physical_x, 640);
    assert_eq!(frame.logical_target(), RenderTarget::PrimaryWindow);
    runtime.finish().unwrap();
}

#[test]
fn deferred_parented_camera_uses_completed_affine_projection() {
    let mut app = app(true);
    app.add_systems(CoreStage::PostUpdate, |mut commands: Commands| {
        let parent = commands
            .spawn(Transform2d {
                translation: Vec2::new(100.0, 40.0),
                scale: Vec2::new(-2.0, 3.0),
                ..Transform2d::IDENTITY
            })
            .id();
        commands.spawn((
            Camera2d {
                viewport: ViewportRect::new(40, 20, 320, 180),
                viewport_height: 180.0,
                ..Camera2d::default()
            },
            Transform2d {
                translation: Vec2::new(10.0, -5.0),
                scale: Vec2::new(0.5, 2.0),
                ..Transform2d::IDENTITY
            },
            ChildOf(parent),
        ));
    })
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    extract(&mut runtime);
    let [view] = views(&runtime).as_slice() else {
        panic!("expected one view")
    };
    assert!(
        view.mapping
            .clip_to_world(Vec2::ZERO)
            .abs_diff_eq(Vec2::new(80.0, 25.0), 1e-4)
    );
    let world_point = Vec2::new(0.0, 295.0);
    let physical = view.world_to_viewport(world_point);
    assert!(physical.abs_diff_eq(Vec2::new(280.0, 65.0), 1e-4));
    assert!(
        view.viewport_to_world(physical)
            .unwrap()
            .abs_diff_eq(world_point, 1e-4)
    );
    runtime.finish().unwrap();
}

#[test]
fn camera_identity_survives_visibility_and_resize_but_changes_on_reinsertion() {
    let mut app = app(true);
    let world = app.world_mut().unwrap();
    let window = world
        .query_filtered::<Entity, nara_ecs::With<Window>>()
        .single(world)
        .unwrap();
    let parent = world.spawn(Visibility::Inherited).id();
    let camera = world
        .spawn((Camera2d::default(), Transform2d::IDENTITY, ChildOf(parent)))
        .id();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    extract(&mut runtime);
    let original = views(&runtime).as_slice()[0].id;

    edit(&mut runtime, |world| {
        world.entity_mut(parent).insert(Visibility::Hidden);
    });
    extract(&mut runtime);
    assert!(views(&runtime).is_empty());
    edit(&mut runtime, |world| {
        world
            .entity_mut(camera)
            .insert((Visibility::Visible, Disabled));
        world.get_mut::<Window>(window).unwrap().resolution = WindowResolution::new(960, 540);
    });
    extract(&mut runtime);
    assert!(views(&runtime).is_empty());
    edit(&mut runtime, |world| {
        world.entity_mut(camera).remove::<Disabled>();
    });
    extract(&mut runtime);
    let view = views(&runtime).as_slice()[0];
    assert_eq!(view.id, original);
    assert_eq!(view.viewport, ViewportRect::new(0, 0, 960, 540).unwrap());

    edit(&mut runtime, |world| {
        world
            .entity_mut(camera)
            .remove::<Camera2d>()
            .insert(Camera2d::default());
    });
    extract(&mut runtime);
    assert_ne!(views(&runtime).as_slice()[0].id, original);
    runtime.finish().unwrap();
}

#[test]
fn canvas_capture_uses_the_raster_that_produced_the_views() {
    let mut app = app(true);
    let canvas = WorldCanvas::new(320, 180).unwrap();
    app.insert_resource(canvas).unwrap();
    let world = app.world_mut().unwrap();
    let window = world
        .query_filtered::<Entity, nara_ecs::With<Window>>()
        .single(world)
        .unwrap();
    world.spawn((
        Camera2d {
            viewport: ViewportRect::new(200, 100, 200, 120),
            ..Camera2d::default()
        },
        Transform2d::IDENTITY,
    ));
    app.add_systems(
        CoreStage::Extract,
        (|mut canvas: ResMut<WorldCanvas>| {
            *canvas = WorldCanvas::new(640, 360).unwrap();
        })
        .after(RenderExtractSet::Views),
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    extract(&mut runtime);
    let frame = packet(&runtime);
    assert_eq!(frame.world_canvas().unwrap().extent(), canvas.extent());
    assert_eq!(
        frame.views()[0].viewport,
        ViewportRect::new(200, 100, 120, 80).unwrap()
    );
    assert_eq!(frame.target().extent(), Extent2d::new(1280, 720).unwrap());
    assert_ne!(*runtime.world().resource::<WorldCanvas>(), canvas);

    edit(&mut runtime, |world| {
        world.get_mut::<Window>(window).unwrap().resolution = WindowResolution::new(0, 0);
    });
    extract(&mut runtime);
    assert!(views(&runtime).is_empty());
    runtime.finish().unwrap();
}

#[test]
fn captured_time_and_pose_survive_exact_step_and_image_adaptation() {
    let mut app = app(true);
    app.insert_resource(RuntimeTimeSettings::default().with_time_scale(0.5).unwrap())
        .unwrap();
    app.insert_resource(WorldCanvas::new(320, 180).unwrap())
        .unwrap();
    app.world_mut()
        .unwrap()
        .spawn((Camera2d::default(), Transform2d::IDENTITY));
    app.add_systems(
        CoreStage::FixedUpdate,
        (|mut camera: Query<&mut Transform2d>| {
            camera.single_mut().unwrap().translation.x += 1.0;
        })
        .in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let step = FixedTime::DEFAULT_TIMESTEP;
    runtime.drive(step * 5).unwrap();
    let first = packet(&runtime);
    let first_time = first.captured_time().unwrap();
    assert_eq!(first_time.real().delta, step * 5);
    assert_eq!(first_time.fixed().tick(), 2);
    assert!((first_time.render().interpolation_alpha() - 0.5).abs() < 1e-5);

    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    extract(&mut runtime);
    let paused = packet(&runtime);
    assert!(matches!(
        runtime.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Accepted(_)
    ));
    extract(&mut runtime);
    assert_eq!(packet(&runtime), paused);
    assert!(runtime.world().resource::<RealTime>().frame > paused.frame_index());

    let image = RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        paused.target().extent(),
    )
    .unwrap();
    let adapted = paused.with_image_target(image).unwrap();
    assert_eq!(adapted.target(), RenderFrameTarget::Image(image));
    assert_eq!(adapted.logical_target(), paused.logical_target());
    assert_eq!(adapted.frame_index(), paused.frame_index());
    assert_eq!(adapted.captured_time(), paused.captured_time());
    assert_eq!(adapted.world_canvas(), paused.world_canvas());
    assert_eq!(adapted.views(), paused.views());

    extract(&mut runtime);
    let next = packet(&runtime);
    assert_eq!(next.captured_time().unwrap().fixed().tick(), 3);
    for (frame, position) in [(next, 3.0), (first, 2.0)] {
        assert!(
            frame.views()[0]
                .mapping
                .clip_to_world(Vec2::ZERO)
                .abs_diff_eq(Vec2::new(position, 0.0), 1e-5)
        );
    }
    runtime.finish().unwrap();
}

#[test]
fn invalid_camera_extraction_preserves_the_previous_views_and_clock() {
    let mut app = app(true);
    let camera = app
        .world_mut()
        .unwrap()
        .spawn((Camera2d::default(), Transform2d::IDENTITY))
        .id();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    extract(&mut runtime);
    let previous = views(&runtime).clone();
    edit(&mut runtime, |world| {
        world.get_mut::<Transform2d>(camera).unwrap().scale = Vec2::ZERO;
    });
    assert!(runtime.drive(Duration::ZERO).is_err());
    assert_eq!(views(&runtime), &previous);
    runtime.finish().unwrap();
}

#[test]
fn non_finite_camera_settings_preserve_the_previous_capture() {
    let invalid_cameras = [
        Camera2d {
            viewport_height: f32::INFINITY,
            ..Camera2d::default()
        },
        Camera2d {
            clear_color: Some(Color::rgba(0.0, f32::NAN, 0.0, 1.0)),
            ..Camera2d::default()
        },
    ];
    for invalid in invalid_cameras {
        let mut app = app(true);
        let camera = app
            .world_mut()
            .unwrap()
            .spawn((Camera2d::default(), Transform2d::IDENTITY))
            .id();
        let mut runtime = RuntimeInstance::start(app).unwrap();
        extract(&mut runtime);
        let previous = views(&runtime).clone();
        edit(&mut runtime, |world| {
            *world.get_mut::<Camera2d>(camera).unwrap() = invalid;
        });
        assert!(runtime.drive(Duration::ZERO).is_err());
        assert_eq!(views(&runtime), &previous);
        runtime.finish().unwrap();
    }
}

#[test]
fn offscreen_extraction_uses_explicit_viewport_and_target() {
    let mut app = app(false);
    let image = RenderImage2d::allocate_handle().unwrap();
    let extent = Extent2d::new(320, 180).unwrap();
    let camera = app
        .world_mut()
        .unwrap()
        .spawn((
            Camera2d {
                target: RenderTarget::Image(image),
                viewport: ViewportRect::from_extent(extent),
                clear_color: Some(Color::WHITE),
                ..Camera2d::default()
            },
            Transform2d::IDENTITY,
        ))
        .id();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    extract(&mut runtime);
    let views = views(&runtime);
    let image = RenderImagePacket::new(image, extent).unwrap();
    let frame = build_image_render_frame_packet(runtime.generation().into(), 1, views, image)
        .unwrap()
        .unwrap();
    assert_eq!(frame.target(), RenderFrameTarget::Image(image));
    assert_eq!(frame.views()[0].camera_entity, camera);
    assert_eq!(frame.views()[0].load, ViewLoadOp::Clear(Color::WHITE));
    assert_eq!(frame.views()[0].viewport.extent(), extent);
    assert_eq!(frame.captured_time(), views.captured_time());
    runtime.finish().unwrap();
}

#[test]
fn packet_rejects_invalid_view_lists() {
    let runtime = RuntimeInstance::start(App::new()).unwrap();
    let window = Window::default();
    let viewport = ViewportRect::from_window_resolution(window.resolution).unwrap();
    let first = ExtractedView {
        id: RenderViewId::try_acquire().unwrap(),
        camera_entity: Entity::PLACEHOLDER,
        target: RenderTarget::PrimaryWindow,
        viewport,
        mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 720.0, viewport).unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(Color::BLACK),
    };
    let later = ExtractedView {
        id: first.id.subview(0),
        order: 1,
        ..first
    };
    let other_window = WindowId::new(2);
    let cases = [
        (
            vec![ExtractedView {
                load: ViewLoadOp::Load,
                ..first
            }],
            RenderFramePacketError::UninitializedTarget,
        ),
        (
            vec![first, later, first],
            RenderFramePacketError::DuplicateViewId { id: first.id },
        ),
        (vec![later, first], RenderFramePacketError::UnorderedViews),
        (
            vec![
                first,
                ExtractedView {
                    target: RenderTarget::Window(other_window),
                    ..later
                },
            ],
            RenderFramePacketError::MixedRenderTargets,
        ),
        (
            vec![ExtractedView {
                target: RenderTarget::Window(other_window),
                ..first
            }],
            RenderFramePacketError::TargetWindowMismatch {
                window_id: other_window,
            },
        ),
        (
            vec![ExtractedView {
                viewport: ViewportRect::new(1, 0, 1280, 720).unwrap(),
                ..first
            }],
            RenderFramePacketError::ViewportOutsideTarget,
        ),
        (
            vec![first; MAX_VIEWS_PER_TARGET + 1],
            RenderFramePacketError::TooManyViews {
                actual: MAX_VIEWS_PER_TARGET + 1,
                maximum: MAX_VIEWS_PER_TARGET,
            },
        ),
    ];
    for (manual, error) in cases {
        let mut views = ExtractedViews::default();
        for view in manual {
            views.push(view);
        }
        assert_eq!(
            build_render_frame_packet(runtime.generation(), 1, None, [&window], &views),
            Err(error)
        );
    }
    runtime.finish().unwrap();
}
