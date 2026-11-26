use std::{
    any::type_name,
    fmt,
    panic::{AssertUnwindSafe, catch_unwind},
    time::Duration,
};

use bevy_platform::time::Instant;
use nara_ecs::{Mut, Resource, World, bevy_ecs::component::Mutable};
use thiserror::Error;

use crate::{
    PluginError, PluginHook, PluginId, PluginShutdownContext,
    plugin::{InstalledPlugin, run_hook},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RuntimeCloseParticipantId(&'static str);

impl RuntimeCloseParticipantId {
    pub const fn new(id: &'static str) -> Self {
        Self(id)
    }

    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for RuntimeCloseParticipantId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCloseProgress {
    Pending,
    Complete,
    /// The participant finished closing and reported a teardown error.
    CompleteWithError {
        code: &'static str,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCloseErrorDisposition {
    Retryable,
    Terminal,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
#[error("runtime close participant failed with code {code}")]
pub struct RuntimeCloseParticipantError {
    code: &'static str,
    disposition: RuntimeCloseErrorDisposition,
}

impl RuntimeCloseParticipantError {
    pub const fn new(code: &'static str) -> Self {
        Self::retryable(code)
    }

    pub const fn retryable(code: &'static str) -> Self {
        Self {
            code,
            disposition: RuntimeCloseErrorDisposition::Retryable,
        }
    }

    pub const fn terminal(code: &'static str) -> Self {
        Self {
            code,
            disposition: RuntimeCloseErrorDisposition::Terminal,
        }
    }

    pub const fn code(self) -> &'static str {
        self.code
    }

    pub const fn disposition(self) -> RuntimeCloseErrorDisposition {
        self.disposition
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeWorldAccessError {
    #[error("runtime resource {type_name} is missing")]
    MissingResource { type_name: &'static str },
}

pub struct RuntimeCloseContext<'a> {
    world: &'a mut World,
}

impl RuntimeCloseContext<'_> {
    pub fn world(&self) -> &World {
        self.world
    }

    pub fn with_world_mut<R>(&mut self, operation: impl FnOnce(&mut World) -> R) -> R {
        operation(self.world)
    }

    pub fn get_resource_mut<R: Resource<Mutability = Mutable>>(
        &mut self,
    ) -> Result<Option<Mut<'_, R>>, RuntimeWorldAccessError> {
        Ok(self.world.get_resource_mut())
    }

    pub fn resource_mut<R: Resource<Mutability = Mutable>>(
        &mut self,
    ) -> Result<Mut<'_, R>, RuntimeWorldAccessError> {
        self.world
            .get_resource_mut()
            .ok_or(RuntimeWorldAccessError::MissingResource {
                type_name: type_name::<R>(),
            })
    }
}

/// Local teardown with explicit ownership until native work has finished.
/// Participants are driven in reverse registration order. Register dependent resources as one
/// participant when a dependency must remain usable through the entire close.
pub trait RuntimeCloseParticipant: 'static {
    fn begin_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError>;
    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeClosePolicy(Duration);

impl Default for RuntimeClosePolicy {
    fn default() -> Self {
        Self(Duration::from_secs(5))
    }
}

impl RuntimeClosePolicy {
    pub const fn new(timeout: Duration) -> Self {
        Self(timeout)
    }

    pub const fn timeout(self) -> Duration {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCloseParticipantPhase {
    Begin,
    Poll,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuntimeCloseCause {
    PluginShutdown {
        plugin: PluginId,
        error: PluginError,
    },
    ParticipantError {
        participant: RuntimeCloseParticipantId,
        phase: RuntimeCloseParticipantPhase,
        code: &'static str,
        disposition: RuntimeCloseErrorDisposition,
    },
    DeadlineExceeded,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeClosePendingParticipant {
    participant: RuntimeCloseParticipantId,
    phase: RuntimeCloseParticipantPhase,
}

impl RuntimeClosePendingParticipant {
    pub const fn participant(self) -> RuntimeCloseParticipantId {
        self.participant
    }

    pub const fn phase(self) -> RuntimeCloseParticipantPhase {
        self.phase
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RuntimeCloseEvidence {
    causes: Vec<RuntimeCloseCause>,
    pending: Vec<RuntimeClosePendingParticipant>,
}

impl RuntimeCloseEvidence {
    pub fn causes(&self) -> &[RuntimeCloseCause] {
        &self.causes
    }

    pub fn pending_participants(&self) -> &[RuntimeClosePendingParticipant] {
        &self.pending
    }

    pub fn is_empty(&self) -> bool {
        self.causes.is_empty() && self.pending.is_empty()
    }

    pub fn plugin_shutdown_failed(&self) -> bool {
        self.causes
            .iter()
            .any(|cause| matches!(cause, RuntimeCloseCause::PluginShutdown { .. }))
    }

    pub fn has_terminal_failure(&self) -> bool {
        self.causes.iter().any(|cause| {
            matches!(
                cause,
                RuntimeCloseCause::PluginShutdown { .. }
                    | RuntimeCloseCause::ParticipantError {
                        disposition: RuntimeCloseErrorDisposition::Terminal,
                        ..
                    }
            )
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeCandidateRetirementState {
    Retiring,
    RetirementIncomplete,
    Retired,
}

#[derive(Default)]
enum CloseState {
    #[default]
    Open,
    Closing(Instant),
    Incomplete,
    Closed,
}

struct CloseEntry {
    participant: Box<dyn RuntimeCloseParticipant>,
    id: RuntimeCloseParticipantId,
    phase: RuntimeCloseParticipantPhase,
    finished: bool,
    terminal_error: bool,
}

#[derive(Default)]
pub(crate) struct Close {
    state: CloseState,
    shutdown_invoked: bool,
    participants: Vec<CloseEntry>,
    pub(crate) evidence: RuntimeCloseEvidence,
    pub(crate) policy: RuntimeClosePolicy,
}

impl Close {
    pub(crate) fn register(
        &mut self,
        id: RuntimeCloseParticipantId,
        participant: impl RuntimeCloseParticipant,
    ) -> Result<(), PluginError> {
        if self.participants.iter().any(|entry| entry.id == id) {
            return Err(PluginError::DuplicateCloseParticipant { id });
        }
        self.participants.push(CloseEntry {
            participant: Box::new(participant),
            id,
            phase: RuntimeCloseParticipantPhase::Begin,
            finished: false,
            terminal_error: false,
        });
        Ok(())
    }

    pub(crate) fn is_open(&self) -> bool {
        matches!(self.state, CloseState::Open)
    }

    pub(crate) fn state(&self) -> RuntimeCandidateRetirementState {
        match self.state {
            CloseState::Open | CloseState::Closing(_) => RuntimeCandidateRetirementState::Retiring,
            CloseState::Incomplete => RuntimeCandidateRetirementState::RetirementIncomplete,
            CloseState::Closed => RuntimeCandidateRetirementState::Retired,
        }
    }

    pub(crate) fn begin(&mut self) {
        if matches!(self.state, CloseState::Open | CloseState::Incomplete) {
            let now = Instant::now();
            self.state = CloseState::Closing(now.checked_add(self.policy.timeout()).unwrap_or(now));
            self.observe_pending();
        }
    }

    pub(crate) fn drive(&mut self, world: &mut World, plugins: &[InstalledPlugin]) {
        let CloseState::Closing(deadline) = self.state else {
            return;
        };
        let shutdown_pending = !std::mem::replace(&mut self.shutdown_invoked, true);
        for installed in plugins.iter().rev().filter(|_| shutdown_pending) {
            let plugin = installed.entry.plugin_id();
            let result = run_hook(plugin, PluginHook::Shutdown, || {
                installed
                    .plugin
                    .shutdown(&mut PluginShutdownContext { world })
            });
            if let Err(error) = result {
                self.evidence
                    .causes
                    .push(RuntimeCloseCause::PluginShutdown { plugin, error });
            }
        }

        let mut failed = false;
        for entry in self
            .participants
            .iter_mut()
            .rev()
            .filter(|entry| !entry.finished)
        {
            if entry.terminal_error {
                failed = true;
                continue;
            }
            let mut context = RuntimeCloseContext { world };
            loop {
                let result = catch_unwind(AssertUnwindSafe(|| match entry.phase {
                    RuntimeCloseParticipantPhase::Begin => {
                        entry.participant.begin_close(&mut context)
                    }
                    RuntimeCloseParticipantPhase::Poll => {
                        entry.participant.poll_close(&mut context)
                    }
                }))
                .unwrap_or(Err(RuntimeCloseParticipantError::terminal(
                    "close-participant-panicked",
                )));
                let error = match result {
                    Ok(RuntimeCloseProgress::Complete) => {
                        entry.finished = true;
                        break;
                    }
                    Ok(RuntimeCloseProgress::Pending)
                        if entry.phase == RuntimeCloseParticipantPhase::Begin =>
                    {
                        entry.phase = RuntimeCloseParticipantPhase::Poll;
                        continue;
                    }
                    Ok(RuntimeCloseProgress::Pending) => break,
                    Ok(RuntimeCloseProgress::CompleteWithError { code }) => {
                        entry.finished = true;
                        RuntimeCloseParticipantError::terminal(code)
                    }
                    Err(error) => {
                        entry.terminal_error =
                            error.disposition() == RuntimeCloseErrorDisposition::Terminal;
                        failed = true;
                        error
                    }
                };
                let cause = RuntimeCloseCause::ParticipantError {
                    participant: entry.id,
                    phase: entry.phase,
                    code: error.code(),
                    disposition: error.disposition(),
                };
                if !self.evidence.causes.contains(&cause) {
                    self.evidence.causes.push(cause);
                }
                break;
            }
        }
        self.observe_pending();
        if self.evidence.pending.is_empty() {
            self.state = CloseState::Closed;
        } else if Instant::now() >= deadline {
            if !self
                .evidence
                .causes
                .contains(&RuntimeCloseCause::DeadlineExceeded)
            {
                self.evidence
                    .causes
                    .push(RuntimeCloseCause::DeadlineExceeded);
            }
            self.state = CloseState::Incomplete;
        } else if failed {
            self.state = CloseState::Incomplete;
        }
    }

    fn observe_pending(&mut self) {
        self.evidence.pending.clear();
        self.evidence.pending.extend(
            self.participants
                .iter()
                .filter(|entry| !entry.finished)
                .map(|entry| RuntimeClosePendingParticipant {
                    participant: entry.id,
                    phase: entry.phase,
                }),
        );
    }

    pub(crate) fn finish(&mut self, world: &mut World, plugins: &[InstalledPlugin]) {
        self.begin();
        self.drive(world, plugins);
        #[cfg(not(target_arch = "wasm32"))]
        while let CloseState::Closing(deadline) = self.state {
            let remaining = deadline.saturating_duration_since(Instant::now());
            std::thread::sleep(remaining.min(Duration::from_millis(1)));
            self.drive(world, plugins);
        }
    }
}
