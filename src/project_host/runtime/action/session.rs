//! Caller-driven Headless execution retains the original product Host between operations.

use std::time::Duration;

use nara_app::{
    AppExit, RuntimeClosePolicy, RuntimeControl, RuntimeControlStatus, RuntimeFault, RuntimeState,
};
use nara_diagnostic::DiagnosticReport;
use nara_ecs::World;
use nara_fs::DirectoryCapability;
use nara_gameplay::{
    GameplayCommandKey, GameplayCommandRejection, GameplayCommandSubmission,
    submit_gameplay_driver_command,
};

use super::{
    CleanupDriveOutcome, HostFault, PreparedProjectStart, ProjectHost,
    StartupContentClosureReceipt, prepare_project_start, prepare_runtime_content_start,
};
use crate::ProductRecipe;

/// Failure of an explicit operation on a retained Headless product.
#[derive(Debug, thiserror::Error)]
pub enum HeadlessSessionError {
    /// The session has not started, has failed, or is closing or closed.
    #[error("the Headless session is unavailable for this operation")]
    Unavailable,
    /// Capture needs a healthy paused Runtime after a completed managed frame.
    #[error("the Headless session has no completed paused frame")]
    NotPaused,
    /// The existing managed control refused this request; the incumbent remains usable.
    #[error("the Headless Runtime control was rejected: {0:?}")]
    ControlRejected(nara_app::RuntimeControlRejection),
    /// The original gameplay queue refused this command without accepting it.
    #[error("the Headless semantic command was rejected: {0}")]
    CommandRejected(GameplayCommandRejection),
    /// Admission ran before the managed scope observed a fault; its output remains authoritative.
    #[error("the Headless Runtime faulted after semantic-command admission")]
    CommandFault {
        outcome: Result<GameplayCommandKey, GameplayCommandRejection>,
        fault: RuntimeFault,
    },
    /// Source admission, managed execution or authority validation failed.
    #[error("the Headless product operation failed")]
    Runtime(DiagnosticReport),
}

/// Physical close observation for the original Headless Host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeadlessSessionClose {
    /// The original owner still retains work; call `close` again to drive it.
    Pending,
    /// Physical retirement completed without a reported failure.
    Complete,
    /// Physical retirement completed, but startup, execution or close reported failure.
    Failed,
}

enum Inputs {
    Source {
        root: DirectoryCapability,
        recipe: ProductRecipe,
    },
    Processed(crate::runtime_content::RuntimeContentBase),
}

/// A source or processed Headless product retained across caller operations.
///
/// This facade owns the same private product Host used by [`super::HeadlessRun`]. The caller
/// selects when to drive frames, pause, step and close; it cannot replace the published World.
/// `capture` requires a completed paused frame, while the game must additionally validate
/// its command watermarks, pending services and complete semantic state. Native resources and
/// Runtime authority are never a snapshot format.
///
/// Drive `close` until it returns a terminal observation. Dropping this value is not evidence
/// of physical completion, and a closed or failed session cannot start a replacement Runtime.
pub struct HeadlessSession {
    inputs: Option<Inputs>,
    host: Option<ProjectHost>,
    diagnostics: DiagnosticReport,
    startup_content_closure: Option<StartupContentClosureReceipt>,
    paused_boundary: bool,
    exit: Option<AppExit>,
    closing: bool,
    closed: bool,
    failed: bool,
}

impl std::fmt::Debug for HeadlessSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HeadlessSession")
            .field("state", &self.state())
            .field("closing", &self.closing)
            .field("closed", &self.closed)
            .field("diagnostics", &self.diagnostics)
            .finish_non_exhaustive()
    }
}

impl HeadlessSession {
    /// Selects a source product without starting it or installing a toolchain.
    #[must_use]
    pub fn new(root: DirectoryCapability, recipe: ProductRecipe) -> Self {
        Self::with_inputs(Inputs::Source { root, recipe })
    }

    /// Selects the immutable executable plan and resources of an admitted Runtime base.
    #[must_use]
    pub fn from_runtime_content(base: crate::runtime_content::RuntimeContentBase) -> Self {
        Self::with_inputs(Inputs::Processed(base))
    }

    fn with_inputs(inputs: Inputs) -> Self {
        Self {
            inputs: Some(inputs),
            host: None,
            diagnostics: DiagnosticReport::default(),
            startup_content_closure: None,
            paused_boundary: false,
            exit: None,
            closing: false,
            closed: false,
            failed: false,
        }
    }

    /// Admits and starts exactly one product through the original Host lifecycle.
    ///
    /// Failure can retain a cleanup owner; call `close` until physical completion is observed.
    pub fn start(&mut self) -> Result<(), HeadlessSessionError> {
        if self.closed || self.closing || self.host.is_some() {
            return Err(HeadlessSessionError::Unavailable);
        }
        let inputs = self
            .inputs
            .take()
            .ok_or(HeadlessSessionError::Unavailable)?;
        let prepared = match inputs {
            Inputs::Source { root, recipe } => prepare_project_start(
                root,
                None,
                RuntimeClosePolicy::default(),
                recipe,
                None,
                None,
                Vec::new(),
            ),
            Inputs::Processed(base) => {
                prepare_runtime_content_start(base, RuntimeClosePolicy::default(), Vec::new())
            }
        };
        let PreparedProjectStart {
            mut host,
            mut attempt,
            plan_diagnostics,
            startup_content_closure,
        } = match prepared {
            Ok(prepared) => prepared,
            Err(error) => {
                self.startup_content_closure = error.startup_content_closure;
                return Err(self.record_fault(error.fault));
            }
        };
        self.diagnostics.extend(plan_diagnostics);
        self.startup_content_closure = startup_content_closure;
        let result = host.complete_start(&mut attempt);
        self.host = Some(host);
        match result {
            Ok(diagnostics) => {
                self.diagnostics.extend(diagnostics);
                Ok(())
            }
            Err(fault) => Err(self.record_fault(fault)),
        }
    }

    /// Observes the actual managed state; a missing value does not attest physical close.
    #[must_use]
    pub fn state(&self) -> Option<RuntimeState> {
        self.host
            .as_ref()
            .and_then(ProjectHost::running_runtime_state)
    }

    /// Returns the process-local Runtime generation for observation, not cross-process authority.
    #[must_use]
    pub fn runtime_generation(&self) -> Option<u64> {
        self.host
            .as_ref()
            .and_then(ProjectHost::running_runtime_generation)
    }

    #[must_use]
    pub fn startup_content_closure(&self) -> Option<&StartupContentClosureReceipt> {
        self.startup_content_closure.as_ref()
    }

    #[must_use]
    pub fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    /// Runs one complete managed App frame under the existing time policy.
    pub fn frame(&mut self, real_delta: Duration) -> Result<Option<AppExit>, HeadlessSessionError> {
        let result = self.available_host()?.drive_one_frame(real_delta);
        self.finish_drive(result)
    }

    /// Enters Pause and completes the existing exact-step entry maintenance frame.
    pub fn pause(&mut self) -> Result<Option<AppExit>, HeadlessSessionError> {
        let result = self.available_host()?.prepare_exact_tick_run();
        self.finish_drive(result)
    }

    /// Executes one exact fixed tick, then a complete paused maintenance frame.
    ///
    /// The maintenance frame runs frame-level owners such as TaskUpdate and Last without a
    /// second fixed tick. Only its completed boundary permits subsequent capture.
    pub fn step(&mut self) -> Result<Option<AppExit>, HeadlessSessionError> {
        if self.state() != Some(RuntimeState::Paused) {
            return Err(HeadlessSessionError::NotPaused);
        }
        let result = self.available_host()?.drive_one_fixed_tick();
        match result {
            Ok(None) => {
                self.paused_boundary = false;
                let maintenance = self.available_host()?.drive_one_frame(Duration::ZERO);
                self.finish_drive(maintenance)
            }
            result => self.finish_drive(result),
        }
    }

    /// Applies Resume at the original control boundary and completes that managed App frame.
    pub fn resume(&mut self) -> Result<Option<AppExit>, HeadlessSessionError> {
        let host = self.available_host()?;
        let ticket = match host.request_runtime_control(RuntimeControl::Resume) {
            Ok(ticket) => ticket,
            Err(fault) => return Err(self.operation_error(fault)),
        };
        let result = host.drive_one_frame(Duration::ZERO);
        let applied = host.runtime_control_status(ticket) == Some(RuntimeControlStatus::Applied);
        let exit = self.finish_drive(result)?;
        if !applied {
            return Err(HeadlessSessionError::Unavailable);
        }
        Ok(exit)
    }

    /// Reads the original published World under its Schema authority checks.
    pub fn observe<R>(
        &self,
        operation: impl FnOnce(&World) -> R,
    ) -> Result<R, HeadlessSessionError> {
        if self.closing
            || self.closed
            || self.failed
            || !matches!(
                self.state(),
                Some(RuntimeState::Running | RuntimeState::Paused)
            )
        {
            return Err(HeadlessSessionError::Unavailable);
        }
        self.host
            .as_ref()
            .ok_or(HeadlessSessionError::Unavailable)?
            .with_running_world(operation)
            .map_err(|fault| HeadlessSessionError::Runtime(fault.diagnostics))
    }

    /// Runs game-owned capture validation only at a healthy completed paused frame.
    ///
    /// The returned game value can itself be a validation result. This does not certify that
    /// future commands, authoritative services or unsupported game state are checkpointable.
    pub fn capture<R>(
        &self,
        operation: impl FnOnce(&World) -> R,
    ) -> Result<R, HeadlessSessionError> {
        if self.exit.is_some()
            || !self.paused_boundary
            || self.state() != Some(RuntimeState::Paused)
        {
            return Err(HeadlessSessionError::NotPaused);
        }
        self.observe(operation)
    }

    /// Admits one command through the existing typed gameplay driver port.
    ///
    /// Normal queue refusal preserves the live session. A managed fault after admission retains
    /// the actual output in `CommandFault`; callers must reconcile it before applying fault policy.
    pub fn submit(
        &mut self,
        command: GameplayCommandSubmission,
    ) -> Result<GameplayCommandKey, HeadlessSessionError> {
        let host = self.available_host()?;
        let runtime = host
            .running_runtime_mut()
            .ok_or(HeadlessSessionError::Unavailable)?;
        let outcome = runtime
            .with_driver_scope(|scope| submit_gameplay_driver_command(scope, command))
            .map_err(|error| match error {
                nara_app::RuntimeScopeError::Unavailable { .. } => {
                    HeadlessSessionError::Unavailable
                }
                nara_app::RuntimeScopeError::Faulted { fault } => HeadlessSessionError::Runtime(
                    super::super::runtime_scope_failure_report(&fault),
                ),
            })?;
        let output = match outcome.output {
            Ok(output) => output,
            Err(_) => {
                if let Some(fault) = outcome.post_operation_fault {
                    return Err(self.record_fault(HostFault::new(
                        super::super::runtime_scope_failure_report(&fault),
                    )));
                }
                return Err(HeadlessSessionError::Unavailable);
            }
        };
        if let Some(fault) = outcome.post_operation_fault {
            self.failed = true;
            self.diagnostics
                .extend(super::super::runtime_scope_failure_report(&fault));
            return Err(HeadlessSessionError::CommandFault {
                outcome: output,
                fault,
            });
        }
        output.map_err(HeadlessSessionError::CommandRejected)
    }

    /// Drives the original bounded physical close owner once; no replacement is started.
    pub fn close(&mut self) -> HeadlessSessionClose {
        if self.closed {
            return self.terminal_close();
        }
        self.inputs = None;
        self.paused_boundary = false;
        if !self.closing {
            self.closing = true;
            if let Some(host) = self.host.as_mut()
                && host.running_runtime_state().is_some()
                && let Err(fault) = host.stop_running()
            {
                self.failed |= fault.diagnostics.has_errors();
                self.diagnostics.extend(fault.diagnostics);
                host.retire_running();
            }
        }
        if let Some(host) = self.host.as_mut()
            && host.has_cleanup_owner()
        {
            match host.drive_cleanup_once() {
                CleanupDriveOutcome::Complete {
                    failed,
                    diagnostics,
                    ..
                } => {
                    self.failed |= failed;
                    self.diagnostics.extend(diagnostics);
                }
                CleanupDriveOutcome::Retiring | CleanupDriveOutcome::RetirementIncomplete => {
                    return HeadlessSessionClose::Pending;
                }
            }
        }
        self.closed = true;
        self.host = None;
        self.terminal_close()
    }

    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed
    }

    /// Returns the terminal AppExit observed by a drive; later execution and input are refused.
    #[must_use]
    pub fn exit(&self) -> Option<AppExit> {
        self.exit
    }

    fn terminal_close(&self) -> HeadlessSessionClose {
        if self.failed {
            HeadlessSessionClose::Failed
        } else {
            HeadlessSessionClose::Complete
        }
    }

    fn available_host(&mut self) -> Result<&mut ProjectHost, HeadlessSessionError> {
        if self.closing
            || self.closed
            || self.failed
            || self.exit.is_some()
            || !matches!(
                self.state(),
                Some(RuntimeState::Running | RuntimeState::Paused)
            )
        {
            return Err(HeadlessSessionError::Unavailable);
        }
        self.host.as_mut().ok_or(HeadlessSessionError::Unavailable)
    }

    fn finish_drive(
        &mut self,
        result: Result<Option<AppExit>, HostFault>,
    ) -> Result<Option<AppExit>, HeadlessSessionError> {
        match result {
            Ok(exit) => {
                self.exit = exit;
                self.paused_boundary = exit.is_none() && self.state() == Some(RuntimeState::Paused);
                Ok(exit)
            }
            Err(fault) => Err(self.operation_error(fault)),
        }
    }

    fn operation_error(&mut self, fault: HostFault) -> HeadlessSessionError {
        if let Some(rejection) = fault.control_rejection {
            HeadlessSessionError::ControlRejected(rejection)
        } else {
            self.paused_boundary = false;
            self.record_fault(fault)
        }
    }

    fn record_fault(&mut self, fault: HostFault) -> HeadlessSessionError {
        self.failed = true;
        self.diagnostics.extend(fault.diagnostics.clone());
        HeadlessSessionError::Runtime(fault.diagnostics)
    }
}
