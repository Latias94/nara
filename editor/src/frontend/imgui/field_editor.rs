//! Dear ImGui field drawing and submission helpers over shared private drafts.

pub(super) mod entity_reference;

use dear_imgui_rs::{Key, Ui};
use nara::{
    reflect::{
        ComponentCapability, ComponentFieldId, ComponentFieldPathSegment, ComponentTypeId,
        ComponentValue, ComponentValueShape,
    },
    scene::SceneEntityId,
    tooling::{
        EditorAction, EditorDocumentMutation, EditorFrameModel, EditorSceneModel,
        SceneInspectorCommand, SceneInspectorComponentView, SceneInspectorFieldState,
        SceneInspectorFieldView,
    },
};
#[cfg(test)]
use nara::{
    reflect::{ComponentSchemaVersion, ComponentValueKind},
    tooling::EditorFieldTarget,
};

use super::authoring::{self, recursive_controls, value_draft};
use super::field_drafts::{Candidate, FieldBasis, FieldDraft, FieldKey, source};
pub(super) use super::field_drafts::{FieldEditors, PreparedFieldEdit};
use super::{
    ImguiEditorPanelContext, ImguiTextOwner, PanelActionDispatch, PanelSubmissionId,
    observe_text_owner,
};
use crate::frontend::controls::editing_enabled;

impl FieldEditors {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn show(
        &mut self,
        ui: &Ui,
        frame: &EditorFrameModel,
        scene: &EditorSceneModel,
        entity: &SceneEntityId,
        component: &SceneInspectorComponentView,
        field: &SceneInspectorFieldView,
        editable: bool,
        text_owner: &mut Option<ImguiTextOwner>,
    ) -> Option<PreparedFieldEdit> {
        let basis = FieldBasis::new(frame, scene, entity, component, field);
        let key = FieldKey::from_target(&basis.target);
        let _id = ui.push_id(field.id.as_str());
        ui.text(format!(
            "{}{}",
            field.path,
            if field.required { " *" } else { "" }
        ));
        if !self.painted.insert(key.clone()) {
            ui.text_wrapped(authoring::value_label(source(field)));
            ui.text_disabled("This field already has an editor in this panel");
            return None;
        }
        if !value_draft::supported(&field.shape) {
            ui.text_wrapped(authoring::value_label(source(field)));
            ui.text_disabled("This field shape is read-only in the generic Inspector");
            self.show_requested_recovery(ui, &key);
            return None;
        }
        if !self.has_draft_capacity(&key) {
            ui.text_wrapped(authoring::value_label(source(field)));
            ui.text_disabled(
                "Field draft capacity reached. Discard retained edits to open another editor.",
            );
            if ui.button("Discard retained field drafts") {
                self.drafts.retain(|_, draft| draft.in_flight());
            }
            return None;
        }
        if !self.drafts.contains_key(&key) {
            self.drafts
                .insert(key.clone(), FieldDraft::new(basis.clone()));
        }
        let draft = self.drafts.get_mut(&key).expect("the field was admitted");
        draft.synchronize(basis.clone());
        let pending = draft.in_flight();
        let mut discard = false;
        let mut apply = false;
        let mut remove = false;
        let mut reapply = false;
        let mut converted = None;
        if draft.incompatible {
            ui.text_wrapped("The schema or source changed. Copy the retained input, then reopen the current field.");
            ui.text_wrapped(format!(
                "Current: {}",
                authoring::value_label(source(field))
            ));
            show_copy(ui, &draft.candidate);
            discard = button(ui, &key, "Discard draft and reopen", !pending);
        } else {
            if draft.conflict {
                ui.text_colored(
                    [1.0, 0.7, 0.2, 1.0],
                    "Conflict: review the accepted value before reapplying.",
                );
                ui.text_wrapped(format!(
                    "Current: {}",
                    authoring::value_label(source(field))
                ));
                ui.text("Retained draft:");
                if draft.remove_intent {
                    ui.text("Remove the authored override");
                }
            }
            let controls_enabled = editable && !pending;
            let before = draft.candidate.clone();
            match &mut draft.candidate {
                Candidate::Text(text) => {
                    let enter = if field.shape == ComponentValueShape::EntityRef {
                        ui.with_disabled_if(!controls_enabled, || {
                            entity_reference::show(ui, &scene.editor.inspector.entities, text);
                        });
                        false
                    } else {
                        let enter = ui.with_disabled_if(!controls_enabled, || {
                            ui.input_text("##value", text)
                                .enter_returns_true(true)
                                .build()
                        });
                        observe_text_owner(ui, text_owner);
                        if ui.is_item_active() && ui.is_key_pressed(Key::Escape) && !pending {
                            discard = true;
                        }
                        enter
                    };
                    record(
                        ui,
                        format!(
                            "field:{}:{}",
                            component.component.as_str(),
                            field.id.as_str()
                        ),
                    );
                    ui.same_line();
                    apply = (button(ui, &key, "Apply", controls_enabled && !draft.conflict)
                        || enter)
                        && controls_enabled
                        && !draft.conflict;
                }
                Candidate::Bool(value) => {
                    apply = ui
                        .with_disabled_if(!controls_enabled, || ui.checkbox("##value", value))
                        && !draft.conflict;
                    record(
                        ui,
                        format!(
                            "field:{}:{}",
                            component.component.as_str(),
                            field.id.as_str()
                        ),
                    );
                    if draft.dirty && !draft.conflict {
                        ui.same_line();
                        apply |= button(ui, &key, "Apply", controls_enabled);
                    }
                }
                Candidate::Structured(root) => {
                    apply = recursive_controls::show_root(
                        ui,
                        &basis.target,
                        &scene.editor.inspector.entities,
                        &field.shape,
                        source(field),
                        root,
                        controls_enabled,
                        !draft.conflict,
                        text_owner,
                    );
                }
            }
            if draft.candidate != before {
                draft.dirty = true;
                draft.error = None;
                draft.remove_intent = false;
            }
            if let Candidate::Structured(root) = &draft.candidate
                && root.node.is_none()
                && before != draft.candidate
            {
                discard = true;
            }
            if draft.dirty && !draft.remove_intent {
                let value = draft.candidate.value(&draft.basis.field.shape);
                if let Err(error) = &value {
                    draft.error = Some(error.clone());
                }
                converted = Some(value);
            }
            if draft.conflict && button(ui, &key, "Reapply draft", controls_enabled) {
                draft.basis = basis.clone();
                draft.conflict = false;
                apply = true;
                reapply = true;
            }
            if draft.dirty {
                ui.same_line();
                discard |= button(ui, &key, "Discard draft", !pending);
            }
            let remove_label = if field.default_value.is_some() {
                "Reset to default"
            } else {
                "Remove field"
            };
            remove = button(
                ui,
                &key,
                remove_label,
                controls_enabled
                    && !draft.conflict
                    && field.state == SceneInspectorFieldState::Present
                    && matches!(
                        field.path.segments().last(),
                        Some(ComponentFieldPathSegment::Field(_))
                    )
                    && (!field.required || field.default_value.is_some()),
            );
        }
        if pending {
            ui.text_disabled("Submitting this field edit...");
        }
        if let Some(error) = &draft.error {
            ui.text_colored([1.0, 0.4, 0.3, 1.0], error);
        }
        if let SceneInspectorFieldState::InvalidPath(reason) = &field.state {
            ui.text_wrapped(reason);
        }
        ui.separator();
        if discard {
            self.drafts.remove(&key);
            return None;
        }
        if pending || (!apply && !remove) {
            return None;
        }
        draft.dirty = true;
        if !reapply {
            draft.remove_intent = remove;
        }
        let target = &draft.basis.target;
        let command = if draft.remove_intent {
            SceneInspectorCommand::RemoveField {
                entity: target.entity().clone(),
                component: target.component().clone(),
                component_version: target.component_version(),
                field: target.field().clone(),
            }
        } else {
            match converted.unwrap_or_else(|| draft.candidate.value(&draft.basis.field.shape)) {
                Ok(value) => SceneInspectorCommand::SetField {
                    entity: target.entity().clone(),
                    component: target.component().clone(),
                    component_version: target.component_version(),
                    field: target.field().clone(),
                    value,
                },
                Err(error) => {
                    draft.error = Some(error);
                    return None;
                }
            }
        };
        Some(PreparedFieldEdit {
            key,
            action: EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
                document: target.document(),
                base_revision: target.base_revision(),
                command,
            }),
        })
    }

    fn show_requested_recovery(&mut self, ui: &Ui, key: &FieldKey) {
        let Some(draft) = self.drafts.get(key).filter(|draft| draft.dirty) else {
            return;
        };
        let _id = ui.push_id(&format!(
            "retained:{}:{}:{}",
            key.entity.as_str(),
            key.component.as_str(),
            key.field.as_str()
        ));
        ui.text_disabled(if draft.incompatible {
            "Retained incompatible draft"
        } else {
            "Retained draft (temporarily unavailable)"
        });
        show_copy(ui, &draft.candidate);
        if button(ui, key, "Discard retained draft", !draft.in_flight()) {
            self.drafts.remove(key);
        }
    }

    #[cfg(test)]
    pub(super) fn retained_text(
        &self,
        component: &str,
        field: &str,
    ) -> Option<(String, bool, bool, bool)> {
        self.drafts
            .iter()
            .find(|(key, draft)| {
                key.component.as_str() == component && key.field.as_str() == field && draft.dirty
            })
            .map(|(_, draft)| {
                (
                    draft.candidate.copy_text(),
                    draft.conflict,
                    draft.incompatible,
                    draft.in_flight(),
                )
            })
    }
}

#[cfg(test)]
impl super::ImguiEditorFrontend {
    pub(crate) fn retained_field_draft(
        &self,
        component: &str,
        field: &str,
    ) -> Option<(String, bool, bool, bool)> {
        self.presentation
            .widgets
            .values()
            .find_map(|widgets| widgets.authoring.fields.retained_text(component, field))
    }

    pub(crate) fn retained_field_error(&self, component: &str, field: &str) -> Option<String> {
        self.presentation.widgets.values().find_map(|widgets| {
            widgets
                .authoring
                .fields
                .drafts
                .iter()
                .find(|(key, draft)| {
                    key.component.as_str() == component
                        && key.field.as_str() == field
                        && draft.dirty
                })
                .and_then(|(_, draft)| draft.error.clone())
        })
    }
}

impl authoring::ImguiAuthoringPanels {
    pub(super) fn action_finished(&mut self, id: PanelSubmissionId, result: &PanelActionDispatch) {
        self.fields.action_finished(id, result);
        self.source_tool.action_finished(id, result);
    }
}

impl ImguiEditorPanelContext<'_, '_> {
    /// Edits one stable root field in this panel's resolved document.
    ///
    /// Drafts, validation, exact submission results and conflict recovery are retained privately
    /// for this panel instance. An unprojected target is shown as unavailable without changing
    /// selection. Present a field at most once per panel paint.
    pub fn edit_field(
        &mut self,
        ui: &Ui,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
        field: &ComponentFieldId,
    ) {
        let Some(document) = self.model.document() else {
            ui.text_disabled("This panel has no authoring document");
            return;
        };
        let frame = self.model.frame();
        let key = FieldKey {
            document: document.document,
            entity: entity.clone(),
            component: component.clone(),
            field: field.clone(),
        };
        let Some(scene) = frame
            .workspace()
            .active_scene
            .as_ref()
            .filter(|scene| scene.document == document.document)
        else {
            ui.text_disabled("The panel's document is not currently projected");
            self.widgets
                .authoring
                .fields
                .show_requested_recovery(ui, &key);
            return;
        };
        let Some(owner) = scene
            .editor
            .inspector
            .selected_entity_view
            .as_ref()
            .filter(|owner| &owner.id == entity)
        else {
            ui.text_disabled("The requested entity is not currently projected");
            self.widgets
                .authoring
                .fields
                .show_requested_recovery(ui, &key);
            return;
        };
        let Some(component) = owner
            .components
            .iter()
            .find(|candidate| &candidate.component == component)
        else {
            ui.text_disabled("The requested component is unavailable");
            self.widgets
                .authoring
                .fields
                .show_requested_recovery(ui, &key);
            return;
        };
        let Some(field) = component
            .fields
            .iter()
            .find(|candidate| &candidate.id == field)
        else {
            ui.text_disabled("The requested field is unavailable");
            self.widgets
                .authoring
                .fields
                .show_requested_recovery(ui, &key);
            return;
        };
        let _scope = ui.push_id(&format!(
            "document:{}:entity:{}:component:{}",
            document.document.raw(),
            entity.as_str(),
            component.component.as_str()
        ));
        let editable = !self.input.read_only
            && editing_enabled(frame)
            && component.access.is_editable()
            && field.capabilities.contains(&ComponentCapability::Edit);
        if let Some(target) = property_target(frame, scene, entity, component, field)
            && component.access.is_editable()
            && field.capabilities.contains(&ComponentCapability::Edit)
            && frame
                .source_tool()
                .active
                .as_ref()
                .is_some_and(|active| active.matches_integer_property(&target))
        {
            let review = self
                .preparation
                .as_ref()
                .ok()
                .and_then(|session| session.source_tool_review());
            let (customized, action) = self.widgets.authoring.source_tool.show_integer_property(
                ui,
                frame,
                &target,
                source(field).and_then(ComponentValue::as_i64),
                editable,
                review.as_ref(),
            );
            if let Some(owner) = self.widgets.authoring.source_tool.text_owner() {
                self.output.text_owner = Some(owner);
            }
            if customized {
                if let Some(action) = action {
                    let result = self.submit_authoring(EditorAction::SourceTool(action));
                    self.widgets.authoring.source_tool.admitted(result);
                }
                return;
            }
        }
        if let Some(prepared) = self.widgets.authoring.fields.show(
            ui,
            frame,
            scene,
            entity,
            component,
            field,
            editable,
            &mut self.output.text_owner,
        ) {
            self.submit_field(prepared);
        }
    }

    pub(super) fn submit_field(&mut self, prepared: PreparedFieldEdit) {
        let result = self.submit_authoring(prepared.action);
        self.widgets
            .authoring
            .fields
            .admitted(&prepared.key, result);
    }
}

pub(super) fn property_target(
    frame: &EditorFrameModel,
    scene: &EditorSceneModel,
    entity: &SceneEntityId,
    component: &SceneInspectorComponentView,
    field: &SceneInspectorFieldView,
) -> Option<nara::tooling::EditorPropertyTarget> {
    let document = frame.workspace().document(scene.document)?;
    Some(nara::tooling::EditorPropertyTarget {
        basis: nara::tooling::EditorDocumentQueryBasis {
            document: document.document,
            revision: scene.revision,
            schema: document.authoring_schema_basis.clone(),
        },
        entity: entity.clone(),
        component: component.component.clone(),
        property: nara::reflect::ComponentPropertyPath::new(field.id.clone()),
    })
}

fn show_copy(ui: &Ui, candidate: &Candidate) {
    ui.text_wrapped(candidate.copy_text());
    if ui.button("Copy retained input") {
        let text = std::ffi::CString::new(candidate.copy_text().replace('\0', "?"))
            .expect("interior NUL characters were sanitized");
        ui.with_bound_context(|| {
            // SAFETY: Ui binds its live Context on this thread. Dear ImGui consumes the
            // NUL-terminated string synchronously through the installed clipboard backend.
            unsafe {
                dear_imgui_rs::sys::igSetClipboardText(text.as_ptr());
            }
        });
    }
}

fn button(ui: &Ui, key: &FieldKey, label: &str, enabled: bool) -> bool {
    let clicked = ui.with_disabled_if(!enabled, || ui.button(label));
    record(
        ui,
        format!(
            "field-action:{}:{}:{label}",
            key.component.as_str(),
            key.field.as_str()
        ),
    );
    if label == "Apply" {
        record(ui, "Apply".to_owned());
    }
    clicked
}

fn record(_ui: &Ui, _name: String) {
    #[cfg(test)]
    super::interaction_tests::record_control(_ui, _name);
}

#[cfg(test)]
mod tests;
