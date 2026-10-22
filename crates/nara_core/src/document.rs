//! Source incarnations and history projections shared by document owners.

use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_DOCUMENT_SOURCE: AtomicU64 = AtomicU64::new(1);

/// Process-local identity of one accepted document incarnation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentSourceId(u64);

/// Monotonic accepted transition within one document incarnation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentRevision {
    source_id: DocumentSourceId,
    generation: u64,
}

/// Identity of a committed content state, retained by Undo and Redo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DocumentContentState {
    source_id: DocumentSourceId,
    generation: u64,
}

/// Read-only history availability; command and inverse storage remain domain-owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DocumentHistoryStatus {
    pub undo_depth: usize,
    pub redo_depth: usize,
}

impl DocumentRevision {
    /// Starts a fresh incarnation. Reopen and source replacement use a new identity.
    #[must_use]
    pub fn initial() -> Self {
        let source = NEXT_DOCUMENT_SOURCE
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .expect("document source identity space is exhausted");
        Self {
            source_id: DocumentSourceId(source),
            generation: 0,
        }
    }

    #[must_use]
    pub const fn source_id(self) -> DocumentSourceId {
        self.source_id
    }

    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }

    /// Advances the domain owner's accepted transition without changing its incarnation.
    #[must_use]
    pub const fn next(self) -> Self {
        Self {
            source_id: self.source_id,
            generation: self
                .generation
                .checked_add(1)
                .expect("document revision space is exhausted"),
        }
    }

    /// Captures the newly committed content identity for a forward edit.
    #[must_use]
    pub const fn content_state(self) -> DocumentContentState {
        DocumentContentState {
            source_id: self.source_id,
            generation: self.generation,
        }
    }
}

impl DocumentContentState {
    #[must_use]
    pub const fn source_id(self) -> DocumentSourceId {
        self.source_id
    }
}
