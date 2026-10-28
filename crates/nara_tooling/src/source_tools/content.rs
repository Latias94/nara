//! Pure observations of Session-owned installed-content protection, without deletion authority.

use std::sync::Arc;

use nara_fs::ContentDigest;

use super::{EditorSourceToolAttemptId, EditorSourceToolInstalledSelection};

operation_id!(EditorSourceToolContentBasis);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSourceToolContentRole {
    Tool,
    Content,
    Metadata,
}

/// A recorded observation, not a permission to remove the currently named file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSourceToolContentObservation {
    Present,
    Pending,
    Removed,
    Missing,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolContentMember {
    pub path: Arc<str>,
    pub role: EditorSourceToolContentRole,
    pub baseline: ContentDigest,
    /// Once an alteration is observed, baseline-equal bytes cannot clear this protection.
    pub modified: bool,
    /// Explicit project adoption is independent of modification and survives withdrawal.
    pub adopted: bool,
    pub observation: EditorSourceToolContentObservation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolContentInstallation {
    pub installed: EditorSourceToolInstalledSelection,
    pub members: Vec<EditorSourceToolContentMember>,
    /// Projects the retained live installation owner; this Boolean grants no filesystem rights.
    pub creation_custody: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSourceToolContentPhase {
    Loading,
    Idle,
    /// Accepted document edits have queued a protection publication.
    ProtectionPending,
    Publishing,
    Reconciling,
    Unavailable,
    RequiresReconcile,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditorSourceToolContentError {
    #[error("the installed-content observation is stale")]
    StaleBasis,
    #[error("installed-content work is still physically pending")]
    Busy,
    #[error("the project is closing or adopting metadata")]
    Closing,
    #[error("the installation belongs to another project directory")]
    RootMismatch,
    #[error("the installed-content record is invalid")]
    InvalidRecord,
    #[error("the installed-content format version is unsupported")]
    UnsupportedFormat,
    #[error("the installed-content record exceeds its limits")]
    TooLarge,
    #[error("the installed-content path is invalid or reserved")]
    InvalidPath,
    #[error("the installed-content record could not be read")]
    Read,
    #[error("the installed-content record could not be written")]
    Write,
    #[error("the original installed-content observation changed")]
    Changed,
    #[error("installed-content publication requires explicit reconciliation")]
    RequiresReconcile,
    #[error("the installed-content capability is read-only")]
    ReadOnly,
    #[error("the installed-content task was not admitted")]
    TaskRejected,
    #[error("the installation or member is not managed by this observation")]
    Unmanaged,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorSourceToolContentOutcome {
    Observed,
    Managed,
    Adopted,
    Protected,
    Reconciled,
    /// Disk protection was weaker; an explicit candidate publication must make it durable.
    NeedsProtectionPublication,
    Failed(EditorSourceToolContentError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolContentAttempt {
    pub attempt: EditorSourceToolAttemptId,
    pub outcome: Option<EditorSourceToolContentOutcome>,
}

/// Bounded protection and recovery facts, separate from graph selection and execution approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolContentView {
    pub basis: EditorSourceToolContentBasis,
    pub installations: Vec<EditorSourceToolContentInstallation>,
    pub phase: EditorSourceToolContentPhase,
    pub operation: Option<EditorSourceToolContentAttempt>,
    pub error: Option<EditorSourceToolContentError>,
    /// Known protection still needs confirmation in the admitted disk record.
    /// An accepted source edit alone does not promise crash-durable protection.
    pub protection_publication_pending: bool,
    /// Actual task return and publication cleanup can outlive a terminal operation.
    pub physical_work_pending: bool,
}
