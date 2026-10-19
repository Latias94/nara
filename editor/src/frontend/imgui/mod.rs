use dear_imgui_rs::{Condition, TextureId, Ui};
use nara::tooling::{
    EditorAction, EditorActionBatch, EditorCloseDecision, EditorFrameModel, EditorNavigationAction,
    EditorPlayAction, EditorPlayState,
};

mod authoring;
#[cfg(test)]
mod canvas_admission_tests;
mod game;
#[cfg(test)]
mod interaction_tests;
#[cfg(all(test, target_os = "windows"))]
pub(crate) use interaction_tests::InteractionProject;
mod layout;
mod project;
mod ui_authoring;
#[cfg(test)]
mod ui_interaction_tests;
#[cfg(test)]
mod ui_structure_tests;
#[cfg(all(test, target_os = "windows"))]
pub(crate) use ui_interaction_tests::{
    UiProject, clear_controls, control_clip, control_rect, control_visible,
};
mod view;
mod workspace;
pub use workspace::{ImguiEditorPanelContext, ImguiEditorPanelPainter};

#[derive(Clone, Copy)]
pub(crate) struct ImguiViewImage {
    pub(crate) texture: TextureId,
    pub(crate) extent: [u32; 2],
}

#[derive(Default)]
pub(crate) struct ImguiFrontendOutput {
    pub(crate) actions: Option<EditorActionBatch>,
    pub(crate) project_action: Option<crate::EditorProjectAction>,
    pub(crate) shell_action: Option<crate::app::EditorProjectShellAction>,
    pub(crate) browser_actions: Vec<nara::tooling::EditorProjectBrowserLocalAction>,
    pub(crate) layout_changed: bool,
    pub(crate) canvas_pick_uv: Option<[f32; 2]>,
    canvas_display: Option<ImguiCanvasDisplay>,
    canvas_actions: Vec<EditorAction>,
    canvas_panel: Option<(
        crate::workspace::EditorPanelInstanceId,
        crate::workspace::EditorPanelTarget,
    )>,
    pub(crate) scene_mapping: Option<nara::render::DisplayMapping2d>,
    pub(crate) scene_pick_point: Option<[f32; 2]>,
    pub(crate) scene_camera_changed: bool,
    pub(crate) authoring_viewport: Option<dear_imgui_rs::Id>,
    pub(crate) panel_windows: Vec<(crate::workspace::EditorPanelInstanceId, dear_imgui_rs::Id)>,
    pub(crate) scene_extent: Option<[f32; 2]>,
    pub(crate) game_extent: Option<[f32; 2]>,
    pub(crate) game_image: Option<ImguiImageInteraction>,
    pub(crate) text_owner: Option<ImguiTextOwner>,
    pub(crate) save_layout: bool,
    pub(crate) reset_layout: bool,
    pub(crate) restore_layout: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ImguiCanvasDisplay {
    rect: nara::render::DisplayRect,
    viewport: dear_imgui_rs::Id,
    window: dear_imgui_rs::Id,
    pixels_per_point: f32,
}

impl From<ImguiImageInteraction> for ImguiCanvasDisplay {
    fn from(image: ImguiImageInteraction) -> Self {
        Self {
            rect: nara::render::DisplayRect {
                min: [
                    image.rect.min[0] + image.viewport_origin[0],
                    image.rect.min[1] + image.viewport_origin[1],
                ],
                max: [
                    image.rect.max[0] + image.viewport_origin[0],
                    image.rect.max[1] + image.viewport_origin[1],
                ],
            },
            viewport: image.viewport,
            window: image.window,
            pixels_per_point: image.pixels_per_point,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ImguiImageInteraction {
    pub(crate) rect: nara::render::DisplayRect,
    pub(crate) viewport: dear_imgui_rs::Id,
    pub(crate) viewport_origin: [f32; 2],
    pub(crate) pixels_per_point: f32,
    pub(crate) owns_pointer: bool,
    pub(crate) capture_requested: bool,
    pub(crate) window: dear_imgui_rs::Id,
}

impl ImguiImageInteraction {
    pub(crate) fn owns_native_pointer(
        self,
        context: &dear_imgui_rs::Context,
        global: [f32; 2],
    ) -> bool {
        let local = [
            global[0] - self.viewport_origin[0],
            global[1] - self.viewport_origin[1],
        ];
        if !self.rect.contains(local) {
            return false;
        }
        context
            .binding()
            .try_with_bound_context(|| {
                let mut hovered = std::ptr::null_mut();
                let mut under_moving = std::ptr::null_mut();
                // SAFETY: the binding keeps this live Context current on its UI thread. Both
                // queries borrow live window identities; no returned pointer leaves this scope.
                unsafe {
                    dear_imgui_rs::sys::igFindHoveredWindowEx(
                        dear_imgui_rs::sys::ImVec2 {
                            x: global[0],
                            y: global[1],
                        }
                        .into(),
                        true,
                        &mut hovered,
                        &mut under_moving,
                    );
                    let game = dear_imgui_rs::sys::igFindWindowByID(self.window.raw());
                    !game.is_null() && hovered == game
                }
            })
            .unwrap_or(false)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ImguiTextOwner {
    pub(crate) item: dear_imgui_rs::Id,
    pub(crate) viewport: dear_imgui_rs::Id,
}

struct ImguiPanelInput<'frame> {
    frame: &'frame EditorFrameModel,
    ui_authoring: Option<&'frame nara::project_host::EditorUiAuthoringFrame>,
    scene: Option<ImguiViewImage>,
    game: Option<ImguiViewImage>,
    game_captured: bool,
    read_only: bool,
    canvas_presented: bool,
}

/// Dear ImGui widget state. Documents, history, project transitions and panel identities belong
/// to EditorApplication and its existing owners, including across native-window replacement.
#[derive(Default)]
pub struct ImguiEditorFrontend {
    presentation: workspace::ImguiPanelPresentation,
    project: project::ImguiProjectPanels,
    toolbar: authoring::ImguiAuthoringPanels,
    workspace_generation: Option<u64>,
    #[cfg(test)]
    test_game_detached: Option<bool>,
    reset_windows: bool,
}

impl ImguiEditorFrontend {
    pub(crate) fn reject_canvas_frame(
        &mut self,
        mut output: ImguiFrontendOutput,
        authoring: Option<&nara::project_host::EditorUiAuthoringFrame>,
    ) -> Option<EditorActionBatch> {
        let revision = output.actions.take()?.frame_revision();
        // A failed render may only roll back its Canvas gesture, not admit other UI actions.
        output.actions = Some(EditorActionBatch::new(revision, []));
        self.finish_canvas_presentation(&mut output, authoring, false);
        output.actions
    }

    pub(crate) fn finish_canvas_presentation(
        &mut self,
        output: &mut ImguiFrontendOutput,
        authoring: Option<&nara::project_host::EditorUiAuthoringFrame>,
        confirmed: bool,
    ) -> bool {
        let Some((instance, target)) = output.canvas_panel.take() else {
            return false;
        };
        let Some(widgets) = self.presentation.widgets.get_mut(&instance) else {
            return false;
        };
        let changed = widgets.ui_authoring.confirm_display(if confirmed {
            output.canvas_display
        } else {
            None
        });
        if !confirmed {
            let active_gesture = authoring
                .and_then(|frame| frame.gesture())
                .map(|gesture| gesture.stamp().id());
            for action in &mut output.canvas_actions {
                if let EditorAction::Gesture(nara::tooling::EditorGestureAction::Commit {
                    gesture,
                    ..
                }) = action
                    && Some(*gesture) == active_gesture
                {
                    *action = EditorAction::Gesture(nara::tooling::EditorGestureAction::Cancel {
                        gesture: *gesture,
                        reason: nara::tooling::EditorGestureCancelReason::TargetRetired,
                    });
                }
            }
            output.canvas_actions.retain(|action| {
                matches!(
                    action,
                    EditorAction::Gesture(nara::tooling::EditorGestureAction::Cancel { .. })
                )
            });
            if let Some(authoring) = authoring {
                widgets
                    .ui_authoring
                    .cancel_canvas_interaction(authoring, &mut output.canvas_actions);
            }
            output.canvas_pick_uv = None;
        }
        let Some(batch) = output.actions.take() else {
            return changed;
        };
        let revision = batch.frame_revision();
        let mut actions = batch.into_actions();
        let first = actions.len();
        actions.append(&mut output.canvas_actions);
        self.presentation
            .observe_panel_actions(instance, &target, first, &actions);
        output.actions = Some(EditorActionBatch::new(revision, actions));
        changed
    }

    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Installs the optional frontend companion of a separately registered trusted contribution.
    pub fn register_panel_painter(
        &mut self,
        kind: impl Into<String>,
        painter: ImguiEditorPanelPainter,
    ) {
        self.presentation.painters.insert(kind.into(), painter);
    }

    /// Draws the ordinary shell and workspace without a native render-view attachment.
    pub fn show(
        &mut self,
        ui: &Ui,
        application: &mut crate::EditorApplication,
    ) -> Result<Option<EditorActionBatch>, String> {
        let mut output = self.show_with_views(
            ui,
            application,
            None,
            None,
            false,
            false,
            "Native detachment is owned by the product Host",
        )?;
        self.finish_canvas_presentation(
            &mut output,
            application.current_ui_authoring_frame().as_deref(),
            false,
        );
        for action in output.browser_actions {
            let _ = application.apply_project_browser_local_action(action);
        }
        if let Some(action) = output.shell_action {
            application.submit_shell_action(action);
        }
        if let Some(action) = output.project_action {
            application.submit_project_action(action);
        }
        Ok(output.actions)
    }

    /// Observes the exact accepted or rejected batch before advancing the existing application.
    pub fn submit_and_drive(
        &mut self,
        application: &mut crate::EditorApplication,
        batch: EditorActionBatch,
        real_delta: std::time::Duration,
    ) -> crate::EditorApplicationUpdate {
        let dispatch = application.dispatch_editor_actions(batch);
        self.observe_dispatch(&dispatch);
        application.drive_editor_frame(real_delta, dispatch.actions_observed(), true)
    }

    pub(crate) fn observe_dispatch(&mut self, dispatch: &crate::app::EditorApplicationDispatch) {
        if let Some(feedback) = dispatch.feedback() {
            match feedback {
                Ok(report) => self.presentation.observe_report(report),
                Err(rejection) => self.presentation.observe_rejection(rejection),
            }
        }
    }

    pub(crate) fn reset_layout(&mut self) {
        self.retire_panel_widgets();
        self.reset_windows = true;
    }

    pub(crate) fn retire_panel_widgets(&mut self) {
        self.presentation.reset_widgets();
        #[cfg(test)]
        {
            self.test_game_detached = None;
        }
    }

    #[cfg(test)]
    pub(crate) fn place_test_game(&mut self, detached: bool) {
        self.test_game_detached = Some(detached);
    }

    pub(crate) fn scene_open(&self, workspace: &crate::workspace::EditorPanelWorkspace) -> bool {
        panel_visible(workspace, "nara.authoring-view")
    }

    pub(crate) fn game_open(&self, workspace: &crate::workspace::EditorPanelWorkspace) -> bool {
        panel_visible(workspace, "nara.game-view")
    }

    pub(crate) fn show_with_views(
        &mut self,
        ui: &Ui,
        application: &mut crate::EditorApplication,
        scene: Option<ImguiViewImage>,
        game: Option<ImguiViewImage>,
        game_captured: bool,
        canvas_presented: bool,
        detachment: &str,
    ) -> Result<ImguiFrontendOutput, String> {
        let generation = application.workspace_generation();
        if self.workspace_generation != generation {
            self.presentation.reset_widgets();
            self.project.reset_workspace();
            self.toolbar.retire();
            self.workspace_generation = generation;
        }
        let mut output = ImguiFrontendOutput::default();
        let mut actions = Vec::new();
        let reset_windows = std::mem::take(&mut self.reset_windows);
        let condition = if reset_windows {
            Condition::Always
        } else {
            Condition::FirstUseEver
        };
        ui.main_menu_bar(|| {
            ui.menu("Workspace", || {
                let descriptors = application
                    .panel_workspace()
                    .registry()
                    .contributions()
                    .cloned()
                    .collect::<Vec<_>>();
                for descriptor in descriptors {
                    if ui.menu_item(format!("Open {}", descriptor.title())) {
                        let _ = application.panel_workspace_mut().open(
                            descriptor.kind(),
                            crate::workspace::EditorPanelTarget::ActiveDocument,
                        );
                        output.layout_changed = true;
                    }
                }
                ui.separator();
                output.layout_changed |=
                    workspace::show_panel_instances(ui, application.panel_workspace_mut());
                ui.separator();
                output.save_layout = ui.menu_item("Save layout");
                output.restore_layout = ui.menu_item("Restore layout");
                output.reset_layout = ui.menu_item("Reset layout");
            });
            #[cfg(test)]
            ui_interaction_tests::record_control(ui, "Workspace menu");
            ui.text(detachment);
        });

        if let Some(model) = application.project_shell_model() {
            ui.window("Project")
                .size([720.0, 460.0], Condition::FirstUseEver)
                .build(|| {
                    output.shell_action =
                        self.project.show_shell(ui, model, &mut output.text_owner);
                });
        }
        let Some(frame) = application.current_frame() else {
            return Ok(output);
        };
        let default_layout = layout::default_layout(application.panel_workspace())?;
        ui.dockspace()
            .layout(
                &default_layout,
                if reset_windows {
                    dear_imgui_rs::DockLayoutApply::Replace
                } else {
                    dear_imgui_rs::DockLayoutApply::IfMissing
                },
            )
            .build()
            .map_err(|error| error.to_string())?;
        let authoring = application.current_ui_authoring_frame();
        let read_only = application.ordinary_input_locked();
        let viewport = ui.main_viewport();
        let scale = ui.current_font_size() / 16.0;
        let origin = viewport.pos();
        let toolbar_key = layout::toolbar_key();
        ui.window(&toolbar_key)
            .size([760.0 * scale, 0.0], condition)
            .size_constraints(
                [0.0, 0.0],
                [viewport.size()[0] - 24.0 * scale, viewport.size()[1] * 0.6],
            )
            .flags(dear_imgui_rs::WindowFlags::ALWAYS_AUTO_RESIZE)
            .build(|| {
                output.project_action =
                    self.project
                        .show_project_actions(ui, !read_only, &mut output.text_owner);
                self.toolbar.set_read_only(read_only);
                self.toolbar
                    .show_document_controls(ui, &frame, &mut actions);
            });

        let input = ImguiPanelInput {
            frame: &frame,
            ui_authoring: authoring
                .as_deref()
                .filter(|authoring| authoring.editor_frame() == frame.revision()),
            scene,
            game,
            game_captured,
            read_only,
            canvas_presented,
        };
        let workspace = application.panel_workspace_mut();
        self.presentation.painted.clear();
        for reason in workspace.restore_reasons() {
            ui.window("Workspace notices")
                .build(|| ui.text(reason.to_string()));
        }
        let instances = workspace
            .layout()
            .instances()
            .iter()
            .filter(|instance| instance.is_visible())
            .cloned()
            .collect::<Vec<_>>();
        let mut placements = std::collections::BTreeMap::new();
        let mut placement_docks = std::collections::BTreeMap::new();
        for instance in &instances {
            let Some(descriptor) = workspace.registry().descriptor(instance.kind()) else {
                continue;
            };
            let window_name = workspace::panel_window_name(descriptor.title(), instance.id());
            let mut open = true;
            let index = placements.entry(instance.placement()).or_insert(0);
            let (position, size) = default_panel_rect(instance.placement(), *index);
            *index += 1;
            let position = [
                origin[0] + position[0] * scale,
                origin[1] + position[1] * scale,
            ];
            let position_condition = condition;
            let floating = instance.placement() == crate::workspace::EditorPanelPlacement::Floating;
            #[cfg(test)]
            let (mut position, mut position_condition, mut floating) =
                (position, position_condition, floating);
            #[cfg(test)]
            if instance.kind() == "nara.game-view" {
                match self.test_game_detached {
                    Some(true) => {
                        floating = true;
                        ui.set_next_window_dock_id(0.into());
                        position = [
                            origin[0] + ui.main_viewport().size()[0] + 40.0,
                            origin[1] + 100.0,
                        ];
                        position_condition = Condition::Always;
                    }
                    Some(false) => {
                        floating = true;
                        ui.set_next_window_dock_id(0.into());
                        ui.set_next_window_viewport(ui.main_viewport().id());
                        position_condition = Condition::Always;
                    }
                    None => {}
                }
            }
            if !floating && let Some(dock) = placement_docks.get(&instance.placement()) {
                ui.set_next_window_dock_id_with_cond(*dock, Condition::FirstUseEver);
            }
            #[cfg(test)]
            ui.set_next_window_class(
                &dear_imgui_rs::WindowClass::default().viewport_flags_override_set(
                    dear_imgui_rs::WindowClassViewportFlags::NO_FOCUS_ON_APPEARING,
                ),
            );
            let window = ui
                .window(window_name)
                .opened(&mut open)
                .size([size[0] * scale, size[1] * scale], condition);
            #[cfg(test)]
            let window = window.flags(dear_imgui_rs::WindowFlags::NO_FOCUS_ON_APPEARING);
            let window = if floating {
                window.position(position, position_condition)
            } else {
                window
            };
            window.build(|| {
                let dock = ui.get_window_dock_id();
                if dock.raw() != 0 {
                    placement_docks.entry(instance.placement()).or_insert(dock);
                }
                output
                    .panel_windows
                    .push((instance.id(), ui.window_viewport().id()));
                let document = instance.target().document(&frame);
                if matches!(
                    instance.target(),
                    crate::workspace::EditorPanelTarget::Source { .. }
                ) && (document.is_none() || document != frame.workspace().active_document)
                {
                    ui.text("Activate this panel's source document to edit it.");
                    if let Some(document) = document
                        && ui.button("Activate document")
                    {
                        actions.push(EditorAction::Navigate(
                            EditorNavigationAction::SetActiveDocument { document },
                        ));
                    }
                    return;
                }
                let Some(painter) = self.presentation.painters.get(instance.kind()).copied() else {
                    ui.text("This contribution has no Dear ImGui painting companion.");
                    return;
                };
                let widgets = self.presentation.widgets.entry(instance.id()).or_default();
                if widgets.document != document || widgets.target != *instance.target() {
                    *widgets = workspace::ImguiPanelWidgets {
                        document,
                        target: instance.target().clone(),
                        ..Default::default()
                    };
                }
                let first_action = actions.len();
                let scope = document
                    .and_then(|document| {
                        frame.workspace().document(document).map(|tab| {
                            format!(
                                "workspace:{generation:?}:document:{document:?}:revision:{:?}",
                                tab.revision
                            )
                        })
                    })
                    .unwrap_or_else(|| format!("workspace:{generation:?}"));
                let _scope = ui.push_id(scope.as_str());
                painter(
                    ui,
                    &mut ImguiEditorPanelContext {
                        model: crate::workspace::EditorPanelModel::new(&frame, instance),
                        widgets,
                        input: &input,
                        actions: &mut actions,
                        output: &mut output,
                    },
                );
                if !widgets.project_inspector {
                    self.presentation.painted.insert(instance.id());
                }
                self.presentation.observe_panel_actions(
                    instance.id(),
                    instance.target(),
                    first_action,
                    &actions,
                );
            });
            if !open {
                workspace.layout_mut().close(instance.id());
                output.layout_changed = true;
            }
        }
        self.presentation.reconcile_gesture(
            workspace,
            input.ui_authoring.and_then(|authoring| authoring.gesture()),
            &mut actions,
        );
        self.presentation.widgets.retain(|id, _| {
            workspace
                .layout()
                .instance(*id)
                .is_some_and(|instance| instance.is_visible())
        });
        if output.browser_actions.iter().any(|action| {
            matches!(
                action,
                nara::tooling::EditorProjectBrowserLocalAction::Select { .. }
            )
        }) {
            if let Some(instance) = instances
                .iter()
                .find(|instance| instance.kind() == "nara.inspector")
            {
                self.presentation
                    .widgets
                    .entry(instance.id())
                    .or_default()
                    .project_inspector = true;
            }
        }
        show_dirty_close_dialog(ui, &frame, &mut actions);
        output.actions = Some(EditorActionBatch::new(frame.revision(), actions));
        Ok(output)
    }
}

fn show_dirty_close_dialog(ui: &Ui, frame: &EditorFrameModel, actions: &mut Vec<EditorAction>) {
    use nara::tooling::{
        EditorPersistenceOperation, EditorPersistenceRequestKind, EditorWorkspaceIntentPhase,
    };

    if let Some(phase) = frame.workspace_intent().phase() {
        ui.window("Unsaved changes")
            .flags(dear_imgui_rs::WindowFlags::ALWAYS_AUTO_RESIZE)
            .build(|| {
                if phase != EditorWorkspaceIntentPhase::AwaitingDecision {
                    ui.text(match phase {
                        EditorWorkspaceIntentPhase::Saving => "Saving changes...",
                        EditorWorkspaceIntentPhase::RetiringRuntime => {
                            "Stopping the game before closing..."
                        }
                        EditorWorkspaceIntentPhase::Prepared => {
                            "Finishing the document operation..."
                        }
                        EditorWorkspaceIntentPhase::AwaitingDecision => unreachable!(),
                    });
                    return;
                }
                let Some(prompt) = frame.workspace_intent().decision_prompt() else {
                    return;
                };
                let Some(document) = frame.workspace().document(prompt.document()) else {
                    return;
                };
                ui.text("Save changes before closing?");
                ui.text(format!("Document: {}", document.title));
                ui.text(format!(
                    "Source: {}",
                    document
                        .source_binding
                        .as_ref()
                        .map_or("Unsaved", |binding| binding.locator())
                ));
                let save_enabled = frame.persistence().operation()
                    == EditorPersistenceOperation::Idle
                    && authoring::persistence_allows(
                        frame.persistence().result(),
                        EditorPersistenceRequestKind::Save,
                        prompt.document(),
                    );
                for (label, enabled, decision) in [
                    ("Save and continue", save_enabled, EditorCloseDecision::Save),
                    ("Discard and continue", true, EditorCloseDecision::Discard),
                    ("Cancel", true, EditorCloseDecision::Cancel),
                ] {
                    let clicked = ui.with_disabled_if(!enabled, || ui.button(label));
                    #[cfg(test)]
                    interaction_tests::record_control(ui, format!("close:{label}"));
                    if clicked {
                        actions.push(EditorAction::WorkspaceIntent(prompt.resolve(decision)));
                    }
                }
            });
    }
}

fn panel_visible(workspace: &crate::workspace::EditorPanelWorkspace, kind: &str) -> bool {
    workspace
        .layout()
        .instances()
        .iter()
        .any(|instance| instance.kind() == kind && instance.is_visible())
}

fn default_panel_rect(
    placement: crate::workspace::EditorPanelPlacement,
    index: usize,
) -> ([f32; 2], [f32; 2]) {
    use crate::workspace::EditorPanelPlacement;
    let offset = (index % 2) as f32 * 28.0;
    match placement {
        EditorPanelPlacement::Left => ([12.0, 175.0], [250.0, 430.0]),
        EditorPanelPlacement::Center => ([276.0, 175.0 + index as f32 * 285.0], [590.0, 275.0]),
        EditorPanelPlacement::Right => ([880.0 + offset, 175.0 + offset], [310.0, 500.0]),
        EditorPanelPlacement::Bottom => ([12.0, 650.0], [850.0, 150.0]),
        EditorPanelPlacement::Floating => ([320.0 + offset, 220.0 + offset], [400.0, 320.0]),
    }
}

fn paint_hierarchy(ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
    context
        .widgets
        .authoring
        .set_read_only(context.input.read_only);
    context
        .widgets
        .ui_authoring
        .synchronize(context.input.ui_authoring);
    if let Some(authoring) = context.input.ui_authoring {
        context.widgets.ui_authoring.show_tree(
            ui,
            authoring,
            !context.input.read_only,
            context.actions,
            &mut context.output.text_owner,
        );
    } else {
        context
            .widgets
            .authoring
            .show_hierarchy(ui, context.input.frame, context.actions);
    }
}

fn paint_project_browser(ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
    context.widgets.project.show_browser(
        ui,
        context.input.frame.project_browser(),
        !context.input.read_only,
        context.actions,
        &mut context.output.browser_actions,
        &mut context.output.text_owner,
    );
}

fn paint_inspector(ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
    if ui
        .selectable_config("Document")
        .selected(!context.widgets.project_inspector)
        .build()
    {
        context.widgets.project_inspector = false;
    }
    ui.same_line();
    if ui
        .selectable_config("Source asset")
        .selected(context.widgets.project_inspector)
        .build()
    {
        context.widgets.project_inspector = true;
    }
    ui.separator();
    context
        .widgets
        .authoring
        .set_read_only(context.input.read_only);
    context
        .widgets
        .ui_authoring
        .synchronize(context.input.ui_authoring);
    if context.widgets.project_inspector {
        context.widgets.authoring.retire();
        if let Some(authoring) = context.input.ui_authoring {
            context.widgets.ui_authoring.cancel_interaction(
                authoring,
                nara::tooling::EditorGestureCancelReason::TargetRetired,
                context.actions,
            );
        }
        context.widgets.project.show_source_inspector(
            ui,
            context.input.frame.project_browser(),
            context.input.frame.image_import(),
            !context.input.read_only,
            context.input.frame.image_import_work_pending(),
            context.actions,
        );
    } else if let Some(authoring) = context.input.ui_authoring {
        context.widgets.ui_authoring.show_inspector(
            ui,
            authoring,
            !context.input.read_only,
            context.actions,
            &mut context.output.text_owner,
        );
    } else {
        context.widgets.authoring.show_inspector(
            ui,
            context.input.frame,
            context.actions,
            &mut context.output.text_owner,
        );
    }
}

fn paint_authoring_view(ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
    context.output.authoring_viewport = Some(ui.window_viewport().id());
    context
        .widgets
        .ui_authoring
        .synchronize(context.input.ui_authoring);
    if let Some(authoring) = context.input.ui_authoring {
        let canvas = context.widgets.ui_authoring.show_canvas(
            ui,
            authoring,
            ui_authoring::ImguiUiCanvasInput {
                image: context.input.scene,
                presented: context.input.canvas_presented,
                notice: None,
            },
            !context.input.read_only,
            &mut context.output.canvas_actions,
        );
        context.output.canvas_panel = Some((
            context.model.instance().id(),
            context.model.instance().target().clone(),
        ));
        context.output.canvas_display = canvas.display;
        context.output.scene_extent = canvas.desired_extent;
        context.output.canvas_pick_uv = canvas.pick_uv;
    } else {
        let scene = context.widgets.scene_view.show(
            ui,
            context.input.scene,
            context.input.canvas_presented,
            !context.input.read_only,
        );
        context.output.scene_extent = scene.desired_extent;
        context.output.scene_mapping = scene.mapping;
        context.output.scene_pick_point = scene.pick_point;
        context.output.scene_camera_changed = scene.camera_changed;
    }
}

fn paint_game_view(ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
    use nara::tooling::{
        EditorDocumentKind, EditorPersistenceOperation, EditorPlayOperationResult,
        EditorRuntimeAvailability,
    };
    let frame = context.input.frame;
    let scene = frame.workspace().active_scene.as_ref();
    let state = frame.play().state();
    let pending = matches!(
        frame.play().result(),
        Some(EditorPlayOperationResult::Pending { .. })
    );
    let can_request = !context.input.read_only
        && frame.workspace_intent().phase().is_none()
        && frame.persistence().operation() == EditorPersistenceOperation::Idle;
    let content_ready = frame.project_content().is_current()
        && !frame
            .workspace()
            .documents
            .iter()
            .any(|document| document.kind == EditorDocumentKind::Prefab && document.dirty);
    if matches!(
        frame.runtime_availability(),
        EditorRuntimeAvailability::ExternalSupervised
    ) {
        project::show_external_play_controls(
            ui,
            scene,
            frame,
            !frame
                .workspace()
                .documents
                .iter()
                .any(|document| document.dirty),
            can_request,
            pending,
            context.actions,
        );
        project::show_play_result(ui, frame);
        ui.text_wrapped("Play runs in its supervised native window.");
        return;
    }
    let controls: Vec<(&str, bool, EditorPlayAction)> = match state {
        EditorPlayState::Empty => scene
            .into_iter()
            .map(|scene| {
                (
                    "Play",
                    content_ready
                        && !matches!(
                            frame.runtime_availability(),
                            EditorRuntimeAvailability::Unavailable(_)
                        ),
                    EditorPlayAction::Play {
                        document: scene.document,
                        base_revision: scene.revision,
                    },
                )
            })
            .collect(),
        EditorPlayState::PreparingPlay | EditorPlayState::Starting => {
            vec![("Cancel", true, EditorPlayAction::Cancel)]
        }
        EditorPlayState::Running => vec![
            ("Pause", true, EditorPlayAction::Pause),
            ("Stop", true, EditorPlayAction::Stop),
            ("Restart", content_ready, EditorPlayAction::Restart),
        ],
        EditorPlayState::Paused => vec![
            ("Resume", true, EditorPlayAction::Resume),
            ("Step", true, EditorPlayAction::StepFixedTick),
            ("Stop", true, EditorPlayAction::Stop),
            ("Restart", content_ready, EditorPlayAction::Restart),
        ],
        EditorPlayState::Stepping => vec![("Stop", true, EditorPlayAction::Stop)],
        EditorPlayState::Faulted => vec![
            ("Stop", true, EditorPlayAction::Stop),
            ("Restart", content_ready, EditorPlayAction::Restart),
        ],
        EditorPlayState::RetirementIncomplete => {
            vec![("Retry retirement", true, EditorPlayAction::RetryRetirement)]
        }
        EditorPlayState::CloseIncomplete => {
            vec![("Retry close", true, EditorPlayAction::RetryClose)]
        }
        EditorPlayState::RetiringPlay | EditorPlayState::Stopping => Vec::new(),
    };
    // Keep button/status/result feedback in one stable region. Its own overflow can scroll;
    // appearing or wrapping messages never change the Game render target's geometry.
    ui.child_window("embedded_game_controls")
        .size([
            0.0,
            ui.frame_height_with_spacing() + ui.text_line_height_with_spacing() * 2.0,
        ])
        .build(ui, || {
            for (label, enabled, action) in controls {
                let cleanup = matches!(
                    action,
                    EditorPlayAction::RetryClose | EditorPlayAction::RetryRetirement
                );
                let while_pending =
                    matches!(action, EditorPlayAction::Cancel | EditorPlayAction::Stop);
                let clicked = ui.with_disabled_if(
                    !enabled || (!can_request && !cleanup) || (pending && !while_pending),
                    || ui.button(label),
                );
                #[cfg(test)]
                ui_interaction_tests::record_control(ui, format!("embedded:{label}"));
                if clicked {
                    context.actions.push(EditorAction::Play(action));
                }
                ui.same_line();
            }
            ui.new_line();
            ui.text(format!("{state:?}"));
            project::show_play_result(ui, frame);
            if let EditorRuntimeAvailability::Unavailable(reason) = frame.runtime_availability() {
                project::show_runtime_unavailable(ui, reason);
            }
        });
    (context.output.game_extent, context.output.game_image) = context.widgets.game_view.show(
        ui,
        context.input.game,
        frame.play(),
        context.input.game_captured,
    );
    #[cfg(test)]
    {
        let min = ui.window_pos();
        let size = ui.window_size();
        ui_interaction_tests::record_rect(
            "embedded:Game window",
            nara::render::DisplayRect {
                min,
                max: [min[0] + size[0], min[1] + size[1]],
            },
        );
        if let Some(image) = context.output.game_image {
            let origin = image.viewport_origin;
            ui_interaction_tests::record_rect(
                "embedded:Game image",
                nara::render::DisplayRect {
                    min: [image.rect.min[0] + origin[0], image.rect.min[1] + origin[1]],
                    max: [image.rect.max[0] + origin[0], image.rect.max[1] + origin[1]],
                },
            );
        }
    }
}

fn paint_diagnostics(ui: &Ui, context: &mut ImguiEditorPanelContext<'_, '_>) {
    let frame = context.input.frame;
    ui.with_disabled_if(context.input.read_only, || {
        project::show_editor_status(ui, frame, context.actions);
    });
    for diagnostic in frame.diagnostics().iter() {
        ui.text_wrapped(format!(
            "{:?} {}: {}",
            diagnostic.severity(),
            diagnostic.code().as_str(),
            diagnostic.summary().as_str()
        ));
    }
    if let Some(scene) = frame.workspace().active_scene.as_ref() {
        for diagnostic in scene.editor.inspector.diagnostics.iter() {
            ui.text_wrapped(format!(
                "Inspector {:?} {}: {}",
                diagnostic.severity(),
                diagnostic.code().as_str(),
                diagnostic.summary().as_str()
            ));
        }
    }
    project::show_external_play_output(ui, frame);
}

fn show_image(
    ui: &Ui,
    image: Option<ImguiViewImage>,
    unavailable: &str,
) -> (Option<[f32; 2]>, Option<ImguiImageInteraction>) {
    let available = ui.content_region_avail();
    let size = [available[0].max(1.0), available[1].max(1.0)];
    let scale = framebuffer_scale(ui);
    let extent = [size[0] * scale[0], size[1] * scale[1]];
    if let Some(image) = image {
        let scale = (size[0] / image.extent[0] as f32).min(size[1] / image.extent[1] as f32);
        ui.image(
            image.texture,
            [
                image.extent[0] as f32 * scale,
                image.extent[1] as f32 * scale,
            ],
        );
        return (Some(extent), Some(image_interaction(ui)));
    } else {
        ui.text(unavailable);
    }
    (Some(extent), None)
}

fn image_interaction(ui: &Ui) -> ImguiImageInteraction {
    let (min, max) = ui.item_rect();
    let viewport = ui.window_viewport();
    let origin = viewport.pos();
    ImguiImageInteraction {
        rect: nara::render::DisplayRect {
            min: [min[0] - origin[0], min[1] - origin[1]],
            max: [max[0] - origin[0], max[1] - origin[1]],
        },
        viewport: viewport.id(),
        viewport_origin: origin,
        pixels_per_point: framebuffer_scale(ui)[0],
        owns_pointer: ui.is_item_hovered(),
        capture_requested: ui.is_item_clicked(),
        window: ui.with_bound_context(|| {
            // SAFETY: the live image item belongs to the current borrowed window.
            unsafe { dear_imgui_rs::Id::from((*dear_imgui_rs::sys::igGetCurrentWindow()).ID) }
        }),
    }
}

fn framebuffer_scale(ui: &Ui) -> [f32; 2] {
    ui.window_viewport().framebuffer_scale()
}

fn observe_text_owner(ui: &Ui, owner: &mut Option<ImguiTextOwner>) {
    if !ui.is_item_active() {
        return;
    }
    let current = ImguiTextOwner {
        item: ui.item_id(),
        viewport: ui.window_viewport().id(),
    };
    *owner = Some(current);
}
