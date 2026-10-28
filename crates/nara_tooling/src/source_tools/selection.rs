//! Pure observations for the Session-owned durable finite source-tool selection.

use std::sync::Arc;

use nara_fs::ContentDigest;

/// One nominal observation of the owning Session's selection state.
#[derive(Debug, Clone)]
pub struct EditorSourceToolSelectionBasis(Arc<()>);

impl EditorSourceToolSelectionBasis {
    /// Issues a fresh identity; only a basis retained by the owning Session is admitted.
    #[doc(hidden)]
    #[must_use]
    pub fn issue() -> Self {
        Self(Arc::new(()))
    }
}

impl PartialEq for EditorSourceToolSelectionBasis {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for EditorSourceToolSelectionBasis {}

/// Persisted installation facts, without executable approval or created-file custody.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolInstalledSelection {
    pub package: Arc<str>,
    pub directory: Arc<str>,
    pub installation: [u8; 32],
    pub receipt: ContentDigest,
    pub manifest: ContentDigest,
}

/// One declared package requirement bound to an exact installed member.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolRequirementBinding {
    pub package: Arc<str>,
    pub installation: [u8; 32],
}

/// Recorded installation facts and explicit requirements, without executable approval.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolInstalledEntry {
    pub installed: EditorSourceToolInstalledSelection,
    pub requirements: Vec<EditorSourceToolRequirementBinding>,
}

/// Code-free installation inspection requested through the owning project capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolInstallationRequest {
    pub directory: Arc<str>,
    pub receipt: ContentDigest,
    pub requirements: Vec<EditorSourceToolRequirementBinding>,
}

/// Whether withdrawal may include members that require the selected installation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSourceToolWithdrawalMode {
    RejectDependents,
    Cascade,
}

/// Durable user intent. Enablement still requires independent Host approval and Session admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolDesiredSelection {
    pub installed: EditorSourceToolInstalledSelection,
    pub enabled: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSourceToolSelectionPhase {
    Loading,
    Idle,
    Probing,
    Publishing,
    Reconciling,
    Unavailable,
    RequiresReconcile,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditorSourceToolSelectionError {
    #[error("the source-tool selection observation is stale")]
    StaleBasis,
    #[error("source-tool selection work is still pending")]
    Busy,
    #[error("the project is closing")]
    Closing,
    #[error("the source-tool selection belongs to another project root")]
    RootMismatch,
    #[error("the selected installation has no matching Host approval")]
    NotApproved,
    #[error("the approved source-tool probe failed; inspect the tool attempt diagnostic")]
    ProbeFailed,
    #[error("the source-tool probe was explicitly cancelled")]
    ProbeCancelled,
    #[error("the source-tool selection path is invalid or cannot be admitted")]
    InvalidPath,
    #[error("the source-tool selection record is invalid")]
    InvalidRecord,
    #[error("the source-tool selection format version is unsupported")]
    UnsupportedFormat,
    #[error("the source-tool selection record exceeds its byte limit")]
    TooLarge,
    #[error("the source-tool selection could not be read")]
    Read,
    #[error("the source-tool selection could not be written")]
    Write,
    #[error("the source-tool selection changed")]
    Changed,
    #[error("the source-tool selection requires explicit reconciliation")]
    RequiresReconcile,
    #[error("the source-tool selection capability is read-only")]
    ReadOnly,
    #[error("the source-tool selection task was not admitted")]
    TaskRejected,
    #[error("the selected source-tool installation could not be admitted")]
    Installation,
    #[error("the installed source-tool graph is invalid or exceeds its limits")]
    Graph,
    #[error("other installed source tools require this installation")]
    RequiredDependents,
    #[error("source-tool installation capture exceeded its shared byte-work allowance")]
    CaptureBudget,
    #[error("the selected source tool does not match the admitted Schema")]
    Schema,
    #[error("the prepared source-tool activation could not be promoted")]
    PromotionRefused,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorSourceToolSelectionOutcome {
    Observed,
    Registered,
    Withdrawn,
    Enabled,
    Disabled,
    PublishedWithoutActivation,
    Reconciled,
    Failed(EditorSourceToolSelectionError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolSelectionAttempt {
    pub attempt: super::EditorSourceToolAttemptId,
    /// Pending until the owning operation reaches its actual terminal outcome.
    pub outcome: Option<EditorSourceToolSelectionOutcome>,
}

/// The durable request and active origin are independent observations of one Session-owned slot.
///
/// Observe/Enable/Reconcile share `operation`; Disable has its own `disable` observation. Each
/// survives until another operation is explicitly accepted in that same lane. They cannot
/// overwrite each other's terminal outcome, including completion within one owner drive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolSelectionView {
    pub basis: EditorSourceToolSelectionBasis,
    pub installed: Vec<EditorSourceToolInstalledEntry>,
    pub desired: Option<EditorSourceToolDesiredSelection>,
    pub active_origin: Option<EditorSourceToolInstalledSelection>,
    pub phase: EditorSourceToolSelectionPhase,
    pub operation: Option<EditorSourceToolSelectionAttempt>,
    pub disable: Option<EditorSourceToolSelectionAttempt>,
    pub error: Option<EditorSourceToolSelectionError>,
}
