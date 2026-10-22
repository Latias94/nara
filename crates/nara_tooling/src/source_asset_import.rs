//! Explicit source-backed metadata authoring for trusted compiled Editor domains.

use std::sync::Arc;

use nara_asset::StableAssetId;
use nara_diagnostic::DiagnosticReport;

/// Encoded settings belong to the named contributor; tooling does not interpret their schema.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceAssetDomainView {
    pub source_kind: &'static str,
    pub default_settings: Arc<str>,
}

/// Discovery facts, not a proof that source bytes are still fresh.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorSourceAssetMetadataView {
    Missing,
    Ready {
        stable_id: StableAssetId,
        source_kind: Arc<str>,
        settings: Option<Arc<str>>,
    },
    Invalid,
    IdentityConflict {
        stable_id: StableAssetId,
    },
}

/// The terminal success refers only to metadata, not an installed Editor/runtime resource.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorSourceAssetImportAttempt {
    Idle,
    Preparing,
    MetadataPublished { stable_id: StableAssetId },
    Failed { diagnostics: DiagnosticReport },
    ReconcileRequired,
}

/// Immutable detail for the selected source and the contributors compiled into this Editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceAssetImportView {
    pub relative_path: Arc<str>,
    pub domains: Box<[EditorSourceAssetDomainView]>,
    pub metadata: EditorSourceAssetMetadataView,
    pub attempt: EditorSourceAssetImportAttempt,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSourceAssetImportAction {
    pub relative_path: Arc<str>,
    pub source_kind: Arc<str>,
    pub settings: Arc<str>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSourceAssetImportRejection {
    Unavailable,
    Busy,
    StaleSource,
    InvalidMetadata,
    InvalidSettings,
    PublicationUnavailable,
}

/// Acceptance starts bounded work. Read the later frame for terminal publication evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSourceAssetImportActionOutcome {
    Started,
    Rejected(EditorSourceAssetImportRejection),
}
