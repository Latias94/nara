//! Shared admission for concurrent decodes and retained typed output.

use std::sync::{Arc, Mutex};

/// Shared admission for retained storage and concurrent work from the same content base.
pub struct DecodeBudget {
    active: Mutex<Active>,
    retained_limit: usize,
    work_limit: usize,
}

struct Active {
    retained: usize,
    work: usize,
}

/// A reservation returned to its original budget when dropped.
pub struct DecodeLease {
    owner: Arc<DecodeBudget>,
    retained: usize,
    work: usize,
}

/// The shared allowance or a pre-decode retained promise could not cover the required bytes.
#[derive(Debug, thiserror::Error)]
pub enum DecodeBudgetError {
    #[error("retained bytes exceed the limit of {limit} (required {required})")]
    RetainedBytes { required: usize, limit: usize },
    #[error("work bytes exceed the limit of {limit} (required {required})")]
    WorkBytes { required: usize, limit: usize },
}

impl DecodeBudget {
    /// Creates a budget with the caller's already admitted fixed base charge.
    #[must_use]
    pub fn new(retained: usize, retained_limit: usize, work_limit: usize) -> Arc<Self> {
        Arc::new(Self {
            active: Mutex::new(Active { retained, work: 0 }),
            retained_limit,
            work_limit,
        })
    }

    /// Admits retained and work bytes together before their allocations.
    pub fn reserve(
        self: &Arc<Self>,
        retained: usize,
        work: usize,
    ) -> Result<DecodeLease, DecodeBudgetError> {
        let mut active = self
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let retained_error = |required| DecodeBudgetError::RetainedBytes {
            required,
            limit: self.retained_limit,
        };
        let work_error = |required| DecodeBudgetError::WorkBytes {
            required,
            limit: self.work_limit,
        };
        let next_retained = active
            .retained
            .checked_add(retained)
            .ok_or_else(|| retained_error(usize::MAX))?;
        let next_work = active
            .work
            .checked_add(work)
            .ok_or_else(|| work_error(usize::MAX))?;
        if next_retained > self.retained_limit {
            return Err(retained_error(next_retained));
        }
        if next_work > self.work_limit {
            return Err(work_error(next_work));
        }
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
    /// Adds to this lease's work reservation before the allocation.
    pub fn admit_work(&mut self, allocation: usize) -> Result<(), DecodeBudgetError> {
        let mut active = self
            .owner
            .active
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let limit = self.owner.work_limit;
        let error = |required| DecodeBudgetError::WorkBytes { required, limit };
        let required = active
            .work
            .checked_add(allocation)
            .ok_or_else(|| error(usize::MAX))?;
        if required > limit {
            return Err(error(required));
        }
        self.work = self
            .work
            .checked_add(allocation)
            .ok_or_else(|| error(usize::MAX))?;
        active.work = required;
        Ok(())
    }

    /// The decoder's original promise must cover the complete retained result. An underestimated
    /// promise cannot borrow unreserved capacity after the allocation has already happened.
    /// Releases work and reduces retained bytes to the checked actual result.
    pub fn finish_decode(&mut self, actual_retained: usize) -> Result<(), DecodeBudgetError> {
        if actual_retained > self.retained {
            return Err(DecodeBudgetError::RetainedBytes {
                required: actual_retained,
                limit: self.retained,
            });
        }
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
