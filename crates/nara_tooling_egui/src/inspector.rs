use std::collections::HashMap;

use egui::{Button, CollapsingHeader, RichText, TextEdit, Ui};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind,
};
use nara_scene::{SceneAuthoringRevision, SceneEntityId};
use nara_tooling::{
    EditorAction, EditorDocumentId, EditorDocumentMutation, EditorFieldFeedback,
    EditorFieldFeedbackReason, EditorFieldTarget, EditorFieldValidationFailure,
    EditorNavigationAction, EditorSceneModel, SceneInspectorCommand, SceneInspectorComponentView,
    SceneInspectorFieldState, SceneInspectorFieldView,
};

use crate::fields::{
    component_title, field_edit_source_text, field_value_label, parse_editable_value,
    value_kind_label,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InspectorDraftScope {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct EguiInspectorFieldKey {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    entity: SceneEntityId,
    component: ComponentTypeId,
    field: ComponentFieldId,
}

impl EguiInspectorFieldKey {
    fn new(
        scene: &EditorSceneModel,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
        field: &ComponentFieldId,
    ) -> Self {
        Self {
            document: scene.document,
            revision: scene.revision,
            entity: entity.clone(),
            component: component.clone(),
            field: field.clone(),
        }
    }

    fn target(&self) -> EditorFieldTarget {
        EditorFieldTarget::new(
            self.document,
            self.revision,
            self.entity.clone(),
            self.component.clone(),
            self.field.clone(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct EguiInspectorComponentKey {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    entity: SceneEntityId,
    component: ComponentTypeId,
}

impl EguiInspectorComponentKey {
    fn new(scene: &EditorSceneModel, entity: &SceneEntityId, component: &ComponentTypeId) -> Self {
        Self {
            document: scene.document,
            revision: scene.revision,
            entity: entity.clone(),
            component: component.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct EguiInspectorFieldTarget {
    key: EguiInspectorFieldKey,
    component_version: ComponentSchemaVersion,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldDraft {
    text: String,
    error: Option<String>,
}

#[derive(Debug, Default)]
pub(crate) struct EguiSceneInspectorPanel {
    active_scope: Option<InspectorDraftScope>,
    field_drafts: HashMap<EguiInspectorFieldKey, FieldDraft>,
}

impl EguiSceneInspectorPanel {
    pub(crate) fn synchronize_scene(&mut self, scene: Option<&EditorSceneModel>) {
        let next = scene.map(|scene| InspectorDraftScope {
            document: scene.document,
            revision: scene.revision,
        });
        if self.active_scope != next {
            self.active_scope = next;
            self.field_drafts.clear();
        }
    }

    pub(crate) fn show_tree(
        &mut self,
        ui: &mut Ui,
        scene: Option<&EditorSceneModel>,
        actions: &mut Vec<EditorAction>,
    ) {
        let _heading = ui.heading("Scene Tree");
        #[cfg(test)]
        crate::editor::record_test_control("Scene Tree heading", _heading.rect);
        let Some(scene) = scene else {
            ui.label(RichText::new("No active scene").weak());
            return;
        };
        if scene.editor.inspector.entities.is_empty() {
            ui.label(RichText::new("No entities").weak());
            return;
        }

        for entity in &scene.editor.inspector.entities {
            let response = ui.horizontal(|ui| {
                ui.add_space(entity.depth as f32 * 16.0);
                let prefab_marker = if entity.has_prefab { " prefab" } else { "" };
                let label = format!(
                    "{} ({} components{})",
                    entity.id.as_str(),
                    entity.inspectable_component_count,
                    prefab_marker
                );
                ui.selectable_label(entity.selected, label)
            });
            #[cfg(test)]
            crate::editor::record_test_control(
                format!("entity:{}", entity.id.as_str()),
                response.inner.rect,
            );
            if response.inner.clicked() {
                actions.push(EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document: scene.document,
                        entity: Some(entity.id.clone()),
                    },
                ));
            }
        }
    }

    pub(crate) fn show_inspector(
        &mut self,
        ui: &mut Ui,
        scene: Option<&EditorSceneModel>,
        field_feedback: Option<&EditorFieldFeedback>,
        editing_enabled: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        let _heading = ui.heading("Inspector");
        #[cfg(test)]
        crate::editor::record_test_control("Inspector heading", _heading.rect);
        let Some(scene) = scene else {
            ui.label(RichText::new("No active scene").weak());
            return;
        };
        let Some(view) = &scene.editor.inspector.selected_entity_view else {
            ui.label(RichText::new("Select an entity").weak());
            return;
        };

        ui.label(RichText::new(view.id.as_str()).strong());
        if let Some(parent) = &view.parent {
            ui.label(format!("Parent: {}", parent.as_str()));
        }
        if view.has_prefab {
            ui.label("Prefab instance");
        }
        if view.components.is_empty() {
            ui.label(RichText::new("No components").weak());
            return;
        }

        for component in &view.components {
            self.show_component(
                ui,
                scene,
                &view.id,
                component,
                field_feedback,
                editing_enabled,
                actions,
            );
        }
    }

    fn show_component(
        &mut self,
        ui: &mut Ui,
        scene: &EditorSceneModel,
        entity: &SceneEntityId,
        component: &SceneInspectorComponentView,
        field_feedback: Option<&EditorFieldFeedback>,
        editing_enabled: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        let component_key = EguiInspectorComponentKey::new(scene, entity, &component.component);
        CollapsingHeader::new(component_title(component))
            .id_salt(component_key)
            .default_open(true)
            .show(ui, |ui| {
                if component.fields.is_empty() {
                    ui.label(RichText::new("No inspectable fields").weak());
                    return;
                }
                for field in &component.fields {
                    let target = EguiInspectorFieldTarget {
                        key: EguiInspectorFieldKey::new(
                            scene,
                            entity,
                            &component.component,
                            &field.id,
                        ),
                        component_version: component.schema_version,
                    };
                    self.show_field(ui, &target, field, field_feedback, editing_enabled, actions);
                }
            });
    }

    fn show_field(
        &mut self,
        ui: &mut Ui,
        target: &EguiInspectorFieldTarget,
        field: &SceneInspectorFieldView,
        field_feedback: Option<&EditorFieldFeedback>,
        editing_enabled: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        let field_editing_enabled =
            editing_enabled && field.capabilities.contains(&ComponentCapability::Edit);
        ui.horizontal(|ui| {
            let mut field_label = field.path.to_string();
            if field.required {
                field_label.push_str(" *");
            }
            ui.label(field_label)
                .on_hover_text(format!("kind: {}", value_kind_label(field.value_kind)));

            match field.value_kind {
                ComponentValueKind::Bool => {
                    self.show_bool_field(ui, target, field, field_editing_enabled, actions)
                }
                ComponentValueKind::I64
                | ComponentValueKind::U64
                | ComponentValueKind::F64
                | ComponentValueKind::String => {
                    self.show_text_field(ui, target, field, field_editing_enabled, actions)
                }
                ComponentValueKind::Null
                | ComponentValueKind::List
                | ComponentValueKind::Map
                | ComponentValueKind::AssetRef
                | ComponentValueKind::EntityRef => {
                    ui.monospace(field_value_label(field));
                }
            }

            if field_editing_enabled
                && field.value.is_some()
                && !field.required
                && ui.small_button("Remove").clicked()
            {
                actions.push(document_mutation(
                    &target.key,
                    SceneInspectorCommand::RemoveField {
                        entity: target.key.entity.clone(),
                        component: target.key.component.clone(),
                        component_version: target.component_version,
                        field: target.key.field.clone(),
                    },
                ));
            }
        });

        if let SceneInspectorFieldState::InvalidPath(reason) = &field.state {
            ui.label(RichText::new(reason).weak());
        }
        let draft_error = self
            .field_drafts
            .get(&target.key)
            .and_then(|draft| draft.error.as_deref());
        if let Some(error) = draft_error {
            ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
        } else if let Some(feedback) = field_feedback
            && feedback.target().same_field(&target.key.target())
        {
            let summary = field_feedback_summary(feedback.reason());
            ui.label(RichText::new(summary).color(ui.visuals().error_fg_color));
            #[cfg(test)]
            crate::editor::record_test_inline_field_feedback(summary);
        }
    }

    fn show_bool_field(
        &mut self,
        ui: &mut Ui,
        target: &EguiInspectorFieldTarget,
        field: &SceneInspectorFieldView,
        editing_enabled: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        let mut value = field
            .value
            .as_ref()
            .or(field.default_value.as_ref())
            .and_then(ComponentValue::as_bool)
            .unwrap_or(false);
        let checkbox = ui.add_enabled(editing_enabled, egui::Checkbox::without_text(&mut value));
        if checkbox.changed() {
            self.field_drafts.remove(&target.key);
            actions.push(document_mutation(
                &target.key,
                SceneInspectorCommand::SetField {
                    entity: target.key.entity.clone(),
                    component: target.key.component.clone(),
                    component_version: target.component_version,
                    field: target.key.field.clone(),
                    value: ComponentValue::Bool(value),
                },
            ));
        }
    }

    fn show_text_field(
        &mut self,
        ui: &mut Ui,
        target: &EguiInspectorFieldTarget,
        field: &SceneInspectorFieldView,
        editing_enabled: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        let mut text = self
            .field_drafts
            .get(&target.key)
            .map_or_else(|| field_edit_source_text(field), |draft| draft.text.clone());
        let edit = ui.add_enabled(
            editing_enabled,
            TextEdit::singleline(&mut text)
                .id_salt(&target.key)
                .desired_width(160.0),
        );
        if edit.changed() {
            self.field_drafts.insert(
                target.key.clone(),
                FieldDraft {
                    text: text.clone(),
                    error: None,
                },
            );
        }
        let apply = {
            let response = ui.add_enabled(editing_enabled, Button::new("Apply"));
            #[cfg(test)]
            crate::editor::record_test_control(
                format!(
                    "field-apply:{}:{}:{}",
                    target.key.entity.as_str(),
                    target.key.component.as_str(),
                    target.key.field.as_str()
                ),
                response.rect,
            );
            response.clicked()
        };

        if !apply {
            return;
        }
        match parse_editable_value(field.value_kind, &text) {
            Ok(value) => {
                self.field_drafts.remove(&target.key);
                actions.push(document_mutation(
                    &target.key,
                    SceneInspectorCommand::SetField {
                        entity: target.key.entity.clone(),
                        component: target.key.component.clone(),
                        component_version: target.component_version,
                        field: target.key.field.clone(),
                        value,
                    },
                ));
            }
            Err(error) => {
                self.field_drafts.insert(
                    target.key.clone(),
                    FieldDraft {
                        text,
                        error: Some(error),
                    },
                );
                actions.push(EditorAction::ReportFieldValidation(
                    EditorFieldValidationFailure::new(target.key.target(), field.value_kind),
                ));
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn seed_test_draft(
        &mut self,
        scene: &EditorSceneModel,
        entity: SceneEntityId,
        component: ComponentTypeId,
        field: ComponentFieldId,
    ) {
        self.synchronize_scene(Some(scene));
        let key = EguiInspectorFieldKey::new(scene, &entity, &component, &field);
        self.field_drafts.insert(
            key,
            FieldDraft {
                text: "draft".to_owned(),
                error: Some("invalid".to_owned()),
            },
        );
    }

    #[cfg(test)]
    pub(crate) fn test_transient_entry_count(&self) -> usize {
        self.field_drafts.len()
    }
}

fn field_feedback_summary(reason: EditorFieldFeedbackReason) -> &'static str {
    match reason {
        EditorFieldFeedbackReason::InvalidValue(ComponentValueKind::Bool) => {
            "Expected true or false"
        }
        EditorFieldFeedbackReason::InvalidValue(ComponentValueKind::I64) => {
            "Expected a signed integer"
        }
        EditorFieldFeedbackReason::InvalidValue(ComponentValueKind::U64) => {
            "Expected an unsigned integer"
        }
        EditorFieldFeedbackReason::InvalidValue(ComponentValueKind::F64) => {
            "Expected a finite number"
        }
        EditorFieldFeedbackReason::InvalidValue(ComponentValueKind::String) => "Expected text",
        EditorFieldFeedbackReason::InvalidValue(
            ComponentValueKind::Null
            | ComponentValueKind::List
            | ComponentValueKind::Map
            | ComponentValueKind::AssetRef
            | ComponentValueKind::EntityRef,
        ) => "This field kind is not editable as text",
        EditorFieldFeedbackReason::StaleAction => "The field changed before this edit was applied",
        EditorFieldFeedbackReason::MutationRejected => {
            "The field edit was rejected; see Diagnostics"
        }
    }
}

fn document_mutation(key: &EguiInspectorFieldKey, command: SceneInspectorCommand) -> EditorAction {
    EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
        document: key.document,
        base_revision: key.revision,
        command,
    })
}
