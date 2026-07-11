use std::num::NonZeroU64;

use thiserror::Error;

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum IdentityAllocationError {
    #[error("identity allocation cannot start at zero")]
    Zero,
    #[error("identity allocation is exhausted")]
    Exhausted,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MonotonicNonZeroU64Allocator {
    next: Option<NonZeroU64>,
}

impl MonotonicNonZeroU64Allocator {
    pub(crate) fn from_next_raw(raw: u64) -> Result<Self, IdentityAllocationError> {
        let next = NonZeroU64::new(raw).ok_or(IdentityAllocationError::Zero)?;
        Ok(Self { next: Some(next) })
    }

    pub(crate) fn peek(&self) -> Result<NonZeroU64, IdentityAllocationError> {
        self.next.ok_or(IdentityAllocationError::Exhausted)
    }

    pub(crate) fn allocate(&mut self) -> Result<NonZeroU64, IdentityAllocationError> {
        let current = self.peek()?;
        self.next = current.get().checked_add(1).and_then(NonZeroU64::new);
        Ok(current)
    }

    pub(crate) fn reserve(&mut self, value: NonZeroU64) {
        let Some(next) = self.next else {
            return;
        };
        if value < next {
            return;
        }
        self.next = value.get().checked_add(1).and_then(NonZeroU64::new);
    }
}
