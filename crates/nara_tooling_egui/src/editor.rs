use egui::{Align, Button, Color32, Layout, RichText, ScrollArea, Ui, Vec2, Window};
use nara_diagnostic::{DiagnosticReport, DiagnosticSeverity};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorCloseDecision, EditorDocumentMutation, EditorFrameModel,
    EditorPersistenceAction, EditorPersistenceOperation, EditorPersistenceRequestKind,
    EditorPersistenceResult, EditorPlayState, EditorWorkspaceIntent, EditorWorkspaceIntentAction,
    EditorWorkspaceIntentPhase, EditorWorkspaceIntentResult,
};

use crate::inspector::EguiSceneInspectorPanel;

const MIN_EDITOR_WIDTH: f32 = 720.0;
const MIN_EDITOR_HEIGHT: f32 = 520.0;
const TREE_PANEL_WIDTH: f32 = 240.0;
const INSPECTOR_PANEL_WIDTH: f32 = 420.0;
const MIN_CONTENT_HEIGHT: f32 = 260.0;
const DIAGNOSTICS_HEIGHT: f32 = 150.0;

#[cfg(test)]
use std::{cell::RefCell, collections::HashMap};

#[cfg(test)]
thread_local! {
    static TEST_CONTROL_RECTS: RefCell<HashMap<String, egui::Rect>> =
        RefCell::new(HashMap::new());
    static TEST_DIAGNOSTIC_CODES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Stateful egui Adapter for one fixed Editor authoring panel.
///
/// The state is limited to transient field buffers and toolkit interaction state. All durable
/// facts come from the supplied frame and all mutations leave as one ordered action batch.
#[derive(Debug, Default)]
pub struct EguiSceneEditorPanel {
    inspector: EguiSceneInspectorPanel,
}

impl EguiSceneEditorPanel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn show(&mut self, ui: &mut Ui, frame: &EditorFrameModel) -> EditorActionBatch {
        #[cfg(test)]
        clear_test_observations();

        let scene = frame.workspace().active_scene.as_ref();
        self.inspector.synchronize_scene(scene);
        let mut actions = Vec::new();

        ui.set_min_size(Vec2::new(MIN_EDITOR_WIDTH, MIN_EDITOR_HEIGHT));
        render_toolbar(ui, frame, &mut actions);
        ui.separator();
        render_authoring_content(ui, scene, &mut self.inspector, frame, &mut actions);
        ui.separator();
        render_diagnostics(ui, frame, &mut actions);
        render_dirty_close_dialog(ui, frame, &mut actions);

        EditorActionBatch::new(frame.revision(), actions)
    }

    #[cfg(test)]
    pub(crate) fn inspector_mut(&mut self) -> &mut EguiSceneInspectorPanel {
        &mut self.inspector
    }
}

fn render_toolbar(ui: &mut Ui, frame: &EditorFrameModel, actions: &mut Vec<EditorAction>) {
    let scene = frame.workspace().active_scene.as_ref();
    let persistence = frame.persistence();
    let no_workspace_intent = frame.workspace_intent().intent().is_none();
    let play_idle = frame.play().state() == EditorPlayState::Empty;
    let persistence_idle = persistence.operation() == EditorPersistenceOperation::Idle;
    let save_retryable = scene.is_some_and(|scene| {
        persistence_result_allows_request(
            persistence.result(),
            EditorPersistenceRequestKind::Save,
            scene.document,
        )
    });
    let reopen_document = match frame.reopen_target() {
        nara_tooling::EditorReopenTarget::OpenDocument { document, .. }
        | nara_tooling::EditorReopenTarget::ClosedSource { document } => document,
    };
    let reopen_retryable = persistence_result_allows_request(
        persistence.result(),
        EditorPersistenceRequestKind::Reopen,
        reopen_document,
    );
    let authoring_available = play_idle && no_workspace_intent && persistence_idle;

    ui.horizontal(|ui| {
        let title = scene.map_or("No active scene", |scene| scene.title.as_str());
        ui.label(RichText::new(title).strong());
        if scene.is_some_and(|scene| scene.dirty) {
            ui.label(RichText::new("Modified").color(ui.visuals().warn_fg_color));
        }
    });

    ScrollArea::horizontal()
        .id_salt("nara_editor_toolbar")
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let history = scene.map(|scene| scene.editor.inspector.history);
                if toolbar_button(
                    ui,
                    authoring_available && history.is_some_and(|history| history.undo_depth > 0),
                    "Undo",
                ) {
                    let scene = scene.expect("Undo is enabled only for an active scene");
                    actions.push(EditorAction::Mutate(EditorDocumentMutation::Undo {
                        document: scene.document,
                        base_revision: scene.revision,
                    }));
                }
                if toolbar_button(
                    ui,
                    authoring_available && history.is_some_and(|history| history.redo_depth > 0),
                    "Redo",
                ) {
                    let scene = scene.expect("Redo is enabled only for an active scene");
                    actions.push(EditorAction::Mutate(EditorDocumentMutation::Redo {
                        document: scene.document,
                        base_revision: scene.revision,
                    }));
                }
                if toolbar_button(
                    ui,
                    authoring_available && save_retryable && scene.is_some_and(|scene| scene.dirty),
                    "Save",
                ) {
                    let scene = scene.expect("Save is enabled only for an active scene");
                    actions.push(EditorAction::Persistence(EditorPersistenceAction::Save {
                        document: scene.document,
                        base_revision: scene.revision,
                    }));
                }
                if toolbar_button(ui, authoring_available && reopen_retryable, "Reopen") {
                    actions.push(EditorAction::Persistence(EditorPersistenceAction::Reopen {
                        target: frame.reopen_target(),
                    }));
                }
            });
        });
}

fn render_authoring_content(
    ui: &mut Ui,
    scene: Option<&nara_tooling::EditorSceneModel>,
    inspector: &mut EguiSceneInspectorPanel,
    frame: &EditorFrameModel,
    actions: &mut Vec<EditorAction>,
) {
    let reserved = DIAGNOSTICS_HEIGHT + ui.spacing().item_spacing.y * 2.0;
    let content_height = (ui.available_height() - reserved).max(MIN_CONTENT_HEIGHT);
    let editing_enabled = frame.play().state() == EditorPlayState::Empty
        && frame.workspace_intent().intent().is_none()
        && frame.persistence().operation() == EditorPersistenceOperation::Idle;

    ScrollArea::horizontal()
        .id_salt("nara_editor_authoring_overflow")
        .max_height(content_height)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.set_min_width(TREE_PANEL_WIDTH + INSPECTOR_PANEL_WIDTH + 24.0);
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(TREE_PANEL_WIDTH, content_height),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_height(content_height);
                        ScrollArea::vertical()
                            .id_salt("nara_editor_scene_tree")
                            .max_height(content_height)
                            .auto_shrink([false, false])
                            .show(ui, |ui| inspector.show_tree(ui, scene, actions));
                    },
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    Vec2::new(
                        INSPECTOR_PANEL_WIDTH.max(ui.available_width()),
                        content_height,
                    ),
                    Layout::top_down(Align::Min),
                    |ui| {
                        ui.set_height(content_height);
                        ScrollArea::vertical()
                            .id_salt("nara_editor_inspector")
                            .max_height(content_height)
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                inspector.show_inspector(ui, scene, editing_enabled, actions)
                            });
                    },
                );
            });
        });
}

fn render_diagnostics(ui: &mut Ui, frame: &EditorFrameModel, actions: &mut Vec<EditorAction>) {
    ui.heading("Diagnostics");
    ScrollArea::vertical()
        .id_salt("nara_editor_diagnostics")
        .max_height(DIAGNOSTICS_HEIGHT)
        .auto_shrink([false, false])
        .show(ui, |ui| {
            let status_rendered = render_editor_status(ui, frame, actions);
            let mut rendered = 0_usize;
            render_diagnostic_report(ui, "Host", frame.diagnostics(), &mut rendered);
            if let Some(scene) = frame.workspace().active_scene.as_ref() {
                render_diagnostic_report(
                    ui,
                    "Inspector",
                    &scene.editor.inspector.diagnostics,
                    &mut rendered,
                );
            }
            if rendered == 0 && !status_rendered {
                ui.label(RichText::new("No diagnostics").weak());
            }
        });
}

fn render_editor_status(
    ui: &mut Ui,
    frame: &EditorFrameModel,
    actions: &mut Vec<EditorAction>,
) -> bool {
    let mut rendered = false;
    if let Some(operation) = persistence_operation_label(frame.persistence().operation()) {
        rendered = true;
        ui.label(RichText::new(operation).weak());
    }
    if let Some(result) = frame.persistence().result() {
        rendered = true;
        ui.horizontal(|ui| {
            ui.label(RichText::new(persistence_result_label(result)).weak());
            if toolbar_button(
                ui,
                frame.persistence().operation() == EditorPersistenceOperation::Idle,
                "Dismiss result",
            ) {
                actions.push(EditorAction::Persistence(
                    EditorPersistenceAction::AcknowledgeResult,
                ));
            }
        });
    }
    if let (Some(intent), Some(phase)) = (
        frame.workspace_intent().intent(),
        frame.workspace_intent().phase(),
    ) {
        rendered = true;
        ui.label(RichText::new(workspace_intent_label(intent, phase)).weak());
    }
    if let Some(result) = frame.workspace_intent().result() {
        rendered = true;
        ui.horizontal(|ui| {
            ui.label(RichText::new(workspace_intent_result_label(result)).weak());
            if toolbar_button(ui, true, "Dismiss workspace") {
                actions.push(EditorAction::WorkspaceIntent(
                    EditorWorkspaceIntentAction::AcknowledgeResult,
                ));
            }
        });
    }
    if frame.play().state() != EditorPlayState::Empty {
        rendered = true;
        ui.label(
            RichText::new("Authoring controls are disabled while a Play owner is active").weak(),
        );
    }
    rendered
}

fn render_diagnostic_report(
    ui: &mut Ui,
    owner: &'static str,
    report: &DiagnosticReport,
    rendered: &mut usize,
) {
    for entry in report.iter() {
        *rendered = rendered.saturating_add(1);
        let color = match entry.severity() {
            DiagnosticSeverity::Error => ui.visuals().error_fg_color,
            DiagnosticSeverity::Warning => ui.visuals().warn_fg_color,
            DiagnosticSeverity::Info => Color32::GRAY,
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(owner).strong());
            ui.label(RichText::new(entry.code().as_str()).color(color));
            ui.label(entry.summary().as_str());
        });
        #[cfg(test)]
        TEST_DIAGNOSTIC_CODES.with(|codes| {
            codes.borrow_mut().push(entry.code().as_str().to_owned());
        });
    }
}

fn render_dirty_close_dialog(
    ui: &mut Ui,
    frame: &EditorFrameModel,
    actions: &mut Vec<EditorAction>,
) {
    if frame.workspace_intent().phase() != Some(EditorWorkspaceIntentPhase::AwaitingDecision) {
        return;
    }
    let save_document = match frame.workspace_intent().intent() {
        Some(EditorWorkspaceIntent::CloseScene { document }) => Some(document),
        Some(EditorWorkspaceIntent::Exit) => frame.workspace().active_document,
        None => None,
    };
    let save_enabled = frame.persistence().operation() == EditorPersistenceOperation::Idle
        && save_document.is_some_and(|document| {
            persistence_result_allows_request(
                frame.persistence().result(),
                EditorPersistenceRequestKind::Save,
                document,
            )
        });
    Window::new("Unsaved changes")
        .collapsible(false)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            ui.label("Save changes before closing?");
            ui.horizontal(|ui| {
                for (label, enabled, decision) in [
                    ("Save", save_enabled, EditorCloseDecision::Save),
                    ("Discard", true, EditorCloseDecision::Discard),
                    ("Cancel", true, EditorCloseDecision::Cancel),
                ] {
                    let response =
                        ui.add_enabled(enabled, Button::new(label).min_size(Vec2::new(80.0, 24.0)));
                    #[cfg(test)]
                    record_test_control(format!("Close {label}"), response.rect);
                    if response.clicked() {
                        actions.push(EditorAction::WorkspaceIntent(
                            EditorWorkspaceIntentAction::Resolve(decision),
                        ));
                    }
                }
            });
        });
}

fn toolbar_button(ui: &mut Ui, enabled: bool, label: &'static str) -> bool {
    let response = ui.add_enabled(enabled, Button::new(label).min_size(Vec2::new(88.0, 24.0)));
    #[cfg(test)]
    record_test_control(label.to_owned(), response.rect);
    response.clicked()
}

fn persistence_operation_label(operation: EditorPersistenceOperation) -> Option<&'static str> {
    match operation {
        EditorPersistenceOperation::Idle => None,
        EditorPersistenceOperation::Saving { .. } => Some("Saving"),
        EditorPersistenceOperation::Opening { .. } => Some("Opening"),
    }
}

fn persistence_result_label(result: EditorPersistenceResult) -> &'static str {
    match result {
        EditorPersistenceResult::Saved { .. } => "Saved",
        EditorPersistenceResult::Opened { .. } => "Reopened",
        EditorPersistenceResult::Rejected { .. } => "Persistence rejected",
        EditorPersistenceResult::Failed { .. } => "Persistence failed",
        EditorPersistenceResult::PersistenceUncertain { .. } => "Persistence uncertain",
    }
}

fn persistence_result_allows_request(
    result: Option<EditorPersistenceResult>,
    request: EditorPersistenceRequestKind,
    document: nara_tooling::EditorDocumentId,
) -> bool {
    match result {
        None => true,
        Some(EditorPersistenceResult::Rejected {
            request: failed_request,
            document: failed_document,
            ..
        })
        | Some(EditorPersistenceResult::Failed {
            request: failed_request,
            document: failed_document,
            ..
        }) => {
            failed_request == request
                && failed_document.is_none_or(|failed_document| failed_document == document)
        }
        Some(EditorPersistenceResult::PersistenceUncertain {
            document: uncertain_document,
            ..
        }) => request == EditorPersistenceRequestKind::Reopen && uncertain_document == document,
        Some(EditorPersistenceResult::Saved { .. })
        | Some(EditorPersistenceResult::Opened { .. }) => false,
    }
}

fn workspace_intent_label(
    intent: EditorWorkspaceIntent,
    phase: EditorWorkspaceIntentPhase,
) -> &'static str {
    match (intent, phase) {
        (
            EditorWorkspaceIntent::CloseScene { .. },
            EditorWorkspaceIntentPhase::AwaitingDecision,
        ) => "Close waiting for a decision",
        (EditorWorkspaceIntent::Exit, EditorWorkspaceIntentPhase::AwaitingDecision) => {
            "Exit waiting for a decision"
        }
        (EditorWorkspaceIntent::CloseScene { .. }, EditorWorkspaceIntentPhase::Saving) => {
            "Saving before close"
        }
        (EditorWorkspaceIntent::Exit, EditorWorkspaceIntentPhase::Saving) => "Saving before exit",
        (EditorWorkspaceIntent::CloseScene { .. }, EditorWorkspaceIntentPhase::RetiringRuntime) => {
            "Closing scene"
        }
        (EditorWorkspaceIntent::Exit, EditorWorkspaceIntentPhase::RetiringRuntime) => "Exiting",
    }
}

fn workspace_intent_result_label(result: EditorWorkspaceIntentResult) -> &'static str {
    match result {
        EditorWorkspaceIntentResult::Applied { .. } => "Workspace intent applied",
        EditorWorkspaceIntentResult::Cancelled { .. } => "Workspace intent cancelled",
        EditorWorkspaceIntentResult::Rejected { .. } => "Workspace intent rejected",
    }
}

#[cfg(test)]
pub(crate) fn record_test_control(id: impl Into<String>, rect: egui::Rect) {
    TEST_CONTROL_RECTS.with(|rects| {
        rects.borrow_mut().insert(id.into(), rect);
    });
}

#[cfg(test)]
pub(crate) fn test_control_rect(id: &str) -> Option<egui::Rect> {
    TEST_CONTROL_RECTS.with(|rects| rects.borrow().get(id).copied())
}

#[cfg(test)]
pub(crate) fn test_diagnostic_codes() -> Vec<String> {
    TEST_DIAGNOSTIC_CODES.with(|codes| codes.borrow().clone())
}

#[cfg(test)]
fn clear_test_observations() {
    TEST_CONTROL_RECTS.with(|rects| rects.borrow_mut().clear());
    TEST_DIAGNOSTIC_CODES.with(|codes| codes.borrow_mut().clear());
}
