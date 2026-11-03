//! Finite admission for retained publication cleanup.

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
