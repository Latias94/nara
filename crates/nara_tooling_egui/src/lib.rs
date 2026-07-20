//! egui adapter for nara's UI-agnostic editor and inspector models.
//!
//! This crate renders tooling models and returns tooling commands. It does not
//! own scene mutation, ECS storage, windowing, or GPU resources.

use std::collections::BTreeMap;

#[cfg(test)]
use std::cell::RefCell;

use egui::{Button, CollapsingHeader, RichText, ScrollArea, TextEdit, Ui, Vec2, Window};
use nara_identity::EntityReference;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind,
};
use nara_scene::SceneEntityId;
use nara_tooling::{
    EditorApplyChangesResult, EditorCloseDecision, EditorPersistenceCommand,
    EditorPersistenceOperation, EditorPersistenceResult, EditorPlayCommand,
    EditorPlayOperationResult, EditorPlayState, EditorProjectView, EditorRuntimeEditRequest,
    EditorRuntimeEditResult, EditorSceneModel, EditorWorkspaceCommand, EditorWorkspaceIntent,
    EditorWorkspaceIntentPhase, SceneApplyChangesRequest, SceneInspectorCommand,
    SceneInspectorComponentView, SceneInspectorFieldState, SceneInspectorFieldView,
    SceneInspectorModel,
};

#[cfg(test)]
thread_local! {
    static TEST_CONTROL_RECTS: RefCell<BTreeMap<&'static str, egui::Rect>> =
        RefCell::new(BTreeMap::new());
}

#[derive(Debug, Default)]
pub struct EguiSceneEditorPanel {
    inspector: EguiSceneInspectorPanel,
}

impl EguiSceneEditorPanel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn inspector(&self) -> &EguiSceneInspectorPanel {
        &self.inspector
    }

    #[must_use]
    pub fn inspector_mut(&mut self) -> &mut EguiSceneInspectorPanel {
        &mut self.inspector
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        scene: Option<&EditorSceneModel>,
        project: &EditorProjectView,
    ) -> EguiSceneEditorPanelResponse {
        #[cfg(test)]
        TEST_CONTROL_RECTS.with(|rects| rects.borrow_mut().clear());
        let mut response = EguiSceneEditorPanelResponse::default();
        render_editor_toolbar(ui, scene, project, &mut response);
        ui.separator();

        if let Some(scene) = scene {
            let play = project.play();
            let workspace_pending = project.workspace_intent().intent().is_some();
            let authoring_edit = play.state() == EditorPlayState::Empty && !workspace_pending;
            let runtime_edit = matches!(
                play.state(),
                EditorPlayState::Running | EditorPlayState::Paused
            ) && !workspace_pending
                && project.runtime_edit_result().is_none();
            let inspector_response =
                self.inspector
                    .show(ui, &scene.editor.inspector, authoring_edit || runtime_edit);
            response.extend_inspector(inspector_response, play, runtime_edit);
        } else {
            ui.label(RichText::new("No active scene").weak());
        }

        render_dirty_close_dialog(ui, project, &mut response);

        response
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EguiSceneEditorPanelResponse {
    pub workspace_commands: Vec<EditorWorkspaceCommand>,
    pub play_commands: Vec<EditorPlayCommand>,
    pub persistence_commands: Vec<EditorPersistenceCommand>,
    pub workspace_intents: Vec<EditorWorkspaceIntent>,
    pub close_decisions: Vec<EditorCloseDecision>,
    pub runtime_edits: Vec<EditorRuntimeEditRequest>,
    pub apply_changes: Vec<SceneApplyChangesRequest>,
    pub acknowledge_runtime_edit_result: bool,
    pub acknowledge_apply_changes_result: bool,
}

impl EguiSceneEditorPanelResponse {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.workspace_commands.is_empty()
            && self.play_commands.is_empty()
            && self.persistence_commands.is_empty()
            && self.workspace_intents.is_empty()
            && self.close_decisions.is_empty()
            && self.runtime_edits.is_empty()
            && self.apply_changes.is_empty()
            && !self.acknowledge_runtime_edit_result
            && !self.acknowledge_apply_changes_result
    }

    fn extend_inspector(
        &mut self,
        response: EguiSceneInspectorPanelResponse,
        play: nara_tooling::EditorPlayView,
        runtime_edit: bool,
    ) {
        for command in response.workspace_commands {
            let EditorWorkspaceCommand::ApplyInspectorCommand {
                command:
                    SceneInspectorCommand::SetField {
                        entity,
                        component,
                        component_version,
                        field,
                        value,
                    },
                ..
            } = command
            else {
                self.workspace_commands.push(command);
                continue;
            };
            if runtime_edit {
                if let (Some(generation), Some(document_revision)) =
                    (play.generation(), play.current_revision())
                {
                    self.runtime_edits.push(EditorRuntimeEditRequest {
                        generation,
                        document_revision,
                        entity,
                        component,
                        component_version,
                        field,
                        value,
                    });
                }
            } else {
                self.workspace_commands
                    .push(EditorWorkspaceCommand::ApplyInspectorCommand {
                        document: None,
                        command: SceneInspectorCommand::SetField {
                            entity,
                            component,
                            component_version,
                            field,
                            value,
                        },
                    });
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct EguiSceneInspectorPanel {
    field_buffers: BTreeMap<EguiInspectorFieldKey, FieldEditBuffer>,
    field_errors: BTreeMap<EguiInspectorFieldKey, String>,
}

impl EguiSceneInspectorPanel {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn show(
        &mut self,
        ui: &mut Ui,
        model: &SceneInspectorModel,
        editing_enabled: bool,
    ) -> EguiSceneInspectorPanelResponse {
        let mut response = EguiSceneInspectorPanelResponse::default();

        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.heading("Scene");
                self.show_entity_list(ui, model, &mut response);
            });

            ui.separator();

            ui.vertical(|ui| {
                ui.heading("Inspector");
                self.show_entity_view(ui, model, editing_enabled, &mut response);
            });
        });

        response
    }

    fn show_entity_list(
        &mut self,
        ui: &mut Ui,
        model: &SceneInspectorModel,
        response: &mut EguiSceneInspectorPanelResponse,
    ) {
        ScrollArea::vertical()
            .id_salt("nara_tooling_egui_entity_list")
            .max_height(280.0)
            .show(ui, |ui| {
                if model.entities.is_empty() {
                    ui.label(RichText::new("No entities").weak());
                    return;
                }

                for entity in &model.entities {
                    let label = entity_row_label(entity);
                    if ui.selectable_label(entity.selected, label).clicked() {
                        response.push_command(SceneInspectorCommand::SelectEntity {
                            entity: Some(entity.id.clone()),
                        });
                    }
                }
            });
    }

    fn show_entity_view(
        &mut self,
        ui: &mut Ui,
        model: &SceneInspectorModel,
        editing_enabled: bool,
        response: &mut EguiSceneInspectorPanelResponse,
    ) {
        let Some(view) = &model.selected_entity_view else {
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
            self.show_component(ui, &view.id, component, editing_enabled, response);
        }
    }

    fn show_component(
        &mut self,
        ui: &mut Ui,
        entity: &SceneEntityId,
        component: &SceneInspectorComponentView,
        editing_enabled: bool,
        response: &mut EguiSceneInspectorPanelResponse,
    ) {
        let title = component_title(component);
        CollapsingHeader::new(title)
            .default_open(true)
            .show(ui, |ui| {
                if component.fields.is_empty() {
                    ui.label(RichText::new("No inspectable fields").weak());
                    return;
                }

                for field in &component.fields {
                    self.show_field(
                        ui,
                        entity,
                        &component.component,
                        component.schema_version,
                        field,
                        editing_enabled,
                        response,
                    );
                }
            });
    }

    fn show_field(
        &mut self,
        ui: &mut Ui,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
        component_version: ComponentSchemaVersion,
        field: &SceneInspectorFieldView,
        editing_enabled: bool,
        response: &mut EguiSceneInspectorPanelResponse,
    ) {
        let key = EguiInspectorFieldKey::new(entity, component, &field.id);
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
                    self.show_bool_field(
                        ui,
                        &key,
                        entity,
                        component,
                        component_version,
                        field,
                        field_editing_enabled,
                        response,
                    );
                }
                ComponentValueKind::I64
                | ComponentValueKind::U64
                | ComponentValueKind::F64
                | ComponentValueKind::String => {
                    self.show_text_field(
                        ui,
                        &key,
                        entity,
                        component,
                        component_version,
                        field,
                        field_editing_enabled,
                        response,
                    );
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
                response.push_command(SceneInspectorCommand::RemoveField {
                    entity: entity.clone(),
                    component: component.clone(),
                    component_version,
                    field: field.id.clone(),
                });
            }
        });

        if let SceneInspectorFieldState::InvalidPath(reason) = &field.state {
            ui.label(RichText::new(reason).weak());
        }

        if let Some(error) = self.field_errors.get(&key) {
            ui.label(RichText::new(error).color(ui.visuals().error_fg_color));
        }
    }

    fn show_bool_field(
        &mut self,
        ui: &mut Ui,
        key: &EguiInspectorFieldKey,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
        component_version: ComponentSchemaVersion,
        field: &SceneInspectorFieldView,
        editing_enabled: bool,
        response: &mut EguiSceneInspectorPanelResponse,
    ) {
        let mut value = field
            .value
            .as_ref()
            .or(field.default_value.as_ref())
            .and_then(ComponentValue::as_bool)
            .unwrap_or(false);
        let checkbox = ui.add_enabled(editing_enabled, egui::Checkbox::without_text(&mut value));
        if checkbox.changed() {
            self.field_errors.remove(key);
            response.push_command(SceneInspectorCommand::SetField {
                entity: entity.clone(),
                component: component.clone(),
                component_version,
                field: field.id.clone(),
                value: ComponentValue::Bool(value),
            });
        }
    }

    fn show_text_field(
        &mut self,
        ui: &mut Ui,
        key: &EguiInspectorFieldKey,
        entity: &SceneEntityId,
        component: &ComponentTypeId,
        component_version: ComponentSchemaVersion,
        field: &SceneInspectorFieldView,
        editing_enabled: bool,
        response: &mut EguiSceneInspectorPanelResponse,
    ) {
        let source_text = field_edit_source_text(field);
        let buffer_text = self.field_buffer(key, source_text);
        ui.add_enabled(
            editing_enabled,
            TextEdit::singleline(buffer_text).desired_width(160.0),
        );

        if ui
            .add_enabled(editing_enabled, Button::new("Apply"))
            .clicked()
        {
            match editable_set_field_command(
                entity,
                component,
                component_version,
                &field.id,
                field.value_kind,
                buffer_text,
            ) {
                Ok(command) => {
                    self.field_errors.remove(key);
                    response.push_command(command);
                }
                Err(error) => {
                    self.field_errors.insert(key.clone(), error);
                }
            }
        }
    }

    fn field_buffer(&mut self, key: &EguiInspectorFieldKey, source_text: String) -> &mut String {
        let buffer = self
            .field_buffers
            .entry(key.clone())
            .or_insert_with(|| FieldEditBuffer {
                text: source_text.clone(),
                source_text: source_text.clone(),
            });

        if buffer.source_text != source_text {
            if buffer.text == buffer.source_text {
                buffer.text = source_text.clone();
            }
            buffer.source_text = source_text;
        }

        &mut buffer.text
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EguiSceneInspectorPanelResponse {
    pub workspace_commands: Vec<EditorWorkspaceCommand>,
}

impl EguiSceneInspectorPanelResponse {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.workspace_commands.is_empty()
    }

    fn push_command(&mut self, command: SceneInspectorCommand) {
        let command = match command {
            SceneInspectorCommand::SelectEntity { entity } => {
                EditorWorkspaceCommand::SelectEntity {
                    document: None,
                    entity,
                }
            }
            command => EditorWorkspaceCommand::ApplyInspectorCommand {
                document: None,
                command,
            },
        };
        self.workspace_commands.push(command);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct EguiInspectorFieldKey {
    entity: String,
    component: String,
    field: String,
}

impl EguiInspectorFieldKey {
    fn new(entity: &SceneEntityId, component: &ComponentTypeId, field: &ComponentFieldId) -> Self {
        Self {
            entity: entity.as_str().to_owned(),
            component: component.as_str().to_owned(),
            field: field.as_str().to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct FieldEditBuffer {
    text: String,
    source_text: String,
}

fn render_editor_toolbar(
    ui: &mut Ui,
    scene: Option<&EditorSceneModel>,
    project: &EditorProjectView,
    response: &mut EguiSceneEditorPanelResponse,
) {
    let play = project.play();
    let persistence = project.persistence();
    let workspace_intent = project.workspace_intent();
    let state = play.state();
    let workspace_pending = workspace_intent.intent().is_some();
    let persistence_idle = persistence.operation() == EditorPersistenceOperation::Idle;
    let persistence_result_clear = persistence.result().is_none();
    let has_scene = scene.is_some();
    let play_controls_available = !workspace_pending;

    ui.horizontal(|ui| {
        ui.label(RichText::new(play_state_label(state)).strong());
        if play.is_out_of_date() {
            ui.label(RichText::new("Out of date").color(ui.visuals().warn_fg_color));
        }
        if let Some(result) = play.result() {
            ui.label(RichText::new(play_result_label(result)).weak());
        }
    });

    ui.horizontal_wrapped(|ui| {
        if toolbar_button(
            ui,
            state == EditorPlayState::Empty
                && has_scene
                && play_controls_available
                && persistence_idle,
            "Play",
        ) {
            response.play_commands.push(EditorPlayCommand::Play);
        }
        if toolbar_button(
            ui,
            matches!(
                state,
                EditorPlayState::PreparingPlay | EditorPlayState::Starting
            ) && play_controls_available,
            "Cancel",
        ) {
            response.play_commands.push(EditorPlayCommand::Cancel);
        }
        if toolbar_button(
            ui,
            state == EditorPlayState::Running && play_controls_available,
            "Pause",
        ) {
            response.play_commands.push(EditorPlayCommand::Pause);
        }
        if toolbar_button(
            ui,
            state == EditorPlayState::Paused && play_controls_available,
            "Resume",
        ) {
            response.play_commands.push(EditorPlayCommand::Resume);
        }
        if toolbar_button(
            ui,
            state == EditorPlayState::Paused && play_controls_available,
            "Step",
        ) {
            response
                .play_commands
                .push(EditorPlayCommand::StepFixedTick);
        }
        if toolbar_button(
            ui,
            matches!(
                state,
                EditorPlayState::Running | EditorPlayState::Paused | EditorPlayState::Faulted
            ) && play_controls_available,
            "Stop",
        ) {
            response.play_commands.push(EditorPlayCommand::Stop);
        }
        if toolbar_button(
            ui,
            matches!(
                state,
                EditorPlayState::Running | EditorPlayState::Paused | EditorPlayState::Faulted
            ) && play_controls_available,
            "Restart",
        ) {
            response.play_commands.push(EditorPlayCommand::Restart);
        }
        if toolbar_button(
            ui,
            state == EditorPlayState::RetirementIncomplete
                && workspace_intent
                    .phase()
                    .is_none_or(|phase| phase == EditorWorkspaceIntentPhase::RetiringRuntime),
            "Retry retire",
        ) {
            response
                .play_commands
                .push(EditorPlayCommand::RetryRetirement);
        }
        if toolbar_button(
            ui,
            state == EditorPlayState::CloseIncomplete
                && workspace_intent
                    .phase()
                    .is_none_or(|phase| phase == EditorWorkspaceIntentPhase::RetiringRuntime),
            "Retry close",
        ) {
            response.play_commands.push(EditorPlayCommand::RetryClose);
        }
    });

    ui.horizontal_wrapped(|ui| {
        let save_enabled = scene.is_some_and(|scene| scene.dirty)
            && persistence_idle
            && persistence_result_clear
            && !workspace_pending;
        if toolbar_button(ui, save_enabled, "Save") {
            response
                .persistence_commands
                .push(EditorPersistenceCommand::Save {
                    document: scene.map(|scene| scene.document),
                });
        }
        let reconcile = matches!(
            persistence.result(),
            Some(EditorPersistenceResult::PersistenceUncertain { .. })
        );
        let reopen_enabled = state == EditorPlayState::Empty
            && persistence_idle
            && !workspace_pending
            && (persistence_result_clear || reconcile);
        if toolbar_button(ui, reopen_enabled, "Reopen") {
            response
                .persistence_commands
                .push(EditorPersistenceCommand::Reopen {
                    document: scene.map(|scene| scene.document),
                });
        }
        if toolbar_button(
            ui,
            persistence.result().is_some() && persistence_idle,
            "Dismiss result",
        ) {
            response
                .persistence_commands
                .push(EditorPersistenceCommand::AcknowledgeResult);
        }
        if toolbar_button(
            ui,
            has_scene && persistence_idle && persistence_result_clear && !workspace_pending,
            "Close",
        ) {
            if let Some(scene) = scene {
                response
                    .workspace_intents
                    .push(EditorWorkspaceIntent::CloseScene {
                        document: scene.document,
                    });
            }
        }
        if toolbar_button(
            ui,
            persistence_idle && persistence_result_clear && !workspace_pending,
            "Exit",
        ) {
            response.workspace_intents.push(EditorWorkspaceIntent::Exit);
        }
        if toolbar_button(ui, play.result().is_some(), "Dismiss Play") {
            response
                .play_commands
                .push(EditorPlayCommand::AcknowledgeResult);
        }
    });

    let apply_request = scene.and_then(apply_changes_request);
    let apply_enabled = matches!(state, EditorPlayState::Running | EditorPlayState::Paused)
        && !play.is_out_of_date()
        && !workspace_pending
        && project.apply_changes_result().is_none()
        && apply_request.is_some();
    let apply_response = ui
        .add_enabled(
            apply_enabled,
            Button::new("Apply Changes").min_size(Vec2::new(112.0, 24.0)),
        )
        .on_hover_text("Persist the selected runtime values into the edit document");
    #[cfg(test)]
    TEST_CONTROL_RECTS.with(|rects| {
        rects
            .borrow_mut()
            .insert("Apply Changes", apply_response.rect);
    });
    if apply_response.clicked() {
        if let Some(request) = apply_request {
            response.apply_changes.push(request);
        }
    }

    if let Some(result) = project.runtime_edit_result() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(runtime_edit_result_label(result)).weak());
            let can_acknowledge = !matches!(result, EditorRuntimeEditResult::Pending(_));
            if toolbar_button(ui, can_acknowledge, "Dismiss edit") {
                response.acknowledge_runtime_edit_result = true;
            }
        });
    }
    if let Some(result) = project.apply_changes_result() {
        ui.horizontal(|ui| {
            ui.label(RichText::new(apply_changes_result_label(result)).weak());
            let can_acknowledge = !matches!(result, EditorApplyChangesResult::Pending { .. });
            if toolbar_button(ui, can_acknowledge, "Dismiss apply") {
                response.acknowledge_apply_changes_result = true;
            }
        });
    }

    if let Some(operation) = persistence_operation_label(persistence.operation()) {
        ui.label(RichText::new(operation).weak());
    }
    if let Some(result) = persistence.result() {
        ui.label(RichText::new(persistence_result_label(result)).weak());
    }
    if let (Some(intent), Some(phase)) = (workspace_intent.intent(), workspace_intent.phase()) {
        ui.label(RichText::new(workspace_intent_label(intent, phase)).weak());
    }
}

fn toolbar_button(ui: &mut Ui, enabled: bool, label: &'static str) -> bool {
    let response = ui.add_enabled(enabled, Button::new(label).min_size(Vec2::new(80.0, 24.0)));
    #[cfg(test)]
    TEST_CONTROL_RECTS.with(|rects| {
        rects.borrow_mut().insert(label, response.rect);
    });
    response.clicked()
}

fn render_dirty_close_dialog(
    ui: &mut Ui,
    project: &EditorProjectView,
    response: &mut EguiSceneEditorPanelResponse,
) {
    let workspace = project.workspace_intent();
    if workspace.phase() != Some(EditorWorkspaceIntentPhase::AwaitingDecision) {
        return;
    }
    let persistence = project.persistence();
    let save_enabled = persistence.operation() == EditorPersistenceOperation::Idle
        && persistence.result().is_none();
    Window::new("Unsaved changes")
        .collapsible(false)
        .resizable(false)
        .show(ui.ctx(), |ui| {
            ui.label("Save changes before closing?");
            ui.horizontal(|ui| {
                if dialog_button(ui, save_enabled, "Save", "Close Save") {
                    response.close_decisions.push(EditorCloseDecision::Save);
                }
                if dialog_button(ui, true, "Discard", "Close Discard") {
                    response.close_decisions.push(EditorCloseDecision::Discard);
                }
                if dialog_button(ui, true, "Cancel", "Close Cancel") {
                    response.close_decisions.push(EditorCloseDecision::Cancel);
                }
            });
        });
}

fn dialog_button(ui: &mut Ui, enabled: bool, label: &'static str, _test_id: &'static str) -> bool {
    let response = ui.add_enabled(enabled, Button::new(label).min_size(Vec2::new(80.0, 24.0)));
    #[cfg(test)]
    TEST_CONTROL_RECTS.with(|rects| {
        rects.borrow_mut().insert(_test_id, response.rect);
    });
    response.clicked()
}

fn apply_changes_request(scene: &EditorSceneModel) -> Option<SceneApplyChangesRequest> {
    let selected = scene.editor.inspector.selected_entity_view.as_ref()?;
    let components = selected
        .components
        .iter()
        .filter(|component| {
            component.capabilities.contains(&ComponentCapability::Scene)
                && component.capabilities.contains(&ComponentCapability::Edit)
        })
        .map(|component| component.component.clone())
        .collect::<Vec<_>>();
    (!components.is_empty()).then(|| SceneApplyChangesRequest::new(selected.id.clone(), components))
}

fn play_state_label(state: EditorPlayState) -> &'static str {
    match state {
        EditorPlayState::Empty => "Edit",
        EditorPlayState::PreparingPlay => "Preparing Play",
        EditorPlayState::Starting => "Starting",
        EditorPlayState::RetiringPlay => "Retiring Play",
        EditorPlayState::Running => "Running",
        EditorPlayState::Paused => "Paused",
        EditorPlayState::Stepping => "Stepping",
        EditorPlayState::Stopping => "Stopping",
        EditorPlayState::Faulted => "Faulted",
        EditorPlayState::RetirementIncomplete => "Retirement incomplete",
        EditorPlayState::CloseIncomplete => "Close incomplete",
    }
}

fn play_result_label(result: EditorPlayOperationResult) -> &'static str {
    match result {
        EditorPlayOperationResult::Pending { .. } => "Pending",
        EditorPlayOperationResult::Applied { .. } => "Applied",
        EditorPlayOperationResult::Rejected { .. } => "Rejected",
        EditorPlayOperationResult::Failed { .. } => "Failed",
        EditorPlayOperationResult::Cancelled { .. } => "Cancelled",
    }
}

fn persistence_operation_label(operation: EditorPersistenceOperation) -> Option<&'static str> {
    match operation {
        EditorPersistenceOperation::Idle => None,
        EditorPersistenceOperation::Saving { .. } => Some("Saving"),
        EditorPersistenceOperation::Opening { .. } => Some("Opening"),
    }
}

fn persistence_result_label(result: EditorPersistenceResult) -> &'static str {
    match result {
        EditorPersistenceResult::Saved { .. } => "Saved",
        EditorPersistenceResult::Opened { .. } => "Opened",
        EditorPersistenceResult::Rejected { .. } => "Persistence rejected",
        EditorPersistenceResult::Failed { .. } => "Persistence failed",
        EditorPersistenceResult::PersistenceUncertain { .. } => "Persistence uncertain",
    }
}

fn runtime_edit_result_label(result: &EditorRuntimeEditResult) -> &'static str {
    match result {
        EditorRuntimeEditResult::Pending(_) => "Runtime edit pending",
        EditorRuntimeEditResult::Applied(_) => "Runtime edit applied",
        EditorRuntimeEditResult::Rejected { .. } => "Runtime edit rejected",
        EditorRuntimeEditResult::Cancelled(_) => "Runtime edit cancelled",
    }
}

fn apply_changes_result_label(result: &EditorApplyChangesResult) -> &'static str {
    match result {
        EditorApplyChangesResult::Pending { .. } => "Apply Changes pending",
        EditorApplyChangesResult::Applied(_) => "Apply Changes applied",
        EditorApplyChangesResult::Rejected { .. } => "Apply Changes rejected",
        EditorApplyChangesResult::Cancelled(_) => "Apply Changes cancelled",
    }
}

fn workspace_intent_label(
    intent: EditorWorkspaceIntent,
    phase: EditorWorkspaceIntentPhase,
) -> &'static str {
    match (intent, phase) {
        (_, EditorWorkspaceIntentPhase::AwaitingDecision) => "Awaiting close decision",
        (_, EditorWorkspaceIntentPhase::Saving) => "Saving before close",
        (EditorWorkspaceIntent::CloseScene { .. }, EditorWorkspaceIntentPhase::RetiringRuntime) => {
            "Closing scene"
        }
        (EditorWorkspaceIntent::Exit, EditorWorkspaceIntentPhase::RetiringRuntime) => "Exiting",
    }
}

fn entity_row_label(entity: &nara_tooling::SceneInspectorEntityRow) -> String {
    let prefab_marker = if entity.has_prefab { " prefab" } else { "" };
    format!(
        "{} ({} components{})",
        entity.id.as_str(),
        entity.inspectable_component_count,
        prefab_marker
    )
}

fn component_title(component: &SceneInspectorComponentView) -> String {
    component.component.as_str().to_owned()
}

fn field_value_label(field: &SceneInspectorFieldView) -> String {
    match field.value.as_ref().or(field.default_value.as_ref()) {
        Some(value) => component_value_label(value),
        None => "<missing>".to_owned(),
    }
}

fn component_value_label(value: &ComponentValue) -> String {
    match value {
        ComponentValue::Null => "null".to_owned(),
        ComponentValue::Bool(value) => value.to_string(),
        ComponentValue::I64(value) => value.to_string(),
        ComponentValue::U64(value) => value.to_string(),
        ComponentValue::F64(value) => value.get().to_string(),
        ComponentValue::String(value) => value.clone(),
        ComponentValue::List(values) => format!("[{} items]", values.len()),
        ComponentValue::Map(fields) => format!("{{{} fields}}", fields.len()),
        ComponentValue::EntityReference(reference) => entity_reference_label(reference),
    }
}

fn entity_reference_label(reference: &EntityReference) -> String {
    match reference {
        EntityReference::SceneLocal { entity } => format!("scene-local:{}", entity.as_str()),
        EntityReference::Persistent { entity } => {
            format!("{}:{}", entity.namespace.as_str(), entity.entity)
        }
    }
}

fn field_edit_source_text(field: &SceneInspectorFieldView) -> String {
    field
        .value
        .as_ref()
        .or(field.default_value.as_ref())
        .and_then(|value| editable_value_text(field.value_kind, value))
        .unwrap_or_else(|| empty_editable_text(field.value_kind).to_owned())
}

fn editable_value_text(kind: ComponentValueKind, value: &ComponentValue) -> Option<String> {
    match kind {
        ComponentValueKind::Bool => value.as_bool().map(|value| value.to_string()),
        ComponentValueKind::I64 => value.as_i64().map(|value| value.to_string()),
        ComponentValueKind::U64 => value.as_u64().map(|value| value.to_string()),
        ComponentValueKind::F64 => value.as_f64().map(|value| value.to_string()),
        ComponentValueKind::String => value.as_str().map(ToOwned::to_owned),
        ComponentValueKind::Null
        | ComponentValueKind::List
        | ComponentValueKind::Map
        | ComponentValueKind::AssetRef
        | ComponentValueKind::EntityRef => None,
    }
}

fn empty_editable_text(kind: ComponentValueKind) -> &'static str {
    match kind {
        ComponentValueKind::Bool => "false",
        ComponentValueKind::I64 | ComponentValueKind::U64 => "0",
        ComponentValueKind::F64 => "0.0",
        ComponentValueKind::String => "",
        ComponentValueKind::Null
        | ComponentValueKind::List
        | ComponentValueKind::Map
        | ComponentValueKind::AssetRef
        | ComponentValueKind::EntityRef => "",
    }
}

fn parse_editable_value(kind: ComponentValueKind, text: &str) -> Result<ComponentValue, String> {
    match kind {
        ComponentValueKind::Bool => text
            .parse::<bool>()
            .map(ComponentValue::Bool)
            .map_err(|_| "expected true or false".to_owned()),
        ComponentValueKind::I64 => text
            .parse::<i64>()
            .map(ComponentValue::I64)
            .map_err(|_| "expected a signed integer".to_owned()),
        ComponentValueKind::U64 => text
            .parse::<u64>()
            .map(ComponentValue::U64)
            .map_err(|_| "expected an unsigned integer".to_owned()),
        ComponentValueKind::F64 => {
            let value = text
                .parse::<f64>()
                .map_err(|_| "expected a finite float".to_owned())?;
            ComponentValue::f64(value).map_err(|_| "expected a finite float".to_owned())
        }
        ComponentValueKind::String => Ok(ComponentValue::String(text.to_owned())),
        ComponentValueKind::Null
        | ComponentValueKind::List
        | ComponentValueKind::Map
        | ComponentValueKind::AssetRef
        | ComponentValueKind::EntityRef => Err(format!(
            "{} fields are read-only in this panel",
            value_kind_label(kind)
        )),
    }
}

fn editable_set_field_command(
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    component_version: ComponentSchemaVersion,
    field: &ComponentFieldId,
    kind: ComponentValueKind,
    text: &str,
) -> Result<SceneInspectorCommand, String> {
    Ok(SceneInspectorCommand::SetField {
        entity: entity.clone(),
        component: component.clone(),
        component_version,
        field: field.clone(),
        value: parse_editable_value(kind, text)?,
    })
}

fn value_kind_label(kind: ComponentValueKind) -> &'static str {
    match kind {
        ComponentValueKind::Null => "null",
        ComponentValueKind::Bool => "bool",
        ComponentValueKind::I64 => "i64",
        ComponentValueKind::U64 => "u64",
        ComponentValueKind::F64 => "f64",
        ComponentValueKind::String => "string",
        ComponentValueKind::List => "list",
        ComponentValueKind::Map => "map",
        ComponentValueKind::AssetRef => "asset_ref",
        ComponentValueKind::EntityRef => "entity_ref",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use nara_diagnostic::DiagnosticReport;
    use nara_scene::{SceneAuthoringHistoryStatus, SceneAuthoringSession, SceneDocument};
    use nara_tooling::{
        EditorDocumentId, EditorExternalReloadState, EditorPersistenceView, EditorPlayOperation,
        EditorPlayView, EditorSceneModel, EditorSelectionSet, EditorWorkspaceIntentView,
        SceneEditorModel, SceneInspectorEntityView,
    };

    #[test]
    fn parses_editable_scalar_values() {
        assert_eq!(
            parse_editable_value(ComponentValueKind::String, "Hero").unwrap(),
            ComponentValue::String("Hero".to_owned())
        );
        assert_eq!(
            parse_editable_value(ComponentValueKind::Bool, "true").unwrap(),
            ComponentValue::Bool(true)
        );
        assert_eq!(
            parse_editable_value(ComponentValueKind::I64, "-7").unwrap(),
            ComponentValue::I64(-7)
        );
        assert_eq!(
            parse_editable_value(ComponentValueKind::U64, "7").unwrap(),
            ComponentValue::U64(7)
        );
        assert_eq!(
            parse_editable_value(ComponentValueKind::F64, "1.25")
                .unwrap()
                .as_f64(),
            Some(1.25)
        );
    }

    #[test]
    fn rejects_non_finite_float_values() {
        assert!(parse_editable_value(ComponentValueKind::F64, "NaN").is_err());
        assert!(parse_editable_value(ComponentValueKind::F64, "inf").is_err());
    }

    #[test]
    fn builds_set_field_command_for_editable_scalar_value() {
        let entity = SceneEntityId::new("player").unwrap();
        let component = ComponentTypeId::new("nara.test.Name");
        let component_version = ComponentSchemaVersion(1);
        let field = ComponentFieldId::new("display_name");

        let command = editable_set_field_command(
            &entity,
            &component,
            component_version,
            &field,
            ComponentValueKind::String,
            "Hero",
        )
        .unwrap();

        assert_eq!(
            command,
            SceneInspectorCommand::SetField {
                entity,
                component,
                component_version,
                field,
                value: ComponentValue::String("Hero".to_owned()),
            }
        );
    }

    #[test]
    fn renders_entity_references_as_stable_read_only_values() {
        let reference = EntityReference::SceneLocal {
            entity: SceneEntityId::new("player/camera").unwrap(),
        };

        assert_eq!(
            entity_reference_label(&reference),
            "scene-local:player/camera"
        );
        assert!(parse_editable_value(ComponentValueKind::EntityRef, "player/camera").is_err());
        assert_eq!(
            value_kind_label(ComponentValueKind::EntityRef),
            "entity_ref"
        );
    }

    #[test]
    fn renders_empty_editor_model_without_window_backend() {
        let mut panel = EguiSceneEditorPanel::new();
        let ctx = egui::Context::default();

        let mut response = None;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            response = Some(panel.show(ui, None, &empty_project_view(None)));
        });

        assert!(response.unwrap().is_empty());
    }

    #[test]
    fn play_controls_emit_only_commands_valid_for_each_visible_state() {
        let scene = scene_model(false, false);
        let cases = [
            (
                EditorPlayState::Empty,
                "Play",
                Some(EditorPlayCommand::Play),
            ),
            (
                EditorPlayState::PreparingPlay,
                "Cancel",
                Some(EditorPlayCommand::Cancel),
            ),
            (
                EditorPlayState::Starting,
                "Cancel",
                Some(EditorPlayCommand::Cancel),
            ),
            (EditorPlayState::RetiringPlay, "Play", None),
            (
                EditorPlayState::Running,
                "Pause",
                Some(EditorPlayCommand::Pause),
            ),
            (
                EditorPlayState::Running,
                "Stop",
                Some(EditorPlayCommand::Stop),
            ),
            (
                EditorPlayState::Running,
                "Restart",
                Some(EditorPlayCommand::Restart),
            ),
            (
                EditorPlayState::Paused,
                "Resume",
                Some(EditorPlayCommand::Resume),
            ),
            (
                EditorPlayState::Paused,
                "Step",
                Some(EditorPlayCommand::StepFixedTick),
            ),
            (EditorPlayState::Stepping, "Stop", None),
            (EditorPlayState::Stopping, "Restart", None),
            (
                EditorPlayState::Faulted,
                "Stop",
                Some(EditorPlayCommand::Stop),
            ),
            (
                EditorPlayState::Faulted,
                "Restart",
                Some(EditorPlayCommand::Restart),
            ),
            (
                EditorPlayState::RetirementIncomplete,
                "Retry retire",
                Some(EditorPlayCommand::RetryRetirement),
            ),
            (
                EditorPlayState::CloseIncomplete,
                "Retry close",
                Some(EditorPlayCommand::RetryClose),
            ),
        ];

        for (state, control, expected) in cases {
            let response = click_control(Some(&scene), project_view(&scene, state), control);
            assert_eq!(
                response.play_commands.first().copied(),
                expected,
                "unexpected command for {state:?} / {control}"
            );
        }
    }

    #[test]
    fn persistence_and_dirty_close_controls_emit_host_commands() {
        let dirty = scene_model(true, false);
        for (state, out_of_date) in [
            (EditorPlayState::Running, false),
            (EditorPlayState::Paused, false),
            (EditorPlayState::Running, true),
        ] {
            let project = EditorProjectView::new(
                play_view(&dirty, state, out_of_date, None),
                EditorPersistenceView::default(),
                EditorWorkspaceIntentView::default(),
            );
            let save = click_control(Some(&dirty), project, "Save");
            assert_eq!(
                save.persistence_commands,
                [EditorPersistenceCommand::Save {
                    document: Some(dirty.document),
                }]
            );
        }

        let clean = scene_model(false, false);
        assert_eq!(
            click_control(
                Some(&clean),
                project_view(&clean, EditorPlayState::Empty),
                "Close",
            )
            .workspace_intents,
            [EditorWorkspaceIntent::CloseScene {
                document: clean.document,
            }]
        );
        assert_eq!(
            click_control(
                Some(&clean),
                project_view(&clean, EditorPlayState::Empty),
                "Exit",
            )
            .workspace_intents,
            [EditorWorkspaceIntent::Exit]
        );

        let awaiting = EditorProjectView::new(
            play_view(&dirty, EditorPlayState::Running, false, None),
            EditorPersistenceView::default(),
            EditorWorkspaceIntentView::new(
                Some(EditorWorkspaceIntent::CloseScene {
                    document: dirty.document,
                }),
                Some(EditorWorkspaceIntentPhase::AwaitingDecision),
                None,
            ),
        );
        for (control, decision) in [
            ("Close Save", EditorCloseDecision::Save),
            ("Close Discard", EditorCloseDecision::Discard),
            ("Close Cancel", EditorCloseDecision::Cancel),
        ] {
            let response = click_control(Some(&dirty), awaiting.clone(), control);
            assert_eq!(response.close_decisions, [decision]);
            assert!(response.play_commands.is_empty());
        }
    }

    #[test]
    fn retained_results_and_reconcile_have_explicit_clicks() {
        let scene = scene_model(true, false);
        let play_result = EditorProjectView::new(
            play_view(
                &scene,
                EditorPlayState::Running,
                false,
                Some(EditorPlayOperationResult::Applied {
                    operation: EditorPlayOperation::Pause,
                    generation: Some(7),
                }),
            ),
            EditorPersistenceView::default(),
            EditorWorkspaceIntentView::default(),
        );
        assert_eq!(
            click_control(Some(&scene), play_result, "Dismiss Play").play_commands,
            [EditorPlayCommand::AcknowledgeResult]
        );

        let uncertain = EditorProjectView::new(
            play_view(&scene, EditorPlayState::Empty, false, None),
            EditorPersistenceView::new(
                EditorPersistenceOperation::Idle,
                Some(EditorPersistenceResult::PersistenceUncertain {
                    document: scene.document,
                    revision: scene.revision,
                    digest: nara_tooling::EditorDocumentDigest::new(0, [0; 32]),
                }),
            ),
            EditorWorkspaceIntentView::default(),
        );
        assert_eq!(
            click_control(Some(&scene), uncertain, "Reopen").persistence_commands,
            [EditorPersistenceCommand::Reopen {
                document: Some(scene.document),
            }]
        );

        let runtime_request = EditorRuntimeEditRequest {
            generation: 7,
            document_revision: scene.revision,
            entity: SceneEntityId::new("player").unwrap(),
            component: ComponentTypeId::new("nara.test.Transform"),
            component_version: ComponentSchemaVersion(1),
            field: ComponentFieldId::new("x"),
            value: ComponentValue::I64(2),
        };
        let runtime_result = project_view(&scene, EditorPlayState::Running).with_inspector_results(
            Some(EditorRuntimeEditResult::Cancelled(runtime_request.clone())),
            None,
        );
        assert!(
            click_control(Some(&scene), runtime_result, "Dismiss edit")
                .acknowledge_runtime_edit_result
        );
        let runtime_pending = project_view(&scene, EditorPlayState::Running)
            .with_inspector_results(
                Some(EditorRuntimeEditResult::Pending(runtime_request)),
                None,
            );
        assert!(
            !click_control(Some(&scene), runtime_pending, "Dismiss edit")
                .acknowledge_runtime_edit_result
        );

        let apply_request = SceneApplyChangesRequest::new(
            SceneEntityId::new("player").unwrap(),
            [ComponentTypeId::new("nara.test.Transform")],
        );
        let apply_result = project_view(&scene, EditorPlayState::Running).with_inspector_results(
            None,
            Some(EditorApplyChangesResult::Cancelled(apply_request)),
        );
        assert!(
            click_control(Some(&scene), apply_result, "Dismiss apply")
                .acknowledge_apply_changes_result
        );
    }

    #[test]
    fn apply_changes_is_selection_bound_and_disabled_when_runtime_is_stale() {
        let scene = scene_model(false, true);
        let current = click_control(
            Some(&scene),
            project_view(&scene, EditorPlayState::Paused),
            "Apply Changes",
        );
        assert_eq!(current.apply_changes.len(), 1);
        assert_eq!(
            current.apply_changes[0].entity,
            SceneEntityId::new("player").unwrap()
        );

        let stale = EditorProjectView::new(
            play_view(&scene, EditorPlayState::Paused, true, None),
            EditorPersistenceView::default(),
            EditorWorkspaceIntentView::default(),
        );
        assert!(
            click_control(Some(&scene), stale, "Apply Changes")
                .apply_changes
                .is_empty()
        );
    }

    #[test]
    fn runtime_inspector_edits_are_generation_and_revision_stamped() {
        let scene = scene_model(false, false);
        let play = play_view(&scene, EditorPlayState::Running, false, None);
        let command = SceneInspectorCommand::SetField {
            entity: SceneEntityId::new("player").unwrap(),
            component: ComponentTypeId::new("nara.test.Transform"),
            component_version: ComponentSchemaVersion(1),
            field: ComponentFieldId::new("x"),
            value: ComponentValue::I64(4),
        };
        let mut response = EguiSceneEditorPanelResponse::default();
        response.extend_inspector(
            EguiSceneInspectorPanelResponse {
                workspace_commands: vec![EditorWorkspaceCommand::ApplyInspectorCommand {
                    document: Some(scene.document),
                    command,
                }],
            },
            play,
            true,
        );

        assert_eq!(response.runtime_edits.len(), 1);
        assert_eq!(response.runtime_edits[0].generation, 7);
        assert_eq!(response.runtime_edits[0].document_revision, scene.revision);
        assert!(response.workspace_commands.is_empty());
    }

    fn scene_model(dirty: bool, selected_component: bool) -> EditorSceneModel {
        let session = SceneAuthoringSession::new(SceneDocument::default());
        let revision = session.revision();
        let selected_entity = SceneEntityId::new("player").unwrap();
        let selected_entity_view = selected_component.then(|| SceneInspectorEntityView {
            id: selected_entity.clone(),
            parent: None,
            has_prefab: false,
            components: vec![SceneInspectorComponentView {
                component: ComponentTypeId::new("nara.test.Transform"),
                document_version: ComponentSchemaVersion(1),
                schema_version: ComponentSchemaVersion(1),
                capabilities: [ComponentCapability::Scene, ComponentCapability::Edit]
                    .into_iter()
                    .collect(),
                fields: Vec::new(),
            }],
        });
        EditorSceneModel {
            document: EditorDocumentId::from_raw(1),
            title: "Scene".to_owned(),
            dirty,
            revision,
            saved_revision: revision,
            external_reload: EditorExternalReloadState::Clean,
            selection: EditorSelectionSet::default(),
            editor: SceneEditorModel {
                inspector: SceneInspectorModel {
                    selected_entity: selected_component.then_some(selected_entity),
                    entities: Vec::new(),
                    selected_entity_view,
                    world_snapshot: None,
                    history: SceneAuthoringHistoryStatus::default(),
                    live_dirty: false,
                    diagnostics: DiagnosticReport::default(),
                },
                diagnostics: DiagnosticReport::default(),
            },
        }
    }

    fn empty_project_view(scene: Option<&EditorSceneModel>) -> EditorProjectView {
        let current = scene.map(|scene| scene.revision);
        EditorProjectView::new(
            EditorPlayView::new(EditorPlayState::Empty, None, None, current, false, None),
            EditorPersistenceView::default(),
            EditorWorkspaceIntentView::default(),
        )
    }

    fn project_view(scene: &EditorSceneModel, state: EditorPlayState) -> EditorProjectView {
        EditorProjectView::new(
            play_view(scene, state, false, None),
            EditorPersistenceView::default(),
            EditorWorkspaceIntentView::default(),
        )
    }

    fn play_view(
        scene: &EditorSceneModel,
        state: EditorPlayState,
        out_of_date: bool,
        result: Option<EditorPlayOperationResult>,
    ) -> EditorPlayView {
        let active = state != EditorPlayState::Empty;
        let generation = matches!(
            state,
            EditorPlayState::Running
                | EditorPlayState::Paused
                | EditorPlayState::Stepping
                | EditorPlayState::Stopping
                | EditorPlayState::Faulted
                | EditorPlayState::CloseIncomplete
        )
        .then_some(7);
        EditorPlayView::new(
            state,
            generation,
            active.then_some(scene.revision),
            Some(scene.revision),
            out_of_date,
            result,
        )
    }

    fn click_control(
        scene: Option<&EditorSceneModel>,
        project: EditorProjectView,
        label: &'static str,
    ) -> EguiSceneEditorPanelResponse {
        let mut panel = EguiSceneEditorPanel::new();
        let context = egui::Context::default();
        let _ = render_editor_frame(
            &context,
            &mut panel,
            scene,
            &project,
            egui::RawInput::default(),
        );
        let _ = render_editor_frame(
            &context,
            &mut panel,
            scene,
            &project,
            egui::RawInput::default(),
        );
        let position = TEST_CONTROL_RECTS.with(|rects| {
            rects
                .borrow()
                .get(label)
                .unwrap_or_else(|| panic!("control {label} was not rendered"))
                .center()
        });

        let mut pressed = egui::RawInput::default();
        pressed.events = vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: egui::Modifiers::default(),
            },
        ];
        let _ = render_editor_frame(&context, &mut panel, scene, &project, pressed);

        let mut released = egui::RawInput::default();
        released.events = vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::default(),
            },
        ];
        render_editor_frame(&context, &mut panel, scene, &project, released)
    }

    fn render_editor_frame(
        context: &egui::Context,
        panel: &mut EguiSceneEditorPanel,
        scene: Option<&EditorSceneModel>,
        project: &EditorProjectView,
        input: egui::RawInput,
    ) -> EguiSceneEditorPanelResponse {
        let mut response = None;
        let _ = context.run_ui(input, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                response = Some(panel.show(ui, scene, project));
            });
        });
        response.expect("the editor frame should render the panel")
    }
}
