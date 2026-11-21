use super::*;
use nara_app::{FixedUpdateSet, RuntimeControl, RuntimeControlRequestResult};
use nara_core::Mat3;
use nara_transform::{
    FixedTransformInterpolation, SpatialSnapshotPhase, presented_spatial_snapshot,
};

const TIMESTEP: Duration = Duration::from_millis(20);

fn advance_camera(mut cameras: Query<&mut Transform2d, nara_ecs::With<Camera2d>>) {
    for mut transform in &mut cameras {
        transform.translation.x += 1.0;
    }
}

fn moving_camera_app() -> App {
    let mut app = render_app(true);
    app.insert_resource(FixedTime::new(TIMESTEP).unwrap())
        .unwrap();
    app.insert_resource(FixedTransformInterpolation).unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        advance_camera.in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    app.world_mut()
        .unwrap()
        .spawn((Camera2d::default(), Transform2d::default()));
    app
}

#[test]
fn parented_camera_presents_signed_scale_changes_without_suppressing_singular_faults() {
    let midpoint = TIMESTEP + TIMESTEP / 2;
    let mut rotation_equivalent = None;
    for (scale, angle, expected_center, mirrored) in [
        (Vec2::new(-1.0, 1.0), 0.0, Vec2::new(7.0, 0.0), true),
        (Vec2::new(1.0, -1.0), 0.0, Vec2::new(13.0, 0.0), true),
        (Vec2::splat(-1.0), 0.0, Vec2::new(10.0, -3.0), false),
        (
            Vec2::ONE,
            std::f32::consts::PI,
            Vec2::new(10.0, -3.0),
            false,
        ),
        (Vec2::new(0.0, 1.0), 0.0, Vec2::ZERO, false),
    ] {
        let mut app = render_app(true);
        app.insert_resource(FixedTime::new(TIMESTEP).unwrap())
            .unwrap();
        app.insert_resource(FixedTransformInterpolation).unwrap();
        let world = app.world_mut().unwrap();
        let parent = world.spawn(Transform2d::IDENTITY).id();
        let camera = world
            .spawn((
                Camera2d::default(),
                Transform2d::from_translation(Vec2::new(3.0, 0.0)),
            ))
            .id();
        place_parented(world, camera, parent);
        app.add_systems(
            CoreStage::FixedUpdate,
            (move |mut poses: Query<&mut Transform2d>| {
                *poses.get_mut(parent).unwrap() = Transform2d {
                    translation: Vec2::new(20.0, 0.0),
                    rotation: angle,
                    scale,
                };
            })
            .in_set(FixedUpdateSet::Simulate),
        )
        .unwrap();

        let mut runtime = RuntimeInstance::start(app).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        let first = capture_packet(&runtime);
        let mid = runtime.drive(midpoint);
        if scale.x == 0.0 {
            let retained = capture_packet(&runtime);
            let retry = runtime.drive(TIMESTEP);
            let tick = runtime.world().resource::<FixedTime>().tick();
            retire_runtime(runtime);
            let fault = mid.unwrap_err();
            assert_eq!(fault.fault().kind(), nara_app::RuntimeFaultKind::System);
            assert_eq!(retained, first);
            assert_eq!(retry.unwrap_err(), fault);
            assert_eq!(tick, 1);
            let singular = Transform2d {
                scale,
                ..Transform2d::IDENTITY
            }
            .matrix();
            assert!(matches!(
                camera_mapping(camera, singular, 100.0, ViewportRect::new(0, 0, 320, 180).unwrap()),
                Err(RenderViewExtractionError::SingularTransform { entity }) if entity == camera
            ));
            continue;
        }

        assert_eq!(mid.unwrap().frame().unwrap().status.fixed_tick, 1);
        let sample = assert_current_sample(&runtime);
        assert_eq!(sample.render().interpolation_alpha(), 0.5);
        assert!(sample.spatial_interpolated());
        let packet = capture_packet(&runtime);
        let view = &packet.views()[0];
        assert_eq!(view.id, first.views()[0].id);
        let matrix = view.mapping.clip_to_world_matrix();
        assert_eq!(matrix.determinant().is_sign_negative(), mirrored);
        assert!(
            view.mapping
                .clip_to_world(Vec2::ZERO)
                .abs_diff_eq(expected_center, 1.0e-4)
        );
        if scale == Vec2::splat(-1.0) {
            rotation_equivalent = Some(matrix);
        } else if angle != 0.0 {
            assert!(matrix.abs_diff_eq(rotation_equivalent.unwrap(), 1.0e-4));
        }
        assert_eq!(
            runtime
                .drive(TIMESTEP)
                .unwrap()
                .frame()
                .unwrap()
                .status
                .fixed_tick,
            2
        );
        assert_eq!(capture_packet(&runtime).views()[0].id, view.id);
        runtime.finish().unwrap();
    }
}

#[test]
fn exact_fixed_cut_uses_one_parent_and_child_camera_basis() {
    let large = 2.0_f32.powi(40);
    let small = large.recip();
    let mut app = render_app(true);
    app.insert_resource(FixedTime::new(TIMESTEP).unwrap())
        .unwrap();
    app.insert_resource(FixedTransformInterpolation).unwrap();
    let world = app.world_mut().unwrap();
    let parent = world
        .spawn(Transform2d {
            scale: Vec2::splat(large),
            ..Transform2d::IDENTITY
        })
        .id();
    let camera = world
        .spawn((
            Camera2d::default(),
            Transform2d {
                scale: Vec2::splat(small),
                ..Transform2d::IDENTITY
            },
        ))
        .id();
    place_parented(world, camera, parent);
    app.add_systems(
        CoreStage::FixedUpdate,
        (move |mut transforms: Query<&mut Transform2d>| {
            transforms.get_mut(parent).unwrap().scale = Vec2::new(-small, small);
            transforms.get_mut(camera).unwrap().scale = Vec2::splat(large);
        })
        .in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let initial = capture_packet(&runtime);
    assert_eq!(
        runtime
            .drive(TIMESTEP)
            .unwrap()
            .frame()
            .unwrap()
            .status
            .fixed_tick,
        1
    );
    assert_eq!(
        assert_current_sample(&runtime)
            .render()
            .interpolation_alpha(),
        0.0
    );
    let boundary = capture_packet(&runtime);
    assert_eq!(boundary.views()[0].mapping, initial.views()[0].mapping);
    assert_eq!(boundary.views()[0].id, initial.views()[0].id);
    runtime.drive(TIMESTEP / 2).unwrap();
    let interior = capture_packet(&runtime);
    assert!(
        interior.views()[0]
            .mapping
            .clip_to_world_matrix()
            .determinant()
            < 0.0
    );
    assert_eq!(interior.views()[0].id, initial.views()[0].id);
    runtime.finish().unwrap();
}

#[test]
fn parented_camera_preserves_a_non_singular_subnormal_midpoint() {
    let mut app = render_app(true);
    app.insert_resource(FixedTime::new(TIMESTEP).unwrap())
        .unwrap();
    app.insert_resource(FixedTransformInterpolation).unwrap();
    let world = app.world_mut().unwrap();
    let parent = world
        .spawn(Transform2d {
            scale: Vec2::new(2.0_f32.powi(126), 1.0),
            ..Transform2d::IDENTITY
        })
        .id();
    let camera = world
        .spawn((
            Camera2d::default(),
            Transform2d {
                scale: Vec2::new(f32::from_bits(1), 1.0),
                ..Transform2d::IDENTITY
            },
        ))
        .id();
    place_parented(world, camera, parent);
    app.add_systems(
        CoreStage::FixedUpdate,
        (move |mut transforms: Query<&mut Transform2d>| {
            transforms.get_mut(parent).unwrap().translation = Vec2::Y;
            transforms.get_mut(camera).unwrap().translation = -Vec2::Y;
        })
        .in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();

    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let initial = capture_packet(&runtime);
    runtime.drive(TIMESTEP + TIMESTEP / 2).unwrap();
    let midpoint = capture_packet(&runtime);
    assert_eq!(midpoint.views()[0].mapping, initial.views()[0].mapping);
    assert_eq!(midpoint.views()[0].id, initial.views()[0].id);
    assert_eq!(
        runtime
            .drive(TIMESTEP)
            .unwrap()
            .frame()
            .unwrap()
            .status
            .fixed_tick,
        2
    );
    runtime.finish().unwrap();
}

fn capture_packet(runtime: &RuntimeInstance) -> RenderFramePacket {
    let world = runtime.world();
    let views = world.resource::<ExtractedViews>();
    build_render_frame_packet_with_world_canvas(
        runtime.generation(),
        world.resource::<RenderFrame>().index,
        Some(WindowId::PRIMARY),
        [&Window::default()],
        views,
        views.world_canvas(),
    )
    .unwrap()
    .unwrap()
}

fn assert_current_sample(runtime: &RuntimeInstance) -> CapturedRenderTime {
    let world = runtime.world();
    let sample = world.resource::<ExtractedViews>().captured_time().unwrap();
    assert_eq!(sample.real(), *world.resource::<RealTime>());
    assert_eq!(sample.virtual_time(), *world.resource::<VirtualTime>());
    assert_eq!(sample.fixed(), *world.resource::<FixedTime>());
    assert_eq!(sample.render(), *world.resource::<RenderTime>());
    assert_eq!(sample.settings(), *world.resource::<RuntimeTimeSettings>());
    assert_eq!(
        sample.spatial_stamp(),
        presented_spatial_snapshot(world).unwrap().stamp().unwrap()
    );
    assert_eq!(sample.spatial_stamp().frame, sample.real().frame);
    assert_eq!(sample.spatial_stamp().tick, sample.fixed().tick());
    assert_eq!(
        sample.spatial_stamp().runtime_generation,
        Some(runtime.generation())
    );
    assert_eq!(
        sample.spatial_stamp().phase,
        SpatialSnapshotPhase::Presented
    );
    sample
}

#[test]
fn extraction_captures_original_scaled_clocks_with_zero_and_multiple_fixed_ticks() {
    let mut app = moving_camera_app();
    app.insert_resource(RuntimeTimeSettings::default().with_time_scale(0.5).unwrap())
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    assert_eq!(
        runtime.world().resource::<ExtractedViews>().captured_time(),
        None
    );

    runtime.drive(TIMESTEP).unwrap();
    let first = assert_current_sample(&runtime);
    assert_eq!(first.real().delta, TIMESTEP);
    assert_eq!(first.real().elapsed, TIMESTEP);
    assert_eq!(first.real().frame, 1);
    assert_eq!(first.virtual_time().delta, TIMESTEP / 2);
    assert_eq!(first.virtual_time().elapsed, TIMESTEP / 2);
    assert_eq!(first.virtual_time().frame, 1);
    assert_eq!(first.fixed().tick(), 0);
    assert_eq!(first.fixed().steps_this_frame(), 0);
    assert_eq!(first.render().remainder(), TIMESTEP / 2);
    assert_eq!(first.render().interpolation_alpha(), 0.5);
    assert!(!first.spatial_interpolated());
    let old_packet = capture_packet(&runtime);
    let old_views = old_packet.views().to_vec();
    assert_eq!(old_packet.captured_time(), Some(first));

    runtime.drive(TIMESTEP * 4).unwrap();
    let second = assert_current_sample(&runtime);
    assert_eq!(second.real().delta, TIMESTEP * 4);
    assert_eq!(second.real().elapsed, TIMESTEP * 5);
    assert_eq!(second.virtual_time().delta, TIMESTEP * 2);
    assert_eq!(second.virtual_time().elapsed, TIMESTEP * 2 + TIMESTEP / 2);
    assert_eq!(second.fixed().tick(), 2);
    assert_eq!(second.fixed().steps_this_frame(), 2);
    assert_eq!(second.fixed().elapsed(), TIMESTEP * 2);
    assert_eq!(second.render().remainder(), TIMESTEP / 2);
    assert_eq!(second.settings().time_scale(), 0.5);
    assert!(second.spatial_interpolated());
    let next_packet = capture_packet(&runtime);
    assert_eq!(next_packet.captured_time(), Some(second));
    assert!(
        next_packet.views()[0]
            .mapping
            .clip_to_world(Vec2::ZERO)
            .abs_diff_eq(Vec2::new(1.5, 0.0), 1.0e-5)
    );
    assert_eq!(old_packet.captured_time(), Some(first));
    assert_eq!(old_packet.views(), old_views.as_slice());
    assert_eq!(
        old_packet.views()[0].mapping.clip_to_world(Vec2::ZERO),
        Vec2::ZERO
    );

    runtime.finish().unwrap();
}

#[test]
fn changing_timestep_in_update_preserves_the_completed_view_clock_and_pose() {
    let mut app = moving_camera_app();
    app.add_systems(
        CoreStage::Update,
        |mut fixed: nara_ecs::ResMut<FixedTime>| {
            if fixed.timestep() == TIMESTEP {
                fixed.set_timestep(TIMESTEP / 2).unwrap();
            }
        },
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();

    let before = runtime.drive(Duration::from_millis(55)).unwrap();
    let before = before.frame().unwrap().status;
    assert_eq!(before.fixed_tick, 2);
    assert_eq!(before.fixed_remainder, Duration::from_millis(15));
    let scheduled = runtime.world().resource::<FixedTime>();
    assert_eq!(scheduled.timestep(), TIMESTEP / 2);
    assert_eq!(scheduled.remainder(), Duration::from_millis(5));
    assert_eq!(scheduled.debt(), TIMESTEP / 2);

    let old = capture_packet(&runtime);
    let clock = old.captured_time().unwrap();
    assert_eq!(clock.fixed().timestep(), TIMESTEP);
    assert_eq!(clock.fixed().delta(), TIMESTEP);
    assert_eq!(clock.fixed().remainder(), before.fixed_remainder);
    assert_eq!(clock.fixed().debt(), Duration::ZERO);
    assert_eq!(clock.render().interpolation_alpha(), 0.75);
    assert!(clock.spatial_interpolated());
    let center = old.views()[0].mapping.clip_to_world(Vec2::ZERO);
    assert!(center.abs_diff_eq(Vec2::new(1.75, 0.0), 1.0e-5));

    let after = runtime.drive(Duration::ZERO).unwrap();
    let after = after.frame().unwrap().status;
    assert_eq!(after.fixed_tick, 3);
    assert_eq!(after.fixed_steps, 1);
    assert_eq!(after.fixed_elapsed, Duration::from_millis(50));
    let clock_after = assert_current_sample(&runtime);
    assert_eq!(clock_after.fixed().timestep(), TIMESTEP / 2);
    assert_eq!(clock_after.fixed().delta(), TIMESTEP / 2);
    assert_eq!(clock_after.fixed().remainder(), Duration::from_millis(5));
    assert_eq!(clock_after.render().interpolation_alpha(), 0.5);
    assert!(clock_after.spatial_interpolated());
    let new = capture_packet(&runtime);
    assert!(
        new.views()[0]
            .mapping
            .clip_to_world(Vec2::ZERO)
            .abs_diff_eq(Vec2::new(2.5, 0.0), 1.0e-5)
    );
    runtime.finish().unwrap();
    assert_eq!(old.captured_time(), Some(clock));
    assert_eq!(old.views()[0].mapping.clip_to_world(Vec2::ZERO), center);
}

#[test]
fn pause_and_exact_step_retain_the_sample_until_the_next_extract() {
    let mut runtime = RuntimeInstance::start(moving_camera_app()).unwrap();
    runtime.drive(TIMESTEP * 2 + TIMESTEP / 2).unwrap();
    assert!(assert_current_sample(&runtime).spatial_interpolated());

    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::from_millis(30)).unwrap();
    let paused = assert_current_sample(&runtime);
    assert!(paused.settings().paused());
    assert_eq!(paused.real().delta, Duration::from_millis(30));
    assert_eq!(paused.virtual_time().delta, Duration::ZERO);
    assert_eq!(paused.fixed().tick(), 2);
    assert_eq!(paused.fixed().steps_this_frame(), 0);
    assert_eq!(paused.render().interpolation_alpha(), 0.5);
    assert!(!paused.spatial_interpolated());
    let paused_views = runtime.world().resource::<ExtractedViews>().clone();
    let paused_packet = capture_packet(&runtime);
    assert_eq!(paused_packet.frame_index(), 2);
    assert!(
        paused_packet.views()[0]
            .mapping
            .clip_to_world(Vec2::ZERO)
            .abs_diff_eq(Vec2::new(2.0, 0.0), 1.0e-5)
    );

    assert!(matches!(
        runtime.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::from_millis(7)).unwrap();
    assert_eq!(runtime.world().resource::<RealTime>().frame, 3);
    assert_eq!(runtime.world().resource::<VirtualTime>().delta, TIMESTEP);
    assert_eq!(runtime.world().resource::<FixedTime>().tick(), 3);
    assert_eq!(*runtime.world().resource::<RenderTime>(), paused.render());
    assert_eq!(runtime.world().resource::<ExtractedViews>(), &paused_views);
    assert_eq!(capture_packet(&runtime), paused_packet);

    runtime.drive(Duration::from_millis(5)).unwrap();
    let after_step = assert_current_sample(&runtime);
    let after_step_packet = capture_packet(&runtime);
    assert_eq!(after_step.real().frame, 4);
    assert_eq!(after_step.virtual_time().frame, 4);
    assert_eq!(after_step.real().delta, Duration::from_millis(5));
    assert_eq!(after_step.virtual_time().delta, Duration::ZERO);
    assert_eq!(
        after_step.virtual_time().elapsed,
        paused.virtual_time().elapsed + TIMESTEP
    );
    assert_eq!(after_step.fixed().tick(), 3);
    assert_eq!(after_step.render().remainder(), paused.render().remainder());
    assert_eq!(
        after_step.render().interpolation_alpha(),
        paused.render().interpolation_alpha()
    );
    assert!(!after_step.spatial_interpolated());
    assert_eq!(after_step_packet.frame_index(), 3);
    assert_ne!(after_step_packet.frame_index(), after_step.real().frame);
    assert_eq!(after_step_packet.captured_time(), Some(after_step));
    assert!(
        after_step_packet.views()[0]
            .mapping
            .clip_to_world(Vec2::ZERO)
            .abs_diff_eq(Vec2::new(3.0, 0.0), 1.0e-5)
    );
    assert_eq!(paused_packet.captured_time(), Some(paused));
    assert_eq!(paused_packet.views(), paused_views.as_slice());

    runtime.finish().unwrap();
}

#[test]
fn elapsed_resume_after_exact_step_captures_the_settled_pose_and_retains_it() {
    let mut runtime = RuntimeInstance::start(moving_camera_app()).unwrap();
    runtime.drive(TIMESTEP * 2 + TIMESTEP / 2).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    let outcome = runtime.drive_elapsed(Duration::from_millis(5)).unwrap();
    assert!(outcome.frame().unwrap().status.simulation_paused);
    let sample = assert_current_sample(&runtime);
    assert!(!sample.settings().paused());
    assert_eq!(sample.virtual_time().delta, Duration::ZERO);
    assert_eq!(sample.fixed().tick(), 3);
    assert_eq!(sample.fixed().remainder(), TIMESTEP / 2);
    assert_eq!(sample.render().interpolation_alpha(), 0.5);
    assert!(!sample.spatial_interpolated());
    let packet = capture_packet(&runtime);
    assert_eq!(packet.captured_time(), Some(sample));
    let center = packet.views()[0].mapping.clip_to_world(Vec2::ZERO);
    assert!(center.abs_diff_eq(Vec2::new(3.0, 0.0), 1.0e-5));

    let next = runtime.drive_elapsed(TIMESTEP / 2).unwrap();
    assert!(!next.frame().unwrap().status.simulation_paused);
    assert_eq!(next.frame().unwrap().status.fixed_tick, 4);
    assert_eq!(next.frame().unwrap().status.fixed_remainder, Duration::ZERO);
    assert_ne!(
        capture_packet(&runtime).captured_time(),
        packet.captured_time()
    );
    runtime.finish().unwrap();
    assert_eq!(packet.captured_time(), Some(sample));
    assert_eq!(packet.views()[0].mapping.clip_to_world(Vec2::ZERO), center);
}

#[test]
fn delayed_packet_build_and_target_adaptation_preserve_the_original_sample() {
    let mut app = moving_camera_app();
    let canvas = WorldCanvas::new(320, 180).unwrap();
    app.insert_resource(canvas).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(TIMESTEP + TIMESTEP / 2).unwrap();
    let views = runtime.world().resource::<ExtractedViews>().clone();
    let sample = views.captured_time().unwrap();
    let frame_index = runtime.world().resource::<RenderFrame>().index;
    let original = capture_packet(&runtime);
    let target = RenderImagePacket::new(
        nara_asset::allocate_anonymous_handle().unwrap(),
        original.target().extent(),
    )
    .unwrap();

    runtime.drive(TIMESTEP * 2).unwrap();
    assert_ne!(assert_current_sample(&runtime), sample);
    let delayed = build_render_frame_packet_with_world_canvas(
        runtime.generation(),
        frame_index,
        Some(WindowId::PRIMARY),
        [&Window::default()],
        &views,
        views.world_canvas(),
    )
    .unwrap()
    .unwrap();
    assert_eq!(delayed, original);
    let adapted = delayed.with_image_target(target).unwrap();
    assert_eq!(adapted.captured_time(), Some(sample));
    assert_eq!(adapted.views(), original.views());
    assert_eq!(adapted.frame_index(), original.frame_index());
    assert_eq!(adapted.source_generation(), original.source_generation());
    assert_eq!(adapted.world_canvas(), original.world_canvas());
    assert_eq!(adapted.target(), RenderFrameTarget::Image(target));
    assert!(matches!(original.target(), RenderFrameTarget::Window(_)));

    runtime.finish().unwrap();
}

#[test]
fn manual_views_need_no_runtime_and_mutations_invalidate_the_captured_sample() {
    let source = RenderFrameSourceGeneration::try_acquire().unwrap();
    let target = RenderImagePacket::new(
        nara_asset::allocate_anonymous_handle().unwrap(),
        Extent2d::new(1280, 720).unwrap(),
    )
    .unwrap();
    let mut manual = ExtractedViews::default();
    manual.push(ExtractedView {
        target: RenderTarget::Image(target.image()),
        ..test_extracted_view()
    });
    let manual_packet = build_image_render_frame_packet(source, 91, &manual, target)
        .unwrap()
        .unwrap();
    assert_eq!(manual.captured_time(), None);
    assert_eq!(manual_packet.captured_time(), None);
    assert_eq!(manual_packet.frame_index(), 91);
    assert_eq!(manual_packet.source_generation(), source);

    let mut runtime = RuntimeInstance::start(moving_camera_app()).unwrap();
    runtime.drive(TIMESTEP + TIMESTEP / 2).unwrap();
    let captured = runtime.world().resource::<ExtractedViews>().clone();
    let sample = captured.captured_time().unwrap();
    let mut appended = captured.clone();
    let first = appended.as_slice()[0];
    appended.push(ExtractedView {
        id: first.id.subview(1),
        order: 1,
        load: ViewLoadOp::Load,
        ..first
    });
    assert_eq!(appended.captured_time(), None);
    let appended_packet = build_render_frame_packet(
        runtime.generation(),
        92,
        Some(WindowId::PRIMARY),
        [&Window::default()],
        &appended,
    )
    .unwrap()
    .unwrap();
    assert_eq!(appended_packet.captured_time(), None);
    let adapted_manual = appended_packet.with_image_target(target).unwrap();
    assert_eq!(adapted_manual.captured_time(), None);
    let mut cleared = captured.clone();
    cleared.clear();
    assert!(cleared.is_empty());
    assert_eq!(cleared.captured_time(), None);
    assert_eq!(captured.captured_time(), Some(sample));

    runtime.finish().unwrap();
}

#[test]
fn failed_extraction_keeps_the_previous_views_and_time_sample_together() {
    fn invalidate_projection(real: Res<RealTime>, mut cameras: Query<&mut Camera2d>) {
        if real.frame > 1 {
            for mut camera in &mut cameras {
                camera.viewport_height = 0.0;
            }
        }
    }

    let mut app = moving_camera_app();
    app.add_systems(CoreStage::PostUpdate, invalidate_projection)
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(TIMESTEP).unwrap();
    let original_views = runtime.world().resource::<ExtractedViews>().clone();
    let original = capture_packet(&runtime);

    assert!(runtime.drive(TIMESTEP).is_err());
    assert_eq!(
        runtime.world().resource::<ExtractedViews>(),
        &original_views
    );
    assert_eq!(capture_packet(&runtime), original);
    assert_eq!(original.captured_time().unwrap().real().frame, 1);
    assert_eq!(runtime.world().resource::<RealTime>().frame, 2);

    retire_runtime(runtime);
}

#[test]
fn matrix_accessors_preserve_the_captured_affine_projection_and_inverse() {
    let world_from_camera = Transform2d {
        translation: Vec2::new(27.0, -13.0),
        rotation: 0.7,
        scale: Vec2::new(2.0, 0.5),
    }
    .matrix();
    let mapping = ViewMapping2d::orthographic(
        world_from_camera,
        100.0,
        ViewportRect::new(40, 20, 400, 200).unwrap(),
    )
    .unwrap();
    let world_to_clip = mapping.world_to_clip_matrix();
    let clip_to_world = mapping.clip_to_world_matrix();
    let world_corner = world_from_camera.transform_point2(Vec2::new(100.0, 50.0));
    assert!(
        world_to_clip
            .transform_point2(world_corner)
            .abs_diff_eq(Vec2::ONE, 1.0e-5)
    );
    assert!(
        clip_to_world
            .transform_point2(Vec2::ONE)
            .abs_diff_eq(world_corner, 1.0e-4)
    );
    assert!((world_to_clip * clip_to_world).abs_diff_eq(Mat3::IDENTITY, 1.0e-5));
    assert!((clip_to_world * world_to_clip).abs_diff_eq(Mat3::IDENTITY, 1.0e-4));
    assert_eq!(
        world_to_clip.transform_point2(world_corner),
        mapping.world_to_clip(world_corner)
    );
    assert_eq!(
        clip_to_world.transform_point2(Vec2::ONE),
        mapping.clip_to_world(Vec2::ONE)
    );
}
