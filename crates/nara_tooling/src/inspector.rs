use nara_asset::{AssetRef, ProjectAssetDatabase};
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_ecs::Entity;
use nara_reflect::{
    ComponentFieldPath, ComponentFieldSchema, ComponentRegistry, ComponentSchema,
    ComponentSchemaCatalog, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind,
};
use nara_scene::{
    SceneAuthoringHistoryStatus, SceneAuthoringSession, SceneComponentRecord, SceneDocument,
    SceneEntityId, ScenePatchDocument, ScenePatchOperation, ScenePatchReport,
};

use crate::snapshot::WorldSnapshot;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SceneInspectorState {
    selected_entity: Option<SceneEntityId>,
}

impl SceneInspectorState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn selected_entity(&self) -> Option<&SceneEntityId> {
        self.selected_entity.as_ref()
    }

    pub fn select_entity(&mut self, entity: Option<SceneEntityId>) {
        self.selected_entity = entity;
    }

    #[must_use]
    pub fn model(
        &self,
        session: &SceneAuthoringSession,
        registry: &ComponentRegistry,
        world_snapshot: Option<&WorldSnapshot>,
    ) -> SceneInspectorModel {
        build_inspector_model(
            session,
            registry,
            self.selected_entity.as_ref(),
            world_snapshot,
        )
    }

    pub fn apply_command(
        &mut self,
        session: &mut SceneAuthoringSession,
        registry: &ComponentRegistry,
        command: SceneInspectorCommand,
    ) -> SceneInspectorCommandReport {
        self.apply_command_with_patch_apply(session, command, |session, patch| {
            session.apply_patch(patch, registry)
        })
    }

    pub fn apply_command_with_asset_database(
        &mut self,
        session: &mut SceneAuthoringSession,
        registry: &ComponentRegistry,
        database: &ProjectAssetDatabase,
        command: SceneInspectorCommand,
    ) -> SceneInspectorCommandReport {
        self.apply_command_with_patch_apply(session, command, |session, patch| {
            session.apply_patch_with_asset_database(patch, registry, database)
        })
    }

    fn apply_command_with_patch_apply(
        &mut self,
        session: &mut SceneAuthoringSession,
        command: SceneInspectorCommand,
        mut apply_patch: impl FnMut(&mut SceneAuthoringSession, &ScenePatchDocument) -> ScenePatchReport,
    ) -> SceneInspectorCommandReport {
        match command {
            SceneInspectorCommand::SelectEntity { entity } => {
                self.apply_selection_command(session.document(), entity)
            }
            command => {
                let Some(entity) = command.target_entity().cloned() else {
                    return inspector_error_report(
                        "tooling.inspector-command-without-target",
                        "inspector command does not target an entity",
                    )
                    .with_entity_selection(self.selected_entity.clone());
                };
                if !document_has_entity(session.document(), &entity) {
                    return inspector_entity_error_report(
                        "tooling.inspector-missing-entity",
                        "inspector command targets an entity that is not in the authoring document",
                        &entity,
                    )
                    .with_entity_selection(self.selected_entity.clone());
                }

                let patch = command
                    .to_patch()
                    .expect("non-selection inspector commands should produce patches");
                let patch_report = apply_patch(session, &patch);
                let applied = patch_report.applied;
                if applied {
                    self.selected_entity = Some(entity);
                }
                SceneInspectorCommandReport {
                    applied,
                    selected_entity: self.selected_entity.clone(),
                    patch: Some(patch),
                    patch_report: Some(patch_report.clone()),
                    diagnostics: patch_report.diagnostics,
                }
            }
        }
    }

    fn apply_selection_command(
        &mut self,
        document: &SceneDocument,
        entity: Option<SceneEntityId>,
    ) -> SceneInspectorCommandReport {
        if let Some(entity) = &entity {
            if !document_has_entity(document, entity) {
                return inspector_entity_error_report(
                    "tooling.inspector-missing-entity",
                    "inspector selection targets an entity that is not in the authoring document",
                    entity,
                )
                .with_entity_selection(self.selected_entity.clone());
            }
        }

        self.selected_entity = entity;
        SceneInspectorCommandReport {
            applied: true,
            selected_entity: self.selected_entity.clone(),
            patch: None,
            patch_report: None,
            diagnostics: DiagnosticReport::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneInspectorModel {
    pub selected_entity: Option<SceneEntityId>,
    pub entities: Vec<SceneInspectorEntityRow>,
    pub selected_entity_view: Option<SceneInspectorEntityView>,
    pub schema_catalog: ComponentSchemaCatalog,
    pub world_snapshot: Option<WorldSnapshot>,
    pub history: SceneAuthoringHistoryStatus,
    pub live_dirty: bool,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneInspectorEntityRow {
    pub id: SceneEntityId,
    pub parent: Option<SceneEntityId>,
    pub component_count: usize,
    pub has_prefab: bool,
    pub selected: bool,
    pub live_entity: Option<Entity>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneInspectorEntityView {
    pub id: SceneEntityId,
    pub parent: Option<SceneEntityId>,
    pub has_prefab: bool,
    pub components: Vec<SceneInspectorComponentView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneInspectorComponentView {
    pub component: ComponentTypeId,
    pub document_version: ComponentSchemaVersion,
    pub schema_version: Option<ComponentSchemaVersion>,
    pub rust_type_path: Option<String>,
    pub serializable: bool,
    pub schema_known: bool,
    pub raw_value: ComponentValue,
    pub fields: Vec<SceneInspectorFieldView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneInspectorFieldView {
    pub path: ComponentFieldPath,
    pub value_kind: ComponentValueKind,
    pub required: bool,
    pub default_value: Option<ComponentValue>,
    pub value: Option<ComponentValue>,
    pub state: SceneInspectorFieldState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneInspectorFieldState {
    Present,
    Missing,
    InvalidPath(String),
}

#[derive(Debug, Clone, PartialEq)]
pub enum SceneInspectorCommand {
    SelectEntity {
        entity: Option<SceneEntityId>,
    },
    SetField {
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        path: ComponentFieldPath,
        value: ComponentValue,
    },
    RemoveField {
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        path: ComponentFieldPath,
    },
    SetAssetRefField {
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        path: ComponentFieldPath,
        asset_ref: AssetRef,
    },
    Reparent {
        entity: SceneEntityId,
        parent: Option<SceneEntityId>,
    },
}

impl SceneInspectorCommand {
    #[must_use]
    pub fn target_entity(&self) -> Option<&SceneEntityId> {
        match self {
            Self::SelectEntity { entity } => entity.as_ref(),
            Self::SetField { entity, .. }
            | Self::RemoveField { entity, .. }
            | Self::SetAssetRefField { entity, .. }
            | Self::Reparent { entity, .. } => Some(entity),
        }
    }

    #[must_use]
    pub fn to_patch(&self) -> Option<ScenePatchDocument> {
        let operation = match self {
            Self::SelectEntity { .. } => return None,
            Self::SetField {
                entity,
                component,
                component_version,
                path,
                value,
            } => ScenePatchOperation::SetField {
                entity: entity.clone(),
                component: component.clone(),
                component_version: *component_version,
                path: path.clone(),
                value: value.clone(),
            },
            Self::RemoveField {
                entity,
                component,
                component_version,
                path,
            } => ScenePatchOperation::RemoveField {
                entity: entity.clone(),
                component: component.clone(),
                component_version: *component_version,
                path: path.clone(),
            },
            Self::SetAssetRefField {
                entity,
                component,
                component_version,
                path,
                asset_ref,
            } => ScenePatchOperation::SetAssetRefField {
                entity: entity.clone(),
                component: component.clone(),
                component_version: *component_version,
                path: path.clone(),
                asset_ref: asset_ref.clone(),
            },
            Self::Reparent { entity, parent } => ScenePatchOperation::Reparent {
                entity: entity.clone(),
                parent: parent.clone(),
            },
        };
        Some(ScenePatchDocument::new([operation]))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneInspectorCommandReport {
    pub applied: bool,
    pub selected_entity: Option<SceneEntityId>,
    pub patch: Option<ScenePatchDocument>,
    pub patch_report: Option<ScenePatchReport>,
    pub diagnostics: DiagnosticReport,
}

impl SceneInspectorCommandReport {
    #[must_use]
    fn with_entity_selection(mut self, selected_entity: Option<SceneEntityId>) -> Self {
        self.selected_entity = selected_entity;
        self
    }
}

fn build_inspector_model(
    session: &SceneAuthoringSession,
    registry: &ComponentRegistry,
    selected_entity: Option<&SceneEntityId>,
    world_snapshot: Option<&WorldSnapshot>,
) -> SceneInspectorModel {
    let mut diagnostics = DiagnosticReport::default();
    let document = session.document();
    let entities = document
        .entities
        .iter()
        .map(|entity| SceneInspectorEntityRow {
            id: entity.id.clone(),
            parent: entity.parent.clone(),
            component_count: entity.components.len(),
            has_prefab: entity.prefab.is_some(),
            selected: selected_entity == Some(&entity.id),
            live_entity: session.live_entity_map().get(&entity.id),
        })
        .collect::<Vec<_>>();

    let selected_entity_view = selected_entity.and_then(|id| {
        let entity = document.entities.iter().find(|entity| entity.id == *id);
        if entity.is_none() {
            diagnostics.push(
                Diagnostic::warning(
                    "tooling.inspector-selected-entity-missing",
                    "selected entity is no longer present in the authoring document",
                )
                .with_entity_id(id.as_str()),
            );
        }
        entity.map(|entity| build_entity_view(entity, registry, &mut diagnostics))
    });

    SceneInspectorModel {
        selected_entity: selected_entity.cloned(),
        entities,
        selected_entity_view,
        schema_catalog: registry.schema_catalog(),
        world_snapshot: world_snapshot.cloned(),
        history: session.history_status(),
        live_dirty: session.is_live_dirty(),
        diagnostics,
    }
}

fn build_entity_view(
    entity: &nara_scene::SceneEntityRecord,
    registry: &ComponentRegistry,
    diagnostics: &mut DiagnosticReport,
) -> SceneInspectorEntityView {
    let components = entity
        .components
        .iter()
        .map(|(component_id, component)| {
            build_component_view(&entity.id, component_id, component, registry, diagnostics)
        })
        .collect();

    SceneInspectorEntityView {
        id: entity.id.clone(),
        parent: entity.parent.clone(),
        has_prefab: entity.prefab.is_some(),
        components,
    }
}

fn build_component_view(
    entity_id: &SceneEntityId,
    component_id: &ComponentTypeId,
    component: &SceneComponentRecord,
    registry: &ComponentRegistry,
    diagnostics: &mut DiagnosticReport,
) -> SceneInspectorComponentView {
    let schema = registry.schema(component_id);
    if schema.is_none() {
        diagnostics.push(
            Diagnostic::warning(
                "tooling.inspector-unknown-component",
                "inspector cannot find schema for a component on the selected entity",
            )
            .with_entity_id(entity_id.as_str())
            .with_component_id(component_id.as_str()),
        );
    }

    SceneInspectorComponentView {
        component: component_id.clone(),
        document_version: component.version,
        schema_version: schema.map(|schema| schema.version),
        rust_type_path: schema.map(|schema| schema.rust_type_path.clone()),
        serializable: schema.is_some_and(|schema| schema.serializable),
        schema_known: schema.is_some(),
        raw_value: component.value.clone(),
        fields: schema
            .map(|schema| build_field_views(schema, &component.value))
            .unwrap_or_default(),
    }
}

fn build_field_views(
    schema: &ComponentSchema,
    value: &ComponentValue,
) -> Vec<SceneInspectorFieldView> {
    schema
        .fields
        .iter()
        .map(|field| build_field_view(field, value))
        .collect()
}

fn build_field_view(
    field: &ComponentFieldSchema,
    value: &ComponentValue,
) -> SceneInspectorFieldView {
    match value.get_path(&field.path) {
        Ok(value) => SceneInspectorFieldView {
            path: field.path.clone(),
            value_kind: field.value_kind,
            required: field.required,
            default_value: field.default_value.clone(),
            value: Some(value.clone()),
            state: SceneInspectorFieldState::Present,
        },
        Err(error) => SceneInspectorFieldView {
            path: field.path.clone(),
            value_kind: field.value_kind,
            required: field.required,
            default_value: field.default_value.clone(),
            value: None,
            state: if field.required {
                SceneInspectorFieldState::InvalidPath(error.to_string())
            } else {
                SceneInspectorFieldState::Missing
            },
        },
    }
}

fn document_has_entity(document: &SceneDocument, entity: &SceneEntityId) -> bool {
    document.entities.iter().any(|record| record.id == *entity)
}

fn inspector_error_report(
    code: impl Into<String>,
    message: impl Into<String>,
) -> SceneInspectorCommandReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(Diagnostic::error(code, message));
    SceneInspectorCommandReport {
        applied: false,
        selected_entity: None,
        patch: None,
        patch_report: None,
        diagnostics,
    }
}

fn inspector_entity_error_report(
    code: impl Into<String>,
    message: impl Into<String>,
    entity: &SceneEntityId,
) -> SceneInspectorCommandReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(Diagnostic::error(code, message).with_entity_id(entity.as_str()));
    SceneInspectorCommandReport {
        applied: false,
        selected_entity: None,
        patch: None,
        patch_report: None,
        diagnostics,
    }
}
