use super::*;
use crate::runtime::{
    WgpuRuntimeCapture, WgpuRuntimeConnection, complete_runtime_frame, take_runtime_frame,
};
#[cfg(feature = "stock-ui")]
use std::sync::Arc;

use nara_app::{App, CoreStage, FrameExecutionStart, RuntimeFaultKind, RuntimeState};
#[cfg(feature = "stock-sprite")]
use nara_ecs::Entity;
#[cfg(feature = "stock-ui")]
use nara_ecs::Res;
use nara_ecs::{Commands, ResMut, Resource, schedule::IntoScheduleConfigs};
#[cfg(feature = "stock-sprite")]
use nara_render::Color;
#[cfg(feature = "stock-ui")]
use nara_render::RenderFrameState;
use nara_render::{
    Extent2d, ExtractedViews, FrameStats, RenderBackendState, RenderBackendStatus, RenderFrame,
    RenderFrameSkipReason,
};
#[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
use nara_render::{RenderTarget, ViewportRect};
#[cfg(feature = "stock-ui")]
use nara_ui_render::{
    __private::{
        UiBackendSubmission, UiBackendSubmissionReader, UiPreparedBackendSubmission,
        UiPresentationOutcomes, UiSubmittedBackendSubmission,
    },
    UiPaintPacket, UiPresentationAttemptId, UiPresentationBackendFaultDomain,
    UiPresentationBackendGeneration, UiPresentationCapabilityGeneration, UiPresentationEpoch,
    UiPresentationOutcome, UiPresentationRuntimeGeneration,
};
#[cfg(feature = "stock-ui")]
use nara_window::{PrimaryWindowId, Window};
use nara_window::{
    WindowId,
    backend::{BackendWindowHandles, WindowPresentationRequirement},
};

#[cfg(feature = "stock-ui")]
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
    #[cfg(feature = "stock-ui")]
    app.add_plugins((
        nara_image::ImagePreparePlugin,
        nara_ui_render::UiRenderPlugin::default(),
    ))
    .unwrap();
    app.add_plugins((
        WgpuCapturePlugin,
        WgpuStockCapturePlugin,
        WgpuStockPolicyPlugin,
        WgpuWindowPresentationPlugin,
    ))
    .unwrap();
    #[cfg(feature = "stock-ui")]
    app.add_plugins(WgpuStockUiPlugin).unwrap();
}

#[cfg(feature = "stock-sprite")]
#[derive(Debug, Default, Resource)]
struct UninstalledSpriteContributionObservation {
    source_instances: usize,
    collected_instances: Option<usize>,
}

#[cfg(feature = "stock-sprite")]
fn inject_sprite_domain_work(
    mut view_id: nara_ecs::Local<Option<nara_render::RenderViewId>>,
    mut views: ResMut<ExtractedViews>,
) {
    let view_id = *view_id.get_or_insert_with(|| nara_render::RenderViewId::try_acquire().unwrap());
    views.clear();
    views.push(nara_render::ExtractedView {
        id: view_id,
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

#[cfg(feature = "stock-sprite")]
fn observe_uninstalled_sprite_contribution(world: &mut nara_ecs::World) {
    let source_instances = world
        .resource::<nara_sprite_render::__private::SpriteBatches>()
        .total_instances();
    let batch = __private::take_captured_policy_batch(world).unwrap();
    #[cfg(feature = "stock-ui")]
    let (captured, ui) = batch.into_parts();
    #[cfg(not(feature = "stock-ui"))]
    let captured = batch.into_render_input();
    #[cfg(feature = "stock-ui")]
    assert!(ui.is_none());
    let input = captured.unwrap().unwrap();
    let capture = input.captured::<WgpuStockPolicy>().unwrap();
    let mut observation = world.resource_mut::<UninstalledSpriteContributionObservation>();
    observation.source_instances = source_instances;
    observation.collected_instances = capture
        .sprite_batches
        .as_ref()
        .map(nara_sprite_render::__private::SpriteBatches::total_instances);
}

#[cfg(feature = "stock-ui")]
#[derive(Debug, Default, Resource)]
struct UninstalledUiContributionObservation {
    packet_count: usize,
    submission_count: usize,
    submissions: Vec<UiBackendSubmission>,
    outcomes: Vec<UiPresentationOutcome>,
}

#[cfg(feature = "stock-ui")]
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

#[cfg(feature = "stock-ui")]
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

#[cfg(feature = "stock-ui")]
fn record_ui_isolation_outcomes(world: &mut nara_ecs::World) {
    let outcomes = nara_ui_render::drain_ui_presentation_outcomes(world)
        .expect("the test-owned rejection must release every backend permit");
    world
        .resource_mut::<UninstalledUiContributionObservation>()
        .outcomes
        .extend(outcomes);
}

#[cfg(feature = "stock-ui")]
#[derive(Default, Resource)]
struct SavedRenderConnection(Option<WgpuRuntimeConnection>);

#[cfg(feature = "stock-ui")]
fn replace_connection_before_policy_capture(world: &mut nara_ecs::World) {
    let original = world.remove_resource::<WgpuRuntimeConnection>().unwrap();
    world.resource_mut::<SavedRenderConnection>().0 = Some(original);
    world.insert_resource(WgpuRuntimeConnection::default());
}

#[cfg(feature = "stock-ui")]
fn restore_connection_after_policy_capture(world: &mut nara_ecs::World) {
    let original = world
        .resource_mut::<SavedRenderConnection>()
        .0
        .take()
        .unwrap();
    world.insert_resource(original);
}

#[cfg(feature = "stock-ui")]
#[derive(Resource)]
struct HeldStockUiCapture {
    capture: Arc<WgpuStockCapture>,
    submissions: Option<Vec<UiBackendSubmission>>,
    frames: u32,
}

#[cfg(feature = "stock-ui")]
fn accept_held_stock_ui_capture(
    installation: Res<WgpuPolicyInstallation<WgpuStockPolicy>>,
    mut writer: WgpuPolicyFrameWriter,
    mut held: ResMut<HeldStockUiCapture>,
) {
    let Some(submissions) = held.submissions.take() else {
        return;
    };
    assert!(writer.can_capture_with_ui(&installation));
    writer
        .capture_with_ui(
            &installation,
            Arc::clone(&held.capture),
            WgpuUiSubmission::from_state(WgpuUiSubmissionState::Prepared(submissions)),
        )
        .unwrap();
}

#[cfg(feature = "stock-ui")]
fn retire_held_stock_ui_capture(world: &mut nara_ecs::World) {
    let original = {
        let mut held = world.resource_mut::<HeldStockUiCapture>();
        held.frames += 1;
        if held.frames != 2 {
            return;
        }
        Arc::clone(&held.capture)
    };
    let batch = __private::take_captured_policy_batch(world).unwrap();
    let (captured, ui) = batch.into_parts();
    let input = captured.unwrap().unwrap();
    assert!(std::ptr::eq(
        input.captured::<WgpuStockPolicy>().unwrap(),
        original.as_ref(),
    ));
    assert_eq!(input.packet().frame_index(), 1);
    for completed in ui
        .unwrap()
        .content_rejected("test transaction retired".to_owned())
    {
        world
            .resource_mut::<UiPresentationOutcomes>()
            .commit_backend(completed);
    }
    record_ui_isolation_outcomes(world);
}

#[cfg(feature = "stock-ui")]
fn held_capture_was_consumed(held: Res<HeldStockUiCapture>) -> bool {
    held.frames == 2
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

#[cfg(feature = "stock-sprite")]
#[test]
fn compiled_but_uninstalled_sprite_contribution_does_not_collect_domain_batches() {
    let mut app = App::new();
    install_render_foundation(&mut app, true);
    app.add_plugins((
        nara_image::ImagePreparePlugin,
        nara_sprite_render::SpriteRenderPlugin,
    ))
    .unwrap();
    app.add_plugins((
        WgpuCapturePlugin,
        WgpuStockCapturePlugin,
        WgpuStockPolicyPlugin,
    ))
    .unwrap();
    app.insert_resource(UninstalledSpriteContributionObservation::default())
        .unwrap();
    app.add_systems(CoreStage::Prepare, inject_sprite_domain_work)
        .unwrap();
    app.add_systems(CoreStage::Last, observe_uninstalled_sprite_contribution)
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

#[cfg(feature = "stock-ui")]
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
    app.add_plugins((
        WgpuCapturePlugin,
        WgpuStockCapturePlugin,
        WgpuStockPolicyPlugin,
    ))
    .unwrap();
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
            .after(WgpuPolicyCaptureSet::Seal),
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
    runtime.finish().unwrap();
}

#[cfg(feature = "stock-ui")]
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
    app.insert_resource(SavedRenderConnection::default())
        .unwrap();
    app.insert_resource(UninstalledUiContributionObservation::default())
        .unwrap();
    app.add_systems(
        CoreStage::Render,
        replace_connection_before_policy_capture
            .after(WgpuPolicyCaptureSet::Reset)
            .before(WgpuPolicyCaptureSet::Capture),
    )
    .unwrap();
    app.add_systems(
        CoreStage::Render,
        (
            observe_undrained_ui_backend_frame,
            restore_connection_after_policy_capture,
        )
            .chain()
            .after(WgpuPolicyCaptureSet::Capture)
            .before(WgpuPolicyCaptureSet::Seal),
    )
    .unwrap();
    app.add_systems(
        CoreStage::Last,
        (
            reject_observed_ui_backend_frame,
            record_ui_isolation_outcomes,
        )
            .chain(),
    )
    .unwrap();
    let mut runtime = admit_runtime(app).complete_startup().unwrap().promote();

    runtime.drive(std::time::Duration::ZERO).unwrap();

    let observation = runtime
        .world()
        .resource::<UninstalledUiContributionObservation>();
    assert_eq!(observation.packet_count, 1);
    assert_eq!(observation.submission_count, 1);
    assert!(matches!(
        observation.outcomes.as_slice(),
        [UiPresentationOutcome::ContentRejected { attempt, .. }]
            if attempt.admission_id().is_some_and(|id| id.get() == 73)
    ));
    let capture = take_captured_frame(&mut runtime);
    assert!(capture.pending.ui.is_none());
    drop(capture);
    runtime.finish().unwrap();
}

#[cfg(feature = "stock-ui")]
#[test]
fn blocked_ui_collection_keeps_the_existing_transaction_and_next_frame_available() {
    use nara_ui_kernel::UiSurfaceId;
    use nara_ui_render::__private::{
        UiPresentationRoute, replace_ui_backend_frame, replace_ui_presentation_routes,
        snapshot_ui_paint_packets,
    };

    let mut app = App::new();
    install_render_foundation(&mut app, true);
    app.add_plugins((
        nara_image::ImagePreparePlugin,
        nara_ui_render::UiRenderPlugin::default(),
    ))
    .unwrap();
    app.add_plugins((
        WgpuCapturePlugin,
        WgpuStockCapturePlugin,
        WgpuStockPolicyPlugin,
        WgpuStockUiPlugin,
    ))
    .unwrap();
    app.world_mut().unwrap().spawn((
        nara_render::Camera2d::default(),
        nara_transform::Transform2d::default(),
    ));

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
    let existing_packet = ui_outcome_packet();
    let existing_submission = {
        let mut outcomes = world.resource_mut::<UiPresentationOutcomes>();
        admit_ui_attempt(
            &mut outcomes,
            prepared_ui_attempt(Arc::clone(&existing_packet), 70, 50),
        )
    };
    let primary_window_id = world.resource::<PrimaryWindowId>().0;
    world.insert_resource(HeldStockUiCapture {
        capture: Arc::new(WgpuStockCapture::new().with_ui(
            vec![existing_packet],
            None,
            Some(primary_window_id),
        )),
        submissions: Some(vec![existing_submission]),
        frames: 0,
    });
    let receiver = nara_ui_render::__private::backend_outcome_receiver(world).unwrap();
    app.insert_resource(UninstalledUiContributionObservation::default())
        .unwrap();
    app.add_systems(
        CoreStage::Render,
        accept_held_stock_ui_capture
            .after(WgpuPolicyCaptureSet::Reset)
            .before(WgpuPolicyCaptureSet::Capture),
    )
    .unwrap();
    app.add_systems(
        CoreStage::Last,
        (
            retire_held_stock_ui_capture,
            observe_undrained_ui_backend_frame.run_if(held_capture_was_consumed),
            reject_observed_ui_backend_frame.run_if(held_capture_was_consumed),
            record_ui_isolation_outcomes.run_if(held_capture_was_consumed),
        )
            .chain(),
    )
    .unwrap();
    let mut runtime = admit_runtime(app).complete_startup().unwrap().promote();

    runtime.drive(std::time::Duration::ZERO).unwrap();

    assert_eq!(
        receiver.drain().unwrap_err(),
        nara_ui_render::UiPresentationIntegrationError::OutstandingBackendOutcomeReservations {
            count: 1,
        },
        "blocked collection must preserve the existing outcome reservation",
    );
    runtime.drive(std::time::Duration::ZERO).unwrap();

    let observation = runtime
        .world()
        .resource::<UninstalledUiContributionObservation>();
    assert_eq!(observation.packet_count, 1);
    assert_eq!(observation.submission_count, 1);
    assert!(matches!(
        observation.outcomes.as_slice(),
        [
            UiPresentationOutcome::ContentRejected { attempt: original, .. },
            UiPresentationOutcome::ContentRejected { attempt: next, .. },
        ] if original.admission_id().is_some_and(|id| id.get() == 70)
            && next.admission_id().is_some_and(|id| id.get() == 71)
    ));
    assert_eq!(receiver.pending_backend_count(), 0);
    runtime.finish().unwrap();
}

#[cfg(feature = "stock-ui")]
#[test]
fn managed_shutdown_returns_buffered_ui_permissions_before_world_destruction() {
    use std::time::Duration;

    use nara_app::{RuntimeControl, RuntimeControlRequestResult};
    use nara_ui_kernel::UiSurfaceId;
    use nara_ui_render::__private::{
        UiPresentationRoute, backend_outcome_receiver, replace_ui_backend_frame,
        replace_ui_presentation_routes, snapshot_ui_paint_packets,
    };

    let mut app = App::new();
    install_render_foundation(&mut app, true);
    app.add_plugins((
        nara_image::ImagePreparePlugin,
        nara_ui_render::UiRenderPlugin::default(),
        WgpuCapturePlugin,
        WgpuStockCapturePlugin,
        WgpuStockPolicyPlugin,
        WgpuStockUiPlugin,
    ))
    .unwrap();
    let world = app.world_mut().unwrap();
    world.spawn((
        nara_render::Camera2d::default(),
        nara_transform::Transform2d::default(),
    ));
    let primary_window_id = world.resource::<PrimaryWindowId>().0;
    replace_ui_presentation_routes(
        world,
        [UiPresentationRoute::new(UiSurfaceId::new(1).unwrap(), primary_window_id).unwrap()],
    )
    .unwrap();
    nara_ui_render::compile_ui_paint_packets(world, &[ui_outcome_frame()]).unwrap();
    let (mut planned, _) = snapshot_ui_paint_packets(world).unwrap().into_parts();
    let packet = planned.pop().unwrap();
    replace_ui_backend_frame(
        world,
        vec![Arc::clone(&packet)],
        vec![prepared_ui_attempt(packet, 72, 52)],
    )
    .unwrap();
    let receiver = backend_outcome_receiver(world).unwrap();

    let mut runtime = admit_runtime(app).complete_startup().unwrap().promote();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(receiver.pending_backend_count(), 1);
    assert!(
        !runtime
            .world()
            .resource::<WgpuFramePacketSlot>()
            .accepts_capture()
    );
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
        inject_second_camera_after_capture.after(WgpuPolicyCaptureSet::Publish),
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

#[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
#[test]
fn legacy_empty_draw_input_has_no_quad_work() {
    assert!(
        frame::LegacyWgpuFramePayload::default()
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

#[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
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

#[cfg(feature = "stock-ui")]
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

#[cfg(feature = "stock-ui")]
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

#[cfg(feature = "stock-ui")]
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

#[cfg(feature = "stock-ui")]
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

#[cfg(feature = "stock-ui")]
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

#[cfg(feature = "stock-ui")]
pub(super) fn admit_ui_attempt(
    outcomes: &mut UiPresentationOutcomes,
    submission: UiPreparedBackendSubmission,
) -> UiBackendSubmission {
    let mut permits = outcomes
        .reserve_backend(std::slice::from_ref(&submission))
        .unwrap();
    UiBackendSubmission::pair(submission, permits.remove(0)).unwrap()
}

#[cfg(feature = "stock-ui")]
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

#[cfg(feature = "stock-ui")]
fn ui_outcome_packet_for(target: RenderTarget) -> Arc<UiPaintPacket> {
    nara_ui_render::UiPresentationPlanner::default()
        .plan(
            ui_outcome_frame(),
            target,
            &nara_ui_render::UiPreparedImages::default(),
        )
        .unwrap()
}

#[cfg(feature = "stock-ui")]
pub(super) fn ui_outcome_packet() -> Arc<UiPaintPacket> {
    ui_outcome_packet_for(RenderTarget::Window(WindowId::PRIMARY))
}

#[cfg(feature = "stock-ui")]
#[derive(Resource)]
struct StockPolicyDraft(Arc<WgpuStockCapture>);

#[cfg(feature = "stock-ui")]
fn capture_stock_policy_draft(
    draft: Res<StockPolicyDraft>,
    installation: Res<WgpuPolicyInstallation<WgpuStockPolicy>>,
    mut writer: WgpuPolicyFrameWriter,
) {
    assert!(writer.capture(&installation, Arc::clone(&draft.0)).unwrap());
}

#[cfg(feature = "stock-ui")]
fn stock_ui_input(
    packets: Vec<Arc<UiPaintPacket>>,
    primary_window_id: WindowId,
) -> Result<WgpuRenderInput, WgpuFrameCaptureError> {
    let mut app = App::new();
    install_render_foundation(&mut app, false);
    let mut window = Window::new(
        "policy UI capture",
        nara_window::WindowResolution::new(16, 16),
    );
    window.id = primary_window_id;
    app.add_plugins((
        nara_window::WindowPlugin {
            primary_window: Some(window),
        },
        WgpuCapturePlugin,
        WgpuStockCapturePlugin,
        WgpuStockPolicyPlugin,
        WgpuWindowPresentationPlugin,
    ))
    .unwrap();
    app.insert_resource(StockPolicyDraft(Arc::new(WgpuStockCapture::new().with_ui(
        packets,
        None,
        Some(primary_window_id),
    ))))
    .unwrap();
    app.add_systems(
        CoreStage::Render,
        capture_stock_policy_draft
            .after(WgpuPolicyCaptureSet::Reset)
            .before(WgpuPolicyCaptureSet::Capture),
    )
    .unwrap();
    app.world_mut().unwrap().spawn((
        nara_render::Camera2d::default(),
        nara_transform::Transform2d::default(),
    ));
    let mut runtime = admit_runtime(app).complete_startup().unwrap().promote();
    runtime.drive(std::time::Duration::ZERO).unwrap();
    let input = take_captured_frame(&mut runtime)
        .pending
        .captured
        .map(|captured| {
            captured
                .expect("the fixture captures one window View")
                .input
        });
    runtime.finish().unwrap();
    input
}

#[cfg(feature = "stock-ui")]
fn build_stock_ui_policy(input: &WgpuRenderInput) -> policy::BuiltPolicy {
    policy::PolicyInstances::default()
        .build(
            input,
            1,
            WgpuRenderProducerId::new(1),
            &Arc::new(std::sync::atomic::AtomicBool::new(false)),
            wgpu::TextureFormat::Rgba8UnormSrgb,
            None,
        )
        .unwrap()
}

#[cfg(feature = "stock-ui")]
#[test]
fn stock_policy_facts_bind_glyph_work_to_one_exact_ui_capture() {
    let packet = ui_outcome_packet();
    let identity = packet.identity();
    let input = stock_ui_input(vec![packet], WindowId::PRIMARY).unwrap();
    let built = build_stock_ui_policy(&input);
    let binding = built.output.ui.as_ref().unwrap();
    let facts = binding.payload_facts();

    assert_eq!(facts.packet_count, 1);
    assert_eq!(facts.packet_identity, Some(identity));
    assert!(facts.primitives > 0);
    assert!(facts.glyph_primitives > 0);
    assert!(facts.glyph_pages > 0);
}

#[cfg(feature = "stock-ui")]
#[test]
fn stock_capture_rejects_ui_packets_for_a_foreign_logical_target() {
    let first_target = RenderTarget::Window(WindowId::new(101));
    let second_target = RenderTarget::Window(WindowId::new(102));
    let first_packet = ui_outcome_packet_for(first_target);
    let second_packet = ui_outcome_packet_for(second_target);
    let result = stock_ui_input(vec![first_packet, second_packet], WindowId::new(101));

    assert!(matches!(
        result,
        Err(WgpuFrameCaptureError::Policy(
            WgpuRenderProgramError::Feature { .. }
        ))
    ));
}

#[cfg(feature = "stock-ui")]
#[test]
fn editor_viewport_maps_logical_primary_window_ui_to_the_private_image() {
    let packet = ui_outcome_packet_for(RenderTarget::PrimaryWindow);
    let packet_identity = packet.identity();
    let original = stock_ui_input(vec![Arc::clone(&packet)], WindowId::new(431)).unwrap();
    let image_target = nara_render::RenderImagePacket::new(
        nara_render::RenderImage2d::allocate_handle().unwrap(),
        Extent2d::new(16, 16).unwrap(),
    )
    .unwrap();
    let adapted = original.with_image_target(image_target).unwrap();
    assert_eq!(adapted.installation(), original.installation());
    assert!(std::ptr::eq(
        adapted.captured::<WgpuStockPolicy>().unwrap(),
        original.captured::<WgpuStockPolicy>().unwrap(),
    ));
    assert!(Arc::ptr_eq(
        &adapted.captured::<WgpuStockPolicy>().unwrap().ui_packets[0],
        &packet,
    ));
    assert_eq!(
        adapted
            .captured::<WgpuStockPolicy>()
            .unwrap()
            .primary_window_id,
        Some(WindowId::new(431)),
    );
    let built = build_stock_ui_policy(&adapted);
    let binding = built.output.ui.as_ref().unwrap();
    let facts = binding.payload_facts();

    assert_eq!(facts.packet_count, 1);
    assert_eq!(facts.packet_identity, Some(packet_identity));
    assert_eq!(binding.facts().len(), 1);
    assert_eq!(binding.facts()[0].packet, packet_identity);
    assert_eq!(binding.facts()[0].target, RenderTarget::PrimaryWindow,);
}

#[cfg(feature = "stock-ui")]
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

#[cfg(feature = "stock-ui")]
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

#[cfg(feature = "stock-ui")]
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

#[cfg(feature = "stock-ui")]
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

#[cfg(feature = "stock-ui")]
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

#[cfg(feature = "stock-ui")]
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
