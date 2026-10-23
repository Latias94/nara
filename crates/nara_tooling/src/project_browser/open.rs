//! Exact asynchronous document-open observation, independent of dispatch completion.

use std::sync::Arc;

use crate::{EditorDocumentId, EditorFrameRevision};

/// Identifies one accepted document-open request and its admitting Host frame incarnation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EditorDocumentOpenId {
    source: EditorFrameRevision,
    sequence: u64,
}

impl EditorDocumentOpenId {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(source: EditorFrameRevision, sequence: u64) -> Self {
        Self { source, sequence }
    }

    /// Returns the local sequence for diagnostics; equality also includes Host provenance.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.sequence
    }
}

/// Preparation and physical cancellation are distinct from successful document publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorDocumentOpenPhase {
    Preparing,
    Cancelling,
}

/// Terminal outcome retained until acknowledgement of this exact request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorDocumentOpenResult {
    Opened {
        request: EditorDocumentOpenId,
        document: EditorDocumentId,
    },
    Rejected {
        request: EditorDocumentOpenId,
        code: &'static str,
    },
    Cancelled {
        request: EditorDocumentOpenId,
    },
}

impl EditorDocumentOpenResult {
    #[must_use]
    pub const fn request(self) -> EditorDocumentOpenId {
        match self {
            Self::Opened { request, .. }
            | Self::Rejected { request, .. }
            | Self::Cancelled { request } => request,
        }
    }
}

/// Immutable project-global observation; changing selection does not retarget the request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EditorDocumentOpenView {
    pending: Option<(EditorDocumentOpenId, Arc<str>, EditorDocumentOpenPhase)>,
    result: Option<EditorDocumentOpenResult>,
}

impl EditorDocumentOpenView {
    #[doc(hidden)]
    #[must_use]
    pub fn new(
        pending: Option<(EditorDocumentOpenId, Arc<str>, EditorDocumentOpenPhase)>,
        result: Option<EditorDocumentOpenResult>,
    ) -> Self {
        Self { pending, result }
    }

    #[must_use]
    pub fn pending(&self) -> Option<(EditorDocumentOpenId, &str, EditorDocumentOpenPhase)> {
        self.pending
            .as_ref()
            .map(|(id, path, phase)| (*id, path.as_ref(), *phase))
    }

    #[must_use]
    pub const fn has_pending_work(&self) -> bool {
        self.pending.is_some()
    }

    #[must_use]
    pub const fn result(&self) -> Option<EditorDocumentOpenResult> {
        self.result
    }
}
