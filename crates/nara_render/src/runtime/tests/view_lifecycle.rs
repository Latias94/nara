use super::*;
use crate::frame::build_render_frame_packet_for_source;
use nara_app::{RuntimeControl, RuntimeControlRequestResult};
use nara_ecs::{IntoSystem, bevy_ecs::system::System};
use nara_scene::spatial::Visibility;

fn edit_world(runtime: &mut RuntimeInstance, edit: impl FnOnce(&mut World) + Send + 'static) {
    let outcome = runtime.apply_command(edit).unwrap();
    assert!(outcome.post_operation_fault.is_none());
}

fn camera_view(runtime: &RuntimeInstance, camera: Entity) -> ExtractedView {
    *runtime
        .world()
        .resource::<ExtractedViews>()
        .as_slice()
        .iter()
        .find(|view| view.camera_entity == camera)
        .expect("the camera must have an extracted View")
}

#[test]
fn hidden_camera_recovers_its_view_identity_after_a_visibility_override() {
    let mut app = render_app(true);
    let world = app.world_mut().unwrap();
    let parent = world.spawn(Visibility::Inherited).id();
    let camera = world
        .spawn((Camera2d::default(), Transform2d::IDENTITY, ChildOf(parent)))
        .id();
    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    let id = camera_view(&runtime, camera).id;

    edit_world(&mut runtime, move |world| {
        world.entity_mut(parent).insert(Visibility::Hidden);
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<ExtractedViews>().is_empty());

    edit_world(&mut runtime, move |world| {
        world.entity_mut(camera).insert(Visibility::Visible);
    });
    runtime.drive(Duration::ZERO).unwrap();
    let restored = camera_view(&runtime, camera);
    assert_eq!(restored.id, id);
    assert_eq!(restored.load, ViewLoadOp::Clear(Color::BLACK));
    runtime.finish().unwrap();
}

#[test]
fn camera_edits_order_resize_target_and_pause_preserve_view_identity() {
    let mut app = render_app(true);
    let world = app.world_mut().unwrap();
    let camera = world
        .spawn((Camera2d::default(), Transform2d::default()))
        .id();
    let other = world
        .spawn((
            Camera2d {
                order: 10,
                ..Default::default()
            },
            Transform2d::default(),
        ))
        .id();
    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    let original = camera_view(&runtime, camera);
    let other_id = camera_view(&runtime, other).id;
    assert_ne!(original.id, other_id);

    edit_world(&mut runtime, move |world| {
        let mut config = world.get_mut::<Camera2d>(camera).unwrap();
        config.order = 20;
        config.viewport_height = 360.0;
        config.clear_color = Some(Color::WHITE);
        world.get_mut::<Transform2d>(camera).unwrap().translation = Vec2::new(12.0, 24.0);
        for mut window in world.query::<&mut Window>().iter_mut(world) {
            window.resolution = nara_window::WindowResolution::new(960, 540);
        }
    });
    runtime.drive(Duration::ZERO).unwrap();
    let edited = camera_view(&runtime, camera);
    assert_eq!(edited.id, original.id);
    assert_eq!(camera_view(&runtime, other).id, other_id);
    assert_eq!(edited.viewport, ViewportRect::new(0, 0, 960, 540).unwrap());
    assert_ne!(edited.mapping, original.mapping);
    assert_eq!(edited.load, ViewLoadOp::Clear(Color::WHITE));
    assert_eq!(
        runtime
            .world()
            .resource::<ExtractedViews>()
            .as_slice()
            .iter()
            .map(|view| view.camera_entity)
            .collect::<Vec<_>>(),
        [other, camera]
    );

    let image = RenderImage2d::allocate_handle().unwrap();
    edit_world(&mut runtime, move |world| {
        // Replacing an existing component value is an edit, not remove/readd.
        world.entity_mut(camera).insert(Camera2d {
            target: RenderTarget::Image(image),
            viewport: Some(ViewportRect::new(0, 0, 320, 180).unwrap()),
            ..Default::default()
        });
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(camera_view(&runtime, camera).id, original.id);
    assert_eq!(
        camera_view(&runtime, camera).target,
        RenderTarget::Image(image)
    );

    for control in [RuntimeControl::Pause, RuntimeControl::Resume] {
        assert!(matches!(
            runtime.request_control(control),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(camera_view(&runtime, camera).id, original.id);
        assert_eq!(camera_view(&runtime, other).id, other_id);
    }
    runtime.finish().unwrap();
}

#[test]
fn split_viewports_follow_the_window_extent_and_recover_their_original_layout() {
    let mut app = render_app(true);
    let world = app.world_mut().unwrap();
    let authored_right = ViewportRect::new(640, 0, 640, 720).unwrap();
    let right = world
        .spawn((
            Camera2d {
                viewport: Some(authored_right),
                ..Default::default()
            },
            Transform2d::default(),
        ))
        .id();
    let left = world
        .spawn((
            Camera2d {
                viewport: ViewportRect::new(0, 0, 640, 720),
                order: 1,
                ..Default::default()
            },
            Transform2d::default(),
        ))
        .id();
    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    let original_right = camera_view(&runtime, right);
    let original_left = camera_view(&runtime, left);

    for (width, visible_right) in [(960, 320), (640, 0), (480, 0), (1280, 640)] {
        let resolution = nara_window::WindowResolution::new(width, 720);
        edit_world(&mut runtime, move |world| {
            for mut window in world.query::<&mut Window>().iter_mut(world) {
                window.resolution = resolution;
            }
        });
        runtime.drive(Duration::ZERO).unwrap();
        let world = runtime.world();
        let window = Window {
            resolution,
            ..Window::default()
        };
        let packet = build_render_frame_packet(
            runtime.generation(),
            world.resource::<RenderFrame>().index,
            Some(window.id),
            [&window],
            world.resource::<ExtractedViews>(),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            world.get::<Camera2d>(right).unwrap().viewport,
            Some(authored_right)
        );
        let left = camera_view(&runtime, left);
        assert_eq!(left.id, original_left.id);
        assert_eq!(
            left.viewport,
            ViewportRect::new(0, 0, width.min(640), 720).unwrap()
        );
        if visible_right == 0 {
            assert_eq!(packet.views(), &[left]);
            assert_eq!(left.load, ViewLoadOp::Clear(Color::BLACK));
        } else {
            let right = camera_view(&runtime, right);
            assert_eq!(packet.views(), &[right, left]);
            assert_eq!(right.id, original_right.id);
            assert_eq!(right.load, ViewLoadOp::Clear(Color::BLACK));
            assert_eq!(left.load, ViewLoadOp::Load);
            assert_eq!(
                right.viewport,
                ViewportRect::new(640, 0, visible_right, 720).unwrap()
            );
            let center = Vec2::new(640.0 + visible_right as f32 * 0.5, 360.0);
            assert_eq!(right.world_to_viewport(Vec2::ZERO), center);
            assert_eq!(right.viewport_to_world(center), Some(Vec2::ZERO));
            assert!(
                right
                    .world_to_viewport(Vec2::new(50.0, 30.0))
                    .abs_diff_eq(center + Vec2::new(50.0, -30.0), 1.0e-4)
            );
        }
    }

    assert_eq!(camera_view(&runtime, right), original_right);
    assert_eq!(camera_view(&runtime, left), original_left);
    runtime.finish().unwrap();
}

#[test]
fn explicit_world_canvas_viewports_are_clipped_to_the_raster_and_skip_a_zero_window() {
    let mut app = render_app(true);
    let world = app.world_mut().unwrap();
    let canvas = WorldCanvas::new(200, 100).unwrap();
    world.insert_resource(canvas);
    let authored = ViewportRect::new(120, 30, u32::MAX, u32::MAX).unwrap();
    let camera = world
        .spawn((
            Camera2d {
                viewport: Some(authored),
                ..Default::default()
            },
            Transform2d::default(),
        ))
        .id();
    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    let original = camera_view(&runtime, camera);
    assert_eq!(
        original.viewport,
        ViewportRect::new(120, 30, 80, 70).unwrap()
    );

    for (width, height) in [(64, 32), (0, 32), (350, 210)] {
        let resolution = nara_window::WindowResolution::new(width, height);
        edit_world(&mut runtime, move |world| {
            for mut window in world.query::<&mut Window>().iter_mut(world) {
                window.resolution = resolution;
            }
        });
        runtime.drive(Duration::ZERO).unwrap();
        let world = runtime.world();
        let window = Window {
            resolution,
            ..Window::default()
        };
        let packet = build_render_frame_packet_with_world_canvas(
            runtime.generation(),
            world.resource::<RenderFrame>().index,
            Some(window.id),
            [&window],
            world.resource::<ExtractedViews>(),
            Some(canvas),
        )
        .unwrap();
        assert_eq!(
            world.get::<Camera2d>(camera).unwrap().viewport,
            Some(authored)
        );
        if width == 0 {
            assert!(packet.is_none());
            assert!(world.resource::<ExtractedViews>().is_empty());
        } else {
            assert_eq!(packet.unwrap().views(), &[original]);
        }
    }
    runtime.finish().unwrap();
}

#[test]
fn missing_zero_and_filtered_targets_preserve_the_camera_lifetime() {
    let mut app = render_app(false);
    let camera = app
        .world_mut()
        .unwrap()
        .spawn((Camera2d::default(), Transform2d::default()))
        .id();
    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<ExtractedViews>().is_empty());
    let id = runtime.world().resource::<CameraViewLifetimes>().ids[&camera];

    edit_world(&mut runtime, move |world| {
        world.get_mut::<Camera2d>(camera).unwrap().viewport = ViewportRect::new(0, 0, 640, 360);
    });
    runtime.drive(Duration::ZERO).unwrap();
    let explicit = camera_view(&runtime, camera);
    assert_eq!(explicit.id, id);
    assert_eq!(
        explicit.viewport,
        ViewportRect::new(0, 0, 640, 360).unwrap()
    );

    edit_world(&mut runtime, |world| {
        world.spawn(Window::default());
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(camera_view(&runtime, camera), explicit);

    edit_world(&mut runtime, |world| {
        for mut window in world.query::<&mut Window>().iter_mut(world) {
            window.resolution = nara_window::WindowResolution::new(0, 0);
        }
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<ExtractedViews>().is_empty());
    assert_eq!(
        runtime.world().resource::<CameraViewLifetimes>().ids[&camera],
        id
    );

    edit_world(&mut runtime, move |world| {
        world.get_mut::<Camera2d>(camera).unwrap().viewport_height = 180.0;
        for mut window in world.query::<&mut Window>().iter_mut(world) {
            window.resolution = nara_window::WindowResolution::new(640, 360);
        }
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(camera_view(&runtime, camera).id, id);
    assert_eq!(
        camera_view(&runtime, camera).viewport,
        ViewportRect::new(0, 0, 640, 360).unwrap()
    );

    let image = RenderImage2d::allocate_handle().unwrap();
    for target in [
        RenderTarget::Window(WindowId::new(99)),
        RenderTarget::Image(image),
    ] {
        edit_world(&mut runtime, move |world| {
            let mut camera = world.get_mut::<Camera2d>(camera).unwrap();
            camera.target = target;
            camera.viewport = None;
        });
        runtime.drive(Duration::ZERO).unwrap();
        assert!(runtime.world().resource::<ExtractedViews>().is_empty());
        assert_eq!(
            runtime.world().resource::<CameraViewLifetimes>().ids[&camera],
            id
        );
    }
    edit_world(&mut runtime, move |world| {
        world.get_mut::<Camera2d>(camera).unwrap().viewport =
            Some(ViewportRect::new(0, 0, 320, 180).unwrap());
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(camera_view(&runtime, camera).id, id);
    runtime.finish().unwrap();
}

#[test]
fn camera_removal_readdition_and_despawn_retire_only_their_own_view() {
    let mut app = render_app(true);
    let world = app.world_mut().unwrap();
    let camera = world
        .spawn((Camera2d::default(), Transform2d::default()))
        .id();
    let other = world
        .spawn((Camera2d::default(), Transform2d::default()))
        .id();
    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    let original_id = camera_view(&runtime, camera).id;
    let other_id = camera_view(&runtime, other).id;

    edit_world(&mut runtime, move |world| {
        world.entity_mut(camera).remove::<Camera2d>();
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().get::<Transform2d>(camera).is_some());
    assert_eq!(runtime.world().resource::<ExtractedViews>().len(), 1);
    let lifetimes = runtime.world().resource::<CameraViewLifetimes>();
    assert_eq!(lifetimes.ids.len(), 1);
    assert!(!lifetimes.ids.contains_key(&camera));
    assert_eq!(camera_view(&runtime, other).id, other_id);

    edit_world(&mut runtime, move |world| {
        world.entity_mut(camera).insert(Camera2d::default());
    });
    runtime.drive(Duration::ZERO).unwrap();
    let readded_id = camera_view(&runtime, camera).id;
    assert_ne!(readded_id, original_id);

    edit_world(&mut runtime, move |world| {
        // No intervening extraction observes the absence of Camera2d.
        world.entity_mut(camera).remove::<Camera2d>();
        world.entity_mut(camera).insert(Camera2d::default());
    });
    runtime.drive(Duration::ZERO).unwrap();
    let replaced_id = camera_view(&runtime, camera).id;
    assert_ne!(replaced_id, readded_id);
    assert_ne!(replaced_id, original_id);
    assert_eq!(camera_view(&runtime, other).id, other_id);

    edit_world(&mut runtime, move |world| {
        assert!(world.despawn(camera));
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.world().resource::<CameraViewLifetimes>().ids.len(),
        1
    );
    assert_eq!(camera_view(&runtime, other).id, other_id);
    runtime.finish().unwrap();
}

#[test]
fn camera_readdition_while_filtered_starts_a_new_lifetime_before_visibility_returns() {
    let mut app = render_app(false);
    let camera = app
        .world_mut()
        .unwrap()
        .spawn((Camera2d::default(), Transform2d::default()))
        .id();
    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    let original_id = runtime.world().resource::<CameraViewLifetimes>().ids[&camera];
    edit_world(&mut runtime, move |world| {
        world.entity_mut(camera).remove::<Camera2d>();
        world.entity_mut(camera).insert(Camera2d::default());
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<ExtractedViews>().is_empty());
    let readded_id = runtime.world().resource::<CameraViewLifetimes>().ids[&camera];
    assert_ne!(readded_id, original_id);

    edit_world(&mut runtime, |world| {
        world.spawn(Window::default());
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(camera_view(&runtime, camera).id, readded_id);
    runtime.finish().unwrap();
}

#[derive(nara_ecs::Component)]
struct HiddenCamera;

#[test]
fn default_query_filters_do_not_retire_camera_membership() {
    use nara_ecs::bevy_ecs::entity_disabling::Disabled;

    let mut app = render_app(true);
    let world = app.world_mut().unwrap();
    world.register_disabling_component::<HiddenCamera>();
    let camera = world
        .spawn((Camera2d::default(), Transform2d::default()))
        .id();
    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    let id = camera_view(&runtime, camera).id;

    for custom in [false, true] {
        edit_world(&mut runtime, move |world| {
            if custom {
                world.entity_mut(camera).insert(HiddenCamera);
            } else {
                world.entity_mut(camera).insert(Disabled);
            }
        });
        runtime.drive(Duration::ZERO).unwrap();
        assert!(runtime.world().resource::<ExtractedViews>().is_empty());
        assert_eq!(
            runtime.world().resource::<CameraViewLifetimes>().ids[&camera],
            id
        );
        edit_world(&mut runtime, move |world| {
            if custom {
                world.entity_mut(camera).remove::<HiddenCamera>();
            } else {
                world.entity_mut(camera).remove::<Disabled>();
            }
        });
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(camera_view(&runtime, camera).id, id);
    }
    runtime.finish().unwrap();
}

#[test]
fn separate_worlds_and_manual_subviews_have_independent_retained_identities() {
    let image = RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        Extent2d::new(320, 180).unwrap(),
    )
    .unwrap();
    let config = Camera2d {
        target: RenderTarget::Image(image.image()),
        viewport: Some(ViewportRect::from_extent(image.extent()).unwrap()),
        ..Default::default()
    };
    let mut first_app = render_app(false);
    let first_camera = first_app
        .world_mut()
        .unwrap()
        .spawn((config, Transform2d::default()))
        .id();
    let mut second_app = render_app(false);
    let second_camera = second_app
        .world_mut()
        .unwrap()
        .spawn((config, Transform2d::default()))
        .id();
    assert_eq!(first_camera, second_camera);
    let mut first_runtime = start_runtime(first_app);
    let mut second_runtime = start_runtime(second_app);
    first_runtime.drive(Duration::ZERO).unwrap();
    second_runtime.drive(Duration::ZERO).unwrap();
    let first = camera_view(&first_runtime, first_camera);
    let second = camera_view(&second_runtime, second_camera);
    assert_ne!(first.id, second.id);
    first_runtime.finish().unwrap();
    second_runtime.finish().unwrap();

    let root = RenderViewId::try_acquire().unwrap();
    let another_root = RenderViewId::try_acquire().unwrap();
    let ids = [
        root,
        root.subview(0),
        root.subview(u32::MAX),
        another_root.subview(0),
    ];
    assert_eq!(root.subview(0).subview(u32::MAX), root.subview(u32::MAX));
    assert_eq!(
        ids.into_iter()
            .collect::<std::collections::HashSet<_>>()
            .len(),
        ids.len()
    );
    assert_eq!(
        ids.into_iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len(),
        ids.len()
    );
    assert!(ids.iter().all(|id| *id != first.id && *id != second.id));

    let mut views = ExtractedViews::default();
    for (index, id) in ids.into_iter().enumerate() {
        views.push(ExtractedView {
            id,
            order: index as i32,
            load: if index == 0 {
                ViewLoadOp::Clear(Color::BLACK)
            } else {
                ViewLoadOp::Load
            },
            ..first
        });
    }
    let source = RenderFrameSourceGeneration::try_acquire().unwrap();
    for frame in [1, 2] {
        let packet = build_image_render_frame_packet(source, frame, &views, image)
            .unwrap()
            .unwrap();
        assert_eq!(packet.views(), views.as_slice());
        assert_eq!(
            packet
                .views()
                .iter()
                .map(|view| view.id)
                .collect::<Vec<_>>(),
            ids
        );
        assert!(
            packet
                .views()
                .iter()
                .all(|view| view.camera_entity == first_camera)
        );
    }
}

#[test]
fn packet_builders_reject_nonadjacent_duplicate_ids_even_for_different_cameras() {
    let original = test_extracted_view();
    let image = RenderImagePacket::new(
        RenderImage2d::allocate_handle().unwrap(),
        original.viewport.extent(),
    )
    .unwrap();
    let source = RenderFrameSourceGeneration::try_acquire().unwrap();
    for target in [
        RenderTarget::PrimaryWindow,
        RenderTarget::Image(image.image()),
    ] {
        let mut views = ExtractedViews::default();
        views.push(ExtractedView { target, ..original });
        views.push(ExtractedView {
            id: RenderViewId::try_acquire().unwrap(),
            target,
            camera_entity: Entity::from_raw_u32(2).unwrap(),
            order: 1,
            load: ViewLoadOp::Load,
            ..original
        });
        views.push(ExtractedView {
            target,
            camera_entity: Entity::from_raw_u32(3).unwrap(),
            order: 2,
            load: ViewLoadOp::Load,
            ..original
        });
        let result = match target {
            RenderTarget::Image(_) => build_image_render_frame_packet(source, 1, &views, image),
            _ => build_render_frame_packet_for_source(
                source,
                1,
                None,
                [&Window::default()],
                &views,
                None,
                None,
            ),
        };
        assert_eq!(
            result,
            Err(RenderFramePacketError::DuplicateViewId { id: original.id })
        );
    }
}

#[test]
fn failed_extraction_observes_all_camera_lifetimes_without_recreating_them_on_retry() {
    let mut world = World::new();
    world.init_resource::<RenderFrame>();
    world.init_resource::<ExtractedViews>();
    world.init_resource::<ExtractedViewScratch>();
    world.init_resource::<CameraViewLifetimes>();
    world.init_resource::<ClearColor>();
    world.init_resource::<RealTime>();
    world.init_resource::<VirtualTime>();
    world.init_resource::<FixedTime>();
    world.init_resource::<RenderTime>();
    world.init_resource::<RuntimeTimeSettings>();
    let prior = test_extracted_view();
    world.resource_mut::<ExtractedViews>().push(prior);
    let camera = world.spawn(Camera2d::default()).id();
    let other = world.spawn(Camera2d::default()).id();
    let mut extraction = IntoSystem::<(), (), _>::into_system(extract_views);
    extraction.initialize(&mut world);

    // The production extractor cannot read a completed spatial publication in this World.
    // Membership observation must still complete before that failure, preserving prior output.
    assert!(extraction.run((), &mut world).is_err());
    let camera_id = world.resource::<CameraViewLifetimes>().ids[&camera];
    let other_id = world.resource::<CameraViewLifetimes>().ids[&other];
    for _ in 0..2 {
        assert!(extraction.run((), &mut world).is_err());
        assert_eq!(
            world.resource::<CameraViewLifetimes>().ids[&camera],
            camera_id
        );
        assert_eq!(
            world.resource::<CameraViewLifetimes>().ids[&other],
            other_id
        );
        assert_eq!(world.resource::<ExtractedViews>().as_slice(), &[prior]);
    }
    world.get_mut::<Camera2d>(camera).unwrap().order = 10;
    world.entity_mut(other).remove::<Camera2d>();
    world.entity_mut(other).insert(Camera2d::default());
    assert!(extraction.run((), &mut world).is_err());
    assert_eq!(
        world.resource::<CameraViewLifetimes>().ids[&camera],
        camera_id
    );
    assert_ne!(
        world.resource::<CameraViewLifetimes>().ids[&other],
        other_id
    );
    assert!(world.despawn(other));
    assert!(extraction.run((), &mut world).is_err());
    assert_eq!(world.resource::<CameraViewLifetimes>().ids.len(), 1);
    assert_eq!(world.resource::<ExtractedViews>().as_slice(), &[prior]);
}
