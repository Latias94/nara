use std::{
    io,
    path::PathBuf,
    time::{Duration, Instant},
};

#[cfg(all(unix, target_os = "linux", target_arch = "x86_64"))]
use std::{fs::File, io::Read as _};

use nara::project_host::{
    SupervisedDesktopGenerationId, SupervisedDesktopLaunchContext,
    SupervisedDesktopReadyAttestation, SupervisedDesktopSessionNonce,
    SupervisedDesktopSourceAdmissionError, SupervisedDesktopSourceAuthority,
    SupervisedDesktopWireError, admit_supervised_desktop_source_receipt,
};
use thiserror::Error;

use super::{
    generation::ImmutableExecutableGeneration,
    handshake::{
        SupervisedDesktopHandshakeFailureKind, SupervisedDesktopHandshakeOwner,
        SupervisedDesktopHandshakeStartError, SupervisedDesktopHandshakeStatus,
        SupervisedDesktopRuntimeExpectation,
    },
    process::{
        SupervisedProcessCleanupReceipt, SupervisedProcessError, SupervisedProcessOwner,
        SupervisedProcessSpawnFailure, SupervisedProcessSpawnOwner,
        SupervisedProcessSpawnRetirement, SupervisedProcessSpawnTerminal,
        SupervisedProductEnvironmentPolicy, SupervisedProductProcessCommand, default_log_limits,
    },
    wake::SupervisedPlayWake,
};

const PROCESS_SPAWN_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct SupervisedDesktopAttemptId(u64);

impl SupervisedDesktopAttemptId {
    pub(super) const fn new(value: u64) -> Option<Self> {
        if value == 0 { None } else { Some(Self(value)) }
    }

    #[cfg(test)]
    pub(super) const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone)]
pub(super) struct SupervisedDesktopLaunchRequest {
    attempt: SupervisedDesktopAttemptId,
    project_root: PathBuf,
    working_directory: PathBuf,
    generation: ImmutableExecutableGeneration,
    source: SupervisedDesktopSourceAuthority,
    runtime_expectation: SupervisedDesktopRuntimeExpectation,
    ready_timeout: Duration,
}

impl SupervisedDesktopLaunchRequest {
    pub(super) fn new(
        attempt: SupervisedDesktopAttemptId,
        project_root: PathBuf,
        working_directory: PathBuf,
        generation: ImmutableExecutableGeneration,
        source_receipt: Vec<u8>,
        ready_timeout: Duration,
    ) -> Result<Self, SupervisedDesktopSupervisorStartError> {
        let source = admit_supervised_desktop_source_receipt(&source_receipt)
            .map_err(SupervisedDesktopSupervisorStartError::SourceAdmission)?;
        if source.canonical_source_receipt() != generation.source_receipt() {
            return Err(SupervisedDesktopSupervisorStartError::SourceReceiptMismatch);
        }
        if ready_timeout.is_zero() {
            return Err(SupervisedDesktopSupervisorStartError::InvalidReadyTimeout);
        }
        if !working_directory.is_absolute() {
            return Err(SupervisedDesktopSupervisorStartError::InvalidWorkingDirectory);
        }
        let runtime_expectation =
            SupervisedDesktopRuntimeExpectation::from_source_authority(&source)
                .map_err(SupervisedDesktopSupervisorStartError::Wire)?;
        Ok(Self {
            attempt,
            project_root,
            working_directory,
            generation,
            source,
            runtime_expectation,
            ready_timeout,
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SupervisedDesktopSupervisorStatus {
    Starting,
    Running,
    Stopping,
    CleanupIncomplete,
    Complete,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SupervisedDesktopControlResult {
    Applied,
    IgnoredStale,
    Busy,
    Complete,
}

pub(super) enum SupervisedDesktopSupervisorTerminal {
    Stopped,
    Exited,
    Failed(SupervisedDesktopSupervisorFailure),
}

enum AfterCleanup {
    Stop,
    NaturalExit,
    Failure(SupervisedDesktopSupervisorFailure),
}

pub(super) struct SupervisedDesktopSessionSupervisor {
    request: SupervisedDesktopLaunchRequest,
    pending_launch: Option<SupervisedDesktopLaunchContext>,
    spawn: Option<SupervisedProcessSpawnOwner>,
    process: Option<SupervisedProcessOwner>,
    wake: SupervisedPlayWake,
    handshake: Option<SupervisedDesktopHandshakeOwner>,
    ready: Option<SupervisedDesktopReadyAttestation>,
    after_cleanup: Option<AfterCleanup>,
    cleanup_incomplete: Option<&'static str>,
    terminal: Option<SupervisedDesktopSupervisorTerminal>,
    process_exit_observed: bool,
}

impl SupervisedDesktopSessionSupervisor {
    pub(super) fn start(
        request: SupervisedDesktopLaunchRequest,
        now: Instant,
        wake: SupervisedPlayWake,
    ) -> Result<Self, SupervisedDesktopSupervisorStartError> {
        let nonce = fresh_session_nonce()?;
        let generation_id = SupervisedDesktopGenerationId::new(*request.generation.id().as_bytes())
            .map_err(SupervisedDesktopSupervisorStartError::Wire)?;
        let launch = SupervisedDesktopLaunchContext::new(
            request.project_root.clone(),
            request.source.project_stable_id(),
            request.source.profile_name(),
            request.generation.product_identity().clone(),
            generation_id,
            nonce,
            request.generation.executable_digest(),
            request.generation.launch_manifest_digest(),
            request.source.canonical_source_receipt().to_vec(),
        )
        .map_err(SupervisedDesktopSupervisorStartError::Wire)?;
        let command = SupervisedProductProcessCommand::new(
            SupervisedProductEnvironmentPolicy::AllowlistedDesktopSession,
            request.working_directory.clone(),
        );
        let spawn_deadline = now
            .checked_add(PROCESS_SPAWN_TIMEOUT)
            .ok_or(SupervisedDesktopSupervisorStartError::SpawnDeadlineOverflow)?;
        let spawn = SupervisedProcessSpawnOwner::start_exact_generation(
            request.generation.clone(),
            command,
            default_log_limits(),
            spawn_deadline,
            wake.clone(),
        )
        .map_err(SupervisedDesktopSupervisorStartError::Spawn)?;

        Ok(Self {
            request,
            pending_launch: Some(launch),
            spawn: Some(spawn),
            process: None,
            wake,
            handshake: None,
            ready: None,
            after_cleanup: None,
            cleanup_incomplete: None,
            terminal: None,
            process_exit_observed: false,
        })
    }

    pub(super) const fn attempt(&self) -> SupervisedDesktopAttemptId {
        self.request.attempt
    }

    pub(super) fn status(&self) -> SupervisedDesktopSupervisorStatus {
        if self.terminal.is_some() {
            SupervisedDesktopSupervisorStatus::Complete
        } else if self.cleanup_incomplete.is_some()
            || self
                .spawn
                .as_ref()
                .is_some_and(SupervisedProcessSpawnOwner::cleanup_incomplete)
        {
            SupervisedDesktopSupervisorStatus::CleanupIncomplete
        } else if self.after_cleanup.is_some()
            || self
                .spawn
                .as_ref()
                .and_then(SupervisedProcessSpawnOwner::retirement)
                .is_some()
        {
            SupervisedDesktopSupervisorStatus::Stopping
        } else if self.ready.is_some() {
            SupervisedDesktopSupervisorStatus::Running
        } else {
            SupervisedDesktopSupervisorStatus::Starting
        }
    }

    pub(super) fn ready(&self) -> Option<&SupervisedDesktopReadyAttestation> {
        self.ready.as_ref()
    }

    pub(super) fn terminal(&self) -> Option<&SupervisedDesktopSupervisorTerminal> {
        self.terminal.as_ref()
    }

    pub(super) fn process(&self) -> Option<&SupervisedProcessOwner> {
        self.process.as_ref()
    }

    pub(super) fn next_deadline(&self) -> Option<Instant> {
        if self.cleanup_is_incomplete() || self.terminal.is_some() {
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
            .chain(
                self.handshake
                    .as_ref()
                    .and_then(SupervisedDesktopHandshakeOwner::next_deadline),
            )
            .min()
    }

    pub(super) fn request_stop(
        &mut self,
        attempt: SupervisedDesktopAttemptId,
        now: Instant,
    ) -> Result<SupervisedDesktopControlResult, SupervisedDesktopSupervisorAdvanceError> {
        self.request_cleanup(attempt, now, AfterCleanup::Stop)
    }

    pub(super) fn request_close(
        &mut self,
        attempt: SupervisedDesktopAttemptId,
        now: Instant,
    ) -> Result<SupervisedDesktopControlResult, SupervisedDesktopSupervisorAdvanceError> {
        self.request_cleanup(attempt, now, AfterCleanup::Stop)
    }

    pub(super) fn retry_cleanup(
        &mut self,
        attempt: SupervisedDesktopAttemptId,
        now: Instant,
    ) -> Result<SupervisedDesktopControlResult, SupervisedDesktopSupervisorAdvanceError> {
        if attempt != self.request.attempt {
            return Ok(SupervisedDesktopControlResult::IgnoredStale);
        }
        if self.terminal.is_some() {
            return Ok(SupervisedDesktopControlResult::Complete);
        }
        let spawn_cleanup_incomplete = self
            .spawn
            .as_ref()
            .is_some_and(SupervisedProcessSpawnOwner::cleanup_incomplete);
        if self.cleanup_incomplete.is_none() && !spawn_cleanup_incomplete {
            return Ok(SupervisedDesktopControlResult::Busy);
        }
        if let Some(spawn) = self.spawn.as_mut() {
            spawn
                .retry_cleanup(now)
                .map_err(SupervisedDesktopSupervisorAdvanceError::Spawn)?;
        }
        if let Some(process) = self.process.as_mut() {
            process
                .retry_cleanup(now)
                .map_err(SupervisedDesktopSupervisorAdvanceError::Process)?;
        }
        if let Some(handshake) = self.handshake.as_mut() {
            handshake
                .request_cancel()
                .map_err(SupervisedDesktopSupervisorAdvanceError::Process)?;
        }
        self.cleanup_incomplete = None;
        Ok(SupervisedDesktopControlResult::Applied)
    }

    pub(super) fn advance(
        &mut self,
        now: Instant,
    ) -> Result<(), SupervisedDesktopSupervisorAdvanceError> {
        if self.terminal.is_some() {
            return Ok(());
        }

        self.advance_spawn(now)?;
        if self.terminal.is_some() || self.process.is_none() {
            return Ok(());
        }
        self.advance_process(now)?;
        Ok(())
    }

    fn advance_spawn(
        &mut self,
        now: Instant,
    ) -> Result<(), SupervisedDesktopSupervisorAdvanceError> {
        let Some(spawn) = self.spawn.as_mut() else {
            return Ok(());
        };
        if let Err(error) = spawn.advance(now) {
            if matches!(error, SupervisedProcessSpawnFailure::Cleanup(_)) {
                self.cleanup_incomplete =
                    Some("the late supervised desktop process could not complete spawn cleanup");
                return Ok(());
            }
            return Err(error.into());
        }

        if let Some(acquired) = spawn.take_acquired() {
            let (process, spawned_at) = acquired.into_parts();
            drop(self.spawn.take());
            self.process = Some(process);
            self.start_handshake(spawned_at);
            return Ok(());
        }

        let Some(terminal) = spawn.take_terminal() else {
            return Ok(());
        };
        drop(self.spawn.take());
        self.complete_spawn(terminal);
        Ok(())
    }

    fn start_handshake(&mut self, spawned_at: Instant) {
        let Some(launch) = self.pending_launch.take() else {
            self.after_cleanup = Some(AfterCleanup::Failure(
                SupervisedDesktopSupervisorFailure::InvalidState,
            ));
            return;
        };
        let Some(deadline) = spawned_at.checked_add(self.request.ready_timeout) else {
            self.after_cleanup = Some(AfterCleanup::Failure(
                SupervisedDesktopSupervisorFailure::ReadyDeadlineOverflow,
            ));
            return;
        };
        let Some(process) = self.process.as_mut() else {
            self.after_cleanup = Some(AfterCleanup::Failure(
                SupervisedDesktopSupervisorFailure::InvalidState,
            ));
            return;
        };
        let handshake = match process.take_bootstrap() {
            Some(channel) => SupervisedDesktopHandshakeOwner::start(
                channel,
                launch,
                self.request.runtime_expectation.clone(),
                deadline,
                self.wake.clone(),
            ),
            None => Err(SupervisedDesktopHandshakeStartError::WriterSpawn(
                io::Error::other("the requested bootstrap endpoint was not returned"),
            )),
        };
        match handshake {
            Ok(handshake) => self.handshake = Some(handshake),
            Err(error) => {
                self.after_cleanup = Some(AfterCleanup::Failure(
                    SupervisedDesktopSupervisorFailure::HandshakeStart(error),
                ));
            }
        }
    }

    fn complete_spawn(&mut self, terminal: SupervisedProcessSpawnTerminal) {
        if matches!(self.after_cleanup, Some(AfterCleanup::Stop)) {
            self.after_cleanup = None;
            self.terminal = Some(SupervisedDesktopSupervisorTerminal::Stopped);
            return;
        }
        match terminal {
            SupervisedProcessSpawnTerminal::Failed(error) => {
                self.terminal = Some(SupervisedDesktopSupervisorTerminal::Failed(
                    SupervisedDesktopSupervisorFailure::Spawn(error),
                ));
            }
            SupervisedProcessSpawnTerminal::Retired(SupervisedProcessSpawnRetirement::TimedOut) => {
                self.terminal = Some(SupervisedDesktopSupervisorTerminal::Failed(
                    SupervisedDesktopSupervisorFailure::SpawnTimeout,
                ));
            }
            SupervisedProcessSpawnTerminal::Retired(
                SupervisedProcessSpawnRetirement::Cancelled,
            ) => {
                let outcome = self.after_cleanup.take().unwrap_or(AfterCleanup::Stop);
                self.terminal = Some(match outcome {
                    AfterCleanup::Stop => SupervisedDesktopSupervisorTerminal::Stopped,
                    AfterCleanup::NaturalExit => SupervisedDesktopSupervisorTerminal::Failed(
                        SupervisedDesktopSupervisorFailure::ExitedBeforeReady,
                    ),
                    AfterCleanup::Failure(failure) => {
                        SupervisedDesktopSupervisorTerminal::Failed(failure)
                    }
                });
            }
        }
    }

    fn advance_process(
        &mut self,
        now: Instant,
    ) -> Result<(), SupervisedDesktopSupervisorAdvanceError> {
        if self.after_cleanup.is_some() {
            self.terminate_process(now)?;
        }

        if let Some(handshake) = self.handshake.as_mut() {
            handshake.advance(now);
        }
        self.observe_handshake_terminal();

        let process_observation = match self
            .process
            .as_mut()
            .ok_or(SupervisedDesktopSupervisorAdvanceError::InvalidState)?
            .advance(now)
        {
            Ok(observation) => observation,
            Err(SupervisedProcessError::CleanupIncomplete(reason)) => {
                self.cleanup_incomplete = Some(reason);
                return Ok(());
            }
            Err(error) => {
                self.cleanup_incomplete = Some("the supervised desktop process could not advance");
                return Err(error.into());
            }
        };

        if process_observation.exit().is_some() {
            self.process_exit_observed = true;
            if let Some(handshake) = self.handshake.as_mut() {
                handshake.advance(now);
            }
            self.observe_handshake_terminal();
            if self.after_cleanup.is_none() {
                self.after_cleanup = Some(if self.ready.is_some() {
                    AfterCleanup::NaturalExit
                } else {
                    AfterCleanup::Failure(SupervisedDesktopSupervisorFailure::ExitedBeforeReady)
                });
            }
        }
        if self.after_cleanup.is_some() {
            self.terminate_process(now)?;
        }

        let receipt = self
            .process
            .as_ref()
            .and_then(SupervisedProcessOwner::cleanup_receipt);
        if let Some(receipt) = receipt
            && self.handshake_cleanup_complete()
        {
            self.finish_cleanup(receipt);
        }

        Ok(())
    }

    fn request_cleanup(
        &mut self,
        attempt: SupervisedDesktopAttemptId,
        now: Instant,
        after_cleanup: AfterCleanup,
    ) -> Result<SupervisedDesktopControlResult, SupervisedDesktopSupervisorAdvanceError> {
        let admission = admit_cleanup_request(
            self.request.attempt,
            attempt,
            self.terminal.is_some(),
            self.after_cleanup.is_some() || self.cleanup_is_incomplete(),
        );
        if admission != SupervisedDesktopControlResult::Applied {
            return Ok(admission);
        }

        self.advance(now)?;
        if self.terminal.is_some() {
            return Ok(SupervisedDesktopControlResult::Complete);
        }

        let cancellation = self
            .handshake
            .as_mut()
            .map_or(Ok(()), SupervisedDesktopHandshakeOwner::request_cancel);
        if self.after_cleanup.is_none() {
            self.after_cleanup = Some(after_cleanup);
        }
        let cleanup = self.request_owned_cleanup(now);
        cancellation?;
        cleanup?;
        Ok(SupervisedDesktopControlResult::Applied)
    }

    fn cleanup_is_incomplete(&self) -> bool {
        self.cleanup_incomplete.is_some()
            || self
                .spawn
                .as_ref()
                .is_some_and(SupervisedProcessSpawnOwner::cleanup_incomplete)
    }

    fn request_owned_cleanup(
        &mut self,
        now: Instant,
    ) -> Result<(), SupervisedDesktopSupervisorAdvanceError> {
        if let Some(spawn) = self.spawn.as_mut() {
            if let Err(error) = spawn.request_cancel(now) {
                if matches!(&error, SupervisedProcessSpawnFailure::Cleanup(_)) {
                    self.cleanup_incomplete =
                        Some("the late supervised desktop process could not begin spawn cleanup");
                }
                return Err(error.into());
            }
            return Ok(());
        }
        self.terminate_process(now)
    }

    fn terminate_process(
        &mut self,
        now: Instant,
    ) -> Result<(), SupervisedDesktopSupervisorAdvanceError> {
        let result = self
            .process
            .as_mut()
            .ok_or(SupervisedDesktopSupervisorAdvanceError::InvalidState)?
            .request_terminate(now);
        if let Err(error) = result {
            self.cleanup_incomplete =
                Some("the supervised desktop process could not begin or continue termination");
            return Err(error.into());
        }
        Ok(())
    }

    fn observe_handshake_terminal(&mut self) {
        let Some(handshake) = self.handshake.as_mut() else {
            return;
        };
        match handshake.status() {
            SupervisedDesktopHandshakeStatus::Ready => {
                if self.ready.is_none()
                    && let Some(ready) = handshake.take_ready()
                {
                    self.ready = Some(ready);
                }
                if self.process_exit_observed && self.after_cleanup.is_none() {
                    self.after_cleanup = Some(AfterCleanup::NaturalExit);
                }
            }
            SupervisedDesktopHandshakeStatus::Failed => {
                if self.after_cleanup.is_none() {
                    let failure = handshake
                        .failure()
                        .expect("a failed handshake retains its failure");
                    self.after_cleanup = Some(AfterCleanup::Failure(
                        SupervisedDesktopSupervisorFailure::Handshake(failure.kind()),
                    ));
                }
            }
            SupervisedDesktopHandshakeStatus::WritingLaunch
            | SupervisedDesktopHandshakeStatus::AwaitingReady => {}
        }
    }

    fn handshake_cleanup_complete(&self) -> bool {
        self.handshake.as_ref().is_none_or(|handshake| {
            handshake.writer_stopped()
                && matches!(
                    handshake.status(),
                    SupervisedDesktopHandshakeStatus::Ready
                        | SupervisedDesktopHandshakeStatus::Failed
                )
        })
    }

    fn finish_cleanup(&mut self, cleanup: SupervisedProcessCleanupReceipt) {
        if !cleanup.containment_empty() || !cleanup.readers_stopped() {
            self.cleanup_incomplete =
                Some("the process receipt did not prove containment and output-reader cleanup");
            return;
        }
        let outcome = self.after_cleanup.take().unwrap_or_else(|| {
            if self.ready.is_some() {
                AfterCleanup::NaturalExit
            } else {
                AfterCleanup::Failure(SupervisedDesktopSupervisorFailure::ExitedBeforeReady)
            }
        });
        self.cleanup_incomplete = None;
        self.terminal = Some(match outcome {
            AfterCleanup::Stop => SupervisedDesktopSupervisorTerminal::Stopped,
            AfterCleanup::NaturalExit if cleanup.exit().success() => {
                SupervisedDesktopSupervisorTerminal::Exited
            }
            AfterCleanup::NaturalExit => SupervisedDesktopSupervisorTerminal::Failed(
                SupervisedDesktopSupervisorFailure::NonZeroExit(cleanup.exit().code()),
            ),
            AfterCleanup::Failure(failure) => SupervisedDesktopSupervisorTerminal::Failed(failure),
        });
    }
}

fn admit_cleanup_request(
    active: SupervisedDesktopAttemptId,
    requested: SupervisedDesktopAttemptId,
    terminal: bool,
    busy: bool,
) -> SupervisedDesktopControlResult {
    if requested != active {
        SupervisedDesktopControlResult::IgnoredStale
    } else if terminal {
        SupervisedDesktopControlResult::Complete
    } else if busy {
        SupervisedDesktopControlResult::Busy
    } else {
        SupervisedDesktopControlResult::Applied
    }
}

fn fresh_session_nonce()
-> Result<SupervisedDesktopSessionNonce, SupervisedDesktopSupervisorStartError> {
    let mut bytes = [0_u8; 32];
    fill_session_nonce(&mut bytes)?;
    SupervisedDesktopSessionNonce::new(bytes).map_err(SupervisedDesktopSupervisorStartError::Wire)
}

#[cfg(windows)]
fn fill_session_nonce(bytes: &mut [u8; 32]) -> Result<(), SupervisedDesktopSupervisorStartError> {
    use windows_sys::Win32::Security::Cryptography::{
        BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
    };

    // SAFETY: `bytes` is writable for its exact length and the system-preferred RNG needs no
    // caller-owned algorithm handle.
    let status = unsafe {
        BCryptGenRandom(
            std::ptr::null_mut(),
            bytes.as_mut_ptr(),
            bytes.len() as u32,
            BCRYPT_USE_SYSTEM_PREFERRED_RNG,
        )
    };
    if status == 0 {
        Ok(())
    } else {
        Err(SupervisedDesktopSupervisorStartError::NonceIo(
            io::Error::other(format!("BCryptGenRandom failed with NTSTATUS {status:#x}")),
        ))
    }
}

#[cfg(all(unix, target_os = "linux", target_arch = "x86_64"))]
fn fill_session_nonce(bytes: &mut [u8; 32]) -> Result<(), SupervisedDesktopSupervisorStartError> {
    File::open("/dev/urandom")
        .and_then(|mut source| source.read_exact(bytes))
        .map_err(SupervisedDesktopSupervisorStartError::NonceIo)
}

#[cfg(not(any(windows, all(unix, target_os = "linux", target_arch = "x86_64"))))]
fn fill_session_nonce(_bytes: &mut [u8; 32]) -> Result<(), SupervisedDesktopSupervisorStartError> {
    Err(SupervisedDesktopSupervisorStartError::UnsupportedPlatform)
}

#[derive(Debug, Error)]
pub(super) enum SupervisedDesktopSupervisorStartError {
    #[error("the supervised desktop source receipt is not admissible")]
    SourceAdmission(#[source] SupervisedDesktopSourceAdmissionError),
    #[error("the supervised desktop source receipt does not match the immutable generation")]
    SourceReceiptMismatch,
    #[error("the supervised desktop READY timeout must be non-zero")]
    InvalidReadyTimeout,
    #[error("the supervised desktop product working directory must be absolute")]
    InvalidWorkingDirectory,
    #[error("the supervised desktop process spawn deadline exceeded Instant")]
    SpawnDeadlineOverflow,
    #[error("the supervised desktop launch context is invalid")]
    Wire(#[source] SupervisedDesktopWireError),
    #[error("the supervised desktop session nonce could not be generated")]
    NonceIo(#[source] io::Error),
    #[cfg(not(any(windows, all(unix, target_os = "linux", target_arch = "x86_64"))))]
    #[error("supervised desktop sessions are unsupported on this platform")]
    UnsupportedPlatform,
    #[error("the supervised desktop process spawn could not be dispatched")]
    Spawn(#[source] SupervisedProcessSpawnFailure),
}

#[derive(Debug, Error)]
pub(super) enum SupervisedDesktopSupervisorAdvanceError {
    #[error("the supervised desktop process spawn could not advance")]
    Spawn(#[from] SupervisedProcessSpawnFailure),
    #[error("the supervised desktop process could not advance")]
    Process(#[from] SupervisedProcessError),
    #[error("the supervised desktop supervisor entered an invalid private state")]
    InvalidState,
}

#[derive(Debug, Error)]
pub(super) enum SupervisedDesktopSupervisorFailure {
    #[error("the supervised desktop handshake could not start")]
    HandshakeStart(#[source] SupervisedDesktopHandshakeStartError),
    #[error("the supervised desktop handshake failed: {0:?}")]
    Handshake(SupervisedDesktopHandshakeFailureKind),
    #[error("the supervised desktop child exited before exact READY admission")]
    ExitedBeforeReady,
    #[error("the supervised desktop child exited unsuccessfully with code {0:?}")]
    NonZeroExit(Option<i32>),
    #[error("the supervised desktop supervisor entered an invalid private state")]
    InvalidState,
    #[error("the supervised desktop READY deadline exceeded Instant")]
    ReadyDeadlineOverflow,
    #[error("the supervised desktop process could not be spawned")]
    Spawn(#[source] SupervisedProcessSpawnFailure),
    #[error("the supervised desktop process spawn deadline was exceeded")]
    SpawnTimeout,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempt_id_rejects_zero_and_preserves_host_stale_identity() {
        assert_eq!(SupervisedDesktopAttemptId::new(0), None);
        assert_eq!(SupervisedDesktopAttemptId::new(7).unwrap().get(), 7);
        assert_ne!(
            SupervisedDesktopAttemptId::new(7),
            SupervisedDesktopAttemptId::new(8)
        );
    }

    #[test]
    fn cleanup_request_reducer_ignores_stale_without_overwriting_pending_work() {
        let active = SupervisedDesktopAttemptId::new(7).unwrap();
        let stale = SupervisedDesktopAttemptId::new(6).unwrap();
        assert_eq!(
            admit_cleanup_request(active, stale, false, true),
            SupervisedDesktopControlResult::IgnoredStale
        );
    }

    #[test]
    fn restart_is_a_host_supplied_post_cleanup_disposition() {
        let active = SupervisedDesktopAttemptId::new(7).unwrap();
        assert_eq!(
            admit_cleanup_request(active, active, false, false),
            SupervisedDesktopControlResult::Applied
        );
        assert_eq!(
            admit_cleanup_request(active, active, false, true),
            SupervisedDesktopControlResult::Busy
        );
        assert_eq!(SupervisedDesktopAttemptId::new(8).unwrap().get(), 8);
    }
}
