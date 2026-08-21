use std::{
    ffi::{OsStr, OsString},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use thiserror::Error;

use super::{
    ProcessOutputStream,
    log::{ProcessLogLimits, ProcessLogRing, ProcessLogSequenceExhausted},
};

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as platform;
#[cfg(windows)]
use windows as platform;

const DEFAULT_TERMINATE_GRACE: Duration = Duration::from_secs(2);
const DEFAULT_FORCE_KILL_GRACE: Duration = Duration::from_secs(2);
const DEFAULT_OUTPUT_DRAIN_GRACE: Duration = Duration::from_millis(500);

pub(super) use super::ProcessOutputStream as SupervisedProcessStream;

pub(super) struct PlatformOutput {
    pub(super) stream: ProcessOutputStream,
    pub(super) bytes: Vec<u8>,
    pub(super) loss: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SupervisedProcessOutput {
    stream: ProcessOutputStream,
    bytes: Vec<u8>,
    loss: bool,
}

impl SupervisedProcessOutput {
    pub(super) const fn stream(&self) -> ProcessOutputStream {
        self.stream
    }

    pub(super) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(super) const fn loss(&self) -> bool {
        self.loss
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SupervisedProcessCommand {
    executable: PathBuf,
    arguments: Vec<OsString>,
    working_directory: PathBuf,
    environment: Vec<(OsString, OsString)>,
    bootstrap: bool,
}

impl SupervisedProcessCommand {
    pub(super) fn new(
        executable: impl Into<PathBuf>,
        working_directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            executable: executable.into(),
            arguments: Vec::new(),
            working_directory: working_directory.into(),
            environment: Vec::new(),
            bootstrap: false,
        }
    }

    pub(super) fn argument(mut self, argument: impl Into<OsString>) -> Self {
        self.arguments.push(argument.into());
        self
    }

    pub(super) fn arguments(
        mut self,
        arguments: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Self {
        self.arguments.extend(arguments.into_iter().map(Into::into));
        self
    }

    pub(super) fn environment(
        mut self,
        name: impl Into<OsString>,
        value: impl Into<OsString>,
    ) -> Self {
        self.environment.push((name.into(), value.into()));
        self
    }

    pub(super) fn with_bootstrap(mut self) -> Self {
        self.bootstrap = true;
        self
    }

    pub(super) fn executable(&self) -> &Path {
        &self.executable
    }

    pub(super) fn arguments_ref(&self) -> &[OsString] {
        &self.arguments
    }

    pub(super) fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    pub(super) fn environment_ref(&self) -> &[(OsString, OsString)] {
        &self.environment
    }

    pub(super) const fn bootstrap_requested(&self) -> bool {
        self.bootstrap
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SupervisedProcessState {
    Running,
    Terminating,
    ForceKilling,
    Draining,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SupervisedProcessExit {
    code: Option<i32>,
    success: bool,
}

impl SupervisedProcessExit {
    pub(super) const fn code(self) -> Option<i32> {
        self.code
    }

    pub(super) const fn success(self) -> bool {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SupervisedProcessCleanupReceipt {
    exit: SupervisedProcessExit,
    containment_empty: bool,
    stdout_reader_stopped: bool,
    stderr_reader_stopped: bool,
    force_killed: bool,
}

impl SupervisedProcessCleanupReceipt {
    pub(super) const fn exit(self) -> SupervisedProcessExit {
        self.exit
    }

    pub(super) const fn containment_empty(self) -> bool {
        self.containment_empty
    }

    pub(super) const fn readers_stopped(self) -> bool {
        self.stdout_reader_stopped && self.stderr_reader_stopped
    }

    pub(super) const fn force_killed(self) -> bool {
        self.force_killed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SupervisedProcessObservation {
    state: SupervisedProcessState,
    exit: Option<SupervisedProcessExit>,
    output_changed: bool,
}

impl SupervisedProcessObservation {
    pub(super) const fn state(self) -> SupervisedProcessState {
        self.state
    }

    pub(super) const fn exit(self) -> Option<SupervisedProcessExit> {
        self.exit
    }

    pub(super) const fn output_changed(self) -> bool {
        self.output_changed
    }
}

pub(super) struct SupervisedBootstrapChannel {
    inner: platform::BootstrapChannel,
}

impl SupervisedBootstrapChannel {
    pub(super) fn write_all(&mut self, bytes: &[u8]) -> Result<(), SupervisedProcessError> {
        self.inner
            .write_all(bytes)
            .map_err(SupervisedProcessError::Bootstrap)
    }

    pub(super) fn try_read(&mut self, bytes: &mut [u8]) -> Result<usize, SupervisedProcessError> {
        self.inner
            .try_read(bytes)
            .map_err(SupervisedProcessError::Bootstrap)
    }
}

pub(super) struct SupervisedProcessOwner {
    process: platform::PlatformProcess,
    bootstrap: Option<SupervisedBootstrapChannel>,
    logs: ProcessLogRing,
    state: SupervisedProcessState,
    exit: Option<SupervisedProcessExit>,
    terminate_deadline: Option<Instant>,
    force_kill_deadline: Option<Instant>,
    drain_deadline: Option<Instant>,
    output_cancel_requested: bool,
    force_killed: bool,
    cleanup_receipt: Option<SupervisedProcessCleanupReceipt>,
}

impl SupervisedProcessOwner {
    pub(super) fn spawn(
        command: &SupervisedProcessCommand,
        log_limits: ProcessLogLimits,
    ) -> Result<Self, SupervisedProcessError> {
        validate_command(command)?;
        let platform::SpawnedProcess { process, bootstrap } =
            platform::spawn(command).map_err(SupervisedProcessError::Spawn)?;
        Ok(Self {
            process,
            bootstrap: bootstrap.map(|inner| SupervisedBootstrapChannel { inner }),
            logs: ProcessLogRing::new(log_limits),
            state: SupervisedProcessState::Running,
            exit: None,
            terminate_deadline: None,
            force_kill_deadline: None,
            drain_deadline: None,
            output_cancel_requested: false,
            force_killed: false,
            cleanup_receipt: None,
        })
    }

    pub(super) const fn state(&self) -> SupervisedProcessState {
        self.state
    }

    pub(super) fn logs(&self) -> &ProcessLogRing {
        &self.logs
    }

    pub(super) fn bootstrap_mut(&mut self) -> Option<&mut SupervisedBootstrapChannel> {
        self.bootstrap.as_mut()
    }

    pub(super) fn request_terminate(&mut self, now: Instant) -> Result<(), SupervisedProcessError> {
        if !matches!(self.state, SupervisedProcessState::Running) {
            return Ok(());
        }
        self.bootstrap.take();
        self.process
            .request_terminate()
            .map_err(SupervisedProcessError::Terminate)?;
        self.state = SupervisedProcessState::Terminating;
        self.terminate_deadline = Some(now + DEFAULT_TERMINATE_GRACE);
        Ok(())
    }

    pub(super) fn advance(
        &mut self,
        now: Instant,
    ) -> Result<SupervisedProcessObservation, SupervisedProcessError> {
        self.advance_with_output(now)
            .map(|(observation, _)| observation)
    }

    pub(super) fn advance_with_output(
        &mut self,
        now: Instant,
    ) -> Result<(SupervisedProcessObservation, Vec<SupervisedProcessOutput>), SupervisedProcessError>
    {
        let mut outputs = Vec::new();
        let observation = self.advance_inner(now, &mut outputs)?;
        Ok((observation, outputs))
    }

    fn advance_inner(
        &mut self,
        now: Instant,
        outputs: &mut Vec<SupervisedProcessOutput>,
    ) -> Result<SupervisedProcessObservation, SupervisedProcessError> {
        if self.state == SupervisedProcessState::Complete {
            return Ok(SupervisedProcessObservation {
                state: self.state,
                exit: self.exit,
                output_changed: false,
            });
        }

        let output_changed = self.drain_output(outputs)?;
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
                self.process
                    .request_terminate()
                    .map_err(SupervisedProcessError::Terminate)?;
                self.terminate_deadline = Some(now + DEFAULT_TERMINATE_GRACE);
            }
            self.state = SupervisedProcessState::Draining;
            self.drain_deadline = Some(now + DEFAULT_OUTPUT_DRAIN_GRACE);
        }

        if self.state == SupervisedProcessState::Terminating
            && self
                .terminate_deadline
                .is_some_and(|deadline| now >= deadline)
        {
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
            if !containment_empty
                && !self.force_killed
                && self
                    .terminate_deadline
                    .is_some_and(|deadline| now >= deadline)
            {
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

            if readers_stopped && containment_empty {
                let exit = self.exit.ok_or(SupervisedProcessError::CleanupIncomplete(
                    "cleanup cannot complete before the direct child is reaped",
                ))?;
                let receipt = SupervisedProcessCleanupReceipt {
                    exit,
                    containment_empty,
                    stdout_reader_stopped: true,
                    stderr_reader_stopped: true,
                    force_killed: self.force_killed,
                };
                self.cleanup_receipt = Some(receipt);
                self.state = SupervisedProcessState::Complete;
            }
        }

        Ok(SupervisedProcessObservation {
            state: self.state,
            exit: self.exit,
            output_changed,
        })
    }

    pub(super) const fn cleanup_receipt(&self) -> Option<SupervisedProcessCleanupReceipt> {
        self.cleanup_receipt
    }

    pub(super) fn purge_logs(&mut self) {
        self.logs.purge();
    }

    fn drain_output(
        &mut self,
        outputs: &mut Vec<SupervisedProcessOutput>,
    ) -> Result<bool, SupervisedProcessError> {
        let mut output_changed = false;
        while let Some(output) = self
            .process
            .try_output()
            .map_err(SupervisedProcessError::ReadOutput)?
        {
            output_changed = true;
            self.logs
                .append(output.stream, &output.bytes)
                .map_err(SupervisedProcessError::LogSequence)?;
            outputs.push(SupervisedProcessOutput {
                stream: output.stream,
                bytes: output.bytes,
                loss: output.loss,
            });
        }
        Ok(output_changed)
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
pub(super) enum SupervisedProcessError {
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

fn bounded_units<'a>(
    values: impl IntoIterator<Item = &'a OsString>,
) -> Result<usize, SupervisedProcessError> {
    values.into_iter().try_fold(0_usize, |total, value| {
        total
            .checked_add(os_string_units(value.as_os_str()))
            .ok_or(SupervisedProcessError::CommandTooLarge)
    })
}

#[cfg(unix)]
fn os_string_units(value: &OsStr) -> usize {
    use std::os::unix::ffi::OsStrExt as _;

    value.as_bytes().len()
}

#[cfg(windows)]
fn os_string_units(value: &OsStr) -> usize {
    use std::os::windows::ffi::OsStrExt as _;

    value.encode_wide().count()
}

pub(super) fn default_log_limits() -> ProcessLogLimits {
    ProcessLogLimits::new(
        NonZeroUsize::new(512).expect("the process log record limit is non-zero"),
        NonZeroUsize::new(256 * 1024).expect("the process log byte limit is non-zero"),
    )
}
