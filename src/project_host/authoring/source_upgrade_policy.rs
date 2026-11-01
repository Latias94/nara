//! Effects and finite Host resources shared by live upgrades and process recovery.

use nara_core::ItemLimit;
use nara_fs::{TemporaryFileCleanup, TemporaryFileFailure};

use super::{AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionError};

// Cleanup reuses the retained candidate, name and parent. It performs bounded metadata/removal
// work, without opening another handle, parsing a path or reading file contents. A failed discard
// can return its cleanup in a Box; reserve that result and bounded native metadata before calling.
pub(super) const CLEANUP_WORK: usize = 1_024;
pub(super) const CLEANUP_SCRATCH: usize = std::mem::size_of::<TemporaryFileCleanup>()
    + std::mem::size_of::<TemporaryFileFailure>()
    + 1_024;

/// Admits one actual cleanup attempt while retaining the operation's original publisher/custody.
/// Call only when cleanup is pending. Failed attempts keep their work charge, and a refused
/// admission never invokes the native cleanup closure or refunds any earlier live resource.
pub(super) fn with_cleanup_admission(
    budget: &mut AuthoringProjectAdmissionBudget,
    cleanup: impl FnOnce() -> bool,
) -> Result<bool, AuthoringProjectAdmissionError> {
    let before = budget.retained_candidate_bytes;
    if let Err(error) = budget.charge_source_bytes(0, 0, CLEANUP_WORK, CLEANUP_SCRATCH) {
        let unused = budget
            .retained_candidate_bytes
            .checked_sub(before)
            .expect("refused cleanup allocates no candidate storage");
        budget
            .reconcile_retained_reservation(unused, 0)
            .expect("refused cleanup retains its original custody");
        return Err(error);
    }
    let cleaned = cleanup();
    budget
        .reconcile_retained_reservation(CLEANUP_SCRATCH, 0)
        .expect("native cleanup returned and its temporary result has dropped");
    Ok(cleaned)
}

/// Successful publication saves the reviewed working copies and begins new histories.
/// Recovery establishes fresh checkpoints; it never reconstructs a previous process's history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoredSchemaUpgradeHistoryPolicy {
    SaveReviewedWorkingCopiesAndStartNewHistory,
}

/// Complete-operation work and native custody selected before the first capture.
///
/// Retrying preserves the same allowance. Ordinary Schema capture keeps its own policy;
/// this value grants neither source publication nor permission to execute project code.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthoredSchemaUpgradeHostLimits {
    work: ItemLimit,
    open_handles: ItemLimit,
}

impl Default for AuthoredSchemaUpgradeHostLimits {
    fn default() -> Self {
        Self {
            work: ItemLimit::new(200_000_000).expect("upgrade work is finite and nonzero"),
            open_handles: ItemLimit::new(64).expect("upgrade native custody is finite and nonzero"),
        }
    }
}

impl AuthoredSchemaUpgradeHostLimits {
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
