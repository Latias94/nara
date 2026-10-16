use std::collections::HashMap;

use egui::{Button, CollapsingHeader, ComboBox, RichText, TextEdit, Ui, Window};
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind,
};
use nara_scene::{SceneAuthoringRevision, SceneEntityId, ScenePlacement, ScenePlacementOwner};
use nara_tooling::{
    EditorAction, EditorDocumentId, EditorDocumentMutation, EditorFieldFeedback,
    EditorFieldFeedbackReason, EditorFieldTarget, EditorFieldValidationFailure,
    EditorNavigationAction, EditorSceneModel, SceneCompositionCommand, SceneCompositionEntityKind,
    SceneInspectorCommand, SceneInspectorComponentView, SceneInspectorEntityRow,
    SceneInspectorFieldState, SceneInspectorFieldView,
};

use crate::fields::{
    component_title, field_edit_source_text, field_value_label, parse_asset_reference,
    parse_editable_value, value_kind_label,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InspectorSceneScope {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct EguiInspectorFieldKey {
    document: EditorDocumentId,
    revision: SceneAuthoringRevision,
    entity: SceneEntityId,
    component: ComponentTypeId,
    component_version: ComponentSchemaVersion,
    field: ComponentFieldId,
}

impl EguiInspectorFieldKey {
    fn new(
        scene: &EditorSceneModel,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
        component_version: ComponentSchemaVersion,
        field: &ComponentFieldId,
    ) -> Self {
        Self {
            document: scene.document,
            revision: scene.revision,
            entity: entity.clone(),
            component: component.clone(),
            component_version,
            field: field.clone(),
        }
    }

    fn target(&self) -> EditorFieldTarget {
        EditorFieldTarget::new(
            self.document,
            self.revision,
            self.entity.clone(),
            self.component.clone(),
            self.component_version,
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct PendingEntityRemoval {
    entity: SceneEntityId,
}

#[derive(Debug, Default)]
pub(crate) struct EguiSceneInspectorPanel {
    active_scope: Option<InspectorSceneScope>,
    field_drafts: HashMap<EguiInspectorFieldKey, FieldDraft>,
    pending_removal: Option<PendingEntityRemoval>,
}

impl EguiSceneInspectorPanel {
    pub(crate) fn synchronize_scene(&mut self, scene: Option<&EditorSceneModel>) {
        let next = scene.map(|scene| InspectorSceneScope {
            document: scene.document,
            revision: scene.revision,
        });
        if self.active_scope != next {
            self.active_scope = next;
            self.field_drafts.clear();
            self.pending_removal = None;
        }
    }

    pub(crate) fn show_tree(
        &mut self,
        ui: &mut Ui,
        scene: Option<&EditorSceneModel>,
        editing_enabled: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        let _heading = ui.heading("Document Tree");
        #[cfg(test)]
        crate::editor::record_test_control("Scene Tree heading", _heading.rect);
        let Some(scene) = scene else {
            ui.label(RichText::new("No active document").weak());
            return;
        };
        self.show_composition_toolbar(ui, scene, editing_enabled, actions);
        if scene.editor.inspector.entities.is_empty() {
            ui.label(RichText::new("No entities").weak());
            self.show_remove_confirmation(ui, scene, editing_enabled, actions);
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
        self.show_remove_confirmation(ui, scene, editing_enabled, actions);
    }

    fn show_composition_toolbar(
        &mut self,
        ui: &mut Ui,
        scene: &EditorSceneModel,
        editing_enabled: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        let selected = scene
            .editor
            .inspector
            .selected_entity
            .as_ref()
            .and_then(|entity| selected_tree_position(&scene.editor.inspector.entities, entity));
        ui.horizontal_wrapped(|ui| {
            let add_root = ui
                .add_enabled(editing_enabled, Button::new("+ Root"))
                .on_hover_text("Create an entity in this document");
            #[cfg(test)]
            crate::editor::record_test_control("Add Root", add_root.rect);
            if add_root.clicked() {
                actions.push(compose_action(
                    scene,
                    SceneCompositionCommand::CreateEntity {
                        kind: if scene.editor.inspector.composition.generic_authoring {
                            SceneCompositionEntityKind::Empty
                        } else {
                            SceneCompositionEntityKind::ColoredSprite
                        },
                        placement: ScenePlacement::last(ScenePlacementOwner::Root),
                    },
                ));
            }

            let add_child = ui
                .add_enabled(
                    editing_enabled && selected.is_some(),
                    Button::new("+ Child"),
                )
                .on_hover_text("Add a visible child to the selection");
            #[cfg(test)]
            crate::editor::record_test_control("Add Child", add_child.rect);
            if add_child.clicked() {
                let parent = selected
                    .expect("Add Child is enabled only with a selection")
                    .row
                    .id
                    .clone();
                actions.push(compose_action(
                    scene,
                    SceneCompositionCommand::CreateEntity {
                        kind: if scene.editor.inspector.composition.generic_authoring {
                            SceneCompositionEntityKind::Empty
                        } else {
                            SceneCompositionEntityKind::ColoredSprite
                        },
                        placement: ScenePlacement::last(ScenePlacementOwner::Parent { parent }),
                    },
                ));
            }

            let (previous, next) =
                selected.map_or((None, None), |selected| (selected.previous, selected.next));
            let move_up = ui
                .add_enabled(
                    editing_enabled
                        && !scene.editor.inspector.composition.generic_authoring
                        && previous.is_some(),
                    Button::new("Up"),
                )
                .on_hover_text(if scene.editor.inspector.composition.generic_authoring {
                    "Generic entity reparenting is not admitted"
                } else {
                    "Move before the previous sibling"
                });
            if move_up.clicked() {
                let entity = selected
                    .expect("Move Up is enabled only with a selection")
                    .row
                    .id
                    .clone();
                let previous = previous.expect("Move Up is enabled only with a previous sibling");
                actions.push(compose_action(
                    scene,
                    SceneCompositionCommand::MoveEntity {
                        entity: entity.clone(),
                        to: ScenePlacement::before(
                            placement_owner(scene, &entity),
                            previous.clone(),
                        ),
                    },
                ));
            }
            let move_down = ui
                .add_enabled(
                    editing_enabled
                        && !scene.editor.inspector.composition.generic_authoring
                        && next.is_some(),
                    Button::new("Down"),
                )
                .on_hover_text("Move after the next sibling");
            if move_down.clicked() {
                let entity = selected
                    .expect("Move Down is enabled only with a selection")
                    .row
                    .id
                    .clone();
                let next = next.expect("Move Down is enabled only with a next sibling");
                actions.push(compose_action(
                    scene,
                    SceneCompositionCommand::MoveEntity {
                        entity: entity.clone(),
                        to: ScenePlacement::after(placement_owner(scene, &entity), next.clone()),
                    },
                ));
            }

            let remove = ui
                .add_enabled(
                    editing_enabled
                        && !scene.editor.inspector.composition.generic_authoring
                        && selected.is_some(),
                    Button::new("Remove"),
                )
                .on_hover_text(if scene.editor.inspector.composition.generic_authoring {
                    "Generic entity deletion is not admitted"
                } else {
                    "Remove the selected entity subtree"
                });
            #[cfg(test)]
            crate::editor::record_test_control("Remove Entity", remove.rect);
            if remove.clicked() {
                let entity = selected
                    .expect("Remove is enabled only with a selection")
                    .row
                    .id
                    .clone();
                self.pending_removal = Some(PendingEntityRemoval { entity });
            }
        });

        let Some(selected) = selected else {
            return;
        };
        if scene.editor.inspector.composition.generic_authoring {
            ui.label(
                RichText::new(
                    "Entity deletion and reparenting are unavailable for generic authoring",
                )
                .weak(),
            );
            return;
        }
        let selected_id = &selected.row.id;
        let current_parent = selected.row.parent.as_ref();
        let selected_text = current_parent.map_or("Root", SceneEntityId::as_str);
        ComboBox::from_id_salt(("nara_editor_parent", selected_id.as_str()))
            .selected_text(format!("Parent: {selected_text}"))
            .show_ui(ui, |ui| {
                if ui
                    .add_enabled(
                        editing_enabled && current_parent.is_some(),
                        Button::new("Root"),
                    )
                    .clicked()
                {
                    actions.push(compose_action(
                        scene,
                        SceneCompositionCommand::MoveEntity {
                            entity: selected_id.clone(),
                            to: ScenePlacement::last(ScenePlacementOwner::Root),
                        },
                    ));
                }
                for (index, candidate) in scene.editor.inspector.entities.iter().enumerate() {
                    let eligible = editing_enabled
                        && index != selected.index
                        && current_parent != Some(&candidate.id)
                        && !selected.is_descendant_index(index);
                    if ui
                        .add_enabled(eligible, Button::new(candidate.id.as_str()))
                        .clicked()
                    {
                        actions.push(compose_action(
                            scene,
                            SceneCompositionCommand::MoveEntity {
                                entity: selected_id.clone(),
                                to: ScenePlacement::last(ScenePlacementOwner::Parent {
                                    parent: candidate.id.clone(),
                                }),
                            },
                        ));
                    }
                }
            });
    }

    fn show_remove_confirmation(
        &mut self,
        ui: &mut Ui,
        scene: &EditorSceneModel,
        editing_enabled: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        let Some(pending) = self.pending_removal.clone() else {
            return;
        };
        let Some(position) =
            selected_tree_position(&scene.editor.inspector.entities, &pending.entity)
        else {
            self.pending_removal = None;
            return;
        };

        Window::new("Remove entity")
            .collapsible(false)
            .resizable(false)
            .show(ui.ctx(), |ui| {
                ui.label(format!(
                    "Remove {} and {} affected entity(s)?",
                    pending.entity.as_str(),
                    position.subtree_end - position.index
                ));
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(editing_enabled, Button::new("Remove"))
                        .clicked()
                    {
                        actions.push(compose_action(
                            scene,
                            SceneCompositionCommand::RemoveEntity {
                                entity: pending.entity.clone(),
                            },
                        ));
                        self.pending_removal = None;
                    }
                    if ui.button("Cancel").clicked() {
                        self.pending_removal = None;
                    }
                });
            });
    }

    fn show_component_toolbar(
        &self,
        ui: &mut Ui,
        scene: &EditorSceneModel,
        view: &nara_tooling::SceneInspectorEntityView,
        editing_enabled: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        ui.horizontal_wrapped(|ui| {
            for capability in &scene.editor.inspector.composition.components {
                let present = view
                    .components
                    .iter()
                    .any(|component| component.component == capability.component);
                let label = match capability.component.as_str() {
                    "nara.transform.Transform2d" => "+ Transform".to_owned(),
                    "nara.sprite.Sprite" => "+ Sprite".to_owned(),
                    id => format!("+ {id}"),
                };
                let response = ui
                    .add_enabled(
                        editing_enabled && !present && capability.add_unavailable_reason.is_none(),
                        Button::new(&label),
                    )
                    .on_hover_text(capability.add_unavailable_reason.unwrap_or(if present {
                        "Component already exists"
                    } else {
                        "Copy this component's complete creation template"
                    }));
                #[cfg(test)]
                crate::editor::record_test_control(&label, response.rect);
                if response.clicked() {
                    actions.push(compose_action(
                        scene,
                        SceneCompositionCommand::AddComponent {
                            entity: view.id.clone(),
                            component: capability.component.clone(),
                        },
                    ));
                }
            }
        });
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
            ui.label(RichText::new("No active document").weak());
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
            let open = ui
                .add_enabled(editing_enabled, Button::new("Open Prefab Source"))
                .on_hover_text("Open the exact Prefab source document");
            #[cfg(test)]
            crate::editor::record_test_control("Open Prefab Source", open.rect);
            if open.clicked() {
                actions.push(EditorAction::Navigate(
                    EditorNavigationAction::OpenPrefabSource {
                        document: scene.document,
                        base_revision: scene.revision,
                        entity: view.id.clone(),
                    },
                ));
            }
        }
        self.show_component_toolbar(ui, scene, view, editing_enabled, actions);
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
                let removable = component.access.is_editable()
                    && component.document_version == component.schema_version
                    && scene
                        .editor
                        .inspector
                        .composition
                        .components
                        .iter()
                        .any(|capability| {
                            capability.component == component.component && capability.removable
                        });
                if ui
                    .add_enabled(
                        editing_enabled && removable,
                        Button::new("Remove Component"),
                    )
                    .on_hover_text(if removable {
                        "Remove this current data-editable component"
                    } else {
                        "Unavailable or unsupported records cannot be removed"
                    })
                    .clicked()
                {
                    actions.push(compose_action(
                        scene,
                        SceneCompositionCommand::RemoveComponent {
                            entity: entity.clone(),
                            component: component.component.clone(),
                        },
                    ));
                }
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
                            component.schema_version,
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
            let field_label = ui
                .label(field_label)
                .on_hover_text(format!("kind: {}", value_kind_label(field.value_kind)));

            match field.value_kind {
                ComponentValueKind::Bool => self.show_bool_field(
                    ui,
                    target,
                    field,
                    field_label.id,
                    field_editing_enabled,
                    actions,
                ),
                ComponentValueKind::I64
                | ComponentValueKind::U64
                | ComponentValueKind::F64
                | ComponentValueKind::String
                | ComponentValueKind::AssetRef => {
                    self.show_text_field(ui, target, field, field_editing_enabled, actions)
                }
                ComponentValueKind::Null
                | ComponentValueKind::List
                | ComponentValueKind::Map
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
        label_id: egui::Id,
        editing_enabled: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        let mut value = field
            .value
            .as_ref()
            .or(field.default_value.as_ref())
            .and_then(ComponentValue::as_bool)
            .unwrap_or(false);
        let checkbox = ui
            .add_enabled(editing_enabled, egui::Checkbox::without_text(&mut value))
            .labelled_by(label_id);
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
        edit.ctx.accesskit_node_builder(edit.id, |builder| {
            builder.set_label(field.path.to_string());
        });
        if edit.changed() {
            self.field_drafts.insert(
                target.key.clone(),
                FieldDraft {
                    text: text.clone(),
                    error: None,
                },
            );
        }
        let apply = edit.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)) || {
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
        let command = if field.value_kind == ComponentValueKind::AssetRef {
            parse_asset_reference(&text).map(|asset_ref| SceneInspectorCommand::SetAssetRefField {
                entity: target.key.entity.clone(),
                component: target.key.component.clone(),
                component_version: target.component_version,
                field: target.key.field.clone(),
                asset_ref,
            })
        } else {
            parse_editable_value(field.value_kind, &text).map(|value| {
                SceneInspectorCommand::SetField {
                    entity: target.key.entity.clone(),
                    component: target.key.component.clone(),
                    component_version: target.component_version,
                    field: target.key.field.clone(),
                    value,
                }
            })
        };
        match command {
            Ok(command) => {
                self.field_drafts.remove(&target.key);
                actions.push(document_mutation(&target.key, command));
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
        let key = EguiInspectorFieldKey::new(
            scene,
            &entity,
            &component,
            ComponentSchemaVersion::ONE,
            &field,
        );
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

fn compose_action(scene: &EditorSceneModel, command: SceneCompositionCommand) -> EditorAction {
    if scene.kind == nara_tooling::EditorDocumentKind::Prefab {
        let command = match command {
            SceneCompositionCommand::CreateEntity { placement, .. } => {
                nara_tooling::PrefabCompositionCommand::CreateEntity {
                    draft: nara_tooling::PrefabCompositionEntityDraft::new(Default::default())
                        .expect("empty draft is within the composition envelope"),
                    placement,
                }
            }
            SceneCompositionCommand::AddComponent { entity, component } => {
                nara_tooling::PrefabCompositionCommand::AddComponent { entity, component }
            }
            SceneCompositionCommand::RemoveComponent { entity, component } => {
                nara_tooling::PrefabCompositionCommand::RemoveComponent { entity, component }
            }
            SceneCompositionCommand::MoveEntity { entity, to } => {
                nara_tooling::PrefabCompositionCommand::MoveEntity { entity, to }
            }
            SceneCompositionCommand::RemoveEntity { entity } => {
                nara_tooling::PrefabCompositionCommand::RemoveEntity { entity }
            }
        };
        return EditorAction::Mutate(EditorDocumentMutation::ComposePrefab {
            document: scene.document,
            base_revision: scene.revision,
            command,
        });
    }
    EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
        document: scene.document,
        base_revision: scene.revision,
        command,
    })
}

fn placement_owner(scene: &EditorSceneModel, entity: &SceneEntityId) -> ScenePlacementOwner {
    scene
        .editor
        .inspector
        .entities
        .iter()
        .find(|row| &row.id == entity)
        .and_then(|row| row.parent.clone())
        .map_or(ScenePlacementOwner::Root, |parent| {
            ScenePlacementOwner::Parent { parent }
        })
}

#[derive(Clone, Copy)]
struct SelectedTreePosition<'rows> {
    index: usize,
    subtree_end: usize,
    row: &'rows SceneInspectorEntityRow,
    previous: Option<&'rows SceneEntityId>,
    next: Option<&'rows SceneEntityId>,
}

impl SelectedTreePosition<'_> {
    fn is_descendant_index(self, index: usize) -> bool {
        (self.index + 1..self.subtree_end).contains(&index)
    }
}

fn selected_tree_position<'rows>(
    rows: &'rows [SceneInspectorEntityRow],
    selected: &SceneEntityId,
) -> Option<SelectedTreePosition<'rows>> {
    let index = rows.iter().position(|row| &row.id == selected)?;
    let row = &rows[index];
    let subtree_end = rows[index + 1..]
        .iter()
        .position(|candidate| candidate.depth <= row.depth)
        .map_or(rows.len(), |offset| index + 1 + offset);
    let previous = rows[..index]
        .iter()
        .rev()
        .find(|candidate| candidate.parent == row.parent)
        .map(|candidate| &candidate.id);
    let next = rows
        .get(subtree_end)
        .filter(|candidate| candidate.parent == row.parent)
        .map(|candidate| &candidate.id);
    Some(SelectedTreePosition {
        index,
        subtree_end,
        row,
        previous,
        next,
    })
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
        EditorFieldFeedbackReason::InvalidValue(ComponentValueKind::AssetRef) => {
            "Expected a logical asset path or stable_id:<UUID>"
        }
        EditorFieldFeedbackReason::InvalidValue(
            ComponentValueKind::Null
            | ComponentValueKind::List
            | ComponentValueKind::Map
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
