use std::{
    cell::Cell,
    collections::VecDeque,
    fmt::{self, Debug, Display},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use nara_ecs::{
    Command, Resource, World, bevy_ecs::schedule::InternedScheduleLabel, schedule::ScheduleLabel,
};
use thiserror::Error;

use crate::{
    App, AppFrameOutcome, AppRunError, ExecutionProfile, FixedTime, RuntimeTimeContinuation,
    RuntimeTimeContinuationError, RuntimeTimeContinuationRejection, RuntimeTimeSettings, time,
};

mod close;
mod fault;

pub(crate) use close::Close;
pub use close::{
    RuntimeCandidateRetirementState, RuntimeCloseCause, RuntimeCloseContext,
    RuntimeCloseErrorDisposition, RuntimeCloseEvidence, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseParticipantPhase,
    RuntimeClosePendingParticipant, RuntimeClosePolicy, RuntimeCloseProgress,
    RuntimeWorldAccessError,
};
pub use fault::{
    RuntimeFault, RuntimeFaultDetail, RuntimeFaultDetailError, RuntimeFaultKind,
    RuntimeFaultReporter, RuntimeValidator,
};
pub(crate) use fault::{execute, handle_execution_error};

static NEXT_RUNTIME_GENERATION: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Resource)]
pub struct RuntimeGeneration(u64);

impl RuntimeGeneration {
    pub const fn get(self) -> u64 {
        self.0
    }

    pub(crate) fn allocate() -> Result<Self, AppRunError> {
        NEXT_RUNTIME_GENERATION
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .map(Self)
            .map_err(|_| AppRunError::runner("runtime generation exhausted"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeState {
    Running,
    Paused,
    Stepping,
    Faulted,
    Stopping,
    CloseIncomplete,
    Stopped,
}

impl RuntimeState {
    fn is_live(self) -> bool {
        matches!(self, Self::Running | Self::Paused)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeControl {
    Pause,
    Resume,
    StepFixedTick,
    Stop,
    RetryClose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuntimeControlTicket {
    generation: RuntimeGeneration,
    sequence: u64,
}

impl RuntimeControlTicket {
    pub const fn get(self) -> u64 {
        self.sequence
    }

    pub const fn generation(self) -> RuntimeGeneration {
        self.generation
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeControlFailure {
    SupersededByStop,
    RuntimeFaulted,
    CloseFailed,
    CloseIncomplete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeControlStatus {
    Pending,
    Applied,
    Failed(RuntimeControlFailure),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeControlRejection {
    Busy,
    InvalidState {
        state: RuntimeState,
        control: RuntimeControl,
    },
    TicketExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeControlRequestResult {
    Accepted(RuntimeControlTicket),
    Rejected(RuntimeControlRejection),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeFrameKind {
    FullFrame,
    ExactFixedTick,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeDriveOutcome {
    state: RuntimeState,
    frame: Option<(RuntimeFrameKind, AppFrameOutcome)>,
}

impl RuntimeDriveOutcome {
    pub const fn state(self) -> RuntimeState {
        self.state
    }

    pub const fn frame(self) -> Option<AppFrameOutcome> {
        match self.frame {
            Some((_, frame)) => Some(frame),
            None => None,
        }
    }

    pub const fn frame_kind(self) -> Option<RuntimeFrameKind> {
        match self.frame {
            Some((kind, _)) => Some(kind),
            None => None,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum RuntimeDriveError {
    #[error("runtime faulted")]
    Faulted { fault: RuntimeFault },
}

impl RuntimeDriveError {
    pub const fn fault(&self) -> &RuntimeFault {
        match self {
            Self::Faulted { fault } => fault,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum RuntimeScheduleError {
    #[error("custom schedule execution is unavailable in state {state:?}")]
    Unavailable { state: RuntimeState },
    #[error("custom schedule execution is unavailable while a runtime control is pending")]
    ControlPending,
    #[error("built-in schedule {schedule:?} is driven by the runtime")]
    BuiltIn { schedule: InternedScheduleLabel },
    #[error("custom schedule {schedule:?} is missing")]
    Missing { schedule: InternedScheduleLabel },
    #[error("custom schedule execution faulted")]
    Faulted { fault: RuntimeFault },
}

impl RuntimeScheduleError {
    pub const fn fault(&self) -> Option<&RuntimeFault> {
        match self {
            Self::Faulted { fault } => Some(fault),
            _ => None,
        }
    }
}

pub struct RuntimeDriverScope<'a> {
    world: &'a mut World,
    state: RuntimeState,
}

impl RuntimeDriverScope<'_> {
    pub const fn state(&self) -> RuntimeState {
        self.state
    }

    pub fn apply_command<C: Command>(&mut self, command: C) -> C::Out {
        command.apply(self.world)
    }

    pub fn with_world_mut<R>(&mut self, operation: impl FnOnce(&mut World) -> R) -> R {
        operation(self.world)
    }
}

#[derive(Debug)]
pub struct RuntimeScopeOutcome<R> {
    pub output: R,
    /// A fault observed after the operation produced `output`.
    pub post_operation_fault: Option<RuntimeFault>,
}

#[derive(Debug, Error, Clone, PartialEq)]
pub enum RuntimeScopeError {
    #[error("runtime world access is unavailable in state {state:?}")]
    Unavailable { state: RuntimeState },
    #[error("runtime faulted during world access")]
    Faulted { fault: RuntimeFault },
}

impl RuntimeScopeError {
    pub const fn fault(&self) -> Option<&RuntimeFault> {
        match self {
            Self::Faulted { fault } => Some(fault),
            _ => None,
        }
    }
}

/// Drives a started App and its close participants on the thread that owns the World.
pub struct RuntimeInstance {
    app: App,
    generation: RuntimeGeneration,
    state: RuntimeState,
    pending: Option<(RuntimeControlTicket, RuntimeControl)>,
    close_ticket: Option<RuntimeControlTicket>,
    stop: Option<(RuntimeControlTicket, RuntimeControlStatus)>,
    recent: VecDeque<(RuntimeControlTicket, RuntimeControlStatus)>,
    next_ticket: u64,
    last_frame: Option<RuntimeFrameKind>,
    fault_delivered: Cell<bool>,
}

impl Debug for RuntimeInstance {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeInstance")
            .field("generation", &self.generation)
            .field("state", &self.state())
            .field("fault", &self.fault())
            .finish_non_exhaustive()
    }
}

impl RuntimeInstance {
    pub fn start(app: App) -> Result<Self, AppRunError> {
        Self::start_with_close_policy(app, RuntimeClosePolicy::default())
    }

    pub fn start_with_close_policy(
        mut app: App,
        policy: RuntimeClosePolicy,
    ) -> Result<Self, AppRunError> {
        app.close.policy = policy;
        let initial = app.start().and_then(|generation| {
            Ok((
                generation,
                time::get::<RuntimeTimeSettings>(&app.world)?.paused(),
            ))
        });
        let (generation, paused) = match initial {
            Ok(value) => value,
            Err(error) => {
                return Err(match RuntimeRetirement::new(app, true).finish() {
                    Ok(_) => error,
                    Err(retirement) => AppRunError::runner_teardown(
                        error,
                        AppRunError::runner(retirement.to_string()),
                    ),
                });
            }
        };
        let state = if paused {
            RuntimeState::Paused
        } else {
            RuntimeState::Running
        };
        Ok(Self {
            app,
            generation,
            state,
            pending: None,
            close_ticket: None,
            stop: None,
            recent: VecDeque::new(),
            next_ticket: 1,
            last_frame: None,
            fault_delivered: Cell::new(false),
        })
    }

    pub const fn generation(&self) -> RuntimeGeneration {
        self.generation
    }

    pub const fn execution_profile(&self) -> ExecutionProfile {
        self.app.execution_profile()
    }

    pub fn state(&self) -> RuntimeState {
        if self.state.is_live() && self.fault().is_some() {
            RuntimeState::Faulted
        } else {
            self.state
        }
    }

    pub fn close_evidence(&self) -> &RuntimeCloseEvidence {
        &self.app.close.evidence
    }

    pub fn world(&self) -> &World {
        &self.app.world
    }

    pub fn fault(&self) -> Option<&RuntimeFault> {
        self.app.reporter.fault_ref()
    }

    pub fn fault_reporter(&self) -> RuntimeFaultReporter {
        self.app.reporter.clone()
    }

    pub fn begin_retirement(self) -> RuntimeRetirement {
        RuntimeRetirement::new(self.app, self.fault_delivered.into_inner())
    }

    pub fn finish(self) -> Result<RuntimeCloseEvidence, RuntimeRetirement> {
        self.begin_retirement().finish()
    }

    pub fn apply_command<C: Command>(
        &mut self,
        command: C,
    ) -> Result<RuntimeScopeOutcome<C::Out>, RuntimeScopeError> {
        self.run_scope(false, |scope| scope.apply_command(command))
    }

    /// Native adapters can reconcile pending work in a faulted or closing runtime.
    pub fn with_driver_scope<R>(
        &mut self,
        operation: impl FnOnce(&mut RuntimeDriverScope<'_>) -> R,
    ) -> Result<RuntimeScopeOutcome<R>, RuntimeScopeError> {
        self.run_scope(true, operation)
    }

    fn run_scope<R>(
        &mut self,
        allow_retirement: bool,
        operation: impl FnOnce(&mut RuntimeDriverScope<'_>) -> R,
    ) -> Result<RuntimeScopeOutcome<R>, RuntimeScopeError> {
        let state = self.state();
        if (!allow_retirement && !state.is_live())
            || matches!(state, RuntimeState::Stepping | RuntimeState::Stopped)
        {
            return Err(RuntimeScopeError::Unavailable { state });
        }
        let prior_fault = self.fault().is_some();
        let result = (|| {
            if state.is_live() {
                self.app.check_health()?;
            }
            let output = execute(|| {
                self.app.world.increment_change_tick();
                self.app.world.check_change_ticks();
                operation(&mut RuntimeDriverScope {
                    world: &mut self.app.world,
                    state,
                })
            })?;
            let post_operation_fault = if state.is_live() {
                self.app.check_health().err()
            } else if !prior_fault {
                self.app.reporter.fault()
            } else {
                None
            };
            Ok(RuntimeScopeOutcome {
                output,
                post_operation_fault,
            })
        })();
        match result {
            Ok(outcome) => {
                if outcome.post_operation_fault.is_some() {
                    self.fault_delivered.set(true);
                }
                Ok(outcome)
            }
            Err(fault) => {
                self.fault_delivered.set(true);
                Err(RuntimeScopeError::Faulted {
                    fault: self.app.reporter.record(fault),
                })
            }
        }
    }

    pub fn run_schedule(
        &mut self,
        schedule: impl ScheduleLabel,
    ) -> Result<(), RuntimeScheduleError> {
        let state = self.state();
        if state != RuntimeState::Running {
            return Err(RuntimeScheduleError::Unavailable { state });
        }
        if self.pending.is_some() {
            return Err(RuntimeScheduleError::ControlPending);
        }
        let schedule = schedule.intern();
        if crate::is_built_in_schedule(schedule) {
            return Err(RuntimeScheduleError::BuiltIn { schedule });
        }
        if self.app.get_schedule(schedule).is_none() {
            return Err(RuntimeScheduleError::Missing { schedule });
        }
        self.app.run_schedule(schedule).map_err(|fault| {
            self.fault_delivered.set(true);
            RuntimeScheduleError::Faulted { fault }
        })
    }

    pub fn request_control(&mut self, control: RuntimeControl) -> RuntimeControlRequestResult {
        use RuntimeControlRejection as Rejection;
        use RuntimeControlRequestResult::{Accepted, Rejected};
        let state = self.state();
        if control == RuntimeControl::Stop
            && let Some((ticket, _)) = self.stop
            && matches!(
                state,
                RuntimeState::Running
                    | RuntimeState::Paused
                    | RuntimeState::Faulted
                    | RuntimeState::Stopping
            )
        {
            return Accepted(ticket);
        }
        if self.pending.is_some() && control != RuntimeControl::Stop {
            return Rejected(Rejection::Busy);
        }
        if !matches!(
            (state, control),
            (
                RuntimeState::Running,
                RuntimeControl::Pause | RuntimeControl::Stop
            ) | (
                RuntimeState::Paused,
                RuntimeControl::Resume | RuntimeControl::StepFixedTick | RuntimeControl::Stop
            ) | (RuntimeState::Faulted, RuntimeControl::Stop)
                | (RuntimeState::CloseIncomplete, RuntimeControl::RetryClose)
        ) {
            return Rejected(Rejection::InvalidState { state, control });
        }

        let Some(next) = self.next_ticket.checked_add(1) else {
            return Rejected(Rejection::TicketExhausted);
        };
        if let Some((previous, _)) = self.pending.take() {
            self.set_status(
                previous,
                RuntimeControlStatus::Failed(RuntimeControlFailure::SupersededByStop),
            );
        }
        let ticket = RuntimeControlTicket {
            generation: self.generation,
            sequence: self.next_ticket,
        };
        self.next_ticket = next;
        self.pending = Some((ticket, control));
        if control == RuntimeControl::Stop {
            self.stop = Some((ticket, RuntimeControlStatus::Pending));
        } else {
            const RETAINED_CONTROLS: usize = 32;
            if self.recent.len() == RETAINED_CONTROLS {
                self.recent.pop_front();
            }
            self.recent
                .push_back((ticket, RuntimeControlStatus::Pending));
        }
        Accepted(ticket)
    }

    pub fn control_status(&self, ticket: RuntimeControlTicket) -> Option<RuntimeControlStatus> {
        self.stop
            .iter()
            .chain(self.recent.iter().rev())
            .find_map(|&(entry, status)| (ticket == entry).then_some(status))
    }

    fn set_status(&mut self, ticket: RuntimeControlTicket, status: RuntimeControlStatus) {
        if let Some((_, value)) = self
            .stop
            .iter_mut()
            .chain(self.recent.iter_mut().rev())
            .find(|entry| entry.0 == ticket)
        {
            *value = status;
        }
    }

    pub fn drive(&mut self, delta: Duration) -> Result<RuntimeDriveOutcome, RuntimeDriveError> {
        self.drive_interval(delta, false)
    }

    /// Settles elapsed time under the previous pause state when resuming a native session.
    pub fn drive_elapsed(
        &mut self,
        delta: Duration,
    ) -> Result<RuntimeDriveOutcome, RuntimeDriveError> {
        self.drive_interval(delta, self.state() == RuntimeState::Paused)
    }

    fn drive_interval(
        &mut self,
        delta: Duration,
        paused_interval: bool,
    ) -> Result<RuntimeDriveOutcome, RuntimeDriveError> {
        self.last_frame = None;
        let result = self.drive_inner(delta, paused_interval);
        match &result {
            Ok(outcome) if outcome.frame().is_some_and(|frame| frame.exit.is_none()) => {
                self.last_frame = outcome.frame_kind()
            }
            Err(_) => self.fault_delivered.set(true),
            _ => {}
        }
        result
    }

    fn drive_inner(
        &mut self,
        delta: Duration,
        paused_interval: bool,
    ) -> Result<RuntimeDriveOutcome, RuntimeDriveError> {
        if self.state() == RuntimeState::Faulted
            && !self
                .pending
                .is_some_and(|(_, control)| control == RuntimeControl::Stop)
        {
            if let Some((ticket, _)) = self.pending.take() {
                self.set_status(
                    ticket,
                    RuntimeControlStatus::Failed(RuntimeControlFailure::RuntimeFaulted),
                );
            }
            let fault = self
                .fault()
                .expect("a faulted runtime has a first fault")
                .clone();
            return Err(RuntimeDriveError::Faulted { fault });
        }

        if let Some((ticket, control)) = self.pending.take() {
            match control {
                RuntimeControl::Stop | RuntimeControl::RetryClose => {
                    self.state = RuntimeState::Stopping;
                    self.app.close.begin();
                    self.close_ticket = Some(ticket);
                    return Ok(RuntimeDriveOutcome {
                        state: self.state,
                        frame: None,
                    });
                }
                RuntimeControl::Pause | RuntimeControl::Resume => {
                    let paused = control == RuntimeControl::Pause;
                    let result = time::get_mut::<RuntimeTimeSettings>(&mut self.app.world)
                        .map(|mut settings| settings.set_paused(paused))
                        .map_err(AppRunError::from);
                    self.finish_control(ticket, result)?;
                    self.state = if paused {
                        RuntimeState::Paused
                    } else {
                        RuntimeState::Running
                    };
                }
                RuntimeControl::StepFixedTick => {
                    let result = time::get::<FixedTime>(&self.app.world)
                        .map(FixedTime::timestep)
                        .map_err(AppRunError::from)
                        .and_then(|step| self.app.run_frame(delta, false, Some(step)));
                    let frame = self.finish_control(ticket, result)?;
                    return Ok(RuntimeDriveOutcome {
                        state: self.state,
                        frame: Some((RuntimeFrameKind::ExactFixedTick, frame)),
                    });
                }
            }
        }

        let frame = match self.state {
            RuntimeState::Running | RuntimeState::Paused => {
                let result = self.app.run_frame(
                    delta,
                    paused_interval || self.state == RuntimeState::Paused,
                    None,
                );
                let frame = result.map_err(|error| self.drive_error(error))?;
                Some((RuntimeFrameKind::FullFrame, frame))
            }
            RuntimeState::Stopping => {
                self.app.drive_close();
                self.state = match self.app.close.state() {
                    RuntimeCandidateRetirementState::Retiring => RuntimeState::Stopping,
                    RuntimeCandidateRetirementState::RetirementIncomplete => {
                        RuntimeState::CloseIncomplete
                    }
                    RuntimeCandidateRetirementState::Retired => RuntimeState::Stopped,
                };
                self.finish_close_control();
                None
            }
            _ => None,
        };
        Ok(RuntimeDriveOutcome {
            state: self.state,
            frame,
        })
    }

    fn drive_error(&self, error: AppRunError) -> RuntimeDriveError {
        RuntimeDriveError::Faulted {
            fault: self.app.reporter.record(RuntimeFault::app(error)),
        }
    }

    fn finish_control<T>(
        &mut self,
        ticket: RuntimeControlTicket,
        result: Result<T, AppRunError>,
    ) -> Result<T, RuntimeDriveError> {
        let status = if result.is_ok() {
            RuntimeControlStatus::Applied
        } else {
            RuntimeControlStatus::Failed(RuntimeControlFailure::RuntimeFaulted)
        };
        self.set_status(ticket, status);
        result.map_err(|error| self.drive_error(error))
    }

    fn finish_close_control(&mut self) {
        let status = match self.state {
            RuntimeState::Stopped if self.close_evidence().has_terminal_failure() => {
                RuntimeControlStatus::Failed(RuntimeControlFailure::CloseFailed)
            }
            RuntimeState::Stopped => RuntimeControlStatus::Applied,
            RuntimeState::CloseIncomplete => {
                RuntimeControlStatus::Failed(RuntimeControlFailure::CloseIncomplete)
            }
            _ => return,
        };
        if let Some(ticket) = self.close_ticket.take() {
            self.set_status(ticket, status);
        }
    }

    pub fn capture_time_continuation(
        &self,
    ) -> Result<RuntimeTimeContinuation, RuntimeTimeContinuationError> {
        self.check_time_continuation()?;
        Ok(RuntimeTimeContinuation::capture(&self.app.world)?)
    }

    pub fn restore_time_continuation(
        &mut self,
        continuation: RuntimeTimeContinuation,
    ) -> Result<(), RuntimeTimeContinuationError> {
        self.check_time_continuation()?;
        let outcome = self
            .run_scope(false, |scope| {
                scope.with_world_mut(|world| continuation.apply(world))
            })
            .map_err(|error| match error {
                RuntimeScopeError::Faulted { fault } => continuation_fault(fault),
                RuntimeScopeError::Unavailable { state } => {
                    RuntimeTimeContinuationRejection::Unavailable { state }.into()
                }
            })?;
        let applied = outcome.output.is_ok();
        if applied {
            self.last_frame = None;
        }
        if let Some(fault) = outcome.post_operation_fault {
            return Err(RuntimeTimeContinuationError::Faulted {
                applied,
                rejection: outcome.output.err(),
                fault,
            });
        }
        Ok(outcome.output?)
    }

    fn check_time_continuation(&self) -> Result<(), RuntimeTimeContinuationError> {
        let state = self.state();
        if let Some(fault) = self.fault() {
            self.fault_delivered.set(true);
            return Err(continuation_fault(fault.clone()));
        }
        if state != RuntimeState::Paused {
            return Err(RuntimeTimeContinuationRejection::Unavailable { state }.into());
        }
        self.app.check_health().map_err(|fault| {
            self.fault_delivered.set(true);
            continuation_fault(fault)
        })?;
        if self.pending.is_some() {
            return Err(RuntimeTimeContinuationRejection::ControlPending.into());
        }
        if self.last_frame != Some(RuntimeFrameKind::FullFrame) {
            return Err(RuntimeTimeContinuationRejection::IncompletePausedFrame.into());
        }
        Ok(())
    }
}

fn continuation_fault(fault: RuntimeFault) -> RuntimeTimeContinuationError {
    RuntimeTimeContinuationError::Faulted {
        applied: false,
        rejection: None,
        fault,
    }
}

/// Holds the App until native teardown completes.
#[must_use]
pub struct RuntimeRetirement {
    app: App,
    fault_delivered: bool,
}

impl RuntimeRetirement {
    pub(crate) fn new(mut app: App, fault_delivered: bool) -> Self {
        app.close.begin();
        Self {
            app,
            fault_delivered,
        }
    }

    pub fn world(&self) -> &World {
        &self.app.world
    }

    pub fn retirement_state(&self) -> RuntimeCandidateRetirementState {
        self.app.close.state()
    }

    pub fn close_evidence(&self) -> &RuntimeCloseEvidence {
        &self.app.close.evidence
    }

    pub fn fault(&self) -> Option<&RuntimeFault> {
        self.app.reporter.fault_ref()
    }

    pub fn drive_retirement(&mut self) -> RuntimeCandidateRetirementState {
        self.app.close.begin();
        self.app.drive_close();
        self.retirement_state()
    }

    pub(crate) fn finish(mut self) -> Result<RuntimeCloseEvidence, Self> {
        self.app.finish_close();
        if self.retirement_state() == RuntimeCandidateRetirementState::Retired
            && !self.close_evidence().has_terminal_failure()
            && (self.fault_delivered || self.fault().is_none())
        {
            Ok(self.close_evidence().clone())
        } else {
            Err(self)
        }
    }
}

impl Debug for RuntimeRetirement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeRetirement")
            .field("state", &self.retirement_state())
            .field("evidence", self.close_evidence())
            .field("fault", &self.fault())
            .finish_non_exhaustive()
    }
}

impl Display for RuntimeRetirement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if !self.fault_delivered
            && let Some(fault) = self.fault()
        {
            write!(formatter, "runtime fault {fault:?}, ")?;
        }
        write!(
            formatter,
            "runtime close {:?}: {:?}",
            self.retirement_state(),
            self.close_evidence().causes()
        )
    }
}

impl std::error::Error for RuntimeRetirement {}
