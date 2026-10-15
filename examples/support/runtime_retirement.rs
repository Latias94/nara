use std::{
    error::Error,
    fmt::{self, Display, Formatter},
};

#[derive(Debug)]
pub(crate) struct RuntimeRetirementFailure {
    prior: Option<nara::app::AppRunError>,
    retirement: nara::app::RuntimeRetirement,
}

impl RuntimeRetirementFailure {
    #[cfg(test)]
    fn retirement_state(&self) -> nara::app::RuntimeCandidateRetirementState {
        self.retirement.retirement_state()
    }

    #[cfg(test)]
    fn close_evidence(&self) -> &nara::app::RuntimeCloseEvidence {
        self.retirement.close_evidence()
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
            self.retirement.close_evidence().causes()
        )
    }
}

impl Error for RuntimeRetirementFailure {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.prior
            .as_ref()
            .map(|error| error as &(dyn Error + 'static))
            .or(Some(&self.retirement))
    }
}

pub(crate) fn finish_runtime_after_winit(
    run_result: Result<nara::app::AppExit, nara::app::AppRunError>,
    runtime: nara::app::RuntimeInstance,
) -> Result<(), Box<dyn Error>> {
    let prior = run_result.err();
    match runtime.finish() {
        Ok(_) => match prior {
            Some(error) => Err(Box::new(error)),
            None => Ok(()),
        },
        Err(retirement) => Err(Box::new(RuntimeRetirementFailure { prior, retirement })),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara::app::{
        App, RuntimeCloseCause, RuntimeCloseContext, RuntimeCloseParticipant,
        RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeClosePolicy,
        RuntimeCloseProgress, RuntimeControl, RuntimeControlRequestResult, RuntimeObligationLedger,
        RuntimeState,
    };
    use std::sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    };
    use std::time::Duration;

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
        nara::app::RuntimeInstance::start_with_options(App::new(), obligations, close_policy)
            .unwrap()
    }

    #[test]
    fn successful_run_and_successful_retirement_return_success() {
        let runtime = start_runtime(
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        );

        let result = finish_runtime_after_winit(Ok(nara::app::AppExit::Success), runtime);

        assert!(result.is_ok());
    }

    #[test]
    fn prior_runner_error_survives_successful_retirement() {
        let runtime = start_runtime(
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        );
        let prior = nara::app::AppRunError::runner("injected runner failure");

        let error = finish_runtime_after_winit(Err(prior.clone()), runtime).unwrap_err();

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

        let prior = nara::app::AppRunError::runner("runner and close both failed");
        let mut error = finish_runtime_after_winit(Err(prior.clone()), runtime).unwrap_err();
        let failure = error
            .downcast_mut::<RuntimeRetirementFailure>()
            .expect("incomplete retirement keeps the typed owner error");
        assert_eq!(failure.prior.as_ref(), Some(&prior));

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
    fn initial_retirement_incomplete_is_retried_once_with_the_runtime_policy() {
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

        let result = finish_runtime_after_winit(Ok(nara::app::AppExit::Success), runtime);

        assert!(result.is_ok());
        assert!(polls.load(Ordering::SeqCst) > polls_before_retry);
    }
}
