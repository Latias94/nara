//! Observations of one explicit project-owned template import.

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
    #[error("the declared template or destination format is unsupported")]
    UnsupportedTemplate,
    #[error("the native template descriptor is invalid")]
    InvalidDescriptor,
    #[error("the original authoring Schema is unavailable or changed")]
    SchemaUnavailable,
    #[error("the native Prefab closure could not be interpreted and remapped")]
    InvalidPrefab,
    #[error("native template identities conflict with the project or another copied member")]
    IdentityConflict,
    #[error("the finite template import exceeded its budget")]
    BudgetExceeded,
    #[error("template import was cancelled after staging began")]
    Cancelled,
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

/// Exact intended project bytes, independent of directory publication and later source admission.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceToolTemplateImportMember {
    pub path: Arc<str>,
    pub digest: ContentDigest,
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
    PrefabImported {
        destination: Arc<str>,
        entry: Arc<str>,
        members: Arc<[EditorSourceToolTemplateImportMember]>,
        parent_directory_sync: StageStatus,
    },
    /// The directory is published even when its current source members fail admission.
    PrefabPublishedWithoutAdmission {
        destination: Arc<str>,
        entry: Arc<str>,
        members: Arc<[EditorSourceToolTemplateImportMember]>,
        parent_directory_sync: StageStatus,
        error: EditorSourceToolTemplateImportError,
    },
    /// Staging may remain. Created paths are facts, never automatic cleanup authority.
    PrefabNotPublished {
        destination: Arc<str>,
        staging: Option<Arc<str>>,
        created: Arc<[EditorSourceToolTemplateImportMember]>,
        created_directories: Arc<[Arc<str>]>,
        error: EditorSourceToolTemplateImportError,
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
