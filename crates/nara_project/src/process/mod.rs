//! Concrete native process custody, bounded output, and physical cleanup for project tools.
//!
//! Consumers own launch-record semantics and lifecycle decisions. This module owns the original
//! child, its native containment, and joined output readers. Only complete physical cleanup and
//! drained output permit the original owner to issue a cleanup receipt.

use std::{
    ffi::{OsStr, OsString},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use nara_core::vfs::capabilities::RestrictiveCapabilityHandleLease;
use thiserror::Error;

mod log;
#[cfg(any(test, windows, all(target_os = "linux", target_arch = "x86_64")))]
mod output;
#[cfg(not(windows))]
mod unix;
mod wake;
#[cfg(windows)]
mod windows;

pub use log::{
    ProcessLogAppendOutcome, ProcessLogEntry, ProcessLogLimits, ProcessLogLimitsError,
    ProcessLogPurge, ProcessLogRing, ProcessLogSequence, ProcessLogSequenceExhausted,
    ProcessLogStats, ProcessLogStream,
};
pub use wake::SupervisedProcessWake;

#[cfg(not(windows))]
use unix as platform;
#[cfg(windows)]
use windows as platform;

const DEFAULT_TERMINATE_GRACE: Duration = Duration::from_secs(2);
const DEFAULT_NATURAL_EXIT_GRACE: Duration = Duration::from_millis(250);
const DEFAULT_FORCE_KILL_GRACE: Duration = Duration::from_secs(2);
const DEFAULT_OUTPUT_DRAIN_GRACE: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessOutputStream {
    Stdout,
    Stderr,
}

#[cfg(any(test, windows, all(target_os = "linux", target_arch = "x86_64")))]
use ProcessOutputStream as SupervisedProcessStream;

struct PlatformOutput {
    stream: ProcessOutputStream,
    bytes: Vec<u8>,
    loss: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisedProcessOutput {
    stream: ProcessOutputStream,
    bytes: Vec<u8>,
    loss: bool,
}

impl SupervisedProcessOutput {
    pub const fn stream(&self) -> ProcessOutputStream {
        self.stream
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub const fn loss(&self) -> bool {
        self.loss
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisedProcessCommand {
    executable: PathBuf,
    arguments: Vec<OsString>,
    working_directory: PathBuf,
    environment: Vec<(OsString, OsString)>,
    bootstrap: bool,
    protocol_stdout_limit: Option<NonZeroUsize>,
}

/// Selects the only ambient environment facts a supervised product may observe.
///
/// Toolchain commands use `SupervisedProcessCommand` and retain their existing inherited
/// environment contract. Exact-executable product launches start from an empty environment and
/// may opt into only this reviewed desktop-session projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisedProductEnvironmentPolicy {
    Sealed,
    AllowlistedDesktopSession,
}

impl SupervisedProductEnvironmentPolicy {
    pub const fn allowlisted_names(self) -> &'static [&'static str] {
        match self {
            Self::Sealed => &[],
            Self::AllowlistedDesktopSession => DESKTOP_SESSION_ENVIRONMENT_ALLOWLIST,
        }
    }

    pub fn allows_name(self, name: &str) -> bool {
        self.allowlisted_names().contains(&name)
    }
}

#[cfg(windows)]
const DESKTOP_SESSION_ENVIRONMENT_ALLOWLIST: &[&str] = &["SystemRoot", "WINDIR", "TEMP", "TMP"];

#[cfg(unix)]
const DESKTOP_SESSION_ENVIRONMENT_ALLOWLIST: &[&str] = &[
    "DISPLAY",
    "XAUTHORITY",
    "WAYLAND_DISPLAY",
    "XDG_RUNTIME_DIR",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
];

#[cfg(not(any(unix, windows)))]
const DESKTOP_SESSION_ENVIRONMENT_ALLOWLIST: &[&str] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupervisedProductProcessCommand {
    arguments: Vec<OsString>,
    working_directory: PathBuf,
    environment_policy: SupervisedProductEnvironmentPolicy,
    bootstrap: bool,
}

impl SupervisedProductProcessCommand {
    /// Creates a product command with closed stdin and no bootstrap endpoint.
    pub fn new(
        environment_policy: SupervisedProductEnvironmentPolicy,
        working_directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            arguments: Vec::new(),
            working_directory: working_directory.into(),
            environment_policy,
            bootstrap: false,
        }
    }

    pub fn arguments(mut self, arguments: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        self.arguments.extend(arguments.into_iter().map(Into::into));
        self
    }

    /// Reserves stdin for the inherited duplex bootstrap endpoint.
    pub fn with_bootstrap(mut self) -> Self {
        self.bootstrap = true;
        self
    }

    pub fn arguments_ref(&self) -> &[OsString] {
        &self.arguments
    }

    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    pub const fn environment_policy(&self) -> SupervisedProductEnvironmentPolicy {
        self.environment_policy
    }

    pub const fn bootstrap_requested(&self) -> bool {
        self.bootstrap
    }
}

/// Move-only authority to one already-verified executable and its immediate directory.
///
/// The platform adapter owns the exact executable and directory handles. This authority does
/// not expose a raw executable path as a launch capability or validate consumer record semantics.
#[must_use = "exact executable authority must be consumed by one immediate spawn attempt"]
pub struct ExactExecutableLaunchAuthority {
    inner: platform::ExactExecutableLaunchAuthority,
    launch_record: Option<RestrictiveCapabilityHandleLease>,
}

impl ExactExecutableLaunchAuthority {
    pub fn from_verified_handles(
        executable: RestrictiveCapabilityHandleLease,
        directory: RestrictiveCapabilityHandleLease,
    ) -> std::io::Result<Self> {
        platform::ExactExecutableLaunchAuthority::new(executable, directory).map(|inner| Self {
            inner,
            launch_record: None,
        })
    }

    /// Retains physical custody of one consumer-owned immutable launch-record lease.
    ///
    /// This only retains physical custody; it does not validate the record, its source, or their
    /// semantics. The lease moves into the original process owner and remains there throughout
    /// cleanup and any retained late-spawn custody. Calling this again replaces the prior lease.
    pub fn with_launch_record(mut self, launch_record: RestrictiveCapabilityHandleLease) -> Self {
        self.launch_record = Some(launch_record);
        self
    }
}

impl SupervisedProcessCommand {
    pub fn new(executable: impl Into<PathBuf>, working_directory: impl Into<PathBuf>) -> Self {
        Self {
            executable: executable.into(),
            arguments: Vec::new(),
            working_directory: working_directory.into(),
            environment: Vec::new(),
            bootstrap: false,
            protocol_stdout_limit: None,
        }
    }

    pub fn argument(mut self, argument: impl Into<OsString>) -> Self {
        self.arguments.push(argument.into());
        self
    }

    pub fn arguments(mut self, arguments: impl IntoIterator<Item = impl Into<OsString>>) -> Self {
        self.arguments.extend(arguments.into_iter().map(Into::into));
        self
    }

    pub fn environment(mut self, name: impl Into<OsString>, value: impl Into<OsString>) -> Self {
        self.environment.push((name.into(), value.into()));
        self
    }

    /// Captures exact protocol stdout up to a cumulative byte limit, separately from tail logs.
    /// Consuming output does not replenish the allowance; overflow or cancellation seals capture
    /// and reports reliable stdout loss before any remaining admitted bytes.
    pub fn bounded_stdout(mut self, limit: NonZeroUsize) -> Self {
        self.protocol_stdout_limit = Some(limit);
        self
    }

    pub const fn protocol_stdout_limit(&self) -> Option<NonZeroUsize> {
        self.protocol_stdout_limit
    }

    /// Reserves stdin for the inherited duplex bootstrap endpoint.
    pub fn with_bootstrap(mut self) -> Self {
        self.bootstrap = true;
        self
    }

    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub fn arguments_ref(&self) -> &[OsString] {
        &self.arguments
    }

    pub fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    pub fn environment_ref(&self) -> &[(OsString, OsString)] {
        &self.environment
    }

    pub const fn bootstrap_requested(&self) -> bool {
        self.bootstrap
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupervisedProcessState {
    Running,
    Terminating,
    ForceKilling,
    Draining,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupervisedProcessExit {
    code: Option<i32>,
    success: bool,
}

impl SupervisedProcessExit {
    pub const fn code(self) -> Option<i32> {
        self.code
    }

    pub const fn success(self) -> bool {
        self.success
    }
}

impl From<std::process::ExitStatus> for SupervisedProcessExit {
    fn from(status: std::process::ExitStatus) -> Self {
        Self {
            code: status.code(),
            success: status.success(),
        }
    }
}

/// Physical cleanup evidence minted only by the original process owner after output is drained.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupervisedProcessCleanupReceipt {
    exit: SupervisedProcessExit,
    containment_empty: bool,
    stdout_reader_stopped: bool,
    stderr_reader_stopped: bool,
    output_drained: bool,
    termination_requested: bool,
    force_killed: bool,
}

impl SupervisedProcessCleanupReceipt {
    pub const fn exit(self) -> SupervisedProcessExit {
        self.exit
    }

    pub const fn containment_empty(self) -> bool {
        self.containment_empty
    }

    pub const fn readers_stopped(self) -> bool {
        self.stdout_reader_stopped && self.stderr_reader_stopped
    }

    /// Whether the owner invoked native termination before issuing this receipt.
    ///
    /// Includes explicit requests, automatic residual-tree termination after direct-child exit,
    /// and force-kill escalation or retries. A request may race with natural exit; this records
    /// the intervention attempt, not whether it killed a process. A false value does not prove
    /// that the application completed its own shutdown protocol.
    pub const fn termination_requested(self) -> bool {
        self.termination_requested
    }

    /// Whether cleanup used the force-kill escalation path.
    ///
    /// Native termination can occur without escalation; see [`Self::termination_requested`].
    pub const fn force_killed(self) -> bool {
        self.force_killed
    }

    pub const fn output_drained(self) -> bool {
        self.output_drained
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SupervisedProcessObservation {
    state: SupervisedProcessState,
    exit: Option<SupervisedProcessExit>,
    output_loss_observed: bool,
}

impl SupervisedProcessObservation {
    pub const fn state(self) -> SupervisedProcessState {
        self.state
    }

    pub const fn exit(self) -> Option<SupervisedProcessExit> {
        self.exit
    }

    pub const fn output_loss_observed(self) -> bool {
        self.output_loss_observed
    }
}

pub struct SupervisedBootstrapChannel {
    inner: platform::BootstrapChannel,
}

#[derive(Clone)]
pub struct SupervisedBootstrapCancellation {
    inner: platform::BootstrapCancellation,
}

impl SupervisedBootstrapCancellation {
    pub fn cancel(&self) -> Result<(), SupervisedProcessError> {
        self.inner
            .cancel()
            .map_err(SupervisedProcessError::Bootstrap)
    }
}

impl SupervisedBootstrapChannel {
    pub fn cancellation(&self) -> SupervisedBootstrapCancellation {
        SupervisedBootstrapCancellation {
            inner: self.inner.cancellation(),
        }
    }

    pub fn write_some(&mut self, bytes: &[u8]) -> Result<usize, SupervisedProcessError> {
        self.inner
            .write_some(bytes)
            .map_err(SupervisedProcessError::Bootstrap)
    }

    pub fn write_all(&mut self, bytes: &[u8]) -> Result<(), SupervisedProcessError> {
        self.inner
            .write_all(bytes)
            .map_err(SupervisedProcessError::Bootstrap)
    }

    pub fn try_read(&mut self, bytes: &mut [u8]) -> Result<usize, SupervisedProcessError> {
        self.inner
            .try_read(bytes)
            .map_err(SupervisedProcessError::Bootstrap)
    }
}

pub struct SupervisedProcessOwner {
    process: platform::PlatformProcess,
    process_id: u32,
    bootstrap: Option<SupervisedBootstrapChannel>,
    logs: ProcessLogRing,
    state: SupervisedProcessState,
    exit: Option<SupervisedProcessExit>,
    natural_exit_deadline: Option<Instant>,
    terminate_deadline: Option<Instant>,
    force_kill_deadline: Option<Instant>,
    drain_deadline: Option<Instant>,
    observation_deadline: Instant,
    containment_observed_empty: bool,
    readers_observed_stopped: bool,
    output_observed_drained: bool,
    output_cancel_requested: bool,
    output_loss_observed: bool,
    termination_requested: bool,
    force_killed: bool,
    cleanup_receipt: Option<SupervisedProcessCleanupReceipt>,
    _launch_record: Option<RestrictiveCapabilityHandleLease>,
}

impl SupervisedProcessOwner {
    pub fn spawn(
        command: &SupervisedProcessCommand,
        log_limits: ProcessLogLimits,
    ) -> Result<Self, SupervisedProcessError> {
        Self::spawn_with_wake(command, log_limits, SupervisedProcessWake::default())
    }

    pub fn spawn_with_wake(
        command: &SupervisedProcessCommand,
        log_limits: ProcessLogLimits,
        wake: SupervisedProcessWake,
    ) -> Result<Self, SupervisedProcessError> {
        validate_command(command)?;
        let spawned = platform::spawn(command, wake).map_err(SupervisedProcessError::Spawn)?;
        Ok(Self::from_spawned(spawned, log_limits, None))
    }

    pub fn spawn_exact(
        authority: ExactExecutableLaunchAuthority,
        command: &SupervisedProductProcessCommand,
        log_limits: ProcessLogLimits,
    ) -> Result<Self, SupervisedProcessError> {
        Self::spawn_exact_with_wake(
            authority,
            command,
            log_limits,
            SupervisedProcessWake::default(),
        )
    }

    pub fn spawn_exact_with_wake(
        authority: ExactExecutableLaunchAuthority,
        command: &SupervisedProductProcessCommand,
        log_limits: ProcessLogLimits,
        wake: SupervisedProcessWake,
    ) -> Result<Self, SupervisedProcessError> {
        validate_product_command(command)?;
        let ExactExecutableLaunchAuthority {
            inner,
            launch_record,
        } = authority;
        let spawned =
            platform::spawn_exact(inner, command, wake).map_err(SupervisedProcessError::Spawn)?;
        Ok(Self::from_spawned(spawned, log_limits, launch_record))
    }

    fn from_spawned(
        spawned: platform::SpawnedProcess,
        log_limits: ProcessLogLimits,
        launch_record: Option<RestrictiveCapabilityHandleLease>,
    ) -> Self {
        let platform::SpawnedProcess { process, bootstrap } = spawned;
        Self {
            process_id: process.process_id(),
            process,
            bootstrap: bootstrap.map(|inner| SupervisedBootstrapChannel { inner }),
            logs: ProcessLogRing::new(log_limits),
            state: SupervisedProcessState::Running,
            exit: None,
            natural_exit_deadline: None,
            terminate_deadline: None,
            force_kill_deadline: None,
            drain_deadline: None,
            observation_deadline: Instant::now() + Duration::from_millis(250),
            containment_observed_empty: false,
            readers_observed_stopped: false,
            output_observed_drained: false,
            output_cancel_requested: false,
            output_loss_observed: false,
            termination_requested: false,
            force_killed: false,
            cleanup_receipt: None,
            _launch_record: launch_record,
        }
    }

    /// Returns the identity captured at spawn for observation, never as control authority.
    pub const fn process_id(&self) -> u32 {
        self.process_id
    }

    pub const fn state(&self) -> SupervisedProcessState {
        self.state
    }

    pub fn logs(&self) -> &ProcessLogRing {
        &self.logs
    }

    pub fn bootstrap_mut(&mut self) -> Option<&mut SupervisedBootstrapChannel> {
        self.bootstrap.as_mut()
    }

    pub fn take_bootstrap(&mut self) -> Option<SupervisedBootstrapChannel> {
        self.bootstrap.take()
    }

    pub fn request_terminate(&mut self, now: Instant) -> Result<(), SupervisedProcessError> {
        if !matches!(self.state, SupervisedProcessState::Running) {
            return Ok(());
        }
        self.bootstrap.take();
        self.termination_requested = true;
        self.process
            .request_terminate()
            .map_err(SupervisedProcessError::Terminate)?;
        self.state = SupervisedProcessState::Terminating;
        self.terminate_deadline = Some(now + DEFAULT_TERMINATE_GRACE);
        Ok(())
    }

    pub fn retry_cleanup(&mut self, now: Instant) -> Result<(), SupervisedProcessError> {
        match self.state {
            SupervisedProcessState::Running => self.request_terminate(now),
            SupervisedProcessState::Terminating => {
                self.termination_requested = true;
                self.process
                    .request_terminate()
                    .map_err(SupervisedProcessError::Terminate)?;
                self.terminate_deadline = Some(now + DEFAULT_TERMINATE_GRACE);
                Ok(())
            }
            SupervisedProcessState::ForceKilling => {
                self.termination_requested = true;
                self.process
                    .force_kill()
                    .map_err(SupervisedProcessError::ForceKill)?;
                self.force_kill_deadline = Some(now + DEFAULT_FORCE_KILL_GRACE);
                Ok(())
            }
            SupervisedProcessState::Draining => {
                self.process
                    .cancel_output()
                    .map_err(SupervisedProcessError::CancelOutput)?;
                self.output_cancel_requested = true;
                self.termination_requested = true;
                self.process
                    .force_kill()
                    .map_err(SupervisedProcessError::ForceKill)?;
                self.force_killed = true;
                self.drain_deadline = Some(now + DEFAULT_OUTPUT_DRAIN_GRACE);
                self.force_kill_deadline = Some(now + DEFAULT_FORCE_KILL_GRACE);
                Ok(())
            }
            SupervisedProcessState::Complete => Ok(()),
        }
    }

    pub fn advance(
        &mut self,
        now: Instant,
    ) -> Result<SupervisedProcessObservation, SupervisedProcessError> {
        self.advance_inner(now, None)
    }

    pub fn advance_with_output(
        &mut self,
        now: Instant,
    ) -> Result<(SupervisedProcessObservation, Vec<SupervisedProcessOutput>), SupervisedProcessError>
    {
        let mut outputs = Vec::new();
        let observation = self.advance_inner(now, Some(&mut outputs))?;
        Ok((observation, outputs))
    }

    fn advance_inner(
        &mut self,
        now: Instant,
        outputs: Option<&mut Vec<SupervisedProcessOutput>>,
    ) -> Result<SupervisedProcessObservation, SupervisedProcessError> {
        if self.state == SupervisedProcessState::Complete {
            return Ok(SupervisedProcessObservation {
                state: self.state,
                exit: self.exit,
                output_loss_observed: self.output_loss_observed,
            });
        }

        self.drain_output(outputs)?;
        self.observation_deadline = now + Duration::from_millis(250);
        if self.exit.is_none() {
            self.exit = self
                .process
                .try_wait()
                .map_err(SupervisedProcessError::Wait)?
                .map(Into::into);
        }
        if self.exit.is_some() {
            self.bootstrap.take();
        }

        if self.exit.is_some() && !matches!(self.state, SupervisedProcessState::Draining) {
            if self.state == SupervisedProcessState::Running {
                self.containment_observed_empty = self
                    .process
                    .containment_empty()
                    .map_err(SupervisedProcessError::Containment)?;
                if !self.containment_observed_empty {
                    self.natural_exit_deadline = Some(now + DEFAULT_NATURAL_EXIT_GRACE);
                }
            }
            self.state = SupervisedProcessState::Draining;
            self.drain_deadline = Some(now + DEFAULT_OUTPUT_DRAIN_GRACE);
        }

        if self.state == SupervisedProcessState::Terminating
            && self
                .terminate_deadline
                .is_some_and(|deadline| now >= deadline)
        {
            self.termination_requested = true;
            self.process
                .force_kill()
                .map_err(SupervisedProcessError::ForceKill)?;
            self.force_killed = true;
            self.state = SupervisedProcessState::ForceKilling;
            self.force_kill_deadline = Some(now + DEFAULT_FORCE_KILL_GRACE);
        }

        if self.state == SupervisedProcessState::ForceKilling
            && self.exit.is_none()
            && self
                .force_kill_deadline
                .is_some_and(|deadline| now >= deadline)
        {
            return Err(SupervisedProcessError::CleanupIncomplete(
                "the direct child did not exit before the force-kill deadline",
            ));
        }

        if self.state == SupervisedProcessState::Draining {
            let readers_stopped = self.process.readers_stopped();
            self.readers_observed_stopped = readers_stopped;
            let output_drained = self.process.output_drained();
            self.output_observed_drained = output_drained;
            if !readers_stopped && self.drain_deadline.is_some_and(|deadline| now >= deadline) {
                if !self.output_cancel_requested {
                    self.process
                        .cancel_output()
                        .map_err(SupervisedProcessError::CancelOutput)?;
                    self.output_cancel_requested = true;
                    self.drain_deadline = Some(now + DEFAULT_OUTPUT_DRAIN_GRACE);
                } else {
                    return Err(SupervisedProcessError::CleanupIncomplete(
                        "process output readers did not terminate before the drain deadline",
                    ));
                }
            }

            let containment_empty = self
                .process
                .containment_empty()
                .map_err(SupervisedProcessError::Containment)?;
            self.containment_observed_empty = containment_empty;
            if containment_empty || self.terminate_deadline.is_some() || self.force_killed {
                self.natural_exit_deadline = None;
            }
            if self
                .natural_exit_deadline
                .is_some_and(|deadline| now >= deadline)
            {
                self.termination_requested = true;
                self.process
                    .request_terminate()
                    .map_err(SupervisedProcessError::Terminate)?;
                self.terminate_deadline = Some(now + DEFAULT_TERMINATE_GRACE);
                self.natural_exit_deadline = None;
            }
            if !containment_empty
                && !self.force_killed
                && self
                    .terminate_deadline
                    .is_some_and(|deadline| now >= deadline)
            {
                self.termination_requested = true;
                self.process
                    .force_kill()
                    .map_err(SupervisedProcessError::ForceKill)?;
                self.force_killed = true;
                self.force_kill_deadline = Some(now + DEFAULT_FORCE_KILL_GRACE);
            } else if !containment_empty
                && self.force_killed
                && self
                    .force_kill_deadline
                    .is_some_and(|deadline| now >= deadline)
            {
                return Err(SupervisedProcessError::CleanupIncomplete(
                    "the supervised process containment did not empty before the force-kill deadline",
                ));
            }

            if readers_stopped && output_drained && containment_empty {
                let exit = self.exit.ok_or(SupervisedProcessError::CleanupIncomplete(
                    "cleanup cannot complete before the direct child is reaped",
                ))?;
                let receipt = SupervisedProcessCleanupReceipt {
                    exit,
                    containment_empty,
                    stdout_reader_stopped: true,
                    stderr_reader_stopped: true,
                    output_drained,
                    termination_requested: self.termination_requested,
                    force_killed: self.force_killed,
                };
                self.cleanup_receipt = Some(receipt);
                self.state = SupervisedProcessState::Complete;
            }
        }

        Ok(SupervisedProcessObservation {
            state: self.state,
            exit: self.exit,
            output_loss_observed: self.output_loss_observed,
        })
    }

    pub const fn cleanup_receipt(&self) -> Option<SupervisedProcessCleanupReceipt> {
        self.cleanup_receipt
    }

    pub const fn output_loss_observed(&self) -> bool {
        self.output_loss_observed
    }

    pub fn next_deadline(&self) -> Option<Instant> {
        match self.state {
            SupervisedProcessState::Running => Some(self.observation_deadline),
            SupervisedProcessState::Terminating => self.terminate_deadline,
            SupervisedProcessState::ForceKilling => self.force_kill_deadline,
            SupervisedProcessState::Draining => [
                self.natural_exit_deadline,
                (!self.containment_observed_empty && !self.force_killed)
                    .then_some(self.terminate_deadline)
                    .flatten(),
                (!self.containment_observed_empty && self.force_killed)
                    .then_some(self.force_kill_deadline)
                    .flatten(),
                (!self.readers_observed_stopped)
                    .then_some(self.drain_deadline)
                    .flatten(),
                (!self.output_observed_drained).then_some(self.observation_deadline),
            ]
            .into_iter()
            .flatten()
            .min(),
            SupervisedProcessState::Complete => None,
        }
    }

    fn drain_output(
        &mut self,
        mut outputs: Option<&mut Vec<SupervisedProcessOutput>>,
    ) -> Result<(), SupervisedProcessError> {
        while let Some(output) = self
            .process
            .try_output()
            .map_err(SupervisedProcessError::ReadOutput)?
        {
            self.output_loss_observed |= output.loss;
            self.logs
                .append(output.stream, &output.bytes)
                .map_err(SupervisedProcessError::LogSequence)?;
            if let Some(outputs) = outputs.as_mut() {
                outputs.push(SupervisedProcessOutput {
                    stream: output.stream,
                    bytes: output.bytes,
                    loss: output.loss,
                });
            }
        }
        Ok(())
    }
}

impl Drop for SupervisedProcessOwner {
    fn drop(&mut self) {
        self.bootstrap.take();
        self.logs.purge();
        if self.state != SupervisedProcessState::Complete {
            self.process.fail_safe_terminate();
        }
    }
}

#[derive(Debug, Error)]
pub enum SupervisedProcessError {
    #[error("supervised process executable must be an absolute path")]
    ExecutableNotAbsolute,
    #[error("supervised process working directory must be an absolute path")]
    WorkingDirectoryNotAbsolute,
    #[error("supervised process arguments or environment exceed their bounded limits")]
    CommandTooLarge,
    #[error("supervised process spawn failed: {0}")]
    Spawn(#[source] std::io::Error),
    #[error("supervised process wait failed: {0}")]
    Wait(#[source] std::io::Error),
    #[error("supervised process termination failed: {0}")]
    Terminate(#[source] std::io::Error),
    #[error("supervised process force kill failed: {0}")]
    ForceKill(#[source] std::io::Error),
    #[error("supervised process output read failed: {0}")]
    ReadOutput(#[source] std::io::Error),
    #[error("supervised process output cancellation failed: {0}")]
    CancelOutput(#[source] std::io::Error),
    #[error("supervised process bootstrap channel failed: {0}")]
    Bootstrap(#[source] std::io::Error),
    #[error("supervised process containment inspection failed: {0}")]
    Containment(#[source] std::io::Error),
    #[error("supervised process log sequence failed: {0}")]
    LogSequence(#[source] ProcessLogSequenceExhausted),
    #[error("supervised process cleanup is incomplete: {0}")]
    CleanupIncomplete(&'static str),
}

fn validate_command(command: &SupervisedProcessCommand) -> Result<(), SupervisedProcessError> {
    if !command.executable().is_absolute() {
        return Err(SupervisedProcessError::ExecutableNotAbsolute);
    }
    if !command.working_directory().is_absolute() {
        return Err(SupervisedProcessError::WorkingDirectoryNotAbsolute);
    }

    let argument_bytes = bounded_units(command.arguments_ref().iter())?;
    let environment_bytes = bounded_units(
        command
            .environment_ref()
            .iter()
            .flat_map(|(name, value)| [name, value]),
    )?;
    if command.arguments_ref().len() > 256
        || command.environment_ref().len() > 128
        || argument_bytes > 64 * 1024
        || environment_bytes > 64 * 1024
    {
        return Err(SupervisedProcessError::CommandTooLarge);
    }
    Ok(())
}

fn validate_product_command(
    command: &SupervisedProductProcessCommand,
) -> Result<(), SupervisedProcessError> {
    if !command.working_directory().is_absolute() {
        return Err(SupervisedProcessError::WorkingDirectoryNotAbsolute);
    }
    let argument_bytes = bounded_units(command.arguments_ref().iter())?;
    if command.arguments_ref().len() > 256 || argument_bytes > 64 * 1024 {
        return Err(SupervisedProcessError::CommandTooLarge);
    }
    Ok(())
}

fn bounded_units<'a>(
    values: impl IntoIterator<Item = &'a OsString>,
) -> Result<usize, SupervisedProcessError> {
    values.into_iter().try_fold(0_usize, |total, value| {
        total
            .checked_add(os_string_units(value.as_os_str()))
            .ok_or(SupervisedProcessError::CommandTooLarge)
    })
}

#[cfg(not(windows))]
fn os_string_units(value: &OsStr) -> usize {
    value.as_encoded_bytes().len()
}

#[cfg(windows)]
fn os_string_units(value: &OsStr) -> usize {
    use std::os::windows::ffi::OsStrExt as _;

    value.encode_wide().count()
}

pub fn default_log_limits() -> ProcessLogLimits {
    ProcessLogLimits::new(
        NonZeroUsize::new(512).expect("the process log record limit is non-zero"),
        NonZeroUsize::new(256 * 1024).expect("the process log byte limit is non-zero"),
    )
}

#[cfg(test)]
mod tests;
