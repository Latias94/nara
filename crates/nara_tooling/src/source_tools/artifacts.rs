//! Bounded Session-owned artifact inventory observations, without removal authority.

use std::sync::Arc;

use nara_fs::ContentDigest;

use super::{EditorSourceToolAttemptId, EditorSourceToolReclamationRecord};

operation_id!(EditorSourceToolArtifactBasis);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSourceToolArtifactRole {
    Tool,
    Content,
    Metadata,
    ReceiptTemporary,
}

/// The last finite observation of a member, never a successful removal receipt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSourceToolArtifactObservation {
    Present,
    Changed,
    Missing,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolArtifactMember {
    pub path: Arc<str>,
    pub role: EditorSourceToolArtifactRole,
    /// Captured intended bytes, independent of the last observed partial or changed content.
    pub baseline: ContentDigest,
    pub observation: EditorSourceToolArtifactObservation,
}

/// Publication facts alone never grant execution, resumption or deletion authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSourceToolArtifactPublication {
    Ready { receipt: ContentDigest },
    Incomplete,
}

/// Exact publication facts and the members actually created in one managed directory.
///
/// The nonzero `id` correlates these facts; it cannot reconstruct live object custody or authorize
/// deletion. An incomplete publication may contain no members and has no ready receipt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolArtifact {
    pub id: [u8; 32],
    pub package: Arc<str>,
    pub directory: Arc<str>,
    pub manifest: ContentDigest,
    pub publication: EditorSourceToolArtifactPublication,
    pub members: Vec<EditorSourceToolArtifactMember>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSourceToolArtifactPhase {
    Loading,
    Idle,
    Publishing,
    Reconciling,
    Unavailable,
    RequiresReconcile,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditorSourceToolArtifactError {
    #[error("the artifact inventory observation is stale")]
    StaleBasis,
    #[error("artifact inventory work is still physically pending")]
    Busy,
    #[error("the project is closing or adopting metadata")]
    Closing,
    #[error("the artifact belongs to another project directory")]
    RootMismatch,
    #[error("the artifact inventory is invalid")]
    InvalidRecord,
    #[error("the artifact inventory format version is unsupported")]
    UnsupportedFormat,
    #[error("the artifact inventory exceeds its limits")]
    TooLarge,
    #[error("the artifact path is invalid or outside the managed store")]
    InvalidPath,
    #[error("the artifact inventory could not be read")]
    Read,
    #[error("the artifact inventory could not be written")]
    Write,
    #[error("the original artifact observation changed")]
    Changed,
    #[error("an ordinary document already owns an artifact member")]
    SourceOwned,
    #[error("artifact inventory publication requires explicit reconciliation")]
    RequiresReconcile,
    #[error("the artifact inventory capability is read-only")]
    ReadOnly,
    #[error("the artifact inventory task was not admitted")]
    TaskRejected,
    #[error("the artifact inventory task returned with failed executor cleanup")]
    TaskCleanupFailed,
    #[error("the artifact is absent from this artifact inventory")]
    Unregistered,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorSourceToolArtifactOutcome {
    Observed,
    Registered,
    Reconciled,
    /// An exact reclamation intent or results record was published and accepted.
    ReclamationRecorded,
    /// The exact inventory was published and accepted before executor cleanup failed.
    /// The accepted artifacts remain observable; this does not certify successful cleanup.
    PublishedWithCleanupFailure,
    Failed(EditorSourceToolArtifactError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolArtifactAttempt {
    pub attempt: EditorSourceToolAttemptId,
    pub outcome: Option<EditorSourceToolArtifactOutcome>,
}

/// Inventory publication and actual work return are independent of graph selection and approval.
///
/// An unavailable inventory does not prevent ordinary document Save, Reopen or Close. Recovery
/// records retain reviewed intent and actual per-member results without authorizing deletion or
/// automatic resumption. Fresh reclamation requires a new explicit review of live objects.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolArtifactView {
    pub basis: EditorSourceToolArtifactBasis,
    pub artifacts: Vec<EditorSourceToolArtifact>,
    pub reclamations: Vec<EditorSourceToolReclamationRecord>,
    pub phase: EditorSourceToolArtifactPhase,
    pub operation: Option<EditorSourceToolArtifactAttempt>,
    pub error: Option<EditorSourceToolArtifactError>,
    pub physical_work_pending: bool,
}
