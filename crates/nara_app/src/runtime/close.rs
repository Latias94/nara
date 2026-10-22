//! Runtime close state and obligation ledger.
//!
//! The public close protocol remains defined in `runtime.rs`; this module owns the
//! implementation state used while an owner is being retired.

use std::{
    collections::BTreeSet,
    fmt::{self, Debug, Display, Formatter},
    time::Instant,
};

use nara_ecs::World;
use thiserror::Error;

use super::{
    RuntimeCloseCause, RuntimeCloseContext, RuntimeCloseErrorDisposition, RuntimeCloseEvidence,
    RuntimeCloseParticipant, RuntimeCloseParticipantId, RuntimeCloseParticipantPhase,
    RuntimeCloseProgress,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PluginShutdownState {
    Pending,
    Complete,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct OwnerCloseAttempt {
    pub(super) deadline: Instant,
    pub(super) plugin_shutdown: PluginShutdownState,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum OwnerCloseState {
    Open,
    Closing(OwnerCloseAttempt),
    Incomplete {
        plugin_shutdown: PluginShutdownState,
    },
    Complete,
}

struct RuntimeCloseEntry {
    id: RuntimeCloseParticipantId,
    participant: Box<dyn RuntimeCloseParticipant>,
    begun: bool,
    phase: RuntimeCloseParticipantPhase,
    complete: bool,
    terminal_failure: bool,
}

/// Current unfinished physical responsibility, separate from historical close errors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeClosePendingParticipant {
    participant: RuntimeCloseParticipantId,
    phase: RuntimeCloseParticipantPhase,
}

impl RuntimeClosePendingParticipant {
    #[must_use]
    pub const fn participant(self) -> RuntimeCloseParticipantId {
        self.participant
    }

    /// The unfinished phase. `Begin` also covers a participant not yet invoked.
    #[must_use]
    pub const fn phase(self) -> RuntimeCloseParticipantPhase {
        self.phase
    }
}

pub struct RuntimeObligationLedger {
    ids: BTreeSet<RuntimeCloseParticipantId>,
    entries: Vec<RuntimeCloseEntry>,
}

impl Default for RuntimeObligationLedger {
    fn default() -> Self {
        Self::new()
    }
}

impl Debug for RuntimeObligationLedger {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeObligationLedger")
            .field("ids", &self.ids)
            .field("entries", &self.entries.len())
            .finish()
    }
}

impl RuntimeObligationLedger {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            ids: BTreeSet::new(),
            entries: Vec::new(),
        }
    }

    pub fn register<P>(
        &mut self,
        id: RuntimeCloseParticipantId,
        participant: P,
    ) -> Result<(), RuntimeObligationRegistrationError<P>>
    where
        P: RuntimeCloseParticipant,
    {
        if !self.ids.insert(id) {
            return Err(RuntimeObligationRegistrationError {
                error: RuntimeObligationLedgerError::Duplicate { id },
                participant,
            });
        }
        self.entries.push(RuntimeCloseEntry {
            id,
            participant: Box::new(participant),
            begun: false,
            phase: RuntimeCloseParticipantPhase::Begin,
            complete: false,
            terminal_failure: false,
        });
        Ok(())
    }

    pub(super) fn preflight_append(
        &self,
        other: &Self,
    ) -> Result<(), RuntimeObligationLedgerError> {
        if let Some(id) = self.ids.intersection(&other.ids).next().copied() {
            return Err(RuntimeObligationLedgerError::Duplicate { id });
        }
        Ok(())
    }

    pub(super) fn append_for_retirement(&mut self, mut other: Self) {
        self.ids.append(&mut other.ids);
        self.entries.append(&mut other.entries);
    }

    pub(crate) fn retain_for_retirement<P>(&mut self, id: RuntimeCloseParticipantId, participant: P)
    where
        P: RuntimeCloseParticipant,
    {
        self.entries.push(RuntimeCloseEntry {
            id,
            participant: Box::new(participant),
            begun: false,
            phase: RuntimeCloseParticipantPhase::Begin,
            complete: false,
            terminal_failure: false,
        });
    }

    pub(super) fn drive_close_once_with_evidence(
        &mut self,
        world: &mut World,
        evidence: &mut RuntimeCloseEvidence,
    ) -> bool {
        let mut failed = false;
        for entry in self.entries.iter_mut().rev() {
            if entry.complete || entry.terminal_failure {
                continue;
            }
            let mut context = RuntimeCloseContext { world };
            if !entry.begun {
                entry.begun = true;
                match entry.participant.begin_close(&mut context) {
                    Ok(RuntimeCloseProgress::Complete) => {
                        entry.complete = true;
                        continue;
                    }
                    Ok(RuntimeCloseProgress::CompleteWithError { code }) => {
                        entry.complete = true;
                        evidence.record(RuntimeCloseCause::ParticipantError {
                            participant: entry.id,
                            phase: RuntimeCloseParticipantPhase::Begin,
                            code,
                            disposition: RuntimeCloseErrorDisposition::Terminal,
                        });
                        continue;
                    }
                    Ok(RuntimeCloseProgress::Pending) => {}
                    Err(error) => {
                        evidence.record(RuntimeCloseCause::ParticipantError {
                            participant: entry.id,
                            phase: RuntimeCloseParticipantPhase::Begin,
                            code: error.code(),
                            disposition: error.disposition(),
                        });
                        entry.terminal_failure =
                            error.disposition() == RuntimeCloseErrorDisposition::Terminal;
                        if !entry.terminal_failure {
                            entry.begun = false;
                        }
                        failed = true;
                        continue;
                    }
                }
            }
            entry.phase = RuntimeCloseParticipantPhase::Poll;
            match entry.participant.poll_close(&mut context) {
                Ok(RuntimeCloseProgress::Complete) => {
                    entry.complete = true;
                }
                Ok(RuntimeCloseProgress::CompleteWithError { code }) => {
                    entry.complete = true;
                    evidence.record(RuntimeCloseCause::ParticipantError {
                        participant: entry.id,
                        phase: RuntimeCloseParticipantPhase::Poll,
                        code,
                        disposition: RuntimeCloseErrorDisposition::Terminal,
                    });
                }
                Ok(RuntimeCloseProgress::Pending) => {}
                Err(error) => {
                    evidence.record(RuntimeCloseCause::ParticipantError {
                        participant: entry.id,
                        phase: RuntimeCloseParticipantPhase::Poll,
                        code: error.code(),
                        disposition: error.disposition(),
                    });
                    entry.terminal_failure =
                        error.disposition() == RuntimeCloseErrorDisposition::Terminal;
                    failed = true;
                }
            }
        }
        failed
    }

    pub(super) fn update_pending_evidence(&self, evidence: &mut RuntimeCloseEvidence) {
        evidence.pending_participants.clear();
        evidence.pending_participants.extend(
            self.entries
                .iter()
                .filter(|entry| !entry.complete)
                .map(|entry| RuntimeClosePendingParticipant {
                    participant: entry.id,
                    phase: entry.phase,
                }),
        );
    }

    pub(super) fn is_close_complete(&self) -> bool {
        self.entries.iter().all(|entry| entry.complete)
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum RuntimeObligationLedgerError {
    #[error("runtime close participant {id} is registered more than once")]
    Duplicate { id: RuntimeCloseParticipantId },
}

pub struct RuntimeObligationRegistrationError<P> {
    error: RuntimeObligationLedgerError,
    participant: P,
}

impl<P> Debug for RuntimeObligationRegistrationError<P> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RuntimeObligationRegistrationError")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl<P> Display for RuntimeObligationRegistrationError<P> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        Display::fmt(&self.error, formatter)
    }
}

impl<P> std::error::Error for RuntimeObligationRegistrationError<P> where P: 'static {}

impl<P> RuntimeObligationRegistrationError<P> {
    #[must_use]
    pub const fn error(&self) -> RuntimeObligationLedgerError {
        self.error
    }

    #[must_use]
    pub fn into_parts(self) -> (RuntimeObligationLedgerError, P) {
        (self.error, self.participant)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use nara_ecs::World;

    use super::RuntimeObligationLedger;
    use crate::runtime::{
        RuntimeCloseContext, RuntimeCloseEvidence, RuntimeCloseParticipant,
        RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseParticipantPhase,
        RuntimeCloseProgress,
    };

    struct TerminalParticipant {
        begins: Arc<AtomicUsize>,
        polls: Arc<AtomicUsize>,
    }

    impl RuntimeCloseParticipant for TerminalParticipant {
        fn begin_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            self.begins.fetch_add(1, Ordering::SeqCst);
            Err(RuntimeCloseParticipantError::terminal("terminal-close"))
        }

        fn poll_close(
            &mut self,
            _context: &mut RuntimeCloseContext<'_>,
        ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
            self.polls.fetch_add(1, Ordering::SeqCst);
            Ok(RuntimeCloseProgress::Complete)
        }
    }

    #[test]
    fn terminal_failure_is_not_reported_as_complete_or_driven_again() {
        let begins = Arc::new(AtomicUsize::new(0));
        let polls = Arc::new(AtomicUsize::new(0));
        let mut ledger = RuntimeObligationLedger::new();
        ledger
            .register(
                RuntimeCloseParticipantId::new("nara.test.terminal-ledger"),
                TerminalParticipant {
                    begins: Arc::clone(&begins),
                    polls: Arc::clone(&polls),
                },
            )
            .expect("participant id is unique");
        let mut world = World::new();
        let mut evidence = RuntimeCloseEvidence::default();

        assert!(ledger.drive_close_once_with_evidence(&mut world, &mut evidence));
        assert!(!ledger.is_close_complete());
        assert_eq!(begins.load(Ordering::SeqCst), 1);
        assert_eq!(polls.load(Ordering::SeqCst), 0);
        ledger.update_pending_evidence(&mut evidence);
        assert_eq!(evidence.pending_participants().len(), 1);
        assert_eq!(
            evidence.pending_participants()[0].participant(),
            RuntimeCloseParticipantId::new("nara.test.terminal-ledger")
        );
        assert_eq!(
            evidence.pending_participants()[0].phase(),
            RuntimeCloseParticipantPhase::Begin
        );
        let failed = evidence.clone();
        assert!(!ledger.drive_close_once_with_evidence(&mut world, &mut evidence));
        ledger.update_pending_evidence(&mut evidence);
        assert!(!ledger.is_close_complete());
        assert_eq!(evidence, failed);
        assert_eq!(begins.load(Ordering::SeqCst), 1);
        assert_eq!(polls.load(Ordering::SeqCst), 0);
    }
}
