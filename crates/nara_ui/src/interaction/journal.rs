use std::{collections::BTreeMap, fmt};

use nara_ecs::Resource;
use nara_ui_kernel::{
    UiActionId, UiActivationSource, UiKey, UiKeyError, UiOpaqueId, UiRouteStamp, UiSurfaceId,
};
use thiserror::Error;

pub const MAX_UI_ACTION_JOURNAL_ENTRIES: usize = 256;
pub const MAX_UI_ACTION_CONSUMERS: usize = 16;

/// Stable identity for one semantic UI-action consumer.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiActionConsumerId(UiOpaqueId);

impl UiActionConsumerId {
    pub fn new(value: impl Into<String>) -> Result<Self, UiKeyError> {
        Ok(Self(UiOpaqueId::new(value)?))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl fmt::Display for UiActionConsumerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Non-forgeable registration cursor for one journal consumer lifetime.
#[derive(Debug, PartialEq, Eq)]
pub struct UiActionCursor {
    consumer: UiActionConsumerId,
    registration: u64,
}

impl UiActionCursor {
    #[must_use]
    pub const fn consumer(&self) -> &UiActionConsumerId {
        &self.consumer
    }
}

/// Monotonic identity assigned to one admitted semantic action.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UiActionSequence(u64);

impl UiActionSequence {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// One typed semantic action admitted from an exact presented UI session.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UiActionEvent {
    sequence: UiActionSequence,
    input_sequence: Option<u64>,
    route: UiRouteStamp,
    frame_revision: u64,
    target: UiKey,
    action: UiActionId,
    source: UiActivationSource,
}

impl UiActionEvent {
    #[must_use]
    pub const fn sequence(&self) -> UiActionSequence {
        self.sequence
    }

    #[must_use]
    pub const fn input_sequence(&self) -> Option<u64> {
        self.input_sequence
    }

    #[must_use]
    pub const fn surface_id(&self) -> UiSurfaceId {
        self.route.surface_id()
    }

    #[must_use]
    pub const fn presentation_epoch(&self) -> u64 {
        self.route.presentation_epoch()
    }

    #[must_use]
    pub const fn frame_revision(&self) -> u64 {
        self.frame_revision
    }

    #[must_use]
    pub const fn target(&self) -> &UiKey {
        &self.target
    }

    #[must_use]
    pub const fn action(&self) -> &UiActionId {
        &self.action
    }

    #[must_use]
    pub const fn source(&self) -> UiActivationSource {
        self.source
    }
}

/// Transaction-local action value awaiting journal sequence assignment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct UiActionDraft {
    input_sequence: Option<u64>,
    route: UiRouteStamp,
    frame_revision: u64,
    target: UiKey,
    action: UiActionId,
    source: UiActivationSource,
}

impl UiActionDraft {
    #[must_use]
    pub(crate) const fn new(
        input_sequence: Option<u64>,
        route: UiRouteStamp,
        frame_revision: u64,
        target: UiKey,
        action: UiActionId,
        source: UiActivationSource,
    ) -> Self {
        Self {
            input_sequence,
            route,
            frame_revision,
            target,
            action,
            source,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConsumerState {
    registration: u64,
    acknowledged: u64,
}

/// Bounded semantic-action storage with explicit consumer retention obligations.
#[derive(Clone, Debug, Resource)]
pub struct UiActionJournal {
    entries: Vec<UiActionEvent>,
    consumers: BTreeMap<UiActionConsumerId, ConsumerState>,
    tail_sequence: u64,
    next_sequence: Option<u64>,
    next_registration: Option<u64>,
    max_entries: usize,
    max_consumers: usize,
}

impl Default for UiActionJournal {
    fn default() -> Self {
        Self::with_limits(MAX_UI_ACTION_JOURNAL_ENTRIES, MAX_UI_ACTION_CONSUMERS)
            .expect("the built-in UI action journal limits are non-zero")
    }
}

impl UiActionJournal {
    pub(crate) fn with_limits(
        max_entries: usize,
        max_consumers: usize,
    ) -> Result<Self, UiActionJournalConfigError> {
        if max_entries == 0 {
            return Err(UiActionJournalConfigError::ZeroEntryLimit);
        }
        if max_consumers == 0 {
            return Err(UiActionJournalConfigError::ZeroConsumerLimit);
        }
        Ok(Self {
            entries: Vec::new(),
            consumers: BTreeMap::new(),
            tail_sequence: 0,
            next_sequence: Some(1),
            next_registration: Some(1),
            max_entries,
            max_consumers,
        })
    }

    /// Registers one retention obligation at the current tail without replaying older entries.
    pub fn register_consumer(
        &mut self,
        consumer: UiActionConsumerId,
    ) -> Result<UiActionCursor, UiActionConsumerError> {
        if self.consumers.contains_key(&consumer) {
            return Err(UiActionConsumerError::AlreadyRegistered { consumer });
        }
        if self.consumers.len() == self.max_consumers {
            return Err(UiActionConsumerError::ConsumerLimitExceeded {
                maximum: self.max_consumers,
            });
        }
        let registration = self
            .next_registration
            .ok_or(UiActionConsumerError::RegistrationSequenceExhausted)?;
        self.next_registration = registration.checked_add(1);
        self.consumers.insert(
            consumer.clone(),
            ConsumerState {
                registration,
                acknowledged: self.tail_sequence,
            },
        );
        Ok(UiActionCursor {
            consumer,
            registration,
        })
    }

    /// Returns actions not yet acknowledged by this registered consumer.
    pub fn pending(
        &self,
        cursor: &UiActionCursor,
    ) -> Result<&[UiActionEvent], UiActionConsumerError> {
        let state = self.consumer_state(cursor)?;
        let start = self
            .entries
            .partition_point(|event| event.sequence.get() <= state.acknowledged);
        Ok(&self.entries[start..])
    }

    /// Advances one consumer and reclaims only the prefix safe for every consumer.
    pub fn acknowledge(
        &mut self,
        cursor: &UiActionCursor,
        sequence: UiActionSequence,
    ) -> Result<(), UiActionConsumerError> {
        let tail_sequence = self.tail_sequence;
        let state = self.consumer_state_mut(cursor)?;
        if sequence.get() < state.acknowledged {
            return Err(UiActionConsumerError::NonMonotonicAcknowledgement {
                consumer: cursor.consumer.clone(),
                current: state.acknowledged,
                candidate: sequence.get(),
            });
        }
        if sequence.get() > tail_sequence {
            return Err(UiActionConsumerError::AcknowledgementBeyondTail {
                consumer: cursor.consumer.clone(),
                tail: tail_sequence,
                candidate: sequence.get(),
            });
        }
        state.acknowledged = sequence.get();
        self.reclaim_acknowledged_prefix();
        Ok(())
    }

    /// Releases one consumer's retention obligation without affecting other consumers.
    pub fn unregister_consumer(
        &mut self,
        cursor: &UiActionCursor,
    ) -> Result<(), UiActionConsumerError> {
        self.consumer_state(cursor)?;
        self.consumers.remove(&cursor.consumer);
        self.reclaim_acknowledged_prefix();
        Ok(())
    }

    #[must_use]
    pub fn retained_len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn consumer_count(&self) -> usize {
        self.consumers.len()
    }

    pub(crate) fn admit(
        &mut self,
        drafts: &[UiActionDraft],
    ) -> Result<Option<UiActionSequence>, UiActionAdmissionError> {
        if drafts.is_empty() {
            return Ok(None);
        }
        for draft in drafts {
            if draft.route.presentation_epoch() == 0 {
                return Err(UiActionAdmissionError::ZeroPresentationEpoch);
            }
            if draft.input_sequence == Some(0) {
                return Err(UiActionAdmissionError::ZeroInputSequence);
            }
            if draft.frame_revision == 0 {
                return Err(UiActionAdmissionError::ZeroFrameRevision);
            }
        }
        if !self.consumers.is_empty()
            && drafts.len() > self.max_entries.saturating_sub(self.entries.len())
        {
            return Err(UiActionAdmissionError::CapacityExceeded {
                maximum: self.max_entries,
                retained: self.entries.len(),
                attempted: drafts.len(),
            });
        }

        let first_sequence = self
            .next_sequence
            .ok_or(UiActionAdmissionError::SequenceExhausted)?;
        let draft_count =
            u64::try_from(drafts.len()).map_err(|_| UiActionAdmissionError::SequenceExhausted)?;
        let last_sequence = first_sequence
            .checked_add(draft_count - 1)
            .ok_or(UiActionAdmissionError::SequenceExhausted)?;

        if !self.consumers.is_empty() {
            self.entries.reserve(drafts.len());
            self.entries
                .extend(
                    (first_sequence..=last_sequence)
                        .zip(drafts)
                        .map(|(sequence, draft)| UiActionEvent {
                            sequence: UiActionSequence(sequence),
                            input_sequence: draft.input_sequence,
                            route: draft.route,
                            frame_revision: draft.frame_revision,
                            target: draft.target.clone(),
                            action: draft.action.clone(),
                            source: draft.source,
                        }),
                );
        }
        self.tail_sequence = last_sequence;
        self.next_sequence = last_sequence.checked_add(1);
        Ok(Some(UiActionSequence(last_sequence)))
    }

    fn consumer_state(
        &self,
        cursor: &UiActionCursor,
    ) -> Result<&ConsumerState, UiActionConsumerError> {
        let state = self.consumers.get(&cursor.consumer).ok_or_else(|| {
            UiActionConsumerError::UnknownConsumer {
                consumer: cursor.consumer.clone(),
            }
        })?;
        if state.registration != cursor.registration {
            return Err(UiActionConsumerError::StaleCursor {
                consumer: cursor.consumer.clone(),
            });
        }
        Ok(state)
    }

    fn consumer_state_mut(
        &mut self,
        cursor: &UiActionCursor,
    ) -> Result<&mut ConsumerState, UiActionConsumerError> {
        let state = self.consumers.get_mut(&cursor.consumer).ok_or_else(|| {
            UiActionConsumerError::UnknownConsumer {
                consumer: cursor.consumer.clone(),
            }
        })?;
        if state.registration != cursor.registration {
            return Err(UiActionConsumerError::StaleCursor {
                consumer: cursor.consumer.clone(),
            });
        }
        Ok(state)
    }

    fn reclaim_acknowledged_prefix(&mut self) {
        let safe_sequence = self
            .consumers
            .values()
            .map(|state| state.acknowledged)
            .min()
            .unwrap_or(self.tail_sequence);
        let retained_start = self
            .entries
            .partition_point(|event| event.sequence.get() <= safe_sequence);
        self.entries.drain(..retained_start);
    }
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum UiActionJournalConfigError {
    #[error("UI action journal entry limit must be non-zero")]
    ZeroEntryLimit,
    #[error("UI action journal consumer limit must be non-zero")]
    ZeroConsumerLimit,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum UiActionConsumerError {
    #[error("UI action consumer {consumer} is already registered")]
    AlreadyRegistered { consumer: UiActionConsumerId },
    #[error("UI action consumer limit {maximum} was exceeded")]
    ConsumerLimitExceeded { maximum: usize },
    #[error("UI action consumer registration sequence was exhausted")]
    RegistrationSequenceExhausted,
    #[error("UI action consumer {consumer} is not registered")]
    UnknownConsumer { consumer: UiActionConsumerId },
    #[error("UI action consumer {consumer} used a stale cursor")]
    StaleCursor { consumer: UiActionConsumerId },
    #[error(
        "UI action consumer {consumer} acknowledgement moved backward from {current} to {candidate}"
    )]
    NonMonotonicAcknowledgement {
        consumer: UiActionConsumerId,
        current: u64,
        candidate: u64,
    },
    #[error(
        "UI action consumer {consumer} acknowledged sequence {candidate} beyond journal tail {tail}"
    )]
    AcknowledgementBeyondTail {
        consumer: UiActionConsumerId,
        tail: u64,
        candidate: u64,
    },
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum UiActionAdmissionError {
    #[error(
        "UI action journal limit {maximum} rejected {attempted} new entries while retaining {retained}"
    )]
    CapacityExceeded {
        maximum: usize,
        retained: usize,
        attempted: usize,
    },
    #[error("UI action journal sequence was exhausted")]
    SequenceExhausted,
    #[error("UI action draft used input sequence zero")]
    ZeroInputSequence,
    #[error("UI action draft used presentation epoch zero")]
    ZeroPresentationEpoch,
    #[error("UI action draft used frame revision zero")]
    ZeroFrameRevision,
}

#[cfg(test)]
mod tests {
    use nara_ui_kernel::{
        UiActionId, UiActivationSource, UiLocalKey, UiRootKey, UiRouteStamp, UiSurfaceId,
    };

    use super::{
        UiActionAdmissionError, UiActionConsumerError, UiActionConsumerId, UiActionDraft,
        UiActionJournal,
    };

    fn action(name: &str) -> UiActionDraft {
        let surface = UiSurfaceId::new(1).unwrap();
        let root = UiRootKey::runtime("journal-test", "root").unwrap();
        let key =
            nara_ui_kernel::UiKey::scoped(root, UiLocalKey::runtime("journal-test", name).unwrap())
                .unwrap();
        UiActionDraft::new(
            Some(7),
            UiRouteStamp::new(surface, 3),
            5,
            key,
            UiActionId::new(name).unwrap(),
            UiActivationSource::Keyboard,
        )
    }

    fn consumer(name: &str) -> UiActionConsumerId {
        UiActionConsumerId::new(name).unwrap()
    }

    #[test]
    fn late_registration_starts_at_the_current_tail_without_replay() {
        let mut journal = UiActionJournal::with_limits(4, 2).unwrap();
        assert_eq!(
            journal.admit(&[action("before")]).unwrap().unwrap().get(),
            1
        );

        let cursor = journal.register_consumer(consumer("late")).unwrap();
        assert!(journal.pending(&cursor).unwrap().is_empty());

        journal.admit(&[action("after")]).unwrap();
        assert_eq!(journal.pending(&cursor).unwrap().len(), 1);
        assert_eq!(journal.pending(&cursor).unwrap()[0].sequence().get(), 2);
    }

    #[test]
    fn acknowledgement_reclaims_only_the_globally_safe_prefix() {
        let mut journal = UiActionJournal::with_limits(4, 2).unwrap();
        let first = journal.register_consumer(consumer("first")).unwrap();
        let second = journal.register_consumer(consumer("second")).unwrap();
        journal.admit(&[action("one"), action("two")]).unwrap();

        let first_tail = journal.pending(&first).unwrap()[1].sequence();
        journal.acknowledge(&first, first_tail).unwrap();
        assert_eq!(journal.retained_len(), 2);

        let second_first = journal.pending(&second).unwrap()[0].sequence();
        journal.acknowledge(&second, second_first).unwrap();
        assert_eq!(journal.retained_len(), 1);
        assert!(journal.pending(&first).unwrap().is_empty());
        assert_eq!(journal.pending(&second).unwrap().len(), 1);
    }

    #[test]
    fn unregister_releases_only_that_consumers_retention_obligation() {
        let mut journal = UiActionJournal::with_limits(4, 2).unwrap();
        let slow = journal.register_consumer(consumer("slow")).unwrap();
        let fast = journal.register_consumer(consumer("fast")).unwrap();
        journal.admit(&[action("one")]).unwrap();
        let tail = journal.pending(&fast).unwrap()[0].sequence();
        journal.acknowledge(&fast, tail).unwrap();
        assert_eq!(journal.retained_len(), 1);

        journal.unregister_consumer(&slow).unwrap();
        assert_eq!(journal.retained_len(), 0);
        assert_eq!(journal.consumer_count(), 1);
    }

    #[test]
    fn slow_consumer_causes_typed_backpressure_without_partial_admission() {
        let mut journal = UiActionJournal::with_limits(2, 1).unwrap();
        let cursor = journal.register_consumer(consumer("slow")).unwrap();
        journal.admit(&[action("one"), action("two")]).unwrap();

        assert_eq!(
            journal.admit(&[action("three")]),
            Err(UiActionAdmissionError::CapacityExceeded {
                maximum: 2,
                retained: 2,
                attempted: 1,
            })
        );
        let pending = journal.pending(&cursor).unwrap();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].sequence().get(), 1);
        assert_eq!(pending[1].sequence().get(), 2);
    }

    #[test]
    fn stale_cursor_cannot_control_a_re_registered_consumer() {
        let mut journal = UiActionJournal::with_limits(2, 1).unwrap();
        let id = consumer("reused");
        let stale = journal.register_consumer(id.clone()).unwrap();
        journal.unregister_consumer(&stale).unwrap();
        let current = journal.register_consumer(id.clone()).unwrap();

        assert_eq!(
            journal.pending(&stale),
            Err(UiActionConsumerError::StaleCursor { consumer: id })
        );
        assert!(journal.pending(&current).unwrap().is_empty());
    }

    #[test]
    fn no_consumers_advance_the_tail_without_retaining_entries() {
        let mut journal = UiActionJournal::with_limits(1, 1).unwrap();
        journal
            .admit(&[action("one"), action("two"), action("three")])
            .unwrap();
        assert_eq!(journal.retained_len(), 0);

        let cursor = journal.register_consumer(consumer("late")).unwrap();
        journal.admit(&[action("four")]).unwrap();
        let pending = journal.pending(&cursor).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].sequence().get(), 4);
    }
}
