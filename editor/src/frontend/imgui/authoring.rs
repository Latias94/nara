use dear_imgui_rs::Ui;
use nara::{
    reflect::{ComponentValue, ComponentValueKind},
    scene::{
        AuthoringDocumentReadiness, PrefabEntityOrigin, PrefabEntityTarget, SceneEntityId,
        ScenePlacement, ScenePlacementParent, ScenePlacementSlot, SourceDocumentKey,
        SourceDocumentOrigin,
    },
    tooling::{
        EditorAction, EditorDocumentId, EditorDocumentKind, EditorDocumentMutation,
        EditorFrameModel, EditorNavigationAction, EditorPersistenceAction,
        EditorPersistenceOperation, EditorPersistenceRejection, EditorPersistenceRequestKind,
        EditorPersistenceResult, EditorPersistenceView, EditorPlayState, EditorReopenTarget,
        EditorRuntimeAvailability, EditorSceneModel, EditorWorkspaceIntent,
        EditorWorkspaceIntentAction, PrefabCompositionCommand, PrefabCompositionEntityDraft,
        SceneCompositionCommand, SceneCompositionEntityKind, SceneInspectorFieldView,
    },
};

use super::field_editor::FieldEditors;

pub(super) mod recursive_controls;
pub(super) mod value_draft;

#[derive(Default)]
pub(super) struct ImguiAuthoringPanels {
    scope: Option<EditorDocumentId>,
    pub(super) fields: FieldEditors,
    pub(super) source_tool: super::source_tool::SourceToolWidgets,
    pending_removal: Option<SceneEntityId>,
    read_only: bool,
}

impl ImguiAuthoringPanels {
    pub(super) fn set_read_only(&mut self, read_only: bool) {
        self.read_only = read_only;
    }

    pub(super) fn hide_document_controls(&mut self) {
        self.pending_removal = None;
    }

    pub(super) fn retire(&mut self) {
        self.scope = None;
        self.fields.clear();
        self.source_tool = Default::default();
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
        let combo = ui.begin_combo("Document", title);
        #[cfg(test)]
        super::ui_interaction_tests::record_control(ui, "Document selector");
        if let Some(_combo) = combo {
            for tab in &frame.workspace().documents {
                let _id = ui.push_id(format!("document:{}", tab.document.raw()).as_str());
                let clicked = ui
                    .selectable_config(format!(
                        "{}{}",
                        tab.title,
                        if tab.dirty { " *" } else { "" }
                    ))
                    .selected(tab.active)
                    .disabled(self.read_only || frame.workspace_intent().intent().is_some())
                    .build();
                #[cfg(test)]
                super::ui_interaction_tests::record_control(
                    ui,
                    format!("document:{}", tab.document.raw()),
                );
                if clicked && (!tab.active || tab.kind == EditorDocumentKind::Scene) {
                    actions.push(EditorAction::Navigate(
                        EditorNavigationAction::SetActiveDocument {
                            document: tab.document,
                        },
                    ));
                }
            }
        }
        if document.is_some_and(|document| document.kind == EditorDocumentKind::Prefab)
            && let Some(document) = frame.scene_preview_document()
        {
            ui.same_line();
            if enabled_button(ui, !self.read_only, "Back to Scene") {
                actions.push(EditorAction::Navigate(
                    EditorNavigationAction::SetActiveDocument { document },
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
                && persistence.operation().is_none()
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
        if let EditorPersistenceView::Terminal { attempt, result } = frame.persistence() {
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
                frame.persistence().operation().is_none(),
                "Dismiss persistence result",
            ) {
                actions.push(EditorAction::Persistence(
                    EditorPersistenceAction::AcknowledgeResult { attempt },
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
        ui.text_disabled("Source declarations; resolved hierarchy is unavailable in this view");
        for entity in &scene.editor.inspector.entities {
            let _id = ui.push_id(entity.id.as_str());
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
            ui.text_wrapped(placement_intent_label(entity.placement.as_ref()));
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
            .and_then(|id| inspector.entities.iter().find(|row| &row.id == id));
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
                    placement: ScenePlacement::last(ScenePlacementParent::Root),
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
                        placement: ScenePlacement::last(ScenePlacementParent::Entity(
                            PrefabEntityTarget::source(selected.id.clone()),
                        )),
                    },
                ));
            }
        }
        let Some(selected) = selected else {
            return;
        };
        if enabled_button(ui, editing && removal_reason.is_none(), "Remove Entity") {
            self.pending_removal = Some(selected.id.clone());
        }
        if let Some(reason) = removal_reason {
            ui.text_disabled(reason);
        }
        let parent = selected
            .placement
            .as_ref()
            .map(|placement| &placement.parent);
        let parent_label = parent.map_or_else(
            || "No placement declaration".to_owned(),
            placement_parent_label,
        );
        ui.text_disabled("Desired placement; compilation checks the resulting layout");
        let parent_combo = ui.begin_combo("Parent", &parent_label);
        #[cfg(test)]
        super::interaction_tests::record_control(ui, "Parent");
        if let Some(_combo) = parent_combo {
            if enabled_button(
                ui,
                editing && parent != Some(&ScenePlacementParent::Root),
                "Root",
            ) {
                actions.push(compose_action(
                    scene,
                    SceneCompositionCommand::MoveEntity {
                        target: PrefabEntityTarget::source(selected.id.clone()),
                        to: ScenePlacement::last(ScenePlacementParent::Root),
                    },
                ));
                ui.close_current_popup();
            }
            #[cfg(test)]
            super::interaction_tests::record_control(ui, "parent:root");
            for candidate in &inspector.entities {
                let desired_parent =
                    ScenePlacementParent::Entity(PrefabEntityTarget::source(candidate.id.clone()));
                let eligible =
                    editing && candidate.id != selected.id && parent != Some(&desired_parent);
                if enabled_button(ui, eligible, candidate.id.as_str()) {
                    actions.push(compose_action(
                        scene,
                        SceneCompositionCommand::MoveEntity {
                            target: PrefabEntityTarget::source(selected.id.clone()),
                            to: ScenePlacement::last(desired_parent),
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
        let Some(placement) = &selected.placement else {
            ui.text_disabled("Choose a desired parent before setting order");
            return;
        };
        let order_combo = ui.begin_combo("Order", placement_slot_label(&placement.slot));
        #[cfg(test)]
        super::interaction_tests::record_control(ui, "Order");
        if let Some(_combo) = order_combo {
            let choices = [ScenePlacementSlot::First, ScenePlacementSlot::Last]
                .into_iter()
                .chain(
                    inspector
                        .entities
                        .iter()
                        .filter(|row| row.id != selected.id)
                        .flat_map(|row| {
                            let target = PrefabEntityTarget::source(row.id.clone());
                            [
                                ScenePlacementSlot::Before(target.clone()),
                                ScenePlacementSlot::After(target),
                            ]
                        }),
                );
            for slot in choices {
                let label = placement_slot_label(&slot);
                if enabled_button(ui, editing && slot != placement.slot, &label) {
                    actions.push(compose_action(
                        scene,
                        SceneCompositionCommand::MoveEntity {
                            target: PrefabEntityTarget::source(selected.id.clone()),
                            to: ScenePlacement {
                                parent: placement.parent.clone(),
                                slot,
                            },
                        },
                    ));
                    ui.close_current_popup();
                }
                #[cfg(test)]
                super::interaction_tests::record_control(ui, format!("order:{label}"));
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
        if !scene
            .editor
            .inspector
            .entities
            .iter()
            .any(|row| row.id == entity)
        {
            self.pending_removal = None;
            return;
        }
        ui.separator();
        ui.text_wrapped(format!(
            "Remove the source-local branch rooted at {}?",
            entity.as_str(),
        ));
        ui.text_wrapped("Removal is refused if the source-local hierarchy cannot be resolved");
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
}

fn enabled_button(ui: &Ui, enabled: bool, label: &str) -> bool {
    let clicked = ui.with_disabled_if(!enabled, || ui.button(label));
    #[cfg(test)]
    if matches!(
        label,
        "+ Root"
            | "Back to Scene"
            | "Undo"
            | "Redo"
            | "Save"
            | "Reopen"
            | "Dismiss persistence result"
            | "Open Prefab Source"
            | "Remove Component"
            | "+ Child"
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
            None | Some(EditorPersistenceOperation::Saving { .. })
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

pub(super) fn compose_action(
    scene: &EditorSceneModel,
    command: SceneCompositionCommand,
) -> EditorAction {
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
            SceneCompositionCommand::MoveEntity { target, to } => {
                PrefabCompositionCommand::MoveEntity { target, to }
            }
            SceneCompositionCommand::RevertPlacement { target } => {
                PrefabCompositionCommand::RevertPlacement { target }
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
            nara::identity::EntityReference::Authored { reference } => {
                use nara::identity::{AuthoredReferenceTarget, PrefabEntityOrigin};
                let origin_label = |origin: &PrefabEntityOrigin| match origin {
                    PrefabEntityOrigin::SourceEntity(entity) => format!("source:{entity}"),
                    PrefabEntityOrigin::InstanceAddition(entity) => format!("addition:{entity}"),
                };
                let endpoint = match &reference.target {
                    AuthoredReferenceTarget::Entity(origin) => origin_label(origin),
                    AuthoredReferenceTarget::Input(input) => format!("input:{input}"),
                };
                let route = reference
                    .through
                    .iter()
                    .map(|hop| origin_label(&hop.anchor))
                    .collect::<Vec<_>>();
                if route.is_empty() {
                    endpoint
                } else {
                    format!("{} / {endpoint}", route.join(" / "))
                }
            }
        },
    }
}

pub(super) fn placement_intent_label(placement: Option<&ScenePlacement>) -> String {
    match placement {
        Some(placement) => format!(
            "Desired: {} / {}",
            placement_parent_label(&placement.parent),
            placement_slot_label(&placement.slot),
        ),
        None => "No source placement declaration".to_owned(),
    }
}

fn placement_parent_label(parent: &ScenePlacementParent) -> String {
    match parent {
        ScenePlacementParent::Root => "Root".to_owned(),
        ScenePlacementParent::Entity(target) => placement_target_label(target),
    }
}

fn placement_slot_label(slot: &ScenePlacementSlot) -> String {
    match slot {
        ScenePlacementSlot::First => "First".to_owned(),
        ScenePlacementSlot::Last => "Last".to_owned(),
        ScenePlacementSlot::Before(target) => format!("Before {}", placement_target_label(target)),
        ScenePlacementSlot::After(target) => format!("After {}", placement_target_label(target)),
    }
}

fn placement_target_label(target: &PrefabEntityTarget) -> String {
    let mut route = target
        .through
        .iter()
        .map(|hop| {
            format!(
                "through {} @ {}",
                placement_origin_label(&hop.anchor),
                source_key_label(&hop.expected_base),
            )
        })
        .collect::<Vec<_>>();
    route.push(placement_origin_label(&target.entity));
    route.join(" / ")
}

fn placement_origin_label(origin: &PrefabEntityOrigin) -> String {
    match origin {
        PrefabEntityOrigin::SourceEntity(entity) => format!("source:{entity}"),
        PrefabEntityOrigin::InstanceAddition(entity) => format!("addition:{entity}"),
    }
}

fn source_key_label(source: &SourceDocumentKey) -> String {
    let origin = match source.origin() {
        SourceDocumentOrigin::Named { universe, name } => format!("named:{name} [{universe}]"),
        SourceDocumentOrigin::ProjectLocation { universe, path } => {
            format!("path:{path} [{universe}]")
        }
        SourceDocumentOrigin::StableAsset { stable_id } => format!("asset:{stable_id}"),
    };
    format!("{:?} {origin}", source.kind())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara::{
        reflect::{ComponentRegistryBuilder, ComponentRegistrySnapshot, ComponentTypeId},
        scene::{SceneAuthoringSession, SceneDocument, SceneEntityRecord},
        tooling::{EditorExternalReloadState, SceneEditorState},
    };

    fn frozen_empty_registry() -> ComponentRegistrySnapshot {
        ComponentRegistryBuilder::new().freeze().unwrap()
    }

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
            editor: SceneEditorState::new().model(&session, &frozen_empty_registry(), None),
        }
    }

    fn document_frame(kind: EditorDocumentKind) -> EditorFrameModel {
        use nara::{
            scene::{PrefabAuthoringSession, PrefabDocument},
            tooling::{
                EditorAuthoringReadiness, EditorDocumentSourceBinding, EditorFrameClock,
                EditorPersistenceView, EditorPlayView, EditorProjectContentView,
                EditorProjectReadinessView, EditorProjectView, EditorWorkspace,
                EditorWorkspaceIntentView,
            },
        };

        let mut workspace = EditorWorkspace::new();
        for title in ["First Scene", "Second Scene"] {
            workspace
                .open_scene_session(title, SceneAuthoringSession::new(SceneDocument::default()))
                .unwrap();
        }
        if kind == EditorDocumentKind::Prefab {
            workspace
                .open_prefab_session(
                    "Ordinary Prefab",
                    EditorDocumentSourceBinding::new("ordinary.prefab.json"),
                    PrefabAuthoringSession::new(PrefabDocument::default()),
                )
                .unwrap();
        }
        EditorFrameClock::new().publish(
            workspace.model(&frozen_empty_registry(), None),
            EditorProjectView::new(
                EditorProjectReadinessView::new(
                    EditorAuthoringReadiness::Complete,
                    EditorRuntimeAvailability::TrustedInProcess,
                ),
                Default::default(),
                EditorPlayView::new(EditorPlayState::Empty, None, None, None, false, None),
                EditorPersistenceView::Idle,
                EditorProjectContentView::NotInstalled,
                EditorWorkspaceIntentView::new(None, None, None, None, None),
            ),
            Default::default(),
            EditorReopenTarget::Unavailable,
            None,
        )
    }

    fn draw_document_controls(
        context: &mut dear_imgui_rs::Context,
        panels: &mut ImguiAuthoringPanels,
        frame: &EditorFrameModel,
    ) -> Vec<EditorAction> {
        super::super::ui_interaction_tests::clear_controls();
        let ui = context.frame();
        let mut actions = Vec::new();
        ui.window("Document navigation")
            .position([0.0, 0.0], dear_imgui_rs::Condition::Always)
            .size([900.0, 500.0], dear_imgui_rs::Condition::Always)
            .build(|| panels.show_document_controls(ui, frame, &mut actions));
        let _ = context.render_legacy();
        actions
    }

    fn click_document_control(
        context: &mut dear_imgui_rs::Context,
        panels: &mut ImguiAuthoringPanels,
        frame: &EditorFrameModel,
        label: &str,
    ) -> Vec<EditorAction> {
        let rect = super::super::ui_interaction_tests::control_rect(label)
            .unwrap_or_else(|| panic!("the document control {label:?} was not painted"));
        context.io_mut().add_mouse_pos_event([
            (rect.min[0] + rect.max[0]) * 0.5,
            (rect.min[1] + rect.max[1]) * 0.5,
        ]);
        let mut actions = draw_document_controls(context, panels, frame);
        for down in [true, false] {
            context
                .io_mut()
                .add_mouse_button_event(dear_imgui_rs::MouseButton::Left, down);
            actions.extend(draw_document_controls(context, panels, frame));
        }
        actions
    }

    fn document_context() -> dear_imgui_rs::Context {
        let mut context = dear_imgui_rs::Context::create();
        context
            .set_ini_filename::<std::path::PathBuf>(None)
            .unwrap();
        context.io_mut().set_display_size([1000.0, 600.0]);
        context.io_mut().set_delta_time(1.0 / 60.0);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context
    }

    #[test]
    fn desired_placement_labels_preserve_routed_source_predicates_and_origin_kinds() {
        let route = |name| PrefabEntityTarget {
            through: vec![nara::scene::PrefabInstanceHop {
                anchor: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("instance").unwrap()),
                expected_base: SourceDocumentKey::named(
                    "f9ab807a-c61e-4c92-bfee-b6fa153b8efc".parse().unwrap(),
                    name,
                    nara::scene::SourceDocumentKind::Prefab,
                )
                .unwrap(),
            }],
            entity: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("root").unwrap()),
        };
        let first = route("first");
        let second = route("second");
        let placement =
            ScenePlacement::after(ScenePlacementParent::Entity(first.clone()), second.clone());
        let label = placement_intent_label(Some(&placement));
        assert!(label.contains("Desired:"));
        assert!(label.contains("named:first"));
        assert!(label.contains("named:second"));
        assert!(label.contains("f9ab807a-c61e-4c92-bfee-b6fa153b8efc"));
        assert_ne!(
            placement_target_label(&first),
            placement_target_label(&second),
        );
        assert_ne!(
            placement_target_label(&PrefabEntityTarget::source(
                SceneEntityId::new("root").unwrap(),
            )),
            placement_target_label(&PrefabEntityTarget::addition(
                SceneEntityId::new("root").unwrap(),
            )),
        );
        assert_eq!(
            placement_intent_label(None),
            "No source placement declaration"
        );
    }

    #[test]
    fn order_widget_declares_an_explicit_slot_without_inventing_effective_siblings() {
        for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
            let mut scene = scene(kind);
            let selected = scene.editor.inspector.entities[0].id.clone();
            let parent = ScenePlacementParent::Entity(PrefabEntityTarget {
                through: vec![nara::scene::PrefabInstanceHop {
                    anchor: PrefabEntityOrigin::SourceEntity(
                        SceneEntityId::new("instance").unwrap(),
                    ),
                    expected_base: SourceDocumentKey::named(
                        "f9ab807a-c61e-4c92-bfee-b6fa153b8efc".parse().unwrap(),
                        "selected-source",
                        nara::scene::SourceDocumentKind::Prefab,
                    )
                    .unwrap(),
                }],
                entity: PrefabEntityOrigin::SourceEntity(SceneEntityId::new("root").unwrap()),
            });
            scene.editor.inspector.entities[0].placement =
                Some(ScenePlacement::last(parent.clone()));
            scene.editor.inspector.selected_entity = Some(selected.clone());
            let mut context = document_context();
            let mut panels = ImguiAuthoringPanels::default();
            let draw = |context: &mut dear_imgui_rs::Context, panels: &mut ImguiAuthoringPanels| {
                super::super::ui_interaction_tests::clear_controls();
                let ui = context.frame();
                let mut actions = Vec::new();
                ui.window("Source placement")
                    .position([0.0, 0.0], dear_imgui_rs::Condition::Always)
                    .size([900.0, 500.0], dear_imgui_rs::Condition::Always)
                    .build(|| {
                        panels.show_composition_controls(ui, &scene, true, None, &mut actions)
                    });
                let _ = context.render_legacy();
                actions
            };
            for _ in 0..3 {
                assert!(draw(&mut context, &mut panels).is_empty());
            }
            assert!(super::super::ui_interaction_tests::control_rect("Up").is_none());
            assert!(super::super::ui_interaction_tests::control_rect("Down").is_none());
            let mut click = |label| {
                let rect = super::super::ui_interaction_tests::control_rect(label).unwrap();
                context.io_mut().add_mouse_pos_event([
                    (rect.min[0] + rect.max[0]) * 0.5,
                    (rect.min[1] + rect.max[1]) * 0.5,
                ]);
                let mut actions = draw(&mut context, &mut panels);
                for down in [true, false] {
                    context
                        .io_mut()
                        .add_mouse_button_event(dear_imgui_rs::MouseButton::Left, down);
                    actions.extend(draw(&mut context, &mut panels));
                }
                for _ in 0..2 {
                    actions.extend(draw(&mut context, &mut panels));
                }
                actions
            };
            assert!(click("Order").is_empty());
            let actions = click("order:First");
            let target = PrefabEntityTarget::source(selected);
            let expected = ScenePlacement::first(parent);
            match actions.as_slice() {
                [
                    EditorAction::Mutate(EditorDocumentMutation::ComposeScene {
                        command: SceneCompositionCommand::MoveEntity { target: actual, to },
                        ..
                    }),
                ] => {
                    assert_eq!(kind, EditorDocumentKind::Scene);
                    assert_eq!((actual, to), (&target, &expected));
                }
                [
                    EditorAction::Mutate(EditorDocumentMutation::ComposePrefab {
                        command: PrefabCompositionCommand::MoveEntity { target: actual, to },
                        ..
                    }),
                ] => {
                    assert_eq!(kind, EditorDocumentKind::Prefab);
                    assert_eq!((actual, to), (&target, &expected));
                }
                other => panic!("order intent changed source authority: {other:?}"),
            }
        }
    }

    #[test]
    fn document_selector_reselects_active_scene_without_reselecting_active_prefab() {
        for kind in [EditorDocumentKind::Scene, EditorDocumentKind::Prefab] {
            let frame = document_frame(kind);
            assert_eq!(frame.scene_preview_document(), None);
            let document = frame.workspace().active_document.unwrap();
            let mut context = document_context();
            let mut panels = ImguiAuthoringPanels::default();
            for _ in 0..3 {
                assert!(draw_document_controls(&mut context, &mut panels, &frame).is_empty());
            }
            assert!(
                click_document_control(&mut context, &mut panels, &frame, "Document selector")
                    .is_empty()
            );
            for _ in 0..2 {
                assert!(draw_document_controls(&mut context, &mut panels, &frame).is_empty());
            }
            let actions = click_document_control(
                &mut context,
                &mut panels,
                &frame,
                &format!("document:{}", document.raw()),
            );
            if kind == EditorDocumentKind::Scene {
                assert!(matches!(
                    actions.as_slice(),
                    [EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
                        document: selected,
                    })] if *selected == document
                ));
            } else {
                assert!(
                    actions.is_empty(),
                    "active Prefab selection must remain a no-op"
                );
            }
        }
    }

    #[test]
    fn back_to_scene_is_absent_without_a_retained_binding_even_with_open_scenes() {
        let frame = document_frame(EditorDocumentKind::Prefab);
        assert_eq!(frame.workspace().scenes.len(), 2);
        assert_eq!(frame.scene_preview_document(), None);
        let mut context = document_context();
        let mut panels = ImguiAuthoringPanels::default();
        for _ in 0..3 {
            assert!(draw_document_controls(&mut context, &mut panels, &frame).is_empty());
        }
        assert!(super::super::ui_interaction_tests::control_rect("Back to Scene").is_none());
    }

    #[test]
    fn back_to_scene_activates_the_bound_scene_instead_of_the_first_open_scene() {
        let frame = document_frame(EditorDocumentKind::Prefab);
        let document = frame.workspace().scenes[1].document;
        assert_ne!(frame.workspace().scenes[0].document, document);
        let frame = frame.with_scene_preview_document(Some(document));
        let mut context = document_context();
        let mut panels = ImguiAuthoringPanels::default();
        for _ in 0..3 {
            assert!(draw_document_controls(&mut context, &mut panels, &frame).is_empty());
        }
        let actions = click_document_control(&mut context, &mut panels, &frame, "Back to Scene");
        assert!(matches!(
            actions.as_slice(),
            [EditorAction::Navigate(EditorNavigationAction::SetActiveDocument {
                document: selected,
            })] if *selected == document
        ));
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
        let mut clock = EditorFrameClock::new();
        let source = clock.publish(
            workspace.model(&frozen_empty_registry(), None),
            EditorProjectView::new(
                EditorProjectReadinessView::new(
                    EditorAuthoringReadiness::Complete,
                    EditorRuntimeAvailability::TrustedInProcess,
                ),
                Default::default(),
                EditorPlayView::new(EditorPlayState::Empty, None, None, None, false, None),
                EditorPersistenceView::Idle,
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
        let attempt = nara::tooling::EditorPersistenceAttemptId::new(source.revision(), 1);
        for (operation, editable) in [
            (None, true),
            (
                Some(EditorPersistenceOperation::Saving {
                    document,
                    captured_revision: revision,
                }),
                true,
            ),
            (
                Some(EditorPersistenceOperation::Opening { document }),
                false,
            ),
        ] {
            let frame = clock.publish(
                workspace.model(&frozen_empty_registry(), None),
                EditorProjectView::new(
                    EditorProjectReadinessView::new(
                        EditorAuthoringReadiness::Complete,
                        EditorRuntimeAvailability::TrustedInProcess,
                    ),
                    Default::default(),
                    EditorPlayView::new(EditorPlayState::Empty, None, None, None, false, None),
                    match operation {
                        Some(operation) => EditorPersistenceView::Pending { attempt, operation },
                        None => EditorPersistenceView::Idle,
                    },
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
