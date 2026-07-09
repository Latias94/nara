//! egui adapter for nara's UI-agnostic editor and inspector models.
//!
//! This crate renders tooling models and returns tooling commands. It does not
//! own scene mutation, ECS storage, windowing, or GPU resources.

use std::collections::BTreeMap;

use egui::{Button, CollapsingHeader, RichText, ScrollArea, TextEdit, Ui};
use nara_reflect::{ComponentFieldPath, ComponentTypeId, ComponentValue, ComponentValueKind};
use nara_scene::SceneEntityId;
use nara_tooling::{
    SceneEditorMode, SceneEditorModel, SceneInspectorCommand, SceneInspectorComponentView,
    SceneInspectorFieldState, SceneInspectorFieldView, SceneInspectorModel,
};

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

    pub fn show(&mut self, ui: &mut Ui, model: &SceneEditorModel) -> EguiSceneEditorPanelResponse {
        let mut response = EguiSceneEditorPanelResponse::default();
        render_editor_toolbar(ui, model.mode, &mut response);
        ui.separator();

        let inspector_response = self
            .inspector
            .show(ui, &model.inspector, model.mode.is_edit());
        response.extend_inspector(inspector_response);

        response
    }
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct EguiSceneEditorPanelResponse {
    pub action: Option<EguiSceneEditorAction>,
    pub inspector_commands: Vec<SceneInspectorCommand>,
}

impl EguiSceneEditorPanelResponse {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.action.is_none() && self.inspector_commands.is_empty()
    }

    fn request_action(&mut self, action: EguiSceneEditorAction) {
        self.action = Some(action);
    }

    fn extend_inspector(&mut self, response: EguiSceneInspectorPanelResponse) {
        self.inspector_commands.extend(response.inspector_commands);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EguiSceneEditorAction {
    StartPlay,
    PausePlay,
    ResumePlay,
    StopPlay,
    ApplyChanges,
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
                if !component.schema_known {
                    ui.label(RichText::new("Schema unavailable").weak());
                    ui.monospace(component_value_label(&component.raw_value));
                    return;
                }

                if component.fields.is_empty() {
                    ui.label(RichText::new("No inspectable fields").weak());
                    return;
                }

                for field in &component.fields {
                    self.show_field(
                        ui,
                        entity,
                        &component.component,
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
        field: &SceneInspectorFieldView,
        editing_enabled: bool,
        response: &mut EguiSceneInspectorPanelResponse,
    ) {
        let key = EguiInspectorFieldKey::new(entity, component, &field.path);
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
                        field,
                        editing_enabled,
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
                        field,
                        editing_enabled,
                        response,
                    );
                }
                ComponentValueKind::Null
                | ComponentValueKind::List
                | ComponentValueKind::Map
                | ComponentValueKind::AssetRef => {
                    ui.monospace(field_value_label(field));
                }
            }

            if editing_enabled && field.value.is_some() && !field.required {
                if ui.small_button("Remove").clicked() {
                    response.push_command(SceneInspectorCommand::RemoveField {
                        entity: entity.clone(),
                        component: component.clone(),
                        path: field.path.clone(),
                    });
                }
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
                path: field.path.clone(),
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
                &field.path,
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
    pub inspector_commands: Vec<SceneInspectorCommand>,
}

impl EguiSceneInspectorPanelResponse {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inspector_commands.is_empty()
    }

    fn push_command(&mut self, command: SceneInspectorCommand) {
        self.inspector_commands.push(command);
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct EguiInspectorFieldKey {
    entity: String,
    component: String,
    path: String,
}

impl EguiInspectorFieldKey {
    fn new(entity: &SceneEntityId, component: &ComponentTypeId, path: &ComponentFieldPath) -> Self {
        Self {
            entity: entity.as_str().to_owned(),
            component: component.as_str().to_owned(),
            path: path.to_string(),
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
    mode: SceneEditorMode,
    response: &mut EguiSceneEditorPanelResponse,
) {
    ui.horizontal(|ui| {
        ui.label(format!("Mode: {}", editor_mode_label(mode)));

        if ui
            .add_enabled(mode.is_edit(), Button::new("Start"))
            .clicked()
        {
            response.request_action(EguiSceneEditorAction::StartPlay);
        }
        if ui
            .add_enabled(mode.is_play(), Button::new("Pause"))
            .clicked()
        {
            response.request_action(EguiSceneEditorAction::PausePlay);
        }
        if ui
            .add_enabled(mode.is_paused(), Button::new("Resume"))
            .clicked()
        {
            response.request_action(EguiSceneEditorAction::ResumePlay);
        }
        if ui
            .add_enabled(!mode.is_edit(), Button::new("Stop"))
            .clicked()
        {
            response.request_action(EguiSceneEditorAction::StopPlay);
        }
        if ui
            .add_enabled(!mode.is_edit(), Button::new("Apply Changes"))
            .clicked()
        {
            response.request_action(EguiSceneEditorAction::ApplyChanges);
        }
    });
}

fn editor_mode_label(mode: SceneEditorMode) -> &'static str {
    match mode {
        SceneEditorMode::Edit => "Edit",
        SceneEditorMode::Play { .. } => "Play",
        SceneEditorMode::Paused { .. } => "Paused",
    }
}

fn entity_row_label(entity: &nara_tooling::SceneInspectorEntityRow) -> String {
    let prefab_marker = if entity.has_prefab { " prefab" } else { "" };
    format!(
        "{} ({} components{})",
        entity.id.as_str(),
        entity.component_count,
        prefab_marker
    )
}

fn component_title(component: &SceneInspectorComponentView) -> String {
    match &component.rust_type_path {
        Some(type_path) => format!("{} ({type_path})", component.component.as_str()),
        None => component.component.as_str().to_owned(),
    }
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
        | ComponentValueKind::AssetRef => None,
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
        | ComponentValueKind::AssetRef => "",
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
        | ComponentValueKind::AssetRef => Err(format!(
            "{} fields are read-only in this panel",
            value_kind_label(kind)
        )),
    }
}

fn editable_set_field_command(
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    path: &ComponentFieldPath,
    kind: ComponentValueKind,
    text: &str,
) -> Result<SceneInspectorCommand, String> {
    Ok(SceneInspectorCommand::SetField {
        entity: entity.clone(),
        component: component.clone(),
        path: path.clone(),
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
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use nara_diagnostic::DiagnosticReport;
    use nara_reflect::ComponentSchemaCatalog;
    use nara_scene::SceneAuthoringHistoryStatus;

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
        let path = ComponentFieldPath::from_fields(["display_name"]);

        let command = editable_set_field_command(
            &entity,
            &component,
            &path,
            ComponentValueKind::String,
            "Hero",
        )
        .unwrap();

        assert_eq!(
            command,
            SceneInspectorCommand::SetField {
                entity,
                component,
                path,
                value: ComponentValue::String("Hero".to_owned()),
            }
        );
    }

    #[test]
    fn renders_empty_editor_model_without_window_backend() {
        let mut panel = EguiSceneEditorPanel::new();
        let ctx = egui::Context::default();
        let model = SceneEditorModel {
            mode: SceneEditorMode::Edit,
            inspector: SceneInspectorModel {
                selected_entity: None,
                entities: Vec::new(),
                selected_entity_view: None,
                schema_catalog: ComponentSchemaCatalog::default(),
                world_snapshot: None,
                history: SceneAuthoringHistoryStatus::default(),
                live_dirty: false,
                diagnostics: DiagnosticReport::default(),
            },
            play_world_snapshot: None,
            diagnostics: DiagnosticReport::default(),
        };

        let mut response = None;
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            response = Some(panel.show(ui, &model));
        });

        assert!(response.unwrap().is_empty());
    }
}
