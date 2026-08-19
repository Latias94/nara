//! UI-neutral Project Browser snapshots, local query state, and Host action seams.

mod state;
#[cfg(test)]
mod tests;

use std::sync::Arc;

pub use state::{
    EditorProjectBrowserKindFilter, EditorProjectBrowserLocalAction,
    EditorProjectBrowserPendingWork, EditorProjectBrowserQueryAdvance, EditorProjectBrowserRow,
    EditorProjectBrowserState, EditorProjectBrowserView,
};

use crate::EditorDocumentId;

/// Source-index publication state visible to Editor Adapters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorProjectBrowserStatus {
    NotRequested,
    Scanning,
    Ready,
    ReadyWithDiagnostics,
    Unavailable,
}

/// Stable source kind projected without filesystem authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum EditorProjectSourceKind {
    ProjectManifest,
    AuthoringSchemaBundle,
    Scene,
    Prefab,
    AssetSource,
    AssetMeta,
    Script,
    Directory,
    Unknown,
    Unsupported,
}

/// Safe explanation for a source that is inspectable but cannot be opened as a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorProjectSourceInspectReason {
    ProjectAuthority,
    ReadOnlySource,
    Directory,
    UnknownSourceKind,
    MissingDeclaredRoot,
    UnavailableDeclaredRoot,
    UnsupportedEntry,
    IdentityUnavailable,
}

/// Operation admitted by one source-index entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorProjectSourceOpenCapability {
    Scene,
    Prefab,
    InspectOnly(EditorProjectSourceInspectReason),
    None,
}

impl EditorProjectSourceOpenCapability {
    #[must_use]
    pub const fn is_document(self) -> bool {
        matches!(self, Self::Scene | Self::Prefab)
    }
}

/// Safe source fact retained by one immutable browser generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorProjectBrowserEntry {
    relative_path: Arc<str>,
    kind: EditorProjectSourceKind,
    open_capability: EditorProjectSourceOpenCapability,
}

impl EditorProjectBrowserEntry {
    /// Creates a source fact from a Host-owned source-index projection.
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        relative_path: Arc<str>,
        kind: EditorProjectSourceKind,
        open_capability: EditorProjectSourceOpenCapability,
    ) -> Self {
        Self {
            relative_path,
            kind,
            open_capability,
        }
    }

    #[must_use]
    pub fn relative_path(&self) -> &str {
        &self.relative_path
    }

    #[must_use]
    pub const fn kind(&self) -> EditorProjectSourceKind {
        self.kind
    }

    #[must_use]
    pub const fn open_capability(&self) -> EditorProjectSourceOpenCapability {
        self.open_capability
    }
}

/// Closed diagnostic phase that does not expose filesystem internals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorProjectBrowserDiagnosticPhase {
    RootNormalization,
    Enumeration,
    LineageRevalidation,
    Publication,
}

/// Safe diagnostic projected from the Host source-index transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorProjectBrowserDiagnosticView {
    code: &'static str,
    phase: EditorProjectBrowserDiagnosticPhase,
    relative_source: Option<Arc<str>>,
    summary: &'static str,
    impact: &'static str,
    repair_hint: &'static str,
}

impl EditorProjectBrowserDiagnosticView {
    /// Creates a closed diagnostic from Host-owned fixed strings and a bounded relative source.
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        code: &'static str,
        phase: EditorProjectBrowserDiagnosticPhase,
        relative_source: Option<Arc<str>>,
        summary: &'static str,
        impact: &'static str,
        repair_hint: &'static str,
    ) -> Self {
        Self {
            code,
            phase,
            relative_source,
            summary,
            impact,
            repair_hint,
        }
    }

    #[must_use]
    pub const fn code(&self) -> &'static str {
        self.code
    }

    #[must_use]
    pub const fn phase(&self) -> EditorProjectBrowserDiagnosticPhase {
        self.phase
    }

    #[must_use]
    pub fn relative_source(&self) -> Option<&str> {
        self.relative_source.as_deref()
    }

    #[must_use]
    pub const fn summary(&self) -> &'static str {
        self.summary
    }

    #[must_use]
    pub const fn impact(&self) -> &'static str {
        self.impact
    }

    #[must_use]
    pub const fn repair_hint(&self) -> &'static str {
        self.repair_hint
    }
}

/// Numeric resource facts for one completed or failed source-index attempt.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditorProjectBrowserBudgetUsage {
    entries: usize,
    directories: usize,
    max_depth: usize,
    path_bytes: usize,
    diagnostics: usize,
    diagnostic_bytes: usize,
    retained_snapshot_bytes: usize,
}

impl EditorProjectBrowserBudgetUsage {
    #[allow(clippy::too_many_arguments)]
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        entries: usize,
        directories: usize,
        max_depth: usize,
        path_bytes: usize,
        diagnostics: usize,
        diagnostic_bytes: usize,
        retained_snapshot_bytes: usize,
    ) -> Self {
        Self {
            entries,
            directories,
            max_depth,
            path_bytes,
            diagnostics,
            diagnostic_bytes,
            retained_snapshot_bytes,
        }
    }

    #[must_use]
    pub const fn entries(self) -> usize {
        self.entries
    }

    #[must_use]
    pub const fn directories(self) -> usize {
        self.directories
    }

    #[must_use]
    pub const fn max_depth(self) -> usize {
        self.max_depth
    }

    #[must_use]
    pub const fn path_bytes(self) -> usize {
        self.path_bytes
    }

    #[must_use]
    pub const fn diagnostics(self) -> usize {
        self.diagnostics
    }

    #[must_use]
    pub const fn diagnostic_bytes(self) -> usize {
        self.diagnostic_bytes
    }

    #[must_use]
    pub const fn retained_snapshot_bytes(self) -> usize {
        self.retained_snapshot_bytes
    }
}

/// Bounded current-attempt facts for a synchronous source-index scan.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorProjectBrowserScanProgress {
    phase: EditorProjectBrowserDiagnosticPhase,
    usage: EditorProjectBrowserBudgetUsage,
}

impl EditorProjectBrowserScanProgress {
    #[doc(hidden)]
    #[must_use]
    pub const fn new(
        phase: EditorProjectBrowserDiagnosticPhase,
        usage: EditorProjectBrowserBudgetUsage,
    ) -> Self {
        Self { phase, usage }
    }

    #[must_use]
    pub const fn phase(self) -> EditorProjectBrowserDiagnosticPhase {
        self.phase
    }

    #[must_use]
    pub const fn usage(self) -> EditorProjectBrowserBudgetUsage {
        self.usage
    }
}

/// Immutable, generation-bound source-index projection used by every Editor Adapter.
#[derive(Debug, PartialEq, Eq)]
pub struct EditorProjectBrowserSnapshot {
    generation: u64,
    entries: Box<[EditorProjectBrowserEntry]>,
    diagnostics: Box<[EditorProjectBrowserDiagnosticView]>,
    usage: EditorProjectBrowserBudgetUsage,
}

impl EditorProjectBrowserSnapshot {
    /// Creates a deterministic projection from Host-owned safe facts.
    #[doc(hidden)]
    #[must_use]
    pub fn new(
        generation: u64,
        entries: impl IntoIterator<Item = EditorProjectBrowserEntry>,
        diagnostics: impl IntoIterator<Item = EditorProjectBrowserDiagnosticView>,
        usage: EditorProjectBrowserBudgetUsage,
    ) -> Self {
        let mut entries = entries.into_iter().collect::<Vec<_>>();
        entries.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));
        Self::from_sorted(generation, entries, diagnostics, usage)
    }

    /// Creates a projection from Host-owned entries already in canonical lexical order.
    #[doc(hidden)]
    #[must_use]
    pub fn from_sorted(
        generation: u64,
        entries: impl IntoIterator<Item = EditorProjectBrowserEntry>,
        diagnostics: impl IntoIterator<Item = EditorProjectBrowserDiagnosticView>,
        usage: EditorProjectBrowserBudgetUsage,
    ) -> Self {
        let entries = entries.into_iter().collect::<Box<[_]>>();
        debug_assert!(
            entries
                .windows(2)
                .all(|pair| pair[0].relative_path <= pair[1].relative_path),
            "Host Project Browser entries must already be in canonical lexical order"
        );
        Self {
            generation,
            entries,
            diagnostics: diagnostics.into_iter().collect(),
            usage,
        }
    }

    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    pub fn entries(&self) -> &[EditorProjectBrowserEntry] {
        &self.entries
    }

    #[must_use]
    pub fn diagnostics(&self) -> &[EditorProjectBrowserDiagnosticView] {
        &self.diagnostics
    }

    #[must_use]
    pub const fn usage(&self) -> EditorProjectBrowserBudgetUsage {
        self.usage
    }

    #[must_use]
    pub fn entry(&self, relative_path: &str) -> Option<&EditorProjectBrowserEntry> {
        self.entries
            .binary_search_by(|entry| entry.relative_path().cmp(relative_path))
            .ok()
            .map(|index| &self.entries[index])
    }
}

/// Frame-bound Project Browser action handled by the concrete Editor Host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorProjectBrowserAction {
    Refresh,
    Open { relative_path: Arc<str> },
}

/// Typed Host result for one accepted Project Browser action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorProjectBrowserActionOutcome {
    RefreshRequested,
    RefreshAlreadyPending,
    Activated { document: EditorDocumentId },
    Opened { document: EditorDocumentId },
    AwaitingClose { closing_document: EditorDocumentId },
    Rejected { code: &'static str },
}
