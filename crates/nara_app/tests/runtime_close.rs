use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

use nara_app::{
    AddPluginsError, App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    PluginShutdownObligationId, RuntimeCandidateRetirementState, RuntimeCloseCause,
    RuntimeCloseContext, RuntimeCloseErrorDisposition, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseParticipantPhase,
    RuntimeClosePolicy, RuntimeCloseProgress, RuntimeControl, RuntimeControlFailure,
    RuntimeControlRequestResult, RuntimeControlStatus, RuntimeInstance, RuntimeObligationLedger,
    RuntimeState,
};

const PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.test.completed-close-error");

const REJECTED_PLUGIN: PluginId = PluginId::new("nara.test.rejected-close-owner");
const DECLARED_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.test.declared-close");
const UNDECLARED_OBLIGATION: PluginShutdownObligationId =
    PluginShutdownObligationId::new("nara.test.undeclared-close");
const REJECTED_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.test.rejected-close");
const RETAINED_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(REJECTED_PLUGIN, PluginCategory::Runtime)
        .shutdown_obligations(&[DECLARED_OBLIGATION]);

#[derive(Clone, Default)]
struct CustodyProbe {
    ready: Arc<AtomicBool>,
    begins: Arc<AtomicUsize>,
    drops: Arc<AtomicUsize>,
}

struct CustodyParticipant(CustodyProbe);

impl Drop for CustodyParticipant {
    fn drop(&mut self) {
        self.0.drops.fetch_add(1, Ordering::SeqCst);
    }
}

impl RuntimeCloseParticipant for CustodyParticipant {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.0.begins.fetch_add(1, Ordering::SeqCst);
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(if self.0.ready.load(Ordering::SeqCst) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }
}

#[derive(Clone, Copy)]
enum RegistrationRejection {
    Undeclared,
    DuplicateObligation,
}

struct RejectedClosePlugin {
    rejection: RegistrationRejection,
    probe: CustodyProbe,
}

impl Plugin for RejectedClosePlugin {
    fn declaration() -> &'static PluginDeclaration {
        &RETAINED_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let obligation = match self.rejection {
            RegistrationRejection::Undeclared => UNDECLARED_OBLIGATION,
            RegistrationRejection::DuplicateObligation => {
                app.register_plugin_runtime_close_participant(
                    DECLARED_OBLIGATION,
                    PARTICIPANT,
                    CustodyParticipant(self.probe.clone()),
                )?;
                DECLARED_OBLIGATION
            }
        };
        app.register_plugin_runtime_close_participant(
            obligation,
            REJECTED_PARTICIPANT,
            CustodyParticipant(self.probe.clone()),
        )?;
        Ok(())
    }
}

#[test]
fn rejected_close_registration_outside_build_retains_custody_until_retry() {
    let probe = CustodyProbe::default();
    let mut app = App::new();
    assert!(matches!(
        app.register_plugin_runtime_close_participant(
            DECLARED_OBLIGATION,
            REJECTED_PARTICIPANT,
            CustodyParticipant(probe.clone()),
        ),
        Err(PluginError::ShutdownObligationOutsideBuild)
    ));
    let drops_before_close = probe.drops.load(Ordering::SeqCst);
    let begins_before_close = probe.begins.load(Ordering::SeqCst);
    let runtime = RuntimeInstance::start_with_options(
        app,
        RuntimeObligationLedger::new(),
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap();
    let mut failure = runtime.finish().unwrap_err();
    let state_before_retry = failure.retirement_state();
    let begins_before_retry = probe.begins.load(Ordering::SeqCst);
    let drops_before_retry = probe.drops.load(Ordering::SeqCst);
    probe.ready.store(true, Ordering::SeqCst);
    let state_after_retry = failure.drive_retirement();
    drop(failure);

    assert_eq!(drops_before_close, 0);
    assert_eq!(begins_before_close, 0);
    assert_eq!(begins_before_retry, 1);
    assert_eq!(drops_before_retry, 0);
    assert_eq!(probe.begins.load(Ordering::SeqCst), 1);
    assert_eq!(
        state_before_retry,
        RuntimeCandidateRetirementState::RetirementIncomplete
    );
    assert_eq!(state_after_retry, RuntimeCandidateRetirementState::Retired);
    assert_eq!(probe.drops.load(Ordering::SeqCst), 1);
}

#[test]
fn rejected_close_registration_inside_build_retains_custody_until_retry() {
    for rejection in [
        RegistrationRejection::Undeclared,
        RegistrationRejection::DuplicateObligation,
    ] {
        let probe = CustodyProbe::default();
        let mut app = App::new();
        let expected = match rejection {
            RegistrationRejection::Undeclared => PluginError::UndeclaredShutdownObligation {
                plugin: REJECTED_PLUGIN,
                obligation: UNDECLARED_OBLIGATION,
            },
            RegistrationRejection::DuplicateObligation => {
                PluginError::DuplicateShutdownObligation {
                    plugin: REJECTED_PLUGIN,
                    obligation: DECLARED_OBLIGATION,
                }
            }
        };
        let owner_count = match rejection {
            RegistrationRejection::Undeclared => 1,
            RegistrationRejection::DuplicateObligation => 2,
        };
        assert!(matches!(
            app.add_plugin(RejectedClosePlugin {
                rejection,
                probe: probe.clone(),
            }),
            Err(AddPluginsError::Plugin(error)) if error.error() == &expected
        ));
        let drops_before_close = probe.drops.load(Ordering::SeqCst);
        let mut failure = RuntimeInstance::start_with_options(
            app,
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap_err();
        let state_before_retry = failure.retirement_state();
        let begins_before_retry = probe.begins.load(Ordering::SeqCst);
        let drops_before_retry = probe.drops.load(Ordering::SeqCst);
        probe.ready.store(true, Ordering::SeqCst);
        let state_after_retry = failure.drive_retirement();
        drop(failure);

        assert_eq!(drops_before_close, 0);
        assert_eq!(begins_before_retry, owner_count);
        assert_eq!(drops_before_retry, 0);
        assert_eq!(probe.begins.load(Ordering::SeqCst), owner_count);
        assert_eq!(
            state_before_retry,
            RuntimeCandidateRetirementState::RetirementIncomplete
        );
        assert_eq!(state_after_retry, RuntimeCandidateRetirementState::Retired);
        assert_eq!(probe.drops.load(Ordering::SeqCst), owner_count);
    }
}

struct CompletedWithError {
    phase: RuntimeCloseParticipantPhase,
    calls: Arc<AtomicUsize>,
}

impl RuntimeCloseParticipant for CompletedWithError {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(match self.phase {
            RuntimeCloseParticipantPhase::Begin => RuntimeCloseProgress::CompleteWithError {
                code: "nara.test.close-error",
            },
            RuntimeCloseParticipantPhase::Poll => RuntimeCloseProgress::Pending,
        })
    }

    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        Ok(RuntimeCloseProgress::CompleteWithError {
            code: "nara.test.close-error",
        })
    }
}

fn with_close_error(phase: RuntimeCloseParticipantPhase) -> (RuntimeInstance, Arc<AtomicUsize>) {
    let calls = Arc::new(AtomicUsize::new(0));
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(
            PARTICIPANT,
            CompletedWithError {
                phase,
                calls: Arc::clone(&calls),
            },
        )
        .unwrap();
    (
        RuntimeInstance::start_with_options(App::new(), obligations, RuntimeClosePolicy::default())
            .unwrap(),
        calls,
    )
}

#[test]
fn completed_participant_error_retires_physically_and_finish_retains_failure() {
    for phase in [
        RuntimeCloseParticipantPhase::Begin,
        RuntimeCloseParticipantPhase::Poll,
    ] {
        let (runtime, calls) = with_close_error(phase);
        let mut failure = runtime.finish().unwrap_err();
        assert_eq!(
            failure.retirement_state(),
            RuntimeCandidateRetirementState::Retired
        );
        assert!(failure.close_evidence().has_terminal_failure());
        assert!(failure.close_evidence().pending_participants().is_empty());
        assert_eq!(
            failure.close_evidence().causes(),
            &[RuntimeCloseCause::ParticipantError {
                participant: PARTICIPANT,
                phase,
                code: "nara.test.close-error",
                disposition: RuntimeCloseErrorDisposition::Terminal,
            }]
        );
        let completed_calls = calls.load(Ordering::Relaxed);
        assert_eq!(
            failure.drive_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
        assert_eq!(calls.load(Ordering::Relaxed), completed_calls);
    }
}

#[test]
fn stop_control_reports_completed_participant_error_without_remaining_stopping() {
    let (mut runtime, _) = with_close_error(RuntimeCloseParticipantPhase::Poll);
    let RuntimeControlRequestResult::Accepted(ticket) =
        runtime.request_control(RuntimeControl::Stop)
    else {
        panic!("running runtime accepts Stop");
    };
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopped);
    assert_eq!(
        runtime.control_status(ticket),
        Some(RuntimeControlStatus::Failed(
            RuntimeControlFailure::CloseFailed
        ))
    );
    assert_eq!(
        runtime.finish().unwrap_err().retirement_state(),
        RuntimeCandidateRetirementState::Retired
    );
}

struct RetryOnce(bool);

impl RuntimeCloseParticipant for RetryOnce {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        if std::mem::replace(&mut self.0, true) {
            Ok(RuntimeCloseProgress::Complete)
        } else {
            Err(RuntimeCloseParticipantError::retryable(
                "nara.test.retry-once",
            ))
        }
    }

    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        unreachable!("begin either completes or asks for a retry");
    }
}

#[test]
fn successful_retry_preserves_history_without_reporting_terminal_failure() {
    let mut obligations = RuntimeObligationLedger::new();
    obligations.register(PARTICIPANT, RetryOnce(false)).unwrap();
    let mut runtime =
        RuntimeInstance::start_with_options(App::new(), obligations, RuntimeClosePolicy::default())
            .unwrap();
    runtime.request_control(RuntimeControl::Stop);
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
    let history = runtime.close_evidence().causes().to_vec();
    let pending = runtime.close_evidence().pending_participants();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].participant(), PARTICIPANT);
    assert_eq!(pending[0].phase(), RuntimeCloseParticipantPhase::Begin);
    let RuntimeControlRequestResult::Accepted(ticket) =
        runtime.request_control(RuntimeControl::RetryClose)
    else {
        panic!("incomplete runtime accepts RetryClose");
    };
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopped);
    assert_eq!(
        runtime.control_status(ticket),
        Some(RuntimeControlStatus::Applied)
    );
    let evidence = runtime.finish().unwrap();
    assert!(!evidence.is_empty());
    assert!(!evidence.has_terminal_failure());
    assert!(evidence.pending_participants().is_empty());
    assert_eq!(evidence.causes(), history.as_slice());
}

struct PendingUntilReleased(Arc<AtomicBool>);

impl RuntimeCloseParticipant for PendingUntilReleased {
    fn begin_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        _: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        Ok(if self.0.load(Ordering::Relaxed) {
            RuntimeCloseProgress::Complete
        } else {
            RuntimeCloseProgress::Pending
        })
    }
}

#[test]
fn pending_deadline_identifies_current_participants_and_retry_preserves_history() {
    let first = RuntimeCloseParticipantId::new("nara.test.first-pending-close");
    let second = RuntimeCloseParticipantId::new("nara.test.second-pending-close");
    let first_released = Arc::new(AtomicBool::new(false));
    let second_released = Arc::new(AtomicBool::new(false));
    let mut obligations = RuntimeObligationLedger::new();
    obligations
        .register(first, PendingUntilReleased(Arc::clone(&first_released)))
        .unwrap();
    obligations
        .register(second, PendingUntilReleased(Arc::clone(&second_released)))
        .unwrap();
    let mut runtime = RuntimeInstance::start_with_options(
        App::new(),
        obligations,
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap();
    assert!(runtime.close_evidence().is_empty());
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopping);
    assert!(runtime.close_evidence().causes().is_empty());
    assert!(!runtime.close_evidence().is_empty());
    assert_eq!(
        runtime
            .close_evidence()
            .pending_participants()
            .iter()
            .map(|pending| (pending.participant(), pending.phase()))
            .collect::<Vec<_>>(),
        vec![
            (first, RuntimeCloseParticipantPhase::Begin),
            (second, RuntimeCloseParticipantPhase::Begin),
        ]
    );

    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
    let timed_out = runtime.close_evidence().clone();
    assert_eq!(timed_out.causes(), &[RuntimeCloseCause::DeadlineExceeded]);
    assert!(!timed_out.has_terminal_failure());
    assert_eq!(
        timed_out
            .pending_participants()
            .iter()
            .map(|pending| (pending.participant(), pending.phase()))
            .collect::<Vec<_>>(),
        vec![
            (first, RuntimeCloseParticipantPhase::Poll),
            (second, RuntimeCloseParticipantPhase::Poll),
        ]
    );

    first_released.store(true, Ordering::Relaxed);
    assert!(matches!(
        runtime.request_control(RuntimeControl::RetryClose),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
    let pending = runtime.close_evidence().pending_participants();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].participant(), second);
    assert_eq!(pending[0].phase(), RuntimeCloseParticipantPhase::Poll);
    assert_eq!(runtime.close_evidence().causes(), timed_out.causes());

    second_released.store(true, Ordering::Relaxed);
    let RuntimeControlRequestResult::Accepted(ticket) =
        runtime.request_control(RuntimeControl::RetryClose)
    else {
        panic!("incomplete runtime accepts RetryClose");
    };
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Stopped);
    assert_eq!(
        runtime.control_status(ticket),
        Some(RuntimeControlStatus::Applied)
    );
    let evidence = runtime.finish().unwrap();
    assert!(evidence.pending_participants().is_empty());
    assert_eq!(evidence.causes(), timed_out.causes());
    assert_eq!(timed_out.pending_participants().len(), 2);
    assert!(!evidence.has_terminal_failure());
}

#[test]
fn completed_error_is_not_pending_while_another_participant_times_out() {
    for phase in [
        RuntimeCloseParticipantPhase::Begin,
        RuntimeCloseParticipantPhase::Poll,
    ] {
        let pending_id = RuntimeCloseParticipantId::new("nara.test.pending-beside-close-error");
        let released = Arc::new(AtomicBool::new(false));
        let mut obligations = RuntimeObligationLedger::new();
        obligations
            .register(pending_id, PendingUntilReleased(Arc::clone(&released)))
            .unwrap();
        obligations
            .register(
                PARTICIPANT,
                CompletedWithError {
                    phase,
                    calls: Arc::new(AtomicUsize::new(0)),
                },
            )
            .unwrap();
        let runtime = RuntimeInstance::start_with_options(
            App::new(),
            obligations,
            RuntimeClosePolicy::new(Duration::ZERO),
        )
        .unwrap();
        let mut retirement = runtime.finish().unwrap_err();
        assert_eq!(
            retirement.retirement_state(),
            RuntimeCandidateRetirementState::RetirementIncomplete
        );
        let evidence = retirement.close_evidence();
        let pending = evidence.pending_participants();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].participant(), pending_id);
        assert_eq!(pending[0].phase(), RuntimeCloseParticipantPhase::Poll);
        assert_eq!(
            evidence.causes(),
            &[
                RuntimeCloseCause::ParticipantError {
                    participant: PARTICIPANT,
                    phase,
                    code: "nara.test.close-error",
                    disposition: RuntimeCloseErrorDisposition::Terminal,
                },
                RuntimeCloseCause::DeadlineExceeded,
            ]
        );

        released.store(true, Ordering::Relaxed);
        assert_eq!(
            retirement.drive_retirement(),
            RuntimeCandidateRetirementState::Retired
        );
        assert!(
            retirement
                .close_evidence()
                .pending_participants()
                .is_empty()
        );
        assert!(retirement.close_evidence().has_terminal_failure());
    }
}
