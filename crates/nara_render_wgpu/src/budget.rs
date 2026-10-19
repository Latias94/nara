use std::sync::{Arc, Mutex, Weak};

use thiserror::Error;

pub(crate) const MAX_INSTANCE_BUFFER_BYTES_PER_SCOPE: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
#[error("instance buffers require {required} bytes; render-scope maximum is {maximum}")]
pub(crate) struct InstanceBudgetError {
    pub(crate) required: u64,
    pub(crate) maximum: u64,
}

#[derive(Debug, Default)]
pub(crate) struct InstanceBudget {
    state: Arc<BudgetState>,
}

#[derive(Debug, Default)]
struct BudgetState {
    reserved: Mutex<u64>,
}

impl InstanceBudget {
    pub(crate) fn reserve(&self, bytes: u64) -> Result<InstanceReservation, InstanceBudgetError> {
        let mut reserved = self
            .state
            .reserved
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let next = reserved.checked_add(bytes).ok_or(InstanceBudgetError {
            required: u64::MAX,
            maximum: MAX_INSTANCE_BUFFER_BYTES_PER_SCOPE,
        })?;
        if next > MAX_INSTANCE_BUFFER_BYTES_PER_SCOPE {
            return Err(InstanceBudgetError {
                required: next,
                maximum: MAX_INSTANCE_BUFFER_BYTES_PER_SCOPE,
            });
        }
        *reserved = next;
        Ok(InstanceReservation {
            state: Arc::downgrade(&self.state),
            bytes,
            committed: false,
        })
    }

    #[cfg(test)]
    pub(crate) fn reserved(&self) -> u64 {
        *self
            .state
            .reserved
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

#[derive(Debug)]
pub(crate) struct InstanceReservation {
    state: Weak<BudgetState>,
    bytes: u64,
    committed: bool,
}

impl InstanceReservation {
    pub(crate) fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for InstanceReservation {
    fn drop(&mut self) {
        if self.committed || self.bytes == 0 {
            return;
        }
        let Some(state) = self.state.upgrade() else {
            return;
        };
        let mut reserved = state
            .reserved
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        *reserved = reserved.saturating_sub(self.bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservation_drop_refunds_and_commit_keeps_scope_charge() {
        let budget = InstanceBudget::default();
        let first = budget.reserve(40 * 1024 * 1024).unwrap();
        assert!(budget.reserve(40 * 1024 * 1024).is_err());
        drop(first);
        assert_eq!(budget.reserved(), 0);
        let mut committed = budget.reserve(40 * 1024 * 1024).unwrap();
        committed.commit();
        drop(committed);
        assert_eq!(budget.reserved(), 40 * 1024 * 1024);
    }

    #[test]
    fn old_scope_reservation_cannot_subtract_from_a_replaced_scope() {
        let old_scope = InstanceBudget::default();
        let old_reservation = old_scope.reserve(40 * 1024 * 1024).unwrap();
        let new_scope = InstanceBudget::default();
        drop(old_reservation);
        let new_reservation = new_scope
            .reserve(MAX_INSTANCE_BUFFER_BYTES_PER_SCOPE)
            .unwrap();
        assert_eq!(new_scope.reserved(), MAX_INSTANCE_BUFFER_BYTES_PER_SCOPE);
        drop(new_reservation);
    }

    #[test]
    fn checked_add_overflow_rejects_without_changing_the_scope_charge() {
        let budget = InstanceBudget::default();
        let reservation = budget.reserve(1).unwrap();
        assert!(budget.reserve(u64::MAX).is_err());
        assert_eq!(budget.reserved(), 1);
        drop(reservation);
        assert_eq!(budget.reserved(), 0);
    }
}
