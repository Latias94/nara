use std::{sync::Arc, time::Duration};

use nara_app::{
    RuntimeControl, RuntimeControlRequestResult, RuntimeInstance, RuntimeState,
    RuntimeWorldAccessError,
};
use nara_ecs::Entity;
use nara_hierarchy::{
    HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyWriter,
};
use nara_input::{
    InputGeometryGeneration, InputIngressError, InputPointerId, InputPresentationEpoch,
    InputPresentedRoute, InputRouteGeneration, InputSeatId, InputSlotGeneration, InputSurfaceId,
    InputTimeline, InputWindowId,
};
use nara_ui::{
    UiColor, UiNode, UiRoot, UiRuntimeKey, UiSafeAreaInsets, UiSize, UiSurfaceContentTransform,
    UiSurfaceEndpoint, UiSurfaceId, UiSurfaceSlot, UiVisual,
};
use nara_ui_render::__private::{
    UiBackendPresentationFrame, UiBackendSubmission, UiCompletedBackendOutcome,
    UiPresentationOutcomes, bind_backend_submission_batch, commit_ui_backend_outcome,
    take_ui_backend_presentation_frame,
};
use nara_ui_render::{UiPresentationBackendGeneration, UiPresentationIntegrationError};

use super::*;
use crate::ui_presentation::UiPresentationBridgeStats;
use crate::{PresentationUiPlugins, RuntimeUiPlugins};

const SURFACE: u64 = 71;

#[derive(Resource)]
struct CloseFixturePort;

enum CloseFixtureInput {
    Take,
    Commit(UiCompletedBackendOutcome),
    CommitPresentations,
    HideWindow,
    RetirePlan,
    ReplaceOutcomes,
    Detach(DetachKind),
    Restore(DetachedAuthority),
    ProbeRoute(InputPresentedRoute),
}

enum CloseFixtureOutput {
    Frame(Result<UiBackendPresentationFrame, UiPresentationIntegrationError>),
    Committed(Result<(), UiCompletedBackendOutcome>),
    Detached(DetachedAuthority),
    Route(Result<(), InputIngressError>),
    Done,
}

#[derive(Clone, Copy)]
enum DetachKind {
    ClosePort,
    Input,
    Ledger,
    Stats,
}

enum DetachedAuthority {
    ClosePort(UiPresentationClosePort),
    Input(InputTimeline),
    Ledger(UiPresentationAttemptLedger),
    Stats(UiPresentationBridgeStats),
}

impl __RuntimeDriverPort for CloseFixturePort {
    type Input = CloseFixtureInput;
    type Output = CloseFixtureOutput;

    fn accepts_driver_state(state: RuntimeState) -> bool {
        matches!(
            state,
            RuntimeState::Running
                | RuntimeState::Paused
                | RuntimeState::Stopping
                | RuntimeState::CloseIncomplete
        )
    }

    fn apply_driver_input(&mut self, _input: Self::Input) -> Self::Output {
        unreachable!("the fixture uses its registered concrete UI operation")
    }
}

fn apply_fixture(
    world: &mut World,
    input: CloseFixtureInput,
) -> Result<CloseFixtureOutput, (RuntimeWorldAccessError, CloseFixtureInput)> {
    Ok(match input {
        CloseFixtureInput::Take => {
            CloseFixtureOutput::Frame(take_ui_backend_presentation_frame(world))
        }
        CloseFixtureInput::Commit(completed) => {
            CloseFixtureOutput::Committed(commit_ui_backend_outcome(world, completed))
        }
        CloseFixtureInput::CommitPresentations => {
            crate::ui_presentation::commit_runtime_ui_presentations(world);
            CloseFixtureOutput::Done
        }
        CloseFixtureInput::HideWindow => {
            let mut windows = world.query::<&mut nara_window::Window>();
            let mut window = windows
                .iter_mut(world)
                .find(|window| window.id == nara_window::WindowId::PRIMARY)
                .unwrap();
            window.resolution = nara_window::WindowResolution::new(0, 0);
            CloseFixtureOutput::Done
        }
        CloseFixtureInput::RetirePlan => {
            let identity = world
                .resource::<UiPresentationAttemptLedger>()
                .active()
                .next()
                .unwrap()
                .identity();
            let _ = nara_ui_render::__private::abort_ui_planned_packet(
                world,
                nara_ui_render::__private::submitted_attempt(identity),
            );
            CloseFixtureOutput::Done
        }
        CloseFixtureInput::ReplaceOutcomes => {
            world.insert_resource(UiPresentationOutcomes::default());
            CloseFixtureOutput::Done
        }
        CloseFixtureInput::Detach(kind) => {
            let detached = match kind {
                DetachKind::ClosePort => DetachedAuthority::ClosePort(
                    world.remove_resource::<UiPresentationClosePort>().unwrap(),
                ),
                DetachKind::Input => {
                    let original = world.remove_resource::<InputTimeline>().unwrap();
                    let mut replacement = InputTimeline::default();
                    replacement
                        .connect_endpoint(InputWindowId::new(1), InputSeatId::PRIMARY)
                        .unwrap();
                    replacement
                        .connect_pointer(
                            InputWindowId::new(1),
                            InputSeatId::PRIMARY,
                            InputPointerId::PRIMARY,
                        )
                        .unwrap();
                    world.insert_resource(replacement);
                    DetachedAuthority::Input(original)
                }
                DetachKind::Ledger => {
                    let original = world
                        .remove_resource::<UiPresentationAttemptLedger>()
                        .unwrap();
                    world.insert_resource(UiPresentationAttemptLedger::default());
                    DetachedAuthority::Ledger(original)
                }
                DetachKind::Stats => DetachedAuthority::Stats(
                    world
                        .remove_resource::<UiPresentationBridgeStats>()
                        .unwrap(),
                ),
            };
            CloseFixtureOutput::Detached(detached)
        }
        CloseFixtureInput::Restore(detached) => {
            match detached {
                DetachedAuthority::ClosePort(port) => world.insert_resource(port),
                DetachedAuthority::Input(input) => world.insert_resource(input),
                DetachedAuthority::Ledger(ledger) => world.insert_resource(ledger),
                DetachedAuthority::Stats(stats) => world.insert_resource(stats),
            }
            CloseFixtureOutput::Done
        }
        CloseFixtureInput::ProbeRoute(route) => {
            let mut input = world.resource_mut::<InputTimeline>();
            let result =
                nara_input::__private::prepare_presented_route(&mut input, route).map(|prepared| {
                    nara_input::__private::abort_presented_route(&mut input, prepared);
                });
            CloseFixtureOutput::Route(result)
        }
    })
}

fn apply(runtime: &mut RuntimeInstance, input: CloseFixtureInput) -> CloseFixtureOutput {
    let outcome = runtime
        .__with_owned_driver_port::<CloseFixturePort>(input)
        .unwrap_or_else(|(error, _)| panic!("the fixture scope was refused: {error}"));
    assert!(outcome.post_operation_fault.is_none());
    outcome
        .output
        .unwrap_or_else(|(error, _)| panic!("the fixture operation was refused: {error}"))
}

fn install_surface(app: &mut App) -> Entity {
    let world = app.world_mut().unwrap();
    {
        let mut input = world.resource_mut::<InputTimeline>();
        input
            .connect_endpoint(InputWindowId::new(1), InputSeatId::PRIMARY)
            .unwrap();
        input
            .connect_pointer(
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
            )
            .unwrap();
    }
    let primary_exists = world
        .query::<&nara_window::Window>()
        .iter(world)
        .any(|window| window.id == nara_window::WindowId::PRIMARY);
    if !primary_exists {
        world.spawn((
            nara_window::Window::new(
                "logical-close",
                nara_window::WindowResolution::new(320, 180),
            )
            .with_id(nara_window::WindowId::PRIMARY),
            nara_window::PrimaryWindow,
        ));
    }
    nara_ui::install_ui_surface(
        world,
        UiSurfaceSlot::new(
            UiSurfaceId::new(SURFACE).unwrap(),
            UiSurfaceEndpoint::new(
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
            ),
            UiSurfaceContentTransform::new(
                UiSize::new(320.0, 180.0).unwrap(),
                UiSize::new(320.0, 180.0).unwrap(),
                UiSafeAreaInsets::default(),
                1.0,
            )
            .unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    let root = world
        .spawn((
            UiRoot::default(),
            UiNode::default(),
            UiRuntimeKey::new("test.logical-close", "root").unwrap(),
            UiVisual::solid(UiColor::rgba(0.2, 0.4, 0.6, 1.0)),
        ))
        .id();
    HierarchyWriter::new(world)
        .apply_batch(&[HierarchyOperation::Place {
            entity: root,
            placement: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        }])
        .unwrap();
    root
}

fn fixture() -> RuntimeInstance {
    let mut app = App::new();
    app.add_plugins((RuntimeUiPlugins, PresentationUiPlugins))
        .unwrap();
    install_surface(&mut app);
    app.insert_resource(CloseFixturePort).unwrap();
    app.__register_driver_port::<CloseFixturePort>(apply_fixture)
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    runtime
}

fn take(runtime: &mut RuntimeInstance) -> Vec<UiBackendSubmission> {
    let CloseFixtureOutput::Frame(frame) = apply(runtime, CloseFixtureInput::Take) else {
        unreachable!()
    };
    let (_, submissions) = frame.unwrap().into_parts();
    assert_eq!(submissions.len(), 1);
    submissions
}

fn stop(runtime: &mut RuntimeInstance) {
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    assert_eq!(
        runtime.drive(Duration::ZERO).unwrap().state(),
        RuntimeState::Stopping
    );
}

fn active_count(runtime: &RuntimeInstance) -> usize {
    runtime
        .world()
        .resource::<UiPresentationAttemptLedger>()
        .active()
        .count()
}

fn drive_after_logical_settlement(runtime: &mut RuntimeInstance) -> RuntimeState {
    let mut state = runtime.drive(Duration::ZERO).unwrap().state();
    assert!(
        !runtime
            .close_evidence()
            .pending_participants()
            .iter()
            .any(|pending| pending.participant() == UI_PRESENTATION_CLOSE_PARTICIPANT),
        "all original UI authorities and permissions must settle in this close drive"
    );
    // The product also owns real Task workers. UI settlement cannot prove that their OS
    // threads joined during the same CPU call, so keep their independent close protocol.
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while state == RuntimeState::Stopping && std::time::Instant::now() < deadline {
        std::thread::yield_now();
        state = runtime.drive(Duration::ZERO).unwrap().state();
    }
    state
}

#[test]
fn logical_close_terminates_the_real_untaken_backend_queue() {
    let mut runtime = fixture();
    let receiver = backend_outcome_receiver(runtime.world()).unwrap();
    assert_eq!(active_count(&runtime), 1);
    assert_eq!(receiver.pending_backend_count(), 0);

    stop(&mut runtime);
    assert_eq!(
        drive_after_logical_settlement(&mut runtime),
        RuntimeState::Stopped
    );
    assert_eq!(active_count(&runtime), 0);
    assert_eq!(
        runtime
            .world()
            .resource::<UiPresentationBridgeStats>()
            .aborted_packets,
        1
    );
    assert!(receiver.drain().unwrap().is_empty());
    assert!(crate::ui_presentation::latest_presented_ui_observation(runtime.world()).is_none());
    runtime.finish().unwrap();
}

#[test]
fn cross_frame_capture_backpressure_releases_after_the_original_result_settles() {
    let mut runtime = fixture();
    let submissions = take(&mut runtime);
    let first_identity = submissions[0].submission().identity();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(active_count(&runtime), 1);

    let mut submitted = bind_backend_submission_batch(
        submissions,
        UiPresentationBackendGeneration::new(503, 1).unwrap(),
    )
    .unwrap();
    let completed = submitted.pop().unwrap().presented();
    let CloseFixtureOutput::Committed(result) =
        apply(&mut runtime, CloseFixtureInput::Commit(completed))
    else {
        unreachable!()
    };
    result.unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(active_count(&runtime), 0);
    assert_eq!(
        runtime
            .world()
            .resource::<UiPresentationBridgeStats>()
            .presented_packets,
        1
    );

    runtime.drive(Duration::ZERO).unwrap();
    let next = take(&mut runtime);
    assert_ne!(next[0].submission().identity(), first_identity);
    stop(&mut runtime);
    assert_eq!(
        runtime.drive(Duration::ZERO).unwrap().state(),
        RuntimeState::Stopping
    );
    drop(next);
    assert_eq!(
        drive_after_logical_settlement(&mut runtime),
        RuntimeState::Stopped
    );
    runtime.finish().unwrap();
}

#[test]
fn logical_close_waits_for_the_real_in_flight_permit_after_cross_frame_capture() {
    let mut runtime = fixture();
    let receiver = backend_outcome_receiver(runtime.world()).unwrap();
    let submissions = take(&mut runtime);
    let identity = submissions[0].submission().identity();
    let original_packet = Arc::clone(submissions[0].submission().packet());
    let route = {
        let ledger = runtime.world().resource::<UiPresentationAttemptLedger>();
        let UiPresentationAttempt::Submitted { authority, .. } = ledger.active().next().unwrap()
        else {
            panic!("the real UI queue must retain its submitted authority")
        };
        let receipt = authority.receipt();
        InputPresentedRoute::new(
            InputWindowId::new(1),
            InputSurfaceId::new(SURFACE),
            InputSeatId::PRIMARY,
            InputPresentationEpoch::new(receipt.presentation_epoch()).unwrap(),
            InputSlotGeneration::new(receipt.slot_generation().get()).unwrap(),
            InputRouteGeneration::new(receipt.route_revision().get()).unwrap(),
            InputGeometryGeneration::new(receipt.geometry_generation().get()).unwrap(),
        )
    };

    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(active_count(&runtime), 1);
    assert_eq!(
        runtime
            .world()
            .resource::<UiPresentationAttemptLedger>()
            .active()
            .next()
            .unwrap()
            .identity(),
        identity,
    );
    let (planned, _) = nara_ui_render::__private::snapshot_ui_paint_packets(runtime.world())
        .unwrap()
        .into_parts();
    assert!(
        planned
            .iter()
            .any(|packet| Arc::ptr_eq(packet, &original_packet))
    );
    assert_eq!(
        runtime
            .world()
            .resource::<UiPresentationBridgeStats>()
            .aborted_packets,
        0
    );

    stop(&mut runtime);
    assert_eq!(
        runtime.drive(Duration::ZERO).unwrap().state(),
        RuntimeState::Stopping
    );
    assert_eq!(active_count(&runtime), 0);
    assert_eq!(receiver.pending_backend_count(), 1);
    let CloseFixtureOutput::Route(result) =
        apply(&mut runtime, CloseFixtureInput::ProbeRoute(route))
    else {
        unreachable!()
    };
    result.expect("logical close must release the original Input route reservation");

    drop(submissions);
    assert_eq!(receiver.pending_backend_count(), 0);
    assert_eq!(
        drive_after_logical_settlement(&mut runtime),
        RuntimeState::Stopped
    );
    let stats = runtime.world().resource::<UiPresentationBridgeStats>();
    assert_eq!(stats.aborted_packets, 1);
    assert_eq!(stats.presented_packets, 0);
    assert_eq!(stats.deferred_packets, 0);
    assert!(receiver.drain().unwrap().is_empty());
    assert!(
        runtime
            .with_driver_scope(|_| panic!("Stopped cannot admit a port"))
            .is_err()
    );
    runtime.finish().unwrap();
}

#[test]
fn logical_close_uses_the_original_receiver_after_outcome_ingress_replacement() {
    let mut runtime = fixture();
    let receiver = backend_outcome_receiver(runtime.world()).unwrap();
    let submissions = take(&mut runtime);
    assert!(matches!(
        apply(&mut runtime, CloseFixtureInput::ReplaceOutcomes),
        CloseFixtureOutput::Done
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(active_count(&runtime), 1);
    assert_eq!(receiver.pending_backend_count(), 1);
    assert_eq!(
        runtime
            .world()
            .resource::<UiPresentationBridgeStats>()
            .aborted_packets,
        0
    );
    stop(&mut runtime);
    assert_eq!(
        runtime.drive(Duration::ZERO).unwrap().state(),
        RuntimeState::Stopping
    );
    assert_eq!(active_count(&runtime), 0);
    assert_eq!(receiver.pending_backend_count(), 1);

    let mut submitted = bind_backend_submission_batch(
        submissions,
        UiPresentationBackendGeneration::new(501, 1).unwrap(),
    )
    .unwrap();
    let completed = submitted.pop().unwrap().presented();
    let CloseFixtureOutput::Committed(result) =
        apply(&mut runtime, CloseFixtureInput::Commit(completed))
    else {
        unreachable!()
    };
    let refused = result.expect_err("a replacement ingress cannot consume the original permit");
    assert_eq!(receiver.pending_backend_count(), 1);
    drop(refused);
    assert_eq!(receiver.pending_backend_count(), 0);
    assert_eq!(
        drive_after_logical_settlement(&mut runtime),
        RuntimeState::Stopped
    );
    assert!(receiver.drain().unwrap().is_empty());
    assert_eq!(
        runtime
            .world()
            .resource::<UiPresentationBridgeStats>()
            .presented_packets,
        0
    );
    assert!(crate::ui_presentation::latest_presented_ui_observation(runtime.world()).is_none());
    runtime.finish().unwrap();
}

#[test]
fn replacing_outcome_ingress_before_capture_cannot_escape_the_registered_close_receiver() {
    let mut runtime = fixture();
    let receiver = backend_outcome_receiver(runtime.world()).unwrap();
    assert!(matches!(
        apply(&mut runtime, CloseFixtureInput::ReplaceOutcomes),
        CloseFixtureOutput::Done
    ));
    let CloseFixtureOutput::Frame(frame) = apply(&mut runtime, CloseFixtureInput::Take) else {
        unreachable!()
    };
    assert!(matches!(
        frame,
        Err(UiPresentationIntegrationError::BackendOutcomeOwnerChanged)
    ));
    assert_eq!(receiver.pending_backend_count(), 0);
    assert_eq!(active_count(&runtime), 1);
    stop(&mut runtime);
    assert_eq!(
        drive_after_logical_settlement(&mut runtime),
        RuntimeState::Stopped
    );
    assert_eq!(active_count(&runtime), 0);
    assert!(receiver.drain().unwrap().is_empty());
    assert_eq!(
        runtime
            .world()
            .resource::<UiPresentationBridgeStats>()
            .aborted_packets,
        1
    );
    runtime.finish().unwrap();
}

#[test]
fn logical_close_discards_real_presented_and_deferred_outcomes_without_retry() {
    for (defer, settle_before_stop) in [(false, false), (true, false), (true, true)] {
        let mut runtime = fixture();
        let receiver = backend_outcome_receiver(runtime.world()).unwrap();
        let submissions = take(&mut runtime);
        let mut submitted = bind_backend_submission_batch(
            submissions,
            UiPresentationBackendGeneration::new(502, 1).unwrap(),
        )
        .unwrap();
        let submitted = submitted.pop().unwrap();
        let completed = if defer {
            submitted.deferred("close before retry".to_owned())
        } else {
            submitted.presented()
        };
        let CloseFixtureOutput::Committed(result) =
            apply(&mut runtime, CloseFixtureInput::Commit(completed))
        else {
            unreachable!()
        };
        result.unwrap();
        if settle_before_stop {
            assert!(matches!(
                apply(&mut runtime, CloseFixtureInput::CommitPresentations),
                CloseFixtureOutput::Done
            ));
            assert!(
                runtime
                    .world()
                    .resource::<UiPresentationAttemptLedger>()
                    .active()
                    .next()
                    .unwrap()
                    .is_deferred()
            );
        }
        assert_eq!(receiver.pending_backend_count(), 0);
        assert_eq!(active_count(&runtime), 1);

        stop(&mut runtime);
        assert_eq!(
            drive_after_logical_settlement(&mut runtime),
            RuntimeState::Stopped
        );
        assert_eq!(active_count(&runtime), 0);
        let stats = runtime.world().resource::<UiPresentationBridgeStats>();
        assert_eq!(stats.aborted_packets, 1);
        assert_eq!(stats.presented_packets, 0);
        assert_eq!(stats.deferred_packets, u64::from(settle_before_stop));
        assert!(receiver.drain().unwrap().is_empty());
        assert!(crate::ui_presentation::latest_presented_ui_observation(runtime.world()).is_none());
        runtime.finish().unwrap();
    }
}

#[test]
fn logical_close_waits_for_a_held_candidate_after_real_window_hide_and_route_retirement() {
    let mut runtime = fixture();
    let receiver = backend_outcome_receiver(runtime.world()).unwrap();
    let submissions = take(&mut runtime);
    assert!(matches!(
        apply(&mut runtime, CloseFixtureInput::HideWindow),
        CloseFixtureOutput::Done
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        nara_ui::active_ui_surface_slots(runtime.world())
            .unwrap()
            .is_empty()
    );
    let (planned, _) = nara_ui_render::__private::snapshot_ui_paint_packets(runtime.world())
        .unwrap()
        .into_parts();
    assert!(planned.is_empty());
    assert_eq!(receiver.pending_backend_count(), 1);

    stop(&mut runtime);
    assert_eq!(
        runtime.drive(Duration::ZERO).unwrap().state(),
        RuntimeState::Stopping
    );
    assert_eq!(active_count(&runtime), 0);
    drop(submissions);
    assert_eq!(
        drive_after_logical_settlement(&mut runtime),
        RuntimeState::Stopped
    );
    assert_eq!(
        runtime
            .world()
            .resource::<UiPresentationBridgeStats>()
            .presented_packets,
        0
    );
    assert!(receiver.drain().unwrap().is_empty());
    runtime.finish().unwrap();
}

#[test]
fn logical_close_aborts_original_ui_input_authority_after_its_target_plan_has_gone() {
    let mut runtime = fixture();
    let receiver = backend_outcome_receiver(runtime.world()).unwrap();
    let submissions = take(&mut runtime);
    assert!(matches!(
        apply(&mut runtime, CloseFixtureInput::RetirePlan),
        CloseFixtureOutput::Done
    ));
    let (planned, _) = nara_ui_render::__private::snapshot_ui_paint_packets(runtime.world())
        .unwrap()
        .into_parts();
    assert!(planned.is_empty());
    assert_eq!(active_count(&runtime), 1);

    stop(&mut runtime);
    assert_eq!(
        runtime.drive(Duration::ZERO).unwrap().state(),
        RuntimeState::Stopping
    );
    assert_eq!(active_count(&runtime), 0);
    assert_eq!(receiver.pending_backend_count(), 1);
    drop(submissions);
    assert_eq!(
        drive_after_logical_settlement(&mut runtime),
        RuntimeState::Stopped
    );
    assert_eq!(
        runtime
            .world()
            .resource::<UiPresentationBridgeStats>()
            .aborted_packets,
        1
    );
    assert!(receiver.drain().unwrap().is_empty());
    runtime.finish().unwrap();
}

#[test]
fn logical_close_refusal_keeps_original_authority_until_its_owner_is_restored() {
    for kind in [DetachKind::ClosePort, DetachKind::Input, DetachKind::Ledger] {
        let mut runtime = fixture();
        let receiver = backend_outcome_receiver(runtime.world()).unwrap();
        let submissions = take(&mut runtime);
        let CloseFixtureOutput::Detached(detached) =
            apply(&mut runtime, CloseFixtureInput::Detach(kind))
        else {
            unreachable!()
        };
        stop(&mut runtime);
        assert_eq!(
            runtime.drive(Duration::ZERO).unwrap().state(),
            RuntimeState::CloseIncomplete
        );
        assert_eq!(receiver.pending_backend_count(), 1);
        match &detached {
            DetachedAuthority::Ledger(ledger) => assert_eq!(ledger.active().count(), 1),
            _ => assert_eq!(active_count(&runtime), 1),
        }
        if let Some(stats) = runtime.world().get_resource::<UiPresentationBridgeStats>() {
            assert_eq!(stats.aborted_packets, 0);
        }
        assert!(matches!(
            apply(&mut runtime, CloseFixtureInput::Restore(detached)),
            CloseFixtureOutput::Done
        ));
        assert!(matches!(
            runtime.request_control(RuntimeControl::RetryClose),
            RuntimeControlRequestResult::Accepted(_)
        ));
        assert_eq!(
            runtime.drive(Duration::ZERO).unwrap().state(),
            RuntimeState::Stopping
        );
        assert_eq!(
            runtime.drive(Duration::ZERO).unwrap().state(),
            RuntimeState::Stopping
        );
        assert_eq!(active_count(&runtime), 0);
        drop(submissions);
        assert_eq!(
            drive_after_logical_settlement(&mut runtime),
            RuntimeState::Stopped
        );
        assert_eq!(
            runtime
                .world()
                .resource::<UiPresentationBridgeStats>()
                .aborted_packets,
            1
        );
        assert!(receiver.drain().unwrap().is_empty());
        runtime.finish().unwrap();
    }
}

#[test]
fn logical_close_does_not_require_the_optional_statistics_observer() {
    let mut runtime = fixture();
    let receiver = backend_outcome_receiver(runtime.world()).unwrap();
    let submissions = take(&mut runtime);
    let CloseFixtureOutput::Detached(stats) =
        apply(&mut runtime, CloseFixtureInput::Detach(DetachKind::Stats))
    else {
        unreachable!()
    };
    stop(&mut runtime);
    assert_eq!(
        runtime.drive(Duration::ZERO).unwrap().state(),
        RuntimeState::Stopping
    );
    assert_eq!(active_count(&runtime), 0);
    assert_eq!(receiver.pending_backend_count(), 1);
    drop(submissions);
    assert_eq!(
        drive_after_logical_settlement(&mut runtime),
        RuntimeState::Stopped
    );
    assert!(receiver.drain().unwrap().is_empty());
    assert!(
        runtime
            .world()
            .get_resource::<UiPresentationBridgeStats>()
            .is_none()
    );
    assert!(crate::ui_presentation::latest_presented_ui_observation(runtime.world()).is_none());
    drop(stats);
    runtime.finish().unwrap();
}

#[test]
fn late_presented_result_cannot_publish_while_the_original_close_port_is_unavailable() {
    let mut runtime = fixture();
    let submissions = take(&mut runtime);
    let CloseFixtureOutput::Detached(port) = apply(
        &mut runtime,
        CloseFixtureInput::Detach(DetachKind::ClosePort),
    ) else {
        unreachable!()
    };
    stop(&mut runtime);
    assert_eq!(
        runtime.drive(Duration::ZERO).unwrap().state(),
        RuntimeState::CloseIncomplete
    );
    let completed = bind_backend_submission_batch(
        submissions,
        UiPresentationBackendGeneration::new(507, 1).unwrap(),
    )
    .unwrap()
    .pop()
    .unwrap()
    .presented();
    let CloseFixtureOutput::Committed(result) =
        apply(&mut runtime, CloseFixtureInput::Commit(completed))
    else {
        unreachable!()
    };
    result.unwrap();
    apply(&mut runtime, CloseFixtureInput::CommitPresentations);
    let presented_during_close = runtime
        .world()
        .resource::<UiPresentationBridgeStats>()
        .presented_packets;
    let interaction_published =
        crate::ui_presentation::latest_presented_ui_observation(runtime.world()).is_some();

    apply(&mut runtime, CloseFixtureInput::Restore(port));
    assert!(matches!(
        runtime.request_control(RuntimeControl::RetryClose),
        RuntimeControlRequestResult::Accepted(_)
    ));
    assert_eq!(
        runtime.drive(Duration::ZERO).unwrap().state(),
        RuntimeState::Stopping
    );
    assert_eq!(
        drive_after_logical_settlement(&mut runtime),
        RuntimeState::Stopped
    );
    let aborted = runtime
        .world()
        .resource::<UiPresentationBridgeStats>()
        .aborted_packets;
    runtime.finish().unwrap();
    assert_eq!(presented_during_close, 0);
    assert!(!interaction_published);
    assert_eq!(aborted, 1);
}
