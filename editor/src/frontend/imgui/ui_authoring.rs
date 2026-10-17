use std::collections::HashMap;

use dear_imgui_rs::{Key, MouseButton, Ui};
use nara::{
    project_host::EditorUiAuthoringFrame,
    render::DisplayRect,
    scene::{SceneAuthoringRevision, SceneEntityId, ScenePlacement, ScenePlacementAnchor},
    tooling::{
        EditorAction, EditorActionBatchReport, EditorActionOutcome, EditorActionRejection,
        EditorDocumentId, EditorGestureAction, EditorGestureCancelReason, EditorGestureStamp,
        EditorNavigationAction, PrefabCompositionOutcome,
    },
    ui::{
        __private::{
            UiAuthoringCanvasProjection, UiAuthoringCommitPolicy, UiAuthoringDimensionKind,
            UiAuthoringDirectEdit, UiAuthoringEditability, UiAuthoringFieldDescriptor,
            UiAuthoringFieldGroup, UiAuthoringFieldKey, UiAuthoringFieldKind,
            UiAuthoringFieldValue, UiAuthoringStructuralCapability, UiBrushAuthoringKind,
            UiCatalogNodeKind, UiCreateParameters, UiDirectManipulationCapability,
            UiImageChoiceGeneration, UiImageChoiceSelection, UiPlacementCandidate,
            UiStructureIntent,
        },
        UiActionId,
    },
};

use super::{ImguiPreedit, ImguiTextOwner, ImguiViewImage, observe_text_owner, show_image};

#[derive(Clone, Copy, PartialEq, Eq)]
struct Scope {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    images: UiImageChoiceGeneration,
}

struct FieldDraft {
    value: UiAuthoringFieldValue,
    dirty: bool,
    error: Option<String>,
}

struct ContinuousField {
    key: UiAuthoringFieldKey,
    value: UiAuthoringFieldValue,
    last_sent: Option<UiAuthoringFieldValue>,
    commit: bool,
}

enum StructureDialog {
    Create {
        kind: UiCatalogNodeKind,
        placement: ScenePlacement,
        text: String,
        image: Option<UiImageChoiceSelection>,
        label: String,
        action: String,
    },
    Move {
        target: SceneEntityId,
        placement: ScenePlacement,
    },
    Delete(SceneEntityId),
}

#[derive(Clone, Copy)]
enum DragKind {
    Move,
    Resize,
}

struct CanvasDrag {
    entity: SceneEntityId,
    kind: DragKind,
    transform: [f32; 7],
    pointer_origin: [f32; 2],
    latest: [f64; 2],
    applied: [f64; 2],
    sent: Option<EditorGestureStamp>,
    commit: bool,
}

impl CanvasDrag {
    fn observe_pointer(&mut self, pointer: [f32; 2], units_per_point: [f64; 2], released: bool) {
        if !self.commit {
            self.latest = [
                f64::from(pointer[0] - self.pointer_origin[0]) * units_per_point[0],
                f64::from(pointer[1] - self.pointer_origin[1]) * units_per_point[1],
            ];
        }
        self.commit |= released;
    }
}

struct CanvasSelection {
    entity: SceneEntityId,
    rect: DisplayRect,
    resize: bool,
    movable: bool,
    units_per_point: [f64; 2],
    transform: [f32; 7],
}

#[derive(Default)]
pub(super) struct ImguiUiAuthoringPanels {
    scope: Option<Scope>,
    fields: HashMap<UiAuthoringFieldKey, FieldDraft>,
    continuous: Option<ContinuousField>,
    structure: Option<StructureDialog>,
    structure_pending: Option<usize>,
    structure_error: Option<String>,
    focus_entity: Option<SceneEntityId>,
    focus_hierarchy: bool,
    drag: Option<CanvasDrag>,
    displayed: Option<super::ImguiCanvasDisplay>,
}

pub(super) struct ImguiUiCanvasInput<'a> {
    pub(super) image: Option<ImguiViewImage>,
    pub(super) presented: bool,
    pub(super) notice: Option<&'a str>,
}

pub(super) struct ImguiUiCanvasOutput {
    pub(super) desired_extent: Option<[f32; 2]>,
    pub(super) pick_uv: Option<[f32; 2]>,
    pub(super) display: Option<super::ImguiCanvasDisplay>,
}

impl ImguiUiAuthoringPanels {
    pub(super) fn confirm_display(&mut self, display: Option<super::ImguiCanvasDisplay>) -> bool {
        let changed = self.displayed != display;
        self.displayed = display;
        changed
    }

    pub(super) fn cancel_canvas_interaction(
        &mut self,
        frame: &EditorUiAuthoringFrame,
        actions: &mut Vec<EditorAction>,
    ) {
        self.cancel_drag(frame, EditorGestureCancelReason::TargetRetired, actions);
    }

    pub(super) fn synchronize(&mut self, authoring: Option<&EditorUiAuthoringFrame>) {
        let next = authoring.map(|frame| Scope {
            document: frame.document(),
            revision: frame.revision(),
            images: frame.image_choice_generation(),
        });
        if self.scope != next {
            let same_document = self
                .scope
                .zip(next)
                .is_some_and(|(before, after)| before.document == after.document);
            let unchanged_revision = self.scope.zip(next).is_some_and(|(before, after)| {
                before.document == after.document && before.revision == after.revision
            });
            let focus_entity = same_document.then(|| self.focus_entity.take()).flatten();
            let focus_hierarchy = same_document && self.focus_hierarchy;
            let continuous = unchanged_revision.then(|| self.continuous.take()).flatten();
            let drag = unchanged_revision.then(|| self.drag.take()).flatten();
            *self = Self {
                scope: next,
                continuous,
                drag,
                focus_entity,
                focus_hierarchy,
                ..Self::default()
            };
        }
    }

    pub(super) fn observe_report(&mut self, report: &EditorActionBatchReport) {
        let Some(index) = self.structure_pending.take() else {
            return;
        };
        match report.outcomes().get(index) {
            Some(EditorActionOutcome::Workspace(report))
                if report.prefab_composition_outcome.is_some() =>
            {
                if let Some(outcome) = &report.prefab_composition_outcome {
                    self.focus_entity = match outcome {
                        PrefabCompositionOutcome::Created { entity }
                        | PrefabCompositionOutcome::Moved { entity }
                        | PrefabCompositionOutcome::ComponentAdded { entity, .. }
                        | PrefabCompositionOutcome::ComponentRemoved { entity, .. }
                        | PrefabCompositionOutcome::NoOp { entity } => Some(entity.clone()),
                        PrefabCompositionOutcome::Removed { selection, .. } => selection.clone(),
                    };
                    self.focus_hierarchy = self.focus_entity.is_none();
                }
                self.structure = None;
                self.structure_error = None;
            }
            Some(EditorActionOutcome::Rejected(rejection)) => {
                self.structure_error = Some(format!("Structure change rejected: {rejection:?}"));
            }
            _ => self.structure_error = Some("The structure change was not applied".to_owned()),
        }
    }

    pub(super) fn observe_rejection(&mut self, rejection: EditorActionRejection) {
        if self.structure_pending.take().is_some() {
            self.structure_error = Some(format!("Structure change rejected: {rejection:?}"));
        }
    }

    pub(super) fn show_tree(
        &mut self,
        ui: &Ui,
        frame: &EditorUiAuthoringFrame,
        editing: bool,
        actions: &mut Vec<EditorAction>,
        text_owner: &mut Option<ImguiTextOwner>,
        preedit: Option<&ImguiPreedit>,
    ) {
        let Ok(projection) = frame.projection() else {
            ui.text("The UI source projection is unavailable");
            return;
        };
        let enabled = editing && frame.structure_editing_enabled();
        let selected = frame.selection().top_entity().and_then(|entity| {
            projection
                .hierarchy()
                .iter()
                .find(|node| node.entity() == entity)
        });
        {
            let _disabled =
                ui.begin_disabled_with_cond(!enabled || self.structure_pending.is_some());
            if enabled && self.focus_hierarchy {
                ui.set_keyboard_focus_here();
                self.focus_hierarchy = false;
            }
            let add = ui.button("Add UI node");
            #[cfg(test)]
            super::ui_interaction_tests::record_control(ui, "Add UI node");
            if add {
                match projection.structure_placement_candidates(None) {
                    Ok(candidates) if !candidates.is_empty() => {
                        self.structure = Some(StructureDialog::Create {
                            kind: UiCatalogNodeKind::Container,
                            placement: candidates[0].placement().clone(),
                            text: "Text".to_owned(),
                            image: None,
                            label: "Button".to_owned(),
                            action: String::new(),
                        });
                        self.structure_error = None;
                    }
                    Ok(_) => {
                        self.structure_error =
                            Some("No source-local placement is available".to_owned())
                    }
                    Err(error) => self.structure_error = Some(error.to_string()),
                }
            }
            ui.same_line();
            let _selected_disabled = ui.begin_disabled_with_cond(selected.is_none_or(|node| {
                node.structural_capability() != UiAuthoringStructuralCapability::Editable
            }));
            let move_subtree = ui.button("Move subtree");
            #[cfg(test)]
            super::ui_interaction_tests::record_control(ui, "Move subtree");
            if move_subtree && let Some(node) = selected {
                match projection.structure_placement_candidates(Some(node.entity())) {
                    Ok(candidates) if !candidates.is_empty() => {
                        let placement = candidates
                            .iter()
                            .find(|candidate| candidate.is_current())
                            .unwrap_or(&candidates[0])
                            .placement()
                            .clone();
                        self.structure = Some(StructureDialog::Move {
                            target: node.entity().clone(),
                            placement,
                        });
                        self.structure_error = None;
                    }
                    Ok(_) => {
                        self.structure_error = Some("No source-local move is available".to_owned())
                    }
                    Err(error) => self.structure_error = Some(error.to_string()),
                }
            }
            ui.same_line();
            let delete = ui.button("Delete subtree");
            #[cfg(test)]
            super::ui_interaction_tests::record_control(ui, "Delete subtree");
            if delete && let Some(node) = selected {
                self.structure = Some(StructureDialog::Delete(node.entity().clone()));
                self.structure_error = None;
            }
        }
        if editing && !frame.structure_editing_enabled() {
            ui.text_disabled("Structure changes are unavailable in preservation-safe authoring");
        }
        if let Some(node) = selected
            && let UiAuthoringStructuralCapability::ReadOnly(reason) = node.structural_capability()
        {
            ui.text_disabled(format!("Structure is read-only: {reason:?}"));
        }
        self.show_structure(ui, frame, enabled, actions, text_owner, preedit);
        ui.separator();
        for node in projection.hierarchy() {
            let _id = ui.push_id(node.entity().as_str());
            let indent = f32::from(node.depth()) * 14.0;
            if indent > 0.0 {
                ui.indent_by(indent);
            }
            let selected = frame
                .selection()
                .entities()
                .any(|entity| entity == node.entity());
            if self.focus_entity.as_ref() == Some(node.entity()) {
                ui.set_keyboard_focus_here();
                self.focus_entity = None;
            }
            if ui
                .selectable_config(node.display_label())
                .selected(selected)
                .build()
            {
                actions.push(EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document: frame.document(),
                        entity: Some(node.entity().clone()),
                    },
                ));
            }
            #[cfg(test)]
            super::ui_interaction_tests::record_control(
                ui,
                format!("ui-entity:{}", node.entity().as_str()),
            );
            if ui.is_item_hovered() {
                ui.tooltip_text(format!("{}: {}", node.kind().label(), node.display_path()));
            }
            if indent > 0.0 {
                ui.unindent_by(indent);
            }
        }
    }

    fn show_structure(
        &mut self,
        ui: &Ui,
        frame: &EditorUiAuthoringFrame,
        enabled: bool,
        actions: &mut Vec<EditorAction>,
        text_owner: &mut Option<ImguiTextOwner>,
        preedit: Option<&ImguiPreedit>,
    ) {
        let Some(mut dialog) = self.structure.take() else {
            if let Some(error) = &self.structure_error {
                ui.text(error);
            }
            return;
        };
        if ui.is_window_focused() && ui.is_key_pressed(Key::Escape) {
            self.structure_error = None;
            self.focus_hierarchy = true;
            return;
        }
        let Ok(projection) = frame.projection() else {
            return;
        };
        ui.separator();
        let _disabled = ui.begin_disabled_with_cond(!enabled || self.structure_pending.is_some());
        let intent = match &mut dialog {
            StructureDialog::Create {
                kind,
                placement,
                text,
                image,
                label,
                action,
            } => {
                let mut index = UiCatalogNodeKind::ALL
                    .iter()
                    .position(|item| item == kind)
                    .unwrap_or(0);
                let labels = UiCatalogNodeKind::ALL.map(UiCatalogNodeKind::label);
                let kind_changed = ui.combo_simple_string("Node kind", &mut index, &labels);
                #[cfg(test)]
                super::ui_interaction_tests::record_control(ui, "Node kind");
                if kind_changed {
                    *kind = UiCatalogNodeKind::ALL[index];
                }
                let placement_valid = match projection.structure_placement_candidates(None) {
                    Ok(candidates) => placement_control(ui, placement, &candidates),
                    Err(error) => {
                        ui.text(error.to_string());
                        false
                    }
                };
                let parameters = match kind {
                    UiCatalogNodeKind::Container => Some(UiCreateParameters::Container),
                    UiCatalogNodeKind::Text => {
                        ui.input_text("Text", text).build();
                        observe_text_owner(ui, text_owner, preedit);
                        Some(UiCreateParameters::Text {
                            content: text.clone(),
                        })
                    }
                    UiCatalogNodeKind::Image => {
                        match frame.image_choices() {
                            Ok(choices) => {
                                let preview = image
                                    .as_ref()
                                    .map_or("Select image", UiImageChoiceSelection::project_path);
                                let image_combo = ui.begin_combo("Image", preview);
                                #[cfg(test)]
                                super::ui_interaction_tests::record_control(ui, "Create image");
                                if let Some(_combo) = image_combo {
                                    for choice in choices.choices() {
                                        if ui.selectable(choice.project_path()) {
                                            *image = Some(choice.selection(choices.generation()));
                                        }
                                        #[cfg(test)]
                                        super::ui_interaction_tests::record_control(
                                            ui,
                                            format!("create-image:{}", choice.project_path()),
                                        );
                                    }
                                }
                            }
                            Err(error) => ui.text(error.to_string()),
                        }
                        Some(UiCreateParameters::Image {
                            selection: image.clone(),
                        })
                    }
                    UiCatalogNodeKind::Button => {
                        ui.input_text("Label", label).build();
                        observe_text_owner(ui, text_owner, preedit);
                        ui.input_text("Action", action).build();
                        observe_text_owner(ui, text_owner, preedit);
                        match UiActionId::new(action.trim().to_owned()) {
                            Ok(action) => Some(UiCreateParameters::Button {
                                label: label.clone(),
                                action: Some(action),
                            }),
                            Err(error) => {
                                ui.text(error.to_string());
                                None
                            }
                        }
                    }
                };
                parameters
                    .filter(|_| placement_valid)
                    .map(|parameters| UiStructureIntent::Create {
                        parameters,
                        placement: placement.clone(),
                    })
            }
            StructureDialog::Move { target, placement } => {
                ui.text(format!("Move {}", target.as_str()));
                match projection.structure_placement_candidates(Some(target)) {
                    Ok(candidates) if placement_control(ui, placement, &candidates) => {
                        Some(UiStructureIntent::Move {
                            target: target.clone(),
                            placement: placement.clone(),
                        })
                    }
                    Ok(_) => None,
                    Err(error) => {
                        ui.text(error.to_string());
                        None
                    }
                }
            }
            StructureDialog::Delete(target) => {
                ui.text(format!(
                    "Delete {} and its source-local subtree?",
                    target.as_str()
                ));
                ui.text_disabled("Undo remains available after deletion");
                Some(UiStructureIntent::Delete {
                    target: target.clone(),
                })
            }
        };
        let prepared = intent
            .as_ref()
            .map(|intent| projection.prepare_structure(intent, frame.image_choices().ok()));
        if let Some(Err(error)) = &prepared {
            ui.text(error.to_string());
        }
        let _invalid = ui.begin_disabled_with_cond(!matches!(prepared, Some(Ok(_))));
        let submit = ui.button("Apply structure change");
        #[cfg(test)]
        super::ui_interaction_tests::record_control(ui, "Apply structure change");
        drop(_invalid);
        ui.same_line();
        let cancel = ui.button("Cancel");
        if let Some(error) = &self.structure_error {
            ui.text(error);
        }
        if cancel {
            self.structure_error = None;
            self.focus_hierarchy = true;
            return;
        }
        if submit && let Some(intent) = intent {
            match frame.lower_structure_intent(&intent) {
                Ok(mutation) => {
                    self.structure_pending = Some(actions.len());
                    self.structure_error = None;
                    actions.push(EditorAction::Mutate(mutation));
                }
                Err(error) => self.structure_error = Some(error.to_string()),
            }
        }
        self.structure = Some(dialog);
    }

    pub(super) fn show_inspector(
        &mut self,
        ui: &Ui,
        frame: &EditorUiAuthoringFrame,
        editing: bool,
        actions: &mut Vec<EditorAction>,
        text_owner: &mut Option<ImguiTextOwner>,
        preedit: Option<&ImguiPreedit>,
    ) {
        let Ok(projection) = frame.projection() else {
            ui.text("The UI authoring projection was rejected");
            return;
        };
        let Some(selected) = frame.selection().top_entity() else {
            ui.text("Select a UI node");
            return;
        };
        ui.text(selected.as_str());
        let mut group = None;
        for field in projection
            .fields()
            .iter()
            .filter(|field| field.key().entity() == selected)
        {
            if group != Some(field.group()) {
                group = Some(field.group());
                ui.separator();
                ui.text(group_label(field.group()));
            }
            let id = format!(
                "{}:{}:{}",
                selected.as_str(),
                field.key().component().as_str(),
                field.key().primary_field().as_str()
            );
            let _id = ui.push_id(id.as_str());
            let editable = editing && frame.field_editing_enabled(field.key());
            let draft = self
                .fields
                .entry(field.key().clone())
                .or_insert_with(|| FieldDraft {
                    value: field.value().clone(),
                    dirty: false,
                    error: None,
                });
            let _disabled = ui.begin_disabled_with_cond(!editable);
            ui.text(field.label());
            let response = field_control(ui, frame, field, &mut draft.value, text_owner, preedit);
            draft.dirty |= response.changed;
            if let UiAuthoringEditability::ReadOnly(reason) = field.editability() {
                ui.text_disabled(format!("Read-only: {reason:?}"));
            } else if !frame.field_editing_enabled(field.key()) {
                ui.text_disabled("This field is read-only in preservation-safe authoring");
            }
            if let Some(error) = &draft.error {
                ui.text(error);
            }
            drop(_disabled);
            if !editable {
                continue;
            }
            if response.escape {
                if self
                    .continuous
                    .as_ref()
                    .is_some_and(|draft| draft.key == *field.key())
                {
                    self.cancel_interaction(frame, EditorGestureCancelReason::Escape, actions);
                } else {
                    self.fields.remove(field.key());
                }
                continue;
            }
            let proposed = draft.value.clone();
            if field.commit_policy() == UiAuthoringCommitPolicy::Discrete {
                if draft.dirty && response.commit {
                    match frame.lower_discrete_field_edit(field.key(), proposed) {
                        Ok(mutation) => {
                            draft.dirty = false;
                            draft.error = None;
                            actions.push(EditorAction::Mutate(mutation));
                        }
                        Err(error) => draft.error = Some(error.to_string()),
                    }
                }
            } else {
                self.continuous_field(frame, field, proposed, response, actions);
            }
        }
    }

    fn continuous_field(
        &mut self,
        frame: &EditorUiAuthoringFrame,
        field: &UiAuthoringFieldDescriptor,
        proposed: UiAuthoringFieldValue,
        response: ControlResponse,
        actions: &mut Vec<EditorAction>,
    ) {
        let gesture = matching_field_gesture(frame, field.key());
        if gesture.is_none() {
            if response.changed && frame.gesture().is_none() {
                match frame.lower_field_edit(field.key(), field.value().clone()) {
                    Ok(baseline) => {
                        actions.push(EditorAction::Gesture(EditorGestureAction::Begin {
                            baseline,
                        }));
                        self.continuous = Some(ContinuousField {
                            key: field.key().clone(),
                            value: proposed,
                            last_sent: None,
                            commit: response.commit,
                        });
                    }
                    Err(error) => {
                        self.fields
                            .get_mut(field.key())
                            .expect("visible field has a draft")
                            .error = Some(error.to_string())
                    }
                }
            }
            return;
        }
        let Some(draft) = self
            .continuous
            .as_mut()
            .filter(|draft| draft.key == *field.key())
        else {
            return;
        };
        if response.changed {
            draft.value = proposed;
        }
        draft.commit |= response.commit;
        let value = draft.value.clone();
        let gesture = gesture.expect("the matching gesture was checked").id();
        match frame.lower_field_edit(field.key(), value.clone()) {
            Ok(edit) if draft.commit => {
                actions.push(EditorAction::Gesture(EditorGestureAction::Commit {
                    gesture,
                    edit,
                }));
                self.continuous = None;
            }
            Ok(edit) if draft.last_sent.as_ref() != Some(&value) => {
                actions.push(EditorAction::Gesture(EditorGestureAction::Preview {
                    gesture,
                    edit,
                }));
                draft.last_sent = Some(value);
            }
            Ok(_) => {}
            Err(error) => {
                self.fields
                    .get_mut(field.key())
                    .expect("visible field has a draft")
                    .error = Some(error.to_string())
            }
        }
    }

    pub(super) fn show_canvas(
        &mut self,
        ui: &Ui,
        frame: &EditorUiAuthoringFrame,
        input: ImguiUiCanvasInput<'_>,
        editing: bool,
        actions: &mut Vec<EditorAction>,
    ) -> ImguiUiCanvasOutput {
        let (desired_extent, image) =
            show_image(ui, input.image, "Waiting for a Nara UI Canvas image");
        let Some(image) = image else {
            self.cancel_drag(frame, EditorGestureCancelReason::TargetRetired, actions);
            return ImguiUiCanvasOutput {
                desired_extent,
                pick_uv: None,
                display: None,
            };
        };
        let rect = DisplayRect {
            min: [
                image.rect.min[0] + image.viewport_origin[0],
                image.rect.min[1] + image.viewport_origin[1],
            ],
            max: [
                image.rect.max[0] + image.viewport_origin[0],
                image.rect.max[1] + image.viewport_origin[1],
            ],
        };
        let cursor = ui.cursor_screen_pos();
        ui.set_cursor_screen_pos(rect.min);
        ui.invisible_button(
            "##ui-canvas-input",
            [rect.max[0] - rect.min[0], rect.max[1] - rect.min[1]],
        );
        #[cfg(test)]
        super::ui_interaction_tests::record_control(ui, "canvas-input");
        let hovered = ui.is_item_hovered();
        let active = ui.is_item_active();
        let released = ui.is_item_deactivated() && ui.is_mouse_released(MouseButton::Left);
        ui.set_cursor_screen_pos(cursor);
        ui.dummy([0.0, 0.0]);
        let display = super::ImguiCanvasDisplay::from(image);
        let presented = input.presented && self.displayed == Some(display);
        let interactive =
            presented && input.notice.is_none() && editing && frame.direct_editing_enabled();
        let selection = paint_canvas_selection(ui, frame, rect, interactive);
        let was_dragging = self.drag.is_some();
        if interactive
            && self.drag.is_none()
            && frame.gesture().is_none()
            && active
            && ui.is_mouse_dragging(MouseButton::Left)
            && let Some(selection) = selection.as_ref()
        {
            let mouse = ui.mouse_pos();
            let dragged = ui.mouse_drag_delta(MouseButton::Left);
            let origin = [mouse[0] - dragged[0], mouse[1] - dragged[1]];
            let handle = resize_handle(selection.rect);
            let kind = if selection.resize && handle.contains(origin) {
                Some(DragKind::Resize)
            } else if selection.movable && selection.rect.contains(origin) {
                Some(DragKind::Move)
            } else {
                None
            };
            if let Some(kind) = kind {
                match frame.lower_direct_edit(&direct_edit(kind, &selection.entity, [0.0, 0.0])) {
                    Ok(baseline) => {
                        actions.push(EditorAction::Gesture(EditorGestureAction::Begin {
                            baseline,
                        }));
                        self.drag = Some(CanvasDrag {
                            entity: selection.entity.clone(),
                            kind,
                            transform: selection.transform,
                            pointer_origin: origin,
                            latest: [0.0, 0.0],
                            applied: [0.0, 0.0],
                            sent: None,
                            commit: false,
                        });
                    }
                    Err(error) => ui.text(error.to_string()),
                }
            }
        }
        if !editing || !frame.direct_editing_enabled() {
            self.cancel_drag(frame, EditorGestureCancelReason::SourceHandoff, actions);
        } else if !ui.is_window_focused() {
            self.cancel_drag(frame, EditorGestureCancelReason::FocusLost, actions);
        } else if ui.is_key_pressed(Key::Escape) {
            self.cancel_drag(frame, EditorGestureCancelReason::Escape, actions);
        } else if input.notice.is_some() || self.displayed != Some(display) {
            self.cancel_drag(frame, EditorGestureCancelReason::TargetRetired, actions);
        } else {
            self.advance_drag(ui, frame, selection.as_ref(), presented, released, actions);
        }
        let pick_uv = (editing
            && presented
            && input.notice.is_none()
            && hovered
            && released
            && !was_dragging
            && self.drag.is_none())
        .then(|| point_to_uv(rect, ui.mouse_pos()))
        .flatten();
        if let Some(notice) = input.notice {
            ui.text(notice);
        }
        ImguiUiCanvasOutput {
            desired_extent,
            pick_uv,
            display: Some(display),
        }
    }

    pub(super) fn cancel_interaction(
        &mut self,
        frame: &EditorUiAuthoringFrame,
        reason: EditorGestureCancelReason,
        actions: &mut Vec<EditorAction>,
    ) {
        self.cancel_drag(frame, reason, actions);
        if let Some(field) = self.continuous.take() {
            self.fields.remove(&field.key);
            if let Some(gesture) = matching_field_gesture(frame, &field.key) {
                actions.push(EditorAction::Gesture(EditorGestureAction::Cancel {
                    gesture: gesture.id(),
                    reason,
                }));
            }
        }
    }

    fn cancel_drag(
        &mut self,
        frame: &EditorUiAuthoringFrame,
        reason: EditorGestureCancelReason,
        actions: &mut Vec<EditorAction>,
    ) {
        if let Some(drag) = self.drag.take()
            && let Some(gesture) = frame
                .gesture()
                .filter(|gesture| gesture.baseline().target().entity() == &drag.entity)
        {
            actions.push(EditorAction::Gesture(EditorGestureAction::Cancel {
                gesture: gesture.stamp().id(),
                reason,
            }));
        }
    }

    fn advance_drag(
        &mut self,
        ui: &Ui,
        frame: &EditorUiAuthoringFrame,
        selection: Option<&CanvasSelection>,
        presented: bool,
        released: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        let Some(mut drag) = self.drag.take() else {
            return;
        };
        let gesture = frame
            .gesture()
            .filter(|gesture| {
                let target = gesture.baseline().target();
                target.document() == frame.document()
                    && target.base_revision() == frame.revision()
                    && target.entity() == &drag.entity
            })
            .map(|gesture| gesture.stamp());
        let Some(selection) = selection.filter(|selection| {
            selection.entity == drag.entity && selection.transform == drag.transform
        }) else {
            if let Some(gesture) = gesture {
                actions.push(EditorAction::Gesture(EditorGestureAction::Cancel {
                    gesture: gesture.id(),
                    reason: EditorGestureCancelReason::CanvasTransformChanged,
                }));
            }
            return;
        };
        drag.observe_pointer(ui.mouse_pos(), selection.units_per_point, released);
        if let Some(gesture) = gesture
            && presented
            && drag.sent != Some(gesture)
        {
            let delta = [
                drag.latest[0] - drag.applied[0],
                drag.latest[1] - drag.applied[1],
            ];
            match frame.lower_direct_edit(&direct_edit(drag.kind, &drag.entity, delta)) {
                Ok(edit) if drag.commit => {
                    actions.push(EditorAction::Gesture(EditorGestureAction::Commit {
                        gesture: gesture.id(),
                        edit,
                    }));
                    return;
                }
                Ok(edit) if delta != [0.0, 0.0] => {
                    actions.push(EditorAction::Gesture(EditorGestureAction::Preview {
                        gesture: gesture.id(),
                        edit,
                    }));
                    drag.applied = drag.latest;
                    drag.sent = Some(gesture);
                }
                Ok(_) => {}
                Err(_) => {
                    actions.push(EditorAction::Gesture(EditorGestureAction::Cancel {
                        gesture: gesture.id(),
                        reason: EditorGestureCancelReason::InvalidPreview,
                    }));
                    return;
                }
            }
        }
        self.drag = Some(drag);
    }
}

fn placement_control(
    ui: &Ui,
    placement: &mut ScenePlacement,
    candidates: &[UiPlacementCandidate],
) -> bool {
    let labels = candidates
        .iter()
        .map(|candidate| {
            let anchor = match &candidate.placement().anchor {
                ScenePlacementAnchor::First => "first".to_owned(),
                ScenePlacementAnchor::Last => "last".to_owned(),
                ScenePlacementAnchor::Before { sibling } => format!("before {}", sibling.as_str()),
                ScenePlacementAnchor::After { sibling } => format!("after {}", sibling.as_str()),
            };
            format!(
                "{} / {anchor}{}",
                candidate.parent().as_str(),
                if candidate.is_current() {
                    " (current)"
                } else {
                    ""
                }
            )
        })
        .collect::<Vec<_>>();
    let mut index = candidates
        .iter()
        .position(|candidate| candidate.placement() == placement)
        .unwrap_or(candidates.len());
    let changed = ui.combo_simple_string("Placement", &mut index, &labels);
    #[cfg(test)]
    super::ui_interaction_tests::record_control(ui, "Placement");
    if changed && let Some(candidate) = candidates.get(index) {
        *placement = candidate.placement().clone();
    }
    candidates
        .iter()
        .any(|candidate| candidate.placement() == placement)
}

#[derive(Default)]
struct ControlResponse {
    changed: bool,
    commit: bool,
    escape: bool,
}

fn field_control(
    ui: &Ui,
    frame: &EditorUiAuthoringFrame,
    field: &UiAuthoringFieldDescriptor,
    value: &mut UiAuthoringFieldValue,
    text_owner: &mut Option<ImguiTextOwner>,
    preedit: Option<&ImguiPreedit>,
) -> ControlResponse {
    let before = value.clone();
    let mut commit = false;
    let mut text = false;
    match (field.kind(), &mut *value) {
        (UiAuthoringFieldKind::Bool, UiAuthoringFieldValue::Bool(value)) => {
            commit = ui.checkbox("##value", value);
        }
        (UiAuthoringFieldKind::SignedInteger, UiAuthoringFieldValue::SignedInteger(value)) => {
            ui.drag("##value", value);
        }
        (UiAuthoringFieldKind::UnsignedInteger, UiAuthoringFieldValue::UnsignedInteger(value)) => {
            ui.drag("##value", value);
        }
        (UiAuthoringFieldKind::Scalar(metadata), UiAuthoringFieldValue::Scalar(value)) => {
            ui.drag_config("##value")
                .speed(metadata.step() as f32)
                .range(
                    metadata.minimum().unwrap_or(f64::MIN),
                    metadata.maximum().unwrap_or(f64::MAX),
                )
                .build(ui, value);
        }
        (UiAuthoringFieldKind::Text { multiline }, UiAuthoringFieldValue::Text(value)) => {
            if *multiline {
                ui.input_text_multiline("##value", value, [0.0, 60.0])
                    .build();
            } else {
                ui.input_text("##value", value).build();
            }
            text = true;
        }
        (UiAuthoringFieldKind::Action, UiAuthoringFieldValue::Action(value)) => {
            ui.input_text("##value", value).build();
            text = true;
        }
        (UiAuthoringFieldKind::Enum { choices }, UiAuthoringFieldValue::Enum(value)) => {
            let mut index = choices
                .iter()
                .position(|choice| *choice == value.as_str())
                .unwrap_or(choices.len());
            if ui.combo_simple_string("##value", &mut index, choices)
                && let Some(choice) = choices.get(index)
            {
                *value = (*choice).to_owned();
                commit = true;
            }
        }
        (UiAuthoringFieldKind::Dimension, UiAuthoringFieldValue::Dimension { kind, value }) => {
            let kinds = [
                UiAuthoringDimensionKind::Auto,
                UiAuthoringDimensionKind::Pixels,
                UiAuthoringDimensionKind::Percent,
            ];
            let mut index = kinds.iter().position(|item| item == kind).unwrap_or(0);
            if ui.combo_simple_string("Units", &mut index, &["Auto", "Pixels", "Percent"]) {
                *kind = kinds[index];
                commit = true;
            }
            if *kind != UiAuthoringDimensionKind::Auto {
                ui.drag("##value", value);
            }
        }
        (
            UiAuthoringFieldKind::Color,
            UiAuthoringFieldValue::Color {
                red,
                green,
                blue,
                alpha,
            },
        ) => {
            let mut rgba = [*red as f32, *green as f32, *blue as f32, *alpha as f32];
            if ui.color_edit4("##value", &mut rgba) {
                *red = f64::from(rgba[0]);
                *green = f64::from(rgba[1]);
                *blue = f64::from(rgba[2]);
                *alpha = f64::from(rgba[3]);
            }
        }
        (
            UiAuthoringFieldKind::Brush,
            UiAuthoringFieldValue::Brush {
                kind,
                red,
                green,
                blue,
                alpha,
                reference,
            },
        ) => {
            let kinds = [
                UiBrushAuthoringKind::Solid,
                UiBrushAuthoringKind::Role,
                UiBrushAuthoringKind::Image,
            ];
            let mut index = kinds.iter().position(|item| item == kind).unwrap_or(0);
            if ui.combo_simple_string("Brush", &mut index, &["Solid", "Theme role", "Image"]) {
                *kind = kinds[index];
                reference.clear();
                commit = *kind == UiBrushAuthoringKind::Solid;
            }
            match kind {
                UiBrushAuthoringKind::Solid => {
                    let mut rgba = [*red as f32, *green as f32, *blue as f32, *alpha as f32];
                    if ui.color_edit4("##value", &mut rgba) {
                        *red = f64::from(rgba[0]);
                        *green = f64::from(rgba[1]);
                        *blue = f64::from(rgba[2]);
                        *alpha = f64::from(rgba[3]);
                    }
                }
                UiBrushAuthoringKind::Role => {
                    ui.input_text("Role", reference).build();
                    text = true;
                }
                UiBrushAuthoringKind::Image => {
                    if let Ok(choices) = frame.image_choices()
                        && let Some(_combo) = ui.begin_combo("Image", reference.as_str())
                    {
                        for choice in choices.choices() {
                            if ui.selectable(choice.project_path()) {
                                *reference = choice.project_path().to_owned();
                                commit = true;
                            }
                        }
                    }
                }
            }
        }
        (_, value) => ui.text_disabled(format!("{value:?}")),
    }
    let active = ui.is_item_active();
    let deactivated = ui.is_item_deactivated();
    #[cfg(test)]
    if matches!(
        field.key().primary_field().as_str(),
        "style.visible" | "style.flex_grow"
    ) {
        super::ui_interaction_tests::record_control(
            ui,
            format!(
                "ui-field:{}:{}",
                field.key().entity().as_str(),
                field.key().primary_field().as_str()
            ),
        );
    }
    if text {
        observe_text_owner(ui, text_owner, preedit);
    }
    commit |= ui.is_item_deactivated_after_edit();
    let changed = *value != before;
    if changed && !text && !active {
        commit = true;
    }
    if let UiAuthoringFieldKind::Scalar(metadata) = field.kind()
        && let Some(unit) = metadata.unit()
    {
        ui.same_line();
        ui.text_disabled(unit);
    }
    ControlResponse {
        changed,
        commit,
        escape: (active || deactivated) && ui.is_key_pressed(Key::Escape),
    }
}

fn matching_field_gesture(
    frame: &EditorUiAuthoringFrame,
    key: &UiAuthoringFieldKey,
) -> Option<EditorGestureStamp> {
    frame
        .gesture()
        .filter(|gesture| {
            let target = gesture.baseline().target();
            target.document() == frame.document()
                && target.base_revision() == frame.revision()
                && target.entity() == key.entity()
                && target.component() == key.component()
                && target.field() == key.primary_field()
        })
        .map(|gesture| gesture.stamp())
}

fn group_label(group: UiAuthoringFieldGroup) -> &'static str {
    match group {
        UiAuthoringFieldGroup::Root => "Root",
        UiAuthoringFieldGroup::Layout => "Layout",
        UiAuthoringFieldGroup::Flex => "Flex",
        UiAuthoringFieldGroup::Appearance => "Appearance",
        UiAuthoringFieldGroup::Text => "Text",
        UiAuthoringFieldGroup::Interaction => "Interaction",
        UiAuthoringFieldGroup::Theme => "Theme",
        UiAuthoringFieldGroup::Semantics => "Semantics",
    }
}

fn point_to_uv(rect: DisplayRect, point: [f32; 2]) -> Option<[f32; 2]> {
    if !rect.contains(point) {
        return None;
    }
    let size = [rect.max[0] - rect.min[0], rect.max[1] - rect.min[1]];
    (size[0] > 0.0 && size[1] > 0.0).then(|| {
        [
            (point[0] - rect.min[0]) / size[0],
            (point[1] - rect.min[1]) / size[1],
        ]
    })
}

fn resize_handle(rect: DisplayRect) -> DisplayRect {
    DisplayRect {
        min: [rect.max[0] - 5.0, rect.max[1] - 5.0],
        max: [rect.max[0] + 5.0, rect.max[1] + 5.0],
    }
}

fn paint_canvas_selection(
    ui: &Ui,
    frame: &EditorUiAuthoringFrame,
    image: DisplayRect,
    interactive: bool,
) -> Option<CanvasSelection> {
    let projection = frame.projection().ok()?;
    let selected = frame.selection().top_entity()?;
    let UiAuthoringCanvasProjection::Ready(canvas) = projection.canvas() else {
        return None;
    };
    let fact = canvas
        .facts()
        .iter()
        .find(|fact| fact.entity() == selected && fact.visible())?;
    let node = projection
        .hierarchy()
        .iter()
        .find(|node| node.entity() == selected)?;
    let transform = canvas.content_transform();
    let physical = transform.physical_surface_size();
    let origin = transform.physical_content_origin();
    let scale = transform.content_scale_factor();
    let size = [image.max[0] - image.min[0], image.max[1] - image.min[1]];
    let map = |point: [f32; 2]| {
        [
            image.min[0] + (origin.x() + point[0] * scale) / physical.width() * size[0],
            image.min[1] + (origin.y() + point[1] * scale) / physical.height() * size[1],
        ]
    };
    let min = fact.rect().origin();
    let max = fact.rect().max();
    let rect = DisplayRect {
        min: map([min.x(), min.y()]),
        max: map([max.x(), max.y()]),
    };
    let color = if interactive {
        [0.3, 0.65, 1.0, 1.0]
    } else {
        [0.5, 0.5, 0.5, 1.0]
    };
    let draw = ui.get_window_draw_list();
    let resize = node.gizmos().resize_node() == UiDirectManipulationCapability::Editable;
    #[cfg(test)]
    super::ui_interaction_tests::record_rect("canvas-selection", rect);
    draw.with_clip_rect(image.min, image.max, || {
        draw.add_rect(rect.min, rect.max, color)
            .thickness(1.5)
            .build();
        if resize {
            let handle = resize_handle(rect);
            #[cfg(test)]
            super::ui_interaction_tests::record_rect("canvas-resize", handle);
            draw.add_rect(handle.min, handle.max, color)
                .filled(true)
                .build();
        }
    });
    Some(CanvasSelection {
        entity: selected.clone(),
        rect,
        resize,
        movable: node.gizmos().move_node() == UiDirectManipulationCapability::Editable,
        units_per_point: [
            f64::from(physical.width() / size[0] / scale),
            f64::from(physical.height() / size[1] / scale),
        ],
        transform: [
            physical.width(),
            physical.height(),
            origin.x(),
            origin.y(),
            scale,
            size[0],
            size[1],
        ],
    })
}

fn direct_edit(kind: DragKind, entity: &SceneEntityId, delta: [f64; 2]) -> UiAuthoringDirectEdit {
    match kind {
        DragKind::Move => UiAuthoringDirectEdit::MoveBy {
            entity: entity.clone(),
            delta,
        },
        DragKind::Resize => UiAuthoringDirectEdit::ResizeBy {
            entity: entity.clone(),
            delta,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        sync::Arc,
        time::{Duration, Instant, SystemTime, UNIX_EPOCH},
    };

    use dear_imgui_rs::{Condition, ConfigFlags, Context};
    use nara::tooling::EditorActionBatch;

    use super::*;

    struct TrustedUiFixture(PathBuf);

    impl TrustedUiFixture {
        fn new(fixture: &Path) -> Self {
            let root = std::env::temp_dir().join(format!(
                "nara-imgui-ui-gesture-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap()
                    .as_nanos(),
            ));
            fs::create_dir(&root).unwrap();
            for directory in ["assets", "scenes", "prefabs", "scripts", "schema"] {
                fs::create_dir(root.join(directory)).unwrap();
            }
            for member in [
                "nara.toml",
                "scenes/startup.scene.json",
                "prefabs/ui.prefab.json",
            ] {
                fs::copy(fixture.join(member), root.join(member)).unwrap();
            }
            let bundle = nara::reflect::AuthoringSchemaBundle::from_trusted_provider_definitions(
                nara::project_host::advanced::built_in_schema_providers(),
            )
            .unwrap();
            fs::write(
                root.join("schema/authoring.bundle.json"),
                bundle.to_json_string().unwrap(),
            )
            .unwrap();
            Self(root)
        }
    }

    impl Drop for TrustedUiFixture {
        fn drop(&mut self) {
            let temporary = std::env::temp_dir().canonicalize().unwrap();
            let root = self.0.canonicalize().unwrap();
            assert!(root.starts_with(temporary));
            assert!(
                root.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("nara-imgui-ui-gesture-"))
            );
            fs::remove_dir_all(root).unwrap();
        }
    }

    fn authoring_frame() -> Arc<EditorUiAuthoringFrame> {
        fixture_frame(false)
    }

    fn fixture_frame(with_gesture: bool) -> Arc<EditorUiAuthoringFrame> {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../tests/fixtures/generic-editor/image-import");
        let owned = with_gesture.then(|| TrustedUiFixture::new(&root));
        let root = owned
            .as_ref()
            .map_or(root.as_path(), |fixture| fixture.0.as_path());
        let options = crate::EditorLaunchOptions::new(root).with_profile("desktop");
        let mut application = if with_gesture {
            crate::EditorApplication::open_trusted_compiled(
                options,
                nara::project_host::TrustedCompiledEditorProjectIntent::new(),
            )
            .expect("the built-in UI fixture opens through the trusted lane")
        } else {
            crate::EditorApplication::new(options)
        };
        application.native_shell_ready();
        for _ in 0..8 {
            if application.current_frame().is_some() {
                break;
            }
            application.advance_project_open();
        }
        let frame = application.current_frame().expect("the UI fixture opens");
        let scene = frame.workspace().active_scene.as_ref().unwrap();
        let anchor = SceneEntityId::new("ui-anchor").unwrap();
        application.submit_and_drive(
            EditorActionBatch::new(
                frame.revision(),
                [
                    EditorAction::Navigate(EditorNavigationAction::SelectEntity {
                        document: scene.document,
                        entity: Some(anchor.clone()),
                    }),
                    EditorAction::Navigate(EditorNavigationAction::OpenPrefabSource {
                        document: scene.document,
                        base_revision: scene.revision,
                        entity: anchor,
                    }),
                ],
            ),
            Duration::ZERO,
        );
        let mut authoring = application
            .current_ui_authoring_frame()
            .expect("the fixture publishes a real UI Prefab projection");
        if with_gesture {
            let field = scalar_field(&authoring);
            let baseline = authoring
                .lower_field_edit(field.key(), field.value().clone())
                .unwrap();
            application.submit_and_drive(
                EditorActionBatch::new(
                    authoring.editor_frame(),
                    [EditorAction::Gesture(EditorGestureAction::Begin {
                        baseline,
                    })],
                ),
                Duration::ZERO,
            );
            authoring = application.current_ui_authoring_frame().unwrap();
            let gesture = authoring
                .gesture()
                .expect("the trusted owner accepts the baseline")
                .stamp()
                .id();
            application.submit_and_drive(
                EditorActionBatch::new(
                    authoring.editor_frame(),
                    [EditorAction::Gesture(EditorGestureAction::Cancel {
                        gesture,
                        reason: EditorGestureCancelReason::TargetRetired,
                    })],
                ),
                Duration::ZERO,
            );
        }
        let mut closed = application.request_exit().exit_ready();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !closed && Instant::now() < deadline {
            closed = application
                .drive_editor_frame(Duration::ZERO, false, true)
                .exit_ready();
            if !closed {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert!(
            closed,
            "the read-only fixture must close without pending work"
        );
        authoring
    }

    fn scalar_field(frame: &EditorUiAuthoringFrame) -> &UiAuthoringFieldDescriptor {
        frame
            .projection()
            .unwrap()
            .fields()
            .iter()
            .find(|field| {
                field.key().entity().as_str() == "image-node"
                    && field.key().component().as_str() == "nara.ui.UiNode"
                    && field.key().primary_field().as_str() == "style.flex_grow"
            })
            .filter(|field| {
                field.commit_policy() == UiAuthoringCommitPolicy::Continuous
                    && matches!(field.kind(), UiAuthoringFieldKind::Scalar(_))
                    && matches!(field.value(), UiAuthoringFieldValue::Scalar(_))
            })
            .expect("the fixture's image-node has a continuous flex-grow scalar")
    }

    fn render_scalar_control(
        context: &mut Context,
        frame: &EditorUiAuthoringFrame,
        field: &UiAuthoringFieldDescriptor,
        value: &mut UiAuthoringFieldValue,
    ) -> (ControlResponse, [f32; 2], bool) {
        let ui = context.frame();
        let mut response = ControlResponse::default();
        let mut center = [0.0; 2];
        let mut active = false;
        ui.window("UI scalar regression")
            .position([20.0, 20.0], Condition::Always)
            .size([400.0, 180.0], Condition::Always)
            .build(|| {
                ui.set_next_item_width(200.0);
                let origin = ui.cursor_screen_pos();
                center = [origin[0] + 100.0, origin[1] + ui.frame_height() * 0.5];
                let control = ui.get_id("##value");
                response = field_control(ui, frame, field, value, &mut None, None);
                active = ui.with_bound_context(|| {
                    // SAFETY: the live Context is bound while its scalar control is inspected.
                    unsafe { dear_imgui_rs::sys::igGetActiveID() == control.raw() }
                });
            });
        let _ = context.render_legacy();
        (response, center, active)
    }

    #[test]
    fn escape_deactivation_cancels_a_continuous_scalar_instead_of_committing() {
        let frame = authoring_frame();
        let field = scalar_field(&frame);
        let mut value = field.value().clone();
        let mut context = Context::create();
        context
            .set_ini_filename::<std::path::PathBuf>(None)
            .unwrap();
        context
            .io_mut()
            .set_config_flags(ConfigFlags::NAV_ENABLE_KEYBOARD);
        context.io_mut().set_display_size([800.0, 600.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        let mut center = [0.0; 2];
        for _ in 0..3 {
            center = render_scalar_control(&mut context, &frame, field, &mut value).1;
        }
        context.io_mut().add_mouse_pos_event(center);
        render_scalar_control(&mut context, &frame, field, &mut value);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        assert!(render_scalar_control(&mut context, &frame, field, &mut value).2);
        context
            .io_mut()
            .add_mouse_pos_event([center[0] + 32.0, center[1]]);
        let (changed, _, active) = render_scalar_control(&mut context, &frame, field, &mut value);
        assert!(
            changed.changed && active,
            "the real drag must edit the active scalar"
        );

        context.io_mut().add_key_event(Key::Escape, true);
        let (escaped, _, active) = render_scalar_control(&mut context, &frame, field, &mut value);
        assert!(
            !active,
            "Dear ImGui deactivates the edited scalar on Escape"
        );
        assert!(
            escaped.escape,
            "Escape must survive the core's active-ID retirement"
        );
    }

    #[test]
    fn cancelling_a_panel_continuous_edit_discards_its_cancelled_field_draft() {
        let frame = authoring_frame();
        let field = scalar_field(&frame);
        let key = field.key().clone();
        let value = UiAuthoringFieldValue::Scalar(123.0);
        let mut panel = ImguiUiAuthoringPanels::default();
        panel.synchronize(Some(&frame));
        panel.fields.insert(
            key.clone(),
            FieldDraft {
                value: value.clone(),
                dirty: true,
                error: None,
            },
        );
        panel.continuous = Some(ContinuousField {
            key: key.clone(),
            value,
            last_sent: None,
            commit: false,
        });
        panel.cancel_interaction(
            &frame,
            EditorGestureCancelReason::TargetRetired,
            &mut Vec::new(),
        );
        panel.synchronize(Some(&frame));
        assert!(panel.continuous.is_none());
        assert!(
            !panel.fields.contains_key(&key),
            "returning to Document must show the source value"
        );
    }

    #[test]
    fn cancelling_an_observer_inspector_does_not_cancel_another_inspectors_gesture() {
        let frame = fixture_frame(true);
        let field = scalar_field(&frame);
        let key = field.key().clone();
        let value = field.value().clone();
        let mut owner = ImguiUiAuthoringPanels::default();
        owner.synchronize(Some(&frame));
        owner.fields.insert(
            key.clone(),
            FieldDraft {
                value: value.clone(),
                dirty: true,
                error: None,
            },
        );
        owner.continuous = Some(ContinuousField {
            key: key.clone(),
            value: value.clone(),
            last_sent: None,
            commit: false,
        });
        let mut observer = ImguiUiAuthoringPanels::default();
        observer.synchronize(Some(&frame));
        observer.fields.insert(
            key.clone(),
            FieldDraft {
                value,
                dirty: false,
                error: None,
            },
        );

        let mut actions = Vec::new();
        observer.cancel_interaction(
            &frame,
            EditorGestureCancelReason::TargetRetired,
            &mut actions,
        );
        assert!(
            actions.is_empty(),
            "matching the visible field does not confer gesture ownership"
        );
        assert!(observer.fields.contains_key(&key));
        owner.cancel_interaction(&frame, EditorGestureCancelReason::Escape, &mut actions);
        assert!(
            matches!(actions.as_slice(), [EditorAction::Gesture(EditorGestureAction::Cancel {
            gesture, reason: EditorGestureCancelReason::Escape,
        })] if *gesture == frame.gesture().unwrap().stamp().id())
        );
        assert!(!owner.fields.contains_key(&key));
        assert!(observer.fields.contains_key(&key));
    }

    #[test]
    fn queued_canvas_commit_keeps_the_pointer_position_at_release() {
        let mut drag = CanvasDrag {
            entity: SceneEntityId::new("selected-node").unwrap(),
            kind: DragKind::Move,
            transform: [0.0; 7],
            pointer_origin: [10.0, 20.0],
            latest: [0.0, 0.0],
            applied: [20.0, 60.0],
            sent: None,
            commit: false,
        };
        let units_per_point = [2.0, 3.0];
        drag.observe_pointer([20.0, 40.0], units_per_point, false);
        assert_eq!(drag.latest, [20.0, 60.0]);

        drag.observe_pointer([25.0, 50.0], units_per_point, true);
        assert!(drag.commit);
        assert_eq!(drag.latest, [30.0, 90.0]);

        // The new preview is still waiting for presentation while the released pointer moves.
        drag.observe_pointer([100.0, 200.0], units_per_point, false);
        assert_eq!(drag.latest, [30.0, 90.0]);
        assert_eq!(
            [
                drag.latest[0] - drag.applied[0],
                drag.latest[1] - drag.applied[1]
            ],
            [10.0, 30.0],
            "the eventual commit must complete the release delta"
        );
    }

    #[test]
    fn a_canvas_pointer_release_requires_confirmation_of_its_current_mapping() {
        fn draw(
            context: &mut Context,
            panel: &mut ImguiUiAuthoringPanels,
            frame: &EditorUiAuthoringFrame,
            position: [f32; 2],
        ) -> ImguiUiCanvasOutput {
            let ui = context.frame();
            let mut output = None;
            ui.window("Canvas mapping regression")
                .position(position, Condition::Always)
                .size([400.0, 300.0], Condition::Always)
                .build(|| {
                    output = Some(panel.show_canvas(
                        ui,
                        frame,
                        ImguiUiCanvasInput {
                            image: Some(ImguiViewImage {
                                texture: dear_imgui_rs::TextureId::new(1),
                                extent: [320, 240],
                            }),
                            presented: true,
                            notice: None,
                        },
                        true,
                        &mut Vec::new(),
                    ));
                });
            let _ = context.render_legacy();
            output.unwrap()
        }

        let frame = authoring_frame();
        let mut panel = ImguiUiAuthoringPanels::default();
        let mut context = Context::create();
        context.set_ini_filename::<PathBuf>(None).unwrap();
        context.io_mut().set_display_size([800.0, 600.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        for _ in 0..3 {
            draw(&mut context, &mut panel, &frame, [20.0, 20.0]);
        }
        let initial = draw(&mut context, &mut panel, &frame, [20.0, 20.0]);
        panel.confirm_display(initial.display);
        let rect = initial.display.unwrap().rect;
        let pointer = [
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ];
        context.io_mut().add_mouse_pos_event(pointer);
        draw(&mut context, &mut panel, &frame, [20.0, 20.0]);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        draw(&mut context, &mut panel, &frame, [20.0, 20.0]);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        let remapped = draw(&mut context, &mut panel, &frame, [40.0, 20.0]);
        assert_ne!(remapped.display, initial.display);
        assert!(
            remapped.pick_uv.is_none(),
            "an old confirmation cannot authorize a remapped release"
        );

        panel.confirm_display(None);
        draw(&mut context, &mut panel, &frame, [40.0, 20.0]);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        draw(&mut context, &mut panel, &frame, [40.0, 20.0]);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        let unconfirmed = draw(&mut context, &mut panel, &frame, [40.0, 20.0]);
        assert!(unconfirmed.pick_uv.is_none());

        panel.confirm_display(unconfirmed.display);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        draw(&mut context, &mut panel, &frame, [40.0, 20.0]);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        let confirmed = draw(&mut context, &mut panel, &frame, [40.0, 20.0]);
        assert!(
            confirmed.pick_uv.is_some(),
            "the same real control must work after its mapping is confirmed"
        );
    }

    #[test]
    fn a_deferred_canvas_frame_preserves_an_already_requested_gesture_cancel() {
        let frame = fixture_frame(true);
        let gesture = frame.gesture().unwrap().stamp().id();
        let field = scalar_field(&frame);
        let edit = frame
            .lower_field_edit(field.key(), field.value().clone())
            .unwrap();
        let instance = crate::workspace::EditorPanelInstanceId(1);
        let mut frontend = super::super::ImguiEditorFrontend::new();
        frontend.presentation.widgets.entry(instance).or_default();
        let mut output = super::super::ImguiFrontendOutput {
            actions: Some(EditorActionBatch::new(frame.editor_frame(), [])),
            canvas_panel: Some((
                instance,
                crate::workspace::EditorPanelTarget::ActiveDocument,
            )),
            canvas_actions: vec![
                EditorAction::Gesture(EditorGestureAction::Preview { gesture, edit }),
                EditorAction::Gesture(EditorGestureAction::Cancel {
                    gesture,
                    reason: EditorGestureCancelReason::TargetRetired,
                }),
            ],
            canvas_pick_uv: Some([0.5, 0.5]),
            ..Default::default()
        };
        frontend.finish_canvas_presentation(&mut output, Some(&frame), false);
        assert!(output.canvas_pick_uv.is_none());
        assert!(
            matches!(output.actions.unwrap().into_actions().as_slice(), [
            EditorAction::Gesture(EditorGestureAction::Cancel {
                gesture: cancelled,
                reason: EditorGestureCancelReason::TargetRetired,
            })
        ] if *cancelled == gesture)
        );
    }
}
