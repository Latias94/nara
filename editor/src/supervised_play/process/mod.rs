use std::{
    ffi::{OsStr, OsString},
    num::NonZeroUsize,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use nara::fs::RestrictiveCapabilityHandleLease;
use thiserror::Error;

use super::{
    ProcessOutputStream,
    log::{ProcessLogLimits, ProcessLogRing, ProcessLogSequenceExhausted},
    wake::SupervisedPlayWake,
};

mod output;
mod spawn;
#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

pub(crate) use spawn::{
    SupervisedProcessSpawnFailure, SupervisedProcessSpawnOwner, SupervisedProcessSpawnRetirement,
    SupervisedProcessSpawnTerminal,
};

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

/// Selects the only ambient environment facts a supervised product may observe.
///
/// Toolchain commands use `SupervisedProcessCommand` and retain their existing inherited
/// environment contract. Exact-generation product launches start from an empty environment and
/// may opt into only this reviewed desktop-session projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SupervisedProductEnvironmentPolicy {
    #[cfg(test)]
    Sealed,
    AllowlistedDesktopSession,
}

impl SupervisedProductEnvironmentPolicy {
    pub(super) const fn allowlisted_names(self) -> &'static [&'static str] {
        match self {
            #[cfg(test)]
            Self::Sealed => &[],
            Self::AllowlistedDesktopSession => DESKTOP_SESSION_ENVIRONMENT_ALLOWLIST,
        }
    }

    #[cfg(test)]
    pub(super) fn allows_name(self, name: &str) -> bool {
        self.allowlisted_names().contains(&name)
    }
}

#[cfg(windows)]
const DESKTOP_SESSION_ENVIRONMENT_ALLOWLIST: &[&str] = &["SystemRoot", "WINDIR", "TEMP", "TMP"];

#[cfg(unix)]
const DESKTOP_SESSION_ENVIRONMENT_ALLOWLIST: &[&str] = &[
    "DISPLAY",
    "WAYLAND_DISPLAY",
    "XDG_RUNTIME_DIR",
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
];

#[cfg(not(any(unix, windows)))]
const DESKTOP_SESSION_ENVIRONMENT_ALLOWLIST: &[&str] = &[];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct SupervisedProductProcessCommand {
    arguments: Vec<OsString>,
    working_directory: PathBuf,
    environment_policy: SupervisedProductEnvironmentPolicy,
}

impl SupervisedProductProcessCommand {
    pub(super) fn new(
        environment_policy: SupervisedProductEnvironmentPolicy,
        working_directory: impl Into<PathBuf>,
    ) -> Self {
        Self {
            arguments: Vec::new(),
            working_directory: working_directory.into(),
            environment_policy,
        }
    }

    #[cfg(test)]
    pub(super) fn arguments(
        mut self,
        arguments: impl IntoIterator<Item = impl Into<OsString>>,
    ) -> Self {
        self.arguments.extend(arguments.into_iter().map(Into::into));
        self
    }

    pub(super) fn arguments_ref(&self) -> &[OsString] {
        &self.arguments
    }

    pub(super) fn working_directory(&self) -> &Path {
        &self.working_directory
    }

    pub(super) const fn environment_policy(&self) -> SupervisedProductEnvironmentPolicy {
        self.environment_policy
    }
}

/// Move-only authority to one already-verified immutable generation.
///
/// The platform adapter owns the exact executable and generation-directory handles. Neither this
/// authority nor the revalidation receipt exposes a raw executable path as a launch capability.
#[must_use = "exact generation authority must be consumed by one immediate spawn attempt"]
pub(super) struct ExactGenerationLaunchAuthority {
    inner: platform::ExactGenerationLaunchAuthority,
}

impl ExactGenerationLaunchAuthority {
    pub(super) fn from_verified_handles(
        executable: RestrictiveCapabilityHandleLease,
        generation_directory: RestrictiveCapabilityHandleLease,
        launch_manifest: RestrictiveCapabilityHandleLease,
    ) -> std::io::Result<Self> {
        platform::ExactGenerationLaunchAuthority::new(
            executable,
            generation_directory,
            launch_manifest,
        )
        .map(|inner| Self { inner })
    }
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

    #[cfg(test)]
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct SupervisedProcessObservation {
    state: SupervisedProcessState,
    exit: Option<SupervisedProcessExit>,
    output_loss_observed: bool,
}

impl SupervisedProcessObservation {
    pub(super) const fn exit(self) -> Option<SupervisedProcessExit> {
        self.exit
    }

    #[cfg(test)]
    pub(super) const fn output_loss_observed(self) -> bool {
        self.output_loss_observed
    }
}

pub(super) struct SupervisedBootstrapChannel {
    inner: platform::BootstrapChannel,
}

#[derive(Clone)]
pub(super) struct SupervisedBootstrapCancellation {
    inner: platform::BootstrapCancellation,
}

impl SupervisedBootstrapCancellation {
    pub(super) fn cancel(&self) -> Result<(), SupervisedProcessError> {
        self.inner
            .cancel()
            .map_err(SupervisedProcessError::Bootstrap)
    }
}

impl SupervisedBootstrapChannel {
    pub(super) fn cancellation(&self) -> SupervisedBootstrapCancellation {
        SupervisedBootstrapCancellation {
            inner: self.inner.cancellation(),
        }
    }

    pub(super) fn write_some(&mut self, bytes: &[u8]) -> Result<usize, SupervisedProcessError> {
        self.inner
            .write_some(bytes)
            .map_err(SupervisedProcessError::Bootstrap)
    }

    #[cfg(test)]
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
    observation_deadline: Instant,
    containment_observed_empty: bool,
    readers_observed_stopped: bool,
    output_cancel_requested: bool,
    output_loss_observed: bool,
    force_killed: bool,
    cleanup_receipt: Option<SupervisedProcessCleanupReceipt>,
}

impl SupervisedProcessOwner {
    #[cfg(test)]
    pub(super) fn spawn(
        command: &SupervisedProcessCommand,
        log_limits: ProcessLogLimits,
    ) -> Result<Self, SupervisedProcessError> {
        Self::spawn_with_wake(command, log_limits, SupervisedPlayWake::default())
    }

    pub(super) fn spawn_with_wake(
        command: &SupervisedProcessCommand,
        log_limits: ProcessLogLimits,
        wake: SupervisedPlayWake,
    ) -> Result<Self, SupervisedProcessError> {
        validate_command(command)?;
        let spawned = platform::spawn(command, wake).map_err(SupervisedProcessError::Spawn)?;
        Ok(Self::from_spawned(spawned, log_limits))
    }

    #[cfg(test)]
    pub(super) fn spawn_exact_generation(
        authority: ExactGenerationLaunchAuthority,
        command: &SupervisedProductProcessCommand,
        log_limits: ProcessLogLimits,
    ) -> Result<Self, SupervisedProcessError> {
        Self::spawn_exact_generation_with_wake(
            authority,
            command,
            log_limits,
            SupervisedPlayWake::default(),
        )
    }

    pub(super) fn spawn_exact_generation_with_wake(
        authority: ExactGenerationLaunchAuthority,
        command: &SupervisedProductProcessCommand,
        log_limits: ProcessLogLimits,
        wake: SupervisedPlayWake,
    ) -> Result<Self, SupervisedProcessError> {
        validate_product_command(command)?;
        let spawned = platform::spawn_exact_generation(authority.inner, command, wake)
            .map_err(SupervisedProcessError::Spawn)?;
        Ok(Self::from_spawned(spawned, log_limits))
    }

    fn from_spawned(spawned: platform::SpawnedProcess, log_limits: ProcessLogLimits) -> Self {
        let platform::SpawnedProcess { process, bootstrap } = spawned;
        Self {
            process,
            bootstrap: bootstrap.map(|inner| SupervisedBootstrapChannel { inner }),
            logs: ProcessLogRing::new(log_limits),
            state: SupervisedProcessState::Running,
            exit: None,
            terminate_deadline: None,
            force_kill_deadline: None,
            drain_deadline: None,
            observation_deadline: Instant::now() + Duration::from_millis(250),
            containment_observed_empty: false,
            readers_observed_stopped: false,
            output_cancel_requested: false,
            output_loss_observed: false,
            force_killed: false,
            cleanup_receipt: None,
        }
    }

    #[cfg(test)]
    pub(super) const fn state(&self) -> SupervisedProcessState {
        self.state
    }

    pub(super) fn logs(&self) -> &ProcessLogRing {
        &self.logs
    }

    #[cfg(test)]
    pub(super) fn bootstrap_mut(&mut self) -> Option<&mut SupervisedBootstrapChannel> {
        self.bootstrap.as_mut()
    }

    pub(super) fn take_bootstrap(&mut self) -> Option<SupervisedBootstrapChannel> {
        self.bootstrap.take()
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

    pub(super) fn retry_cleanup(&mut self, now: Instant) -> Result<(), SupervisedProcessError> {
        match self.state {
            SupervisedProcessState::Running => self.request_terminate(now),
            SupervisedProcessState::Terminating => {
                self.process
                    .request_terminate()
                    .map_err(SupervisedProcessError::Terminate)?;
                self.terminate_deadline = Some(now + DEFAULT_TERMINATE_GRACE);
                Ok(())
            }
            SupervisedProcessState::ForceKilling => {
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

    pub(super) fn advance(
        &mut self,
        now: Instant,
    ) -> Result<SupervisedProcessObservation, SupervisedProcessError> {
        self.advance_inner(now, None)
    }

    pub(super) fn advance_with_output(
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
            self.readers_observed_stopped = readers_stopped;
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
            output_loss_observed: self.output_loss_observed,
        })
    }

    pub(super) const fn cleanup_receipt(&self) -> Option<SupervisedProcessCleanupReceipt> {
        self.cleanup_receipt
    }

    pub(super) const fn output_loss_observed(&self) -> bool {
        self.output_loss_observed
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        match self.state {
            SupervisedProcessState::Running => Some(self.observation_deadline),
            SupervisedProcessState::Terminating => self.terminate_deadline,
            SupervisedProcessState::ForceKilling => self.force_kill_deadline,
            SupervisedProcessState::Draining => [
                (!self.containment_observed_empty && !self.force_killed)
                    .then_some(self.terminate_deadline)
                    .flatten(),
                (!self.containment_observed_empty && self.force_killed)
                    .then_some(self.force_kill_deadline)
                    .flatten(),
                (!self.readers_observed_stopped)
                    .then_some(self.drain_deadline)
                    .flatten(),
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
pub(super) enum SupervisedProcessError {
    #[error("the exact executable generation could not be revalidated: {0}")]
    Generation(#[source] crate::supervised_play::generation::GenerationStoreError),
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

#[cfg(test)]
mod policy_tests {
    use super::SupervisedProductEnvironmentPolicy;

    #[test]
    fn product_environment_allowlist_excludes_authority_and_injection_variables() {
        let policy = SupervisedProductEnvironmentPolicy::AllowlistedDesktopSession;
        for rejected in [
            "PATH",
            "HOME",
            "USERPROFILE",
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "AWS_ACCESS_KEY_ID",
            "LD_PRELOAD",
            "LD_LIBRARY_PATH",
            "DYLD_INSERT_LIBRARIES",
        ] {
            assert!(!policy.allows_name(rejected), "{rejected} must stay sealed");
        }
        assert!(
            SupervisedProductEnvironmentPolicy::Sealed
                .allowlisted_names()
                .is_empty()
        );
    }
}

#[cfg(all(test, any(windows, all(target_os = "linux", target_arch = "x86_64"))))]
mod loss_observation_tests {
    use std::time::Instant;

    use super::{SupervisedProcessOwner, default_log_limits};

    #[test]
    fn quiet_running_process_has_a_stable_exit_observation_deadline() {
        let mut owner = SupervisedProcessOwner::spawn(
            &crate::supervised_play::tests::helper_command("wait"),
            default_log_limits(),
        )
        .unwrap();
        let now = Instant::now();
        owner.advance(now).unwrap();
        let deadline = owner
            .next_deadline()
            .expect("a silent child still needs exit observation");
        assert!(deadline > now);
        assert_eq!(
            owner.next_deadline(),
            Some(deadline),
            "querying a deadline must not postpone it"
        );
    }

    #[test]
    fn advanced_cleanup_phase_does_not_reuse_the_expired_terminate_deadline() {
        let mut owner = SupervisedProcessOwner::spawn(
            &crate::supervised_play::tests::helper_command("wait"),
            default_log_limits(),
        )
        .unwrap();
        let now = Instant::now();
        owner.terminate_deadline = Some(now);
        owner.force_kill_deadline = Some(now + std::time::Duration::from_secs(1));
        owner.drain_deadline = Some(now + std::time::Duration::from_secs(2));
        owner.state = super::SupervisedProcessState::ForceKilling;
        owner.force_killed = true;
        assert_eq!(owner.next_deadline(), owner.force_kill_deadline);
        owner.state = super::SupervisedProcessState::Draining;
        assert_eq!(owner.next_deadline(), owner.force_kill_deadline);
        owner.state = super::SupervisedProcessState::Complete;
        let completed_deadline = owner.next_deadline();
        owner.state = super::SupervisedProcessState::Running;
        assert_eq!(completed_deadline, None);
    }

    #[test]
    fn advance_retains_output_loss_without_returning_a_raw_batch() {
        let mut owner = SupervisedProcessOwner::spawn(
            &crate::supervised_play::tests::helper_command("wait"),
            default_log_limits(),
        )
        .expect("the helper process spawns");
        owner.process.test_enqueue_loss();

        let observation = owner.advance(Instant::now()).expect("the process advances");

        assert!(observation.output_loss_observed());
        assert!(owner.output_loss_observed());
    }
}
