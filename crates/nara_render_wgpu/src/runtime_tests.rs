use std::time::Duration;

use nara_app::{
    Plugin, PluginDeclaration, PluginId, RuntimeAdmissionReservation,
    RuntimeCandidateRetirementState, RuntimeClosePolicy, RuntimeControl,
    RuntimeControlRequestResult, RuntimeFault, RuntimeFaultKind, RuntimeFaultReporter,
    RuntimeObligationLedger,
};
use nara_window::backend::BackendWindowHandles;

use super::*;
use crate::WgpuRenderHost;

#[derive(Resource)]
struct HarnessControl;

#[derive(Debug)]
enum HarnessMutation {
    Connection(WgpuRuntimeConnection),
    RemoveConnection,
    RemoveStats,
    RestoreStats,
    BackendStatus(RenderBackendStatus),
    RuntimeGeneration(RuntimeGeneration),
    CaptureError(WgpuFrameCaptureError),
    EmptyFrame(u64),
    Fault,
}

impl __RuntimeDriverPort for HarnessControl {
    type Input = HarnessMutation;
    type Output = Option<WgpuRuntimeConnection>;

    fn accepts_driver_state(state: RuntimeState) -> bool {
        !matches!(state, RuntimeState::Stopped | RuntimeState::Stepping)
    }

    fn apply_driver_input(&mut self, _: Self::Input) -> Self::Output {
        unreachable!("the test configures a finite World operation")
    }
}

fn mutate(
    world: &mut World,
    mutation: HarnessMutation,
) -> Result<Option<WgpuRuntimeConnection>, (RuntimeWorldAccessError, HarnessMutation)> {
    Ok(match mutation {
        HarnessMutation::Connection(connection) => {
            let original = world.remove_resource::<WgpuRuntimeConnection>();
            world.insert_resource(connection);
            original
        }
        HarnessMutation::RemoveConnection => world.remove_resource::<WgpuRuntimeConnection>(),
        HarnessMutation::RemoveStats => {
            world.remove_resource::<FrameStats>();
            None
        }
        HarnessMutation::RestoreStats => {
            world.insert_resource(FrameStats::default());
            None
        }
        HarnessMutation::BackendStatus(status) => {
            world.insert_resource(status);
            None
        }
        HarnessMutation::RuntimeGeneration(generation) => {
            world.insert_resource(generation);
            None
        }
        HarnessMutation::CaptureError(error) => {
            let connection = world.resource::<WgpuRuntimeConnection>();
            let mut owner = connection.owner.lock().unwrap();
            owner
                .pending
                .as_mut()
                .expect("the harness published a pending frame before capture")
                .captured = Err(error);
            None
        }
        HarnessMutation::EmptyFrame(index) => {
            let frame = RenderFrame {
                index,
                state: nara_render::RenderFrameState::Rendering,
            };
            *world.resource_mut::<RenderFrame>() = frame;
            world
                .resource_mut::<WgpuRuntimeConnection>()
                .publish(PendingFrame {
                    captured: Ok(None),
                    frame,
                    window: None,
                    canvas: None,
                    #[cfg(feature = "stock-ui")]
                    ui: None,
                });
            None
        }
        HarnessMutation::Fault => {
            world
                .resource::<RuntimeFaultReporter>()
                .report(test_fault());
            None
        }
    })
}

fn test_fault() -> RuntimeFault {
    RuntimeFault::engine(RuntimeFaultKind::System, "nara.test.after-render-result")
}

fn return_then_fault(
    world: &mut World,
    input: ConnectionInput,
) -> Result<ConnectionOutput, (RuntimeWorldAccessError, ConnectionInput)> {
    let returning = matches!(input.0, ConnectionOperation::Return(..));
    let output = apply_connection(world, input);
    if returning {
        world
            .resource::<RuntimeFaultReporter>()
            .report(test_fault());
    }
    output
}

struct ConnectionHarness {
    fault_after_return: bool,
}

impl Plugin for ConnectionHarness {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.render-connection"),
            nara_app::PluginCategory::Backend,
        )
        .shutdown_obligations(&[crate::WGPU_BACKEND_SHUTDOWN_OBLIGATION]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(RenderFrame {
            index: 17,
            state: nara_render::RenderFrameState::Rendering,
        })?;
        app.init_resource::<FrameStats>()?;
        app.init_resource::<RenderBackendStatus>()?;
        app.init_resource::<WgpuRenderObservation>()?;
        app.init_resource::<WgpuFramePacketSlot>()?;
        app.insert_resource(WgpuRuntimeConnection::default())?;
        let original = Arc::clone(&app.world().resource::<WgpuRuntimeConnection>().owner);
        app.insert_resource(ConnectionAuthority(Arc::clone(&original)))?;
        app.__register_driver_port::<WgpuRuntimeConnection>(if self.fault_after_return {
            return_then_fault
        } else {
            apply_connection
        })?;
        app.register_plugin_runtime_close_participant(
            crate::WGPU_BACKEND_SHUTDOWN_OBLIGATION,
            RuntimeCloseParticipantId::new("nara.test.original-render-connection"),
            ConnectionClose(original),
        )?;
        app.insert_resource(HarnessControl)?;
        app.__register_driver_port::<HarnessControl>(mutate)?;
        crate::install_wgpu_policy(
            app,
            crate::WgpuPolicyDefinition::new(crate::WgpuStockPolicy::default())
                .install()
                .unwrap(),
        )?;
        app.world_mut()?
            .resource_mut::<WgpuRuntimeConnection>()
            .publish(PendingFrame {
                captured: Ok(None),
                frame: RenderFrame {
                    index: 17,
                    state: nara_render::RenderFrameState::Rendering,
                },
                window: None,
                canvas: None,
                #[cfg(feature = "stock-ui")]
                ui: None,
            });
        Ok(())
    }
}

fn runtime(fault_after_return: bool) -> RuntimeInstance {
    let mut app = App::new();
    app.add_plugins(ConnectionHarness { fault_after_return })
        .unwrap();
    RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            app.seal().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote()
}

fn control(
    runtime: &mut RuntimeInstance,
    mutation: HarnessMutation,
) -> Option<WgpuRuntimeConnection> {
    runtime
        .__with_owned_driver_port::<HarnessControl>(mutation)
        .unwrap()
        .output
        .unwrap()
}

fn capture(runtime: &mut RuntimeInstance) -> WgpuRuntimeCapture {
    take_runtime_frame(runtime).unwrap().output.unwrap()
}

fn result(runtime: &mut RuntimeInstance) -> WgpuRuntimeResult {
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    host.execute_runtime_frame(
        &producer,
        &BackendWindowHandles::default(),
        capture(runtime),
    )
}

fn window_capture(runtime: &mut RuntimeInstance, source: RuntimeGeneration) -> WgpuRuntimeCapture {
    use nara_render::{
        ExtractedView, ExtractedViews, RenderTarget, ViewLoadOp, ViewMapping2d, ViewportRect,
    };
    let viewport = ViewportRect::new(0, 0, 8, 8).unwrap();
    let mut views = ExtractedViews::default();
    views.push(ExtractedView {
        id: nara_render::RenderViewId::try_acquire().unwrap(),
        camera_entity: nara_ecs::Entity::PLACEHOLDER,
        target: RenderTarget::PrimaryWindow,
        viewport,
        mapping: ViewMapping2d::orthographic(nara_core::Mat3::IDENTITY, 8.0, viewport).unwrap(),
        order: 0,
        load: ViewLoadOp::Clear(nara_core::Color::BLACK),
    });
    let window =
        nara_window::Window::new("captured window", nara_window::WindowResolution::new(8, 8));
    let packet = nara_render::build_render_frame_packet(
        source,
        1,
        Some(nara_window::WindowId::PRIMARY),
        [&window],
        &views,
    )
    .unwrap()
    .unwrap();
    let input = runtime
        .world()
        .resource::<crate::WgpuPolicyInstallation<crate::WgpuStockPolicy>>()
        .capture(packet, Arc::new(crate::WgpuStockCapture::default()))
        .unwrap();
    let mut capture = capture(runtime);
    capture.pending.captured = Ok(Some(crate::frame::WgpuCapturedFrame {
        input,
        app_frame_index: 1,
        work_started_at: std::time::Instant::now(),
    }));
    capture
}

#[test]
fn replacing_world_generation_faults_before_a_foreign_capture_can_be_issued() {
    let mut owner = runtime(false);
    let foreign = runtime(false);
    control(
        &mut owner,
        HarnessMutation::RuntimeGeneration(foreign.generation()),
    );
    assert_eq!(owner.state(), RuntimeState::Faulted);
    let failure = take_runtime_frame(&mut owner).unwrap_err();
    assert!(matches!(failure.error, WgpuRuntimeExchangeError::Port(_)));
    let generation = owner.generation();
    control(&mut owner, HarnessMutation::RuntimeGeneration(generation));
    finish(owner);
    finish(foreign);
}

#[test]
fn policy_capture_rejection_keeps_the_runtime_running_and_the_next_frame_completes() {
    let mut runtime = runtime(false);
    control(
        &mut runtime,
        HarnessMutation::CaptureError(WgpuFrameCaptureError::Policy(
            crate::WgpuRenderProgramError::feature("policy capture rejected this frame"),
        )),
    );
    assert_eq!(runtime.state(), RuntimeState::Running);
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let rejected = capture(&mut runtime);
    let rejected_identity = rejected.identity();
    let rejected =
        host.execute_runtime_frame(&producer, &BackendWindowHandles::default(), rejected);
    assert_eq!(rejected.identity(), rejected_identity);
    let skip = rejected.status.as_ref().unwrap().last_skip().unwrap();
    assert_eq!(
        skip.reason(),
        nara_render::RenderFrameSkipReason::ContentRejected,
    );
    assert!(
        skip.message()
            .unwrap()
            .contains("policy capture rejected this frame")
    );
    assert_eq!(rejected.observation().unwrap().device_epoch(), 0);
    assert_eq!(host.frame_transaction_stats().packet_rejections(), 1);
    assert_eq!(host.frame_transaction_stats().queue_submissions(), 0);
    assert_eq!(host.retained_resource_bytes(), 0);

    let completed = complete_runtime_frame(&mut runtime, rejected).unwrap();
    assert!(!completed.output.retired);
    assert!(completed.output.presented.is_none());
    assert!(completed.post_operation_fault.is_none());
    assert_eq!(runtime.state(), RuntimeState::Running);
    assert!(runtime.fault().is_none());
    assert_eq!(
        runtime
            .world()
            .resource::<RenderBackendStatus>()
            .last_skip()
            .unwrap()
            .reason(),
        nara_render::RenderFrameSkipReason::ContentRejected,
    );
    assert!(
        runtime
            .world()
            .resource::<WgpuRuntimeConnection>()
            .can_capture()
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Running);

    // Publish through the same original connection after the first result releases its claim.
    control(&mut runtime, HarnessMutation::EmptyFrame(18));
    let recovered = capture(&mut runtime);
    assert_eq!(
        recovered.identity().connection(),
        rejected_identity.connection(),
    );
    assert_eq!(
        recovered.identity().runtime_generation(),
        runtime.generation()
    );
    assert_eq!(
        recovered.identity().sequence(),
        rejected_identity.sequence() + 1,
    );
    let recovered_identity = recovered.identity();
    let recovered =
        host.execute_runtime_frame(&producer, &BackendWindowHandles::default(), recovered);
    assert_eq!(recovered.identity(), recovered_identity);
    assert_eq!(recovered.observation().unwrap().device_epoch(), 0);
    assert_eq!(host.frame_transaction_stats().queue_submissions(), 0);
    assert_eq!(host.frame_transaction_stats().packet_rejections(), 0);
    assert_eq!(host.retained_resource_bytes(), 0);
    let completed = complete_runtime_frame(&mut runtime, recovered).unwrap();
    assert!(!completed.output.retired);
    assert!(completed.output.presented.is_none());
    assert!(completed.post_operation_fault.is_none());
    assert_eq!(runtime.state(), RuntimeState::Running);
    assert!(runtime.fault().is_none());
    assert_eq!(runtime.world().resource::<RenderFrame>().index, 18);
    assert_eq!(
        runtime
            .world()
            .resource::<RenderBackendStatus>()
            .last_skip()
            .unwrap()
            .reason(),
        nara_render::RenderFrameSkipReason::NoViews,
    );
    assert!(
        runtime
            .world()
            .resource::<WgpuRuntimeConnection>()
            .can_capture()
    );
    drop(host);
    finish(runtime);
}

#[test]
fn a_topology_capture_error_still_faults_the_actual_runtime() {
    let mut runtime = runtime(false);
    control(
        &mut runtime,
        HarnessMutation::CaptureError(WgpuFrameCaptureError::Topology(
            nara_render::RenderFramePacketError::UninitializedTarget,
        )),
    );
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let rejected = host.execute_runtime_frame(
        &producer,
        &BackendWindowHandles::default(),
        capture(&mut runtime),
    );
    assert_eq!(rejected.observation().unwrap().device_epoch(), 0);
    assert_eq!(host.frame_transaction_stats().queue_submissions(), 0);
    let completed = complete_runtime_frame(&mut runtime, rejected).unwrap();
    assert!(!completed.output.retired);
    assert!(completed.output.presented.is_none());
    assert_eq!(
        completed.post_operation_fault.unwrap().kind(),
        RuntimeFaultKind::System,
    );
    assert_eq!(runtime.state(), RuntimeState::Faulted);
    assert_eq!(runtime.fault().unwrap().kind(), RuntimeFaultKind::System);
    drop(host);
    finish(runtime);
}

#[cfg(feature = "stock-ui")]
#[test]
fn ui_collection_backpressure_returns_the_original_result_and_allows_runtime_recovery() {
    use nara_ui_render::UiPresentationIntegrationError;

    for error in [
        UiPresentationIntegrationError::OutcomeCapacityUnavailable { additional: 1 },
        UiPresentationIntegrationError::PendingPresentationOutcomes { count: 1 },
        UiPresentationIntegrationError::OutstandingBackendOutcomeReservations { count: 1 },
    ] {
        let mut runtime = runtime(false);
        control(
            &mut runtime,
            HarnessMutation::CaptureError(WgpuFrameCaptureError::UiPresentation(error)),
        );
        let mut host = WgpuRenderHost::offscreen();
        let producer = host.create_producer().unwrap();
        let captured = capture(&mut runtime);
        let identity = captured.identity();
        let result =
            host.execute_runtime_frame(&producer, &BackendWindowHandles::default(), captured);
        assert_eq!(result.identity(), identity);
        assert_eq!(
            result
                .status
                .as_ref()
                .unwrap()
                .last_skip()
                .unwrap()
                .reason(),
            nara_render::RenderFrameSkipReason::Deferred,
        );
        assert_eq!(host.frame_transaction_stats().queue_submissions(), 0);
        let completed = complete_runtime_frame(&mut runtime, result).unwrap();
        assert!(completed.post_operation_fault.is_none());
        assert_eq!(runtime.state(), RuntimeState::Running);
        assert!(runtime.fault().is_none());
        control(&mut runtime, HarnessMutation::EmptyFrame(19));
        let next = capture(&mut runtime);
        assert_eq!(next.identity().connection(), identity.connection());
        assert_eq!(next.identity().sequence(), identity.sequence() + 1);
        let result = host.execute_runtime_frame(&producer, &BackendWindowHandles::default(), next);
        let completed = complete_runtime_frame(&mut runtime, result).unwrap();
        assert!(completed.post_operation_fault.is_none());
        assert_eq!(runtime.state(), RuntimeState::Running);
        assert_eq!(runtime.world().resource::<RenderFrame>().index, 19);
        assert_eq!(host.retained_resource_bytes(), 0);
        drop(host);
        finish(runtime);
    }
}

#[cfg(feature = "stock-ui")]
#[test]
fn a_changed_ui_outcome_owner_still_faults_the_actual_runtime() {
    let mut runtime = runtime(false);
    control(
        &mut runtime,
        HarnessMutation::CaptureError(WgpuFrameCaptureError::UiPresentation(
            nara_ui_render::UiPresentationIntegrationError::BackendOutcomeOwnerChanged,
        )),
    );
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let captured = capture(&mut runtime);
    let result = host.execute_runtime_frame(&producer, &BackendWindowHandles::default(), captured);
    assert_eq!(host.frame_transaction_stats().queue_submissions(), 0);
    let completed = complete_runtime_frame(&mut runtime, result).unwrap();
    assert_eq!(
        completed.post_operation_fault.unwrap().kind(),
        RuntimeFaultKind::System
    );
    assert_eq!(runtime.state(), RuntimeState::Faulted);
    drop(host);
    finish(runtime);
}

#[test]
fn host_rejects_foreign_captured_source_before_native_authority_and_returns_original_result() {
    let mut owner = runtime(false);
    let foreign = runtime(false);
    let capture = window_capture(&mut owner, foreign.generation());
    let identity = capture.identity();
    let mut host = crate::WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let result = host.execute_runtime_frame(&producer, &BackendWindowHandles::default(), capture);
    assert_eq!(result.identity(), identity);
    assert_eq!(result.observation().unwrap().device_epoch(), 0);
    assert_eq!(
        result
            .observation()
            .unwrap()
            .frame_transaction_stats()
            .packet_rejections(),
        1
    );
    let outcome = complete_runtime_frame(&mut owner, result).unwrap();
    assert!(outcome.output.presented.is_none());
    assert!(outcome.post_operation_fault.is_some());
    assert_eq!(owner.state(), RuntimeState::Faulted);
    assert_eq!(owner.fault().unwrap().kind(), RuntimeFaultKind::System);
    assert!(
        host.create_producer().is_ok(),
        "one invalid source cannot retire a healthy Host"
    );
    drop(host);
    finish(owner);
    finish(foreign);
}

#[test]
fn a_foreign_runtime_producer_does_not_poison_the_receiving_host() {
    let mut owner = runtime(false);
    let generation = owner.generation();
    let capture = window_capture(&mut owner, generation);
    let mut original = crate::WgpuRenderHost::offscreen();
    let foreign_producer = original.create_producer().unwrap();
    let mut receiving = crate::WgpuRenderHost::offscreen();
    let result = receiving.execute_runtime_frame(
        &foreign_producer,
        &BackendWindowHandles::default(),
        capture,
    );
    assert_eq!(result.observation().unwrap().device_epoch(), 0);
    assert_ne!(
        result.observation().unwrap().state(),
        crate::WgpuBackendState::Unavailable
    );
    assert!(receiving.create_producer().is_ok());
    let completed = complete_runtime_frame(&mut owner, result).unwrap();
    assert!(completed.post_operation_fault.is_none());
    drop((original, receiving));
    finish(owner);
}

#[test]
fn a_missing_first_runtime_window_does_not_bind_foreign_registry_authority() {
    let mut host = crate::WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    for handles in [
        BackendWindowHandles::default(),
        BackendWindowHandles::default(),
    ] {
        let mut owner = runtime(false);
        let generation = owner.generation();
        let capture = window_capture(&mut owner, generation);
        let result = host.execute_runtime_frame(&producer, &handles, capture);
        assert_eq!(
            result
                .status
                .as_ref()
                .unwrap()
                .last_skip()
                .unwrap()
                .reason(),
            nara_render::RenderFrameSkipReason::SurfaceUnavailable,
            "each missing registry must reach target admission instead of a foreign-authority refusal"
        );
        assert_eq!(result.observation().unwrap().device_epoch(), 0);
        assert_eq!(
            result
                .observation()
                .unwrap()
                .frame_transaction_stats()
                .queue_submissions(),
            0
        );
        let completed = complete_runtime_frame(&mut owner, result).unwrap();
        assert!(completed.post_operation_fault.is_none());
        finish(owner);
    }
}

fn stop(runtime: &mut RuntimeInstance) {
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    for _ in 0..20 {
        let _ = runtime.drive(Duration::ZERO);
        if matches!(
            runtime.state(),
            RuntimeState::CloseIncomplete | RuntimeState::Stopped
        ) {
            return;
        }
    }
    panic!("the zero-budget logical close must either settle or retain incomplete custody");
}

fn finish(runtime: RuntimeInstance) {
    let mut retirement = runtime.begin_retirement();
    for _ in 0..20 {
        if retirement.drive_retirement() == RuntimeCandidateRetirementState::Retired {
            return;
        }
    }
    panic!("the original logical result should no longer block retirement");
}

fn retry_close(runtime: &mut RuntimeInstance) {
    assert!(matches!(
        runtime.request_control(RuntimeControl::RetryClose),
        RuntimeControlRequestResult::Accepted(_)
    ));
    for _ in 0..20 {
        runtime.drive(Duration::ZERO).unwrap();
        if runtime.state() == RuntimeState::Stopped {
            return;
        }
    }
    panic!("returning the original responsibility must let RetryClose complete");
}

#[test]
fn refused_result_retains_original_custody_and_can_be_retried() {
    let mut runtime = runtime(false);
    let result = result(&mut runtime);
    let identity = result.identity();
    control(&mut runtime, HarnessMutation::RemoveStats);

    let refused = complete_runtime_frame(&mut runtime, result).unwrap_err();
    assert!(matches!(
        refused.error,
        WgpuRuntimeExchangeError::Port(RuntimeWorldAccessError::MissingResource { .. })
    ));
    assert_eq!(refused.result.identity(), identity);
    assert!(take_runtime_frame(&mut runtime).unwrap().output.is_none());
    control(&mut runtime, HarnessMutation::RestoreStats);
    let completed = complete_runtime_frame(&mut runtime, refused.result).unwrap();
    assert!(!completed.output.retired);
    assert!(completed.post_operation_fault.is_none());
    assert_eq!(
        runtime
            .world()
            .resource::<WgpuRenderObservation>()
            .frame_transaction_stats()
            .render_transaction_frame_index(),
        Some(17)
    );
    finish(runtime);
}

#[test]
fn replacement_cannot_take_or_apply_an_original_connection_capture() {
    let mut runtime = runtime(false);
    let result = result(&mut runtime);
    let identity = result.identity();
    let original = control(
        &mut runtime,
        HarnessMutation::Connection(WgpuRuntimeConnection::default()),
    )
    .unwrap();
    assert!(matches!(
        take_runtime_frame(&mut runtime).unwrap_err().error,
        WgpuRuntimeExchangeError::OwnerChanged
    ));
    let failure = complete_runtime_frame(&mut runtime, result).unwrap_err();
    assert!(matches!(
        failure.error,
        WgpuRuntimeExchangeError::OwnerChanged
    ));
    assert_eq!(failure.result.identity(), identity);
    control(&mut runtime, HarnessMutation::Connection(original));
    let completed = complete_runtime_frame(&mut runtime, failure.result).unwrap();
    assert!(completed.post_operation_fault.is_none());
    finish(runtime);
}

#[test]
fn logical_close_waits_for_a_returned_original_result() {
    let mut runtime = runtime(false);
    let result = result(&mut runtime);
    stop(&mut runtime);
    assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
    let completion = complete_runtime_frame(&mut runtime, result).unwrap();
    assert!(completion.output.retired);
    assert!(completion.output.presented.is_none());
    retry_close(&mut runtime);
    assert_eq!(runtime.state(), RuntimeState::Stopped);
    finish(runtime);
}

#[test]
fn dropping_a_refused_result_releases_only_its_original_close_claim() {
    let mut runtime = runtime(false);
    let result = result(&mut runtime);
    let original = control(&mut runtime, HarnessMutation::RemoveConnection).unwrap();
    let refused = complete_runtime_frame(&mut runtime, result).unwrap_err();
    stop(&mut runtime);
    assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
    drop(refused.result);
    assert!(!original.can_capture());
    retry_close(&mut runtime);
    assert_eq!(runtime.state(), RuntimeState::Stopped);
    drop(original);
    finish(runtime);
}

#[test]
fn close_revokes_an_unexecuted_capture_without_issuing_gpu_observation() {
    let mut runtime = runtime(false);
    let capture = capture(&mut runtime);
    stop(&mut runtime);
    assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
    let mut host = WgpuRenderHost::offscreen();
    let producer = host.create_producer().unwrap();
    let result = host.execute_runtime_frame(&producer, &BackendWindowHandles::default(), capture);
    assert!(result.observation().is_none());
    assert_eq!(host.frame_transaction_stats().queue_submissions(), 0);
    assert_eq!(
        host.frame_transaction_stats()
            .render_transaction_frame_index(),
        None
    );
    let completion = complete_runtime_frame(&mut runtime, result).unwrap();
    assert!(completion.output.retired);
    assert_eq!(
        runtime
            .world()
            .resource::<WgpuRenderObservation>()
            .instance_id(),
        0
    );
    finish(runtime);
}

#[test]
fn consumed_result_keeps_its_output_when_the_scope_reports_a_fault() {
    let mut runtime = runtime(true);
    let result = result(&mut runtime);
    let completed = complete_runtime_frame(&mut runtime, result).unwrap();
    assert_eq!(completed.post_operation_fault, Some(test_fault()));
    assert_eq!(
        runtime
            .world()
            .resource::<WgpuRenderObservation>()
            .frame_transaction_stats()
            .render_transaction_frame_index(),
        Some(17)
    );
    assert!(
        runtime
            .world()
            .resource::<WgpuRuntimeConnection>()
            .owner
            .lock()
            .unwrap()
            .in_flight
            .is_none()
    );
    finish(runtime);
}

#[test]
fn rejected_result_preserves_both_input_and_a_post_operation_fault() {
    let mut runtime = runtime(true);
    let result = result(&mut runtime);
    let identity = result.identity();
    control(&mut runtime, HarnessMutation::RemoveStats);
    let failure = complete_runtime_frame(&mut runtime, result).unwrap_err();
    assert_eq!(failure.result.identity(), identity);
    assert_eq!(failure.post_operation_fault, Some(test_fault()));
    assert!(matches!(
        failure.error,
        WgpuRuntimeExchangeError::Port(RuntimeWorldAccessError::MissingResource { .. })
    ));
    assert_eq!(
        runtime
            .world()
            .resource::<WgpuRenderObservation>()
            .instance_id(),
        0
    );
    drop(failure.result);
    finish(runtime);
}

#[test]
fn faulted_runtime_accepts_its_original_result_only_as_retirement() {
    let mut runtime = runtime(false);
    let result = result(&mut runtime);
    let identity = result.identity();
    control(&mut runtime, HarnessMutation::Fault);
    assert_eq!(runtime.state(), RuntimeState::Faulted);
    let completed = complete_runtime_frame(&mut runtime, result).unwrap();
    assert!(completed.output.retired);
    assert!(completed.output.presented.is_none());
    assert!(
        runtime
            .world()
            .resource::<WgpuRuntimeConnection>()
            .owner
            .lock()
            .unwrap()
            .in_flight
            .is_none()
    );
    assert_eq!(identity.runtime_generation(), runtime.generation());
    finish(runtime);
}

#[test]
fn stopped_foreign_runtime_refuses_entry_and_returns_the_uninvoked_result() {
    let mut original = runtime(false);
    let result = result(&mut original);
    let identity = result.identity();
    let mut stopped = runtime(false);
    stop(&mut stopped);
    assert_eq!(stopped.state(), RuntimeState::Stopped);
    let failure = complete_runtime_frame(&mut stopped, result).unwrap_err();
    assert_eq!(failure.result.identity(), identity);
    assert!(matches!(
        failure.error,
        WgpuRuntimeExchangeError::Scope(RuntimeScopeError::Unavailable {
            state: RuntimeState::Stopped
        })
    ));
    let completed = complete_runtime_frame(&mut original, failure.result).unwrap();
    assert!(completed.post_operation_fault.is_none());
    finish(stopped);
    finish(original);
}

#[test]
fn cancellation_preserves_last_native_backend_state_and_error() {
    for ready in [true, false] {
        let mut runtime = runtime(false);
        let mut previous = RenderBackendStatus::default();
        if ready {
            previous.mark_ready("wgpu");
        } else {
            previous.mark_unavailable("wgpu", "original native failure");
        }
        control(
            &mut runtime,
            HarnessMutation::BackendStatus(previous.clone()),
        );
        let cancelled = cancel_runtime_frame(capture(&mut runtime));
        assert!(cancelled.observation().is_none());
        let completed = complete_runtime_frame(&mut runtime, cancelled).unwrap();
        assert!(completed.output.presented.is_none());
        let current = runtime.world().resource::<RenderBackendStatus>();
        assert_eq!(current.backend(), previous.backend());
        assert_eq!(current.state(), previous.state());
        assert_eq!(current.last_error(), previous.last_error());
        assert_eq!(
            current.last_skip().unwrap().reason(),
            nara_render::RenderFrameSkipReason::NoRenderableTarget
        );
        assert_eq!(
            runtime
                .world()
                .resource::<WgpuRenderObservation>()
                .instance_id(),
            0
        );
        finish(runtime);
    }
}
