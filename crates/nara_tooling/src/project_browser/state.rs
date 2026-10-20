//! Mutable Project Browser query state and its immutable frame projection.

use std::{collections::BTreeSet, sync::Arc};

use super::{
    EditorProjectBrowserDiagnosticView, EditorProjectBrowserEntry,
    EditorProjectBrowserScanProgress, EditorProjectBrowserSnapshot, EditorProjectBrowserStatus,
    EditorProjectSourceKind, EditorProjectSourceOpenCapability,
};

/// Coarse filter applied by the UI-neutral row projection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EditorProjectBrowserKindFilter {
    #[default]
    All,
    Documents,
    Assets,
    Scripts,
    Directories,
    Other,
}

impl EditorProjectBrowserKindFilter {
    fn matches(self, kind: EditorProjectSourceKind) -> bool {
        match self {
            Self::All => true,
            Self::Documents => matches!(
                kind,
                EditorProjectSourceKind::Scene | EditorProjectSourceKind::Prefab
            ),
            Self::Assets => matches!(
                kind,
                EditorProjectSourceKind::AssetSource | EditorProjectSourceKind::AssetMeta
            ),
            Self::Scripts => kind == EditorProjectSourceKind::Script,
            Self::Directories => kind == EditorProjectSourceKind::Directory,
            Self::Other => matches!(
                kind,
                EditorProjectSourceKind::ProjectManifest
                    | EditorProjectSourceKind::AuthoringSchemaBundle
                    | EditorProjectSourceKind::Unknown
                    | EditorProjectSourceKind::Unsupported
            ),
        }
    }
}

/// One cached, flattened row consumed by the Project Browser Adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorProjectBrowserRow {
    relative_path: Arc<str>,
    kind: EditorProjectSourceKind,
    open_capability: EditorProjectSourceOpenCapability,
    depth: usize,
    has_children: bool,
    expanded: bool,
}

impl EditorProjectBrowserRow {
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

    #[must_use]
    pub const fn depth(&self) -> usize {
        self.depth
    }

    #[must_use]
    pub const fn has_children(&self) -> bool {
        self.has_children
    }

    #[must_use]
    pub const fn expanded(&self) -> bool {
        self.expanded
    }
}

/// Pending Browser work that requires another maintenance advance.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EditorProjectBrowserPendingWork {
    #[default]
    None,
    Scan,
    Query,
}

impl EditorProjectBrowserPendingWork {
    #[must_use]
    pub const fn is_pending(self) -> bool {
        !matches!(self, Self::None)
    }
}

/// Owned, cheap-to-clone browser facts published in one immutable Editor frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorProjectBrowserView {
    status: EditorProjectBrowserStatus,
    snapshot: Option<Arc<EditorProjectBrowserSnapshot>>,
    selected_path: Option<Arc<str>>,
    path_filter: Arc<str>,
    kind_filter: EditorProjectBrowserKindFilter,
    rows: Arc<Vec<EditorProjectBrowserRow>>,
    failure: Option<EditorProjectBrowserDiagnosticView>,
    scan_progress: Option<EditorProjectBrowserScanProgress>,
    pending_work: EditorProjectBrowserPendingWork,
    work_observed: bool,
}

impl EditorProjectBrowserView {
    #[must_use]
    pub const fn status(&self) -> EditorProjectBrowserStatus {
        self.status
    }

    #[must_use]
    pub fn snapshot(&self) -> Option<&EditorProjectBrowserSnapshot> {
        self.snapshot.as_deref()
    }

    #[must_use]
    pub fn snapshot_generation(&self) -> Option<u64> {
        self.snapshot
            .as_deref()
            .map(EditorProjectBrowserSnapshot::generation)
    }

    #[must_use]
    pub fn entry(&self, relative_path: &str) -> Option<&EditorProjectBrowserEntry> {
        self.snapshot.as_deref()?.entry(relative_path)
    }

    #[must_use]
    pub fn selected_path(&self) -> Option<&str> {
        self.selected_path.as_deref()
    }

    #[must_use]
    pub fn selected_entry(&self) -> Option<&EditorProjectBrowserEntry> {
        self.entry(self.selected_path()?)
    }

    #[must_use]
    pub fn path_filter(&self) -> &str {
        &self.path_filter
    }

    #[must_use]
    pub const fn kind_filter(&self) -> EditorProjectBrowserKindFilter {
        self.kind_filter
    }

    #[must_use]
    pub fn rows(&self) -> &[EditorProjectBrowserRow] {
        self.rows.as_slice()
    }

    #[must_use]
    pub const fn failure(&self) -> Option<&EditorProjectBrowserDiagnosticView> {
        self.failure.as_ref()
    }

    #[must_use]
    pub const fn scan_progress(&self) -> Option<EditorProjectBrowserScanProgress> {
        self.scan_progress
    }

    #[must_use]
    pub const fn pending_work(&self) -> EditorProjectBrowserPendingWork {
        self.pending_work
    }

    #[must_use]
    pub const fn has_pending_work(&self) -> bool {
        self.pending_work.is_pending()
    }

    /// Returns whether Browser preparation, query or Open facts changed for this frame.
    #[must_use]
    pub const fn work_observed(&self) -> bool {
        self.work_observed
    }
}

impl Default for EditorProjectBrowserView {
    fn default() -> Self {
        EditorProjectBrowserState::new().view()
    }
}

/// Tooling-local interaction that never crosses the Host action boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorProjectBrowserLocalAction {
    Select {
        relative_path: Option<Arc<str>>,
    },
    SetPathFilter {
        value: Arc<str>,
    },
    SetKindFilter {
        filter: EditorProjectBrowserKindFilter,
    },
    ToggleExpanded {
        relative_path: Arc<str>,
    },
}

/// Result of one bounded query slice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditorProjectBrowserQueryAdvance {
    processed_entries: usize,
    pending: bool,
}

impl EditorProjectBrowserQueryAdvance {
    #[must_use]
    pub const fn processed_entries(self) -> usize {
        self.processed_entries
    }

    #[must_use]
    pub const fn pending(self) -> bool {
        self.pending
    }
}

/// Mutable UI-neutral browser state owned by one Editor project session.
#[doc(hidden)]
#[derive(Debug)]
pub struct EditorProjectBrowserState {
    status: EditorProjectBrowserStatus,
    snapshot: Option<Arc<EditorProjectBrowserSnapshot>>,
    selected_path: Option<Arc<str>>,
    path_filter: Arc<str>,
    kind_filter: EditorProjectBrowserKindFilter,
    expanded_paths: BTreeSet<Arc<str>>,
    rows: Arc<Vec<EditorProjectBrowserRow>>,
    failure: Option<EditorProjectBrowserDiagnosticView>,
    scan_progress: Option<EditorProjectBrowserScanProgress>,
    scan_pending: bool,
    work_observed: bool,
    query: Option<EditorProjectBrowserQuery>,
}

impl EditorProjectBrowserState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            status: EditorProjectBrowserStatus::NotRequested,
            snapshot: None,
            selected_path: None,
            path_filter: Arc::from(""),
            kind_filter: EditorProjectBrowserKindFilter::All,
            expanded_paths: BTreeSet::new(),
            rows: Arc::new(Vec::new()),
            failure: None,
            scan_progress: None,
            scan_pending: false,
            work_observed: false,
            query: None,
        }
    }

    /// Synchronizes Host publication state without copying native authority into the model.
    pub fn synchronize(
        &mut self,
        status: EditorProjectBrowserStatus,
        snapshot: Option<Arc<EditorProjectBrowserSnapshot>>,
        failure: Option<EditorProjectBrowserDiagnosticView>,
        scan_progress: Option<EditorProjectBrowserScanProgress>,
        scan_pending: bool,
        scan_work_observed: bool,
    ) {
        let snapshot_changed = match (&self.snapshot, &snapshot) {
            (Some(current), Some(next)) => !Arc::ptr_eq(current, next),
            (None, None) => false,
            _ => true,
        };
        self.status = status;
        self.failure = failure;
        self.scan_progress = scan_progress;
        self.scan_pending = scan_pending;
        self.work_observed = scan_work_observed;
        if !snapshot_changed {
            return;
        }
        self.snapshot = snapshot;
        if self
            .selected_path
            .as_deref()
            .is_some_and(|path| self.snapshot_entry(path).is_none())
        {
            self.selected_path = None;
        }
        let snapshot = self.snapshot.as_ref().map(Arc::clone);
        self.expanded_paths.retain(|path| {
            snapshot
                .as_deref()
                .and_then(|snapshot| snapshot.entry(path))
                .is_some_and(|entry| entry.kind() == EditorProjectSourceKind::Directory)
        });
        self.begin_query();
    }

    #[must_use]
    pub fn view(&self) -> EditorProjectBrowserView {
        EditorProjectBrowserView {
            status: self.status,
            snapshot: self.snapshot.as_ref().map(Arc::clone),
            selected_path: self.selected_path.as_ref().map(Arc::clone),
            path_filter: Arc::clone(&self.path_filter),
            kind_filter: self.kind_filter,
            rows: Arc::clone(&self.rows),
            failure: self.failure.clone(),
            scan_progress: self.scan_progress,
            pending_work: self.pending_work(),
            work_observed: self.work_observed,
        }
    }

    #[must_use]
    pub const fn query_work_pending(&self) -> bool {
        self.query.is_some()
    }

    #[must_use]
    pub const fn pending_work(&self) -> EditorProjectBrowserPendingWork {
        if self.scan_pending {
            EditorProjectBrowserPendingWork::Scan
        } else if self.query.is_some() {
            EditorProjectBrowserPendingWork::Query
        } else {
            EditorProjectBrowserPendingWork::None
        }
    }

    /// Applies one local selection/query action and returns whether visible model state changed.
    pub fn apply_local_action(&mut self, action: EditorProjectBrowserLocalAction) -> bool {
        match action {
            EditorProjectBrowserLocalAction::Select { relative_path } => {
                if relative_path
                    .as_deref()
                    .is_some_and(|path| self.snapshot_entry(path).is_none())
                    || self.selected_path == relative_path
                {
                    return false;
                }
                self.selected_path = relative_path;
                true
            }
            EditorProjectBrowserLocalAction::SetPathFilter { value } => {
                if self.path_filter == value {
                    return false;
                }
                self.path_filter = value;
                self.begin_query();
                true
            }
            EditorProjectBrowserLocalAction::SetKindFilter { filter } => {
                if self.kind_filter == filter {
                    return false;
                }
                self.kind_filter = filter;
                self.begin_query();
                true
            }
            EditorProjectBrowserLocalAction::ToggleExpanded { relative_path } => {
                if !self
                    .snapshot_entry(&relative_path)
                    .is_some_and(|entry| entry.kind() == EditorProjectSourceKind::Directory)
                {
                    return false;
                }
                if !self.expanded_paths.remove(&relative_path) {
                    self.expanded_paths.insert(relative_path);
                }
                self.begin_query();
                true
            }
        }
    }

    /// Advances at most `max_entries` source facts and atomically publishes completed rows.
    pub fn advance_query(&mut self, max_entries: usize) -> EditorProjectBrowserQueryAdvance {
        let Some(mut query) = self.query.take() else {
            return EditorProjectBrowserQueryAdvance::default();
        };
        let start = query.cursor;
        let end = query
            .cursor
            .saturating_add(max_entries)
            .min(query.snapshot.entries().len());
        while query.cursor < end {
            let index = query.cursor;
            let entry = &query.snapshot.entries()[index];
            query.cursor += 1;
            if !self.kind_filter.matches(entry.kind())
                || !path_matches_filter(entry.relative_path(), &self.path_filter)
                || !self.row_is_visible(entry.relative_path())
            {
                continue;
            }
            query.rows.push(EditorProjectBrowserRow {
                relative_path: Arc::clone(&entry.relative_path),
                kind: entry.kind,
                open_capability: entry.open_capability,
                depth: entry.relative_path().matches('/').count(),
                has_children: entry_has_children(&query.snapshot, index),
                expanded: self.expanded_paths.contains(&entry.relative_path),
            });
        }
        let processed_entries = end - start;
        self.work_observed |= processed_entries != 0;
        if query.cursor == query.snapshot.entries().len() {
            self.rows = Arc::new(query.rows);
            EditorProjectBrowserQueryAdvance {
                processed_entries,
                pending: false,
            }
        } else {
            self.query = Some(query);
            EditorProjectBrowserQueryAdvance {
                processed_entries,
                pending: true,
            }
        }
    }

    fn snapshot_entry(&self, relative_path: &str) -> Option<&EditorProjectBrowserEntry> {
        self.snapshot.as_deref()?.entry(relative_path)
    }

    fn begin_query(&mut self) {
        self.rows = Arc::new(Vec::new());
        self.query = self
            .snapshot
            .as_ref()
            .map(|snapshot| EditorProjectBrowserQuery {
                snapshot: Arc::clone(snapshot),
                cursor: 0,
                rows: Vec::new(),
            });
    }

    fn row_is_visible(&self, relative_path: &str) -> bool {
        if !self.path_filter.is_empty() {
            return true;
        }
        relative_path
            .match_indices('/')
            .all(|(separator, _)| self.expanded_paths.contains(&relative_path[..separator]))
    }
}

impl Default for EditorProjectBrowserState {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug)]
struct EditorProjectBrowserQuery {
    snapshot: Arc<EditorProjectBrowserSnapshot>,
    cursor: usize,
    rows: Vec<EditorProjectBrowserRow>,
}

fn path_matches_filter(relative_path: &str, filter: &str) -> bool {
    filter.is_empty()
        || relative_path
            .as_bytes()
            .windows(filter.len())
            .any(|candidate| candidate.eq_ignore_ascii_case(filter.as_bytes()))
}

fn entry_has_children(snapshot: &EditorProjectBrowserSnapshot, index: usize) -> bool {
    let entry = &snapshot.entries()[index];
    if entry.kind() != EditorProjectSourceKind::Directory {
        return false;
    }
    snapshot.entries().get(index + 1).is_some_and(|next| {
        next.relative_path()
            .strip_prefix(entry.relative_path())
            .is_some_and(|suffix| suffix.starts_with('/'))
    })
}
