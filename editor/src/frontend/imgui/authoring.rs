use std::collections::HashMap;

use dear_imgui_rs::{Key, TreeNodeFlags, Ui};
use nara::{
    asset::AssetRef,
    reflect::{ComponentCapability, ComponentValue, ComponentValueKind},
    scene::{SceneAuthoringRevision, SceneEntityId, ScenePlacement, ScenePlacementOwner},
    tooling::{
        EditorAction, EditorDocumentId, EditorDocumentKind, EditorDocumentMutation,
        EditorFieldFeedbackReason, EditorFieldTarget, EditorFieldValidationFailure,
        EditorFrameModel, EditorNavigationAction, EditorPersistenceAction,
        EditorPersistenceOperation, EditorPersistenceRequestKind, EditorPersistenceResult,
        EditorPlayState, EditorReopenTarget, EditorRuntimeAvailability, EditorSceneModel,
        EditorWorkspaceIntent, EditorWorkspaceIntentAction, PrefabCompositionCommand,
        PrefabCompositionEntityDraft, SceneCompositionCommand, SceneCompositionEntityKind,
        SceneInspectorCommand, SceneInspectorComponentView, SceneInspectorEntityRow,
        SceneInspectorFieldState, SceneInspectorFieldView,
    },
};

use super::{ImguiTextOwner, observe_text_owner};

mod recursive_controls;
mod value_draft;

#[derive(Default)]
pub(super) struct ImguiAuthoringPanels {
    scope: Option<(
        EditorDocumentId,
        SceneAuthoringRevision,
        Option<SceneEntityId>,
    )>,
    drafts: HashMap<EditorFieldTarget, FieldDraft>,
    structured_drafts: HashMap<EditorFieldTarget, value_draft::RootDraft>,
    pending_removal: Option<SceneEntityId>,
    read_only: bool,
}

struct FieldDraft {
    text: String,
    error: Option<String>,
}

impl ImguiAuthoringPanels {
    pub(super) fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub(super) fn retire(&mut self) {
        self.scope = None;
        self.drafts.clear();
        self.structured_drafts.clear();
        self.pending_removal = None;
    }

    fn synchronize(&mut self, scene: Option<&EditorSceneModel>) {
        let scope = scene.map(|scene| {
            (
                scene.document,
                scene.revision,
                scene.editor.inspector.selected_entity.clone(),
            )
        });
        if self.scope != scope {
            self.retire();
            self.scope = scope;
        }
    }

    pub(super) fn show_document_controls(
        &mut self,
        ui: &Ui,
        frame: &EditorFrameModel,
        actions: &mut Vec<EditorAction>,
    ) {
        self.synchronize(frame.workspace().active_scene.as_ref());
        let document = frame
            .workspace()
            .active_document
            .and_then(|id| frame.workspace().document(id));
        let editing = !self.read_only && editing_enabled(frame);
        let title = document.map_or("No active document", |document| document.title.as_str());
        if let Some(_combo) = ui.begin_combo("Document", title) {
            for tab in &frame.workspace().documents {
                let _id = ui.push_id(format!("document:{}", tab.document.raw()).as_str());
                if ui
                    .selectable_config(format!(
                        "{}{}",
                        tab.title,
                        if tab.dirty { " *" } else { "" }
                    ))
                    .selected(tab.active)
                    .disabled(self.read_only || frame.workspace_intent().intent().is_some())
                    .build()
                    && !tab.active
                {
                    actions.push(EditorAction::Navigate(
                        EditorNavigationAction::SetActiveDocument {
                            document: tab.document,
                        },
                    ));
                }
            }
        }
        if document.is_some_and(|document| document.kind == EditorDocumentKind::Prefab)
            && let Some(scene) = frame
                .workspace()
                .documents
                .iter()
                .find(|document| document.kind == EditorDocumentKind::Scene)
        {
            ui.same_line();
            if enabled_button(ui, !self.read_only, "Back to Scene") {
                actions.push(EditorAction::Navigate(
                    EditorNavigationAction::SetActiveDocument {
                        document: scene.document,
                    },
                ));
            }
        }
        if let Some(document) = document {
            ui.text(format!(
                "{}{}",
                document.title,
                if document.dirty { " *" } else { "" }
            ));
            if enabled_button(ui, editing && document.history.undo_depth > 0, "Undo") {
                actions.push(EditorAction::Mutate(EditorDocumentMutation::Undo {
                    document: document.document,
                    base_revision: document.revision,
                }));
            }
            ui.same_line();
            if enabled_button(ui, editing && document.history.redo_depth > 0, "Redo") {
                actions.push(EditorAction::Mutate(EditorDocumentMutation::Redo {
                    document: document.document,
                    base_revision: document.revision,
                }));
            }
            ui.same_line();
            let persistence = frame.persistence();
            let idle = !self.read_only
                && frame.workspace_intent().intent().is_none()
                && persistence.operation() == EditorPersistenceOperation::Idle
                && (document.kind == EditorDocumentKind::Prefab
                    || frame.play().state() == EditorPlayState::Empty
                    || matches!(
                        frame.runtime_availability(),
                        EditorRuntimeAvailability::ExternalSupervised
                    ));
            let save_allowed = persistence_allows(
                persistence.result(),
                EditorPersistenceRequestKind::Save,
                document.document,
            );
            if enabled_button(ui, idle && save_allowed && document.dirty, "Save") {
                actions.push(EditorAction::Persistence(EditorPersistenceAction::Save {
                    document: document.document,
                    base_revision: document.revision,
                }));
            }
            ui.same_line();
            let reopen_allowed = match frame.reopen_target() {
                EditorReopenTarget::OpenDocument { document, .. } => persistence_allows(
                    persistence.result(),
                    EditorPersistenceRequestKind::Reopen,
                    document,
                ),
                EditorReopenTarget::Unavailable => false,
            };
            if enabled_button(ui, idle && reopen_allowed, "Reopen") {
                actions.push(EditorAction::Persistence(EditorPersistenceAction::Reopen {
                    target: frame.reopen_target(),
                }));
            }
            ui.same_line();
            if enabled_button(
                ui,
                !self.read_only && frame.workspace_intent().intent().is_none(),
                "Close Document",
            ) {
                actions.push(EditorAction::WorkspaceIntent(
                    EditorWorkspaceIntentAction::Request(EditorWorkspaceIntent::CloseDocument {
                        document: document.document,
                    }),
                ));
            }
        } else {
            ui.text_disabled("No active document");
        }
        if let Some(result) = frame.persistence().result() {
            match result {
                EditorPersistenceResult::Saved { .. } => ui.text("Saved"),
                EditorPersistenceResult::Opened { .. } => ui.text("Reopened"),
                EditorPersistenceResult::Rejected { reason, .. } => {
                    ui.text_wrapped(format!("Persistence rejected: {reason:?}"));
                }
                EditorPersistenceResult::Failed { stage, .. } => {
                    ui.text_wrapped(format!("Persistence failed: {stage:?}"));
                }
                EditorPersistenceResult::PersistenceUncertain { .. } => {
                    ui.text_wrapped("Saving could not be confirmed. Reopen this document to reconcile its source.");
                }
            }
            if enabled_button(
                ui,
                frame.persistence().operation() == EditorPersistenceOperation::Idle,
                "Dismiss persistence result",
            ) {
                actions.push(EditorAction::Persistence(
                    EditorPersistenceAction::AcknowledgeResult,
                ));
            }
        }
    }

    pub(super) fn show_hierarchy(
        &mut self,
        ui: &Ui,
        frame: &EditorFrameModel,
        actions: &mut Vec<EditorAction>,
    ) {
        self.synchronize(frame.workspace().active_scene.as_ref());
        let Some(scene) = frame.workspace().active_scene.as_ref() else {
            ui.text_disabled("No active document");
            return;
        };
        let editing = !self.read_only && editing_enabled(frame);
        self.show_composition_controls(ui, scene, editing, actions);
        for entity in &scene.editor.inspector.entities {
            let _id = ui.push_id(entity.id.as_str());
            let indent = entity.depth as f32 * 16.0;
            if indent > 0.0 {
                ui.indent_by(indent);
            }
            if ui
                .selectable_config(format!(
                    "{} ({} components{})",
                    entity.id.as_str(),
                    entity.inspectable_component_count,
                    if entity.has_prefab { ", prefab" } else { "" }
                ))
                .selected(entity.selected)
                .disabled(self.read_only || frame.workspace_intent().intent().is_some())
                .build()
            {
                actions.push(EditorAction::Navigate(
                    EditorNavigationAction::SelectEntity {
                        document: scene.document,
                        entity: Some(entity.id.clone()),
                    },
                ));
            }
            #[cfg(test)]
            super::interaction_tests::record_control(ui, format!("entity:{}", entity.id.as_str()));
            if indent > 0.0 {
                ui.unindent_by(indent);
            }
        }
        if scene.editor.inspector.entities.is_empty() {
            ui.text_disabled("No entities");
        }
        self.show_remove_confirmation(ui, scene, editing, actions);
    }

    fn show_composition_controls(
        &mut self,
        ui: &Ui,
        scene: &EditorSceneModel,
        editing: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        let inspector = &scene.editor.inspector;
        let selected = inspector
            .selected_entity
            .as_ref()
            .and_then(|id| tree_position(&inspector.entities, id));
        let kind = if inspector.composition.generic_authoring {
            SceneCompositionEntityKind::Empty
        } else {
            SceneCompositionEntityKind::ColoredSprite
        };
        if enabled_button(ui, editing, "+ Root") {
            actions.push(compose_action(
                scene,
                SceneCompositionCommand::CreateEntity {
                    kind,
                    placement: ScenePlacement::last(ScenePlacementOwner::Root),
                },
            ));
        }
        ui.same_line();
        if enabled_button(ui, editing && selected.is_some(), "+ Child") {
            if let Some(selected) = selected {
                actions.push(compose_action(
                    scene,
                    SceneCompositionCommand::CreateEntity {
                        kind,
                        placement: ScenePlacement::last(ScenePlacementOwner::Parent {
                            parent: selected.row.id.clone(),
                        }),
                    },
                ));
            }
        }
        let Some(selected) = selected else {
            return;
        };
        let structural_editing = editing && !inspector.composition.generic_authoring;
        let owner = selected
            .row
            .parent
            .clone()
            .map_or(ScenePlacementOwner::Root, |parent| {
                ScenePlacementOwner::Parent { parent }
            });
        if enabled_button(ui, structural_editing && selected.previous.is_some(), "Up") {
            if let Some(previous) = selected.previous {
                actions.push(compose_action(
                    scene,
                    SceneCompositionCommand::MoveEntity {
                        entity: selected.row.id.clone(),
                        to: ScenePlacement::before(owner.clone(), previous.clone()),
                    },
                ));
            }
        }
        ui.same_line();
        if enabled_button(ui, structural_editing && selected.next.is_some(), "Down") {
            if let Some(next) = selected.next {
                actions.push(compose_action(
                    scene,
                    SceneCompositionCommand::MoveEntity {
                        entity: selected.row.id.clone(),
                        to: ScenePlacement::after(owner, next.clone()),
                    },
                ));
            }
        }
        ui.same_line();
        if enabled_button(ui, structural_editing, "Remove Entity") {
            self.pending_removal = Some(selected.row.id.clone());
        }
        if inspector.composition.generic_authoring {
            ui.text_disabled(
                "Entity deletion and reparenting are unavailable for generic authoring",
            );
            return;
        }
        let parent_label = selected
            .row
            .parent
            .as_ref()
            .map_or("Root", SceneEntityId::as_str);
        if let Some(_combo) = ui.begin_combo("Parent", parent_label) {
            if enabled_button(ui, editing && selected.row.parent.is_some(), "Root") {
                actions.push(compose_action(
                    scene,
                    SceneCompositionCommand::MoveEntity {
                        entity: selected.row.id.clone(),
                        to: ScenePlacement::last(ScenePlacementOwner::Root),
                    },
                ));
            }
            for (index, candidate) in inspector.entities.iter().enumerate() {
                let eligible = editing
                    && index != selected.index
                    && !(selected.index + 1..selected.subtree_end).contains(&index)
                    && selected.row.parent.as_ref() != Some(&candidate.id);
                if enabled_button(ui, eligible, candidate.id.as_str()) {
                    actions.push(compose_action(
                        scene,
                        SceneCompositionCommand::MoveEntity {
                            entity: selected.row.id.clone(),
                            to: ScenePlacement::last(ScenePlacementOwner::Parent {
                                parent: candidate.id.clone(),
                            }),
                        },
                    ));
                }
            }
        }
    }

    fn show_remove_confirmation(
        &mut self,
        ui: &Ui,
        scene: &EditorSceneModel,
        editing: bool,
        actions: &mut Vec<EditorAction>,
    ) {
        let Some(entity) = self.pending_removal.clone() else {
            return;
        };
        let Some(selected) = tree_position(&scene.editor.inspector.entities, &entity) else {
            self.pending_removal = None;
            return;
        };
        ui.separator();
        ui.text_wrapped(format!(
            "Remove {} and {} affected entity(s)?",
            entity.as_str(),
            selected.subtree_end - selected.index
        ));
        if enabled_button(ui, editing, "Confirm removal") {
            actions.push(compose_action(
                scene,
                SceneCompositionCommand::RemoveEntity { entity },
            ));
            self.pending_removal = None;
        }
        ui.same_line();
        if ui.button("Cancel removal") {
            self.pending_removal = None;
        }
    }

    pub(super) fn show_inspector(
        &mut self,
        ui: &Ui,
        frame: &EditorFrameModel,
        actions: &mut Vec<EditorAction>,
        text_owner: &mut Option<ImguiTextOwner>,
    ) {
        self.synchronize(frame.workspace().active_scene.as_ref());
        let Some(scene) = frame.workspace().active_scene.as_ref() else {
            ui.text_disabled("No active document");
            return;
        };
        let Some(entity) = scene.editor.inspector.selected_entity_view.as_ref() else {
            ui.text_disabled("Select an entity");
            return;
        };
        let scope_id = format!(
            "document:{}:{:?}:entity:{}",
            scene.document.raw(),
            scene.revision,
            entity.id.as_str()
        );
        let _scope = ui.push_id(scope_id.as_str());
        let editing = !self.read_only && editing_enabled(frame);
        ui.text(entity.id.as_str());
        if let Some(parent) = &entity.parent {
            ui.text(format!("Parent: {}", parent.as_str()));
        }
        if entity.has_prefab && enabled_button(ui, editing, "Open Prefab Source") {
            actions.push(EditorAction::Navigate(
                EditorNavigationAction::OpenPrefabSource {
                    document: scene.document,
                    base_revision: scene.revision,
                    entity: entity.id.clone(),
                },
            ));
        }
        let component_combo = ui.begin_combo("Add Component", "Choose a component");
        #[cfg(test)]
        super::interaction_tests::record_control(ui, "Add Component");
        if let Some(_combo) = component_combo {
            for capability in &scene.editor.inspector.composition.components {
                let present = entity
                    .components
                    .iter()
                    .any(|component| component.component == capability.component);
                if enabled_button(
                    ui,
                    editing && !present && capability.add_unavailable_reason.is_none(),
                    capability.component.as_str(),
                ) {
                    actions.push(compose_action(
                        scene,
                        SceneCompositionCommand::AddComponent {
                            entity: entity.id.clone(),
                            component: capability.component.clone(),
                        },
                    ));
                }
                if let Some(reason) = capability.add_unavailable_reason {
                    ui.text_disabled(reason);
                } else if present {
                    ui.text_disabled("Component already exists");
                }
            }
        }
        for component in &entity.components {
            self.show_component(
                ui, frame, scene, &entity.id, component, editing, actions, text_owner,
            );
        }
        if entity.components.is_empty() {
            ui.text_disabled("No components");
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn show_component(
        &mut self,
        ui: &Ui,
        frame: &EditorFrameModel,
        scene: &EditorSceneModel,
        entity: &SceneEntityId,
        component: &SceneInspectorComponentView,
        editing: bool,
        actions: &mut Vec<EditorAction>,
        text_owner: &mut Option<ImguiTextOwner>,
    ) {
        let component_id = format!(
            "{}:{:?}",
            component.component.as_str(),
            component.schema_version
        );
        let _id = ui.push_id(component_id.as_str());
        if !ui.collapsing_header(component.component.as_str(), TreeNodeFlags::DEFAULT_OPEN) {
            return;
        }
        if let Some(reason) = component.access.read_only_reason() {
            ui.text_disabled(format!("Read-only component: {reason:?}"));
        }
        let removable = editing
            && component.access.is_editable()
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
        if enabled_button(ui, removable, "Remove Component") {
            actions.push(compose_action(
                scene,
                SceneCompositionCommand::RemoveComponent {
                    entity: entity.clone(),
                    component: component.component.clone(),
                },
            ));
        }
        for field in &component.fields {
            let target = EditorFieldTarget::new(
                scene.document,
                scene.revision,
                entity.clone(),
                component.component.clone(),
                component.schema_version,
                field.id.clone(),
            );
            let editable = editing
                && component.access.is_editable()
                && field.capabilities.contains(&ComponentCapability::Edit);
            self.show_field(ui, frame, target, field, editable, actions, text_owner);
        }
        if component.fields.is_empty() {
            ui.text_disabled("No inspectable fields");
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn show_field(
        &mut self,
        ui: &Ui,
        frame: &EditorFrameModel,
        target: EditorFieldTarget,
        field: &SceneInspectorFieldView,
        editable: bool,
        actions: &mut Vec<EditorAction>,
        text_owner: &mut Option<ImguiTextOwner>,
    ) {
        let _id = ui.push_id(field.id.as_str());
        let supported = value_draft::supported(&field.shape);
        let editable = editable && supported;
        ui.text(format!(
            "{}{}",
            field.path,
            if field.required { " *" } else { "" }
        ));
        if !matches!(
            field.shape,
            nara::reflect::ComponentValueShape::Scalar { .. }
                | nara::reflect::ComponentValueShape::AssetRef
        ) && supported
        {
            let draft = self.structured_drafts.entry(target.clone()).or_default();
            match recursive_controls::show_root(
                ui,
                &target,
                &field.shape,
                field.value.as_ref().or(field.default_value.as_ref()),
                draft,
                editable,
                text_owner,
            ) {
                Some(Ok(value)) => actions.push(field_action(
                    &target,
                    SceneInspectorCommand::SetField {
                        entity: target.entity().clone(),
                        component: target.component().clone(),
                        component_version: target.component_version(),
                        field: target.field().clone(),
                        value,
                    },
                )),
                Some(Err(())) => actions.push(EditorAction::ReportFieldValidation(
                    EditorFieldValidationFailure::new(target.clone(), field.value_kind),
                )),
                None => {}
            }
        } else {
            match field.value_kind {
                ComponentValueKind::Bool => {
                    let mut value = field
                        .value
                        .as_ref()
                        .or(field.default_value.as_ref())
                        .and_then(ComponentValue::as_bool)
                        .unwrap_or(false);
                    let changed =
                        ui.with_disabled_if(!editable, || ui.checkbox("##value", &mut value));
                    if changed {
                        self.drafts.remove(&target);
                        actions.push(field_action(
                            &target,
                            SceneInspectorCommand::SetField {
                                entity: target.entity().clone(),
                                component: target.component().clone(),
                                component_version: target.component_version(),
                                field: target.field().clone(),
                                value: ComponentValue::Bool(value),
                            },
                        ));
                    }
                }
                ComponentValueKind::I64
                | ComponentValueKind::U64
                | ComponentValueKind::F64
                | ComponentValueKind::String
                | ComponentValueKind::AssetRef => {
                    self.show_text_field(ui, &target, field, editable, actions, text_owner);
                }
                ComponentValueKind::Null
                | ComponentValueKind::List
                | ComponentValueKind::Map
                | ComponentValueKind::EntityRef => {
                    ui.text_wrapped(value_label(
                        field.value.as_ref().or(field.default_value.as_ref()),
                    ));
                    ui.text_disabled("This field kind is read-only in the generic Inspector");
                }
            }
        }
        if enabled_button(
            ui,
            editable && field.value.is_some() && !field.required,
            "Remove field",
        ) {
            actions.push(field_action(
                &target,
                SceneInspectorCommand::RemoveField {
                    entity: target.entity().clone(),
                    component: target.component().clone(),
                    component_version: target.component_version(),
                    field: target.field().clone(),
                },
            ));
        }
        if let SceneInspectorFieldState::InvalidPath(reason) = &field.state {
            ui.text_wrapped(reason);
        }
        if let Some(error) = self
            .drafts
            .get(&target)
            .and_then(|draft| draft.error.as_deref())
            .or_else(|| {
                self.structured_drafts
                    .get(&target)
                    .and_then(|draft| draft.error.as_deref())
            })
        {
            ui.text_colored([1.0, 0.4, 0.3, 1.0], error);
        } else if let Some(feedback) = frame.field_feedback()
            && feedback.target().same_field(&target)
        {
            ui.text_colored(
                [1.0, 0.4, 0.3, 1.0],
                field_feedback_label(feedback.reason()),
            );
        }
        ui.separator();
    }

    #[allow(clippy::too_many_arguments)]
    fn show_text_field(
        &mut self,
        ui: &Ui,
        target: &EditorFieldTarget,
        field: &SceneInspectorFieldView,
        editable: bool,
        actions: &mut Vec<EditorAction>,
        text_owner: &mut Option<ImguiTextOwner>,
    ) {
        let mut text = self
            .drafts
            .get(target)
            .map_or_else(|| field_source_text(field), |draft| draft.text.clone());
        let before = text.clone();
        let enter = ui.with_disabled_if(!editable, || {
            ui.input_text("##value", &mut text)
                .enter_returns_true(true)
                .build()
        });
        #[cfg(test)]
        super::interaction_tests::record_control(
            ui,
            format!(
                "field:{}:{}",
                target.component().as_str(),
                target.field().as_str()
            ),
        );
        observe_text_owner(ui, text_owner);
        let cancel = ui.is_item_active() && ui.is_key_pressed(Key::Escape);
        if cancel {
            self.drafts.remove(target);
            return;
        }
        if text != before {
            self.drafts.insert(
                target.clone(),
                FieldDraft {
                    text: text.clone(),
                    error: None,
                },
            );
        }
        ui.same_line();
        let apply = enabled_button(ui, editable, "Apply");
        if (enter || apply) && editable {
            match parse_field_command(target, field.value_kind, &text) {
                Ok(command) => {
                    self.drafts.remove(target);
                    actions.push(field_action(target, command));
                }
                Err(error) => {
                    self.drafts.insert(
                        target.clone(),
                        FieldDraft {
                            text,
                            error: Some(error),
                        },
                    );
                    actions.push(EditorAction::ReportFieldValidation(
                        EditorFieldValidationFailure::new(target.clone(), field.value_kind),
                    ));
                }
            }
        }
    }
}

fn enabled_button(ui: &Ui, enabled: bool, label: &str) -> bool {
    let clicked = ui.with_disabled_if(!enabled, || ui.button(label));
    #[cfg(test)]
    if matches!(
        label,
        "+ Root"
            | "Undo"
            | "Redo"
            | "Save"
            | "Reopen"
            | "Dismiss persistence result"
            | "Open Prefab Source"
            | "Remove Component"
            | "Apply"
    ) || label == super::interaction_tests::CUSTOM_COMPONENT
    {
        super::interaction_tests::record_control(ui, label);
    }
    clicked
}

fn editing_enabled(frame: &EditorFrameModel) -> bool {
    (matches!(
        frame.runtime_availability(),
        EditorRuntimeAvailability::ExternalSupervised
    ) || matches!(
        frame.play().state(),
        EditorPlayState::Empty | EditorPlayState::Paused | EditorPlayState::Faulted
    )) && frame.workspace_intent().intent().is_none()
        && frame.persistence().operation() == EditorPersistenceOperation::Idle
}

pub(super) fn persistence_allows(
    result: Option<EditorPersistenceResult>,
    request: EditorPersistenceRequestKind,
    document: EditorDocumentId,
) -> bool {
    match result {
        None => true,
        Some(EditorPersistenceResult::Rejected {
            request: failed,
            document: target,
            ..
        })
        | Some(EditorPersistenceResult::Failed {
            request: failed,
            document: target,
            ..
        }) => failed == request && target.is_none_or(|target| target == document),
        Some(EditorPersistenceResult::PersistenceUncertain {
            document: target, ..
        }) => request == EditorPersistenceRequestKind::Reopen && target == document,
        Some(EditorPersistenceResult::Saved { .. } | EditorPersistenceResult::Opened { .. }) => {
            false
        }
    }
}

fn compose_action(scene: &EditorSceneModel, command: SceneCompositionCommand) -> EditorAction {
    if scene.kind == EditorDocumentKind::Prefab {
        let command = match command {
            SceneCompositionCommand::CreateEntity { placement, .. } => {
                PrefabCompositionCommand::CreateEntity {
                    draft: PrefabCompositionEntityDraft::new(Default::default())
                        .expect("empty composition draft is bounded"),
                    placement,
                }
            }
            SceneCompositionCommand::AddComponent { entity, component } => {
                PrefabCompositionCommand::AddComponent { entity, component }
            }
            SceneCompositionCommand::RemoveComponent { entity, component } => {
                PrefabCompositionCommand::RemoveComponent { entity, component }
            }
            SceneCompositionCommand::MoveEntity { entity, to } => {
                PrefabCompositionCommand::MoveEntity { entity, to }
            }
            SceneCompositionCommand::RemoveEntity { entity } => {
                PrefabCompositionCommand::RemoveEntity { entity }
            }
        };
        EditorAction::Mutate(EditorDocumentMutation::ComposePrefab {
            document: scene.document,
            base_revision: scene.revision,
            command,
        })
    } else {
        EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
            document: scene.document,
            base_revision: scene.revision,
            command,
        })
    }
}

fn field_action(target: &EditorFieldTarget, command: SceneInspectorCommand) -> EditorAction {
    EditorAction::Mutate(EditorDocumentMutation::ApplyInspectorCommand {
        document: target.document(),
        base_revision: target.base_revision(),
        command,
    })
}

fn parse_field_command(
    target: &EditorFieldTarget,
    kind: ComponentValueKind,
    text: &str,
) -> Result<SceneInspectorCommand, String> {
    if kind == ComponentValueKind::AssetRef {
        let asset_ref = if let Some(id) = text.strip_prefix("stable_id:") {
            AssetRef::stable_id(id).map_err(|error| error.to_string())
        } else {
            AssetRef::path(text).map_err(|error| error.to_string())
        }?;
        return Ok(SceneInspectorCommand::SetAssetRefField {
            entity: target.entity().clone(),
            component: target.component().clone(),
            component_version: target.component_version(),
            field: target.field().clone(),
            asset_ref,
        });
    }
    let value = match kind {
        ComponentValueKind::Bool => text
            .parse()
            .map(ComponentValue::Bool)
            .map_err(|_| "Expected true or false")?,
        ComponentValueKind::I64 => text
            .parse()
            .map(ComponentValue::I64)
            .map_err(|_| "Expected a signed integer")?,
        ComponentValueKind::U64 => text
            .parse()
            .map(ComponentValue::U64)
            .map_err(|_| "Expected an unsigned integer")?,
        ComponentValueKind::F64 => {
            ComponentValue::f64(text.parse().map_err(|_| "Expected a finite number")?)
                .map_err(|_| "Expected a finite number")?
        }
        ComponentValueKind::String => ComponentValue::String(text.to_owned()),
        ComponentValueKind::Null
        | ComponentValueKind::List
        | ComponentValueKind::Map
        | ComponentValueKind::EntityRef
        | ComponentValueKind::AssetRef => {
            return Err("This field kind is read-only in the generic Inspector".to_owned());
        }
    };
    Ok(SceneInspectorCommand::SetField {
        entity: target.entity().clone(),
        component: target.component().clone(),
        component_version: target.component_version(),
        field: target.field().clone(),
        value,
    })
}

fn field_source_text(field: &SceneInspectorFieldView) -> String {
    let value = field.value.as_ref().or(field.default_value.as_ref());
    if field.value_kind == ComponentValueKind::AssetRef {
        if let Some(ComponentValue::Map(fields)) = value {
            if let (Some(kind), Some(value)) = (
                fields.get("kind").and_then(ComponentValue::as_str),
                fields.get("value").and_then(ComponentValue::as_str),
            ) {
                return if kind == "stable_id" {
                    format!("stable_id:{value}")
                } else {
                    value.to_owned()
                };
            }
        }
        return String::new();
    }
    if let Some(value) = value {
        return value_label(Some(value));
    }
    match field.value_kind {
        ComponentValueKind::Bool => "false".to_owned(),
        ComponentValueKind::I64 | ComponentValueKind::U64 => "0".to_owned(),
        ComponentValueKind::F64 => "0.0".to_owned(),
        _ => String::new(),
    }
}

fn value_label(value: Option<&ComponentValue>) -> String {
    match value {
        None => "<missing>".to_owned(),
        Some(ComponentValue::Null) => "null".to_owned(),
        Some(ComponentValue::Bool(value)) => value.to_string(),
        Some(ComponentValue::I64(value)) => value.to_string(),
        Some(ComponentValue::U64(value)) => value.to_string(),
        Some(ComponentValue::F64(value)) => value.get().to_string(),
        Some(ComponentValue::String(value)) => value.clone(),
        Some(ComponentValue::List(values)) => format!("[{} items]", values.len()),
        Some(ComponentValue::Map(values)) => format!("{{{} fields}}", values.len()),
        Some(ComponentValue::EntityReference(reference)) => match reference {
            nara::identity::EntityReference::SceneLocal { entity } => {
                format!("scene-local:{}", entity.as_str())
            }
            nara::identity::EntityReference::Persistent { entity } => {
                format!("{}:{}", entity.namespace.as_str(), entity.entity)
            }
        },
    }
}

fn field_feedback_label(reason: EditorFieldFeedbackReason) -> &'static str {
    match reason {
        EditorFieldFeedbackReason::InvalidValue(_) => {
            "Invalid value for this field; correct it and apply again"
        }
        EditorFieldFeedbackReason::StaleAction => "The field changed before this edit was applied",
        EditorFieldFeedbackReason::MutationRejected => {
            "The field edit was rejected; see Diagnostics"
        }
    }
}

#[derive(Clone, Copy)]
struct TreePosition<'a> {
    index: usize,
    subtree_end: usize,
    row: &'a SceneInspectorEntityRow,
    previous: Option<&'a SceneEntityId>,
    next: Option<&'a SceneEntityId>,
}

fn tree_position<'a>(
    rows: &'a [SceneInspectorEntityRow],
    selected: &SceneEntityId,
) -> Option<TreePosition<'a>> {
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
        .map(|row| &row.id);
    let next = rows
        .get(subtree_end)
        .filter(|candidate| candidate.parent == row.parent)
        .map(|row| &row.id);
    Some(TreePosition {
        index,
        subtree_end,
        row,
        previous,
        next,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara::{
        reflect::{ComponentFieldId, ComponentRegistry, ComponentSchemaVersion, ComponentTypeId},
        scene::{SceneAuthoringSession, SceneDocument, SceneEntityRecord},
        tooling::{EditorExternalReloadState, SceneEditorState},
    };

    fn scene(kind: EditorDocumentKind) -> EditorSceneModel {
        let session = SceneAuthoringSession::new(SceneDocument::from_ordered_roots([
            SceneEntityRecord::new(SceneEntityId::new("owner").unwrap()),
        ]));
        EditorSceneModel {
            document: EditorDocumentId::from_raw(7),
            kind,
            title: "Test document".to_owned(),
            dirty: false,
            revision: session.revision(),
            saved_revision: session.revision(),
            external_reload: EditorExternalReloadState::Clean,
            selection: Default::default(),
            editor: SceneEditorState::new().model(&session, &ComponentRegistry::default(), None),
        }
    }

    fn target(scene: &EditorSceneModel) -> EditorFieldTarget {
        EditorFieldTarget::new(
            scene.document,
            scene.revision,
            SceneEntityId::new("owner").unwrap(),
            ComponentTypeId::new("custom.stats"),
            ComponentSchemaVersion::ONE,
            ComponentFieldId::new("value"),
        )
    }

    #[test]
    fn scalar_edits_preserve_integer_precision_and_reject_non_finite_numbers() {
        let scene = scene(EditorDocumentKind::Scene);
        let target = target(&scene);
        for (kind, text, expected) in [
            (
                ComponentValueKind::I64,
                "-9223372036854775808",
                ComponentValue::I64(i64::MIN),
            ),
            (
                ComponentValueKind::U64,
                "18446744073709551615",
                ComponentValue::U64(u64::MAX),
            ),
            (
                ComponentValueKind::String,
                "中文 / clipboard",
                ComponentValue::String("中文 / clipboard".to_owned()),
            ),
        ] {
            let command = parse_field_command(&target, kind, text).unwrap();
            let SceneInspectorCommand::SetField { value, .. } = command else {
                panic!("scalar edit did not preserve its typed field command");
            };
            assert_eq!(value, expected);
        }
        for text in ["NaN", "inf", "-inf", "1e999"] {
            assert!(parse_field_command(&target, ComponentValueKind::F64, text).is_err());
        }
        assert!(parse_field_command(&target, ComponentValueKind::U64, "-1").is_err());
        assert!(
            parse_field_command(&target, ComponentValueKind::I64, "9223372036854775808").is_err()
        );
    }

    #[test]
    fn asset_edits_use_the_asset_reference_command_and_generic_entity_fields_stay_read_only() {
        let scene = scene(EditorDocumentKind::Scene);
        let target = target(&scene);
        for text in [
            "textures/player.png",
            "stable_id:550e8400-e29b-41d4-a716-446655440000",
        ] {
            let command = parse_field_command(&target, ComponentValueKind::AssetRef, text).unwrap();
            let expected = if let Some(id) = text.strip_prefix("stable_id:") {
                AssetRef::stable_id(id).unwrap()
            } else {
                AssetRef::path(text).unwrap()
            };
            assert!(matches!(command, SceneInspectorCommand::SetAssetRefField {
                asset_ref, ..
            } if asset_ref == expected));
        }
        assert!(
            parse_field_command(&target, ComponentValueKind::AssetRef, "stable_id:invalid")
                .is_err()
        );
        assert!(
            parse_field_command(&target, ComponentValueKind::EntityRef, "scene-local:owner")
                .is_err()
        );
        let reference =
            ComponentValue::EntityReference(nara::identity::EntityReference::SceneLocal {
                entity: SceneEntityId::new("owner").unwrap(),
            });
        assert_eq!(value_label(Some(&reference)), "scene-local:owner");
    }

    #[test]
    fn revision_selection_and_panel_retirement_discard_only_local_drafts() {
        let mut scene = scene(EditorDocumentKind::Scene);
        let mut panel = ImguiAuthoringPanels::default();
        panel.synchronize(Some(&scene));
        let key = target(&scene);
        panel.drafts.insert(
            key.clone(),
            FieldDraft {
                text: "unapplied".to_owned(),
                error: None,
            },
        );
        panel
            .structured_drafts
            .insert(key.clone(), value_draft::RootDraft::default());
        panel.synchronize(Some(&scene));
        assert_eq!(panel.drafts[&key].text, "unapplied");
        assert!(panel.structured_drafts.contains_key(&key));

        scene.editor.inspector.selected_entity = Some(SceneEntityId::new("owner").unwrap());
        panel.synchronize(Some(&scene));
        assert!(panel.drafts.is_empty());
        assert!(panel.structured_drafts.is_empty());
        panel.drafts.insert(
            key.clone(),
            FieldDraft {
                text: "stale".to_owned(),
                error: None,
            },
        );
        scene.revision = SceneAuthoringSession::new(SceneDocument::default()).revision();
        panel.synchronize(Some(&scene));
        assert!(panel.drafts.is_empty());

        panel.drafts.insert(
            key,
            FieldDraft {
                text: "closed".to_owned(),
                error: None,
            },
        );
        panel.pending_removal = Some(SceneEntityId::new("owner").unwrap());
        panel
            .structured_drafts
            .insert(target(&scene), value_draft::RootDraft::default());
        panel.retire();
        assert!(panel.scope.is_none());
        assert!(panel.drafts.is_empty());
        assert!(panel.structured_drafts.is_empty());
        assert!(panel.pending_removal.is_none());
        assert!(!scene.dirty);
    }

    #[test]
    fn component_actions_keep_the_exact_document_kind_revision_and_stable_component_id() {
        for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
            let scene = scene(kind);
            let entity = SceneEntityId::new("owner").unwrap();
            let component = ComponentTypeId::new("external.authoring.Custom");
            let action = compose_action(
                &scene,
                SceneCompositionCommand::AddComponent {
                    entity: entity.clone(),
                    component: component.clone(),
                },
            );
            match action {
                EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
                    document,
                    base_revision,
                    command:
                        SceneCompositionCommand::AddComponent {
                            entity: actual_entity,
                            component: actual_component,
                        },
                }) => {
                    assert_eq!(kind, EditorDocumentKind::Scene);
                    assert_eq!((document, base_revision), (scene.document, scene.revision));
                    assert_eq!((actual_entity, actual_component), (entity, component));
                }
                EditorAction::Mutate(EditorDocumentMutation::ComposePrefab {
                    document,
                    base_revision,
                    command:
                        PrefabCompositionCommand::AddComponent {
                            entity: actual_entity,
                            component: actual_component,
                        },
                }) => {
                    assert_eq!(kind, EditorDocumentKind::Prefab);
                    assert_eq!((document, base_revision), (scene.document, scene.revision));
                    assert_eq!((actual_entity, actual_component), (entity, component));
                }
                other => panic!("component action changed document authority: {other:?}"),
            }
        }
    }

    #[test]
    fn an_uncertain_save_allows_only_reopening_its_exact_document() {
        let scene = scene(EditorDocumentKind::Scene);
        let result = Some(EditorPersistenceResult::PersistenceUncertain {
            document: scene.document,
            revision: scene.revision,
            digest: nara::tooling::EditorDocumentDigest::new(0, [0; 32]),
        });
        assert!(!persistence_allows(
            result,
            EditorPersistenceRequestKind::Save,
            scene.document
        ));
        assert!(persistence_allows(
            result,
            EditorPersistenceRequestKind::Reopen,
            scene.document
        ));
        assert!(!persistence_allows(
            result,
            EditorPersistenceRequestKind::Reopen,
            EditorDocumentId::from_raw(8)
        ));
    }
}
