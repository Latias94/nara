use std::sync::Arc;

use nara_tooling::{
    EditorProjectBrowserAction, EditorProjectBrowserActionOutcome, EditorProjectBrowserBudgetUsage,
    EditorProjectBrowserDiagnosticPhase, EditorProjectBrowserDiagnosticView,
    EditorProjectBrowserEntry, EditorProjectBrowserLocalAction, EditorProjectBrowserQueryAdvance,
    EditorProjectBrowserScanProgress, EditorProjectBrowserSnapshot, EditorProjectBrowserState,
    EditorProjectBrowserStatus, EditorProjectBrowserView, EditorProjectSourceInspectReason,
    EditorProjectSourceKind, EditorProjectSourceOpenCapability,
};

use crate::project_host::authoring::{
    AuthoringDocumentKind, ProjectSourceIndexDiagnostic, ProjectSourceIndexDiagnosticPhase,
    ProjectSourceIndexEntry, ProjectSourceIndexFailure, ProjectSourceIndexProgress,
    ProjectSourceIndexSnapshot, ProjectSourceIndexStateKind, ProjectSourceInspectReason,
    ProjectSourceKind, ProjectSourceOpenCapability,
};

use super::{EditorDocumentOpenRequestResult, EditorProjectSession};

const QUERY_ENTRIES_PER_FRAME: usize = 256;

pub(super) struct EditorProjectBrowserHost {
    state: EditorProjectBrowserState,
    projection: Option<EditorProjectBrowserProjection>,
}

struct EditorProjectBrowserProjection {
    source: Arc<ProjectSourceIndexSnapshot>,
    safe: Arc<EditorProjectBrowserSnapshot>,
}

impl EditorProjectBrowserHost {
    pub(super) fn new() -> Self {
        Self {
            state: EditorProjectBrowserState::new(),
            projection: None,
        }
    }

    pub(super) fn synchronize(
        &mut self,
        kind: ProjectSourceIndexStateKind,
        snapshot: Option<Arc<ProjectSourceIndexSnapshot>>,
        failure: Option<ProjectSourceIndexFailure>,
        progress: Option<ProjectSourceIndexProgress>,
        scan_pending: bool,
        scan_work_observed: bool,
    ) -> EditorProjectBrowserQueryAdvance {
        let safe_snapshot = snapshot
            .as_ref()
            .map(|snapshot| self.project_snapshot(snapshot));
        self.state.synchronize(
            project_status(kind),
            safe_snapshot,
            failure.as_ref().map(project_failure),
            progress.map(project_progress),
            scan_pending,
            scan_work_observed,
        );
        self.state.advance_query(QUERY_ENTRIES_PER_FRAME)
    }

    pub(super) fn synchronize_unavailable(&mut self) -> EditorProjectBrowserQueryAdvance {
        self.state.synchronize(
            EditorProjectBrowserStatus::Unavailable,
            None,
            Some(EditorProjectBrowserDiagnosticView::new(
                "project.source-index.authoring-project-required",
                EditorProjectBrowserDiagnosticPhase::Publication,
                None,
                "Project Browser indexing is unavailable for this project lane",
                "The trusted compiled lane keeps Runtime authority but has no generic source index.",
                "Open the project through the ordinary standalone Editor project flow.",
            )),
            None,
            false,
            false,
        );
        self.state.advance_query(QUERY_ENTRIES_PER_FRAME)
    }

    pub(super) fn view(&self) -> EditorProjectBrowserView {
        self.state.view()
    }

    pub(super) fn apply_local_action(&mut self, action: EditorProjectBrowserLocalAction) -> bool {
        self.state.apply_local_action(action)
    }

    fn project_snapshot(
        &mut self,
        source: &Arc<ProjectSourceIndexSnapshot>,
    ) -> Arc<EditorProjectBrowserSnapshot> {
        if let Some(projection) = &self.projection
            && Arc::ptr_eq(&projection.source, source)
        {
            return Arc::clone(&projection.safe);
        }
        let safe = Arc::new(EditorProjectBrowserSnapshot::from_sorted(
            source.generation().get(),
            source.entries().iter().map(project_entry),
            source.diagnostics().iter().map(project_diagnostic),
            project_usage(source.usage()),
        ));
        self.projection = Some(EditorProjectBrowserProjection {
            source: Arc::clone(source),
            safe: Arc::clone(&safe),
        });
        safe
    }
}

impl Default for EditorProjectBrowserHost {
    fn default() -> Self {
        Self::new()
    }
}

impl EditorProjectSession {
    pub(super) fn synchronize_project_browser(
        &mut self,
        scan_work_observed: bool,
    ) -> Option<Arc<ProjectSourceIndexSnapshot>> {
        let snapshot = self.project_source_index_snapshot();
        if self.authoring_project().is_some() {
            let kind = self.project_source_index_kind();
            let failure = self.project_source_index_failure().cloned();
            let progress = self.project_source_index_progress();
            let scan_pending = self.project_source_index_work_pending();
            self.project_browser.synchronize(
                kind,
                snapshot.as_ref().map(Arc::clone),
                failure,
                progress,
                scan_pending,
                scan_work_observed,
            );
        } else {
            self.project_browser.synchronize_unavailable();
        }
        snapshot
    }

    /// Applies one Project Browser interaction that is intentionally outside frame-bound Host
    /// actions. The next published frame carries the resulting query state.
    #[doc(hidden)]
    pub fn apply_project_browser_local_action(
        &mut self,
        action: EditorProjectBrowserLocalAction,
    ) -> bool {
        self.project_browser.apply_local_action(action)
    }

    pub(super) fn dispatch_project_browser_action(
        &mut self,
        frame: &nara_tooling::EditorFrameModel,
        source_index: Option<&Arc<ProjectSourceIndexSnapshot>>,
        action: EditorProjectBrowserAction,
    ) -> EditorProjectBrowserActionOutcome {
        match action {
            EditorProjectBrowserAction::Refresh => {
                if self.authoring_project().is_none() {
                    return rejected("project.editor.browser-refresh-requires-authoring-project");
                }
                if self.project_source_index_work_pending() {
                    return EditorProjectBrowserActionOutcome::RefreshAlreadyPending;
                }
                if self.request_project_source_index_refresh() {
                    EditorProjectBrowserActionOutcome::RefreshRequested
                } else {
                    rejected("project.editor.browser-refresh-unavailable")
                }
            }
            EditorProjectBrowserAction::Open { relative_path } => {
                let Some(index) = source_index else {
                    return rejected("project.editor.document-index-unavailable");
                };
                if frame.project_browser().snapshot_generation() != Some(index.generation().get()) {
                    return rejected("project.editor.document-index-stale");
                }
                let Some(frame_entry) = frame.project_browser().entry(&relative_path) else {
                    return rejected("project.editor.document-not-indexed");
                };
                if !frame_entry.open_capability().is_document() {
                    return rejected("project.editor.document-source-not-openable");
                }
                let Some(entry) = index.entry(&relative_path) else {
                    return rejected("project.editor.document-not-indexed");
                };
                let kind = match entry.open_capability() {
                    ProjectSourceOpenCapability::Scene => AuthoringDocumentKind::Scene,
                    ProjectSourceOpenCapability::Prefab => AuthoringDocumentKind::Prefab,
                    ProjectSourceOpenCapability::InspectOnly(_)
                    | ProjectSourceOpenCapability::None => {
                        return rejected("project.editor.document-source-not-openable");
                    }
                };
                map_open_result(self.request_indexed_authoring_document(
                    Arc::clone(index),
                    &relative_path,
                    kind,
                ))
            }
            EditorProjectBrowserAction::CancelOpen { request } => {
                if self.cancel_document_open_request(request) {
                    EditorProjectBrowserActionOutcome::OpenCancellationRequested { request }
                } else {
                    rejected("project.editor.document-open-cancel-stale")
                }
            }
            EditorProjectBrowserAction::AcknowledgeOpen { request } => {
                if self.acknowledge_document_open(request) {
                    EditorProjectBrowserActionOutcome::OpenAcknowledged { request }
                } else {
                    rejected("project.editor.document-open-acknowledgement-stale")
                }
            }
        }
    }
}

fn project_status(kind: ProjectSourceIndexStateKind) -> EditorProjectBrowserStatus {
    match kind {
        ProjectSourceIndexStateKind::NotRequested => EditorProjectBrowserStatus::NotRequested,
        ProjectSourceIndexStateKind::Scanning => EditorProjectBrowserStatus::Scanning,
        ProjectSourceIndexStateKind::Ready => EditorProjectBrowserStatus::Ready,
        ProjectSourceIndexStateKind::ReadyWithDiagnostics => {
            EditorProjectBrowserStatus::ReadyWithDiagnostics
        }
        ProjectSourceIndexStateKind::Unavailable => EditorProjectBrowserStatus::Unavailable,
    }
}

fn project_entry(entry: &ProjectSourceIndexEntry) -> EditorProjectBrowserEntry {
    EditorProjectBrowserEntry::new(
        Arc::clone(entry.relative_path_arc()),
        match entry.kind() {
            ProjectSourceKind::ProjectManifest => EditorProjectSourceKind::ProjectManifest,
            ProjectSourceKind::AuthoringSchemaBundle => {
                EditorProjectSourceKind::AuthoringSchemaBundle
            }
            ProjectSourceKind::Scene => EditorProjectSourceKind::Scene,
            ProjectSourceKind::Prefab => EditorProjectSourceKind::Prefab,
            ProjectSourceKind::AssetSource => EditorProjectSourceKind::AssetSource,
            ProjectSourceKind::AssetMeta => EditorProjectSourceKind::AssetMeta,
            ProjectSourceKind::Script => EditorProjectSourceKind::Script,
            ProjectSourceKind::Directory => EditorProjectSourceKind::Directory,
            ProjectSourceKind::Unknown => EditorProjectSourceKind::Unknown,
            ProjectSourceKind::Unsupported => EditorProjectSourceKind::Unsupported,
        },
        match entry.open_capability() {
            ProjectSourceOpenCapability::Scene => EditorProjectSourceOpenCapability::Scene,
            ProjectSourceOpenCapability::Prefab => EditorProjectSourceOpenCapability::Prefab,
            ProjectSourceOpenCapability::InspectOnly(reason) => {
                EditorProjectSourceOpenCapability::InspectOnly(project_inspect_reason(reason))
            }
            ProjectSourceOpenCapability::None => EditorProjectSourceOpenCapability::None,
        },
    )
}

fn project_inspect_reason(reason: ProjectSourceInspectReason) -> EditorProjectSourceInspectReason {
    match reason {
        ProjectSourceInspectReason::ProjectAuthority => {
            EditorProjectSourceInspectReason::ProjectAuthority
        }
        ProjectSourceInspectReason::ReadOnlySource => {
            EditorProjectSourceInspectReason::ReadOnlySource
        }
        ProjectSourceInspectReason::Directory => EditorProjectSourceInspectReason::Directory,
        ProjectSourceInspectReason::UnknownSourceKind => {
            EditorProjectSourceInspectReason::UnknownSourceKind
        }
        ProjectSourceInspectReason::MissingDeclaredRoot => {
            EditorProjectSourceInspectReason::MissingDeclaredRoot
        }
        ProjectSourceInspectReason::UnavailableDeclaredRoot => {
            EditorProjectSourceInspectReason::UnavailableDeclaredRoot
        }
        ProjectSourceInspectReason::UnsupportedEntry => {
            EditorProjectSourceInspectReason::UnsupportedEntry
        }
        ProjectSourceInspectReason::IdentityUnavailable => {
            EditorProjectSourceInspectReason::IdentityUnavailable
        }
    }
}

fn project_diagnostic(
    diagnostic: &ProjectSourceIndexDiagnostic,
) -> EditorProjectBrowserDiagnosticView {
    EditorProjectBrowserDiagnosticView::new(
        diagnostic.code().as_str(),
        project_phase(diagnostic.phase()),
        diagnostic.relative_source().map(Arc::from),
        diagnostic.summary(),
        diagnostic.impact(),
        diagnostic.repair_hint(),
    )
}

fn project_progress(progress: ProjectSourceIndexProgress) -> EditorProjectBrowserScanProgress {
    EditorProjectBrowserScanProgress::new(
        project_phase(progress.phase()),
        project_usage(progress.usage()),
    )
}

const fn project_phase(
    phase: ProjectSourceIndexDiagnosticPhase,
) -> EditorProjectBrowserDiagnosticPhase {
    match phase {
        ProjectSourceIndexDiagnosticPhase::RootNormalization => {
            EditorProjectBrowserDiagnosticPhase::RootNormalization
        }
        ProjectSourceIndexDiagnosticPhase::Enumeration => {
            EditorProjectBrowserDiagnosticPhase::Enumeration
        }
        ProjectSourceIndexDiagnosticPhase::LineageRevalidation => {
            EditorProjectBrowserDiagnosticPhase::LineageRevalidation
        }
        ProjectSourceIndexDiagnosticPhase::Publication => {
            EditorProjectBrowserDiagnosticPhase::Publication
        }
    }
}

fn project_failure(failure: &ProjectSourceIndexFailure) -> EditorProjectBrowserDiagnosticView {
    project_diagnostic(failure.diagnostic())
}

fn project_usage(
    usage: crate::project_host::authoring::ProjectSourceIndexBudgetUsage,
) -> EditorProjectBrowserBudgetUsage {
    EditorProjectBrowserBudgetUsage::new(
        usage.entries(),
        usage.directories(),
        usage.max_depth(),
        usage.path_bytes(),
        usage.diagnostics(),
        usage.diagnostic_bytes(),
        usage.retained_snapshot_bytes(),
    )
}

fn map_open_result(result: EditorDocumentOpenRequestResult) -> EditorProjectBrowserActionOutcome {
    match result {
        EditorDocumentOpenRequestResult::Preparing { request } => {
            EditorProjectBrowserActionOutcome::Preparing { request }
        }
        EditorDocumentOpenRequestResult::Activated { document } => {
            EditorProjectBrowserActionOutcome::Activated { document }
        }
        #[cfg(test)]
        EditorDocumentOpenRequestResult::Opened { .. } => {
            unreachable!("the product request always prepares before publication")
        }
        EditorDocumentOpenRequestResult::Rejected { code } => {
            EditorProjectBrowserActionOutcome::Rejected { code }
        }
    }
}

const fn rejected(code: &'static str) -> EditorProjectBrowserActionOutcome {
    EditorProjectBrowserActionOutcome::Rejected { code }
}
