//! Observations of one explicit project-owned text-template copy.

use std::sync::Arc;

use nara_fs::{ContentDigest, StageStatus};

use super::EditorSourceToolAttemptId;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditorSourceToolTemplateImportError {
    #[error("the original installed selection changed")]
    StaleBasis,
    #[error("template import or installed selection work is pending")]
    Busy,
    #[error("the project is closing or adopting metadata")]
    Closing,
    #[error("the supplied capability belongs to another project")]
    RootMismatch,
    #[error("template import requires a strict read-write project capability")]
    Capability,
    #[error("the template or destination path is invalid")]
    InvalidPath,
    #[error("only declared finite UTF-8 text templates and .txt destinations are supported")]
    UnsupportedTemplate,
    #[error("the selected installation is unavailable or differs from its exact graph entry")]
    Installation,
    #[error("the destination already exists")]
    TargetExists,
    #[error("the captured source or destination directory changed")]
    Changed,
    #[error("the template could not be read")]
    Read,
    #[error("the project copy could not be published")]
    Write,
    #[error("the template import task was not admitted")]
    TaskRejected,
    #[error("template publication has no certain completion observation")]
    RequiresReconcile,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorSourceToolTemplateImportOutcome {
    Imported {
        destination: Arc<str>,
        digest: ContentDigest,
        parent_directory_sync: StageStatus,
    },
    /// Publication occurred, but its current project namespace could not be admitted.
    PublishedWithoutAdmission {
        destination: Arc<str>,
        digest: ContentDigest,
        parent_directory_sync: StageStatus,
    },
    /// Retains the filesystem's uncertain evidence without inventing a successful copy.
    PublicationUncertain {
        destination: Arc<str>,
        digest: ContentDigest,
        parent_directory_sync: StageStatus,
    },
    Cancelled,
    Failed(EditorSourceToolTemplateImportError),
}

/// Actual publication, executor cleanup and physical pending remain separate observations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditorSourceToolTemplateImportView {
    pub attempt: Option<EditorSourceToolAttemptId>,
    pub outcome: Option<EditorSourceToolTemplateImportOutcome>,
    pub physical_work_pending: bool,
    pub cleanup_failed: bool,
}
