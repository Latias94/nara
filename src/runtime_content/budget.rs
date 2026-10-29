//! Shared admission for concurrent decodes and retained typed output.

use std::sync::{Arc, Mutex};

use super::{RuntimeContentBudgetKind, RuntimeContentError, budget_error, check};

pub(super) struct DecodeBudget {
    active: Mutex<Active>,
    retained_limit: usize,
    work_limit: usize,
}

struct Active {
    retained: usize,
    work: usize,
}

pub(super) struct DecodeLease {
    owner: Arc<DecodeBudget>,
    retained: usize,
    work: usize,
}

impl DecodeBudget {
    pub(super) fn new(retained: usize, retained_limit: usize, work_limit: usize) -> Arc<Self> {
        Arc::new(Self {
            active: Mutex::new(Active { retained, work: 0 }),
            retained_limit,
            work_limit,
        })
    }

    pub(super) fn reserve(
        self: &Arc<Self>,
        retained: usize,
        work: usize,
    ) -> Result<DecodeLease, RuntimeContentError> {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let next_retained = active.retained.checked_add(retained).ok_or_else(|| {
            budget_error(
                RuntimeContentBudgetKind::RetainedBytes,
                usize::MAX,
                self.retained_limit,
            )
        })?;
        let next_work = active.work.checked_add(work).ok_or_else(|| {
            budget_error(
                RuntimeContentBudgetKind::WorkBytes,
                usize::MAX,
                self.work_limit,
            )
        })?;
        check(
            RuntimeContentBudgetKind::RetainedBytes,
            next_retained,
            self.retained_limit,
        )?;
        check(
            RuntimeContentBudgetKind::WorkBytes,
            next_work,
            self.work_limit,
        )?;
        active.retained = next_retained;
        active.work = next_work;
        Ok(DecodeLease {
            owner: Arc::clone(self),
            retained,
            work,
        })
    }
}

impl DecodeLease {
    pub(super) fn admit_work(&mut self, allocation: usize) -> Result<(), RuntimeContentError> {
        let mut active = self
            .owner
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let required = active.work.checked_add(allocation).ok_or_else(|| {
            budget_error(
                RuntimeContentBudgetKind::WorkBytes,
                usize::MAX,
                self.owner.work_limit,
            )
        })?;
        check(
            RuntimeContentBudgetKind::WorkBytes,
            required,
            self.owner.work_limit,
        )?;
        self.work = self.work.checked_add(allocation).ok_or_else(|| {
            budget_error(
                RuntimeContentBudgetKind::WorkBytes,
                usize::MAX,
                self.owner.work_limit,
            )
        })?;
        active.work = required;
        Ok(())
    }

    /// The decoder's original promise must cover the complete retained result. An underestimated
    /// promise cannot borrow unreserved capacity after the allocation has already happened.
    pub(super) fn finish_decode(
        &mut self,
        actual_retained: usize,
    ) -> Result<(), RuntimeContentError> {
        check(
            RuntimeContentBudgetKind::RetainedBytes,
            actual_retained,
            self.retained,
        )?;
        let mut active = self
            .owner
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        active.retained -= self.retained - actual_retained;
        active.work -= self.work;
        self.retained = actual_retained;
        self.work = 0;
        Ok(())
    }
}

impl Drop for DecodeLease {
    fn drop(&mut self) {
        let mut active = self
            .owner
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        active.retained -= self.retained;
        active.work -= self.work;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refused_or_dropped_work_does_not_consume_the_original_capacity() {
        let budget = DecodeBudget::new(10, 40, 20);
        let mut first = budget.reserve(20, 15).unwrap();
        assert!(budget.reserve(1, 6).is_err());
        assert!(budget.reserve(11, 1).is_err());
        first.finish_decode(12).unwrap();
        let second = budget.reserve(18, 20).unwrap();
        assert!(budget.reserve(1, 1).is_err());
        drop(second);
        drop(first);
        let _all = budget.reserve(30, 20).unwrap();
    }

    #[test]
    fn decoded_output_cannot_exceed_its_predecode_promise() {
        let budget = DecodeBudget::new(0, 100, 100);
        let mut lease = budget.reserve(5, 5).unwrap();
        assert!(lease.finish_decode(6).is_err());
        drop(lease);
        let _all = budget.reserve(100, 100).unwrap();
    }
}
