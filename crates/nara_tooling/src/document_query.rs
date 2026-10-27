use std::{error::Error, fmt, sync::Arc};

use crate::{EditorDocumentId, EditorDocumentKind};
use nara_scene::{
    AuthoringReadError, AuthoringSchemaBasis, PrefabDeclarationError, SceneAuthoringRevision,
};

/// Exact open-document facts used to begin enumeration or describe an owned page.
///
/// Obtain these facts from the document tab model. The revision includes its source
/// incarnation; a source locator or saved content identity is not a substitute.
/// `schema` is `None` for sessions without an authoring Schema admission, not a wildcard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorDocumentQueryBasis {
    pub document: EditorDocumentId,
    pub revision: SceneAuthoringRevision,
    pub schema: Option<AuthoringSchemaBasis>,
}

/// Exact rejection of a query without changing document or presentation state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorDocumentQueryRejection {
    /// No readable project workspace is available through the current adapter.
    WorkspaceUnavailable,
    /// The owning Editor is closing or resolving a workspace intent; callers must not retry a
    /// read-only observation as though the workspace had merely not been installed yet.
    Closing,
    /// The continuation belongs to another workspace, including a replaced project workspace.
    WrongWorkspace,
    /// The explicitly named document is no longer open, or was never open here.
    DocumentNotOpen { document: EditorDocumentId },
    /// Only direct Scene and Prefab source records are supported.
    UnsupportedDocumentKind { kind: EditorDocumentKind },
    /// The slot no longer has the source kind captured by the continuation.
    DocumentKindChanged {
        expected: EditorDocumentKind,
        actual: EditorDocumentKind,
    },
    /// The document was reopened or replaced; a matching locator cannot rebase the query.
    StaleIncarnation,
    /// The document changed within the captured incarnation, even if Undo restores its content.
    StaleRevision,
    /// The exact retained Schema admission changed; equal catalogs are not equal admissions.
    StaleSchema,
    /// A positive limit exceeded its cap, or either limit was zero.
    InvalidLimits {
        max_scan_records: usize,
        max_rows: usize,
    },
    /// The owner-created continuation does not identify a valid position in its exact revision.
    InvalidContinuation,
    /// A discovery scan, row or metadata limit is zero or above its public cap.
    InvalidAuthoringLimits,
    /// A selected-property scan, metadata or value limit is zero or above its public cap.
    InvalidPropertyReadLimits,
    /// The aggregate owned observation's UTF-8 metadata exceeds the operation's limit.
    MetadataBudgetExceeded,
    /// The selected value exceeds its node or logical-byte limit; nothing is truncated.
    ValueBudgetExceeded,
    /// Actual presence/readability/default interpretation is rejected by the source owner.
    Source(AuthoringReadError),
    /// Bounded source Prefab declaration observation is rejected by its owning session.
    PrefabDeclaration(PrefabDeclarationError),
}

impl fmt::Display for EditorDocumentQueryRejection {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WorkspaceUnavailable => {
                formatter.write_str("no readable document workspace is available")
            }
            Self::Closing => formatter.write_str("document workspace is closing"),
            Self::WrongWorkspace => {
                formatter.write_str("document query belongs to another workspace")
            }
            Self::DocumentNotOpen { .. } => {
                formatter.write_str("document query document is not open")
            }
            Self::UnsupportedDocumentKind { .. } => {
                formatter.write_str("document query requires a Scene or Prefab source")
            }
            Self::DocumentKindChanged { .. } => {
                formatter.write_str("document query document kind changed")
            }
            Self::StaleIncarnation => {
                formatter.write_str("document query source incarnation changed")
            }
            Self::StaleRevision => formatter.write_str("document query document revision changed"),
            Self::StaleSchema => formatter.write_str("document query Schema admission changed"),
            Self::InvalidLimits { .. } => {
                formatter.write_str("document query limits are out of range")
            }
            Self::InvalidContinuation => {
                formatter.write_str("document query continuation is invalid")
            }
            Self::InvalidAuthoringLimits => {
                formatter.write_str("authoring discovery limits are out of range")
            }
            Self::InvalidPropertyReadLimits => {
                formatter.write_str("property read limits are out of range")
            }
            Self::MetadataBudgetExceeded => {
                formatter.write_str("document query metadata budget exceeded")
            }
            Self::ValueBudgetExceeded => formatter.write_str("property read value budget exceeded"),
            Self::Source(error) => error.fmt(formatter),
            Self::PrefabDeclaration(error) => error.fmt(formatter),
        }
    }
}

impl Error for EditorDocumentQueryRejection {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            Self::PrefabDeclaration(error) => Some(error),
            _ => None,
        }
    }
}

impl From<AuthoringReadError> for EditorDocumentQueryRejection {
    fn from(error: AuthoringReadError) -> Self {
        Self::Source(error)
    }
}

impl From<PrefabDeclarationError> for EditorDocumentQueryRejection {
    fn from(error: PrefabDeclarationError) -> Self {
        Self::PrefabDeclaration(error)
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct EditorDocumentQueryOwner(Arc<()>);

impl PartialEq for EditorDocumentQueryOwner {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for EditorDocumentQueryOwner {}
