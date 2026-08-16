use egui::{
    Align, Button, Color32, Layout, Rect, RichText, ScrollArea, Sense, Ui, UiBuilder, Vec2, Window,
};
use nara_diagnostic::{DiagnosticReport, DiagnosticSeverity};
use nara_tooling::{
    EditorAction, EditorActionBatch, EditorCloseDecision, EditorDocumentMutation, EditorFrameModel,
    EditorPersistenceAction, EditorPersistenceOperation, EditorPersistenceRequestKind,
    EditorPersistenceResult, EditorPlayState, EditorWorkspaceIntent, EditorWorkspaceIntentAction,
    EditorWorkspaceIntentPhase, EditorWorkspaceIntentResult,
};

use crate::{
    inspector::EguiSceneInspectorPanel,
    scene_view::{EguiSceneViewInput, EguiSceneViewOutput, EguiSceneViewPanel},
};

const MIN_EDITOR_WIDTH: f32 = 720.0;
const MIN_EDITOR_HEIGHT: f32 = 520.0;
// Keep the fixed columns usable on a high-DPI laptop window. The Scene View receives the
// remaining width, while the tree and inspector retain enough room for their bounded controls.
const TREE_PANEL_WIDTH: f32 = 180.0;
const SCENE_PANEL_WIDTH: f32 = 240.0;
const INSPECTOR_PANEL_WIDTH: f32 = 280.0;
const COLUMN_GAP: f32 = 10.0;
const MIN_CONTENT_HEIGHT: f32 = 220.0;
const DIAGNOSTICS_HEIGHT: f32 = 120.0;

#[cfg(test)]
use std::{cell::RefCell, collections::HashMap};

#[cfg(test)]
thread_local! {
    static TEST_CONTROL_RECTS: RefCell<HashMap<String, egui::Rect>> =
        RefCell::new(HashMap::new());
    static TEST_DIAGNOSTIC_CODES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    static TEST_INLINE_FIELD_FEEDBACK: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// Stateful egui Adapter for one fixed Editor authoring panel.
///
/// The state is limited to transient field buffers and toolkit interaction state. All durable
/// facts come from the supplied frame and all mutations leave as one ordered action batch.
#[derive(Debug, Default)]
pub struct EguiSceneEditorPanel {
    inspector: EguiSceneInspectorPanel,
    scene_view: EguiSceneViewPanel,
}

/// One ordered Editor action batch plus transient Scene View interaction from the same redraw.
#[derive(Debug)]
pub struct EguiSceneEditorOutput {
    actions: EditorActionBatch,
    scene_view: EguiSceneViewOutput,
}

impl EguiSceneEditorOutput {
    #[must_use]
    pub fn into_parts(self) -> (EditorActionBatch, EguiSceneViewOutput) {
        (self.actions, self.scene_view)
    }
}

impl EguiSceneEditorPanel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn show(&mut self, ui: &mut Ui, frame: &EditorFrameModel) -> EditorActionBatch {
        self.show_with_scene_view(ui, frame, EguiSceneViewInput::unavailable())
            .actions
    }

    #[must_use]
    pub fn show_with_scene_view(
        &mut self,
        ui: &mut Ui,
        frame: &EditorFrameModel,
        scene_view: EguiSceneViewInput,
    ) -> EguiSceneEditorOutput {
        #[cfg(test)]
        clear_test_observations();

        let scene = frame.workspace().active_scene.as_ref();
        self.inspector.synchronize_scene(scene);
        let mut actions = Vec::new();

        ui.set_min_size(Vec2::new(MIN_EDITOR_WIDTH, MIN_EDITOR_HEIGHT));
        render_toolbar(ui, frame, &mut actions);
        ui.separator();
        let scene_view = render_authoring_content(
            ui,
            scene,
            &mut self.inspector,
            &mut self.scene_view,
            scene_view,
            frame,
            &mut actions,
        );
        ui.separator();
        render_diagnostics(ui, frame, &mut actions);
        render_dirty_close_dialog(ui, frame, &mut actions);

        EguiSceneEditorOutput {
            actions: EditorActionBatch::new(frame.revision(), actions),
            scene_view,
        }
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
    scene_view_panel: &mut EguiSceneViewPanel,
    scene_view: EguiSceneViewInput,
    frame: &EditorFrameModel,
    actions: &mut Vec<EditorAction>,
) -> EguiSceneViewOutput {
    let reserved = DIAGNOSTICS_HEIGHT + ui.spacing().item_spacing.y * 2.0;
    let content_height = (ui.available_height() - reserved).max(MIN_CONTENT_HEIGHT);
    let editing_enabled = frame.play().state() == EditorPlayState::Empty
        && frame.workspace_intent().intent().is_none()
        && frame.persistence().operation() == EditorPersistenceOperation::Idle;

    let visible_width = (ui.ctx().viewport_rect().right() - ui.cursor().left()).max(1.0);
    let content_width = ui
        .available_width()
        .min(visible_width)
        .max(TREE_PANEL_WIDTH + SCENE_PANEL_WIDTH + INSPECTOR_PANEL_WIDTH + COLUMN_GAP * 2.0);
    let (row_rect, _) =
        ui.allocate_exact_size(Vec2::new(content_width, content_height), Sense::hover());
    let tree_rect = Rect::from_min_size(row_rect.min, Vec2::new(TREE_PANEL_WIDTH, content_height));
    let inspector_rect = Rect::from_min_size(
        row_rect.right_top() - Vec2::new(INSPECTOR_PANEL_WIDTH, 0.0),
        Vec2::new(INSPECTOR_PANEL_WIDTH, content_height),
    );
    let scene_rect = Rect::from_min_max(
        tree_rect.right_top() + Vec2::new(COLUMN_GAP, 0.0),
        inspector_rect.left_bottom() - Vec2::new(COLUMN_GAP, 0.0),
    );

    let mut tree_ui = fixed_column(ui, "nara_editor_scene_tree_column", tree_rect);
    ScrollArea::vertical()
        .id_salt("nara_editor_scene_tree")
        .max_height(content_height)
        .auto_shrink([false, false])
        .show(&mut tree_ui, |ui| {
            inspector.show_tree(ui, scene, editing_enabled, actions)
        });

    let mut scene_ui = fixed_column(ui, "nara_editor_scene_view_column", scene_rect);
    let scene_view = scene_view_panel.show(&mut scene_ui, scene_view, scene_rect.size());

    let mut inspector_ui = fixed_column(ui, "nara_editor_inspector_column", inspector_rect);
    ScrollArea::vertical()
        .id_salt("nara_editor_inspector")
        .max_height(content_height)
        .auto_shrink([false, false])
        .show(&mut inspector_ui, |ui| {
            inspector.show_inspector(ui, scene, frame.field_feedback(), editing_enabled, actions)
        });

    let separator = ui.visuals().widgets.noninteractive.bg_stroke;
    ui.painter().vline(
        tree_rect.right() + COLUMN_GAP * 0.5,
        row_rect.y_range(),
        separator,
    );
    ui.painter().vline(
        inspector_rect.left() - COLUMN_GAP * 0.5,
        row_rect.y_range(),
        separator,
    );
    scene_view
}

fn fixed_column(parent: &mut Ui, id_salt: &'static str, rect: Rect) -> Ui {
    let mut child = parent.new_child(
        UiBuilder::new()
            .id_salt(id_salt)
            .max_rect(rect)
            .layout(Layout::top_down(Align::Min)),
    );
    child.set_clip_rect(parent.clip_rect().intersect(rect));
    child
}

fn render_diagnostics(ui: &mut Ui, frame: &EditorFrameModel, actions: &mut Vec<EditorAction>) {
    ui.allocate_ui_with_layout(
        Vec2::new(ui.available_width(), DIAGNOSTICS_HEIGHT),
        Layout::top_down(Align::Min),
        |ui| {
            ui.set_height(DIAGNOSTICS_HEIGHT);
            let _heading = ui.heading("Diagnostics");
            #[cfg(test)]
            record_test_control("Diagnostics heading", _heading.rect);
            ScrollArea::vertical()
                .id_salt("nara_editor_diagnostics")
                .max_height(ui.available_height())
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
        },
    );
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
pub(crate) fn record_test_inline_field_feedback(summary: &'static str) {
    TEST_INLINE_FIELD_FEEDBACK.with(|feedback| feedback.borrow_mut().push(summary.to_owned()));
}

#[cfg(test)]
pub(crate) fn test_inline_field_feedback() -> Vec<String> {
    TEST_INLINE_FIELD_FEEDBACK.with(|feedback| feedback.borrow().clone())
}

#[cfg(test)]
fn clear_test_observations() {
    TEST_CONTROL_RECTS.with(|rects| rects.borrow_mut().clear());
    TEST_DIAGNOSTIC_CODES.with(|codes| codes.borrow_mut().clear());
    TEST_INLINE_FIELD_FEEDBACK.with(|feedback| feedback.borrow_mut().clear());
}
