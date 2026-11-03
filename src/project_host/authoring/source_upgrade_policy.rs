//! History policy for live authored-schema upgrades and process recovery.

/// Successful publication saves the reviewed working copies and begins new histories.
/// Recovery establishes fresh checkpoints; it never reconstructs a previous process's history.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoredSchemaUpgradeHistoryPolicy {
    SaveReviewedWorkingCopiesAndStartNewHistory,
}
