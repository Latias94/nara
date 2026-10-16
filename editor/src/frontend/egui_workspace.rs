use std::collections::BTreeMap;

use nara::{
    tooling::{
        EditorAction, EditorActionOutcome, EditorDocumentId, EditorGestureAction,
        EditorGestureCancelReason, EditorGestureId, EditorGestureOutcome, EditorGestureView,
        EditorNavigationAction, EditorProjectBrowserLocalAction,
    },
    tooling_egui::{
        EguiCentralViewportOutput, EguiEditorLayout, EguiEditorLayoutInput, EguiEditorPanel,
        EguiEditorViewportIntent, EguiSceneViewOutput,
    },
};

use crate::workspace::{
    EditorPanelInstance, EditorPanelInstanceId, EditorPanelModel, EditorPanelPlacement,
    EditorPanelTarget, EditorPanelWorkspace,
};

/// Optional egui painting companion for one trusted source-built semantic contribution.
/// It can observe projections and emit admitted actions, without application/session/GPU access.
pub type EguiEditorPanelPainter = fn(&mut egui::Ui, &mut EguiEditorPanelContext<'_, '_, '_>);

pub struct EguiEditorPanelContext<'scope, 'frame, 'authoring> {
    instance: &'scope EditorPanelInstance,
    widgets: &'scope mut EguiEditorPanel,
    input: &'scope mut EguiEditorLayoutInput<'frame, 'authoring>,
    actions: &'scope mut Vec<EditorAction>,
    browser_actions: &'scope mut Vec<EditorProjectBrowserLocalAction>,
    viewport: &'scope mut Option<(EguiCentralViewportOutput, Option<EguiEditorViewportIntent>)>,
    project_browser_selected: bool,
}

impl EguiEditorPanelContext<'_, '_, '_> {
    #[must_use]
    pub fn model(&self) -> EditorPanelModel<'_> {
        EditorPanelModel::new(self.input.frame, self.instance)
    }
    pub fn emit(&mut self, action: EditorAction) {
        self.actions.push(action);
    }
}

#[derive(Debug, Default)]
pub(super) struct EguiPanelWidgets {
    document: Option<EditorDocumentId>,
    panel: EguiEditorPanel,
}

#[derive(Debug)]
struct PendingPanelGesture {
    instance: EditorPanelInstanceId,
    target: EditorPanelTarget,
    action_index: usize,
}

#[derive(Debug)]
pub(super) struct EguiPanelPresentation {
    pub painters: BTreeMap<String, EguiEditorPanelPainter>,
    widgets: BTreeMap<EditorPanelInstanceId, EguiPanelWidgets>,
    pub select_project_browser: bool,
    pub observed_project_browser_visible: bool,
    gesture_owner: Option<(EditorPanelInstanceId, EditorPanelTarget, EditorGestureId)>,
    gesture_owner_retired: bool,
    pending_gestures: Vec<PendingPanelGesture>,
}

impl Default for EguiPanelPresentation {
    fn default() -> Self {
        let painters: [(&str, EguiEditorPanelPainter); 5] = [
            ("nara.hierarchy", paint_hierarchy),
            ("nara.project-browser", paint_project_browser),
            ("nara.authoring-view", paint_viewport),
            ("nara.inspector", paint_inspector),
            ("nara.diagnostics", paint_diagnostics),
        ];
        Self {
            painters: painters
                .into_iter()
                .map(|(kind, painter)| (kind.to_owned(), painter))
                .collect(),
            widgets: BTreeMap::new(),
            select_project_browser: false,
            observed_project_browser_visible: false,
            gesture_owner: None,
            gesture_owner_retired: false,
            pending_gestures: Vec::new(),
        }
    }
}

impl EguiPanelPresentation {
    pub fn reset_widgets(&mut self) {
        self.widgets.clear();
        self.gesture_owner_retired |= self.gesture_owner.is_some();
    }
    pub(super) fn observe_panel_actions(
        &mut self,
        owner: EditorPanelInstanceId,
        target: &EditorPanelTarget,
        first_action: usize,
        actions: &[EditorAction],
    ) {
        for (action_index, action) in actions.iter().enumerate().skip(first_action) {
            if matches!(
                action,
                EditorAction::Gesture(EditorGestureAction::Begin { .. })
            ) {
                self.pending_gestures.push(PendingPanelGesture {
                    instance: owner,
                    target: target.clone(),
                    action_index,
                });
            }
        }
    }
    pub fn observe_report(&mut self, report: &nara::tooling::EditorActionBatchReport) {
        for pending in self.pending_gestures.drain(..) {
            if let Some(EditorActionOutcome::Gesture(EditorGestureOutcome::Began(view))) =
                report.outcomes().get(pending.action_index)
            {
                self.gesture_owner = Some((pending.instance, pending.target, view.stamp().id()));
                self.gesture_owner_retired = false;
            } else if self
                .gesture_owner
                .as_ref()
                .is_none_or(|(owner, _, _)| *owner != pending.instance)
            {
                self.widgets.remove(&pending.instance);
            }
        }
        for widgets in self.widgets.values_mut() {
            widgets.panel.observe_action_report(report);
        }
    }
    pub fn observe_rejection(&mut self, rejection: nara::tooling::EditorActionRejection) {
        for pending in self.pending_gestures.drain(..) {
            if self
                .gesture_owner
                .as_ref()
                .is_none_or(|(owner, _, _)| *owner != pending.instance)
            {
                self.widgets.remove(&pending.instance);
            }
        }
        for widgets in self.widgets.values_mut() {
            widgets.panel.observe_action_rejection(rejection);
        }
    }
}

pub(super) struct RegisteredEguiLayout<'a> {
    pub workspace: &'a mut EditorPanelWorkspace,
    pub presentation: &'a mut EguiPanelPresentation,
    pub active_gesture: Option<&'a EditorGestureView>,
}

impl EguiEditorLayout for RegisteredEguiLayout<'_> {
    fn show(
        &mut self,
        ui: &mut egui::Ui,
        _panels: &mut EguiEditorPanel,
        mut input: EguiEditorLayoutInput<'_, '_>,
        actions: &mut Vec<EditorAction>,
        browser_actions: &mut Vec<EditorProjectBrowserLocalAction>,
    ) -> (EguiCentralViewportOutput, Option<EguiEditorViewportIntent>) {
        self.panel_menu(ui);
        if self.presentation.select_project_browser {
            self.presentation.select_project_browser = false;
            self.select_sidebar(true);
        }
        self.reconcile_gesture(self.active_gesture, actions);
        self.presentation.observed_project_browser_visible =
            project_browser_selected(self.workspace);
        for reason in self.workspace.restore_reasons() {
            ui.colored_label(ui.visuals().warn_fg_color, reason.to_string());
        }
        let instances = self
            .workspace
            .layout()
            .instances()
            .iter()
            .filter(|panel| panel.is_visible())
            .cloned()
            .collect::<Vec<_>>();
        let bottom_open = instances
            .iter()
            .any(|panel| panel.placement() == EditorPanelPlacement::Bottom);
        let content_height =
            (ui.available_height() - if bottom_open { 140.0 } else { 0.0 }).max(220.0);
        let left_open = instances
            .iter()
            .any(|panel| panel.placement() == EditorPanelPlacement::Left);
        let right_open = instances
            .iter()
            .any(|panel| panel.placement() == EditorPanelPlacement::Right);
        let width = ui.available_width().max(1.0);
        let left_width = if left_open { 180.0 } else { 0.0 };
        let right_width = if right_open { 280.0 } else { 0.0 };
        let gap = 10.0;
        let center_width = (width - left_width - right_width - gap * 2.0).max(80.0);
        let (row, _) =
            ui.allocate_exact_size(egui::vec2(width, content_height), egui::Sense::hover());
        let regions = [
            (
                EditorPanelPlacement::Left,
                egui::Rect::from_min_size(row.min, egui::vec2(left_width, content_height)),
            ),
            (
                EditorPanelPlacement::Center,
                egui::Rect::from_min_size(
                    row.min + egui::vec2(left_width + gap, 0.0),
                    egui::vec2(center_width, content_height),
                ),
            ),
            (
                EditorPanelPlacement::Right,
                egui::Rect::from_min_size(
                    row.right_top() - egui::vec2(right_width, 0.0),
                    egui::vec2(right_width, content_height),
                ),
            ),
        ];
        let mut viewport = None;
        for (placement, rect) in regions {
            let panels = instances
                .iter()
                .filter(|panel| panel.placement() == placement)
                .collect::<Vec<_>>();
            if panels.is_empty() {
                continue;
            }
            let mut column = ui.new_child(
                egui::UiBuilder::new()
                    .id_salt(("registered-panels", placement))
                    .max_rect(rect)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            column.set_clip_rect(ui.clip_rect().intersect(rect));
            if placement == EditorPanelPlacement::Left {
                let project_browser_selected = project_browser_selected(self.workspace);
                column.horizontal(|ui| {
                    if ui
                        .selectable_label(!project_browser_selected, "Scene")
                        .clicked()
                    {
                        self.select_sidebar(false);
                    }
                    if ui
                        .selectable_label(project_browser_selected, "Project")
                        .clicked()
                    {
                        self.select_sidebar(true);
                    }
                });
                column.separator();
            }
            let panel_height = (column.available_height() / panels.len() as f32).max(1.0);
            for panel in panels {
                column.allocate_ui_with_layout(
                    egui::vec2(rect.width(), panel_height),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        self.paint(
                            ui,
                            panel,
                            &mut input,
                            actions,
                            browser_actions,
                            &mut viewport,
                        );
                    },
                );
            }
        }
        for instance in instances
            .iter()
            .filter(|panel| panel.placement() == EditorPanelPlacement::Bottom)
        {
            self.paint(
                ui,
                instance,
                &mut input,
                actions,
                browser_actions,
                &mut viewport,
            );
        }
        for instance in instances
            .iter()
            .filter(|panel| panel.placement() == EditorPanelPlacement::Floating)
        {
            let title = self
                .workspace
                .registry()
                .descriptor(instance.kind())
                .map_or(instance.kind(), |descriptor| descriptor.title())
                .to_owned();
            let mut open = true;
            egui::Window::new(title)
                .id(egui::Id::new(("panel-instance", instance.id())))
                .open(&mut open)
                .show(ui.ctx(), |ui| {
                    self.paint(
                        ui,
                        instance,
                        &mut input,
                        actions,
                        browser_actions,
                        &mut viewport,
                    );
                });
            if !open {
                self.workspace.layout_mut().close(instance.id());
            }
        }
        self.presentation.widgets.retain(|id, _| {
            self.workspace
                .layout()
                .instance(*id)
                .is_some_and(|panel| panel.is_open())
        });
        viewport.unwrap_or((
            EguiCentralViewportOutput::Scene(EguiSceneViewOutput::unavailable()),
            None,
        ))
    }
}

impl RegisteredEguiLayout<'_> {
    pub(super) fn reconcile_gesture(
        &mut self,
        active: Option<&EditorGestureView>,
        actions: &mut Vec<EditorAction>,
    ) {
        let Some(active) = active else {
            self.presentation.gesture_owner = None;
            self.presentation.gesture_owner_retired = false;
            return;
        };
        let Some((owner, target, gesture)) = &self.presentation.gesture_owner else {
            return;
        };
        if *gesture != active.stamp().id() {
            self.presentation.gesture_owner = None;
            self.presentation.gesture_owner_retired = false;
            return;
        }
        let owner_available = self
            .workspace
            .layout()
            .instance(*owner)
            .is_some_and(|panel| {
                panel.is_visible()
                    && panel.target() == target
                    && (panel.kind() != "nara.inspector"
                        || !project_browser_selected(self.workspace))
            });
        if !owner_available || self.presentation.gesture_owner_retired {
            actions.push(EditorAction::Gesture(EditorGestureAction::Cancel {
                gesture: active.stamp().id(),
                reason: EditorGestureCancelReason::TargetRetired,
            }));
            self.presentation.widgets.remove(owner);
            self.presentation.gesture_owner_retired = true;
        }
    }

    fn panel_menu(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.menu_button("Panels", |ui| {
                let descriptors = self
                    .workspace
                    .registry()
                    .contributions()
                    .cloned()
                    .collect::<Vec<_>>();
                for descriptor in descriptors {
                    if ui.button(format!("Open {}", descriptor.title())).clicked() {
                        let _ = self
                            .workspace
                            .open(descriptor.kind(), EditorPanelTarget::ActiveDocument);
                        ui.close();
                    }
                }
                ui.separator();
                let instances = self.workspace.layout().instances().to_vec();
                for panel in instances {
                    let title = self
                        .workspace
                        .registry()
                        .descriptor(panel.kind())
                        .map_or(panel.kind(), |descriptor| descriptor.title());
                    let mut visible = panel.is_visible();
                    if ui
                        .checkbox(&mut visible, format!("{title} {:?}", panel.id()))
                        .changed()
                    {
                        if visible {
                            self.workspace.layout_mut().reopen(panel.id());
                        } else {
                            self.workspace.layout_mut().set_visible(panel.id(), false);
                        }
                    }
                }
                if ui.button("Reset workspace layout").clicked() {
                    self.workspace.reset();
                    self.presentation.reset_widgets();
                    ui.close();
                }
                if ui
                    .add_enabled(
                        self.workspace.has_preferences(),
                        egui::Button::new("Save workspace layout"),
                    )
                    .clicked()
                {
                    self.workspace.save_preferences();
                    ui.close();
                }
            });
        });
    }

    fn select_sidebar(&mut self, project: bool) {
        let show = if project {
            "nara.project-browser"
        } else {
            "nara.hierarchy"
        };
        let hide = if project {
            "nara.hierarchy"
        } else {
            "nara.project-browser"
        };
        let _ = self.workspace.open(show, EditorPanelTarget::ActiveDocument);
        let hidden = self
            .workspace
            .layout()
            .instances()
            .iter()
            .filter(|panel| panel.kind() == hide)
            .map(|panel| panel.id())
            .collect::<Vec<_>>();
        for id in hidden {
            self.workspace.layout_mut().set_visible(id, false);
        }
    }

    fn paint(
        &mut self,
        ui: &mut egui::Ui,
        instance: &EditorPanelInstance,
        input: &mut EguiEditorLayoutInput<'_, '_>,
        actions: &mut Vec<EditorAction>,
        browser_actions: &mut Vec<EditorProjectBrowserLocalAction>,
        viewport: &mut Option<(EguiCentralViewportOutput, Option<EguiEditorViewportIntent>)>,
    ) {
        let Some(title) = self
            .workspace
            .registry()
            .descriptor(instance.kind())
            .map(|descriptor| descriptor.title().to_owned())
        else {
            return;
        };
        ui.push_id(("panel", instance.id()), |ui| {
            let mut closed = false;
            ui.horizontal(|ui| {
                ui.strong(&title);
                if ui.small_button("Close panel").clicked() {
                    closed = self.workspace.layout_mut().close(instance.id());
                }
            });
            if closed {
                return;
            }
            let target = instance.target().document(input.frame);
            if matches!(instance.target(), EditorPanelTarget::Source { .. })
                && (target.is_none() || target != input.frame.workspace().active_document)
            {
                ui.label("Activate this panel's document to edit it.");
                if let Some(document) = target {
                    if ui.button("Activate document").clicked() {
                        actions.push(EditorAction::Navigate(
                            EditorNavigationAction::SetActiveDocument { document },
                        ));
                    }
                } else {
                    ui.label("The targeted source document is not open.");
                }
                return;
            }
            let Some(painter) = self.presentation.painters.get(instance.kind()).copied() else {
                ui.label("This contribution has no egui painting companion.");
                return;
            };
            let widgets = self.presentation.widgets.entry(instance.id()).or_default();
            if widgets.document != target {
                *widgets = EguiPanelWidgets {
                    document: target,
                    panel: EguiEditorPanel::new(),
                };
            }
            let first_action = actions.len();
            {
                let mut context = EguiEditorPanelContext {
                    instance,
                    widgets: &mut widgets.panel,
                    input,
                    actions,
                    browser_actions,
                    viewport,
                    project_browser_selected: project_browser_selected(self.workspace),
                };
                painter(ui, &mut context);
            }
            self.presentation.observe_panel_actions(
                instance.id(),
                instance.target(),
                first_action,
                actions,
            );
        });
    }
}

fn project_browser_selected(workspace: &EditorPanelWorkspace) -> bool {
    workspace
        .layout()
        .instances()
        .iter()
        .any(|panel| panel.kind() == "nara.project-browser" && panel.is_visible())
}

fn paint_hierarchy(ui: &mut egui::Ui, context: &mut EguiEditorPanelContext<'_, '_, '_>) {
    egui::ScrollArea::vertical()
        .id_salt("hierarchy-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            context.widgets.show_hierarchy_panel(
                ui,
                context.input.frame,
                context.input.ui_authoring.as_deref_mut(),
                context.input.selected_viewport,
                context.actions,
            );
        });
}
fn paint_project_browser(ui: &mut egui::Ui, context: &mut EguiEditorPanelContext<'_, '_, '_>) {
    context.widgets.show_project_browser_panel(
        ui,
        context.input.frame,
        context.actions,
        context.browser_actions,
    );
}
fn paint_viewport(ui: &mut egui::Ui, context: &mut EguiEditorPanelContext<'_, '_, '_>) {
    *context.viewport = Some(context.widgets.show_viewport_panel(
        ui,
        context.input.frame,
        context.input.ui_authoring.as_deref_mut(),
        context.input.viewport,
        context.input.selected_viewport,
        context.actions,
    ));
}
fn paint_inspector(ui: &mut egui::Ui, context: &mut EguiEditorPanelContext<'_, '_, '_>) {
    egui::ScrollArea::vertical()
        .id_salt("inspector-scroll")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if context.project_browser_selected {
                context.widgets.show_project_inspector_panel(
                    ui,
                    context.input.frame,
                    context.actions,
                );
            } else {
                context.widgets.show_inspector_panel(
                    ui,
                    context.input.frame,
                    context.input.ui_authoring.as_deref_mut(),
                    context.input.selected_viewport,
                    context.actions,
                );
            }
        });
}
fn paint_diagnostics(ui: &mut egui::Ui, context: &mut EguiEditorPanelContext<'_, '_, '_>) {
    context
        .widgets
        .show_diagnostics_panel(ui, context.input.frame, context.actions);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn restored_project_sidebar_and_reset_derive_the_inspector_mode_from_layout() {
        let mut workspace = EditorPanelWorkspace::default();
        let mut presentation = EguiPanelPresentation::default();
        RegisteredEguiLayout {
            workspace: &mut workspace,
            presentation: &mut presentation,
            active_gesture: None,
        }
        .select_sidebar(true);
        let saved = crate::workspace::EditorWorkspaceLayout::from_json(
            &workspace.layout().to_json().unwrap(),
        )
        .unwrap();
        workspace.restore(saved, "egui", 1, &[]);
        let _fresh = EguiPanelPresentation::default();
        assert!(project_browser_selected(&workspace));
        workspace.reset();
        assert!(!project_browser_selected(&workspace));
    }
}
