use std::{path::PathBuf, sync::Arc};

use dear_imgui_rs::{ListClipper, Ui};
use nara::tooling::{
    EditorAction, EditorActionBatchReport, EditorActionOutcome, EditorActionRejection,
    EditorExternalPlayCompilerLevel, EditorExternalPlayMessageKind, EditorExternalPlayState,
    EditorFrameModel, EditorImageColorSpace, EditorImageImportAttemptView,
    EditorImageImportAvailabilityView, EditorImageImportDetailRevision,
    EditorImageImportFailureReason, EditorImageImportFreshness, EditorImageImportMetadataIssue,
    EditorImageImportMetadataView, EditorImageImportSettings, EditorImageImportView,
    EditorPlayAction, EditorPlayControl, EditorPlayFailure, EditorPlayOperation,
    EditorPlayOperationResult, EditorPlayRejection, EditorPlayState, EditorProjectBrowserAction,
    EditorProjectBrowserActionOutcome, EditorProjectBrowserDiagnosticPhase,
    EditorProjectBrowserDiagnosticView, EditorProjectBrowserKindFilter,
    EditorProjectBrowserLocalAction, EditorProjectBrowserPendingWork, EditorProjectBrowserStatus,
    EditorProjectBrowserView, EditorProjectContentAction, EditorProjectContentRefreshFailure,
    EditorProjectContentView, EditorProjectSourceInspectReason, EditorProjectSourceKind,
    EditorProjectSourceOpenCapability, EditorRuntimeUnavailableReason, EditorSceneModel,
    EditorWorkspaceIntentAction, EditorWorkspaceIntentResult,
};

use super::{ImguiPreedit, ImguiTextOwner, observe_text_owner};
use crate::{
    EditorProjectAction,
    app::{EditorProjectShellAction, EditorProjectShellModel},
};

#[derive(Debug, Default)]
pub(super) struct ImguiProjectPanels {
    manager_entry: Option<u64>,
    project_path: String,
    profile: String,
    filter: String,
    filter_active: bool,
    browser_notice: Option<&'static str>,
    image_draft: Option<ImageImportDraft>,
}

#[derive(Debug)]
struct ImageImportDraft {
    relative_path: Arc<str>,
    revision: EditorImageImportDetailRevision,
    settings: EditorImageImportSettings,
}

impl ImguiProjectPanels {
    pub(super) fn reset_workspace(&mut self) {
        *self = Self::default();
    }

    pub(super) fn show_shell(
        &mut self,
        ui: &Ui,
        model: EditorProjectShellModel<'_>,
        text_owner: &mut Option<ImguiTextOwner>,
        preedit: Option<&ImguiPreedit>,
    ) -> Option<EditorProjectShellAction> {
        match model {
            EditorProjectShellModel::ProjectManager {
                entry_generation,
                suggested_request,
            } => {
                let focus = self.synchronize_manager(
                    entry_generation,
                    suggested_request.project_root().to_string_lossy().as_ref(),
                    suggested_request
                        .owned_profile()
                        .as_deref()
                        .unwrap_or_default(),
                );
                ui.text("Project Manager");
                ui.text_wrapped("Open an existing Nara project without loading project code.");
                self.show_open_fields(ui, focus, text_owner, preedit);
                if shell_button(ui, "Open Project") {
                    return Some(EditorProjectShellAction::OpenProject {
                        project_root: PathBuf::from(self.project_path.trim()),
                        profile: Some(self.profile.clone()),
                    });
                }
                ui.same_line();
                shell_button(ui, "Exit").then_some(EditorProjectShellAction::Exit)
            }
            EditorProjectShellModel::Opening {
                attempt_generation,
                request,
                phase,
                summary,
                replacement,
                ..
            } => {
                ui.text(if replacement {
                    "Opening replacement project"
                } else {
                    "Opening project"
                });
                ui.text_wrapped(summary);
                ui.text_wrapped(format!("Project: {}", request.project_root().display()));
                ui.text(format!(
                    "Profile: {}",
                    request.owned_profile().unwrap_or_default()
                ));
                ui.text(format!("Attempt: {attempt_generation}"));
                ui.text(format!("Phase: {phase}"));
                shell_button(ui, "Cancel").then_some(EditorProjectShellAction::Cancel)
            }
            EditorProjectShellModel::Recovery {
                attempt_generation,
                request,
                diagnostics,
                ..
            } => {
                ui.text("Project recovery");
                ui.text_wrapped(
                    "The authoring project was not published. Runtime actions remain unavailable.",
                );
                ui.text_wrapped(format!("Project: {}", request.project_root().display()));
                ui.text(format!(
                    "Profile: {}",
                    request.owned_profile().unwrap_or_default()
                ));
                ui.text(format!("Attempt: {attempt_generation}"));
                ui.child_window("recovery_diagnostics")
                    .size([0.0, ui.text_line_height_with_spacing() * 15.0])
                    .build(ui, || {
                        if diagnostics.is_empty() {
                            ui.text_wrapped("No structured recovery diagnostic was supplied.");
                        }
                        for diagnostic in diagnostics {
                            ui.separator();
                            ui.text(&diagnostic.code);
                            ui.text(format!("Phase: {}", diagnostic.phase));
                            if let Some(source) = &diagnostic.relative_source {
                                ui.text_wrapped(source);
                            }
                            ui.text_wrapped(&diagnostic.summary);
                            ui.text_wrapped(&diagnostic.impact);
                            ui.text_wrapped(&diagnostic.repair_hint);
                        }
                    });
                if shell_button(ui, "Retry") {
                    return Some(EditorProjectShellAction::Retry);
                }
                ui.same_line();
                if shell_button(ui, "Close Project") {
                    return Some(EditorProjectShellAction::CloseProject);
                }
                ui.same_line();
                shell_button(ui, "Open Project")
                    .then_some(EditorProjectShellAction::ReturnToProjectManager)
            }
        }
    }

    pub(super) fn show_project_actions(
        &mut self,
        ui: &Ui,
        enabled: bool,
        text_owner: &mut Option<ImguiTextOwner>,
        preedit: Option<&ImguiPreedit>,
    ) -> Option<EditorProjectAction> {
        let _disabled = ui.begin_disabled_with_cond(!enabled);
        if shell_button(ui, "Open Project...") {
            ui.open_popup("open_project");
        }
        ui.same_line();
        let mut action =
            shell_button(ui, "Close Project").then_some(EditorProjectAction::CloseProject);
        if let Some(_popup) = ui.begin_popup("open_project") {
            self.show_open_fields(ui, false, text_owner, preedit);
            if shell_button(ui, "Open Project") {
                action = Some(EditorProjectAction::OpenProject {
                    project_root: PathBuf::from(self.project_path.trim()),
                    profile: Some(self.profile.clone()),
                });
                ui.close_current_popup();
            }
            ui.same_line();
            if shell_button(ui, "Cancel") {
                ui.close_current_popup();
            }
        }
        action
    }

    fn synchronize_manager(&mut self, entry: u64, project_path: &str, profile: &str) -> bool {
        if self.manager_entry == Some(entry) {
            return false;
        }
        self.manager_entry = Some(entry);
        project_path.clone_into(&mut self.project_path);
        profile.clone_into(&mut self.profile);
        true
    }

    fn show_open_fields(
        &mut self,
        ui: &Ui,
        focus_path: bool,
        text_owner: &mut Option<ImguiTextOwner>,
        preedit: Option<&ImguiPreedit>,
    ) {
        if focus_path {
            ui.set_keyboard_focus_here();
        }
        ui.set_next_item_width(ui.content_region_avail()[0].min(520.0));
        ui.input_text("Project path", &mut self.project_path)
            .build();
        #[cfg(test)]
        super::interaction_tests::record_control(ui, "Project path");
        observe_text_owner(ui, text_owner, preedit);
        ui.set_next_item_width(ui.content_region_avail()[0].min(520.0));
        ui.input_text("Profile", &mut self.profile)
            .hint("desktop")
            .build();
        #[cfg(test)]
        super::interaction_tests::record_control(ui, "Profile");
        observe_text_owner(ui, text_owner, preedit);
    }

    pub(super) fn show_browser(
        &mut self,
        ui: &Ui,
        view: &EditorProjectBrowserView,
        host_actions_enabled: bool,
        actions: &mut Vec<EditorAction>,
        local_actions: &mut Vec<EditorProjectBrowserLocalAction>,
        text_owner: &mut Option<ImguiTextOwner>,
        preedit: Option<&ImguiPreedit>,
    ) {
        ui.text(browser_status(view.status()));
        ui.same_line();
        if enabled_button(ui, "Refresh", host_actions_enabled) {
            actions.push(EditorAction::ProjectBrowser(
                EditorProjectBrowserAction::Refresh,
            ));
        }
        if let Some(generation) = view.snapshot_generation() {
            ui.text(format!(
                "{} {generation}",
                if matches!(
                    view.status(),
                    EditorProjectBrowserStatus::Scanning | EditorProjectBrowserStatus::Unavailable
                ) {
                    "Last-good generation"
                } else {
                    "Generation"
                }
            ));
        }
        if let Some(progress) = view.scan_progress() {
            ui.text(format!(
                "{} | {} entries | {} directories",
                browser_phase(progress.phase()),
                progress.usage().entries(),
                progress.usage().directories(),
            ));
        }
        if !self.filter_active {
            view.path_filter().clone_into(&mut self.filter);
        }
        if ui.input_text("Filter sources", &mut self.filter).build() {
            local_actions.push(EditorProjectBrowserLocalAction::SetPathFilter {
                value: Arc::from(self.filter.as_str()),
            });
        }
        self.filter_active = ui.is_item_active();
        observe_text_owner(ui, text_owner, preedit);
        let filters = [
            EditorProjectBrowserKindFilter::All,
            EditorProjectBrowserKindFilter::Documents,
            EditorProjectBrowserKindFilter::Assets,
            EditorProjectBrowserKindFilter::Scripts,
            EditorProjectBrowserKindFilter::Directories,
            EditorProjectBrowserKindFilter::Other,
        ];
        let mut selected = filters
            .iter()
            .position(|filter| *filter == view.kind_filter())
            .unwrap_or(0);
        if ui.combo_simple_string(
            "Source kind",
            &mut selected,
            &[
                "All sources",
                "Scenes and Prefabs",
                "Assets",
                "Scripts",
                "Directories",
                "Other",
            ],
        ) {
            local_actions.push(EditorProjectBrowserLocalAction::SetKindFilter {
                filter: filters[selected],
            });
        }
        if view.pending_work() == EditorProjectBrowserPendingWork::Query {
            ui.text("Updating filtered rows...");
        }
        if view.rows().is_empty() && !view.has_pending_work() {
            ui.text_wrapped(match view.status() {
                EditorProjectBrowserStatus::NotRequested => {
                    "Refresh to build the project source index"
                }
                EditorProjectBrowserStatus::Unavailable => "Project source indexing is unavailable",
                _ => "No project sources match the current filter",
            });
        }
        ui.child_window("project_sources").build(ui, || {
            let row_height = ui.frame_height();
            let mut clipper = ListClipper::new(view.rows().len())
                .items_height(ui.frame_height_with_spacing())
                .begin(ui);
            while clipper.step() {
                for row in &view.rows()[clipper.display_start()..clipper.display_end()] {
                    let _id = ui.push_id(row.relative_path());
                    ui.with_indent_by(row.depth() as f32 * 12.0, || {
                        if row.has_children() {
                            if ui.small_button(if row.expanded() { "-" } else { "+" }) {
                                local_actions.push(
                                    EditorProjectBrowserLocalAction::ToggleExpanded {
                                        relative_path: Arc::from(row.relative_path()),
                                    },
                                );
                            }
                            ui.same_line();
                        }
                        if ui
                            .selectable_config(format!(
                                "[{}] {}",
                                source_badge(row.kind()),
                                row.relative_path()
                            ))
                            .selected(view.selected_path() == Some(row.relative_path()))
                            .size([0.0, row_height])
                            .build()
                        {
                            local_actions.push(EditorProjectBrowserLocalAction::Select {
                                relative_path: Some(Arc::from(row.relative_path())),
                            });
                        }
                    });
                }
            }
        });
    }

    pub(super) fn show_source_inspector(
        &mut self,
        ui: &Ui,
        view: &EditorProjectBrowserView,
        image_import: Option<&EditorImageImportView>,
        host_actions_enabled: bool,
        image_import_work_pending: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        let selected = view.selected_entry();
        let image_import = image_import.filter(|image| {
            selected.is_some_and(|entry| {
                entry.kind() == EditorProjectSourceKind::AssetSource
                    && entry.relative_path() == image.relative_path()
            })
        });
        if let Some(image) = image_import {
            self.show_image_import(
                ui,
                image,
                host_actions_enabled && !image_import_work_pending,
                actions,
            );
        } else {
            self.image_draft = None;
            ui.text("Project Source");
            if let Some(entry) = selected {
                ui.text_wrapped(entry.relative_path());
                ui.text(format!("Kind: {}", source_kind(entry.kind())));
                match entry.open_capability() {
                    EditorProjectSourceOpenCapability::Scene
                    | EditorProjectSourceOpenCapability::Prefab => {
                        if enabled_button(ui, "Open", host_actions_enabled) {
                            actions.push(EditorAction::ProjectBrowser(
                                EditorProjectBrowserAction::Open {
                                    relative_path: Arc::from(entry.relative_path()),
                                },
                            ));
                        }
                    }
                    EditorProjectSourceOpenCapability::InspectOnly(reason) => {
                        ui.text_wrapped(inspect_reason(reason))
                    }
                    EditorProjectSourceOpenCapability::None => {
                        ui.text("No operation is available for this source")
                    }
                }
            } else {
                ui.text_wrapped("Select a project source to inspect it");
            }
        }
        if let Some(notice) = self.browser_notice {
            ui.separator();
            ui.text_wrapped(notice);
        }
        if let Some(failure) = view.failure() {
            ui.separator();
            show_browser_diagnostic(ui, failure);
        }
        if let Some(snapshot) = view.snapshot() {
            ui.separator();
            let usage = snapshot.usage();
            ui.text(format!(
                "Indexed {} entries in {} directories",
                usage.entries(),
                usage.directories()
            ));
            ui.text(format!(
                "Depth {} | path bytes {} | retained bytes {}",
                usage.max_depth(),
                usage.path_bytes(),
                usage.retained_snapshot_bytes()
            ));
            for diagnostic in snapshot.diagnostics().iter().take(32) {
                ui.separator();
                show_browser_diagnostic(ui, diagnostic);
            }
        }
    }

    pub(super) fn observe_action_report(&mut self, report: &EditorActionBatchReport) {
        if let Some(outcome) = report
            .outcomes()
            .iter()
            .rev()
            .find_map(|outcome| match outcome {
                EditorActionOutcome::ProjectBrowser(outcome) => Some(*outcome),
                _ => None,
            })
        {
            self.browser_notice = Some(match outcome {
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
    }

    pub(super) fn observe_action_rejection(&mut self, rejection: EditorActionRejection) {
        if matches!(
            rejection,
            EditorActionRejection::NoPublishedFrame | EditorActionRejection::StaleFrame { .. }
        ) {
            self.browser_notice = Some("Project Browser action came from a stale Editor frame");
        }
    }

    fn synchronize_image(&mut self, view: &EditorImageImportView) {
        if self.image_draft.as_ref().is_some_and(|draft| {
            draft.relative_path.as_ref() == view.relative_path()
                && draft.revision == view.detail_revision()
        }) {
            return;
        }
        self.image_draft = view.draft_seed().map(|settings| ImageImportDraft {
            relative_path: Arc::from(view.relative_path()),
            revision: view.detail_revision(),
            settings,
        });
    }

    fn show_image_import(
        &mut self,
        ui: &Ui,
        view: &EditorImageImportView,
        host_actions_enabled: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        self.synchronize_image(view);
        ui.text("Image Import");
        ui.text_wrapped(view.relative_path());
        ui.text_wrapped(metadata_status(view.metadata()));
        if let EditorImageImportMetadataView::Ready {
            committed_settings, ..
        } = view.metadata()
        {
            ui.text(format!(
                "Committed color space: {}",
                color_space(committed_settings.color_space())
            ));
        }
        match view.availability() {
            EditorImageImportAvailabilityView::Unavailable => {
                ui.text("Last-good image: Unavailable")
            }
            EditorImageImportAvailabilityView::LastGood {
                active_settings,
                dimensions,
                freshness,
                ..
            } => {
                ui.text_wrapped(freshness_status(freshness));
                ui.text(format!(
                    "Active color space: {}",
                    color_space(active_settings.color_space())
                ));
                ui.text(format!(
                    "Dimensions: {} x {}",
                    dimensions.width(),
                    dimensions.height()
                ));
            }
        }
        ui.text_wrapped(attempt_status(view.attempt()));
        let Some(draft) = self.image_draft.as_mut() else {
            ui.text_wrapped("Import settings are unavailable until metadata is repaired");
            return;
        };
        ui.separator();
        let mut selected =
            usize::from(draft.settings.color_space() == EditorImageColorSpace::Linear);
        if ui.combo_simple_string("Draft color space", &mut selected, &["sRGB", "Linear"]) {
            draft.settings = EditorImageImportSettings::new(if selected == 0 {
                EditorImageColorSpace::Srgb
            } else {
                EditorImageColorSpace::Linear
            });
        }
        let seed = view.draft_seed();
        if enabled_button(
            ui,
            "Revert",
            seed.is_some_and(|seed| seed != draft.settings),
        ) && let Some(seed) = seed
        {
            draft.settings = seed;
        }
        ui.same_line();
        let label = if view.metadata().is_reimport() {
            "Reimport"
        } else {
            "Import"
        };
        if enabled_button(ui, label, host_actions_enabled && view.can_submit()) {
            actions.push(EditorAction::ImageImport(
                view.submit_action(draft.settings),
            ));
        }
    }
}

pub(super) fn show_editor_status(
    ui: &Ui,
    frame: &EditorFrameModel,
    actions: &mut Vec<EditorAction>,
) {
    match frame.project_content() {
        EditorProjectContentView::NotInstalled => {
            ui.text_wrapped("Runtime project content is not installed");
        }
        EditorProjectContentView::Current { .. } => {}
        EditorProjectContentView::RefreshPending { .. } => {
            ui.text_wrapped("Saved source; refreshing project content");
        }
        EditorProjectContentView::Stale { failure, .. } => {
            ui.text_wrapped(match failure {
                EditorProjectContentRefreshFailure::SavedSourceChanged => {
                    "Saved source changed before project refresh; Reopen to reconcile"
                }
                EditorProjectContentRefreshFailure::InvalidContent => {
                    "Saved source is invalid for project content"
                }
                EditorProjectContentRefreshFailure::HostUnavailable => {
                    "Project content source is temporarily unavailable"
                }
                EditorProjectContentRefreshFailure::CapacityExceeded => {
                    "Project content exceeds the configured capacity"
                }
                EditorProjectContentRefreshFailure::RuntimeRetirementRequired => {
                    "Stop the older Game Runtime before refreshing this content generation"
                }
                EditorProjectContentRefreshFailure::PreviewRejected => {
                    "Project content loaded, but the Editor preview replacement failed"
                }
            });
            if failure.retryable() && ui.button("Retry refresh") {
                actions.push(EditorAction::ProjectContent(
                    EditorProjectContentAction::RetryRefresh,
                ));
            }
        }
    }
    if let Some(result) = frame.workspace_intent().result() {
        ui.text_wrapped(match result {
            EditorWorkspaceIntentResult::Applied { .. } => "Workspace intent applied",
            EditorWorkspaceIntentResult::Cancelled { .. } => "Workspace intent cancelled",
            EditorWorkspaceIntentResult::Rejected { .. } => "Workspace intent rejected",
        });
        if ui.button("Dismiss workspace") {
            actions.push(EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::AcknowledgeResult,
            ));
        }
    }
}

pub(super) fn show_play_result(ui: &Ui, frame: &EditorFrameModel) {
    let Some(result) = frame.play().result() else {
        return;
    };
    let message = match result {
        EditorPlayOperationResult::Pending { .. } => return,
        EditorPlayOperationResult::Applied { operation, .. } => match operation {
            EditorPlayOperation::Play => "Play ready.",
            EditorPlayOperation::Pause => "Paused.",
            EditorPlayOperation::Resume => "Running.",
            EditorPlayOperation::StepFixedTick => "Step complete.",
            EditorPlayOperation::Stop => "Stopped.",
            EditorPlayOperation::Restart => "Restarted.",
            EditorPlayOperation::RunLastGood => "Last good started.",
            EditorPlayOperation::Cancel
            | EditorPlayOperation::RetryRetirement
            | EditorPlayOperation::RetryClose => "Play updated.",
        },
        EditorPlayOperationResult::Rejected { reason, .. } => match reason {
            EditorPlayRejection::Busy => "Play rejected: another operation is still running.",
            EditorPlayRejection::NoActiveDocument => {
                "Play rejected: open a Scene document before starting Play."
            }
            EditorPlayRejection::InvalidState => {
                "Play rejected: this control is unavailable in the current Play state."
            }
            EditorPlayRejection::ResultPending => {
                "Play rejected: the previous operation result has not been cleared."
            }
            EditorPlayRejection::StaleDocument => {
                "Play rejected: the document changed. Try again from its current state."
            }
            EditorPlayRejection::RuntimeOutOfDate => {
                "Play rejected: save changes and refresh project content before starting Play."
            }
            EditorPlayRejection::RuntimeUnavailable(reason) => {
                ui.text_wrapped(
                    "Play rejected: runtime execution is unavailable for this project.",
                );
                show_runtime_unavailable(ui, reason);
                return;
            }
            EditorPlayRejection::UnsupportedControl(control) => match control {
                EditorPlayControl::Pause => {
                    "Play rejected: Pause is unavailable for this Play placement."
                }
                EditorPlayControl::Resume => {
                    "Play rejected: Resume is unavailable for this Play placement."
                }
                EditorPlayControl::StepFixedTick => {
                    "Play rejected: Step is unavailable for this Play placement."
                }
            },
        },
        EditorPlayOperationResult::Failed { failure, .. } => match failure {
            EditorPlayFailure::Preparation => "Play failed while preparing the runtime.",
            EditorPlayFailure::Superseded => "Play failed: the request was superseded.",
            EditorPlayFailure::Start => "Play failed while starting the runtime.",
            EditorPlayFailure::Runtime => "Play failed while the runtime was running.",
            EditorPlayFailure::Presentation => "Play failed while presenting the Game View.",
            EditorPlayFailure::Retirement => {
                "Play failed: runtime cleanup is incomplete. Retry retirement."
            }
            EditorPlayFailure::Close => "Play failed: runtime close is incomplete. Retry close.",
        },
        EditorPlayOperationResult::Cancelled { .. } => "Play cancelled.",
    };
    ui.text_wrapped(message);
}

pub(super) fn show_external_play_controls(
    ui: &Ui,
    scene: Option<&EditorSceneModel>,
    frame: &EditorFrameModel,
    content_ready: bool,
    can_request: bool,
    play_pending: bool,
    actions: &mut Vec<EditorAction>,
) {
    let external = frame.play().external();
    let state = external.map_or(EditorExternalPlayState::Idle, |external| external.state());
    ui.text(external_state(state));
    let mut push = |label: &str, enabled: bool, allow_pending: bool, action| {
        let cleanup = matches!(
            action,
            EditorPlayAction::RetryClose | EditorPlayAction::RetryRetirement
        );
        if enabled_button(
            ui,
            label,
            enabled && (can_request || cleanup) && (allow_pending || !play_pending),
        ) {
            actions.push(EditorAction::Play(action));
        }
        ui.same_line();
    };
    let play = scene.map(|scene| EditorPlayAction::Play {
        document: scene.document,
        base_revision: scene.revision,
    });
    match state {
        EditorExternalPlayState::Idle => {
            if let Some(play) = play {
                push("Play", content_ready, false, play);
            } else {
                enabled_button(ui, "Play", false);
            }
        }
        EditorExternalPlayState::Building => push("Cancel", true, true, EditorPlayAction::Cancel),
        EditorExternalPlayState::Starting => push("Stop", true, true, EditorPlayAction::Stop),
        EditorExternalPlayState::Running => {
            push("Stop", true, false, EditorPlayAction::Stop);
            push("Restart", true, false, EditorPlayAction::Restart);
        }
        EditorExternalPlayState::Stopping => {}
        EditorExternalPlayState::Faulted => {
            push("Stop", true, false, EditorPlayAction::Stop);
            if let Some(play) = play {
                push("Retry", content_ready, false, play);
            }
            push(
                "Run Last Good",
                content_ready && external.is_some_and(|external| external.last_good_available()),
                false,
                EditorPlayAction::RunLastGood,
            );
        }
        EditorExternalPlayState::CleanupIncomplete => {
            if frame.play().state() == EditorPlayState::CloseIncomplete {
                push("Retry close", true, false, EditorPlayAction::RetryClose);
            } else {
                push(
                    "Retry retirement",
                    true,
                    false,
                    EditorPlayAction::RetryRetirement,
                );
            }
        }
    }
    ui.new_line();
    if let Some(external) = external {
        if let Some(generation) = external.generation() {
            ui.text(format!("External generation {generation}"));
        }
        let diagnostics = external.diagnostics();
        if !diagnostics.is_empty() {
            ui.text(format!(
                "Build diagnostics: {} errors, {} warnings, {} retained",
                diagnostics.error_count(),
                diagnostics.warning_count(),
                diagnostics.retained_count()
            ));
        }
        if external.output_truncated()
            || diagnostics.dropped_count() > 0
            || diagnostics.truncated_count() > 0
        {
            ui.text("External output and diagnostics bounded");
        }
    }
    if frame.play().is_out_of_date() {
        ui.text_wrapped("Saved source changed; Restart keeps the admitted generation, while Stop then Play builds saved changes");
    }
}

pub(super) fn show_external_play_output(ui: &Ui, frame: &EditorFrameModel) {
    let Some(external) = frame.play().external() else {
        return;
    };
    for message in external.output().messages() {
        let owner = match message.kind() {
            EditorExternalPlayMessageKind::Compiler(level) => match level {
                EditorExternalPlayCompilerLevel::Error => "Compiler error",
                EditorExternalPlayCompilerLevel::Warning => "Compiler warning",
                EditorExternalPlayCompilerLevel::Note => "Compiler note",
                EditorExternalPlayCompilerLevel::Help => "Compiler help",
                EditorExternalPlayCompilerLevel::FailureNote => "Compiler failure",
                EditorExternalPlayCompilerLevel::Other => "Compiler",
            },
            EditorExternalPlayMessageKind::ProductStdout => "Product stdout",
            EditorExternalPlayMessageKind::ProductStderr => "Product stderr",
            EditorExternalPlayMessageKind::OutputNotice => "Supervisor",
        };
        ui.text_wrapped(format!(
            "{owner} {}: {}",
            message.code().unwrap_or_default(),
            message.text()
        ));
    }
    if external.output().is_truncated() {
        ui.text(format!(
            "External output bounded ({} dropped messages, {} truncated messages)",
            external.output().dropped_messages(),
            external.output().truncated_messages()
        ));
    }
}

pub(super) fn show_runtime_unavailable(ui: &Ui, reason: EditorRuntimeUnavailableReason) {
    ui.text_wrapped(match reason {
        EditorRuntimeUnavailableReason::SupervisedPlayContractMissing => "Play unavailable: add [profiles.<selected-profile>.execution] with contract = \"desktop-supervised-play/v1\" to nara.toml",
        EditorRuntimeUnavailableReason::SupervisedPlayContractUnsupported => "Play unavailable: the project Play contract is unsupported",
        EditorRuntimeUnavailableReason::ProjectIdentityMissing => "Play unavailable: the project is missing a stable identity",
        EditorRuntimeUnavailableReason::SupervisedPlayPlatformUnsupported => "Play unavailable: this platform cannot supervise the desktop product",
        EditorRuntimeUnavailableReason::SupervisedPlayUnavailable => "Play unavailable: supervised desktop execution is unavailable",
    });
}

fn shell_button(ui: &Ui, label: &str) -> bool {
    let clicked = ui.button(label);
    #[cfg(test)]
    super::interaction_tests::record_control(ui, format!("shell:{label}"));
    clicked
}

fn enabled_button(ui: &Ui, label: &str, enabled: bool) -> bool {
    let _disabled = ui.begin_disabled_with_cond(!enabled);
    let clicked = ui.button(label);
    #[cfg(test)]
    super::interaction_tests::record_control(ui, format!("project:{label}"));
    clicked
}

fn browser_status(status: EditorProjectBrowserStatus) -> &'static str {
    match status {
        EditorProjectBrowserStatus::NotRequested => "Not indexed",
        EditorProjectBrowserStatus::Scanning => "Scanning",
        EditorProjectBrowserStatus::Ready => "Ready",
        EditorProjectBrowserStatus::ReadyWithDiagnostics => "Ready with diagnostics",
        EditorProjectBrowserStatus::Unavailable => "Unavailable",
    }
}

fn browser_phase(phase: EditorProjectBrowserDiagnosticPhase) -> &'static str {
    match phase {
        EditorProjectBrowserDiagnosticPhase::RootNormalization => "Normalizing roots",
        EditorProjectBrowserDiagnosticPhase::Enumeration => "Enumerating sources",
        EditorProjectBrowserDiagnosticPhase::LineageRevalidation => {
            "Revalidating directory lineage"
        }
        EditorProjectBrowserDiagnosticPhase::Publication => "Publishing index",
    }
}

fn show_browser_diagnostic(ui: &Ui, diagnostic: &EditorProjectBrowserDiagnosticView) {
    ui.text(diagnostic.code());
    if let Some(source) = diagnostic.relative_source() {
        ui.text_wrapped(source);
    }
    ui.text_wrapped(diagnostic.summary());
    ui.text_wrapped(diagnostic.impact());
    ui.text_wrapped(diagnostic.repair_hint());
}

fn source_badge(kind: EditorProjectSourceKind) -> &'static str {
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

fn source_kind(kind: EditorProjectSourceKind) -> &'static str {
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

fn inspect_reason(reason: EditorProjectSourceInspectReason) -> &'static str {
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

fn color_space(space: EditorImageColorSpace) -> &'static str {
    match space {
        EditorImageColorSpace::Srgb => "sRGB",
        EditorImageColorSpace::Linear => "Linear",
    }
}

fn metadata_status(metadata: EditorImageImportMetadataView) -> &'static str {
    match metadata {
        EditorImageImportMetadataView::Missing => "Metadata: Missing",
        EditorImageImportMetadataView::Ready { .. } => "Metadata: Ready",
        EditorImageImportMetadataView::IdentityConflict { .. } => "Metadata: Identity conflict",
        EditorImageImportMetadataView::Invalid { issue } => match issue {
            EditorImageImportMetadataIssue::SourceUnavailable => "Metadata: Source unavailable",
            EditorImageImportMetadataIssue::SourceChanged => "Metadata: Source changed",
            EditorImageImportMetadataIssue::MetadataUnavailable => "Metadata: Sidecar unavailable",
            EditorImageImportMetadataIssue::MetadataChanged => "Metadata: Sidecar changed",
            EditorImageImportMetadataIssue::InvalidFormat => "Metadata: Invalid format",
            EditorImageImportMetadataIssue::PathMismatch => "Metadata: Path mismatch",
            EditorImageImportMetadataIssue::SourceKindMismatch => "Metadata: Source kind mismatch",
            EditorImageImportMetadataIssue::InvalidSettings => "Metadata: Invalid settings",
        },
    }
}

fn freshness_status(freshness: EditorImageImportFreshness) -> &'static str {
    match freshness {
        EditorImageImportFreshness::Fresh => "Last-good image: Current",
        EditorImageImportFreshness::SourceUnverified => {
            "Last-good image: Available; source not verified"
        }
        EditorImageImportFreshness::SourceChanged => "Last-good image: Available; source changed",
        EditorImageImportFreshness::SettingsChanged => {
            "Last-good image: Available; settings changed"
        }
        EditorImageImportFreshness::DesiredStateUnavailable => {
            "Last-good image: Available; desired settings unavailable"
        }
    }
}

fn attempt_status(attempt: EditorImageImportAttemptView) -> &'static str {
    match attempt {
        EditorImageImportAttemptView::Idle => "Attempt: Idle",
        EditorImageImportAttemptView::Pending { .. } => "Attempt: Decoding and validating",
        EditorImageImportAttemptView::Succeeded { .. } => "Attempt: Succeeded",
        EditorImageImportAttemptView::Stale { .. } => "Attempt: Stale",
        EditorImageImportAttemptView::Cancelled { .. } => "Attempt: Cancelled",
        EditorImageImportAttemptView::ReconcileRequired { .. } => {
            "Attempt: Refresh required before retry"
        }
        EditorImageImportAttemptView::Failed { reason, .. } => match reason {
            EditorImageImportFailureReason::CatalogUnavailable => {
                "Attempt failed: Catalog unavailable"
            }
            EditorImageImportFailureReason::MetadataInvalid => "Attempt failed: Metadata invalid",
            EditorImageImportFailureReason::IdentityUnavailable => {
                "Attempt failed: Identity unavailable"
            }
            EditorImageImportFailureReason::CapacityExceeded => "Attempt failed: Capacity exceeded",
            EditorImageImportFailureReason::SourceUnavailable => {
                "Attempt failed: Source unavailable"
            }
            EditorImageImportFailureReason::MetadataPublication => {
                "Attempt failed: Metadata publication"
            }
            EditorImageImportFailureReason::ImageProcessing => "Attempt failed: Image processing",
        },
    }
}

fn external_state(state: EditorExternalPlayState) -> &'static str {
    match state {
        EditorExternalPlayState::Idle => "Edit",
        EditorExternalPlayState::Building => "Building",
        EditorExternalPlayState::Starting => "Starting external window",
        EditorExternalPlayState::Running => "Running in external window",
        EditorExternalPlayState::Stopping => "Stopping external window",
        EditorExternalPlayState::Faulted => "External Play faulted",
        EditorExternalPlayState::CleanupIncomplete => "External cleanup incomplete",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    fn unsaved_external_play_rejection_is_rendered_from_the_real_owner_frame() {
        use std::{
            cell::RefCell,
            rc::Rc,
            time::{Duration, Instant},
        };

        use dear_imgui_rs::{ClipboardBackend, Condition, Context};
        use nara::{
            reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
            scene::{AuthoringFieldEdit, SceneEntityId},
            tooling::{
                EditorActionBatch, EditorCloseDecision, EditorDocumentMutation,
                EditorPlayOperation, EditorPlayOperationResult, EditorPlayRejection,
                EditorRuntimeAvailability,
            },
        };

        struct RenderedText(Rc<RefCell<String>>);

        impl ClipboardBackend for RenderedText {
            fn get(&mut self) -> Option<String> {
                Some(self.0.borrow().clone())
            }

            fn set(&mut self, value: &str) {
                value.clone_into(&mut self.0.borrow_mut());
            }
        }

        fn submit(application: &mut crate::EditorApplication, action: EditorAction) {
            let frame = application.current_frame().unwrap();
            application.submit_and_drive(
                EditorActionBatch::new(frame.revision(), [action]),
                Duration::ZERO,
            );
        }

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../samples/collect-game");
        let source_path = root.join("scenes/startup.scene.json");
        let original_source = std::fs::read(&source_path).unwrap();
        let mut application = crate::EditorApplication::new(
            crate::EditorLaunchOptions::new(root).with_profile("desktop"),
        );
        application.native_shell_ready();
        for _ in 0..8 {
            application.advance_project_open();
            if application.current_frame().is_some() {
                break;
            }
        }
        let frame = application.current_frame().unwrap();
        let availability = frame.runtime_availability();
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        let document = scene.document;
        submit(
            &mut application,
            EditorAction::Mutate(EditorDocumentMutation::ApplyAuthoringFieldEdit {
                document,
                base_revision: scene.revision,
                edit: AuthoringFieldEdit::single(
                    SceneEntityId::new("main-camera").unwrap(),
                    ComponentTypeId::new("nara.render.Camera2d"),
                    ComponentSchemaVersion::ONE,
                    ComponentFieldId::new("viewport_height"),
                    ComponentValue::f64(361.0).unwrap(),
                )
                .unwrap(),
            }),
        );
        let dirty = application.current_frame().unwrap();
        let dirty_document = dirty.workspace().document(document).unwrap().clone();
        submit(
            &mut application,
            EditorAction::Play(EditorPlayAction::Play {
                document,
                base_revision: dirty_document.revision,
            }),
        );
        let rejected = application.current_frame().unwrap();
        let log = Rc::new(RefCell::new(String::new()));
        let mut context = Context::create();
        context
            .set_ini_filename::<std::path::PathBuf>(None)
            .unwrap();
        context.set_clipboard_backend(RenderedText(Rc::clone(&log)));
        context.io_mut().set_display_size([1000.0, 600.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        let mut feedback_height = 0.0;
        let mut vertices = 0;
        for _ in 0..3 {
            log.borrow_mut().clear();
            let ui = context.frame();
            ui.window("Game result")
                .position([20.0, 20.0], Condition::Always)
                .size([900.0, 200.0], Condition::Always)
                .build(|| {
                    ui.log_to_clipboard(0_u32);
                    let before = ui.cursor_pos_y();
                    show_play_result(ui, &rejected);
                    feedback_height = ui.cursor_pos_y() - before;
                    ui.log_finish();
                });
            vertices = context.render_legacy().total_vtx_count();
        }
        let mut closed = application.request_exit().exit_ready();
        if !closed {
            submit(
                &mut application,
                EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Resolve(
                    EditorCloseDecision::Discard,
                )),
            );
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while !closed && Instant::now() < deadline {
            closed = application
                .drive_editor_frame(Duration::ZERO, false, true)
                .exit_ready();
            if !closed {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert!(closed);
        assert_eq!(availability, EditorRuntimeAvailability::ExternalSupervised);
        assert!(dirty_document.dirty);
        assert_eq!(
            rejected.workspace().document(document),
            Some(&dirty_document)
        );
        assert_eq!(
            rejected.play().external().unwrap().state(),
            EditorExternalPlayState::Idle
        );
        assert!(matches!(
            rejected.play().result(),
            Some(EditorPlayOperationResult::Rejected {
                operation: EditorPlayOperation::Play,
                reason: EditorPlayRejection::RuntimeOutOfDate,
            })
        ));
        assert!(vertices > 0);
        assert!(
            feedback_height > 0.0,
            "the feedback must occupy a rendered text item"
        );
        assert!(log.borrow().contains(
            "Play rejected: save changes and refresh project content before starting Play."
        ));
        assert_eq!(std::fs::read(source_path).unwrap(), original_source);
    }

    #[test]
    fn status_dismissal_releases_a_cancelled_close_for_the_next_request() {
        use std::time::{Duration, Instant};

        use dear_imgui_rs::{Condition, Context, MouseButton};
        use nara::{
            reflect::{ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue},
            scene::{AuthoringFieldEdit, SceneEntityId},
            tooling::{
                EditorActionBatch, EditorCloseDecision, EditorDocumentMutation,
                EditorWorkspaceIntent,
            },
        };

        fn submit(application: &mut crate::EditorApplication, action: EditorAction) {
            let frame = application.current_frame().unwrap();
            application.submit_and_drive(
                EditorActionBatch::new(frame.revision(), [action]),
                Duration::ZERO,
            );
        }

        fn paint(context: &mut Context, frame: &EditorFrameModel) -> (Vec<EditorAction>, [f32; 2]) {
            let ui = context.frame();
            let mut actions = Vec::new();
            let mut center = [0.0; 2];
            ui.window("Diagnostics")
                .position([20.0, 20.0], Condition::Always)
                .size([600.0, 300.0], Condition::Always)
                .build(|| {
                    show_editor_status(ui, frame, &mut actions);
                    let (min, max) = ui.item_rect();
                    center = [(min[0] + max[0]) * 0.5, (min[1] + max[1]) * 0.5];
                });
            let _ = context.render_legacy();
            (actions, center)
        }

        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/generic-editor/known-unbound");
        let mut application = crate::EditorApplication::new(crate::EditorLaunchOptions::new(root));
        application.native_shell_ready();
        for _ in 0..8 {
            application.advance_project_open();
            if application.current_frame().is_some() {
                break;
            }
        }
        let frame = application.current_frame().unwrap();
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        let document = scene.document;
        submit(
            &mut application,
            EditorAction::Mutate(EditorDocumentMutation::ApplyAuthoringFieldEdit {
                document,
                base_revision: scene.revision,
                edit: AuthoringFieldEdit::single(
                    SceneEntityId::new("signal-node").unwrap(),
                    ComponentTypeId::new("nara.scene.Name"),
                    ComponentSchemaVersion::ONE,
                    ComponentFieldId::new("value"),
                    ComponentValue::String("Unsaved close proof".to_owned()),
                )
                .unwrap(),
            }),
        );
        let request = EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Request(
            EditorWorkspaceIntent::CloseDocument { document },
        ));
        submit(&mut application, request.clone());
        submit(
            &mut application,
            EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Resolve(
                EditorCloseDecision::Cancel,
            )),
        );
        let cancelled = application.current_frame().unwrap();
        let mut context = Context::create();
        context
            .set_ini_filename::<std::path::PathBuf>(None)
            .unwrap();
        context.io_mut().set_display_size([800.0, 600.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        let mut center = [0.0; 2];
        for _ in 0..3 {
            center = paint(&mut context, &cancelled).1;
        }
        context.io_mut().add_mouse_pos_event(center);
        paint(&mut context, &cancelled);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        paint(&mut context, &cancelled);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        let actions = paint(&mut context, &cancelled).0;
        application.submit_and_drive(
            EditorActionBatch::new(cancelled.revision(), actions.clone()),
            Duration::ZERO,
        );
        let acknowledged = application.current_frame().unwrap();
        submit(&mut application, request);
        let requested_again = application.current_frame().unwrap();
        submit(
            &mut application,
            EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Resolve(
                EditorCloseDecision::Cancel,
            )),
        );
        let mut closed = application.request_exit().exit_ready();
        if !closed {
            submit(
                &mut application,
                EditorAction::WorkspaceIntent(EditorWorkspaceIntentAction::Resolve(
                    EditorCloseDecision::Discard,
                )),
            );
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        while !closed && Instant::now() < deadline {
            closed = application
                .drive_editor_frame(Duration::ZERO, false, true)
                .exit_ready();
            if !closed {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert!(closed);
        assert!(matches!(
            cancelled.workspace_intent().result(),
            Some(EditorWorkspaceIntentResult::Cancelled { .. })
        ));
        assert!(matches!(
            actions.as_slice(),
            [EditorAction::WorkspaceIntent(
                EditorWorkspaceIntentAction::AcknowledgeResult
            )]
        ));
        assert!(acknowledged.workspace_intent().result().is_none());
        assert!(requested_again.workspace_intent().phase().is_some());
        assert!(
            requested_again
                .workspace()
                .document(document)
                .unwrap()
                .dirty
        );
    }

    #[test]
    fn project_manager_preserves_current_entry_draft_and_resets_on_reentry() {
        let mut panels = ImguiProjectPanels::default();
        assert!(panels.synchronize_manager(1, "first", "desktop"));
        panels.project_path = "typed path".to_owned();
        assert!(!panels.synchronize_manager(1, "first", "desktop"));
        assert_eq!(panels.project_path, "typed path");
        assert!(panels.synchronize_manager(2, "replacement", "tools"));
        assert_eq!(panels.project_path, "replacement");
        assert_eq!(panels.profile, "tools");
    }

    #[test]
    fn image_recipe_draft_tracks_exact_path_revision_and_blocked_metadata() {
        let view = |path: &str, revision, metadata| {
            EditorImageImportView::new(
                Arc::from(path),
                EditorImageImportDetailRevision::new(revision),
                metadata,
                EditorImageImportAvailabilityView::Unavailable,
                EditorImageImportAttemptView::Idle,
                true,
            )
        };
        let mut panels = ImguiProjectPanels::default();
        let first = view("assets/a.png", 1, EditorImageImportMetadataView::Missing);
        panels.synchronize_image(&first);
        panels.image_draft.as_mut().unwrap().settings =
            EditorImageImportSettings::new(EditorImageColorSpace::Linear);
        panels.synchronize_image(&first);
        assert_eq!(
            panels.image_draft.as_ref().unwrap().settings.color_space(),
            EditorImageColorSpace::Linear
        );
        panels.synchronize_image(&view(
            "assets/a.png",
            2,
            EditorImageImportMetadataView::Missing,
        ));
        assert_eq!(
            panels.image_draft.as_ref().unwrap().settings.color_space(),
            EditorImageColorSpace::Srgb
        );
        panels.image_draft.as_mut().unwrap().settings =
            EditorImageImportSettings::new(EditorImageColorSpace::Linear);
        panels.synchronize_image(&view(
            "assets/b.png",
            2,
            EditorImageImportMetadataView::Missing,
        ));
        assert_eq!(
            panels.image_draft.as_ref().unwrap().settings.color_space(),
            EditorImageColorSpace::Srgb
        );
        panels.synchronize_image(&view(
            "assets/b.png",
            3,
            EditorImageImportMetadataView::Invalid {
                issue: EditorImageImportMetadataIssue::InvalidFormat,
            },
        ));
        assert!(panels.image_draft.is_none());
        panels.synchronize_image(&first);
        panels.reset_workspace();
        assert!(panels.image_draft.is_none());
    }
}
