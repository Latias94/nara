//! Private output/history publication comparison. Native submission and epoch validity are
//! explicit caller prerequisites; a ready candidate alone never changes this owner.
//!
//! Registered groups have immutable membership and order. Slot keys and group IDs are never
//! reused within this finite owner. Limits count lifetime registrations, not active entries;
//! they are probe bounds, not a production reclamation policy. The caller owns pending tickets.

use std::sync::Arc;

const MAX_SLOTS: usize = 128;
const MAX_GROUPS: usize = 128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SlotKey {
    pub producer: u64,
    pub view: u64,
    pub instance: u64,
    pub incarnation: u64,
    pub slot: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum HistoryError {
    Limit,
    DuplicateSlot,
    UnknownSlot,
    DuplicateGroup,
    UnknownGroup,
    GroupOverlap,
    MixedProducer,
    Retired,
    EmptyGroup,
    ForeignOwner,
    StaleBasis,
    RepeatedCapture,
    InvalidPosition,
    OutputCount,
    Exhausted,
}

struct Slot<T> {
    key: SlotKey,
    active: bool,
    revision: u64,
    reset: u64,
    last_capture: Option<u64>,
    current: Option<Arc<T>>,
}

struct Group {
    id: u64,
    active: bool,
    slots: Vec<usize>,
}

pub(super) struct History<T> {
    owner: Arc<()>,
    slots: Vec<Slot<T>>,
    groups: Vec<Group>,
}

struct Expected<T> {
    index: usize,
    revision: u64,
    reset: u64,
    previous: Option<Arc<T>>,
}

/// Move-only ticket for a complete registered group, including exact empty-slot stamps.
pub(super) struct Ticket<T> {
    owner: Arc<()>,
    group: usize,
    capture: u64,
    expected: Vec<Expected<T>>,
}

impl<T> Ticket<T> {
    /// Positions follow immutable registration order. Missing is not empty history.
    pub fn previous(&self, index: usize) -> Result<Option<&Arc<T>>, HistoryError> {
        self.expected
            .get(index)
            .map(|entry| entry.previous.as_ref())
            .ok_or(HistoryError::InvalidPosition)
    }
}

/// Old values retained after an already committed mutation. Dropping this value performs cleanup;
/// it cannot undo publication, reset, or retirement, even if a user destructor panics.
/// This is not GPU completion evidence; native submission custody remains independent.
#[must_use = "the mutation committed; release retired values after recording that outcome"]
pub(super) struct RetiredValues<T> {
    values: Vec<Arc<T>>,
}

impl<T> RetiredValues<T> {
    pub fn len(&self) -> usize {
        self.values.len()
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

impl<T> History<T> {
    pub fn new() -> Self {
        Self {
            owner: Arc::new(()),
            slots: Vec::new(),
            groups: Vec::new(),
        }
    }

    pub fn register(&mut self, key: SlotKey) -> Result<(), HistoryError> {
        if self.slots.iter().any(|slot| slot.key == key) {
            return Err(HistoryError::DuplicateSlot);
        }
        if self.slots.len() == MAX_SLOTS {
            return Err(HistoryError::Limit);
        }
        self.slots.push(Slot {
            key,
            active: true,
            revision: 0,
            reset: 0,
            last_capture: None,
            current: None,
        });
        Ok(())
    }

    /// Register one nonreused group incarnation with fixed order and one producer's capture clock.
    /// Active groups cannot share slots. Retire a group before explicitly regrouping survivors.
    pub fn register_group(&mut self, group: u64, keys: &[SlotKey]) -> Result<(), HistoryError> {
        if self.groups.iter().any(|entry| entry.id == group) {
            return Err(HistoryError::DuplicateGroup);
        }
        if self.groups.len() == MAX_GROUPS {
            return Err(HistoryError::Limit);
        }
        self.validate_keys(keys)?;
        let producer = keys[0].producer;
        let mut indices = Vec::with_capacity(keys.len());
        for key in keys {
            let index = self.index(*key)?;
            if !self.slots[index].active {
                return Err(HistoryError::Retired);
            }
            if key.producer != producer {
                return Err(HistoryError::MixedProducer);
            }
            if self
                .groups
                .iter()
                .any(|entry| entry.active && entry.slots.contains(&index))
            {
                return Err(HistoryError::GroupOverlap);
            }
            indices.push(index);
        }
        self.groups.push(Group {
            id: group,
            active: true,
            slots: indices,
        });
        Ok(())
    }

    pub fn prepare(&self, capture: u64, group: u64) -> Result<Ticket<T>, HistoryError> {
        let group_index = self.group_index(group)?;
        let group = &self.groups[group_index];
        if !group.active {
            return Err(HistoryError::Retired);
        }
        let mut expected = Vec::with_capacity(group.slots.len());
        for &index in &group.slots {
            let slot = &self.slots[index];
            if !slot.active {
                return Err(HistoryError::Retired);
            }
            if slot.last_capture.is_some_and(|last| capture <= last) {
                return Err(HistoryError::RepeatedCapture);
            }
            slot.revision
                .checked_add(1)
                .ok_or(HistoryError::Exhausted)?;
            expected.push(Expected {
                index,
                revision: slot.revision,
                reset: slot.reset,
                previous: slot.current.clone(),
            });
        }
        Ok(Ticket {
            owner: Arc::clone(&self.owner),
            group: group_index,
            capture,
            expected,
        })
    }

    /// Read-only pre-submit check. Publication repeats it; checking does not reserve the basis.
    /// The native caller separately checks its Host/device epoch and submission outcome.
    pub fn validate_ticket(&self, ticket: &Ticket<T>) -> Result<(), HistoryError> {
        if !Arc::ptr_eq(&self.owner, &ticket.owner) {
            return Err(HistoryError::ForeignOwner);
        }
        if !self.groups[ticket.group].active {
            return Err(HistoryError::Retired);
        }
        for entry in &ticket.expected {
            let slot = &self.slots[entry.index];
            if !slot.active {
                return Err(HistoryError::Retired);
            }
            if slot.revision != entry.revision || slot.reset != entry.reset {
                return Err(HistoryError::StaleBasis);
            }
            if slot.last_capture.is_some_and(|last| ticket.capture <= last) {
                return Err(HistoryError::RepeatedCapture);
            }
            slot.revision
                .checked_add(1)
                .ok_or(HistoryError::Exhausted)?;
        }
        Ok(())
    }

    /// Publish every output in registration order after native submission/epoch admission.
    /// Only Ok permits exposing the complete output group. Record that committed outcome before
    /// releasing returned retirement custody; cleanup failure cannot turn it into a failed commit.
    pub fn publish(
        &mut self,
        mut ticket: Ticket<T>,
        outputs: &[Arc<T>],
    ) -> Result<RetiredValues<T>, HistoryError> {
        self.validate_ticket(&ticket)?;
        if ticket.expected.len() != outputs.len() {
            return Err(HistoryError::OutputCount);
        }
        let mut values = Vec::with_capacity(ticket.expected.len());
        // All checks and capacity allocation precede mutation. Keep every predecessor pinned in
        // the borrowed ticket until all pointer updates finish, so no old T is destroyed midway.
        for (entry, output) in ticket.expected.iter().zip(outputs) {
            let slot = &mut self.slots[entry.index];
            slot.revision += 1;
            slot.last_capture = Some(ticket.capture);
            slot.current = Some(Arc::clone(output));
        }
        for entry in &mut ticket.expected {
            if let Some(previous) = entry.previous.take() {
                values.push(previous);
            }
        }
        Ok(RetiredValues { values })
    }

    /// Reset selected slots without changing group membership or clearing capture watermarks.
    /// A fresh group ticket pins unchanged members and the explicitly reset empty slots together.
    pub fn reset(&mut self, keys: &[SlotKey]) -> Result<RetiredValues<T>, HistoryError> {
        self.validate_keys(keys)?;
        let mut indices = Vec::with_capacity(keys.len());
        let mut values = Vec::with_capacity(keys.len());
        for key in keys {
            let index = self.index(*key)?;
            let slot = &self.slots[index];
            if !slot.active {
                return Err(HistoryError::Retired);
            }
            slot.revision
                .checked_add(1)
                .ok_or(HistoryError::Exhausted)?;
            slot.reset.checked_add(1).ok_or(HistoryError::Exhausted)?;
            indices.push(index);
        }
        for index in indices {
            let slot = &mut self.slots[index];
            slot.revision += 1;
            slot.reset += 1;
            if let Some(previous) = slot.current.take() {
                values.push(previous);
            }
            // Keep last_capture: reset is not permission to accumulate one capture again.
        }
        Ok(RetiredValues { values })
    }

    /// Retire this slot and its complete group. Surviving slots keep their own history; admit
    /// their next composition explicitly under a fresh group ID and replacement SlotKey.
    pub fn retire(&mut self, key: SlotKey) -> Result<RetiredValues<T>, HistoryError> {
        let index = self.index(key)?;
        let retired = self.reset(&[key])?;
        self.slots[index].active = false;
        for group in &mut self.groups {
            if group.slots.contains(&index) {
                group.active = false;
            }
        }
        Ok(retired)
    }

    /// Fence every ticket for this incarnation, retaining slots and their capture watermarks.
    pub fn retire_group(&mut self, group: u64) -> Result<(), HistoryError> {
        let index = self.group_index(group)?;
        if !self.groups[index].active {
            return Err(HistoryError::Retired);
        }
        self.groups[index].active = false;
        Ok(())
    }

    pub fn current(&self, key: SlotKey) -> Result<Option<&Arc<T>>, HistoryError> {
        let slot = &self.slots[self.index(key)?];
        if !slot.active {
            return Err(HistoryError::Retired);
        }
        Ok(slot.current.as_ref())
    }

    fn index(&self, key: SlotKey) -> Result<usize, HistoryError> {
        self.slots
            .iter()
            .position(|slot| slot.key == key)
            .ok_or(HistoryError::UnknownSlot)
    }

    fn group_index(&self, group: u64) -> Result<usize, HistoryError> {
        self.groups
            .iter()
            .position(|entry| entry.id == group)
            .ok_or(HistoryError::UnknownGroup)
    }

    fn validate_keys(&self, keys: &[SlotKey]) -> Result<(), HistoryError> {
        if keys.is_empty() {
            return Err(HistoryError::EmptyGroup);
        }
        if keys.len() > MAX_SLOTS {
            return Err(HistoryError::Limit);
        }
        for (index, key) in keys.iter().enumerate() {
            if keys[..index].contains(key) {
                return Err(HistoryError::DuplicateSlot);
            }
            self.index(*key)?;
        }
        Ok(())
    }
}
