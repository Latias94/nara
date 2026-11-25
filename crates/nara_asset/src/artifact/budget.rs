//! Shared logical allocation admission, including escaped immutable groups.

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use super::model::{ArtifactBuildError, ArtifactBuildLimits};

pub(super) struct ArtifactBuildBudget {
    limits: ArtifactBuildLimits,
    work: AtomicUsize,
    retained: AtomicUsize,
}

impl ArtifactBuildBudget {
    pub(super) fn new(limits: ArtifactBuildLimits) -> Arc<Self> {
        Arc::new(Self {
            limits,
            work: AtomicUsize::new(0),
            retained: AtomicUsize::new(0),
        })
    }

    pub(super) fn reserve_work(
        self: &Arc<Self>,
        bytes: usize,
    ) -> Result<ArtifactBuildLease, ArtifactBuildError> {
        self.reserve(bytes, false)
    }

    pub(super) fn reserve_retained(
        self: &Arc<Self>,
        bytes: usize,
    ) -> Result<ArtifactBuildLease, ArtifactBuildError> {
        self.reserve(bytes, true)
    }

    fn reserve(
        self: &Arc<Self>,
        bytes: usize,
        retained: bool,
    ) -> Result<ArtifactBuildLease, ArtifactBuildError> {
        let (used, maximum, kind) = if retained {
            (&self.retained, self.limits.retained_bytes.get(), "retained")
        } else {
            (&self.work, self.limits.work_bytes.get(), "work")
        };
        let mut current = used.load(Ordering::Acquire);
        loop {
            let next = current
                .checked_add(bytes)
                .ok_or(ArtifactBuildError::Overflow)?;
            if next > maximum {
                return Err(ArtifactBuildError::Budget {
                    kind,
                    required: next,
                    limit: maximum,
                });
            }
            match used.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
                Ok(_) => {
                    return Ok(ArtifactBuildLease {
                        budget: Arc::clone(self),
                        bytes,
                        retained,
                    });
                }
                Err(observed) => current = observed,
            }
        }
    }
}

/// A unique charge. Share the containing group with Arc instead of cloning this lease.
pub(super) struct ArtifactBuildLease {
    budget: Arc<ArtifactBuildBudget>,
    bytes: usize,
    retained: bool,
}

impl ArtifactBuildLease {
    pub(super) fn shrink_to(&mut self, bytes: usize) -> Result<(), ArtifactBuildError> {
        if bytes > self.bytes {
            return Err(ArtifactBuildError::Budget {
                kind: "lease-growth",
                required: bytes,
                limit: self.bytes,
            });
        }
        let used = if self.retained {
            &self.budget.retained
        } else {
            &self.budget.work
        };
        used.fetch_sub(self.bytes - bytes, Ordering::AcqRel);
        self.bytes = bytes;
        Ok(())
    }
}

impl std::fmt::Debug for ArtifactBuildLease {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ArtifactBuildLease")
            .field("bytes", &self.bytes)
            .field("retained", &self.retained)
            .finish_non_exhaustive()
    }
}

impl Drop for ArtifactBuildLease {
    fn drop(&mut self) {
        let used = if self.retained {
            &self.budget.retained
        } else {
            &self.budget.work
        };
        used.fetch_sub(self.bytes, Ordering::AcqRel);
    }
}

pub(super) fn add(left: usize, right: usize) -> Result<usize, ArtifactBuildError> {
    left.checked_add(right).ok_or(ArtifactBuildError::Overflow)
}

pub(super) fn mul(left: usize, right: usize) -> Result<usize, ArtifactBuildError> {
    left.checked_mul(right).ok_or(ArtifactBuildError::Overflow)
}

pub(super) fn check(
    kind: &'static str,
    required: usize,
    limit: usize,
) -> Result<(), ArtifactBuildError> {
    if required > limit {
        Err(ArtifactBuildError::Budget {
            kind,
            required,
            limit,
        })
    } else {
        Ok(())
    }
}
