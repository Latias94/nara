use std::time::Duration;

use nara_app::{
    Plugin, PluginDeclaration, PluginId, RuntimeAdmissionReservation,
    RuntimeCandidateRetirementState, RuntimeClosePolicy, RuntimeControl,
    RuntimeControlRequestResult, RuntimeFault, RuntimeFaultKind, RuntimeFaultReporter,
    RuntimeObligationLedger,
};
use nara_window::backend::BackendWindowHandles;

use super::*;
use crate::WgpuRenderBackend;

#[derive(Resource)]
struct HarnessControl;

#[derive(Debug)]
enum HarnessMutation {
    Connection(WgpuRuntimeConnection),
    RemoveConnection,
    RemoveStats,
    RestoreStats,
    BackendStatus(RenderBackendStatus),
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
    let returning = matches!(input, ConnectionInput::Return(..));
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
                #[cfg(feature = "ui-submitter")]
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
    WgpuRenderBackend::default()
        .execute_runtime_frame(&BackendWindowHandles::default(), capture(runtime))
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
    let mut backend = WgpuRenderBackend::default();
    let result = backend.execute_runtime_frame(&BackendWindowHandles::default(), capture);
    assert!(result.observation().is_none());
    assert_eq!(backend.frame_transaction_stats().queue_submissions(), 0);
    assert_eq!(
        backend
            .frame_transaction_stats()
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
