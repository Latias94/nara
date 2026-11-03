//! Finite work and native custody for complete authoring source operations.

use nara_core::ItemLimit;

/// Complete-operation work and native custody selected before the first capture.
///
/// Retrying preserves the same allowance. Ordinary source capture keeps its own policy;
/// this value grants neither source publication nor permission to execute project code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthoringSourceOperationLimits {
    work: ItemLimit,
    open_handles: ItemLimit,
}

impl Default for AuthoringSourceOperationLimits {
    fn default() -> Self {
        Self {
            work: ItemLimit::new(200_000_000).expect("source operation work is finite and nonzero"),
            open_handles: ItemLimit::new(64)
                .expect("source operation native custody is finite and nonzero"),
        }
    }
}

impl AuthoringSourceOperationLimits {
    #[must_use]
    pub const fn with_host_work(mut self, limit: ItemLimit) -> Self {
        self.work = limit;
        self
    }
    #[must_use]
    pub const fn with_open_handles(mut self, limit: ItemLimit) -> Self {
        self.open_handles = limit;
        self
    }
    pub const fn host_work(self) -> ItemLimit {
        self.work
    }
    pub const fn open_handles(self) -> ItemLimit {
        self.open_handles
    }
}
