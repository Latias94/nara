use std::sync::Arc;

use egui::{Button, ComboBox, RichText, ScrollArea, TextEdit, Ui};
use nara_tooling::{
    EditorAction, EditorActionBatchReport, EditorActionOutcome, EditorActionRejection,
    EditorImageImportView, EditorProjectBrowserAction, EditorProjectBrowserActionOutcome,
    EditorProjectBrowserKindFilter, EditorProjectBrowserLocalAction,
    EditorProjectBrowserPendingWork, EditorProjectBrowserStatus, EditorProjectBrowserView,
    EditorProjectSourceInspectReason, EditorProjectSourceKind, EditorProjectSourceOpenCapability,
};

use crate::image_import::EguiImageImportPanel;

const MAX_RENDERED_ROWS_PER_FRAME: usize = 256;
const MAX_RENDERED_DIAGNOSTICS: usize = 32;

#[cfg(test)]
use std::{cell::RefCell, collections::HashMap};

#[cfg(test)]
thread_local! {
    static TEST_CONTROL_RECTS: RefCell<HashMap<String, egui::Rect>> =
        RefCell::new(HashMap::new());
    static TEST_RENDERED_ROWS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static TEST_STATUS_LABEL: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Stateful egui Adapter for the UI-neutral Project Browser model.
#[derive(Debug, Default)]
pub(crate) struct EguiProjectBrowserPanel {
    filter_draft: String,
    notice: Option<&'static str>,
    image_import: EguiImageImportPanel,
}

impl EguiProjectBrowserPanel {
    #[cfg(test)]
    #[must_use]
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn show_tree(
        &mut self,
        ui: &mut Ui,
        view: &EditorProjectBrowserView,
        host_actions_enabled: bool,
        actions: &mut Vec<EditorAction>,
        local_actions: &mut Vec<EditorProjectBrowserLocalAction>,
    ) {
        #[cfg(test)]
        clear_test_observations();

        if !ui.memory(|memory| memory.has_focus(ui.id().with("project_browser_filter")))
            && self.filter_draft != view.path_filter()
        {
            view.path_filter().clone_into(&mut self.filter_draft);
        }

        ui.horizontal(|ui| {
            let status = status_label(view.status());
            #[cfg(test)]
            TEST_STATUS_LABEL.with(|label| *label.borrow_mut() = Some(status.to_owned()));
            ui.label(RichText::new(status).strong());
            let refresh = ui.add_enabled(host_actions_enabled, Button::new("Refresh"));
            #[cfg(test)]
            record_test_control("Project Refresh", refresh.rect);
            if refresh.clicked() {
                actions.push(EditorAction::ProjectBrowser(
                    EditorProjectBrowserAction::Refresh,
                ));
            }
            if let Some(generation) = view.snapshot_generation() {
                let prefix = if matches!(
                    view.status(),
                    EditorProjectBrowserStatus::Scanning | EditorProjectBrowserStatus::Unavailable
                ) {
                    "Last-good generation"
                } else {
                    "Generation"
                };
                ui.label(RichText::new(format!("{prefix} {generation}")).weak());
            }
        });

        if let Some(progress) = view.scan_progress() {
            let usage = progress.usage();
            ui.label(
                RichText::new(format!(
                    "{} · {} entries · {} directories",
                    diagnostic_phase_label(progress.phase()),
                    usage.entries(),
                    usage.directories(),
                ))
                .weak(),
            );
        }

        let filter = ui.add(
            TextEdit::singleline(&mut self.filter_draft)
                .id_source("project_browser_filter")
                .hint_text("Filter project sources"),
        );
        #[cfg(test)]
        record_test_control("Project Filter", filter.rect);
        if filter.changed() {
            local_actions.push(EditorProjectBrowserLocalAction::SetPathFilter {
                value: Arc::from(self.filter_draft.as_str()),
            });
        }

        let mut selected_filter = view.kind_filter();
        ComboBox::from_id_salt("nara_project_browser_kind_filter")
            .selected_text(kind_filter_label(selected_filter))
            .show_ui(ui, |ui| {
                for filter in [
                    EditorProjectBrowserKindFilter::All,
                    EditorProjectBrowserKindFilter::Documents,
                    EditorProjectBrowserKindFilter::Assets,
                    EditorProjectBrowserKindFilter::Scripts,
                    EditorProjectBrowserKindFilter::Directories,
                    EditorProjectBrowserKindFilter::Other,
                ] {
                    ui.selectable_value(&mut selected_filter, filter, kind_filter_label(filter));
                }
            });
        if selected_filter != view.kind_filter() {
            local_actions.push(EditorProjectBrowserLocalAction::SetKindFilter {
                filter: selected_filter,
            });
        }

        if view.pending_work() == EditorProjectBrowserPendingWork::Query {
            ui.label(RichText::new("Updating filtered rows…").weak());
        }
        if view.rows().is_empty() && !view.has_pending_work() {
            ui.label(RichText::new(empty_label(view.status())).weak());
        }

        let row_height = ui.spacing().interact_size.y;
        ScrollArea::vertical()
            .id_salt("nara_editor_project_browser_rows")
            .auto_shrink([false, false])
            .show_rows(ui, row_height, view.rows().len(), |ui, range| {
                for row in view.rows()[range].iter().take(MAX_RENDERED_ROWS_PER_FRAME) {
                    ui.horizontal(|ui| {
                        ui.add_space(row.depth() as f32 * 12.0);
                        if row.has_children() {
                            let toggle =
                                ui.small_button(if row.expanded() { "▾" } else { "▸" });
                            if toggle.clicked() {
                                local_actions.push(
                                    EditorProjectBrowserLocalAction::ToggleExpanded {
                                        relative_path: Arc::from(row.relative_path()),
                                    },
                                );
                            }
                        } else {
                            ui.add_space(18.0);
                        }
                        let selected = view.selected_path() == Some(row.relative_path());
                        let response = ui.selectable_label(
                            selected,
                            format!("{}  {}", source_kind_badge(row.kind()), row.relative_path()),
                        );
                        #[cfg(test)]
                        {
                            record_test_control(row.relative_path(), response.rect);
                            TEST_RENDERED_ROWS.with(|rows| {
                                rows.borrow_mut().push(row.relative_path().to_owned())
                            });
                        }
                        if response.clicked() {
                            local_actions.push(EditorProjectBrowserLocalAction::Select {
                                relative_path: Some(Arc::from(row.relative_path())),
                            });
                        }
                    });
                }
            });
    }

    pub(crate) fn show_inspector(
        &mut self,
        ui: &mut Ui,
        view: &EditorProjectBrowserView,
        image_import: Option<&EditorImageImportView>,
        host_actions_enabled: bool,
        image_import_work_pending: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        if let Some(image_import) = eligible_image_import(view, image_import) {
            self.image_import.show(
                ui,
                image_import,
                host_actions_enabled && !image_import_work_pending,
                actions,
            );
        } else {
            self.image_import.clear();

            ui.heading("Project Source");
            if let Some(entry) = view.selected_entry() {
                ui.label(entry.relative_path());
                ui.label(format!("Kind: {}", source_kind_label(entry.kind())));
                match entry.open_capability() {
                    EditorProjectSourceOpenCapability::Scene
                    | EditorProjectSourceOpenCapability::Prefab => {
                        let open = ui.add_enabled(host_actions_enabled, Button::new("Open"));
                        #[cfg(test)]
                        record_test_control("Project Open", open.rect);
                        if open.clicked() {
                            actions.push(EditorAction::ProjectBrowser(
                                EditorProjectBrowserAction::Open {
                                    relative_path: Arc::from(entry.relative_path()),
                                },
                            ));
                        }
                    }
                    EditorProjectSourceOpenCapability::InspectOnly(reason) => {
                        ui.label(RichText::new(inspect_reason_label(reason)).weak());
                    }
                    EditorProjectSourceOpenCapability::None => {
                        ui.label(RichText::new("No operation is available for this source").weak());
                    }
                }
            } else {
                ui.label(RichText::new("Select a project source to inspect it").weak());
            }
        }

        if let Some(notice) = self.notice {
            ui.separator();
            ui.label(RichText::new(notice).color(ui.visuals().warn_fg_color));
        }
        if let Some(failure) = view.failure() {
            ui.separator();
            render_diagnostic(ui, failure);
        }
        if let Some(snapshot) = view.snapshot() {
            let usage = snapshot.usage();
            ui.separator();
            ui.label(format!(
                "Indexed {} entries in {} directories",
                usage.entries(),
                usage.directories(),
            ));
            ui.label(
                RichText::new(format!(
                    "Depth {} · path bytes {} · retained bytes {}",
                    usage.max_depth(),
                    usage.path_bytes(),
                    usage.retained_snapshot_bytes(),
                ))
                .weak(),
            );
            for diagnostic in snapshot.diagnostics().iter().take(MAX_RENDERED_DIAGNOSTICS) {
                ui.separator();
                render_diagnostic(ui, diagnostic);
            }
        }
    }

    pub(crate) fn observe_action_report(&mut self, report: &EditorActionBatchReport) {
        let Some(outcome) = report
            .outcomes()
            .iter()
            .rev()
            .find_map(|outcome| match outcome {
                EditorActionOutcome::ProjectBrowser(outcome) => Some(*outcome),
                _ => None,
            })
        else {
            return;
        };
        self.notice = Some(match outcome {
            EditorProjectBrowserActionOutcome::RefreshRequested => "Project refresh requested",
            EditorProjectBrowserActionOutcome::RefreshAlreadyPending => {
                "Project refresh is already running"
            }
            EditorProjectBrowserActionOutcome::Activated { .. } => "Project document activated",
            EditorProjectBrowserActionOutcome::Opened { .. } => "Project document opened",
            EditorProjectBrowserActionOutcome::AwaitingClose { .. } => {
                "Resolve the current dirty document before opening this source"
            }
            EditorProjectBrowserActionOutcome::Rejected { .. } => {
                "Project Browser action was safely rejected"
            }
        });
    }

    pub(crate) fn observe_action_rejection(&mut self, rejection: EditorActionRejection) {
        if matches!(
            rejection,
            EditorActionRejection::NoPublishedFrame | EditorActionRejection::StaleFrame { .. }
        ) {
            self.notice = Some("Project Browser action came from a stale Editor frame");
        }
    }
}

fn eligible_image_import<'view>(
    browser: &EditorProjectBrowserView,
    image_import: Option<&'view EditorImageImportView>,
) -> Option<&'view EditorImageImportView> {
    let entry = browser.selected_entry()?;
    image_import.filter(|image_import| {
        entry.kind() == EditorProjectSourceKind::AssetSource
            && entry.relative_path() == image_import.relative_path()
    })
}

fn diagnostic_phase_label(
    phase: nara_tooling::EditorProjectBrowserDiagnosticPhase,
) -> &'static str {
    match phase {
        nara_tooling::EditorProjectBrowserDiagnosticPhase::RootNormalization => "Normalizing roots",
        nara_tooling::EditorProjectBrowserDiagnosticPhase::Enumeration => "Enumerating sources",
        nara_tooling::EditorProjectBrowserDiagnosticPhase::LineageRevalidation => {
            "Revalidating directory lineage"
        }
        nara_tooling::EditorProjectBrowserDiagnosticPhase::Publication => "Publishing index",
    }
}

fn render_diagnostic(ui: &mut Ui, diagnostic: &nara_tooling::EditorProjectBrowserDiagnosticView) {
    ui.label(RichText::new(diagnostic.code()).strong());
    if let Some(source) = diagnostic.relative_source() {
        ui.label(source);
    }
    ui.label(diagnostic.summary());
    ui.label(RichText::new(diagnostic.impact()).weak());
    ui.label(RichText::new(diagnostic.repair_hint()).italics());
}

fn status_label(status: EditorProjectBrowserStatus) -> &'static str {
    match status {
        EditorProjectBrowserStatus::NotRequested => "Not indexed",
        EditorProjectBrowserStatus::Scanning => "Scanning",
        EditorProjectBrowserStatus::Ready => "Ready",
        EditorProjectBrowserStatus::ReadyWithDiagnostics => "Ready with diagnostics",
        EditorProjectBrowserStatus::Unavailable => "Unavailable",
    }
}

fn empty_label(status: EditorProjectBrowserStatus) -> &'static str {
    match status {
        EditorProjectBrowserStatus::NotRequested => "Refresh to build the project source index",
        EditorProjectBrowserStatus::Scanning => "Scanning project sources…",
        EditorProjectBrowserStatus::Ready | EditorProjectBrowserStatus::ReadyWithDiagnostics => {
            "No project sources match the current filter"
        }
        EditorProjectBrowserStatus::Unavailable => "Project source indexing is unavailable",
    }
}

fn kind_filter_label(filter: EditorProjectBrowserKindFilter) -> &'static str {
    match filter {
        EditorProjectBrowserKindFilter::All => "All sources",
        EditorProjectBrowserKindFilter::Documents => "Scenes and Prefabs",
        EditorProjectBrowserKindFilter::Assets => "Assets",
        EditorProjectBrowserKindFilter::Scripts => "Scripts",
        EditorProjectBrowserKindFilter::Directories => "Directories",
        EditorProjectBrowserKindFilter::Other => "Other",
    }
}

fn source_kind_badge(kind: EditorProjectSourceKind) -> &'static str {
    match kind {
        EditorProjectSourceKind::ProjectManifest => "MAN",
        EditorProjectSourceKind::AuthoringSchemaBundle => "SCH",
        EditorProjectSourceKind::Scene => "SCN",
        EditorProjectSourceKind::Prefab => "PFB",
        EditorProjectSourceKind::AssetSource => "AST",
        EditorProjectSourceKind::AssetMeta => "META",
        EditorProjectSourceKind::Script => "RS",
        EditorProjectSourceKind::Directory => "DIR",
        EditorProjectSourceKind::Unknown => "?",
        EditorProjectSourceKind::Unsupported => "!",
    }
}

fn source_kind_label(kind: EditorProjectSourceKind) -> &'static str {
    match kind {
        EditorProjectSourceKind::ProjectManifest => "Project manifest",
        EditorProjectSourceKind::AuthoringSchemaBundle => "Authoring schema bundle",
        EditorProjectSourceKind::Scene => "Scene",
        EditorProjectSourceKind::Prefab => "Prefab",
        EditorProjectSourceKind::AssetSource => "Asset source",
        EditorProjectSourceKind::AssetMeta => "Asset metadata",
        EditorProjectSourceKind::Script => "Script",
        EditorProjectSourceKind::Directory => "Directory",
        EditorProjectSourceKind::Unknown => "Unknown source",
        EditorProjectSourceKind::Unsupported => "Unsupported source",
    }
}

fn inspect_reason_label(reason: EditorProjectSourceInspectReason) -> &'static str {
    match reason {
        EditorProjectSourceInspectReason::ProjectAuthority => {
            "This project authority source is read-only in the browser"
        }
        EditorProjectSourceInspectReason::ReadOnlySource => {
            "This source is available for inspection only"
        }
        EditorProjectSourceInspectReason::Directory => {
            "Expand this directory to inspect its sources"
        }
        EditorProjectSourceInspectReason::UnknownSourceKind => {
            "No Editor operation is registered for this source kind"
        }
        EditorProjectSourceInspectReason::MissingDeclaredRoot => {
            "The declared project source root is missing"
        }
        EditorProjectSourceInspectReason::UnavailableDeclaredRoot => {
            "The declared project source root cannot be safely enumerated"
        }
        EditorProjectSourceInspectReason::UnsupportedEntry => {
            "This native entry is not supported by the capability boundary"
        }
        EditorProjectSourceInspectReason::IdentityUnavailable => {
            "This source has no exact identity proof and cannot be opened"
        }
    }
}

#[cfg(test)]
pub(crate) fn test_control_rect(label: &str) -> Option<egui::Rect> {
    TEST_CONTROL_RECTS.with(|rects| rects.borrow().get(label).copied())
}

#[cfg(test)]
pub(crate) fn test_rendered_rows() -> Vec<String> {
    TEST_RENDERED_ROWS.with(|rows| rows.borrow().clone())
}

#[cfg(test)]
pub(crate) fn test_status_label() -> Option<String> {
    TEST_STATUS_LABEL.with(|label| label.borrow().clone())
}

#[cfg(test)]
fn clear_test_observations() {
    TEST_CONTROL_RECTS.with(|rects| rects.borrow_mut().clear());
    TEST_RENDERED_ROWS.with(|rows| rows.borrow_mut().clear());
    TEST_STATUS_LABEL.with(|label| *label.borrow_mut() = None);
}

#[cfg(test)]
fn record_test_control(label: &str, rect: egui::Rect) {
    TEST_CONTROL_RECTS.with(|rects| {
        rects.borrow_mut().insert(label.to_owned(), rect);
    });
}
