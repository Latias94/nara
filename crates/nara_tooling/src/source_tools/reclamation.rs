//! Bounded reclamation observations. Durable records never grant deletion authority.

use std::sync::Arc;

use super::EditorSourceToolAttemptId;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSourceToolReclamationRecordPhase {
    Intent,
    Results,
}

/// A retained member fact. Missing does not claim that this attempt removed the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSourceToolReclamationMemberState {
    Pending,
    Removed,
    Missing,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolReclamationMember {
    pub path: Arc<str>,
    pub state: EditorSourceToolReclamationMemberState,
}

/// Recovery facts for one artifact, without native object identities or live authority.
///
/// The nonzero random `id` correlates one explicit review's durable intent and results. It is
/// independent of Session attempt identities and never authorizes resumption or deletion.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolReclamationRecord {
    pub id: [u8; 32],
    pub artifact: [u8; 32],
    pub phase: EditorSourceToolReclamationRecordPhase,
    pub members: Vec<EditorSourceToolReclamationMember>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditorSourceToolReclamationError {
    #[error("the original reclamation review is stale")]
    StaleBasis,
    #[error("reclamation or conflicting source-tool work is still pending")]
    Busy,
    #[error("the project is closing or adopting metadata")]
    Closing,
    #[error("the reclamation review belongs to another project directory")]
    RootMismatch,
    #[error("the artifact is absent from the artifact inventory")]
    Unregistered,
    #[error("the artifact is still present in the installed graph")]
    StillInstalled,
    #[error("an ordinary document owns an artifact member")]
    SourceOwned,
    #[error("the original artifact observation changed")]
    Changed,
    #[error("the reclamation facts could not be read")]
    Read,
    #[error("the reclamation intent, member removal or results could not be published")]
    Write,
    #[error("the capability or platform does not support reviewed artifact reclamation")]
    Unsupported,
    #[error("the reclamation task was not admitted")]
    TaskRejected,
    #[error("the reclamation task returned with failed executor cleanup")]
    TaskCleanupFailed,
    #[error("reclamation publication requires explicit reconciliation")]
    RequiresReconcile,
    #[error("the reclamation exceeds its finite allowance")]
    Limit,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EditorSourceToolReclamationPhase {
    #[default]
    Idle,
    PublishingIntent,
    Removing,
    PublishingResults,
    Terminal,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorSourceToolReclamationOutcome {
    Complete,
    Partial,
    Cancelled,
    Failed(EditorSourceToolReclamationError),
}

/// Logical outcome, durable publication debt and physical return remain independent facts.
///
/// Fresh Open only observes a durable record. Continuing its pending members requires a new
/// explicit review; neither a terminal outcome nor an idle phase grants removal permission.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditorSourceToolReclamationView {
    pub attempt: Option<EditorSourceToolAttemptId>,
    pub phase: EditorSourceToolReclamationPhase,
    pub record: Option<EditorSourceToolReclamationRecord>,
    pub outcome: Option<EditorSourceToolReclamationOutcome>,
    pub error: Option<EditorSourceToolReclamationError>,
    pub record_publication_pending: bool,
    pub physical_work_pending: bool,
}
