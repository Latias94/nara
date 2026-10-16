use std::collections::BTreeMap;

use dear_imgui_rs::{Condition, TextureId, Ui};
use nara::{
    reflect::ComponentValue,
    tooling::{
        EditorAction, EditorActionBatch, EditorCloseDecision, EditorDocumentMutation,
        EditorFieldTarget, EditorFrameModel, EditorNavigationAction, EditorPersistenceAction,
        EditorPlayAction, EditorPlayState, EditorWorkspaceIntentAction, SceneInspectorCommand,
    },
};

#[derive(Clone, Copy)]
pub(crate) struct ImguiViewImage {
    pub(crate) texture: TextureId,
    pub(crate) extent: [u32; 2],
}

#[derive(Default)]
pub(crate) struct ImguiFrontendOutput {
    pub(crate) actions: Option<EditorActionBatch>,
    pub(crate) scene_extent: Option<[f32; 2]>,
    pub(crate) game_extent: Option<[f32; 2]>,
    pub(crate) game_image: Option<ImguiImageInteraction>,
    pub(crate) scene_panel_rect: Option<nara::render::DisplayRect>,
    pub(crate) game_panel_rect: Option<nara::render::DisplayRect>,
    pub(crate) text_owner: Option<ImguiTextOwner>,
    pub(crate) text_probe_rect: Option<nara::render::DisplayRect>,
    pub(crate) text_probe_window_rect: Option<nara::render::DisplayRect>,
    pub(crate) play_rect: Option<nara::render::DisplayRect>,
    pub(crate) stop_rect: Option<nara::render::DisplayRect>,
    pub(crate) save_layout: bool,
    pub(crate) reset_layout: bool,
    pub(crate) restore_layout: bool,
}

#[derive(Clone, Copy)]
pub(crate) struct ImguiImageInteraction {
    pub(crate) rect: nara::render::DisplayRect,
    pub(crate) viewport: dear_imgui_rs::Id,
    pub(crate) viewport_origin: [f32; 2],
    pub(crate) pixels_per_point: f32,
    pub(crate) owns_pointer: bool,
    pub(crate) capture_requested: bool,
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
                    let game = dear_imgui_rs::sys::igFindWindowByName(c"Game".as_ptr());
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

pub(crate) struct ImguiPreedit {
    pub(crate) owner: ImguiTextOwner,
    pub(crate) text: String,
    pub(crate) cursor: Option<(usize, usize)>,
}

struct FieldDraft {
    target: EditorFieldTarget,
    text: String,
}

pub(crate) struct ImguiEditorFrontend {
    drafts: BTreeMap<String, FieldDraft>,
    text_probe: String,
    inspector_open: bool,
    hierarchy_open: bool,
    diagnostics_open: bool,
    scene_open: bool,
    game_open: bool,
    qualification_game_detached: Option<bool>,
    reset_windows: bool,
}

impl Default for ImguiEditorFrontend {
    fn default() -> Self {
        Self {
            drafts: BTreeMap::new(),
            text_probe: String::new(),
            inspector_open: true,
            hierarchy_open: true,
            diagnostics_open: true,
            scene_open: true,
            game_open: true,
            qualification_game_detached: None,
            reset_windows: false,
        }
    }
}

impl ImguiEditorFrontend {
    pub(crate) fn text_probe(&self) -> &str {
        &self.text_probe
    }
    pub(crate) fn reset_layout(&mut self) {
        self.inspector_open = true;
        self.hierarchy_open = true;
        self.diagnostics_open = true;
        self.scene_open = true;
        self.game_open = true;
        self.qualification_game_detached = None;
        self.reset_windows = true;
    }

    pub(crate) fn restore_panel_state(&mut self, layout: &crate::workspace::EditorWorkspaceLayout) {
        for panel in layout.instances() {
            let open = match panel.kind() {
                "nara.hierarchy" => &mut self.hierarchy_open,
                "nara.inspector" => &mut self.inspector_open,
                "nara.diagnostics" => &mut self.diagnostics_open,
                "nara.authoring-view" => &mut self.scene_open,
                "nara.game-view" => &mut self.game_open,
                _ => continue,
            };
            *open = panel.is_open();
        }
    }

    pub(crate) fn save_panel_state(&self, layout: &mut crate::workspace::EditorWorkspaceLayout) {
        let states = layout
            .instances()
            .iter()
            .filter_map(|panel| {
                let open = match panel.kind() {
                    "nara.hierarchy" => self.hierarchy_open,
                    "nara.inspector" => self.inspector_open,
                    "nara.diagnostics" => self.diagnostics_open,
                    "nara.authoring-view" => self.scene_open,
                    "nara.game-view" => self.game_open,
                    _ => return None,
                };
                Some((panel.id(), open))
            })
            .collect::<Vec<_>>();
        for (id, open) in states {
            if open {
                layout.reopen(id);
            } else {
                layout.close(id);
            }
        }
    }
    pub(crate) fn place_qualification_game(&mut self, detached: bool) {
        self.qualification_game_detached = Some(detached);
    }
    pub(crate) fn set_qualification_game_open(&mut self, open: bool) {
        self.game_open = open;
    }
    pub(crate) fn set_qualification_scene_open(&mut self, open: bool) {
        self.scene_open = open;
    }
    pub(crate) const fn scene_open(&self) -> bool {
        self.scene_open
    }
    pub(crate) const fn game_open(&self) -> bool {
        self.game_open
    }

    pub(crate) fn show(
        &mut self,
        ui: &Ui,
        frame: Option<&EditorFrameModel>,
        scene: Option<ImguiViewImage>,
        game: Option<ImguiViewImage>,
        detachment: &str,
        status: &str,
        preedit: Option<&ImguiPreedit>,
    ) -> Result<ImguiFrontendOutput, String> {
        ui.dockspace().build().map_err(|error| error.to_string())?;
        let reset_windows = std::mem::take(&mut self.reset_windows);
        let condition = if reset_windows {
            Condition::Always
        } else {
            Condition::FirstUseEver
        };
        let origin = ui.main_viewport().pos();
        let scale = ui.current_font_size() / 16.0;
        let position = |x: f32, y: f32| [origin[0] + x * scale, origin[1] + y * scale];
        let size = |width: f32, height: f32| [width * scale, height * scale];
        let mut output = ImguiFrontendOutput::default();
        let mut actions = Vec::new();
        prepare_default_window(ui, reset_windows);
        ui.window("Qualification")
            .position(position(12.0, 32.0), condition)
            .size(size(1160.0, 130.0), condition)
            .build(|| {
                ui.text("Nara / Dear ImGui 0.20.0 qualification (not the production frontend)");
                ui.text(detachment);
                ui.text(status);
                output.save_layout = ui.button("Save layout");
                ui.same_line();
                output.restore_layout = ui.button("Restore layout");
                ui.same_line();
                output.reset_layout = ui.button("Reset layout");
                ui.same_line();
                if ui.button("Reopen panels") {
                    self.inspector_open = true;
                    self.hierarchy_open = true;
                    self.diagnostics_open = true;
                    self.scene_open = true;
                    self.game_open = true;
                }
                ui.input_text("IME / clipboard", &mut self.text_probe)
                    .build();
                let (min, max) = ui.item_rect();
                output.text_probe_rect = Some(nara::render::DisplayRect { min, max });
                let min = ui.window_pos();
                let size = ui.window_size();
                output.text_probe_window_rect = Some(nara::render::DisplayRect {
                    min,
                    max: [min[0] + size[0], min[1] + size[1]],
                });
                observe_text_owner(ui, &mut output.text_owner, preedit);
            });
        let Some(frame) = frame else {
            self.drafts.clear();
            ui.window("Project status")
                .size([640.0, 240.0], Condition::FirstUseEver)
                .build(|| ui.text(status));
            return Ok(output);
        };
        self.drafts.retain(|_, draft| {
            frame.workspace().documents.iter().any(|document| {
                document.document == draft.target.document()
                    && document.revision == draft.target.base_revision()
            })
        });
        if self.hierarchy_open {
            prepare_default_window(ui, reset_windows);
            ui.window("Hierarchy")
                .opened(&mut self.hierarchy_open)
                .position(position(12.0, 174.0), condition)
                .size(size(250.0, 420.0), condition)
                .build(|| {
                    for tab in &frame.workspace().documents {
                        if ui
                            .selectable_config(&tab.title)
                            .selected(tab.active)
                            .build()
                        {
                            actions.push(EditorAction::Navigate(
                                EditorNavigationAction::SetActiveDocument {
                                    document: tab.document,
                                },
                            ));
                        }
                    }
                    ui.separator();
                    if let Some(scene) = &frame.workspace().active_scene {
                        for entity in &scene.editor.inspector.entities {
                            let label =
                                format!("{}{}", "  ".repeat(entity.depth), entity.id.as_str());
                            if ui
                                .selectable_config(label)
                                .selected(entity.selected)
                                .build()
                            {
                                actions.push(EditorAction::Navigate(
                                    EditorNavigationAction::SelectEntity {
                                        document: scene.document,
                                        entity: Some(entity.id.clone()),
                                    },
                                ));
                            }
                        }
                    }
                });
        }
        let mut inspector_open = self.inspector_open;
        if inspector_open {
            prepare_default_window(ui, reset_windows);
            ui.window("Inspector")
                .opened(&mut inspector_open)
                .position(position(880.0, 174.0), condition)
                .size(size(300.0, 500.0), condition)
                .build(|| {
                    self.show_inspector(ui, frame, &mut actions, &mut output.text_owner, preedit)
                });
        }
        self.inspector_open = inspector_open;
        if self.diagnostics_open {
            prepare_default_window(ui, reset_windows);
            ui.window("Diagnostics")
                .opened(&mut self.diagnostics_open)
                .position(position(12.0, 610.0), condition)
                .size(size(850.0, 165.0), condition)
                .build(|| {
                    ui.text(format!("Readiness: {:?}", frame.project_readiness()));
                    ui.text(format!("Persistence: {:?}", frame.persistence()));
                    for diagnostic in frame.diagnostics().iter() {
                        ui.text(format!(
                            "{:?}: {}",
                            diagnostic.severity(),
                            diagnostic.summary().as_str()
                        ));
                    }
                });
        }
        if self.scene_open {
            prepare_default_window(ui, reset_windows);
            ui.window("Scene")
                .opened(&mut self.scene_open)
                .position(position(276.0, 174.0), condition)
                .size(size(590.0, 275.0), condition)
                .build(|| {
                    (output.scene_extent, _) =
                        show_image(ui, scene, "Waiting for a Nara Scene image");
                    output.scene_panel_rect = Some(panel_rect(ui));
                });
        }
        if self.game_open {
            prepare_default_window(ui, reset_windows);
            let (game_position, game_condition) = match self.qualification_game_detached {
                Some(true) => (
                    [
                        origin[0] + ui.main_viewport().size()[0] + 40.0 * scale,
                        origin[1] + 100.0 * scale,
                    ],
                    Condition::Always,
                ),
                Some(false) => {
                    ui.set_next_window_viewport(ui.main_viewport().id());
                    (position(276.0, 464.0), Condition::Always)
                }
                None => (position(276.0, 464.0), condition),
            };
            ui.window("Game")
                .opened(&mut self.game_open)
                .position(game_position, game_condition)
                .size(size(590.0, 275.0), condition)
                .build(|| {
                    if let Some(scene) = &frame.workspace().active_scene {
                        let play = match frame.play().state() {
                            EditorPlayState::Empty => Some((
                                "Play",
                                EditorPlayAction::Play {
                                    document: scene.document,
                                    base_revision: scene.revision,
                                },
                            )),
                            EditorPlayState::Running => Some(("Pause", EditorPlayAction::Pause)),
                            EditorPlayState::Paused => Some(("Resume", EditorPlayAction::Resume)),
                            _ => None,
                        };
                        if let Some((label, play)) = play {
                            if ui.button(label) {
                                actions.push(EditorAction::Play(play));
                            }
                            let (min, max) = ui.item_rect();
                            output.play_rect = Some(nara::render::DisplayRect { min, max });
                        }
                        ui.same_line();
                        if ui.button("Stop") {
                            actions.push(EditorAction::Play(EditorPlayAction::Stop));
                        }
                        let (min, max) = ui.item_rect();
                        output.stop_rect = Some(nara::render::DisplayRect { min, max });
                        ui.same_line();
                        if ui.button("Step") {
                            actions.push(EditorAction::Play(EditorPlayAction::StepFixedTick));
                        }
                        ui.text(format!("{:?}", frame.play().state()));
                    }
                    (output.game_extent, output.game_image) =
                        show_image(ui, game, "Start an admitted embedded Game View");
                    output.game_panel_rect = Some(panel_rect(ui));
                });
        }
        if frame.workspace_intent().phase().is_some() {
            ui.window("Unsaved changes")
                .size([500.0, 160.0], Condition::Appearing)
                .build(|| {
                    ui.text("The existing document owner requires a close decision.");
                    for (label, decision) in [
                        ("Save and close", EditorCloseDecision::Save),
                        ("Discard and close", EditorCloseDecision::Discard),
                        ("Cancel", EditorCloseDecision::Cancel),
                    ] {
                        if ui.button(label) {
                            actions.push(EditorAction::WorkspaceIntent(
                                EditorWorkspaceIntentAction::Resolve(decision),
                            ));
                        }
                        ui.same_line();
                    }
                });
        }
        output.actions = Some(EditorActionBatch::new(frame.revision(), actions));
        Ok(output)
    }

    fn show_inspector(
        &mut self,
        ui: &Ui,
        frame: &EditorFrameModel,
        actions: &mut Vec<EditorAction>,
        text_owner: &mut Option<ImguiTextOwner>,
        preedit: Option<&ImguiPreedit>,
    ) {
        let Some(scene) = &frame.workspace().active_scene else {
            ui.text("No active document");
            return;
        };
        ui.text(format!(
            "{}{}",
            scene.title,
            if scene.dirty { " *" } else { "" }
        ));
        for (label, action) in [
            (
                "Undo",
                EditorAction::Mutate(EditorDocumentMutation::Undo {
                    document: scene.document,
                    base_revision: scene.revision,
                }),
            ),
            (
                "Redo",
                EditorAction::Mutate(EditorDocumentMutation::Redo {
                    document: scene.document,
                    base_revision: scene.revision,
                }),
            ),
            (
                "Save",
                EditorAction::Persistence(EditorPersistenceAction::Save {
                    document: scene.document,
                    base_revision: scene.revision,
                }),
            ),
            (
                "Reopen",
                EditorAction::Persistence(EditorPersistenceAction::Reopen {
                    target: frame.reopen_target(),
                }),
            ),
        ] {
            if ui.button(label) {
                actions.push(action);
            }
            ui.same_line();
        }
        ui.new_line();
        let Some(entity) = &scene.editor.inspector.selected_entity_view else {
            ui.text("Select an entity in Hierarchy");
            return;
        };
        ui.text(entity.id.as_str());
        for component in &entity.components {
            ui.separator();
            ui.text(component.component.as_str());
            if !component.access.is_editable() {
                ui.text(format!(
                    "Read only: {:?}",
                    component.access.read_only_reason()
                ));
            }
            for field in &component.fields {
                let Some(
                    value @ (ComponentValue::Bool(_)
                    | ComponentValue::I64(_)
                    | ComponentValue::U64(_)
                    | ComponentValue::F64(_)
                    | ComponentValue::String(_)),
                ) = field.value.as_ref()
                else {
                    ui.text(format!("{}: {:?}", field.id.as_str(), field.value));
                    continue;
                };
                let target = EditorFieldTarget::new(
                    scene.document,
                    scene.revision,
                    entity.id.clone(),
                    component.component.clone(),
                    component.document_version,
                    field.id.clone(),
                );
                let key = format!(
                    "{}/{}/{}",
                    entity.id.as_str(),
                    component.component.as_str(),
                    field.id.as_str()
                );
                let draft = self
                    .drafts
                    .entry(key.clone())
                    .or_insert_with(|| FieldDraft {
                        target: target.clone(),
                        text: format_scalar(value).expect("supported scalar"),
                    });
                if draft.target != target {
                    draft.target = target;
                    draft.text = format_scalar(value).expect("supported scalar");
                }
                let _id = ui.push_id(key.as_str());
                ui.with_disabled_if(!component.access.is_editable(), || {
                    let commit = ui
                        .input_text(field.id.as_str(), &mut draft.text)
                        .enter_returns_true(true)
                        .build();
                    observe_text_owner(ui, text_owner, preedit);
                    if commit {
                        if let Some(value) = parse_scalar(field.value.as_ref(), &draft.text) {
                            actions.push(EditorAction::Mutate(
                                EditorDocumentMutation::ApplyInspectorCommand {
                                    document: draft.target.document(),
                                    base_revision: draft.target.base_revision(),
                                    command: SceneInspectorCommand::SetField {
                                        entity: draft.target.entity().clone(),
                                        component: draft.target.component().clone(),
                                        component_version: draft.target.component_version(),
                                        field: draft.target.field().clone(),
                                        value,
                                    },
                                },
                            ));
                        }
                    }
                });
            }
        }
        if let Some(feedback) = frame.field_feedback() {
            ui.text(format!("Field feedback: {:?}", feedback.reason()));
        }
    }
}

fn prepare_default_window(ui: &Ui, reset: bool) {
    if reset {
        ui.set_next_window_viewport(ui.main_viewport().id());
        ui.set_next_window_dock_id(0.into());
    }
}

fn panel_rect(ui: &Ui) -> nara::render::DisplayRect {
    let min = ui.window_pos();
    let size = ui.window_size();
    nara::render::DisplayRect {
        min,
        max: [min[0] + size[0], min[1] + size[1]],
    }
}

fn show_image(
    ui: &Ui,
    image: Option<ImguiViewImage>,
    unavailable: &str,
) -> (Option<[f32; 2]>, Option<ImguiImageInteraction>) {
    let available = ui.content_region_avail();
    let size = [available[0].max(1.0), available[1].max(1.0)];
    let scale = ui.window_viewport().framebuffer_scale();
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
        let (min, max) = ui.item_rect();
        let viewport = ui.window_viewport();
        let origin = viewport.pos();
        return (
            Some(extent),
            Some(ImguiImageInteraction {
                rect: nara::render::DisplayRect {
                    min: [min[0] - origin[0], min[1] - origin[1]],
                    max: [max[0] - origin[0], max[1] - origin[1]],
                },
                viewport: viewport.id(),
                viewport_origin: origin,
                pixels_per_point: viewport.framebuffer_scale()[0],
                owns_pointer: ui.is_item_hovered(),
                capture_requested: ui.is_item_clicked(),
            }),
        );
    } else {
        ui.text(unavailable);
    }
    (Some(extent), None)
}

fn observe_text_owner(ui: &Ui, owner: &mut Option<ImguiTextOwner>, preedit: Option<&ImguiPreedit>) {
    if !ui.is_item_active() {
        return;
    }
    let current = ImguiTextOwner {
        item: ui.item_id(),
        viewport: ui.window_viewport().id(),
    };
    *owner = Some(current);
    let Some(preedit) =
        preedit.filter(|preedit| preedit.owner == current && !preedit.text.is_empty())
    else {
        return;
    };
    let (min, max) = ui.item_rect();
    let position = ui.with_bound_context(|| {
        // SAFETY: copy only the current frame's caret facts from the bound live Context. Dear
        // fills these after the active InputText, using the same coordinates as its IME callback.
        let data = unsafe { (*dear_imgui_rs::sys::igGetCurrentContext()).PlatformImeData };
        if data.WantVisible && data.ViewportId == current.viewport.raw() {
            [data.InputPos.x, data.InputPos.y]
        } else {
            [min[0], max[1] + 3.0]
        }
    });
    let size = ui.calc_text_size(&preedit.text);
    let draw = ui.with_bound_context(|| {
        // SAFETY: this live window's viewport and foreground list belong to the borrowed Ui
        // frame. The wrapper stays local and is the only mutable borrow of this list.
        unsafe {
            let viewport = dear_imgui_rs::sys::igGetWindowViewport();
            let list = dear_imgui_rs::sys::igGetForegroundDrawList_ViewportPtr(viewport);
            dear_imgui_rs::DrawListMut::from_raw_mut(ui, list)
        }
    });
    draw.add_rect(
        [position[0] - 3.0, position[1] - 2.0],
        [position[0] + size[0] + 3.0, position[1] + size[1] + 3.0],
        [0.08, 0.09, 0.12, 1.0],
    )
    .filled(true)
    .build();
    draw.add_text(position, [1.0, 1.0, 1.0, 1.0], &preedit.text);
    draw.add_line(
        [position[0], position[1] + size[1] + 1.0],
        [position[0] + size[0], position[1] + size[1] + 1.0],
        [0.5, 0.75, 1.0, 1.0],
    )
    .build();
    if let Some((start, end)) = preedit.cursor {
        if let (Some(before), Some(active)) =
            (preedit.text.get(..start), preedit.text.get(start..end))
        {
            let left = position[0] + ui.calc_text_size(before)[0];
            let right = left + ui.calc_text_size(active)[0];
            draw.add_line(
                [left, position[1]],
                [left, position[1] + size[1]],
                [1.0, 0.8, 0.25, 1.0],
            )
            .build();
            draw.add_line(
                [left, position[1] + size[1]],
                [right, position[1] + size[1]],
                [1.0, 0.8, 0.25, 1.0],
            )
            .thickness(2.0)
            .build();
        }
    }
}

fn format_scalar(value: &ComponentValue) -> Option<String> {
    match value {
        ComponentValue::Bool(value) => Some(value.to_string()),
        ComponentValue::I64(value) => Some(value.to_string()),
        ComponentValue::U64(value) => Some(value.to_string()),
        ComponentValue::F64(value) => Some(value.get().to_string()),
        ComponentValue::String(value) => Some(value.clone()),
        _ => None,
    }
}

fn parse_scalar(current: Option<&ComponentValue>, text: &str) -> Option<ComponentValue> {
    match current? {
        ComponentValue::Bool(_) => text.parse().ok().map(ComponentValue::Bool),
        ComponentValue::I64(_) => text.parse().ok().map(ComponentValue::I64),
        ComponentValue::U64(_) => text.parse().ok().map(ComponentValue::U64),
        ComponentValue::F64(_) => ComponentValue::f64(text.parse().ok()?).ok(),
        ComponentValue::String(_) => Some(ComponentValue::String(text.to_owned())),
        _ => None,
    }
}
