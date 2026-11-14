//! Control observations retain a reissuable Stop independently from disposable history.

use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use nara_app::{
    App, RuntimeCloseContext, RuntimeCloseParticipant, RuntimeCloseParticipantError,
    RuntimeCloseParticipantId, RuntimeClosePolicy, RuntimeCloseProgress, RuntimeControl,
    RuntimeControlFailure, RuntimeControlRequestResult, RuntimeControlStatus, RuntimeControlTicket,
    RuntimeInstance, RuntimeObligationLedger, RuntimeState,
};

struct RetryUntilReleased(Arc<AtomicBool>);

impl RuntimeCloseParticipant for RetryUntilReleased {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        if self.0.load(Ordering::SeqCst) {
            Ok(RuntimeCloseProgress::Complete)
        } else {
            Err(RuntimeCloseParticipantError::retryable(
                "nara.test.control-history-pending",
            ))
        }
    }

    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        unreachable!("each begin completes or asks for an explicit retry")
    }
}

fn accept(runtime: &mut RuntimeInstance, control: RuntimeControl) -> RuntimeControlTicket {
    match runtime.request_control(control) {
        RuntimeControlRequestResult::Accepted(ticket) => ticket,
        rejected => panic!("expected accepted {control:?}, got {rejected:?}"),
    }
}

#[test]
fn reissued_stop_remains_observable_after_many_failed_close_attempts() {
    let released = Arc::new(AtomicBool::new(false));
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            RuntimeCloseParticipantId::new("nara.test.control-history-owner"),
            RetryUntilReleased(Arc::clone(&released)),
        )
        .unwrap();
    let mut runtime =
        RuntimeInstance::start_with_options(App::new(), obligations, RuntimeClosePolicy::default())
            .unwrap();
    let stop = accept(&mut runtime, RuntimeControl::Stop);
    assert_eq!(
        runtime.control_status(stop),
        Some(RuntimeControlStatus::Pending)
    );
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
    let first_result = runtime.control_status(stop);
    let mut observed = Vec::new();
    let mut first_retry = None;
    for _ in 0..48 {
        let retry = accept(&mut runtime, RuntimeControl::RetryClose);
        first_retry.get_or_insert(retry);
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.state(), RuntimeState::Stopping);
        assert_eq!(
            runtime.control_status(retry),
            Some(RuntimeControlStatus::Pending)
        );
        let reissued = accept(&mut runtime, RuntimeControl::Stop);
        observed.push((reissued, runtime.control_status(reissued)));
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
        assert_eq!(
            runtime.control_status(retry),
            Some(RuntimeControlStatus::Failed(
                RuntimeControlFailure::CloseIncomplete
            )),
        );
    }
    let expired_retry = runtime.control_status(first_retry.unwrap());
    released.store(true, Ordering::SeqCst);
    let retry = accept(&mut runtime, RuntimeControl::RetryClose);
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopped);
    let final_stop = runtime.control_status(stop);
    let final_retry = runtime.control_status(retry);
    let evidence = runtime.finish().unwrap();
    assert!(evidence.pending_participants().is_empty());
    assert!(!evidence.has_terminal_failure());
    let incomplete = Some(RuntimeControlStatus::Failed(
        RuntimeControlFailure::CloseIncomplete,
    ));
    assert_eq!(first_result, incomplete);
    assert_eq!(expired_retry, None);
    assert_eq!(final_retry, Some(RuntimeControlStatus::Applied));
    assert_eq!(final_stop, incomplete);
    assert!(
        observed
            .iter()
            .all(|(ticket, status)| *ticket == stop && *status == incomplete)
    );
}

#[test]
fn ordinary_history_expires_while_current_and_foreign_observations_stay_distinct() {
    let mut runtime = RuntimeInstance::start(App::new()).unwrap();
    let first = accept(&mut runtime, RuntimeControl::Pause);
    runtime.drive(Duration::ZERO).unwrap();
    let mut latest = first;
    for _ in 0..40 {
        for control in [RuntimeControl::Resume, RuntimeControl::Pause] {
            latest = accept(&mut runtime, control);
            assert_eq!(
                runtime.control_status(latest),
                Some(RuntimeControlStatus::Pending)
            );
            runtime.drive(Duration::ZERO).unwrap();
        }
    }
    let expired = runtime.control_status(first);
    let current = runtime.control_status(latest);
    let stop = accept(&mut runtime, RuntimeControl::Stop);
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopped);
    runtime.finish().unwrap();
    let mut other = RuntimeInstance::start(App::new()).unwrap();
    let foreign = other.control_status(stop);
    accept(&mut other, RuntimeControl::Stop);
    other.drive(Duration::ZERO).unwrap();
    other.drive(Duration::ZERO).unwrap();
    assert_eq!(other.state(), RuntimeState::Stopped);
    other.finish().unwrap();
    assert_eq!(expired, None);
    assert_eq!(current, Some(RuntimeControlStatus::Applied));
    assert_eq!(foreign, None);
}
