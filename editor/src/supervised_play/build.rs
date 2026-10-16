mod cargo_driver;
mod source;

use std::{
    fs, io,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use nara::{
    fs::ContentDigest,
    project_host::MAX_SUPERVISED_DESKTOP_EXECUTABLE_BYTES,
    project_workflow::__private::{
        CargoDesktopBuildDecoder, CargoDesktopBuildError, CargoDesktopBuildFacts,
        CargoDesktopBuildInvocation, CargoDesktopInvocationError, CargoDiagnostics,
        CargoMetadataFacts, CargoMetadataFactsError,
    },
};
use thiserror::Error;

use super::{
    ProcessOutputStream,
    log::{ProcessLogRing, ProcessLogSequenceExhausted},
    process::{
        SupervisedProcessCleanupReceipt, SupervisedProcessCommand, SupervisedProcessError,
        SupervisedProcessOwner, SupervisedProcessSpawnFailure, SupervisedProcessSpawnOwner,
        SupervisedProcessSpawnRetirement, SupervisedProcessSpawnTerminal, default_log_limits,
    },
    wake::SupervisedPlayWake,
};
pub(super) use cargo_driver::*;
pub(super) use source::*;

const MAX_TOOL_IDENTITY_BYTES: usize = 16 * 1024;
const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;
const MAX_MANIFEST_BYTES: u64 = 4 * 1024 * 1024;
const MAX_LOCKFILE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_TOOL_PATH_BYTES: usize = 32 * 1024;
const PROCESS_SPAWN_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DesktopBuildPhase {
    CargoIdentity,
    RustcIdentity,
    Metadata,
    CargoBuild,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DesktopBuildStatus {
    Running,
    Cancelling,
    CleaningUp,
    Succeeded,
    Failed(DesktopBuildFailureKind),
    Cancelled,
    Superseded(DesktopBuildSupersededReason),
}

#[derive(Debug)]
pub(super) struct DesktopBuildRequest {
    project_root: PathBuf,
    manifest_path: PathBuf,
    lockfile_path: PathBuf,
    target_directory: PathBuf,
    expected_revision: DesktopBuildInputRevision,
    tools: DesktopBuildTools,
}

impl DesktopBuildRequest {
    pub(super) fn new(
        project_root: impl Into<PathBuf>,
        expected_revision: DesktopBuildInputRevision,
        tools: DesktopBuildTools,
    ) -> Result<Self, DesktopBuildRequestError> {
        let project_root = project_root.into();
        if !project_root.is_absolute() {
            return Err(DesktopBuildRequestError::ProjectRootNotAbsolute);
        }
        let project_root = fs::canonicalize(&project_root)
            .map_err(DesktopBuildRequestError::ProjectRootCanonicalize)?;
        if !fs::metadata(&project_root)
            .map_err(DesktopBuildRequestError::ProjectRootInspect)?
            .is_dir()
        {
            return Err(DesktopBuildRequestError::ProjectRootNotDirectory);
        }

        Ok(Self {
            manifest_path: project_root.join("Cargo.toml"),
            lockfile_path: project_root.join("Cargo.lock"),
            target_directory: project_root.join("target"),
            project_root,
            expected_revision,
            tools,
        })
    }

    pub(super) fn project_root(&self) -> &Path {
        &self.project_root
    }
}

#[derive(Debug, Error)]
pub(super) enum DesktopBuildRequestError {
    #[error("desktop build project root must be absolute")]
    ProjectRootNotAbsolute,
    #[error("desktop build project root could not be canonicalized")]
    ProjectRootCanonicalize(#[source] io::Error),
    #[error("desktop build project root could not be inspected")]
    ProjectRootInspect(#[source] io::Error),
    #[error("desktop build project root is not a directory")]
    ProjectRootNotDirectory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DesktopBuildToolchainFacts {
    cargo_version_digest: ContentDigest,
    rustc_verbose_version_digest: ContentDigest,
    host_target: String,
}

impl DesktopBuildToolchainFacts {
    pub(super) const fn cargo_version_digest(&self) -> ContentDigest {
        self.cargo_version_digest
    }

    pub(super) const fn rustc_verbose_version_digest(&self) -> ContentDigest {
        self.rustc_verbose_version_digest
    }

    pub(super) fn host_target(&self) -> &str {
        &self.host_target
    }
}

#[derive(Debug)]
pub(super) struct VerifiedDesktopBuildCandidate {
    source_revision: DesktopBuildInputRevision,
    manifest_path: PathBuf,
    lockfile_path: PathBuf,
    toolchain: DesktopBuildToolchainFacts,
    metadata_digest: ContentDigest,
    invocation: CargoDesktopBuildInvocation,
    invocation_digest: ContentDigest,
    build: CargoDesktopBuildFacts,
}

impl VerifiedDesktopBuildCandidate {
    pub(super) const fn source_revision(&self) -> DesktopBuildInputRevision {
        self.source_revision
    }

    /// Revalidates the two Cargo authority members immediately before generation publication.
    ///
    /// The absolute paths are transient Editor process facts and must not enter a generation
    /// manifest. A caller that receives `Err` must publish no generation.
    pub(super) fn revalidate_for_publication(&self) -> Result<(), DesktopBuildSupersededReason> {
        let current = capture_input_revision_at(&self.manifest_path, &self.lockfile_path)
            .map_err(|error| DesktopBuildSupersededReason::InputUnavailable(error.member()))?;
        match revision_mismatch(self.source_revision, current) {
            Some(DesktopBuildInputMember::Manifest) => {
                Err(DesktopBuildSupersededReason::ManifestChanged)
            }
            Some(DesktopBuildInputMember::Lockfile) => {
                Err(DesktopBuildSupersededReason::LockfileChanged)
            }
            None => Ok(()),
        }
    }

    pub(super) const fn toolchain(&self) -> &DesktopBuildToolchainFacts {
        &self.toolchain
    }

    pub(super) const fn metadata_digest(&self) -> ContentDigest {
        self.metadata_digest
    }

    pub(super) const fn invocation(&self) -> &CargoDesktopBuildInvocation {
        &self.invocation
    }

    pub(super) const fn invocation_digest(&self) -> ContentDigest {
        self.invocation_digest
    }

    pub(super) fn artifact_path(&self) -> &Path {
        self.build.artifact().executable()
    }
}

#[derive(Debug)]
pub(super) enum DesktopBuildTerminal {
    Succeeded,
    Failed(DesktopBuildFailure),
    Cancelled,
    Superseded(DesktopBuildSupersededReason),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DesktopBuildSupersededReason {
    ManifestChanged,
    LockfileChanged,
    InputUnavailable(DesktopBuildInputMember),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DesktopBuildFailureKind {
    ProcessExit,
    OutputLoss,
    OutputLimitExceeded,
    OutputAllocationFailed,
    UnterminatedCargoMessage,
    InvalidToolIdentity,
    Metadata,
    Invocation,
    CargoMessages,
    ArtifactNotExecutable,
    ArtifactTooLarge,
    ArtifactInspect,
    Spawn,
    SpawnTimeout,
}

#[derive(Debug, Error)]
pub(super) enum DesktopBuildFailure {
    #[error("the supervised {phase:?} process exited unsuccessfully with code {code:?}")]
    ProcessExit {
        phase: DesktopBuildPhase,
        code: Option<i32>,
    },
    #[error("the supervised {phase:?} process lost {stream:?} output")]
    OutputLoss {
        phase: DesktopBuildPhase,
        stream: ProcessOutputStream,
    },
    #[error("the supervised {phase:?} stdout exceeded its bounded size")]
    OutputLimitExceeded { phase: DesktopBuildPhase },
    #[error("the supervised {phase:?} stdout allocation failed")]
    OutputAllocationFailed { phase: DesktopBuildPhase },
    #[error("the supervised Cargo JSON stream ended without its record delimiter")]
    UnterminatedCargoMessage,
    #[error("the {phase:?} tool emitted an invalid bounded identity")]
    InvalidToolIdentity { phase: DesktopBuildPhase },
    #[error("Cargo metadata could not be admitted")]
    Metadata(#[source] CargoMetadataFactsError),
    #[error("the exact desktop Cargo invocation could not be derived")]
    Invocation(#[source] CargoDesktopInvocationError),
    #[error("the exact desktop Cargo JSON stream could not be admitted")]
    CargoMessages(#[source] CargoDesktopBuildError),
    #[error("the selected desktop artifact is not one regular executable file")]
    ArtifactNotExecutable,
    #[error("the selected desktop artifact exceeded the generation byte limit")]
    ArtifactTooLarge,
    #[error("the selected desktop artifact could not be inspected")]
    ArtifactInspect(#[source] io::Error),
    #[error("the next supervised {phase:?} process could not be started")]
    Spawn {
        phase: DesktopBuildPhase,
        #[source]
        source: SupervisedProcessSpawnFailure,
    },
    #[error("the supervised {phase:?} process spawn deadline exceeded Instant")]
    SpawnDeadlineOverflow { phase: DesktopBuildPhase },
    #[error("the supervised {phase:?} process did not finish spawning before its deadline")]
    SpawnTimeout { phase: DesktopBuildPhase },
}

impl DesktopBuildFailure {
    const fn kind(&self) -> DesktopBuildFailureKind {
        match self {
            Self::ProcessExit { .. } => DesktopBuildFailureKind::ProcessExit,
            Self::OutputLoss { .. } => DesktopBuildFailureKind::OutputLoss,
            Self::OutputLimitExceeded { .. } => DesktopBuildFailureKind::OutputLimitExceeded,
            Self::OutputAllocationFailed { .. } => DesktopBuildFailureKind::OutputAllocationFailed,
            Self::UnterminatedCargoMessage => DesktopBuildFailureKind::UnterminatedCargoMessage,
            Self::InvalidToolIdentity { .. } => DesktopBuildFailureKind::InvalidToolIdentity,
            Self::Metadata(_) => DesktopBuildFailureKind::Metadata,
            Self::Invocation(_) => DesktopBuildFailureKind::Invocation,
            Self::CargoMessages(_) => DesktopBuildFailureKind::CargoMessages,
            Self::ArtifactNotExecutable => DesktopBuildFailureKind::ArtifactNotExecutable,
            Self::ArtifactTooLarge => DesktopBuildFailureKind::ArtifactTooLarge,
            Self::ArtifactInspect(_) => DesktopBuildFailureKind::ArtifactInspect,
            Self::Spawn { .. } | Self::SpawnDeadlineOverflow { .. } => {
                DesktopBuildFailureKind::Spawn
            }
            Self::SpawnTimeout { .. } => DesktopBuildFailureKind::SpawnTimeout,
        }
    }
}

#[derive(Debug, Error)]
pub(super) enum DesktopBuildStartError {
    #[error("desktop build input could not be captured")]
    Input(#[source] DesktopBuildInputError),
    #[error("desktop build input no longer matches the admitted source receipt")]
    SourceChanged { member: DesktopBuildInputMember },
    #[error("the Cargo identity process could not be started")]
    Spawn(#[source] SupervisedProcessSpawnFailure),
    #[error("the Cargo identity process spawn deadline exceeded Instant")]
    SpawnDeadlineOverflow,
}

#[derive(Debug, Error)]
pub(super) enum DesktopBuildAdvanceError {
    #[error("the supervised build process spawn could not advance")]
    Spawn(#[from] SupervisedProcessSpawnFailure),
    #[error("the supervised build process could not advance")]
    Process(#[from] SupervisedProcessError),
    #[error("the aggregate supervised build log sequence was exhausted")]
    RawLog(#[from] ProcessLogSequenceExhausted),
    #[error("the supervised build owner entered an invalid private state")]
    InvalidState,
    #[error("a cleanup receipt did not prove containment and reader termination")]
    InvalidCleanupReceipt,
}

enum PendingTerminal {
    Failed(DesktopBuildFailure),
    Cancelled,
}

pub(super) struct DesktopBuildAttempt {
    request: DesktopBuildRequest,
    phase: DesktopBuildPhase,
    spawn: Option<SupervisedProcessSpawnOwner>,
    process: Option<SupervisedProcessOwner>,
    wake: SupervisedPlayWake,
    phase_stdout: BoundedStdout,
    cargo_decoder: Option<CargoDesktopBuildDecoder>,
    cargo_stdout_observed: bool,
    cargo_stdout_ended_with_newline: bool,
    raw_logs: ProcessLogRing,
    cleanup_incomplete: bool,
    pending_terminal: Option<PendingTerminal>,
    terminal: Option<DesktopBuildTerminal>,
    terminal_diagnostics: Option<CargoDiagnostics>,
    candidate: Option<VerifiedDesktopBuildCandidate>,
    cargo_identity: Option<ToolIdentityFact>,
    rustc_identity: Option<RustcIdentityFact>,
    metadata: Option<CargoMetadataFacts>,
    metadata_digest: Option<ContentDigest>,
    invocation: Option<CargoDesktopBuildInvocation>,
    invocation_digest: Option<ContentDigest>,
}

impl DesktopBuildAttempt {
    pub(super) fn start(
        request: DesktopBuildRequest,
        now: Instant,
        wake: SupervisedPlayWake,
    ) -> Result<Self, DesktopBuildStartError> {
        let current = capture_input_revision(&request).map_err(DesktopBuildStartError::Input)?;
        if let Some(member) = revision_mismatch(request.expected_revision, current) {
            return Err(DesktopBuildStartError::SourceChanged { member });
        }

        let spawn_deadline = now
            .checked_add(PROCESS_SPAWN_TIMEOUT)
            .ok_or(DesktopBuildStartError::SpawnDeadlineOverflow)?;
        let spawn = SupervisedProcessSpawnOwner::start_command(
            cargo_identity_command(&request),
            default_log_limits(),
            spawn_deadline,
            wake.clone(),
        )
        .map_err(DesktopBuildStartError::Spawn)?;
        Ok(Self::from_spawn_owner(request, spawn, wake))
    }

    #[cfg(test)]
    fn from_spawned_process(request: DesktopBuildRequest, process: SupervisedProcessOwner) -> Self {
        Self::from_parts(request, None, Some(process), SupervisedPlayWake::default())
    }

    fn from_spawn_owner(
        request: DesktopBuildRequest,
        spawn: SupervisedProcessSpawnOwner,
        wake: SupervisedPlayWake,
    ) -> Self {
        Self::from_parts(request, Some(spawn), None, wake)
    }

    fn from_parts(
        request: DesktopBuildRequest,
        spawn: Option<SupervisedProcessSpawnOwner>,
        process: Option<SupervisedProcessOwner>,
        wake: SupervisedPlayWake,
    ) -> Self {
        Self {
            request,
            phase: DesktopBuildPhase::CargoIdentity,
            spawn,
            process,
            wake,
            phase_stdout: BoundedStdout::new(MAX_TOOL_IDENTITY_BYTES),
            cargo_decoder: None,
            cargo_stdout_observed: false,
            cargo_stdout_ended_with_newline: false,
            raw_logs: ProcessLogRing::new(default_log_limits()),
            cleanup_incomplete: false,
            pending_terminal: None,
            terminal: None,
            terminal_diagnostics: None,
            candidate: None,
            cargo_identity: None,
            rustc_identity: None,
            metadata: None,
            metadata_digest: None,
            invocation: None,
            invocation_digest: None,
        }
    }

    #[cfg(test)]
    pub(super) fn waiting_test_attempt() -> Self {
        let executable = std::env::current_exe().expect("the test executable is available");
        let project_root = executable
            .parent()
            .expect("the test executable has a parent")
            .to_path_buf();
        let request = DesktopBuildRequest {
            manifest_path: project_root.join("Cargo.toml"),
            lockfile_path: project_root.join("Cargo.lock"),
            target_directory: project_root.join("target"),
            project_root,
            expected_revision: DesktopBuildInputRevision::new(
                ContentDigest::of_bytes(b"manifest"),
                ContentDigest::of_bytes(b"lockfile"),
            ),
            tools: DesktopBuildTools {
                cargo: executable.clone(),
                rustc: executable,
            },
        };
        let process = SupervisedProcessOwner::spawn(
            &crate::supervised_play::tests::helper_command("wait"),
            default_log_limits(),
        )
        .expect("the waiting build helper starts");
        Self::from_spawned_process(request, process)
    }

    pub(super) fn status(&self) -> DesktopBuildStatus {
        match self.terminal.as_ref() {
            Some(DesktopBuildTerminal::Succeeded) => DesktopBuildStatus::Succeeded,
            Some(DesktopBuildTerminal::Failed(failure)) => {
                DesktopBuildStatus::Failed(failure.kind())
            }
            Some(DesktopBuildTerminal::Cancelled) => DesktopBuildStatus::Cancelled,
            Some(DesktopBuildTerminal::Superseded(reason)) => {
                DesktopBuildStatus::Superseded(*reason)
            }
            None => match self.pending_terminal.as_ref() {
                Some(PendingTerminal::Cancelled) => DesktopBuildStatus::Cancelling,
                Some(PendingTerminal::Failed(_)) => DesktopBuildStatus::CleaningUp,
                None if self.cleanup_incomplete
                    || self
                        .spawn
                        .as_ref()
                        .and_then(SupervisedProcessSpawnOwner::retirement)
                        .is_some() =>
                {
                    DesktopBuildStatus::CleaningUp
                }
                None => DesktopBuildStatus::Running,
            },
        }
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        if self.cleanup_incomplete || self.terminal.is_some() {
            return None;
        }
        self.spawn
            .as_ref()
            .and_then(SupervisedProcessSpawnOwner::next_deadline)
            .into_iter()
            .chain(
                self.process
                    .as_ref()
                    .and_then(SupervisedProcessOwner::next_deadline),
            )
            .min()
    }

    pub(super) fn diagnostics_snapshot(&self) -> Option<CargoDiagnostics> {
        self.cargo_decoder
            .as_ref()
            .map(CargoDesktopBuildDecoder::diagnostics)
            .or_else(|| self.terminal_diagnostics.clone())
    }

    #[cfg(test)]
    pub(super) fn terminal(&self) -> Option<&DesktopBuildTerminal> {
        self.terminal.as_ref()
    }

    pub(super) fn take_candidate(&mut self) -> Option<VerifiedDesktopBuildCandidate> {
        matches!(
            self.terminal.as_ref(),
            Some(DesktopBuildTerminal::Succeeded)
        )
        .then(|| self.candidate.take())
        .flatten()
    }

    pub(super) fn request_cancel(&mut self, now: Instant) -> Result<(), DesktopBuildAdvanceError> {
        if self.terminal.is_some() {
            return Ok(());
        }
        self.pending_terminal
            .get_or_insert(PendingTerminal::Cancelled);
        self.request_owned_cleanup(now)
    }

    pub(super) fn advance(&mut self, now: Instant) -> Result<(), DesktopBuildAdvanceError> {
        if self.terminal.is_some() {
            return Ok(());
        }

        self.advance_spawn(now)?;
        if self.terminal.is_some() || self.process.is_none() {
            return Ok(());
        }

        let process_advance = self
            .process
            .as_mut()
            .ok_or(DesktopBuildAdvanceError::InvalidState)?
            .advance_with_output(now);
        let (_, outputs) = match process_advance {
            Ok(result) => result,
            Err(error) => {
                self.cleanup_incomplete = true;
                return Err(error.into());
            }
        };
        for output in outputs {
            self.raw_logs.append(output.stream(), output.bytes())?;
            if output.loss() && self.pending_terminal.is_none() {
                self.pending_terminal =
                    Some(PendingTerminal::Failed(DesktopBuildFailure::OutputLoss {
                        phase: self.phase,
                        stream: output.stream(),
                    }));
                continue;
            }
            if self.pending_terminal.is_some() || output.stream() != ProcessOutputStream::Stdout {
                continue;
            }
            self.observe_stdout(output.bytes())?;
        }

        if self.pending_terminal.is_some() {
            self.request_process_terminate(now)?;
        }

        if let Some(receipt) = self
            .process
            .as_ref()
            .and_then(SupervisedProcessOwner::cleanup_receipt)
        {
            if !receipt.containment_empty() || !receipt.readers_stopped() {
                return Err(DesktopBuildAdvanceError::InvalidCleanupReceipt);
            }
            drop(self.process.take());
            self.cleanup_incomplete = false;
            self.complete_phase(receipt, now)?;
        }

        Ok(())
    }

    fn advance_spawn(&mut self, now: Instant) -> Result<(), DesktopBuildAdvanceError> {
        let Some(spawn) = self.spawn.as_mut() else {
            return Ok(());
        };
        if let Err(error) = spawn.advance(now) {
            self.cleanup_incomplete = spawn.cleanup_incomplete();
            return Err(error.into());
        }

        if let Some(acquired) = spawn.take_acquired() {
            let (process, _) = acquired.into_parts();
            drop(self.spawn.take());
            self.process = Some(process);
            self.cleanup_incomplete = false;
            return Ok(());
        }

        let Some(terminal) = spawn.take_terminal() else {
            return Ok(());
        };
        drop(self.spawn.take());
        self.cleanup_incomplete = false;
        self.complete_spawn(terminal);
        Ok(())
    }

    fn complete_spawn(&mut self, terminal: SupervisedProcessSpawnTerminal) {
        if matches!(self.pending_terminal, Some(PendingTerminal::Cancelled)) {
            self.pending_terminal = None;
            self.terminal = Some(DesktopBuildTerminal::Cancelled);
            return;
        }
        self.terminal = Some(DesktopBuildTerminal::Failed(match terminal {
            SupervisedProcessSpawnTerminal::Failed(source) => DesktopBuildFailure::Spawn {
                phase: self.phase,
                source,
            },
            SupervisedProcessSpawnTerminal::Retired(SupervisedProcessSpawnRetirement::TimedOut) => {
                DesktopBuildFailure::SpawnTimeout { phase: self.phase }
            }
            SupervisedProcessSpawnTerminal::Retired(
                SupervisedProcessSpawnRetirement::Cancelled,
            ) => DesktopBuildFailure::Spawn {
                phase: self.phase,
                source: SupervisedProcessSpawnFailure::WorkerDisconnected,
            },
        }));
    }

    fn observe_stdout(&mut self, bytes: &[u8]) -> Result<(), DesktopBuildAdvanceError> {
        if self.phase == DesktopBuildPhase::CargoBuild {
            if !bytes.is_empty() {
                self.cargo_stdout_observed = true;
                self.cargo_stdout_ended_with_newline = bytes.last() == Some(&b'\n');
            }
            let error = self
                .cargo_decoder
                .as_mut()
                .ok_or(DesktopBuildAdvanceError::InvalidState)?
                .push(bytes)
                .err();
            if let Some(source) = error {
                self.terminal_diagnostics = self
                    .cargo_decoder
                    .as_ref()
                    .map(CargoDesktopBuildDecoder::diagnostics);
                self.pending_terminal = Some(PendingTerminal::Failed(
                    DesktopBuildFailure::CargoMessages(source),
                ));
            }
            return Ok(());
        }

        if let Err(error) = self.phase_stdout.push(bytes) {
            self.pending_terminal = Some(PendingTerminal::Failed(match error {
                BoundedStdoutError::LimitExceeded => {
                    DesktopBuildFailure::OutputLimitExceeded { phase: self.phase }
                }
                BoundedStdoutError::AllocationFailed => {
                    DesktopBuildFailure::OutputAllocationFailed { phase: self.phase }
                }
            }));
        }
        Ok(())
    }

    fn request_owned_cleanup(&mut self, now: Instant) -> Result<(), DesktopBuildAdvanceError> {
        if let Some(spawn) = self.spawn.as_mut() {
            if spawn.cleanup_incomplete() {
                spawn.retry_cleanup(now)?;
            }
            spawn.request_cancel(now)?;
            self.cleanup_incomplete = false;
            return Ok(());
        }
        let process = self
            .process
            .as_mut()
            .ok_or(DesktopBuildAdvanceError::InvalidState)?;
        if self.cleanup_incomplete {
            process.retry_cleanup(now)?;
            self.cleanup_incomplete = false;
        } else {
            process.request_terminate(now)?;
        }
        Ok(())
    }

    fn request_process_terminate(&mut self, now: Instant) -> Result<(), DesktopBuildAdvanceError> {
        self.process
            .as_mut()
            .ok_or(DesktopBuildAdvanceError::InvalidState)?
            .request_terminate(now)?;
        Ok(())
    }

    fn complete_phase(
        &mut self,
        receipt: SupervisedProcessCleanupReceipt,
        now: Instant,
    ) -> Result<(), DesktopBuildAdvanceError> {
        if let Some(pending) = self.pending_terminal.take() {
            self.terminal = Some(match pending {
                PendingTerminal::Failed(failure) => DesktopBuildTerminal::Failed(failure),
                PendingTerminal::Cancelled => DesktopBuildTerminal::Cancelled,
            });
            return Ok(());
        }

        if self.phase == DesktopBuildPhase::CargoBuild {
            return self.complete_cargo_build(receipt);
        }
        if !receipt.exit().success() {
            self.terminal = Some(DesktopBuildTerminal::Failed(
                DesktopBuildFailure::ProcessExit {
                    phase: self.phase,
                    code: receipt.exit().code(),
                },
            ));
            return Ok(());
        }

        match self.phase {
            DesktopBuildPhase::CargoIdentity => self.complete_cargo_identity(now),
            DesktopBuildPhase::RustcIdentity => self.complete_rustc_identity(now),
            DesktopBuildPhase::Metadata => self.complete_metadata(now),
            DesktopBuildPhase::CargoBuild => Err(DesktopBuildAdvanceError::InvalidState),
        }
    }

    fn complete_cargo_identity(&mut self, now: Instant) -> Result<(), DesktopBuildAdvanceError> {
        let output = self.phase_stdout.take();
        let Some(identity) = parse_tool_identity(&output) else {
            self.terminal = Some(DesktopBuildTerminal::Failed(
                DesktopBuildFailure::InvalidToolIdentity { phase: self.phase },
            ));
            return Ok(());
        };
        self.cargo_identity = Some(identity);
        self.start_phase(
            DesktopBuildPhase::RustcIdentity,
            rustc_identity_command(&self.request),
            MAX_TOOL_IDENTITY_BYTES,
            None,
            now,
        )
    }

    fn complete_rustc_identity(&mut self, now: Instant) -> Result<(), DesktopBuildAdvanceError> {
        let output = self.phase_stdout.take();
        let Some(identity) = parse_rustc_identity(&output) else {
            self.terminal = Some(DesktopBuildTerminal::Failed(
                DesktopBuildFailure::InvalidToolIdentity { phase: self.phase },
            ));
            return Ok(());
        };
        self.rustc_identity = Some(identity);
        self.start_phase(
            DesktopBuildPhase::Metadata,
            metadata_command(&self.request),
            MAX_METADATA_BYTES,
            None,
            now,
        )
    }

    fn complete_metadata(&mut self, now: Instant) -> Result<(), DesktopBuildAdvanceError> {
        if let Some(reason) = self.source_superseded() {
            self.terminal = Some(DesktopBuildTerminal::Superseded(reason));
            return Ok(());
        }
        let metadata_bytes = self.phase_stdout.take();
        let metadata_digest = ContentDigest::of_bytes(&metadata_bytes);
        let metadata = match CargoMetadataFacts::parse(&metadata_bytes, &self.request.manifest_path)
        {
            Ok(metadata) => metadata,
            Err(error) => {
                self.terminal = Some(DesktopBuildTerminal::Failed(DesktopBuildFailure::Metadata(
                    error,
                )));
                return Ok(());
            }
        };
        let host_target = self
            .rustc_identity
            .as_ref()
            .ok_or(DesktopBuildAdvanceError::InvalidState)?
            .host_target
            .clone();
        let invocation = match metadata.desktop_invocation(host_target) {
            Ok(invocation) => invocation,
            Err(error) => {
                self.terminal = Some(DesktopBuildTerminal::Failed(
                    DesktopBuildFailure::Invocation(error),
                ));
                return Ok(());
            }
        };
        let invocation_digest = digest_build_invocation(&invocation);
        let decoder = CargoDesktopBuildDecoder::new(invocation.clone());
        let command = cargo_build_command(&self.request, &invocation);
        self.metadata = Some(metadata);
        self.metadata_digest = Some(metadata_digest);
        self.invocation = Some(invocation);
        self.invocation_digest = Some(invocation_digest);
        self.start_phase(
            DesktopBuildPhase::CargoBuild,
            command,
            0,
            Some(decoder),
            now,
        )
    }

    fn complete_cargo_build(
        &mut self,
        receipt: SupervisedProcessCleanupReceipt,
    ) -> Result<(), DesktopBuildAdvanceError> {
        if let Some(reason) = self.source_superseded() {
            self.terminal = Some(DesktopBuildTerminal::Superseded(reason));
            return Ok(());
        }
        if self.cargo_stdout_observed && !self.cargo_stdout_ended_with_newline {
            self.terminal = Some(DesktopBuildTerminal::Failed(
                DesktopBuildFailure::UnterminatedCargoMessage,
            ));
            self.terminal_diagnostics = self
                .cargo_decoder
                .as_ref()
                .map(CargoDesktopBuildDecoder::diagnostics);
            return Ok(());
        }

        let decoder = self
            .cargo_decoder
            .as_mut()
            .ok_or(DesktopBuildAdvanceError::InvalidState)?;
        let build = match decoder.finish() {
            Ok(build) => build,
            Err(error) => {
                self.terminal_diagnostics = Some(decoder.diagnostics());
                self.terminal = Some(DesktopBuildTerminal::Failed(
                    DesktopBuildFailure::CargoMessages(error),
                ));
                return Ok(());
            }
        };
        if !receipt.exit().success() {
            self.terminal_diagnostics = Some(build.diagnostics().clone());
            self.terminal = Some(DesktopBuildTerminal::Failed(
                DesktopBuildFailure::ProcessExit {
                    phase: self.phase,
                    code: receipt.exit().code(),
                },
            ));
            return Ok(());
        }
        if let Err(failure) = verify_artifact(build.artifact().executable()) {
            self.terminal_diagnostics = Some(build.diagnostics().clone());
            self.terminal = Some(DesktopBuildTerminal::Failed(failure));
            return Ok(());
        }

        let cargo_identity = self
            .cargo_identity
            .take()
            .ok_or(DesktopBuildAdvanceError::InvalidState)?;
        let rustc_identity = self
            .rustc_identity
            .take()
            .ok_or(DesktopBuildAdvanceError::InvalidState)?;
        let toolchain = DesktopBuildToolchainFacts {
            cargo_version_digest: cargo_identity.digest,
            rustc_verbose_version_digest: rustc_identity.identity.digest,
            host_target: rustc_identity.host_target,
        };
        let _ = cargo_identity.text;
        let _ = rustc_identity.identity.text;
        self.terminal_diagnostics = Some(build.diagnostics().clone());
        self.candidate = Some(VerifiedDesktopBuildCandidate {
            source_revision: self.request.expected_revision,
            manifest_path: self.request.manifest_path.clone(),
            lockfile_path: self.request.lockfile_path.clone(),
            toolchain,
            metadata_digest: self
                .metadata_digest
                .take()
                .ok_or(DesktopBuildAdvanceError::InvalidState)?,
            invocation: self
                .invocation
                .take()
                .ok_or(DesktopBuildAdvanceError::InvalidState)?,
            invocation_digest: self
                .invocation_digest
                .take()
                .ok_or(DesktopBuildAdvanceError::InvalidState)?,
            build,
        });
        self.terminal = Some(DesktopBuildTerminal::Succeeded);
        Ok(())
    }

    fn start_phase(
        &mut self,
        phase: DesktopBuildPhase,
        command: SupervisedProcessCommand,
        stdout_limit: usize,
        decoder: Option<CargoDesktopBuildDecoder>,
        now: Instant,
    ) -> Result<(), DesktopBuildAdvanceError> {
        self.phase = phase;
        self.process = None;
        self.phase_stdout.reset(stdout_limit);
        self.cargo_decoder = decoder;
        self.cargo_stdout_observed = false;
        self.cargo_stdout_ended_with_newline = false;
        let Some(spawn_deadline) = now.checked_add(PROCESS_SPAWN_TIMEOUT) else {
            self.terminal = Some(DesktopBuildTerminal::Failed(
                DesktopBuildFailure::SpawnDeadlineOverflow { phase },
            ));
            return Ok(());
        };
        match SupervisedProcessSpawnOwner::start_command(
            command,
            default_log_limits(),
            spawn_deadline,
            self.wake.clone(),
        ) {
            Ok(spawn) => self.spawn = Some(spawn),
            Err(source) => {
                self.terminal = Some(DesktopBuildTerminal::Failed(DesktopBuildFailure::Spawn {
                    phase,
                    source,
                }));
            }
        }
        Ok(())
    }

    fn source_superseded(&self) -> Option<DesktopBuildSupersededReason> {
        match capture_input_revision(&self.request) {
            Ok(current) => revision_mismatch(self.request.expected_revision, current).map(
                |member| match member {
                    DesktopBuildInputMember::Manifest => {
                        DesktopBuildSupersededReason::ManifestChanged
                    }
                    DesktopBuildInputMember::Lockfile => {
                        DesktopBuildSupersededReason::LockfileChanged
                    }
                },
            ),
            Err(error) => Some(DesktopBuildSupersededReason::InputUnavailable(
                error.member(),
            )),
        }
    }
}

impl Drop for DesktopBuildAttempt {
    fn drop(&mut self) {
        self.raw_logs.purge();
    }
}

#[cfg(test)]
#[path = "build/tests.rs"]
mod tests;
