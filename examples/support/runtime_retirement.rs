use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    time::{Duration, Instant},
};

const RUNTIME_RETIREMENT_POLL_INTERVAL: Duration = Duration::from_millis(1);
const RUNTIME_RETIREMENT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Debug)]
pub(crate) struct RuntimeRetirementFailure {
    prior: Option<nara::app::AppRunError>,
    evidence: nara::app::RuntimeCloseEvidence,
    retirement: nara::app::RuntimeRetirement,
}

impl RuntimeRetirementFailure {
    #[cfg(test)]
    fn retirement_state(&self) -> nara::app::RuntimeCandidateRetirementState {
        self.retirement.retirement_state()
    }

    #[cfg(test)]
    fn close_evidence(&self) -> &nara::app::RuntimeCloseEvidence {
        &self.evidence
    }
}

impl Display for RuntimeRetirementFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        if let Some(prior) = &self.prior {
            write!(formatter, "{prior}; ")?;
        }
        write!(
            formatter,
            "runtime retirement remained {:?}; close evidence={:?}",
            self.retirement.retirement_state(),
            self.evidence.causes()
        )
    }
}

impl Error for RuntimeRetirementFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.prior
            .as_ref()
            .map(|error| error as &(dyn Error + 'static))
    }
}

pub(crate) fn finish_runtime_after_winit(
    run_result: Result<nara::app::AppExit, nara::app::AppRunError>,
    runtime: nara::app::RuntimeInstance,
) -> Result<(), Box<dyn Error>> {
    finish_runtime_after_winit_with_timeout(run_result, runtime, RUNTIME_RETIREMENT_TIMEOUT)
}

fn finish_runtime_after_winit_with_timeout(
    run_result: Result<nara::app::AppExit, nara::app::AppRunError>,
    runtime: nara::app::RuntimeInstance,
    timeout: Duration,
) -> Result<(), Box<dyn Error>> {
    if runtime.state() == nara::app::RuntimeState::Stopped {
        return run_result
            .map(|_| ())
            .map_err(|error| Box::new(error) as Box<dyn Error>);
    }

    let prior = run_result.err();
    let mut retirement = runtime.begin_retirement();
    let deadline = Instant::now() + timeout;
    while retirement.retirement_state() != nara::app::RuntimeCandidateRetirementState::Retired
        && Instant::now() < deadline
    {
        retirement.drive_retirement();
        if retirement.retirement_state() == nara::app::RuntimeCandidateRetirementState::Retired {
            break;
        }
        let remaining = deadline.saturating_duration_since(Instant::now());
        std::thread::park_timeout(RUNTIME_RETIREMENT_POLL_INTERVAL.min(remaining));
    }

    if retirement.retirement_state() == nara::app::RuntimeCandidateRetirementState::Retired {
        return match prior {
            Some(error) => Err(Box::new(error)),
            None => Ok(()),
        };
    }

    let evidence = retirement.close_evidence().clone();
    Err(Box::new(RuntimeRetirementFailure {
        prior,
        evidence,
        retirement,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara::app::{
        App, RuntimeAdmissionReservation, RuntimeCloseCause, RuntimeCloseContext,
        RuntimeCloseParticipant, RuntimeCloseParticipantError, RuntimeCloseParticipantId,
        RuntimeClosePolicy, RuntimeCloseProgress, RuntimeControl, RuntimeControlRequestResult,
        RuntimeObligationLedger, RuntimeState,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    struct ReleasedCloseParticipant {
        released: Arc<AtomicBool>,
        polls: Arc<AtomicUsize>,
    }

    impl RuntimeCloseParticipant for ReleasedCloseParticipant {
        fn begin_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            Ok(RuntimeCloseProgress::Pending)
        }

        fn poll_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Ok(if self.released.load(Ordering::SeqCst) {
                RuntimeCloseProgress::Complete
            } else {
                RuntimeCloseProgress::Pending
            })
        }
    }

    fn start_runtime(
        obligations: RuntimeObligationLedger,
        close_policy: RuntimeClosePolicy,
    ) -> nara::app::RuntimeInstance {
        let candidate = RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(App::new().seal().unwrap(), obligations, close_policy)
            .unwrap();
        candidate.complete_startup().unwrap().promote()
    }

    #[test]
    fn successful_run_and_successful_retirement_return_success() {
        let runtime = start_runtime(
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        );

        let result = finish_runtime_after_winit_with_timeout(
            Ok(nara::app::AppExit::Success),
            runtime,
            Duration::from_millis(10),
        );

        assert!(result.is_ok());
    }

    #[test]
    fn prior_runner_error_survives_successful_retirement() {
        let runtime = start_runtime(
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        );
        let prior = nara::app::AppRunError::runner("injected runner failure");

        let error = finish_runtime_after_winit_with_timeout(
            Err(prior.clone()),
            runtime,
            Duration::from_millis(10),
        )
        .unwrap_err();

        assert_eq!(error.downcast_ref::<nara::app::AppRunError>(), Some(&prior));
    }

    #[test]
    fn incomplete_retirement_error_retains_owner_and_structured_evidence() {
        let released = Arc::new(AtomicBool::new(false));
        let polls = Arc::new(AtomicUsize::new(0));
        let mut obligations = RuntimeObligationLedger::new();
        obligations
            .register(
                RuntimeCloseParticipantId::new("nara.example.pending-close"),
                ReleasedCloseParticipant {
                    released: released.clone(),
                    polls: polls.clone(),
                },
            )
            .unwrap();
        let runtime = start_runtime(obligations, RuntimeClosePolicy::new(Duration::ZERO));

        let mut error = finish_runtime_after_winit_with_timeout(
            Ok(nara::app::AppExit::Success),
            runtime,
            Duration::from_millis(10),
        )
        .unwrap_err();
        let failure = error
            .downcast_mut::<RuntimeRetirementFailure>()
            .expect("incomplete retirement keeps the typed owner error");

        assert_eq!(
            failure.retirement_state(),
            nara::app::RuntimeCandidateRetirementState::RetirementIncomplete
        );
        assert!(
            failure
                .close_evidence()
                .causes()
                .contains(&RuntimeCloseCause::DeadlineExceeded)
        );
        let polls_before_release = polls.load(Ordering::SeqCst);
        released.store(true, Ordering::SeqCst);
        assert_eq!(
            failure.retirement.drive_retirement(),
            nara::app::RuntimeCandidateRetirementState::Retired
        );
        assert!(polls.load(Ordering::SeqCst) > polls_before_release);
    }

    #[test]
    fn initial_retirement_incomplete_is_retried_within_the_remaining_budget() {
        let released = Arc::new(AtomicBool::new(false));
        let polls = Arc::new(AtomicUsize::new(0));
        let mut obligations = RuntimeObligationLedger::new();
        obligations
            .register(
                RuntimeCloseParticipantId::new("nara.example.retry-close"),
                ReleasedCloseParticipant {
                    released: released.clone(),
                    polls: polls.clone(),
                },
            )
            .unwrap();
        let mut runtime = start_runtime(obligations, RuntimeClosePolicy::new(Duration::ZERO));
        assert!(matches!(
            runtime.request_control(RuntimeControl::Stop),
            RuntimeControlRequestResult::Accepted(_)
        ));
        while runtime.state() == RuntimeState::Running || runtime.state() == RuntimeState::Stopping
        {
            runtime.drive(Duration::ZERO).unwrap();
        }
        assert_eq!(runtime.state(), RuntimeState::CloseIncomplete);
        let polls_before_retry = polls.load(Ordering::SeqCst);
        released.store(true, Ordering::SeqCst);

        let result = finish_runtime_after_winit_with_timeout(
            Ok(nara::app::AppExit::Success),
            runtime,
            Duration::from_millis(10),
        );

        assert!(result.is_ok());
        assert!(polls.load(Ordering::SeqCst) > polls_before_retry);
    }
}
