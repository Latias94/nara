use super::*;
use crate::runtime::{
    WgpuRuntimeCapture, WgpuRuntimeConnection, complete_runtime_frame, take_runtime_frame,
};
#[cfg(feature = "ui-submitter")]
use std::sync::Arc;

use nara_app::{App, CoreStage, FrameExecutionStart, RuntimeFaultKind, RuntimeState};
use nara_ecs::{Commands, ResMut, Resource, schedule::IntoScheduleConfigs};
#[cfg(feature = "sprite-submitter")]
use nara_ecs::{Entity, Res};
#[cfg(feature = "ui-submitter")]
use nara_ecs::{bevy_ecs::system::System, system::IntoSystem};
#[cfg(feature = "sprite-submitter")]
use nara_render::Color;
#[cfg(feature = "ui-submitter")]
use nara_render::RenderFrameState;
use nara_render::{
    Extent2d, ExtractedViews, FrameStats, RenderBackendState, RenderBackendStatus, RenderFrame,
    RenderFrameSkipReason,
};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_render::{RenderTarget, ViewportRect};
#[cfg(feature = "ui-submitter")]
use nara_ui_render::{
    __private::{
        UiBackendSubmission, UiBackendSubmissionReader, UiPreparedBackendSubmission,
        UiPresentationOutcomes, UiSubmittedBackendSubmission,
    },
    UiPaintPacket, UiPresentationAttemptId, UiPresentationBackendFaultDomain,
    UiPresentationBackendGeneration, UiPresentationCapabilityGeneration, UiPresentationEpoch,
    UiPresentationOutcome, UiPresentationRuntimeGeneration,
};
use nara_window::{
    WindowId,
    backend::{BackendWindowHandles, WindowPresentationRequirement},
};

#[cfg(feature = "ui-submitter")]
use crate::frame::WgpuUiSubmissionState;

fn admit_runtime(app: App) -> nara_app::RuntimeCandidate {
    nara_app::RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            app.seal().unwrap(),
            nara_app::RuntimeObligationLedger::new(),
            nara_app::RuntimeClosePolicy::default(),
        )
        .unwrap()
}

fn take_captured_frame(runtime: &mut nara_app::RuntimeInstance) -> WgpuRuntimeCapture {
    let outcome = take_runtime_frame(runtime).unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome
        .output
        .expect("the managed frame must publish a capture")
}

fn install_render_foundation(app: &mut App, with_window: bool) {
    app.add_plugins((
        nara_reflect::ComponentRegistryPlugin,
        nara_hierarchy::HierarchyPlugin,
        nara_transform::TransformPlugin,
        nara_render::RenderComponentsPlugin,
        nara_render::RenderPlugin::default(),
    ))
    .unwrap();
    if with_window {
        app.add_plugins(nara_window::WindowPlugin::default())
            .unwrap();
    }
}

fn install_test_plugins(app: &mut App, with_window: bool) {
    install_render_foundation(app, with_window);
    #[cfg(feature = "ui-submitter")]
    app.add_plugins((
        nara_image::ImagePreparePlugin,
        nara_ui_render::UiRenderPlugin::default(),
    ))
    .unwrap();
    app.add_plugins(WgpuRenderPlugin).unwrap();
    #[cfg(feature = "ui-submitter")]
    app.add_plugins(WgpuUiSubmitterPlugin).unwrap();
}

#[cfg(feature = "sprite-submitter")]
#[derive(Debug, Default, Resource)]
struct UninstalledSpriteContributionObservation {
    source_instances: usize,
    collected_instances: Option<usize>,
}

#[cfg(feature = "sprite-submitter")]
fn inject_sprite_domain_work(mut views: ResMut<ExtractedViews>) {
    views.clear();
    views.push(nara_render::ExtractedView {
        camera_entity: Entity::PLACEHOLDER,
        target: nara_render::RenderTarget::PrimaryWindow,
        viewport: nara_render::ViewportRect::new(0, 0, 16, 16).unwrap(),
        mapping: nara_render::ViewMapping2d::orthographic(
            nara_core::Mat3::IDENTITY,
            16.0,
            ViewportRect::new(0, 0, 16, 16).unwrap(),
        )
        .unwrap(),
        order: 0,
        load: nara_render::ViewLoadOp::Clear(nara_core::Color::BLACK),
    });
}

#[cfg(feature = "sprite-submitter")]
fn observe_uninstalled_sprite_contribution(
    batches: Res<nara_sprite_render::__private::SpriteBatches>,
    slot: Res<WgpuFramePacketSlot>,
    mut observation: ResMut<UninstalledSpriteContributionObservation>,
    mut views: ResMut<ExtractedViews>,
) {
    observation.source_instances = batches.total_instances();
    observation.collected_instances = slot.collected_sprite_instances();
    views.clear();
}

#[cfg(feature = "ui-submitter")]
#[derive(Debug, Default, Resource)]
struct UninstalledUiContributionObservation {
    packet_count: usize,
    submission_count: usize,
    submissions: Vec<UiBackendSubmission>,
    outcomes: Vec<UiPresentationOutcome>,
}

#[cfg(feature = "ui-submitter")]
fn observe_undrained_ui_backend_frame(
    mut reader: UiBackendSubmissionReader,
    mut observation: ResMut<UninstalledUiContributionObservation>,
) {
    let frame = reader
        .take()
        .expect("the uninstalled contribution must leave the backend frame available");
    let (packets, submissions) = frame.into_parts();
    observation.packet_count = packets.len();
    observation.submission_count = submissions.len();
    observation.submissions = submissions;
}

#[cfg(feature = "ui-submitter")]
fn reject_observed_ui_backend_frame(
    mut observation: ResMut<UninstalledUiContributionObservation>,
    mut outcomes: ResMut<UiPresentationOutcomes>,
) {
    for submission in std::mem::take(&mut observation.submissions) {
        outcomes.commit_backend(submission.content_rejected(
            "the isolation test deliberately rejected the undrained frame".to_owned(),
        ));
    }
}

#[cfg(feature = "ui-submitter")]
fn record_ui_isolation_outcomes(world: &mut nara_ecs::World) {
    let outcomes = nara_ui_render::drain_ui_presentation_outcomes(world)
        .expect("the test-owned rejection must release every backend permit");
    world
        .resource_mut::<UninstalledUiContributionObservation>()
        .outcomes = outcomes;
}

#[derive(Debug, Default, Resource)]
struct SecondCameraInjection(bool);

fn inject_second_camera_after_capture(
    mut commands: Commands,
    mut injection: ResMut<SecondCameraInjection>,
) {
    if injection.0 {
        return;
    }
    injection.0 = true;
    commands.spawn((
        nara_render::Camera2d {
            target: nara_render::RenderTarget::Window(WindowId::new(999)),
            viewport: Some(nara_render::ViewportRect::new(0, 0, 16, 16).unwrap()),
            ..Default::default()
        },
        nara_transform::Transform2d::default(),
    ));
}

#[test]
fn plugin_installs_cpu_capture_connection_and_render_resources() {
    let mut app = App::new();
    install_test_plugins(&mut app, false);

    assert!(app.world().contains_resource::<WgpuRuntimeConnection>());
    assert_eq!(
        app.world()
            .resource::<WgpuRenderObservation>()
            .instance_id(),
        0,
        "installing capture systems must not construct a native execution owner",
    );
    assert!(app.world().contains_resource::<ExtractedViews>());
    assert!(app.world().contains_resource::<RenderFrame>());
    assert!(app.world().contains_resource::<FrameStats>());
    assert!(app.world().contains_resource::<RenderBackendStatus>());
    assert_eq!(
        app.world()
            .resource::<WindowPresentationRequirement>()
            .backend(),
        WGPU_RENDER_BACKEND
    );
}

#[cfg(feature = "sprite-submitter")]
#[test]
fn compiled_but_uninstalled_sprite_contribution_does_not_collect_domain_batches() {
    let mut app = App::new();
    install_render_foundation(&mut app, false);
    app.add_plugins((
        nara_image::ImagePreparePlugin,
        nara_sprite_render::SpriteRenderPlugin,
    ))
    .unwrap();
    app.add_plugins(WgpuRenderPlugin).unwrap();
    app.insert_resource(UninstalledSpriteContributionObservation::default())
        .unwrap();
    app.add_systems(CoreStage::Prepare, inject_sprite_domain_work)
        .unwrap();
    app.add_systems(
        CoreStage::Render,
        observe_uninstalled_sprite_contribution
            .after(frame::WgpuFrameSet::CollectSprite)
            .before(frame::WgpuFrameSet::Seal),
    )
    .unwrap();
    let world = app.world_mut().unwrap();
    let entity = world
        .spawn((
            nara_sprite::Sprite::from_color(nara_core::Vec2::ONE, Color::WHITE),
            nara_transform::Transform2d::default(),
        ))
        .id();
    nara_hierarchy::HierarchyWriter::new(world)
        .apply_batch(&[nara_hierarchy::HierarchyOperation::Place {
            entity,
            placement: nara_hierarchy::HierarchyPlacement::last(
                nara_hierarchy::HierarchyPlacementOwner::Root,
            ),
        }])
        .unwrap();
    let candidate = admit_runtime(app);
    let mut runtime = candidate.complete_startup().unwrap().promote();

    runtime.drive(std::time::Duration::ZERO).unwrap();

    let observation = runtime
        .world()
        .resource::<UninstalledSpriteContributionObservation>();
    let source_instances = observation.source_instances;
    let collected_instances = observation.collected_instances;
    let stats = runtime.world().resource::<FrameStats>();
    let draw_calls = stats.draw_calls;
    let sprites = stats.sprites;
    runtime.finish().unwrap();
    assert_eq!(source_instances, 1);
    assert_eq!(collected_instances, None);
    assert_eq!(draw_calls, 0);
    assert_eq!(sprites, 0);
}

#[cfg(feature = "ui-submitter")]
#[test]
fn compiled_but_uninstalled_ui_contribution_does_not_drain_the_backend_frame() {
    use nara_ui_kernel::UiSurfaceId;
    use nara_ui_render::__private::{
        UiPresentationRoute, replace_ui_backend_frame, replace_ui_presentation_routes,
        snapshot_ui_paint_packets,
    };

    let mut app = App::new();
    install_render_foundation(&mut app, false);
    app.add_plugins((
        nara_image::ImagePreparePlugin,
        nara_ui_render::UiRenderPlugin::default(),
    ))
    .unwrap();
    app.add_plugins(WgpuRenderPlugin).unwrap();
    app.insert_resource(UninstalledUiContributionObservation::default())
        .unwrap();
    app.add_systems(
        CoreStage::Render,
        (
            observe_undrained_ui_backend_frame,
            reject_observed_ui_backend_frame,
            record_ui_isolation_outcomes,
        )
            .chain()
            .after(frame::WgpuFrameSet::Seal),
    )
    .unwrap();

    let surface_id = UiSurfaceId::new(1).unwrap();
    let world = app.world_mut().unwrap();
    replace_ui_presentation_routes(
        world,
        [UiPresentationRoute::new(surface_id, WindowId::PRIMARY).unwrap()],
    )
    .unwrap();
    nara_ui_render::compile_ui_paint_packets(world, &[ui_outcome_frame()]).unwrap();
    let (mut planned, _) = snapshot_ui_paint_packets(world).unwrap().into_parts();
    let packet = planned
        .pop()
        .expect("the planner must publish one exact backend packet");
    assert!(
        replace_ui_backend_frame(
            world,
            vec![Arc::clone(&packet)],
            vec![prepared_ui_attempt(packet, 61, 41)],
        )
        .unwrap()
        .is_empty()
    );

    let candidate = admit_runtime(app);
    let mut runtime = candidate.complete_startup().unwrap().promote();
    runtime.drive(std::time::Duration::ZERO).unwrap();

    let observation = runtime
        .world()
        .resource::<UninstalledUiContributionObservation>();
    assert_eq!(observation.packet_count, 1);
    assert_eq!(observation.submission_count, 1);
    assert!(observation.submissions.is_empty());
    assert!(matches!(
        observation.outcomes.as_slice(),
        [UiPresentationOutcome::ContentRejected { attempt, .. }]
            if attempt.admission_id().is_some_and(|id| id.get() == 61)
    ));
}

#[cfg(feature = "ui-submitter")]
#[test]
fn replacement_before_ui_collection_cannot_take_original_permissions() {
    use nara_ui_kernel::UiSurfaceId;
    use nara_ui_render::__private::{
        UiPresentationRoute, replace_ui_backend_frame, replace_ui_presentation_routes,
        snapshot_ui_paint_packets,
    };

    let mut app = App::new();
    install_test_plugins(&mut app, false);
    let world = app.world_mut().unwrap();
    replace_ui_presentation_routes(
        world,
        [UiPresentationRoute::new(UiSurfaceId::new(1).unwrap(), WindowId::PRIMARY).unwrap()],
    )
    .unwrap();
    nara_ui_render::compile_ui_paint_packets(world, &[ui_outcome_frame()]).unwrap();
    let (mut planned, _) = snapshot_ui_paint_packets(world).unwrap().into_parts();
    let packet = planned.pop().unwrap();
    replace_ui_backend_frame(
        world,
        vec![Arc::clone(&packet)],
        vec![prepared_ui_attempt(packet, 73, 53)],
    )
    .unwrap();
    world.insert_resource(WgpuRuntimeConnection::default());

    let mut collect = IntoSystem::into_system(collect_wgpu_ui_payload);
    collect.initialize(world);
    collect.run((), world).unwrap();
    assert!(
        world
            .resource_mut::<WgpuFramePacketSlot>()
            .take_ui_submissions()
            .is_none()
    );
    assert!(world.resource::<WgpuRuntimeConnection>().can_capture());

    fn take_original(
        mut reader: UiBackendSubmissionReader,
    ) -> nara_ui_render::__private::UiBackendPresentationFrame {
        reader.take().unwrap()
    }
    let mut take = IntoSystem::into_system(take_original);
    take.initialize(world);
    let original = take.run((), world).unwrap();
    let (packets, submissions) = original.into_parts();
    assert_eq!(packets.len(), 1);
    assert_eq!(submissions.len(), 1);
    assert_eq!(
        submissions[0].submission().identity().admission_id().get(),
        73
    );
    for submission in submissions {
        world
            .resource_mut::<UiPresentationOutcomes>()
            .commit_backend(
                submission.content_rejected("the original test queue is now retired".to_owned()),
            );
    }
    assert!(
        matches!(nara_ui_render::drain_ui_presentation_outcomes(world).unwrap().as_slice(),
        [UiPresentationOutcome::ContentRejected { attempt, .. }]
            if attempt.admission_id().is_some_and(|id| id.get() == 73))
    );
}

#[cfg(feature = "ui-submitter")]
#[test]
fn blocked_ui_collection_keeps_the_existing_transaction_and_next_frame_available() {
    use nara_ui_kernel::UiSurfaceId;
    use nara_ui_render::__private::{
        UiPresentationRoute, replace_ui_backend_frame, replace_ui_presentation_routes,
        snapshot_ui_paint_packets,
    };

    let mut app = App::new();
    install_render_foundation(&mut app, false);
    app.add_plugins((
        nara_image::ImagePreparePlugin,
        nara_ui_render::UiRenderPlugin::default(),
    ))
    .unwrap();
    app.add_plugins(WgpuRenderPlugin).unwrap();

    let world = app.world_mut().unwrap();
    let surface_id = UiSurfaceId::new(1).unwrap();
    replace_ui_presentation_routes(
        world,
        [UiPresentationRoute::new(surface_id, WindowId::PRIMARY).unwrap()],
    )
    .unwrap();
    nara_ui_render::compile_ui_paint_packets(world, &[ui_outcome_frame()]).unwrap();
    let (mut planned, _) = snapshot_ui_paint_packets(world).unwrap().into_parts();
    let packet = planned.pop().unwrap();
    replace_ui_backend_frame(
        world,
        vec![Arc::clone(&packet)],
        vec![prepared_ui_attempt(packet, 71, 51)],
    )
    .unwrap();
    let existing_submission = {
        let mut outcomes = world.resource_mut::<UiPresentationOutcomes>();
        admit_ui_attempt(
            &mut outcomes,
            prepared_ui_attempt(ui_outcome_packet(), 70, 50),
        )
    };
    world
        .resource_mut::<WgpuFramePacketSlot>()
        .collect_ui(Vec::new(), vec![existing_submission]);

    let mut collect = IntoSystem::into_system(collect_wgpu_ui_payload);
    collect.initialize(world);
    collect.run((), world).unwrap();
    assert_eq!(
        nara_ui_render::drain_ui_presentation_outcomes(world).unwrap_err(),
        nara_ui_render::UiPresentationIntegrationError::OutstandingBackendOutcomeReservations {
            count: 1,
        },
        "blocked collection must preserve the existing outcome reservation",
    );

    let existing = world
        .resource_mut::<WgpuFramePacketSlot>()
        .take_ui_submissions()
        .unwrap();
    let WgpuUiSubmissionState::Prepared(existing) = existing else {
        panic!("the blocked transaction must retain its prepared state");
    };
    {
        let mut outcomes = world.resource_mut::<UiPresentationOutcomes>();
        for submission in existing {
            outcomes
                .commit_backend(submission.content_rejected("test transaction retired".to_owned()));
        }
    }
    let retired = nara_ui_render::drain_ui_presentation_outcomes(world).unwrap();
    assert!(matches!(
        retired.as_slice(),
        [UiPresentationOutcome::ContentRejected { attempt, .. }]
            if attempt.admission_id().is_some_and(|id| id.get() == 70)
    ));

    fn take_backend_frame(
        mut reader: UiBackendSubmissionReader,
    ) -> Result<
        nara_ui_render::__private::UiBackendPresentationFrame,
        nara_ui_render::UiPresentationIntegrationError,
    > {
        reader.take()
    }

    let mut take = IntoSystem::into_system(take_backend_frame);
    take.initialize(world);
    let next = take
        .run((), world)
        .unwrap()
        .expect("blocked collection must leave the next backend frame available");
    let (packets, submissions) = next.into_parts();
    assert_eq!(packets.len(), 1);
    assert!(matches!(
        submissions.as_slice(),
        [submission]
            if submission.submission().identity().admission_id().get() == 71
    ));
    {
        let mut outcomes = world.resource_mut::<UiPresentationOutcomes>();
        for submission in submissions {
            outcomes.commit_backend(submission.content_rejected("test frame retired".to_owned()));
        }
    }
    let retired = nara_ui_render::drain_ui_presentation_outcomes(world).unwrap();
    assert!(matches!(
        retired.as_slice(),
        [UiPresentationOutcome::ContentRejected { attempt, .. }]
            if attempt.admission_id().is_some_and(|id| id.get() == 71)
    ));
}

#[cfg(feature = "ui-submitter")]
#[test]
fn managed_shutdown_returns_buffered_ui_permissions_before_world_destruction() {
    use std::time::Duration;

    use nara_app::{RuntimeControl, RuntimeControlRequestResult};
    use nara_ui_render::__private::backend_outcome_receiver;

    let mut app = App::new();
    install_test_plugins(&mut app, false);
    let receiver = backend_outcome_receiver(app.world()).unwrap();
    let packet = ui_outcome_packet();
    let submission = {
        let mut outcomes = app
            .world_mut()
            .unwrap()
            .resource_mut::<UiPresentationOutcomes>();
        admit_ui_attempt(
            &mut outcomes,
            prepared_ui_attempt(Arc::clone(&packet), 72, 52),
        )
    };
    app.world_mut()
        .unwrap()
        .resource_mut::<WgpuFramePacketSlot>()
        .collect_ui(vec![packet], vec![submission]);
    assert_eq!(receiver.pending_backend_count(), 1);

    let mut runtime = admit_runtime(app).complete_startup().unwrap().promote();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while runtime.state() != RuntimeState::Stopped {
        runtime.drive(Duration::ZERO).unwrap();
        assert!(std::time::Instant::now() < deadline);
    }
    // Stopped still retains this World. Logical close, rather than World destruction,
    // must have returned the original permission held in its private frame slot.
    assert!(runtime.world().contains_resource::<WgpuFramePacketSlot>());
    assert_eq!(receiver.pending_backend_count(), 0);
    assert!(matches!(
        receiver.drain().unwrap().as_slice(),
        [UiPresentationOutcome::Aborted { attempt, .. }]
            if attempt.admission_id().is_some_and(|id| id.get() == 72)
    ));
    runtime.finish().unwrap();
}

#[test]
fn managed_frame_captures_before_local_execution_publishes_its_terminal_state() {
    let mut app = App::new();
    install_test_plugins(&mut app, false);
    let candidate = admit_runtime(app);
    let mut runtime = candidate.complete_startup().unwrap().promote();
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let handles = BackendWindowHandles::default();

    runtime.drive(std::time::Duration::ZERO).unwrap();
    assert_eq!(
        runtime.world().resource::<RenderFrame>().state,
        nara_render::RenderFrameState::Rendering,
    );
    assert_eq!(
        runtime
            .world()
            .resource::<WgpuRenderObservation>()
            .instance_id(),
        0,
    );

    let capture = take_captured_frame(&mut runtime);
    let identity = capture.identity();
    let result = host.execute_runtime_frame(&producer, &handles, capture);
    assert_eq!(result.identity(), identity);
    let completion = complete_runtime_frame(&mut runtime, result).unwrap();
    assert!(completion.post_operation_fault.is_none());
    assert!(completion.output.presented.is_none());
    assert!(!completion.output.retired);

    assert_eq!(
        runtime.world().resource::<RenderFrame>().state,
        nara_render::RenderFrameState::Skipped,
        "the original result must publish the local owner's terminal state",
    );
    assert_eq!(
        runtime
            .world()
            .resource::<RenderBackendStatus>()
            .last_skip()
            .unwrap()
            .reason(),
        RenderFrameSkipReason::NoViews,
    );
    assert_eq!(
        runtime
            .world()
            .resource::<WgpuRenderObservation>()
            .instance_id(),
        host.backend_instance_id(),
    );
    runtime.finish().unwrap();
}

#[test]
fn delayed_result_preserves_the_current_frame_and_survives_the_next_capture_reset() {
    let mut app = App::new();
    install_test_plugins(&mut app, false);
    let mut runtime = admit_runtime(app).complete_startup().unwrap().promote();
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let handles = BackendWindowHandles::default();

    runtime.drive(std::time::Duration::ZERO).unwrap();
    let completed_frame = runtime.world().resource::<RenderFrame>().index;
    let capture = take_captured_frame(&mut runtime);
    let result = host.execute_runtime_frame(&producer, &handles, capture);

    runtime.drive(std::time::Duration::ZERO).unwrap();
    let current_frame = *runtime.world().resource::<RenderFrame>();
    assert!(current_frame.index > completed_frame);
    let completion = complete_runtime_frame(&mut runtime, result).unwrap();
    assert!(completion.post_operation_fault.is_none());
    assert_eq!(*runtime.world().resource::<RenderFrame>(), current_frame);
    let completed = runtime
        .world()
        .resource::<WgpuRenderObservation>()
        .frame_transaction_stats();
    assert_eq!(
        completed.render_transaction_frame_index(),
        Some(completed_frame),
    );

    runtime.drive(std::time::Duration::ZERO).unwrap();
    let next_frame = runtime.world().resource::<RenderFrame>().index;
    assert!(next_frame > current_frame.index);
    assert_eq!(
        runtime
            .world()
            .resource::<WgpuRenderObservation>()
            .frame_transaction_stats(),
        completed,
        "a new CPU capture must preserve the last completed native observation",
    );
    let capture = take_captured_frame(&mut runtime);
    let result = host.execute_runtime_frame(&producer, &handles, capture);
    assert_eq!(
        result
            .observation()
            .unwrap()
            .frame_transaction_stats()
            .render_transaction_frame_index(),
        Some(next_frame),
    );
    let completion = complete_runtime_frame(&mut runtime, result).unwrap();
    assert!(completion.post_operation_fault.is_none());
    assert_eq!(runtime.world().resource::<RenderFrame>().index, next_frame);
    assert_eq!(
        runtime.world().resource::<RenderFrame>().state,
        nara_render::RenderFrameState::Skipped,
    );
    runtime.finish().unwrap();
}

#[test]
fn exact_fixed_step_does_not_require_app_and_render_frame_counters_to_match() {
    let mut app = App::new();
    install_test_plugins(&mut app, true);
    app.world_mut().unwrap().spawn((
        nara_render::Camera2d::default(),
        nara_transform::Transform2d::default(),
    ));
    let candidate = admit_runtime(app);
    let mut runtime = candidate.complete_startup().unwrap().promote();
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let handles = BackendWindowHandles::default();

    runtime.drive(std::time::Duration::ZERO).unwrap();
    let capture = take_captured_frame(&mut runtime);
    let result = host.execute_runtime_frame(&producer, &handles, capture);
    let completion = complete_runtime_frame(&mut runtime, result).unwrap();
    assert!(completion.post_operation_fault.is_none());
    assert!(matches!(
        runtime.request_control(nara_app::RuntimeControl::Pause),
        nara_app::RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(std::time::Duration::ZERO).unwrap();
    let capture = take_captured_frame(&mut runtime);
    let result = host.execute_runtime_frame(&producer, &handles, capture);
    let completion = complete_runtime_frame(&mut runtime, result).unwrap();
    assert!(completion.post_operation_fault.is_none());
    assert!(matches!(
        runtime.request_control(nara_app::RuntimeControl::StepFixedTick),
        nara_app::RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(std::time::Duration::ZERO).unwrap();
    runtime.drive(std::time::Duration::ZERO).unwrap();

    let app_frame = runtime.world().resource::<FrameExecutionStart>().frame();
    let render_frame = runtime.world().resource::<RenderFrame>().index;
    assert_eq!(app_frame, render_frame + 1);
    assert_eq!(runtime.state(), RuntimeState::Paused);
    assert_eq!(runtime.fault(), None);
    let capture = take_captured_frame(&mut runtime);
    assert_eq!(
        capture.identity().runtime_generation(),
        runtime.generation()
    );
    let result = host.execute_runtime_frame(&producer, &handles, capture);
    let transaction = result.observation().unwrap().frame_transaction_stats();
    assert_eq!(transaction.app_frame_index(), Some(app_frame));
    assert_eq!(
        transaction.render_transaction_frame_index(),
        Some(render_frame),
    );
    assert_eq!(transaction.packet_admissions(), 1);
    assert_eq!(transaction.packet_rejections(), 0);
    let completion = complete_runtime_frame(&mut runtime, result).unwrap();
    assert!(completion.post_operation_fault.is_none());
    assert!(completion.output.presented.is_none());
    assert!(!completion.output.retired);
    assert_eq!(runtime.fault(), None);
    runtime.finish().unwrap();
}

#[test]
fn captured_topology_is_immutable_and_next_frame_rejects_before_surface_work() {
    let mut app = App::new();
    install_test_plugins(&mut app, true);
    app.insert_resource(SecondCameraInjection::default())
        .unwrap();
    app.world_mut().unwrap().spawn((
        nara_render::Camera2d::default(),
        nara_transform::Transform2d::default(),
    ));
    app.add_systems(
        CoreStage::Render,
        inject_second_camera_after_capture.after(capture_wgpu_frame_packet),
    )
    .unwrap();
    let candidate = admit_runtime(app);
    let mut runtime = candidate.complete_startup().unwrap().promote();
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let handles = BackendWindowHandles::default();

    runtime.drive(std::time::Duration::ZERO).unwrap();
    let first_frame = *runtime.world().resource::<RenderFrame>();
    let capture = take_captured_frame(&mut runtime);
    let result = host.execute_runtime_frame(&producer, &handles, capture);
    let first = result.observation().unwrap().frame_transaction_stats();
    let completion = complete_runtime_frame(&mut runtime, result).unwrap();
    assert!(completion.post_operation_fault.is_none());
    assert_eq!(
        first.render_transaction_frame_index(),
        Some(first_frame.index)
    );
    assert_eq!(first.packet_admissions(), 1);
    assert_eq!(first.packet_rejections(), 0);
    assert_eq!(first.surface_acquire_attempts(), 0);
    assert_eq!(first.surface_acquires(), 0);
    assert_eq!(first.queue_submissions(), 0);
    assert_eq!(first.presents(), 0);

    runtime.drive(std::time::Duration::ZERO).unwrap();
    let rejected_frame = *runtime.world().resource::<RenderFrame>();
    let capture = take_captured_frame(&mut runtime);
    let result = host.execute_runtime_frame(&producer, &handles, capture);
    let rejected = result.observation().unwrap().frame_transaction_stats();
    assert_eq!(
        runtime.fault(),
        None,
        "local execution has not returned its fault yet",
    );
    let completion = complete_runtime_frame(&mut runtime, result).unwrap();
    let fault = completion
        .post_operation_fault
        .expect("the returned rejection must retain its Runtime fault");
    assert_eq!(fault.kind(), RuntimeFaultKind::System);
    assert_eq!(fault.source(), "nara.render.frame-packet");
    assert!(completion.output.presented.is_none());
    assert_eq!(
        rejected.render_transaction_frame_index(),
        Some(rejected_frame.index)
    );
    assert_eq!(rejected.packet_admissions(), 0);
    assert_eq!(rejected.packet_rejections(), 1);
    assert_eq!(rejected.surface_acquire_attempts(), 0);
    assert_eq!(rejected.surface_acquires(), 0);
    assert_eq!(rejected.queue_submissions(), 0);
    assert_eq!(rejected.presents(), 0);
    assert_eq!(
        runtime
            .world()
            .resource::<WgpuRenderObservation>()
            .frame_transaction_stats(),
        rejected,
        "a post-operation fault must retain the result already consumed by its port",
    );
    runtime.finish().unwrap();
}

#[test]
fn surface_policy_distinguishes_resize_reconfigure_and_loss() {
    let current = Extent2d::new(320, 180).unwrap();
    let resized = Extent2d::new(640, 360).unwrap();

    assert_eq!(
        surface_resize_action(
            current,
            Extent2d {
                width: 0,
                height: 180,
            },
        ),
        SurfaceResizeAction::SkipZeroSized
    );
    assert_eq!(
        surface_resize_action(current, current),
        SurfaceResizeAction::Unchanged
    );
    assert_eq!(
        surface_resize_action(current, resized),
        SurfaceResizeAction::Reconfigure(resized)
    );
    assert_eq!(
        surface_acquire_policy(SurfaceTextureStatus::Outdated),
        SurfaceAcquireAction::Reconfigure
    );
    assert_eq!(
        surface_acquire_policy(SurfaceTextureStatus::Lost),
        SurfaceAcquireAction::RecreateSurface
    );
}

#[test]
fn configured_present_mode_reports_the_exact_wgpu_mode() {
    let cases = [
        (
            wgpu::PresentMode::AutoVsync,
            WgpuConfiguredPresentMode::AutoVsync,
            "auto-vsync",
        ),
        (
            wgpu::PresentMode::AutoNoVsync,
            WgpuConfiguredPresentMode::AutoNoVsync,
            "auto-no-vsync",
        ),
        (
            wgpu::PresentMode::Fifo,
            WgpuConfiguredPresentMode::Fifo,
            "fifo",
        ),
        (
            wgpu::PresentMode::FifoRelaxed,
            WgpuConfiguredPresentMode::FifoRelaxed,
            "fifo-relaxed",
        ),
        (
            wgpu::PresentMode::Immediate,
            WgpuConfiguredPresentMode::Immediate,
            "immediate",
        ),
        (
            wgpu::PresentMode::Mailbox,
            WgpuConfiguredPresentMode::Mailbox,
            "mailbox",
        ),
    ];

    for (wgpu_mode, expected, label) in cases {
        let configured = WgpuConfiguredPresentMode::from_wgpu(wgpu_mode);
        assert_eq!(configured, expected);
        assert_eq!(configured.as_str(), label);
    }
}

#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
#[test]
fn enabled_submitter_input_has_no_quad_work() {
    assert!(
        frame::WgpuFramePayload::default()
            .lower_draw(
                RenderTarget::PrimaryWindow,
                ViewportRect::new(0, 0, 1, 1).unwrap(),
                None,
            )
            .unwrap()
            .batches
            .is_empty()
    );
}

#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
#[test]
fn closed_first_party_plan_orders_content_before_ui() {
    let content = WgpuQuadBatch {
        material: crate::quad::WgpuQuadMaterialKey {
            texture: crate::quad::WgpuQuadTextureKey::Fallback,
            sampler: nara_material::SamplerDescriptor::default(),
            alpha_mode: nara_material::AlphaMode2d::Blend,
        },
        texture: crate::quad::WgpuQuadTexture::Fallback,
        scissor: None,
        instances: Vec::new(),
        counts_as_sprites: true,
        material_extension: None,
    };
    let ui = WgpuQuadBatch {
        counts_as_sprites: false,
        ..content.clone()
    };

    let ordered = nara_render::__private::assemble_first_party_draws(vec![content], vec![ui]);

    assert!(ordered[0].counts_as_sprites);
    assert!(!ordered[1].counts_as_sprites);
}

#[test]
fn upload_deferral_skips_one_frame_without_poisoning_the_backend() {
    let mut backend = WgpuRenderBackend::default();
    let initial_backend_state = backend.state();
    let mut status = RenderBackendStatus::default();
    status.mark_ready(WGPU_RENDER_BACKEND);

    let fault = handle_wgpu_render_error(
        &mut backend,
        WgpuRenderError::UploadDeferred {
            message: "glyph upload budget exhausted".to_owned(),
        },
        7,
        &mut status,
    );

    assert_eq!(fault, None);
    assert_eq!(backend.state(), initial_backend_state);
    assert_eq!(status.state(), RenderBackendState::Ready);
    assert_eq!(status.last_error(), None);
    let skip = status.last_skip().unwrap();
    assert_eq!(skip.frame_index(), 7);
    assert_eq!(skip.reason(), RenderFrameSkipReason::Deferred);
}

#[test]
fn rejected_content_skips_one_frame_without_poisoning_the_backend() {
    let mut backend = WgpuRenderBackend::default();
    let initial_backend_state = backend.state();
    let mut status = RenderBackendStatus::default();
    status.mark_ready(WGPU_RENDER_BACKEND);

    let fault = handle_wgpu_render_error(
        &mut backend,
        WgpuRenderError::ContentRejected {
            message: "quad instance capacity exceeded".to_owned(),
        },
        11,
        &mut status,
    );

    assert_eq!(fault, None);
    assert_eq!(backend.state(), initial_backend_state);
    assert_eq!(status.state(), RenderBackendState::Ready);
    assert_eq!(status.last_error(), None);
    let skip = status.last_skip().unwrap();
    assert_eq!(skip.frame_index(), 11);
    assert_eq!(skip.reason(), RenderFrameSkipReason::ContentRejected);
}

#[test]
fn target_fault_does_not_retire_the_backend_or_device_generation() {
    let mut backend = WgpuRenderBackend::default();
    let instance_id = backend.instance_id();
    let device_epoch = backend.device_epoch();
    let initial_state = backend.state();
    let mut status = RenderBackendStatus::default();
    status.mark_ready(WGPU_RENDER_BACKEND);

    let fault = handle_wgpu_render_error(
        &mut backend,
        WgpuRenderError::SurfaceMissing {
            window_id: WindowId::PRIMARY,
        },
        12,
        &mut status,
    );

    assert_eq!(fault, None);
    assert_eq!(backend.instance_id(), instance_id);
    assert_eq!(backend.device_epoch(), device_epoch);
    assert_eq!(backend.state(), initial_state);
    assert_eq!(status.last_error(), None);
    assert_eq!(
        status.last_skip().unwrap().reason(),
        RenderFrameSkipReason::SurfaceUnavailable
    );
}

#[cfg(feature = "ui-submitter")]
#[test]
fn impossible_ui_packet_state_is_scoped_to_the_attempt() {
    let mut backend = WgpuRenderBackend::default();
    let mut status = RenderBackendStatus::default();
    status.mark_ready(WGPU_RENDER_BACKEND);

    let fault = handle_wgpu_render_error(
        &mut backend,
        WgpuRenderError::UiPacketInvariant {
            message: "missing planner-owned image index".to_owned(),
        },
        12,
        &mut status,
    );

    let fault = fault.expect("the local attempt fault must travel with its returned result");
    assert_eq!(fault.kind(), RuntimeFaultKind::System);
    assert_eq!(fault.source(), "nara.render.frame-packet");
    assert_eq!(backend.state(), WgpuBackendState::Uninitialized);
    assert_eq!(status.state(), RenderBackendState::Ready);
    assert_eq!(status.last_error(), None);
    let skip = status.last_skip().unwrap();
    assert_eq!(skip.frame_index(), 12);
    assert_eq!(skip.reason(), RenderFrameSkipReason::InvalidTopology);
}

#[cfg(feature = "ui-submitter")]
#[test]
fn ui_outcomes_bind_submission_and_retry_to_the_exact_backend_generation() {
    let packet = ui_outcome_packet();
    let backend_generation = UiPresentationBackendGeneration::new(17, 3).unwrap();
    let mut outcomes = UiPresentationOutcomes::default();
    let submitted = submitted_ui_attempt(
        &mut outcomes,
        Arc::clone(&packet),
        41,
        29,
        backend_generation,
    );

    frame::publish_ui_outcomes(
        &mut outcomes,
        WgpuUiSubmissionState::Submitted(vec![submitted]),
        &Ok(()),
        RenderFrameState::Submitted,
        None,
        None,
    );
    let mut world = nara_ecs::World::new();
    world.insert_resource(outcomes);
    let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
    let UiPresentationOutcome::Presented(commit) = published.remove(0) else {
        panic!("a submitted packet must issue one presentation commit");
    };
    assert_eq!(commit.attempt().admission_id().unwrap().get(), 41);
    assert_eq!(commit.presentation_epoch().get(), 29);
    assert_eq!(commit.backend_generation(), backend_generation);
    let accepted = nara_ui_render::__private::discard_ui_backend_acceptance(commit);
    assert!(Arc::ptr_eq(accepted.packet(), &packet));

    let mut outcomes = UiPresentationOutcomes::default();
    let deferred = Err(WgpuRenderError::UploadDeferred {
        message: "shared upload budget exhausted".to_owned(),
    });
    let submitted = submitted_ui_attempt(
        &mut outcomes,
        Arc::clone(&packet),
        42,
        30,
        backend_generation,
    );
    frame::publish_ui_outcomes(
        &mut outcomes,
        WgpuUiSubmissionState::Submitted(vec![submitted]),
        &deferred,
        RenderFrameState::Skipped,
        None,
        None,
    );
    let mut world = nara_ecs::World::new();
    world.insert_resource(outcomes);
    let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
    let UiPresentationOutcome::Deferred { retry, .. } = published.remove(0) else {
        panic!("retryable upload work must retain an exact retry identity");
    };
    assert_eq!(retry.attempt().admission_id().unwrap().get(), 42);
    assert_eq!(retry.backend_generation(), backend_generation);
    let retry = nara_ui_render::__private::retry_deferred(retry);
    assert_eq!(
        retry.required_backend_generation(),
        Some(backend_generation)
    );
    let mut retry_outcomes = UiPresentationOutcomes::default();
    let mut retry_state =
        WgpuUiSubmissionState::Prepared(vec![admit_ui_attempt(&mut retry_outcomes, retry)]);
    assert!(matches!(
        retry_state.bind_backend(UiPresentationBackendGeneration::new(17, 4).unwrap()),
        Err(WgpuRenderError::UiSubmissionRejected { .. })
    ));
    retry_state
        .bind_backend(backend_generation)
        .expect("a rejected generation bind must retain the exact retry authority");
    assert!(matches!(retry_state, WgpuUiSubmissionState::Submitted(_)));

    let mut outcomes = UiPresentationOutcomes::default();
    let prepared = prepared_ui_attempt(packet, 43, 31);
    let prepared = admit_ui_attempt(&mut outcomes, prepared);
    frame::publish_ui_outcomes(
        &mut outcomes,
        WgpuUiSubmissionState::Prepared(vec![prepared]),
        &Ok(()),
        RenderFrameState::Skipped,
        None,
        Some("the surface is unavailable"),
    );
    let mut world = nara_ecs::World::new();
    world.insert_resource(outcomes);
    let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
    assert!(matches!(
        published.remove(0),
        UiPresentationOutcome::BackendFault {
            backend_generation: None,
            domain: UiPresentationBackendFaultDomain::Target,
            ..
        }
    ));
}

#[cfg(feature = "ui-submitter")]
#[test]
fn ui_outcomes_preserve_exact_fault_domains_for_prepared_and_submitted_attempts() {
    let packet = ui_outcome_packet();
    let backend_generation = UiPresentationBackendGeneration::new(17, 3).unwrap();

    let mut outcomes = UiPresentationOutcomes::default();
    let prepared = admit_ui_attempt(
        &mut outcomes,
        prepared_ui_attempt(Arc::clone(&packet), 44, 32),
    );
    frame::publish_ui_outcomes(
        &mut outcomes,
        WgpuUiSubmissionState::Prepared(vec![prepared]),
        &Err(WgpuRenderError::ContentRejected {
            message: "invalid content".to_owned(),
        }),
        RenderFrameState::Skipped,
        None,
        None,
    );
    let mut world = nara_ecs::World::new();
    world.insert_resource(outcomes);
    let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
    assert!(matches!(
        published.remove(0),
        UiPresentationOutcome::ContentRejected { attempt, .. }
            if attempt.admission_id().is_some_and(|id| id.get() == 44)
    ));
    assert!(published.is_empty());

    let mut outcomes = UiPresentationOutcomes::default();
    let prepared = admit_ui_attempt(
        &mut outcomes,
        prepared_ui_attempt(Arc::clone(&packet), 45, 33),
    );
    frame::publish_ui_outcomes(
        &mut outcomes,
        WgpuUiSubmissionState::Prepared(vec![prepared]),
        &Err(WgpuRenderError::DeviceLost),
        RenderFrameState::Skipped,
        Some(backend_generation),
        None,
    );
    let mut world = nara_ecs::World::new();
    world.insert_resource(outcomes);
    let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
    assert!(matches!(
        published.remove(0),
        UiPresentationOutcome::BackendFault {
            attempt: Some(attempt),
            backend_generation: Some(generation),
            domain: UiPresentationBackendFaultDomain::Device,
            ..
        } if attempt.admission_id().is_some_and(|id| id.get() == 45)
            && generation == backend_generation
    ));
    assert!(published.is_empty());

    let mut outcomes = UiPresentationOutcomes::default();
    let submitted = submitted_ui_attempt(
        &mut outcomes,
        Arc::clone(&packet),
        46,
        34,
        backend_generation,
    );
    frame::publish_ui_outcomes(
        &mut outcomes,
        WgpuUiSubmissionState::Submitted(vec![submitted]),
        &Err(WgpuRenderError::SurfaceMissing {
            window_id: WindowId::PRIMARY,
        }),
        RenderFrameState::Skipped,
        None,
        None,
    );
    let mut world = nara_ecs::World::new();
    world.insert_resource(outcomes);
    let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
    assert!(matches!(
        published.remove(0),
        UiPresentationOutcome::BackendFault {
            attempt: Some(attempt),
            backend_generation: Some(generation),
            domain: UiPresentationBackendFaultDomain::Target,
            ..
        } if attempt.admission_id().is_some_and(|id| id.get() == 46)
            && generation == backend_generation
    ));
    assert!(published.is_empty());

    let mut outcomes = UiPresentationOutcomes::default();
    let submitted = submitted_ui_attempt(
        &mut outcomes,
        Arc::clone(&packet),
        47,
        35,
        backend_generation,
    );
    frame::publish_ui_outcomes(
        &mut outcomes,
        WgpuUiSubmissionState::Submitted(vec![submitted]),
        &Err(WgpuRenderError::ContentRejected {
            message: "late content rejection".to_owned(),
        }),
        RenderFrameState::Skipped,
        None,
        None,
    );
    let mut world = nara_ecs::World::new();
    world.insert_resource(outcomes);
    let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
    assert!(matches!(
        published.remove(0),
        UiPresentationOutcome::BackendFault {
            attempt: Some(attempt),
            backend_generation: Some(generation),
            domain: UiPresentationBackendFaultDomain::Attempt,
            ..
        } if attempt.admission_id().is_some_and(|id| id.get() == 47)
            && generation == backend_generation
    ));
    assert!(published.is_empty());

    let mut outcomes = UiPresentationOutcomes::default();
    let prepared = admit_ui_attempt(
        &mut outcomes,
        prepared_ui_attempt(Arc::clone(&packet), 48, 36),
    );
    frame::publish_ui_outcomes(
        &mut outcomes,
        WgpuUiSubmissionState::Prepared(vec![prepared]),
        &Err(WgpuRenderError::UiSubmissionRejected {
            message: "prepared submission mismatch".to_owned(),
        }),
        RenderFrameState::Skipped,
        Some(backend_generation),
        None,
    );
    let mut world = nara_ecs::World::new();
    world.insert_resource(outcomes);
    let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
    assert!(matches!(
        published.remove(0),
        UiPresentationOutcome::BackendFault {
            attempt: Some(attempt),
            backend_generation: None,
            domain: UiPresentationBackendFaultDomain::Attempt,
            ..
        } if attempt.admission_id().is_some_and(|id| id.get() == 48)
    ));
    assert!(published.is_empty());

    let mut outcomes = UiPresentationOutcomes::default();
    let submitted = submitted_ui_attempt(
        &mut outcomes,
        Arc::clone(&packet),
        49,
        37,
        backend_generation,
    );
    frame::publish_ui_outcomes(
        &mut outcomes,
        WgpuUiSubmissionState::Submitted(vec![submitted]),
        &Ok(()),
        RenderFrameState::Skipped,
        None,
        Some("surface submit was skipped"),
    );
    let mut world = nara_ecs::World::new();
    world.insert_resource(outcomes);
    let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
    assert!(matches!(
        published.remove(0),
        UiPresentationOutcome::BackendFault {
            attempt: Some(attempt),
            backend_generation: Some(generation),
            domain: UiPresentationBackendFaultDomain::Target,
            ..
        } if attempt.admission_id().is_some_and(|id| id.get() == 49)
            && generation == backend_generation
    ));
    assert!(published.is_empty());

    let mut outcomes = UiPresentationOutcomes::default();
    let submitted = submitted_ui_attempt(&mut outcomes, packet, 50, 38, backend_generation);
    frame::publish_ui_outcomes(
        &mut outcomes,
        WgpuUiSubmissionState::Submitted(vec![submitted]),
        &Err(WgpuRenderError::BackendUnavailable),
        RenderFrameState::Skipped,
        None,
        None,
    );
    let mut world = nara_ecs::World::new();
    world.insert_resource(outcomes);
    let mut published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
    assert!(matches!(
        published.remove(0),
        UiPresentationOutcome::BackendFault {
            attempt: Some(attempt),
            backend_generation: Some(generation),
            domain: UiPresentationBackendFaultDomain::Device,
            ..
        } if attempt.admission_id().is_some_and(|id| id.get() == 50)
            && generation == backend_generation
    ));
    assert!(published.is_empty());
}

#[cfg(feature = "ui-submitter")]
pub(super) fn prepared_ui_attempt(
    packet: Arc<UiPaintPacket>,
    admission_id: u64,
    presentation_epoch: u64,
) -> UiPreparedBackendSubmission {
    nara_ui_render::__private::prepare_backend_submission(
        UiPresentationAttemptId::new(admission_id).unwrap(),
        packet,
        UiPresentationEpoch::new(presentation_epoch).unwrap(),
        UiPresentationRuntimeGeneration::new(7).unwrap(),
        UiPresentationCapabilityGeneration::new(11).unwrap(),
    )
}

#[cfg(feature = "ui-submitter")]
fn submitted_ui_attempt(
    outcomes: &mut UiPresentationOutcomes,
    packet: Arc<UiPaintPacket>,
    admission_id: u64,
    presentation_epoch: u64,
    backend_generation: UiPresentationBackendGeneration,
) -> UiSubmittedBackendSubmission {
    let admitted = admit_ui_attempt(
        outcomes,
        prepared_ui_attempt(packet, admission_id, presentation_epoch),
    );
    nara_ui_render::__private::bind_backend_submission_batch(vec![admitted], backend_generation)
        .unwrap()
        .remove(0)
}

#[cfg(feature = "ui-submitter")]
pub(super) fn admit_ui_attempt(
    outcomes: &mut UiPresentationOutcomes,
    submission: UiPreparedBackendSubmission,
) -> UiBackendSubmission {
    let mut permits = outcomes
        .reserve_backend(std::slice::from_ref(&submission))
        .unwrap();
    UiBackendSubmission::pair(submission, permits.remove(0)).unwrap()
}

#[cfg(feature = "ui-submitter")]
fn ui_outcome_frame() -> Arc<nara_ui_kernel::UiFrame> {
    use nara_text::bundled_font_face;
    use nara_ui_kernel::{
        UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiLayer, UiLocalKey,
        UiNode, UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiText, UiTheme, UiVisual,
    };

    let snapshot = UiAuthoringSnapshotBuilder::new(1)
        .with_root(
            UiRoot::new(
                UiRootKey::runtime("wgpu-outcome", "root").unwrap(),
                UiLayer::Base,
                UiStyle::fill(),
            )
            .with_child(
                UiNode::new(UiStyle::absolute(0.0, 0.0, 16.0, 16.0))
                    .with_key(UiLocalKey::runtime("wgpu-outcome", "node").unwrap())
                    .with_visual(UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0)))
                    .with_text(UiText::new("UI", 12.0).unwrap()),
            ),
        )
        .build()
        .unwrap();
    let environment =
        UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 16.0, 16.0, 1.0, 1, 1, 1, 1).unwrap();
    UiCompiler::new(bundled_font_face().unwrap(), Default::default())
        .unwrap()
        .compile(&snapshot, environment, UiTheme::default())
        .unwrap()
}

#[cfg(feature = "ui-submitter")]
fn ui_outcome_packet_for(target: RenderTarget) -> Arc<UiPaintPacket> {
    nara_ui_render::UiPresentationPlanner::default()
        .plan(
            ui_outcome_frame(),
            target,
            &nara_ui_render::UiPreparedImages::default(),
        )
        .unwrap()
}

#[cfg(feature = "ui-submitter")]
pub(super) fn ui_outcome_packet() -> Arc<UiPaintPacket> {
    ui_outcome_packet_for(RenderTarget::Window(WindowId::PRIMARY))
}

#[cfg(feature = "ui-submitter")]
#[test]
fn payload_facts_bind_glyph_work_to_one_exact_ui_packet() {
    let packet = ui_outcome_packet();
    let identity = packet.identity();
    let payload = frame::WgpuFramePayload {
        primary_window_id: Some(WindowId::PRIMARY),
        ui_logical_target: None,
        #[cfg(feature = "sprite-submitter")]
        sprite_batches: None,
        ui_packets: vec![packet],
    };

    let facts = payload.ui_facts(RenderTarget::Window(WindowId::PRIMARY));

    assert_eq!(facts.packet_count, 1);
    assert_eq!(facts.packet_identity, Some(identity));
    assert!(facts.primitives > 0);
    assert!(facts.glyph_primitives > 0);
    assert!(facts.glyph_pages > 0);
}

#[cfg(feature = "ui-submitter")]
#[test]
fn payload_facts_follow_the_same_window_target_as_the_submitter() {
    let first_target = RenderTarget::Window(WindowId::new(101));
    let second_target = RenderTarget::Window(WindowId::new(102));
    let first_packet = ui_outcome_packet_for(first_target);
    let first_identity = first_packet.identity();
    let second_packet = ui_outcome_packet_for(second_target);
    let payload = frame::WgpuFramePayload {
        primary_window_id: Some(WindowId::PRIMARY),
        ui_logical_target: None,
        #[cfg(feature = "sprite-submitter")]
        sprite_batches: None,
        ui_packets: vec![first_packet, second_packet],
    };

    let facts = payload.ui_facts(first_target);

    assert_eq!(facts.packet_count, 1);
    assert_eq!(facts.packet_identity, Some(first_identity));
    assert!(facts.primitives > 0);
    assert!(facts.glyph_primitives > 0);
    assert!(facts.glyph_pages > 0);
}

#[cfg(feature = "ui-submitter")]
#[test]
fn editor_viewport_maps_logical_primary_window_ui_to_the_private_image() {
    let packet = ui_outcome_packet();
    let packet_identity = packet.identity();
    let image_target = RenderTarget::Image(nara_render::RenderImage2d::allocate_handle().unwrap());
    let payload = frame::WgpuFramePayload::for_editor_runtime_ui(vec![packet]);
    let viewport = ViewportRect::new(0, 0, 16, 16).unwrap();

    let facts = payload.ui_facts(image_target);
    let lowered = payload.lower_draw(image_target, viewport, None).unwrap();

    assert_eq!(facts.packet_count, 1);
    assert_eq!(facts.packet_identity, Some(packet_identity));
    assert_eq!(lowered.ui_packets.len(), 1);
    assert_eq!(lowered.ui_packets[0].packet, packet_identity);
    assert_eq!(
        lowered.ui_packets[0].target,
        RenderTarget::Window(WindowId::PRIMARY)
    );
}

#[cfg(feature = "ui-submitter")]
#[test]
fn submitted_packet_facts_preserve_distinct_app_and_render_identities() {
    let packet = ui_outcome_packet();
    let lowering = crate::ui::lower_ui_packets(
        std::slice::from_ref(&packet),
        RenderTarget::Window(WindowId::PRIMARY),
        Some(WindowId::PRIMARY),
        ViewportRect::new(0, 0, 16, 16).unwrap(),
    )
    .unwrap();
    let prepared = lowering.packet_facts;
    let mut outcomes = UiPresentationOutcomes::default();
    let backend_generation = UiPresentationBackendGeneration::new(77, 3).unwrap();
    let submitted = submitted_ui_attempt(
        &mut outcomes,
        Arc::clone(&packet),
        81,
        91,
        backend_generation,
    );
    let state = frame::WgpuUiSubmissionState::Submitted(vec![submitted]);
    let bound = state
        .bound_submissions()
        .first()
        .expect("one submitted attempt should remain available");
    let mut backend = WgpuRenderBackend::default();
    let facts = WgpuRenderBackend::prepare_submitted_ui_facts(
        state.bound_submissions(),
        &prepared,
        backend::WgpuSubmittedFrameFacts::new(10, 9, 4, 3, 4096, 2048),
    )
    .unwrap();
    backend.publish_submitted_ui_facts(facts);
    let facts = backend
        .submitted_ui_facts(bound.submission().identity(), backend_generation)
        .expect("the exact submitted packet should retain its first-pass lowering facts");

    assert_eq!(facts.submission(), bound.submission().identity());
    assert_eq!(facts.backend_generation(), backend_generation);
    assert_eq!(facts.app_frame_index(), 10);
    assert_eq!(facts.render_transaction_frame_index(), 9);
    assert!(facts.primitive_count() > 0);
    assert!(facts.glyph_primitive_count() > 0);
    assert!(facts.glyph_page_count() > 0);
    assert!(facts.submitted_quad_count() > 0);
    assert!(facts.submitted_glyph_quad_count() > 0);
    assert!(facts.referenced_texture_binding_count() > 0);
    assert_eq!(facts.frame_submitted_sprite_count(), 4);
    assert_eq!(facts.frame_submitted_image_sprite_count(), 3);
    assert_eq!(facts.submitted_texture_bytes(), 4096);
    assert_eq!(facts.submitted_instance_buffer_bytes(), 2048);
    assert_eq!(facts.submitted_total_bytes(), 6144);
}

#[cfg(feature = "ui-submitter")]
#[test]
fn submitted_facts_remain_bound_to_each_exact_submission() {
    let packet = ui_outcome_packet();
    let lowering = crate::ui::lower_ui_packets(
        std::slice::from_ref(&packet),
        RenderTarget::Window(WindowId::PRIMARY),
        Some(WindowId::PRIMARY),
        ViewportRect::new(0, 0, 16, 16).unwrap(),
    )
    .unwrap();
    let mut outcomes = UiPresentationOutcomes::default();
    let backend_generation = UiPresentationBackendGeneration::new(77, 3).unwrap();
    let prepared = vec![
        prepared_ui_attempt(Arc::clone(&packet), 81, 91),
        prepared_ui_attempt(Arc::clone(&packet), 82, 92),
    ];
    let mut permits = outcomes.reserve_backend(&prepared).unwrap();
    let admitted = prepared
        .into_iter()
        .zip(permits.drain(..))
        .map(|(submission, permit)| UiBackendSubmission::pair(submission, permit).unwrap())
        .collect::<Vec<_>>();
    let submitted =
        nara_ui_render::__private::bind_backend_submission_batch(admitted, backend_generation)
            .unwrap();
    let first_identity = submitted[0].submission().identity();
    let second_identity = submitted[1].submission().identity();
    let mut backend = WgpuRenderBackend::default();
    backend.begin_frame_transaction(9, Some(10));
    let facts = WgpuRenderBackend::prepare_submitted_ui_facts(
        &submitted,
        &lowering.packet_facts,
        backend::WgpuSubmittedFrameFacts::new(10, 9, 4, 3, 4096, 2048),
    )
    .unwrap();
    backend.publish_submitted_ui_facts(facts);

    let first = backend
        .submitted_ui_facts(first_identity, backend_generation)
        .expect("the earlier exact submission should remain queryable");
    let second = backend
        .submitted_ui_facts(second_identity, backend_generation)
        .expect("the later exact submission should remain queryable");

    assert_eq!(first.submission(), first_identity);
    assert_eq!(first.app_frame_index(), 10);
    assert_eq!(first.render_transaction_frame_index(), 9);
    assert_eq!(first.frame_submitted_sprite_count(), 4);
    assert_eq!(first.frame_submitted_image_sprite_count(), 3);
    assert_eq!(second.submission(), second_identity);
    assert_eq!(second.app_frame_index(), 10);
    assert_eq!(second.render_transaction_frame_index(), 9);
    assert_eq!(second.frame_submitted_sprite_count(), 4);
    assert_eq!(second.frame_submitted_image_sprite_count(), 3);

    let foreign_generation = UiPresentationBackendGeneration::new(77, 4).unwrap();
    assert!(
        backend
            .submitted_ui_facts(first_identity, foreign_generation)
            .is_none()
    );
    assert!(
        backend
            .submitted_ui_facts(second_identity, foreign_generation)
            .is_none()
    );
}

#[cfg(feature = "ui-submitter")]
#[test]
fn ui_submission_lowering_mismatch_is_rejected_before_submission() {
    let packet = ui_outcome_packet();
    let mut outcomes = UiPresentationOutcomes::default();
    let backend_generation = UiPresentationBackendGeneration::new(77, 3).unwrap();
    let submitted = submitted_ui_attempt(
        &mut outcomes,
        Arc::clone(&packet),
        81,
        91,
        backend_generation,
    );
    let submissions = vec![submitted];

    let error = WgpuRenderBackend::prepare_submitted_ui_facts(
        &submissions,
        &[],
        backend::WgpuSubmittedFrameFacts::new(10, 9, 4, 3, 4096, 2048),
    )
    .expect_err("a bound submission without an exact lowered packet must fail closed");

    assert!(matches!(
        error,
        WgpuRenderError::UiSubmissionRejected { .. }
    ));
}

#[cfg(feature = "ui-submitter")]
#[test]
fn duplicate_lowered_packet_facts_are_rejected_before_submission() {
    let packet = ui_outcome_packet();
    let lowering = crate::ui::lower_ui_packets(
        &[Arc::clone(&packet), Arc::clone(&packet)],
        RenderTarget::Window(WindowId::PRIMARY),
        Some(WindowId::PRIMARY),
        ViewportRect::new(0, 0, 16, 16).unwrap(),
    )
    .unwrap();
    let mut outcomes = UiPresentationOutcomes::default();
    let backend_generation = UiPresentationBackendGeneration::new(77, 3).unwrap();
    let submitted = submitted_ui_attempt(&mut outcomes, packet, 81, 91, backend_generation);
    let submissions = vec![submitted];

    let error = WgpuRenderBackend::prepare_submitted_ui_facts(
        &submissions,
        &lowering.packet_facts,
        backend::WgpuSubmittedFrameFacts::new(10, 9, 4, 3, 4096, 2048),
    )
    .expect_err("duplicate lowered packet facts must fail closed");

    assert!(matches!(
        error,
        WgpuRenderError::UiSubmissionRejected { .. }
    ));
}

#[cfg(feature = "ui-submitter")]
#[test]
fn lowered_packet_for_a_foreign_target_is_rejected_before_submission() {
    let submitted_target = RenderTarget::Window(WindowId::new(101));
    let foreign_target = RenderTarget::Window(WindowId::new(102));
    let packet = ui_outcome_packet_for(submitted_target);
    let foreign_packet = ui_outcome_packet_for(foreign_target);
    let lowering = crate::ui::lower_ui_packets(
        std::slice::from_ref(&foreign_packet),
        foreign_target,
        Some(WindowId::PRIMARY),
        ViewportRect::new(0, 0, 16, 16).unwrap(),
    )
    .unwrap();
    let mut outcomes = UiPresentationOutcomes::default();
    let backend_generation = UiPresentationBackendGeneration::new(77, 3).unwrap();
    let submitted = submitted_ui_attempt(&mut outcomes, packet, 81, 91, backend_generation);
    let submissions = vec![submitted];

    let error = WgpuRenderBackend::prepare_submitted_ui_facts(
        &submissions,
        &lowering.packet_facts,
        backend::WgpuSubmittedFrameFacts::new(10, 9, 4, 3, 4096, 2048),
    )
    .expect_err("a lowered packet for another target must fail closed");

    assert!(matches!(
        error,
        WgpuRenderError::UiSubmissionRejected { .. }
    ));
}

#[cfg(feature = "ui-submitter")]
#[test]
fn transaction_facts_only_name_an_identity_for_one_frame_wide_packet() {
    let packet = ui_outcome_packet();
    let facts = frame::WgpuUiPayloadFacts {
        packet_count: 1,
        packet_identity: Some(packet.identity()),
        primitives: 3,
        glyph_primitives: 2,
        glyph_pages: 1,
    };
    let mut transaction = WgpuFrameTransactionStats::default();

    transaction.record_ui_payload(facts);
    assert_eq!(transaction.ui_packet_identity(), Some(packet.identity()));

    transaction.record_ui_payload(frame::WgpuUiPayloadFacts::default());
    assert_eq!(transaction.ui_packet_identity(), Some(packet.identity()));

    transaction.record_ui_payload(facts);
    assert_eq!(transaction.ui_packet_count(), 2);
    assert_eq!(transaction.ui_packet_identity(), None);
    assert_eq!(transaction.ui_primitives(), 6);
    assert_eq!(transaction.ui_glyph_primitives(), 4);
    assert_eq!(transaction.ui_glyph_pages(), 2);
}
