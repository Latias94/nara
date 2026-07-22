#![forbid(unsafe_code)]

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use engine::{
    app::{
        App, CoreStage, RuntimeAdmissionReservation, RuntimeCandidateRetirementState,
        RuntimeClosePolicy, RuntimeControl, RuntimeControlRequestResult, RuntimeControlStatus,
        RuntimeControlTicket, RuntimeFaultKind, RuntimeInstance, RuntimeObligationLedger,
        RuntimeState,
    },
    ecs::{Res, Resource, error::BevyError},
};

const MAX_CLOSE_DRIVES: usize = 4;
const MAX_RETIREMENT_DRIVES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExternalRunReport {
    pub pause_applied: bool,
    pub exact_step_applied: bool,
    pub exact_step_runs: usize,
    pub observed_fault: RuntimeFaultKind,
    pub normal_stop_completed: bool,
    pub faulted_stop_completed: bool,
}

#[derive(Debug)]
pub enum ExternalRunnerError {
    Reservation(String),
    Configuration(String),
    Admission(String),
    Startup(RuntimeFaultKind),
    ControlRejected { control: &'static str },
    ControlNotApplied { control: &'static str },
    Drive(RuntimeFaultKind),
    ExactStepDidNotRun { observed: usize },
    FaultNotObserved { state: RuntimeState },
    UnexpectedFault(RuntimeFaultKind),
    StopIncomplete { state: RuntimeState },
}

impl Display for ExternalRunnerError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Reservation(error) => write!(formatter, "runtime reservation failed: {error}"),
            Self::Configuration(error) => {
                write!(formatter, "runtime configuration failed: {error}")
            }
            Self::Admission(error) => write!(formatter, "runtime admission failed: {error}"),
            Self::Startup(kind) => write!(formatter, "runtime startup faulted: {kind:?}"),
            Self::ControlRejected { control } => write!(formatter, "runtime rejected {control}"),
            Self::ControlNotApplied { control } => {
                write!(formatter, "runtime did not apply {control}")
            }
            Self::Drive(kind) => write!(formatter, "runtime drive faulted: {kind:?}"),
            Self::ExactStepDidNotRun { observed } => {
                write!(formatter, "exact step ran {observed} fixed systems")
            }
            Self::FaultNotObserved { state } => {
                write!(formatter, "expected a runtime fault, reached {state:?}")
            }
            Self::UnexpectedFault(kind) => write!(formatter, "unexpected runtime fault: {kind:?}"),
            Self::StopIncomplete { state } => {
                write!(
                    formatter,
                    "runtime did not stop within the bounded loop: {state:?}"
                )
            }
        }
    }
}

impl Error for ExternalRunnerError {}

#[derive(Resource)]
struct FixedStepProbe(Arc<AtomicUsize>);

#[derive(Debug)]
struct FixtureSystemFailure;

impl Display for FixtureSystemFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("external runner fault probe")
    }
}

impl Error for FixtureSystemFailure {}

fn count_exact_steps(probe: Res<FixedStepProbe>) {
    probe.0.fetch_add(1, Ordering::SeqCst);
}

fn fail_in_variable_update() -> Result<(), BevyError> {
    Err(BevyError::error(FixtureSystemFailure))
}

pub fn run() -> Result<ExternalRunReport, ExternalRunnerError> {
    let (mut normal, fixed_steps) = start_runtime(false)?;

    let pause = request_control(&mut normal, RuntimeControl::Pause, "pause")?;
    apply_control(&mut normal, pause, RuntimeState::Paused, "pause")?;

    let exact_step = request_control(&mut normal, RuntimeControl::StepFixedTick, "exact step")?;
    apply_control(&mut normal, exact_step, RuntimeState::Paused, "exact step")?;
    let exact_step_runs = fixed_steps.load(Ordering::SeqCst);
    if exact_step_runs != 1 {
        return Err(ExternalRunnerError::ExactStepDidNotRun {
            observed: exact_step_runs,
        });
    }
    stop_runtime(&mut normal)?;

    let (mut faulting, _) = start_runtime(true)?;
    let observed_fault = match faulting.drive(Duration::ZERO) {
        Err(error) => error.fault().kind(),
        Ok(outcome) => {
            return Err(ExternalRunnerError::FaultNotObserved {
                state: outcome.state(),
            });
        }
    };
    if faulting.state() != RuntimeState::Faulted {
        return Err(ExternalRunnerError::FaultNotObserved {
            state: faulting.state(),
        });
    }
    if observed_fault != RuntimeFaultKind::System {
        return Err(ExternalRunnerError::UnexpectedFault(observed_fault));
    }
    stop_runtime(&mut faulting)?;

    Ok(ExternalRunReport {
        pause_applied: true,
        exact_step_applied: true,
        exact_step_runs,
        observed_fault,
        normal_stop_completed: true,
        faulted_stop_completed: true,
    })
}

fn start_runtime(
    inject_variable_update_fault: bool,
) -> Result<(RuntimeInstance, Arc<AtomicUsize>), ExternalRunnerError> {
    let reservation = RuntimeAdmissionReservation::try_acquire()
        .map_err(|error| ExternalRunnerError::Reservation(error.to_string()))?;
    let fixed_steps = Arc::new(AtomicUsize::new(0));
    let mut app = App::new();
    app.insert_resource(FixedStepProbe(Arc::clone(&fixed_steps)))
        .map_err(|error| ExternalRunnerError::Configuration(error.to_string()))?;
    app.add_systems(CoreStage::FixedUpdate, count_exact_steps)
        .map_err(|error| ExternalRunnerError::Configuration(error.to_string()))?;
    if inject_variable_update_fault {
        app.add_systems(CoreStage::Update, fail_in_variable_update)
            .map_err(|error| ExternalRunnerError::Configuration(error.to_string()))?;
    }
    let sealed = app
        .seal()
        .map_err(|error| ExternalRunnerError::Configuration(error.to_string()))?;
    let candidate = match reservation.admit(
        sealed,
        RuntimeObligationLedger::new(),
        RuntimeClosePolicy::default(),
    ) {
        Ok(candidate) => candidate,
        Err(failure) => {
            let error = failure.error().to_string();
            let mut retirement = failure.begin_retirement();
            for _ in 0..MAX_RETIREMENT_DRIVES {
                if retirement.drive_retirement() == RuntimeCandidateRetirementState::Retired {
                    break;
                }
            }
            return Err(ExternalRunnerError::Admission(error));
        }
    };
    let ready = match candidate.complete_startup() {
        Ok(ready) => ready,
        Err(mut failure) => {
            let fault = failure.fault().kind();
            for _ in 0..MAX_RETIREMENT_DRIVES {
                if failure.drive_retirement() == RuntimeCandidateRetirementState::Retired {
                    break;
                }
            }
            return Err(ExternalRunnerError::Startup(fault));
        }
    };

    Ok((ready.promote(), fixed_steps))
}

fn request_control(
    runtime: &mut RuntimeInstance,
    control: RuntimeControl,
    label: &'static str,
) -> Result<RuntimeControlTicket, ExternalRunnerError> {
    match runtime.request_control(control) {
        RuntimeControlRequestResult::Accepted(ticket) => Ok(ticket),
        RuntimeControlRequestResult::Rejected(_) => {
            Err(ExternalRunnerError::ControlRejected { control: label })
        }
    }
}

fn apply_control(
    runtime: &mut RuntimeInstance,
    ticket: RuntimeControlTicket,
    expected_state: RuntimeState,
    label: &'static str,
) -> Result<(), ExternalRunnerError> {
    let outcome = runtime
        .drive(Duration::ZERO)
        .map_err(|error| ExternalRunnerError::Drive(error.fault().kind()))?;
    if outcome.state() != expected_state
        || runtime.control_status(ticket) != Some(RuntimeControlStatus::Applied)
    {
        return Err(ExternalRunnerError::ControlNotApplied { control: label });
    }
    Ok(())
}

fn stop_runtime(runtime: &mut RuntimeInstance) -> Result<(), ExternalRunnerError> {
    let ticket = request_control(runtime, RuntimeControl::Stop, "stop")?;
    for _ in 0..MAX_CLOSE_DRIVES {
        let outcome = runtime
            .drive(Duration::ZERO)
            .map_err(|error| ExternalRunnerError::Drive(error.fault().kind()))?;
        if outcome.state() == RuntimeState::Stopped {
            if runtime.control_status(ticket) == Some(RuntimeControlStatus::Applied) {
                return Ok(());
            }
            return Err(ExternalRunnerError::ControlNotApplied { control: "stop" });
        }
    }
    Err(ExternalRunnerError::StopIncomplete {
        state: runtime.state(),
    })
}

#[cfg(test)]
mod tests {
    use super::{RuntimeFaultKind, run};

    #[test]
    fn concrete_runner_uses_the_public_managed_runtime_contract() {
        let report = run().expect("the external concrete loop must complete");

        assert!(report.pause_applied);
        assert!(report.exact_step_applied);
        assert_eq!(report.exact_step_runs, 1);
        assert_eq!(report.observed_fault, RuntimeFaultKind::System);
        assert!(report.normal_stop_completed);
        assert!(report.faulted_stop_completed);
    }
}
