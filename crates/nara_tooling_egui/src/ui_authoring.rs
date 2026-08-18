use std::collections::HashMap;

use egui::{
    Align2, Button, Color32, ComboBox, DragValue, FontId, PointerButton, Pos2, Rect, RichText,
    Sense, Stroke, TextEdit, TextureId, Ui, Vec2,
};
use nara_scene::{SceneAuthoringRevision, SceneEntityId, ScenePlacement, ScenePlacementAnchor};
use nara_tooling::{
    EditorAction, EditorActionBatchReport, EditorActionOutcome, EditorActionRejection,
    EditorAtomicFieldEdit, EditorDocumentId, EditorDocumentMutation, EditorGestureAction,
    EditorGestureCancelReason, EditorGestureView, EditorNavigationAction, EditorSelectionSet,
    PrefabCompositionOutcome,
};
use nara_ui::__private::{
    UiAuthoringCanvasProjection, UiAuthoringCommitPolicy, UiAuthoringDirectEdit,
    UiAuthoringEditability, UiAuthoringFieldDescriptor, UiAuthoringFieldGroup, UiAuthoringFieldKey,
    UiAuthoringFieldKind, UiAuthoringFieldValue, UiAuthoringProjection, UiAuthoringProjectionError,
    UiAuthoringReadOnlyReason, UiAuthoringStructuralCapability,
    UiAuthoringStructuralReadOnlyReason, UiBrushAuthoringKind, UiCatalogNodeKind,
    UiCreateParameters, UiDirectManipulationCapability, UiImageChoiceGeneration,
    UiImageChoiceSelection, UiImageChoiceSet, UiPlacementCandidate, UiStructureIntent,
    UiStructureRejection,
};
use nara_ui::UiActionId;

use crate::scene_view::fitted_image_rect;

/// Borrowed first-party Adapter input for one exact UI authoring frame.
///
/// Document identity and lowering remain Host-owned. This value is valid only for one egui redraw.
pub struct EguiUiAuthoringInput<'a> {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    selection: &'a EditorSelectionSet,
    gesture: Option<&'a EditorGestureView>,
    projection: Result<&'a UiAuthoringProjection, UiAuthoringProjectionError>,
    image_choice_generation: UiImageChoiceGeneration,
    image_choices: Result<&'a UiImageChoiceSet, UiStructureRejection>,
    lower_field_edit: &'a mut dyn for<'key> FnMut(
        &'key UiAuthoringFieldKey,
        UiAuthoringFieldValue,
    ) -> Result<EditorAtomicFieldEdit, String>,
    lower_direct_edit: &'a mut dyn for<'edit> FnMut(
        &'edit UiAuthoringDirectEdit,
    ) -> Result<EditorAtomicFieldEdit, String>,
    lower_structure_intent:
        &'a mut dyn for<'intent> FnMut(
            &'intent UiStructureIntent,
        ) -> Result<EditorDocumentMutation, String>,
}

impl<'a> EguiUiAuthoringInput<'a> {
    /// Creates one redraw-local Adapter input from the concrete Editor Host.
    #[doc(hidden)]
    #[must_use]
    pub fn new(
        document: EditorDocumentId,
        revision: SceneAuthoringRevision,
        selection: &'a EditorSelectionSet,
        gesture: Option<&'a EditorGestureView>,
        projection: Result<&'a UiAuthoringProjection, UiAuthoringProjectionError>,
        image_choice_generation: UiImageChoiceGeneration,
        image_choices: Result<&'a UiImageChoiceSet, UiStructureRejection>,
        lower_field_edit: &'a mut dyn for<'key> FnMut(
            &'key UiAuthoringFieldKey,
            UiAuthoringFieldValue,
        )
            -> Result<EditorAtomicFieldEdit, String>,
        lower_direct_edit: &'a mut dyn for<'edit> FnMut(
            &'edit UiAuthoringDirectEdit,
        )
            -> Result<EditorAtomicFieldEdit, String>,
        lower_structure_intent: &'a mut dyn for<'intent> FnMut(
            &'intent UiStructureIntent,
        ) -> Result<
            EditorDocumentMutation,
            String,
        >,
    ) -> Self {
        Self {
            document,
            revision,
            selection,
            gesture,
            projection,
            image_choice_generation,
            image_choices,
            lower_field_edit,
            lower_direct_edit,
            lower_structure_intent,
        }
    }

    pub(crate) const fn document(&self) -> EditorDocumentId {
        self.document
    }

    pub(crate) const fn revision(&self) -> SceneAuthoringRevision {
        self.revision
    }

    fn projection(&self) -> Result<&UiAuthoringProjection, UiAuthoringProjectionError> {
        self.projection
    }

    fn image_choice_generation(&self) -> UiImageChoiceGeneration {
        self.image_choice_generation
    }

    fn image_choices(&self) -> Result<&UiImageChoiceSet, UiStructureRejection> {
        self.image_choices
    }

    fn selected_entity(&self) -> Option<&SceneEntityId> {
        self.selection.top_entity()
    }

    fn gesture(&self) -> Option<&EditorGestureView> {
        self.gesture
    }

    fn lower_field_edit(
        &mut self,
        key: &UiAuthoringFieldKey,
        value: UiAuthoringFieldValue,
    ) -> Result<EditorAtomicFieldEdit, String> {
        (self.lower_field_edit)(key, value)
    }

    fn lower_direct_edit(
        &mut self,
        edit: &UiAuthoringDirectEdit,
    ) -> Result<EditorAtomicFieldEdit, String> {
        (self.lower_direct_edit)(edit)
    }

    fn lower_structure_intent(
        &mut self,
        intent: &UiStructureIntent,
    ) -> Result<EditorDocumentMutation, String> {
        (self.lower_structure_intent)(intent)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EguiUiCanvasContent {
    Presented {
        texture_id: TextureId,
        extent: [u32; 2],
    },
    Previewing {
        texture_id: TextureId,
        extent: [u32; 2],
    },
    Resizing,
    Unavailable,
}

/// Toolkit-local texture facts for one UI Canvas redraw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EguiUiCanvasInput {
    content: EguiUiCanvasContent,
    notice: Option<&'static str>,
}

impl EguiUiCanvasInput {
    #[must_use]
    pub const fn presented(texture_id: TextureId, extent: [u32; 2]) -> Self {
        Self {
            content: EguiUiCanvasContent::Presented { texture_id, extent },
            notice: None,
        }
    }

    #[must_use]
    pub const fn previewing(texture_id: TextureId, extent: [u32; 2]) -> Self {
        Self {
            content: EguiUiCanvasContent::Previewing { texture_id, extent },
            notice: None,
        }
    }

    #[must_use]
    pub const fn resizing() -> Self {
        Self {
            content: EguiUiCanvasContent::Resizing,
            notice: None,
        }
    }

    #[must_use]
    pub const fn unavailable() -> Self {
        Self {
            content: EguiUiCanvasContent::Unavailable,
            notice: None,
        }
    }

    #[must_use]
    pub const fn with_notice(mut self, notice: &'static str) -> Self {
        self.notice = Some(notice);
        self
    }

    const fn is_presented(self) -> bool {
        matches!(self.content, EguiUiCanvasContent::Presented { .. })
    }

    const fn keeps_gesture_alive(self) -> bool {
        self.notice.is_none()
            && matches!(
                self.content,
                EguiUiCanvasContent::Presented { .. } | EguiUiCanvasContent::Previewing { .. }
            )
    }
}

impl Default for EguiUiCanvasInput {
    fn default() -> Self {
        Self::unavailable()
    }
}

/// Transient interaction emitted by one fixed-fit UI Canvas redraw.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EguiUiCanvasOutput {
    desired_size_points: Vec2,
    pick_uv: Option<[f32; 2]>,
}

impl EguiUiCanvasOutput {
    #[must_use]
    pub const fn desired_size_points(self) -> Vec2 {
        self.desired_size_points
    }

    #[must_use]
    pub const fn pick_uv(self) -> Option<[f32; 2]> {
        self.pick_uv
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct EguiUiFieldDraftKey {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    field: UiAuthoringFieldKey,
}

impl EguiUiFieldDraftKey {
    fn new(input: &EguiUiAuthoringInput<'_>, field: &UiAuthoringFieldKey) -> Self {
        Self {
            document: input.document(),
            revision: input.revision(),
            field: field.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct EguiUiTextDraft {
    text: String,
    dirty: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct EguiUiContinuousDraft {
    key: EguiUiFieldDraftKey,
    value: UiAuthoringFieldValue,
    last_sent: Option<UiAuthoringFieldValue>,
    commit_when_active: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EguiUiAuthoringScope {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    image_choice_generation: UiImageChoiceGeneration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EguiUiStructureDialog {
    Create(EguiUiCreateDraft),
    Move(EguiUiMoveDraft),
    Delete(EguiUiDeleteDraft),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EguiUiCreateDraft {
    kind: UiCatalogNodeKind,
    placement: ScenePlacement,
    text: String,
    image: Option<UiImageChoiceSelection>,
    button_label: String,
    button_action: String,
    focus: EguiUiCreateFocus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EguiUiCreateField {
    Placement,
    Text,
    Image,
    ButtonLabel,
    ButtonAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EguiUiCreateFocus {
    Initial,
    Settled,
    Invalid(EguiUiCreateField),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EguiUiCreateValidation {
    field: EguiUiCreateField,
    message: String,
}

impl EguiUiCreateDraft {
    fn new(placement: ScenePlacement) -> Self {
        Self {
            kind: UiCatalogNodeKind::Container,
            placement,
            text: "Text".to_owned(),
            image: None,
            button_label: "Button".to_owned(),
            button_action: String::new(),
            focus: EguiUiCreateFocus::Initial,
        }
    }

    fn parameters(&self) -> Result<UiCreateParameters, EguiUiCreateValidation> {
        match self.kind {
            UiCatalogNodeKind::Container => Ok(UiCreateParameters::Container),
            UiCatalogNodeKind::Text => Ok(UiCreateParameters::Text {
                content: self.text.clone(),
            }),
            UiCatalogNodeKind::Image => Ok(UiCreateParameters::Image {
                selection: self.image.clone(),
            }),
            UiCatalogNodeKind::Button => {
                let action =
                    UiActionId::new(self.button_action.trim().to_owned()).map_err(|error| {
                        EguiUiCreateValidation {
                            field: EguiUiCreateField::ButtonAction,
                            message: error.to_string(),
                        }
                    })?;
                Ok(UiCreateParameters::Button {
                    label: self.button_label.clone(),
                    action: Some(action),
                })
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EguiUiMoveDraft {
    target: SceneEntityId,
    placement: ScenePlacement,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EguiUiDeleteDraft {
    target: SceneEntityId,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum EguiUiStructureFocus {
    Hierarchy,
    Entity(SceneEntityId),
}

#[derive(Debug, Default)]
pub(crate) struct EguiUiAuthoringPanel {
    scope: Option<EguiUiAuthoringScope>,
    text_drafts: HashMap<EguiUiFieldDraftKey, EguiUiTextDraft>,
    field_errors: HashMap<EguiUiFieldDraftKey, String>,
    continuous: Option<EguiUiContinuousDraft>,
    structure_dialog: Option<EguiUiStructureDialog>,
    structure_error: Option<String>,
    structure_pending_action: Option<usize>,
    structure_focus: Option<EguiUiStructureFocus>,
}

impl EguiUiAuthoringPanel {
    pub(crate) fn synchronize(&mut self, input: Option<&EguiUiAuthoringInput<'_>>) {
        let next = input.map(|input| EguiUiAuthoringScope {
            document: input.document(),
            revision: input.revision(),
            image_choice_generation: input.image_choice_generation(),
        });
        if self.scope != next {
            if self.structure_dialog.is_some() || self.structure_pending_action.is_some() {
                self.structure_focus = Some(EguiUiStructureFocus::Hierarchy);
            }
            self.scope = next;
            self.text_drafts.clear();
            self.field_errors.clear();
            self.continuous = None;
            self.structure_dialog = None;
            self.structure_error = None;
            self.structure_pending_action = None;
        }
    }

    pub(crate) fn observe_action_report(&mut self, report: &EditorActionBatchReport) {
        let Some(action_index) = self.structure_pending_action else {
            return;
        };
        match report.outcomes().get(action_index) {
            Some(EditorActionOutcome::Workspace(report)) => {
                if let Some(outcome) = &report.prefab_composition_outcome {
                    self.accept_structure_outcome(outcome);
                    return;
                }
            }
            Some(EditorActionOutcome::Rejected(rejection)) => {
                self.reject_structure_action(*rejection);
                return;
            }
            _ => {}
        }
        self.structure_pending_action = None;
        self.structure_error = Some("The structure change was not applied".to_owned());
    }

    pub(crate) fn observe_action_rejection(&mut self, rejection: EditorActionRejection) {
        if self.structure_pending_action.is_some() {
            self.reject_structure_action(rejection);
        }
    }

    fn accept_structure_outcome(&mut self, outcome: &PrefabCompositionOutcome) {
        self.structure_focus = Some(match outcome {
            PrefabCompositionOutcome::Created { entity }
            | PrefabCompositionOutcome::Moved { entity }
            | PrefabCompositionOutcome::NoOp { entity } => {
                EguiUiStructureFocus::Entity(entity.clone())
            }
            PrefabCompositionOutcome::Removed {
                selection: Some(entity),
                ..
            } => EguiUiStructureFocus::Entity(entity.clone()),
            PrefabCompositionOutcome::Removed {
                selection: None, ..
            } => EguiUiStructureFocus::Hierarchy,
        });
        self.structure_dialog = None;
        self.structure_error = None;
        self.structure_pending_action = None;
    }

    fn reject_structure_action(&mut self, rejection: EditorActionRejection) {
        self.structure_pending_action = None;
        self.structure_error = Some(structure_action_rejection_label(rejection).to_owned());
    }

    pub(crate) fn show_hierarchy(
        &mut self,
        ui: &mut Ui,
        input: &mut EguiUiAuthoringInput<'_>,
        editing_enabled: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        ui.heading("UI Hierarchy");
        let requested_intent = {
            let Ok(projection) = input.projection() else {
                if self.structure_dialog.take().is_some() {
                    self.structure_focus = Some(EguiUiStructureFocus::Hierarchy);
                }
                self.structure_pending_action = None;
                ui.label(RichText::new("UI source is unavailable").weak());
                return;
            };
            if let Some(EguiUiStructureFocus::Entity(entity)) = &self.structure_focus
                && !projection
                    .hierarchy()
                    .iter()
                    .any(|node| node.entity() == entity)
            {
                self.structure_focus = Some(EguiUiStructureFocus::Hierarchy);
            }
            let selected = input.selected_entity().and_then(|selected| {
                projection
                    .hierarchy()
                    .iter()
                    .find(|node| node.entity() == selected)
            });
            let structure_editable = selected.is_some_and(|node| {
                node.structural_capability() == UiAuthoringStructuralCapability::Editable
            });

            ui.horizontal_wrapped(|ui| {
                let add = ui
                    .add_enabled(
                        editing_enabled && self.structure_pending_action.is_none(),
                        Button::new("+ Add"),
                    )
                    .on_hover_text("Add one closed-catalog UI node");
                record_structure_control("ui-structure-add", &add);
                if matches!(self.structure_focus, Some(EguiUiStructureFocus::Hierarchy))
                    && add.enabled()
                {
                    add.request_focus();
                    self.structure_focus = None;
                }
                if add.clicked() {
                    match projection.structure_placement_candidates(None) {
                        Ok(candidates) if !candidates.is_empty() => {
                            self.structure_dialog = Some(EguiUiStructureDialog::Create(
                                EguiUiCreateDraft::new(candidates[0].placement().clone()),
                            ));
                            self.structure_error = None;
                        }
                        Ok(_) => {
                            self.structure_error =
                                Some("No exact source-local placement is available".to_owned());
                        }
                        Err(error) => self.structure_error = Some(error.to_string()),
                    }
                }

                let structural_reason =
                    selected.and_then(|node| match node.structural_capability() {
                        UiAuthoringStructuralCapability::Editable => None,
                        UiAuthoringStructuralCapability::ReadOnly(reason) => {
                            Some(structural_read_only_label(reason))
                        }
                    });
                let move_node = ui
                    .add_enabled(
                        editing_enabled
                            && structure_editable
                            && self.structure_pending_action.is_none(),
                        Button::new("\u{2195} Move"),
                    )
                    .on_hover_text("Move or reparent the selected UI subtree")
                    .on_disabled_hover_text(
                        structural_reason.unwrap_or("Select an editable UI node"),
                    );
                record_structure_control("ui-structure-move", &move_node);
                if move_node.clicked()
                    && let Some(selected) = selected
                {
                    match projection.structure_placement_candidates(Some(selected.entity())) {
                        Ok(candidates) if !candidates.is_empty() => {
                            let placement = candidates
                                .iter()
                                .find(|candidate| candidate.is_current())
                                .unwrap_or(&candidates[0])
                                .placement()
                                .clone();
                            self.structure_dialog =
                                Some(EguiUiStructureDialog::Move(EguiUiMoveDraft {
                                    target: selected.entity().clone(),
                                    placement,
                                }));
                            self.structure_error = None;
                        }
                        Ok(_) => {
                            self.structure_error =
                                Some("No exact move target is available".to_owned());
                        }
                        Err(error) => self.structure_error = Some(error.to_string()),
                    }
                }

                let delete = ui
                    .add_enabled(
                        editing_enabled
                            && structure_editable
                            && self.structure_pending_action.is_none(),
                        Button::new("\u{2715} Delete"),
                    )
                    .on_hover_text("Delete the selected source-local UI subtree")
                    .on_disabled_hover_text(
                        structural_reason.unwrap_or("Select an editable UI node"),
                    );
                record_structure_control("ui-structure-delete", &delete);
                if delete.clicked()
                    && let Some(selected) = selected
                {
                    self.structure_dialog =
                        Some(EguiUiStructureDialog::Delete(EguiUiDeleteDraft {
                            target: selected.entity().clone(),
                        }));
                    self.structure_error = None;
                }
            });

            let requested =
                self.show_structure_dialog(ui, projection, input.image_choices(), editing_enabled);

            ui.separator();
            for node in projection.hierarchy() {
                ui.horizontal(|ui| {
                    ui.add_space(f32::from(node.depth()) * 16.0);
                    let selected = input
                        .selection
                        .entities()
                        .any(|entity| entity == node.entity());
                    let response = ui
                        .selectable_label(selected, node.display_label())
                        .on_hover_ui(|ui| {
                            ui.label(format!("{} - {}", node.kind().label(), node.display_path()));
                        });
                    #[cfg(test)]
                    record_structure_control(
                        &format!("ui-node:{}", node.entity().as_str()),
                        &response,
                    );
                    if matches!(
                        self.structure_focus.as_ref(),
                        Some(EguiUiStructureFocus::Entity(entity)) if entity == node.entity()
                    ) {
                        response.request_focus();
                        self.structure_focus = None;
                    }
                    if response.clicked() {
                        actions.push(EditorAction::Navigate(
                            EditorNavigationAction::SelectEntity {
                                document: input.document(),
                                entity: Some(node.entity().clone()),
                            },
                        ));
                    }
                });
            }
            requested
        };

        if let Some(intent) = requested_intent {
            match input.lower_structure_intent(&intent) {
                Ok(mutation) => {
                    self.structure_error = None;
                    self.structure_pending_action = Some(actions.len());
                    actions.push(EditorAction::Mutate(mutation));
                }
                Err(error) => {
                    self.structure_error = Some(error);
                    self.structure_pending_action = None;
                }
            }
        }
    }

    fn show_structure_dialog(
        &mut self,
        ui: &mut Ui,
        projection: &UiAuthoringProjection,
        image_choices: Result<&UiImageChoiceSet, UiStructureRejection>,
        editing_enabled: bool,
    ) -> Option<UiStructureIntent> {
        let Some(mut dialog) = self.structure_dialog.take() else {
            if let Some(error) = &self.structure_error {
                ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
            }
            return None;
        };
        if ui.input_mut(|input| input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
            self.structure_error = None;
            self.structure_pending_action = None;
            self.structure_focus = Some(EguiUiStructureFocus::Hierarchy);
            return None;
        }

        ui.separator();
        let EguiUiStructureDialogResponse {
            cancelled,
            changed,
            intent,
        } = match &mut dialog {
            EguiUiStructureDialog::Create(draft) => show_create_dialog(
                ui,
                draft,
                projection,
                image_choices,
                editing_enabled && self.structure_pending_action.is_none(),
            ),
            EguiUiStructureDialog::Move(draft) => show_move_dialog(
                ui,
                draft,
                projection,
                editing_enabled && self.structure_pending_action.is_none(),
            ),
            EguiUiStructureDialog::Delete(draft) => show_delete_dialog(
                ui,
                draft,
                projection,
                editing_enabled && self.structure_pending_action.is_none(),
            ),
        };
        if changed {
            self.structure_error = None;
        }
        if let Some(error) = &self.structure_error {
            ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
        }
        if self.structure_pending_action.is_some() {
            ui.label(RichText::new("Applying structure change").weak());
        }
        if cancelled {
            self.structure_error = None;
            self.structure_focus = Some(EguiUiStructureFocus::Hierarchy);
            return None;
        } else {
            self.structure_dialog = Some(dialog);
        }
        intent
    }

    pub(crate) fn show_inspector(
        &mut self,
        ui: &mut Ui,
        input: &mut EguiUiAuthoringInput<'_>,
        editing_enabled: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        ui.heading("Inspector");
        let Ok(projection) = input.projection() else {
            ui.label(RichText::new("The UI authoring projection was rejected").weak());
            return;
        };
        let Some(selected) = input.selected_entity().cloned() else {
            ui.label(RichText::new("Select a UI node").weak());
            return;
        };
        ui.label(RichText::new(selected.as_str()).strong());

        let fields = projection
            .fields()
            .iter()
            .filter(|field| field.key().entity() == &selected)
            .cloned()
            .collect::<Vec<_>>();
        if fields.is_empty() {
            ui.label(RichText::new("No authorable UI fields").weak());
            return;
        }

        let mut current_group = None;
        for field in fields {
            if current_group != Some(field.group()) {
                current_group = Some(field.group());
                ui.separator();
                ui.label(RichText::new(group_label(field.group())).strong());
            }
            self.show_field(ui, input, &field, editing_enabled, actions);
        }
    }

    fn show_field(
        &mut self,
        ui: &mut Ui,
        input: &mut EguiUiAuthoringInput<'_>,
        field: &UiAuthoringFieldDescriptor,
        editing_enabled: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        let editable =
            editing_enabled && matches!(field.editability(), UiAuthoringEditability::Editable);
        let draft_key = EguiUiFieldDraftKey::new(input, field.key());
        let displayed = self
            .continuous
            .as_ref()
            .filter(|draft| draft.key == draft_key)
            .map_or_else(|| field.value().clone(), |draft| draft.value.clone());
        let mut proposed = displayed.clone();
        let response = ui
            .horizontal(|ui| {
                let label = ui.label(field.label());
                show_typed_control(
                    ui,
                    field,
                    &draft_key,
                    &mut self.text_drafts,
                    &mut proposed,
                    editable,
                    label.id,
                )
            })
            .inner;

        if let UiAuthoringEditability::ReadOnly(reason) = field.editability() {
            ui.label(RichText::new(read_only_label(reason)).weak());
        }
        if let Some(error) = self.field_errors.get(&draft_key) {
            ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
        }

        if field.commit_policy() == UiAuthoringCommitPolicy::Continuous {
            self.handle_continuous_field(input, field, draft_key, proposed, response, actions);
        } else if response.commit_requested {
            match input.lower_field_edit(field.key(), proposed) {
                Ok(edit) => {
                    self.field_errors.remove(&draft_key);
                    if let Some(draft) = self.text_drafts.get_mut(&draft_key) {
                        draft.dirty = false;
                    }
                    actions.push(EditorAction::Mutate(
                        EditorDocumentMutation::ApplyPrefabFieldEdit { edit },
                    ));
                }
                Err(error) => {
                    self.field_errors.insert(draft_key, error);
                }
            }
        }
    }

    fn handle_continuous_field(
        &mut self,
        input: &mut EguiUiAuthoringInput<'_>,
        field: &UiAuthoringFieldDescriptor,
        key: EguiUiFieldDraftKey,
        proposed: UiAuthoringFieldValue,
        response: EguiUiControlResponse,
        actions: &mut Vec<EditorAction>,
    ) {
        let gesture = input.gesture();
        let gesture_matches = gesture.is_some_and(|gesture| {
            let target = gesture.baseline().target();
            target.document() == key.document
                && target.base_revision() == key.revision
                && target.entity() == key.field.entity()
                && target.component() == key.field.component()
                && target.field() == key.field.primary_field()
        });

        if response.escape_pressed && gesture_matches {
            let gesture = gesture.expect("matching gesture must exist");
            actions.push(EditorAction::Gesture(EditorGestureAction::Cancel {
                gesture: gesture.stamp().id(),
                reason: EditorGestureCancelReason::Escape,
            }));
            self.continuous = None;
            return;
        }

        if !gesture_matches {
            if response.changed {
                match input.lower_field_edit(field.key(), field.value().clone()) {
                    Ok(baseline) => {
                        actions.push(EditorAction::Gesture(EditorGestureAction::Begin {
                            baseline,
                        }));
                        self.continuous = Some(EguiUiContinuousDraft {
                            key: key.clone(),
                            value: proposed,
                            last_sent: None,
                            commit_when_active: response.commit_requested,
                        });
                        self.field_errors.remove(&key);
                    }
                    Err(error) => {
                        self.field_errors.insert(key, error);
                    }
                }
            }
            return;
        }

        let Some(draft) = self.continuous.as_mut().filter(|draft| draft.key == key) else {
            return;
        };
        if response.changed {
            draft.value = proposed;
        }
        draft.commit_when_active |= response.commit_requested;
        let value = draft.value.clone();
        let gesture_id = gesture.expect("matching gesture must exist").stamp().id();
        match input.lower_field_edit(field.key(), value.clone()) {
            Ok(edit) if draft.commit_when_active => {
                self.field_errors.remove(&key);
                actions.push(EditorAction::Gesture(EditorGestureAction::Commit {
                    gesture: gesture_id,
                    edit,
                }));
                self.continuous = None;
            }
            Ok(edit) if draft.last_sent.as_ref() != Some(&value) => {
                actions.push(EditorAction::Gesture(EditorGestureAction::Preview {
                    gesture: gesture_id,
                    edit,
                }));
                draft.last_sent = Some(value);
                self.field_errors.remove(&key);
            }
            Ok(_) => {}
            Err(error) => {
                self.field_errors.insert(key, error);
            }
        }
    }
}

struct EguiUiStructureDialogResponse {
    cancelled: bool,
    changed: bool,
    intent: Option<UiStructureIntent>,
}

fn show_create_dialog(
    ui: &mut Ui,
    draft: &mut EguiUiCreateDraft,
    projection: &UiAuthoringProjection,
    image_choices: Result<&UiImageChoiceSet, UiStructureRejection>,
    enabled: bool,
) -> EguiUiStructureDialogResponse {
    ui.label(RichText::new("Add UI node").strong());
    ui.label(RichText::new("Kind").weak());
    let previous_kind = draft.kind;
    ui.horizontal_wrapped(|ui| {
        for kind in UiCatalogNodeKind::ALL {
            let response = ui.add_enabled(
                enabled,
                Button::new(kind.label()).selected(draft.kind == kind),
            );
            #[cfg(test)]
            record_structure_control(&format!("ui-structure-kind:{}", kind.label()), &response);
            if response.clicked() {
                draft.kind = kind;
            }
        }
    });
    let mut changed = draft.kind != previous_kind;
    if changed {
        draft.focus = EguiUiCreateFocus::Initial;
    }

    let candidates = projection.structure_placement_candidates(None);
    let (placement_valid, placement_response) = match &candidates {
        Ok(candidates) => {
            let (valid, placement_changed, response) = show_placement_control(
                ui,
                "ui-structure-create-placement",
                &mut draft.placement,
                candidates,
                enabled,
            );
            changed |= placement_changed;
            (valid, Some(response))
        }
        Err(error) => {
            ui.label(RichText::new(error.to_string()).color(ui.visuals().error_fg_color));
            (false, None)
        }
    };

    let intent = draft
        .parameters()
        .map(|parameters| UiStructureIntent::Create {
            parameters,
            placement: draft.placement.clone(),
        });
    let validation = match &intent {
        Err(validation) => Some(validation.clone()),
        Ok(intent) => projection
            .prepare_structure(intent, image_choices.ok())
            .err()
            .map(|error| create_validation_for_rejection(draft.kind, error)),
    };
    if validation.is_none() && matches!(draft.focus, EguiUiCreateFocus::Invalid(_)) {
        draft.focus = EguiUiCreateFocus::Settled;
    }
    let should_focus_invalid = validation
        .as_ref()
        .is_some_and(|validation| match draft.focus {
            EguiUiCreateFocus::Initial => false,
            EguiUiCreateFocus::Settled => true,
            EguiUiCreateFocus::Invalid(field) => field != validation.field,
        });

    let mut initial_focus_applied = false;
    let mut invalid_focus_applied = false;
    if let Some(response) = &placement_response {
        invalid_focus_applied |= show_create_validation_for_field(
            ui,
            validation.as_ref(),
            EguiUiCreateField::Placement,
            response,
            should_focus_invalid,
        );
    }
    match draft.kind {
        UiCatalogNodeKind::Container => {
            if matches!(draft.focus, EguiUiCreateFocus::Initial)
                && let Some(response) = &placement_response
            {
                response.request_focus();
                initial_focus_applied = true;
            }
        }
        UiCatalogNodeKind::Text => {
            let label = ui.label(RichText::new("Text").weak());
            let response = ui
                .add_enabled(enabled, TextEdit::singleline(&mut draft.text))
                .labelled_by(label.id);
            record_structure_control("ui-structure-create-text", &response);
            if matches!(draft.focus, EguiUiCreateFocus::Initial) {
                response.request_focus();
                initial_focus_applied = true;
            }
            changed |= response.changed();
            invalid_focus_applied |= show_create_validation_for_field(
                ui,
                validation.as_ref(),
                EguiUiCreateField::Text,
                &response,
                should_focus_invalid,
            );
        }
        UiCatalogNodeKind::Image => match image_choices {
            Ok(choices) if !choices.choices().is_empty() => {
                let label = ui.label(RichText::new("Image").weak());
                let selected = draft
                    .image
                    .as_ref()
                    .map_or("Select image", UiImageChoiceSelection::project_path);
                let response = ComboBox::from_id_salt("ui-structure-create-image")
                    .selected_text(selected)
                    .show_ui(ui, |ui| {
                        for choice in choices.choices() {
                            let selected = draft.image.as_ref().is_some_and(|selection| {
                                selection.generation() == choices.generation()
                                    && selection.source_identity() == choice.source_identity()
                                    && selection.project_path() == choice.project_path()
                            });
                            if ui
                                .selectable_label(selected, choice.project_path())
                                .clicked()
                            {
                                draft.image = Some(choice.selection(choices.generation()));
                                changed = true;
                            }
                        }
                    })
                    .response
                    .labelled_by(label.id);
                record_structure_control("ui-structure-create-image", &response);
                if matches!(draft.focus, EguiUiCreateFocus::Initial) {
                    response.request_focus();
                    initial_focus_applied = true;
                }
                invalid_focus_applied |= show_create_validation_for_field(
                    ui,
                    validation.as_ref(),
                    EguiUiCreateField::Image,
                    &response,
                    should_focus_invalid,
                );
            }
            Ok(_) => {
                ui.label(RichText::new("No project images are available").weak());
                show_create_validation_message(ui, validation.as_ref(), EguiUiCreateField::Image);
            }
            Err(error) => {
                ui.label(RichText::new(error.to_string()).color(ui.visuals().error_fg_color));
            }
        },
        UiCatalogNodeKind::Button => {
            let label_name = ui.label(RichText::new("Label").weak());
            let label = ui
                .add_enabled(enabled, TextEdit::singleline(&mut draft.button_label))
                .labelled_by(label_name.id);
            record_structure_control("ui-structure-create-button-label", &label);
            if matches!(draft.focus, EguiUiCreateFocus::Initial) {
                label.request_focus();
                initial_focus_applied = true;
            }
            changed |= label.changed();
            invalid_focus_applied |= show_create_validation_for_field(
                ui,
                validation.as_ref(),
                EguiUiCreateField::ButtonLabel,
                &label,
                should_focus_invalid,
            );
            let action_name = ui.label(RichText::new("Action").weak());
            let action = ui
                .add_enabled(enabled, TextEdit::singleline(&mut draft.button_action))
                .labelled_by(action_name.id);
            record_structure_control("ui-structure-create-button-action", &action);
            changed |= action.changed();
            invalid_focus_applied |= show_create_validation_for_field(
                ui,
                validation.as_ref(),
                EguiUiCreateField::ButtonAction,
                &action,
                should_focus_invalid,
            );
        }
    }
    if initial_focus_applied {
        draft.focus = EguiUiCreateFocus::Settled;
    }
    if invalid_focus_applied && let Some(validation) = &validation {
        draft.focus = EguiUiCreateFocus::Invalid(validation.field);
    }

    let mut cancelled = false;
    let mut request = None;
    ui.horizontal(|ui| {
        let create = ui.add_enabled(
            placement_valid && validation.is_none() && enabled,
            Button::new("Create UI node"),
        );
        record_structure_control("ui-structure-create-submit", &create);
        if create.clicked() {
            request = intent.clone().ok();
        }
        let cancel = ui.add_enabled(enabled, Button::new("Cancel"));
        record_structure_control("ui-structure-cancel", &cancel);
        cancelled = cancel.clicked();
    });
    EguiUiStructureDialogResponse {
        cancelled,
        changed,
        intent: request,
    }
}

fn create_validation_for_rejection(
    kind: UiCatalogNodeKind,
    rejection: UiStructureRejection,
) -> EguiUiCreateValidation {
    let field = match rejection {
        UiStructureRejection::ImageChoiceRequired
        | UiStructureRejection::StaleImageChoice
        | UiStructureRejection::ImageChoiceUnavailable
        | UiStructureRejection::InvalidImageChoiceFact
        | UiStructureRejection::ImageChoiceLimitExceeded
        | UiStructureRejection::DuplicateImageChoice => EguiUiCreateField::Image,
        UiStructureRejection::ButtonActionRequired => EguiUiCreateField::ButtonAction,
        UiStructureRejection::ButtonLabelRequired => EguiUiCreateField::ButtonLabel,
        UiStructureRejection::CreateTextLimitExceeded => EguiUiCreateField::Text,
        UiStructureRejection::InvalidRecipe => match kind {
            UiCatalogNodeKind::Container => EguiUiCreateField::Placement,
            UiCatalogNodeKind::Text => EguiUiCreateField::Text,
            UiCatalogNodeKind::Image => EguiUiCreateField::Image,
            UiCatalogNodeKind::Button => EguiUiCreateField::ButtonLabel,
        },
        UiStructureRejection::TargetNotProjected
        | UiStructureRejection::ProtectedRoot
        | UiStructureRejection::OmittedSourceDescendant
        | UiStructureRejection::NestedUiRoot
        | UiStructureRejection::ParentNotProjected
        | UiStructureRejection::DescendantParent
        | UiStructureRejection::InvalidPlacementOwner
        | UiStructureRejection::InvalidSiblingAnchor => EguiUiCreateField::Placement,
    };
    EguiUiCreateValidation {
        field,
        message: rejection.to_string(),
    }
}

fn show_create_validation_for_field(
    ui: &mut Ui,
    validation: Option<&EguiUiCreateValidation>,
    field: EguiUiCreateField,
    response: &egui::Response,
    request_focus: bool,
) -> bool {
    if !show_create_validation_message(ui, validation, field) {
        return false;
    }
    if request_focus {
        response.request_focus();
        return true;
    }
    false
}

fn show_create_validation_message(
    ui: &mut Ui,
    validation: Option<&EguiUiCreateValidation>,
    field: EguiUiCreateField,
) -> bool {
    let Some(validation) = validation.filter(|validation| validation.field == field) else {
        return false;
    };
    ui.label(RichText::new(&validation.message).color(ui.visuals().error_fg_color));
    true
}

fn show_move_dialog(
    ui: &mut Ui,
    draft: &mut EguiUiMoveDraft,
    projection: &UiAuthoringProjection,
    enabled: bool,
) -> EguiUiStructureDialogResponse {
    ui.label(RichText::new("Move UI subtree").strong());
    if let Some(node) = projection
        .hierarchy()
        .iter()
        .find(|node| node.entity() == &draft.target)
    {
        ui.label(node.display_path());
    }
    let candidates = projection.structure_placement_candidates(Some(&draft.target));
    let (placement_valid, changed) = match &candidates {
        Ok(candidates) => {
            let (valid, changed, _) = show_placement_control(
                ui,
                "ui-structure-move-placement",
                &mut draft.placement,
                candidates,
                enabled,
            );
            (valid, changed)
        }
        Err(error) => {
            ui.label(RichText::new(error.to_string()).color(ui.visuals().error_fg_color));
            (false, false)
        }
    };
    let mut cancelled = false;
    let mut request = None;
    ui.horizontal(|ui| {
        let apply = ui.add_enabled(placement_valid && enabled, Button::new("Apply move"));
        record_structure_control("ui-structure-move-submit", &apply);
        if apply.clicked() {
            request = Some(UiStructureIntent::Move {
                target: draft.target.clone(),
                placement: draft.placement.clone(),
            });
        }
        let cancel = ui.add_enabled(enabled, Button::new("Cancel"));
        record_structure_control("ui-structure-cancel", &cancel);
        cancelled = cancel.clicked();
    });
    EguiUiStructureDialogResponse {
        cancelled,
        changed,
        intent: request,
    }
}

fn show_delete_dialog(
    ui: &mut Ui,
    draft: &EguiUiDeleteDraft,
    projection: &UiAuthoringProjection,
    enabled: bool,
) -> EguiUiStructureDialogResponse {
    ui.label(RichText::new("Delete UI subtree").strong());
    let target = projection
        .hierarchy()
        .iter()
        .find(|node| node.entity() == &draft.target);
    let valid = target.is_some()
        && projection
            .prepare_structure(
                &UiStructureIntent::Delete {
                    target: draft.target.clone(),
                },
                None,
            )
            .is_ok();
    if let Some(target) = target {
        ui.label(format!(
            "Delete {} and its complete source-local subtree?",
            target.display_path()
        ));
    } else {
        ui.label(
            RichText::new("The selected source subtree is no longer available")
                .color(ui.visuals().error_fg_color),
        );
    }
    ui.label(RichText::new("Undo remains available after deletion.").weak());
    let mut cancelled = false;
    let mut request = None;
    ui.horizontal(|ui| {
        let delete = ui.add_enabled(valid && enabled, Button::new("Delete subtree"));
        record_structure_control("ui-structure-delete-confirm", &delete);
        if delete.clicked() {
            request = Some(UiStructureIntent::Delete {
                target: draft.target.clone(),
            });
        }
        let cancel = ui.add_enabled(enabled, Button::new("Cancel"));
        record_structure_control("ui-structure-cancel", &cancel);
        cancelled = cancel.clicked();
    });
    EguiUiStructureDialogResponse {
        cancelled,
        changed: false,
        intent: request,
    }
}

fn show_placement_control(
    ui: &mut Ui,
    id: &'static str,
    placement: &mut ScenePlacement,
    candidates: &[UiPlacementCandidate],
    enabled: bool,
) -> (bool, bool, egui::Response) {
    let label = ui.label(RichText::new("Placement").weak());
    let before = placement.clone();
    let selected = candidates
        .iter()
        .find(|candidate| candidate.placement() == placement)
        .map_or_else(|| "Placement unavailable".to_owned(), placement_label);
    let response = ui
        .add_enabled_ui(enabled, |ui| {
            ComboBox::from_id_salt(id)
                .selected_text(selected)
                .show_ui(ui, |ui| {
                    for candidate in candidates {
                        ui.selectable_value(
                            placement,
                            candidate.placement().clone(),
                            placement_label(candidate),
                        );
                    }
                })
                .response
        })
        .inner
        .labelled_by(label.id);
    record_structure_control(id, &response);
    (
        candidates
            .iter()
            .any(|candidate| candidate.placement() == placement),
        *placement != before,
        response,
    )
}

fn placement_label(candidate: &UiPlacementCandidate) -> String {
    let anchor = match &candidate.placement().anchor {
        ScenePlacementAnchor::First => "first".to_owned(),
        ScenePlacementAnchor::Last => "last".to_owned(),
        ScenePlacementAnchor::Before { sibling } => format!("before {}", sibling.as_str()),
        ScenePlacementAnchor::After { sibling } => format!("after {}", sibling.as_str()),
    };
    format!(
        "{} / {}{}",
        candidate.parent().as_str(),
        anchor,
        if candidate.is_current() {
            " (current)"
        } else {
            ""
        }
    )
}

fn structural_read_only_label(reason: UiAuthoringStructuralReadOnlyReason) -> &'static str {
    match reason {
        UiAuthoringStructuralReadOnlyReason::ProtectedRoot => {
            "The authored UI root cannot be moved or deleted"
        }
        UiAuthoringStructuralReadOnlyReason::OmittedSourceDescendant => {
            "The complete source subtree is not authorable in this view"
        }
        UiAuthoringStructuralReadOnlyReason::NestedUiRoot => {
            "The source subtree contains a protected nested UI root"
        }
    }
}

fn structure_action_rejection_label(rejection: EditorActionRejection) -> &'static str {
    match rejection {
        EditorActionRejection::NoPublishedFrame
        | EditorActionRejection::StaleFrame { .. }
        | EditorActionRejection::FrameAlreadyDispatched { .. } => {
            "The Editor frame changed before the structure change was applied"
        }
        EditorActionRejection::DocumentNotInFrame { .. }
        | EditorActionRejection::StaleDocumentFact { .. }
        | EditorActionRejection::ActiveDocumentChanged { .. } => {
            "The active Prefab source changed; review the current hierarchy and try again"
        }
        EditorActionRejection::MutationAlreadySubmitted { .. } => {
            "Another document mutation already used this Editor frame"
        }
        EditorActionRejection::StructureContractMismatch { .. } => {
            "The structure request no longer matches the current UI source facts"
        }
        EditorActionRejection::FieldContractMismatch { .. }
        | EditorActionRejection::RuntimeGenerationChanged { .. }
        | EditorActionRejection::RuntimeDocumentChanged { .. }
        | EditorActionRejection::InvalidReopenTarget => {
            "The structure change was rejected by the current Editor authority"
        }
    }
}

fn record_structure_control(id: &str, response: &egui::Response) {
    #[cfg(test)]
    crate::editor::record_test_control(id.to_owned(), response.rect);
    #[cfg(not(test))]
    let _ = (id, response);
}

#[derive(Debug, Clone, Copy)]
struct EguiUiControlResponse {
    changed: bool,
    commit_requested: bool,
    escape_pressed: bool,
}

fn show_typed_control(
    ui: &mut Ui,
    field: &UiAuthoringFieldDescriptor,
    draft_key: &EguiUiFieldDraftKey,
    text_drafts: &mut HashMap<EguiUiFieldDraftKey, EguiUiTextDraft>,
    value: &mut UiAuthoringFieldValue,
    enabled: bool,
    label_id: egui::Id,
) -> EguiUiControlResponse {
    let response = match (field.kind(), &mut *value) {
        (UiAuthoringFieldKind::Bool, UiAuthoringFieldValue::Bool(value)) => ui
            .add_enabled(enabled, egui::Checkbox::without_text(value))
            .labelled_by(label_id),
        (UiAuthoringFieldKind::SignedInteger, UiAuthoringFieldValue::SignedInteger(value)) => {
            ui.add_enabled(enabled, DragValue::new(value))
        }
        (UiAuthoringFieldKind::UnsignedInteger, UiAuthoringFieldValue::UnsignedInteger(value)) => {
            ui.add_enabled(enabled, DragValue::new(value))
        }
        (UiAuthoringFieldKind::Scalar(metadata), UiAuthoringFieldValue::Scalar(value)) => {
            let mut control = DragValue::new(value)
                .speed(metadata.step())
                .max_decimals(usize::from(metadata.precision()));
            if let (Some(minimum), Some(maximum)) = (metadata.minimum(), metadata.maximum()) {
                control = control.range(minimum..=maximum);
            } else if let Some(minimum) = metadata.minimum() {
                control = control.range(minimum..=f64::INFINITY);
            } else if let Some(maximum) = metadata.maximum() {
                control = control.range(f64::NEG_INFINITY..=maximum);
            }
            let response = ui.add_enabled(enabled, control);
            if let Some(unit) = metadata.unit() {
                ui.label(RichText::new(unit).weak());
            }
            response
        }
        (UiAuthoringFieldKind::Text { multiline }, UiAuthoringFieldValue::Text(current)) => {
            return show_text_control(ui, draft_key, text_drafts, current, enabled, *multiline);
        }
        (UiAuthoringFieldKind::Action, UiAuthoringFieldValue::Action(current)) => {
            return show_text_control(ui, draft_key, text_drafts, current, enabled, false);
        }
        (UiAuthoringFieldKind::Enum { choices }, UiAuthoringFieldValue::Enum(current)) => {
            let before = current.clone();
            ui.add_enabled_ui(enabled, |ui| {
                ComboBox::from_id_salt(draft_key)
                    .selected_text(current.as_str())
                    .show_ui(ui, |ui| {
                        for choice in choices {
                            ui.selectable_value(current, (*choice).to_owned(), *choice);
                        }
                    });
            });
            return EguiUiControlResponse {
                changed: *current != before,
                commit_requested: *current != before,
                escape_pressed: ui.input(|input| input.key_pressed(egui::Key::Escape)),
            };
        }
        (UiAuthoringFieldKind::Dimension, UiAuthoringFieldValue::Dimension { kind, value }) => {
            let before = (*kind, *value);
            ui.add_enabled_ui(enabled, |ui| {
                ComboBox::from_id_salt((draft_key, "dimension"))
                    .selected_text(dimension_label(*kind))
                    .show_ui(ui, |ui| {
                        for candidate in [
                            nara_ui::__private::UiAuthoringDimensionKind::Auto,
                            nara_ui::__private::UiAuthoringDimensionKind::Pixels,
                            nara_ui::__private::UiAuthoringDimensionKind::Percent,
                        ] {
                            ui.selectable_value(kind, candidate, dimension_label(candidate));
                        }
                    });
                if !matches!(*kind, nara_ui::__private::UiAuthoringDimensionKind::Auto) {
                    ui.add(DragValue::new(value).speed(1.0));
                }
            });
            return EguiUiControlResponse {
                changed: (*kind, *value) != before,
                commit_requested: (*kind, *value) != before,
                escape_pressed: ui.input(|input| input.key_pressed(egui::Key::Escape)),
            };
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
            let response = ui.add_enabled_ui(enabled, |ui| {
                ui.color_edit_button_rgba_unmultiplied(&mut rgba)
            });
            *red = f64::from(rgba[0]);
            *green = f64::from(rgba[1]);
            *blue = f64::from(rgba[2]);
            *alpha = f64::from(rgba[3]);
            response.inner
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
            let before = (*kind, *red, *green, *blue, *alpha, reference.clone());
            ui.add_enabled_ui(enabled, |ui| {
                ComboBox::from_id_salt((draft_key, "brush"))
                    .selected_text(brush_label(*kind))
                    .show_ui(ui, |ui| {
                        for candidate in [
                            UiBrushAuthoringKind::Solid,
                            UiBrushAuthoringKind::Role,
                            UiBrushAuthoringKind::Image,
                        ] {
                            ui.selectable_value(kind, candidate, brush_label(candidate));
                        }
                    });
                match kind {
                    UiBrushAuthoringKind::Solid => {
                        let mut rgba = [*red as f32, *green as f32, *blue as f32, *alpha as f32];
                        ui.color_edit_button_rgba_unmultiplied(&mut rgba);
                        *red = f64::from(rgba[0]);
                        *green = f64::from(rgba[1]);
                        *blue = f64::from(rgba[2]);
                        *alpha = f64::from(rgba[3]);
                    }
                    UiBrushAuthoringKind::Role | UiBrushAuthoringKind::Image => {
                        ui.add_enabled(
                            !matches!(kind, UiBrushAuthoringKind::Image),
                            TextEdit::singleline(reference),
                        );
                    }
                }
            });
            let after = (*kind, *red, *green, *blue, *alpha, reference.clone());
            return EguiUiControlResponse {
                changed: after != before,
                commit_requested: after != before,
                escape_pressed: ui.input(|input| input.key_pressed(egui::Key::Escape)),
            };
        }
        (UiAuthoringFieldKind::ReadOnly, current)
        | (_, current @ UiAuthoringFieldValue::Raw(_)) => {
            ui.label(RichText::new(value_label(current)).monospace().weak())
        }
        _ => ui.label(RichText::new("Invalid authoring value").weak()),
    };
    #[cfg(test)]
    crate::editor::record_test_control(
        format!(
            "ui-field:{}:{}:{}",
            draft_key.field.entity().as_str(),
            draft_key.field.component().as_str(),
            draft_key.field.primary_field().as_str()
        ),
        response.rect,
    );
    EguiUiControlResponse {
        changed: response.changed(),
        commit_requested: response.drag_stopped() || (response.changed() && !response.dragged()),
        escape_pressed: ui.input(|input| input.key_pressed(egui::Key::Escape)),
    }
}

fn show_text_control(
    ui: &mut Ui,
    key: &EguiUiFieldDraftKey,
    drafts: &mut HashMap<EguiUiFieldDraftKey, EguiUiTextDraft>,
    current: &mut String,
    enabled: bool,
    multiline: bool,
) -> EguiUiControlResponse {
    let draft = drafts
        .entry(key.clone())
        .or_insert_with(|| EguiUiTextDraft {
            text: current.clone(),
            dirty: false,
        });
    let edit = if multiline {
        TextEdit::multiline(&mut draft.text).desired_rows(2)
    } else {
        TextEdit::singleline(&mut draft.text)
    };
    let response = ui.add_enabled(enabled, edit);
    if response.changed() {
        draft.dirty = true;
    }
    let commit_requested = draft.dirty && response.lost_focus();
    if commit_requested {
        *current = draft.text.clone();
    }
    EguiUiControlResponse {
        changed: commit_requested,
        commit_requested,
        escape_pressed: ui.input(|input| input.key_pressed(egui::Key::Escape)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EguiUiCanvasDragKind {
    Move,
    Resize,
}

#[derive(Debug, Clone, PartialEq)]
struct EguiUiCanvasDrag {
    entity: SceneEntityId,
    kind: EguiUiCanvasDragKind,
    transform: [f32; 7],
    latest_delta: [f64; 2],
    applied_delta: [f64; 2],
    last_sent_stamp: Option<nara_tooling::EditorGestureStamp>,
    commit_requested: bool,
}

#[derive(Debug, Clone)]
struct EguiUiCanvasSelection {
    entity: SceneEntityId,
    rect: Rect,
    resize_handle: Option<Rect>,
    move_enabled: bool,
    authored_units_per_point: [f64; 2],
    transform: [f32; 7],
}

#[derive(Debug, Default)]
pub(crate) struct EguiUiCanvasPanel {
    scope: Option<EguiUiAuthoringScope>,
    drag: Option<EguiUiCanvasDrag>,
}

impl EguiUiCanvasPanel {
    pub(crate) fn synchronize(&mut self, input: Option<&EguiUiAuthoringInput<'_>>) {
        let next = input.map(|input| EguiUiAuthoringScope {
            document: input.document(),
            revision: input.revision(),
            image_choice_generation: input.image_choice_generation(),
        });
        if self.scope != next {
            self.scope = next;
            self.drag = None;
        }
    }

    pub(crate) fn show(
        &mut self,
        ui: &mut Ui,
        input: EguiUiCanvasInput,
        authoring: &mut EguiUiAuthoringInput<'_>,
        panel_size: Vec2,
        actions: &mut Vec<EditorAction>,
    ) -> EguiUiCanvasOutput {
        let panel_top = ui.cursor().top();
        ui.horizontal(|ui| {
            ui.heading("UI Canvas");
            ui.label(RichText::new(canvas_state_label(input.content)).weak());
        });
        let header_height = (ui.cursor().top() - panel_top).max(0.0);
        let desired_size =
            Vec2::new(panel_size.x, panel_size.y - header_height).max(Vec2::splat(1.0));
        let (outer_rect, response) = ui.allocate_exact_size(desired_size, Sense::click_and_drag());
        ui.painter()
            .rect_filled(outer_rect, 2.0, Color32::from_rgb(18, 20, 25));
        let image_rect = canvas_texture_facts(input.content).map_or(outer_rect, |(_, extent)| {
            fitted_image_rect(outer_rect, extent)
        });
        if let Some((texture_id, _)) = canvas_texture_facts(input.content) {
            ui.painter().image(
                texture_id,
                image_rect,
                Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                Color32::WHITE,
            );
        }
        let selection = self.paint_selection(ui, authoring, image_rect, input.is_presented());
        self.handle_gesture(ui, input, authoring, selection.as_ref(), actions);

        let pick_uv = (input.is_presented() && response.clicked_by(PointerButton::Primary))
            .then(|| response.interact_pointer_pos())
            .flatten()
            .filter(|point| image_rect.contains(*point))
            .map(|point| normalized_pointer(point, image_rect));

        if !input.is_presented() || input.notice.is_some() {
            ui.painter().text(
                outer_rect.center(),
                Align2::CENTER_CENTER,
                input
                    .notice
                    .unwrap_or_else(|| canvas_state_label(input.content)),
                FontId::proportional(13.0),
                Color32::from_gray(210),
            );
        }
        EguiUiCanvasOutput {
            desired_size_points: outer_rect.size(),
            pick_uv,
        }
    }

    fn paint_selection(
        &self,
        ui: &Ui,
        authoring: &EguiUiAuthoringInput<'_>,
        image_rect: Rect,
        interactive: bool,
    ) -> Option<EguiUiCanvasSelection> {
        let (Ok(projection), Some(selected)) =
            (authoring.projection(), authoring.selected_entity())
        else {
            return None;
        };
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
        let content_origin = transform.physical_content_origin();
        let scale = transform.content_scale_factor();
        let rect = fact.rect();
        let min = rect.origin();
        let max = rect.max();
        let uv_min = Vec2::new(
            (content_origin.x() + min.x() * scale) / physical.width(),
            (content_origin.y() + min.y() * scale) / physical.height(),
        );
        let uv_max = Vec2::new(
            (content_origin.x() + max.x() * scale) / physical.width(),
            (content_origin.y() + max.y() * scale) / physical.height(),
        );
        let selected_rect = Rect::from_min_max(
            image_rect.min + uv_min * image_rect.size(),
            image_rect.min + uv_max * image_rect.size(),
        );
        let color = if interactive {
            Color32::from_rgb(75, 160, 255)
        } else {
            Color32::from_gray(130)
        };
        ui.painter().rect_stroke(
            selected_rect,
            0.0,
            Stroke::new(1.5, color),
            egui::StrokeKind::Inside,
        );
        let resize_handle = (node.gizmos().resize_node()
            == UiDirectManipulationCapability::Editable)
            .then(|| Rect::from_center_size(selected_rect.max, Vec2::splat(9.0)));
        if let Some(handle) = resize_handle {
            ui.painter().rect_filled(handle, 1.0, color);
        }
        Some(EguiUiCanvasSelection {
            entity: selected.clone(),
            rect: selected_rect,
            resize_handle,
            move_enabled: node.gizmos().move_node() == UiDirectManipulationCapability::Editable,
            authored_units_per_point: [
                f64::from(physical.width() / image_rect.width() / scale),
                f64::from(physical.height() / image_rect.height() / scale),
            ],
            transform: [
                physical.width(),
                physical.height(),
                content_origin.x(),
                content_origin.y(),
                scale,
                image_rect.width(),
                image_rect.height(),
            ],
        })
    }

    fn handle_gesture(
        &mut self,
        ui: &mut Ui,
        input: EguiUiCanvasInput,
        authoring: &mut EguiUiAuthoringInput<'_>,
        selection: Option<&EguiUiCanvasSelection>,
        actions: &mut Vec<EditorAction>,
    ) {
        let move_response = selection
            .filter(|selection| selection.move_enabled)
            .map(|selection| {
                ui.interact(
                    selection.rect,
                    ui.id().with(("ui-canvas-move", selection.entity.as_str())),
                    Sense::drag(),
                )
            });
        let resize_response = selection
            .and_then(|selection| selection.resize_handle.map(|handle| (selection, handle)))
            .map(|(selection, handle)| {
                ui.interact(
                    handle,
                    ui.id()
                        .with(("ui-canvas-resize", selection.entity.as_str())),
                    Sense::drag(),
                )
            });

        if self.drag.is_none() && input.is_presented() && authoring.gesture().is_none() {
            let started = resize_response
                .as_ref()
                .filter(|response| response.drag_started())
                .and_then(|_| selection.map(|selection| (selection, EguiUiCanvasDragKind::Resize)))
                .or_else(|| {
                    move_response
                        .as_ref()
                        .filter(|response| response.drag_started())
                        .and_then(|_| {
                            selection.map(|selection| (selection, EguiUiCanvasDragKind::Move))
                        })
                });
            if let Some((selection, kind)) = started {
                let baseline = canvas_direct_edit(kind, &selection.entity, [0.0, 0.0]);
                if let Ok(baseline) = authoring.lower_direct_edit(&baseline) {
                    actions.push(EditorAction::Gesture(EditorGestureAction::Begin {
                        baseline,
                    }));
                    self.drag = Some(EguiUiCanvasDrag {
                        entity: selection.entity.clone(),
                        kind,
                        transform: selection.transform,
                        latest_delta: [0.0, 0.0],
                        applied_delta: [0.0, 0.0],
                        last_sent_stamp: None,
                        commit_requested: false,
                    });
                }
            }
        }

        let Some(mut drag) = self.drag.take() else {
            return;
        };
        let response = match drag.kind {
            EguiUiCanvasDragKind::Move => move_response.as_ref(),
            EguiUiCanvasDragKind::Resize => resize_response.as_ref(),
        };
        let gesture = authoring
            .gesture()
            .filter(|gesture| {
                let target = gesture.baseline().target();
                target.document() == authoring.document()
                    && target.base_revision() == authoring.revision()
                    && target.entity() == &drag.entity
            })
            .map(EditorGestureView::stamp);
        let exact_selection = selection.filter(|selection| {
            selection.entity == drag.entity && selection.transform == drag.transform
        });
        if !input.keeps_gesture_alive() || exact_selection.is_none() {
            if let Some(gesture) = gesture {
                actions.push(EditorAction::Gesture(EditorGestureAction::Cancel {
                    gesture: gesture.id(),
                    reason: EditorGestureCancelReason::CanvasTransformChanged,
                }));
            }
            return;
        }
        let exact_selection = exact_selection.expect("the exact selection was checked");
        if let Some(response) = response {
            let delta = response.drag_delta();
            drag.latest_delta = [
                f64::from(delta.x) * exact_selection.authored_units_per_point[0],
                f64::from(delta.y) * exact_selection.authored_units_per_point[1],
            ];
            drag.commit_requested |= response.drag_stopped();
        }

        if ui.input(|state| state.viewport().focused == Some(false)) {
            if let Some(gesture) = gesture {
                actions.push(EditorAction::Gesture(EditorGestureAction::Cancel {
                    gesture: gesture.id(),
                    reason: EditorGestureCancelReason::FocusLost,
                }));
            }
            return;
        }
        if ui.input(|state| state.key_pressed(egui::Key::Escape)) {
            if let Some(gesture) = gesture {
                actions.push(EditorAction::Gesture(EditorGestureAction::Cancel {
                    gesture: gesture.id(),
                    reason: EditorGestureCancelReason::Escape,
                }));
            }
            return;
        }
        let Some(gesture) = gesture else {
            self.drag = Some(drag);
            return;
        };
        if !input.is_presented() {
            self.drag = Some(drag);
            return;
        }
        if drag.last_sent_stamp == Some(gesture) {
            self.drag = Some(drag);
            return;
        }

        let delta = [
            drag.latest_delta[0] - drag.applied_delta[0],
            drag.latest_delta[1] - drag.applied_delta[1],
        ];
        let edit = canvas_direct_edit(drag.kind, &drag.entity, delta);
        let Ok(edit) = authoring.lower_direct_edit(&edit) else {
            actions.push(EditorAction::Gesture(EditorGestureAction::Cancel {
                gesture: gesture.id(),
                reason: EditorGestureCancelReason::InvalidPreview,
            }));
            return;
        };
        if drag.commit_requested {
            actions.push(EditorAction::Gesture(EditorGestureAction::Commit {
                gesture: gesture.id(),
                edit,
            }));
            return;
        }
        if delta != [0.0, 0.0] {
            actions.push(EditorAction::Gesture(EditorGestureAction::Preview {
                gesture: gesture.id(),
                edit,
            }));
            drag.applied_delta = drag.latest_delta;
            drag.last_sent_stamp = Some(gesture);
        }
        self.drag = Some(drag);
    }
}

fn canvas_direct_edit(
    kind: EguiUiCanvasDragKind,
    entity: &SceneEntityId,
    delta: [f64; 2],
) -> UiAuthoringDirectEdit {
    match kind {
        EguiUiCanvasDragKind::Move => UiAuthoringDirectEdit::MoveBy {
            entity: entity.clone(),
            delta,
        },
        EguiUiCanvasDragKind::Resize => UiAuthoringDirectEdit::ResizeBy {
            entity: entity.clone(),
            delta,
        },
    }
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

fn read_only_label(reason: UiAuthoringReadOnlyReason) -> &'static str {
    match reason {
        UiAuthoringReadOnlyReason::SemanticAuthoringDeferred => {
            "Semantic authoring is not available in this Editor slice"
        }
        UiAuthoringReadOnlyReason::AssetSelectionDeferred => {
            "Asset selection requires a future Asset Browser"
        }
        UiAuthoringReadOnlyReason::UnsupportedComplexValue => "This structured value is read-only",
        UiAuthoringReadOnlyReason::ComponentVersionMismatch => {
            "The component version does not match the authoring schema"
        }
        UiAuthoringReadOnlyReason::InvalidStoredValue => "The stored value is invalid",
    }
}

fn dimension_label(kind: nara_ui::__private::UiAuthoringDimensionKind) -> &'static str {
    match kind {
        nara_ui::__private::UiAuthoringDimensionKind::Auto => "Auto",
        nara_ui::__private::UiAuthoringDimensionKind::Pixels => "Pixels",
        nara_ui::__private::UiAuthoringDimensionKind::Percent => "Percent",
    }
}

fn brush_label(kind: UiBrushAuthoringKind) -> &'static str {
    match kind {
        UiBrushAuthoringKind::Solid => "Solid",
        UiBrushAuthoringKind::Role => "Theme role",
        UiBrushAuthoringKind::Image => "Image",
    }
}

fn value_label(value: &UiAuthoringFieldValue) -> &'static str {
    match value {
        UiAuthoringFieldValue::Bool(_) => "Boolean",
        UiAuthoringFieldValue::SignedInteger(_) => "Integer",
        UiAuthoringFieldValue::UnsignedInteger(_) => "Unsigned integer",
        UiAuthoringFieldValue::Scalar(_) => "Scalar",
        UiAuthoringFieldValue::Text(_) => "Text",
        UiAuthoringFieldValue::Action(_) => "Action",
        UiAuthoringFieldValue::Enum(_) => "Enum",
        UiAuthoringFieldValue::Dimension { .. } => "Dimension",
        UiAuthoringFieldValue::Color { .. } => "Color",
        UiAuthoringFieldValue::Brush { .. } => "Brush",
        UiAuthoringFieldValue::Raw(_) => "Structured value",
    }
}

fn canvas_state_label(content: EguiUiCanvasContent) -> &'static str {
    match content {
        EguiUiCanvasContent::Presented { .. } => "Ready",
        EguiUiCanvasContent::Previewing { .. } => "Updating",
        EguiUiCanvasContent::Resizing => "Resizing",
        EguiUiCanvasContent::Unavailable => "Unavailable",
    }
}

fn canvas_texture_facts(content: EguiUiCanvasContent) -> Option<(TextureId, [u32; 2])> {
    match content {
        EguiUiCanvasContent::Presented { texture_id, extent }
        | EguiUiCanvasContent::Previewing { texture_id, extent } => Some((texture_id, extent)),
        EguiUiCanvasContent::Resizing | EguiUiCanvasContent::Unavailable => None,
    }
}

fn normalized_pointer(pointer: Pos2, rect: Rect) -> [f32; 2] {
    [
        ((pointer.x - rect.left()) / rect.width()).clamp(0.0, 1.0),
        ((pointer.y - rect.top()) / rect.height()).clamp(0.0, 1.0),
    ]
}
