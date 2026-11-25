use crate::{
    RuntimeCandidate, RuntimeCloseCause, RuntimeCloseErrorDisposition, RuntimeClosePolicy,
    RuntimeControlFailure, RuntimeControlStatus, RuntimeObligationLedger, RuntimeState,
};

use super::*;

fn start(app: App) -> RuntimeInstance {
    let candidate = RuntimeCandidate::admit(
        app.seal().unwrap(),
        RuntimeObligationLedger::new(),
        RuntimeClosePolicy::new(Duration::from_secs(2)),
    )
    .unwrap();
    candidate
        .complete_startup()
        .map_err(|failure| format!("{:?}", failure.fault()))
        .unwrap()
        .promote()
}

fn drive_until(runtime: &mut RuntimeInstance, state: RuntimeState) {
    wait_until(|| {
        runtime.drive(Duration::ZERO).unwrap();
        runtime.state() == state
    });
}

#[test]
fn managed_close_preserves_preinstalled_and_replacement_facilities() {
    let mut external_app = App::new();
    external_app
        .insert_resource(TaskPools::inline_for_tests(config(1)).unwrap())
        .unwrap();
    external_app.add_plugins(plugin(config(1))).unwrap();
    let mut external = start(external_app);
    assert!(matches!(
        external.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    drive_until(&mut external, RuntimeState::Stopped);
    assert!(!external.world().contains_resource::<TaskShutdownReport>());
    assert!(
        external
            .world()
            .resource::<TaskPools>()
            .shutdown_blocking()
            .unwrap()
            .is_complete()
    );

    let mut app = App::new();
    app.add_plugins(plugin(config(1))).unwrap();
    let (entered, started) = mpsc::sync_channel(1);
    let (release, held) = mpsc::channel();
    let task = app
        .world()
        .resource::<TaskPools>()
        .spawn(TaskPoolKind::Io, request(1), move |_| {
            entered.send(()).unwrap();
            let _ = held.recv_timeout(Duration::from_secs(5));
        })
        .into_handle()
        .unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    app.insert_resource(TaskPools::inline_for_tests(config(2)).unwrap())
        .unwrap();
    let mut runtime = start(app);
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    drive_until(&mut runtime, RuntimeState::CloseIncomplete);
    assert!(
        !runtime
            .world()
            .resource::<TaskShutdownReport>()
            .is_complete()
    );
    assert_eq!(task.return_observer().state(), TaskReturnState::Pending);
    release.send(()).unwrap();
    wait_until(|| task.return_observer().state() == TaskReturnState::Returned);
    assert!(matches!(
        runtime.request_control(RuntimeControl::RetryClose),
        RuntimeControlRequestResult::Accepted(_)
    ));
    drive_until(&mut runtime, RuntimeState::Stopped);
    assert!(
        runtime
            .world()
            .resource::<TaskShutdownReport>()
            .is_complete()
    );
    assert_eq!(*runtime.world().resource::<TaskPools>().config(), config(2));
}

struct PanickingCapture;

impl Drop for PanickingCapture {
    fn drop(&mut self) {
        panic!("test capture cleanup failure");
    }
}

#[test]
fn managed_close_reports_cancelled_capture_cleanup_failure_after_physical_return() {
    let mut app = App::new();
    let config = config(1)
        .with_shutdown_policy(TaskShutdownPolicy::new(
            TimeLimit::MIN,
            TimeLimit::new(Duration::from_secs(1)).unwrap(),
            TimeLimit::new(Duration::from_secs(1)).unwrap(),
        ))
        .unwrap();
    app.add_plugins(plugin(config)).unwrap();
    let pools = app.world().resource::<TaskPools>();
    let (entered, started) = mpsc::sync_channel(1);
    let (release, held) = mpsc::channel();
    let _running = pools
        .spawn(TaskPoolKind::Io, request(1), move |_| {
            entered.send(()).unwrap();
            let _ = held.recv_timeout(Duration::from_secs(5));
        })
        .into_handle()
        .unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let capture = PanickingCapture;
    let pending = pools
        .spawn(TaskPoolKind::Io, request(2), move |_| drop(capture))
        .into_handle()
        .unwrap();
    let mut runtime = start(app);
    let RuntimeControlRequestResult::Accepted(ticket) =
        runtime.request_control(RuntimeControl::Stop)
    else {
        panic!("Stop must be admitted");
    };
    wait_until(|| {
        runtime.drive(Duration::ZERO).unwrap();
        pending.return_observer().state() == TaskReturnState::CleanupFailed
    });
    release.send(()).unwrap();
    drive_until(&mut runtime, RuntimeState::Stopped);
    let report = runtime.world().resource::<TaskShutdownReport>();
    assert!(report.is_complete());
    assert!(report.has_cleanup_failure());
    assert!(!runtime.world().contains_resource::<TaskPools>());
    assert!(
        runtime
            .close_evidence()
            .causes()
            .iter()
            .any(|cause| matches!(
                cause,
                RuntimeCloseCause::ParticipantError {
                    code: "nara.tasks.cleanup-failed",
                    disposition: RuntimeCloseErrorDisposition::Terminal,
                    ..
                }
            ))
    );
    assert!(runtime.close_evidence().pending_participants().is_empty());
    assert_eq!(
        runtime.control_status(ticket),
        Some(RuntimeControlStatus::Failed(
            RuntimeControlFailure::CloseFailed
        ))
    );
}
