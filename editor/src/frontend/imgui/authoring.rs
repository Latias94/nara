use dear_imgui_rs::{TreeNodeFlags, Ui};
use nara::{
    reflect::{ComponentCapability, ComponentValue, ComponentValueKind},
    scene::{AuthoringDocumentReadiness, SceneEntityId, ScenePlacement, ScenePlacementOwner},
    tooling::{
        EditorAction, EditorDocumentId, EditorDocumentKind, EditorDocumentMutation,
        EditorFrameModel, EditorNavigationAction, EditorPersistenceAction,
        EditorPersistenceOperation, EditorPersistenceRejection, EditorPersistenceRequestKind,
        EditorPersistenceResult, EditorPlayState, EditorReopenTarget, EditorRuntimeAvailability,
        EditorSceneModel, EditorWorkspaceIntent, EditorWorkspaceIntentAction,
        PrefabCompositionCommand, PrefabCompositionEntityDraft, SceneCompositionCommand,
        SceneCompositionEntityKind, SceneInspectorComponentView, SceneInspectorEntityRow,
        SceneInspectorFieldView,
    },
};

use super::{
    ImguiTextOwner,
    field_editor::{FieldEditors, PreparedFieldEdit},
};

pub(super) mod recursive_controls;
pub(super) mod value_draft;

#[derive(Default)]
pub(super) struct ImguiAuthoringPanels {
    scope: Option<EditorDocumentId>,
    pub(super) fields: FieldEditors,
    pending_removal: Option<SceneEntityId>,
    read_only: bool,
}

impl ImguiAuthoringPanels {
    pub(super) fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub(super) fn retire(&mut self) {
        self.scope = None;
        self.fields.clear();
        self.pending_removal = None;
    }

    fn synchronize(&mut self, scene: Option<&EditorSceneModel>) {
        let scope = scene.map(|scene| scene.document);
        if self.scope.is_some() && scope.is_some() && self.scope != scope {
            self.retire();
        }
        self.scope = scope;
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
                && (document.kind != EditorDocumentKind::Scene
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
                EditorPersistenceResult::Rejected {
                    reason: EditorPersistenceRejection::RequiresReconcile,
                    ..
                } => ui.text_wrapped(
                    "Saving could not be confirmed. Reopen this document to reconcile its source.",
                ),
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
        let removal_reason = entity_removal_unavailable_reason(frame, scene);
        self.show_composition_controls(ui, scene, editing, removal_reason, actions);
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
        self.show_remove_confirmation(ui, scene, editing && removal_reason.is_none(), actions);
    }

    fn show_composition_controls(
        &mut self,
        ui: &Ui,
        scene: &EditorSceneModel,
        editing: bool,
        removal_reason: Option<&'static str>,
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
        let owner = selected
            .row
            .parent
            .clone()
            .map_or(ScenePlacementOwner::Root, |parent| {
                ScenePlacementOwner::Parent { parent }
            });
        if enabled_button(ui, editing && selected.previous.is_some(), "Up") {
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
        if enabled_button(ui, editing && selected.next.is_some(), "Down") {
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
        if enabled_button(ui, editing && removal_reason.is_none(), "Remove Entity") {
            self.pending_removal = Some(selected.row.id.clone());
        }
        if let Some(reason) = removal_reason {
            ui.text_disabled(reason);
        }
        let parent_label = selected
            .row
            .parent
            .as_ref()
            .map_or("Root", SceneEntityId::as_str);
        let parent_combo = ui.begin_combo("Parent", parent_label);
        #[cfg(test)]
        super::interaction_tests::record_control(ui, "Parent");
        if let Some(_combo) = parent_combo {
            if enabled_button(ui, editing && selected.row.parent.is_some(), "Root") {
                actions.push(compose_action(
                    scene,
                    SceneCompositionCommand::MoveEntity {
                        entity: selected.row.id.clone(),
                        to: ScenePlacement::last(ScenePlacementOwner::Root),
                    },
                ));
                ui.close_current_popup();
            }
            #[cfg(test)]
            super::interaction_tests::record_control(ui, "parent:root");
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
                    ui.close_current_popup();
                }
                #[cfg(test)]
                super::interaction_tests::record_control(
                    ui,
                    format!("parent:{}", candidate.id.as_str()),
                );
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
        if enabled_button(ui, true, "Cancel removal") {
            self.pending_removal = None;
        }
    }

    pub(super) fn show_inspector(
        &mut self,
        ui: &Ui,
        frame: &EditorFrameModel,
        actions: &mut Vec<EditorAction>,
        text_owner: &mut Option<ImguiTextOwner>,
        prepared: &mut Vec<PreparedFieldEdit>,
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
            "document:{}:entity:{}",
            scene.document.raw(),
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
                    ui.close_current_popup();
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
                ui, frame, scene, &entity.id, component, editing, actions, text_owner, prepared,
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
        prepared: &mut Vec<PreparedFieldEdit>,
    ) {
        let _id = ui.push_id(component.component.as_str());
        let open = ui.collapsing_header(component.component.as_str(), TreeNodeFlags::DEFAULT_OPEN);
        #[cfg(test)]
        super::interaction_tests::record_control(
            ui,
            format!("component:{}", component.component.as_str()),
        );
        if !open {
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
            let editable = editing
                && component.access.is_editable()
                && field.capabilities.contains(&ComponentCapability::Edit);
            if let Some(edit) = self.fields.show(
                ui, frame, scene, entity, component, field, editable, text_owner,
            ) {
                prepared.push(edit);
            }
        }
        if component.fields.is_empty() {
            ui.text_disabled("No inspectable fields");
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
            | "+ Child"
            | "Up"
            | "Down"
            | "Remove Entity"
            | "Confirm removal"
            | "Cancel removal"
            | "Apply"
    ) || label == super::interaction_tests::CUSTOM_COMPONENT
    {
        super::interaction_tests::record_control(ui, label);
    }
    clicked
}

fn entity_removal_unavailable_reason(
    frame: &EditorFrameModel,
    scene: &EditorSceneModel,
) -> Option<&'static str> {
    if !scene.editor.inspector.composition.generic_authoring {
        return None;
    }
    if frame
        .workspace()
        .document(scene.document)
        .and_then(|tab| tab.authoring_readiness)
        != Some(AuthoringDocumentReadiness::Complete)
    {
        return Some("Entity deletion requires a Complete authoring document");
    }
    if scene
        .editor
        .inspector
        .entities
        .iter()
        .any(|entity| entity.has_prefab)
    {
        return Some("Entity deletion requires a source without Prefab anchors");
    }
    None
}

pub(super) fn editing_enabled(frame: &EditorFrameModel) -> bool {
    (matches!(
        frame.runtime_availability(),
        EditorRuntimeAvailability::ExternalSupervised
    ) || matches!(
        frame.play().state(),
        EditorPlayState::Empty | EditorPlayState::Paused | EditorPlayState::Faulted
    )) && frame.workspace_intent().intent().is_none()
        && matches!(
            frame.persistence().operation(),
            EditorPersistenceOperation::Idle | EditorPersistenceOperation::Saving { .. }
        )
}

pub(super) fn persistence_allows(
    result: Option<EditorPersistenceResult>,
    request: EditorPersistenceRequestKind,
    document: EditorDocumentId,
) -> bool {
    match result {
        None => true,
        Some(EditorPersistenceResult::Rejected {
            document: target,
            reason: EditorPersistenceRejection::RequiresReconcile,
            ..
        }) => request == EditorPersistenceRequestKind::Reopen && target == Some(document),
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

pub(super) fn field_source_text(field: &SceneInspectorFieldView) -> String {
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

pub(super) fn value_label(value: Option<&ComponentValue>) -> String {
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
        reflect::{ComponentRegistry, ComponentTypeId},
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
        for result in [
            Some(EditorPersistenceResult::PersistenceUncertain {
                document: scene.document,
                revision: scene.revision,
                digest: nara::tooling::EditorDocumentDigest::new(0, [0; 32]),
            }),
            Some(EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Save,
                document: Some(scene.document),
                reason: EditorPersistenceRejection::RequiresReconcile,
            }),
        ] {
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
        assert!(!persistence_allows(
            Some(EditorPersistenceResult::Rejected {
                request: EditorPersistenceRequestKind::Save,
                document: None,
                reason: EditorPersistenceRejection::RequiresReconcile,
            }),
            EditorPersistenceRequestKind::Reopen,
            scene.document,
        ));
    }

    #[test]
    fn an_ordinary_save_keeps_the_authoring_frame_editable_but_reopen_does_not() {
        use nara::tooling::{
            EditorAuthoringReadiness, EditorFrameClock, EditorPersistenceView, EditorPlayView,
            EditorProjectContentView, EditorProjectReadinessView, EditorProjectView,
            EditorWorkspace, EditorWorkspaceIntentView,
        };
        let mut workspace = EditorWorkspace::new();
        let report = workspace
            .open_scene_session(
                "Editable",
                SceneAuthoringSession::new(SceneDocument::from_ordered_roots([
                    SceneEntityRecord::new(SceneEntityId::new("owner").unwrap()),
                ])),
            )
            .unwrap();
        let document = report.opened_document.unwrap();
        let revision = workspace.scene(document).unwrap().revision();
        for (operation, editable) in [
            (EditorPersistenceOperation::Idle, true),
            (
                EditorPersistenceOperation::Saving {
                    document,
                    captured_revision: revision,
                },
                true,
            ),
            (EditorPersistenceOperation::Opening { document }, false),
        ] {
            let frame = EditorFrameClock::new().publish(
                workspace.model(&ComponentRegistry::default(), None),
                EditorProjectView::new(
                    EditorProjectReadinessView::new(
                        EditorAuthoringReadiness::Complete,
                        EditorRuntimeAvailability::TrustedInProcess,
                    ),
                    Default::default(),
                    EditorPlayView::new(EditorPlayState::Empty, None, None, None, false, None),
                    EditorPersistenceView::new(operation, None),
                    EditorProjectContentView::NotInstalled,
                    EditorWorkspaceIntentView::new(None, None, None, None, None),
                ),
                Default::default(),
                EditorReopenTarget::OpenDocument {
                    document,
                    base_revision: revision,
                },
                None,
            );
            assert_eq!(editing_enabled(&frame), editable, "{operation:?}");
        }
    }
}
