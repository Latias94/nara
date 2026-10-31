use super::*;
use nara_app::{__RuntimeDriverPort, RuntimeControl, RuntimeControlRequestResult};
use nara_ecs::{IntoSystem, bevy_ecs::system::System};

type WorldEdit = Box<dyn FnOnce(&mut World)>;

#[derive(Default, Resource)]
struct ViewTestPort;

impl __RuntimeDriverPort for ViewTestPort {
    type Input = WorldEdit;
    type Output = ();

    fn apply_driver_input(&mut self, _input: Self::Input) {
        unreachable!("the configured test port owns the World edit")
    }
}

fn lifecycle_app(with_window: bool) -> App {
    let mut app = render_app(with_window);
    app.init_resource::<ViewTestPort>().unwrap();
    app.__register_driver_port::<ViewTestPort>(|world, edit| {
        edit(world);
        Ok(())
    })
    .unwrap();
    app
}

fn edit_world(runtime: &mut RuntimeInstance, edit: impl FnOnce(&mut World) + 'static) {
    let outcome = runtime
        .with_driver_scope(|scope| scope.__apply_port::<ViewTestPort>(Box::new(edit)))
        .unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output.unwrap();
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
fn camera_edits_order_resize_target_and_pause_preserve_view_identity() {
    let mut app = lifecycle_app(true);
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
fn missing_zero_and_filtered_targets_preserve_the_camera_lifetime() {
    let mut app = lifecycle_app(false);
    let camera = app
        .world_mut()
        .unwrap()
        .spawn((Camera2d::default(), Transform2d::default()))
        .id();
    let mut runtime = start_runtime(app);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<ExtractedViews>().is_empty());
    let id = runtime.world().resource::<CameraViewLifetimes>().ids[&camera];

    edit_world(&mut runtime, |world| {
        world.spawn(Window::default());
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(camera_view(&runtime, camera).id, id);

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
            world.get_mut::<Camera2d>(camera).unwrap().target = target;
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
    let mut app = lifecycle_app(true);
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
    let mut app = lifecycle_app(false);
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

    let mut app = lifecycle_app(true);
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
