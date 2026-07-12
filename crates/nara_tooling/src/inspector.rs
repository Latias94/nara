use std::collections::BTreeSet;

use nara_asset::{AssetRef, ProjectAssetDatabase};
use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_identity::WorldEntityLocator;
use nara_reflect::{
    ComponentCapability, ComponentFieldId, ComponentFieldPath, ComponentFieldSchema,
    ComponentRegistry, ComponentSchema, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
    ComponentValueKind,
};
use nara_scene::{
    SceneAuthoringHistoryStatus, SceneAuthoringSession, SceneComponentRecord, SceneDocument,
    SceneEntityId, ScenePatchDocument, ScenePatchOperation, ScenePatchReport,
};

use crate::{diagnostic, snapshot::WorldIdentitySnapshot};

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
        world_snapshot: Option<&WorldIdentitySnapshot>,
    ) -> SceneInspectorModel {
        build_inspector_model(
            session,
            registry,
            self.selected_entity.as_ref(),
            world_snapshot,
        )
    }

    #[must_use]
    pub fn model_with_selection(
        &self,
        session: &SceneAuthoringSession,
        registry: &ComponentRegistry,
        selected_entity: Option<&SceneEntityId>,
        world_snapshot: Option<&WorldIdentitySnapshot>,
    ) -> SceneInspectorModel {
        build_inspector_model(session, registry, selected_entity, world_snapshot)
    }

    pub fn apply_command(
        &mut self,
        session: &mut SceneAuthoringSession,
        registry: &ComponentRegistry,
        command: SceneInspectorCommand,
    ) -> SceneInspectorCommandReport {
        self.apply_command_with_patch_apply(session, registry, command, |session, patch| {
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
        self.apply_command_with_patch_apply(session, registry, command, |session, patch| {
            session.apply_patch_with_asset_database(patch, registry, database)
        })
    }

    fn apply_command_with_patch_apply(
        &mut self,
        session: &mut SceneAuthoringSession,
        registry: &ComponentRegistry,
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

                let patch = match command.to_patch_for_document(session.document(), registry) {
                    Ok(Some(patch)) => patch,
                    Ok(None) => {
                        unreachable!("non-selection inspector commands should produce patches")
                    }
                    Err(diagnostic) => {
                        let mut diagnostics = DiagnosticReport::default();
                        diagnostics.push(diagnostic);
                        return SceneInspectorCommandReport {
                            applied: false,
                            selected_entity: self.selected_entity.clone(),
                            patch: None,
                            patch_report: None,
                            diagnostics,
                        };
                    }
                };
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
        if let Some(entity) = &entity
            && !document_has_entity(document, entity)
        {
            return inspector_entity_error_report(
                "tooling.inspector-missing-entity",
                "inspector selection targets an entity that is not in the authoring document",
                entity,
            )
            .with_entity_selection(self.selected_entity.clone());
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
    pub world_snapshot: Option<WorldIdentitySnapshot>,
    pub history: SceneAuthoringHistoryStatus,
    pub live_dirty: bool,
    pub diagnostics: DiagnosticReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneInspectorEntityRow {
    pub id: SceneEntityId,
    pub parent: Option<SceneEntityId>,
    pub inspectable_component_count: usize,
    pub has_prefab: bool,
    pub selected: bool,
    pub live_locator: Option<WorldEntityLocator>,
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
    pub schema_version: ComponentSchemaVersion,
    pub capabilities: BTreeSet<ComponentCapability>,
    pub fields: Vec<SceneInspectorFieldView>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SceneInspectorFieldView {
    pub id: ComponentFieldId,
    pub aliases: Vec<String>,
    pub path: ComponentFieldPath,
    pub value_kind: ComponentValueKind,
    pub required: bool,
    pub capabilities: BTreeSet<ComponentCapability>,
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
        field: ComponentFieldId,
        value: ComponentValue,
    },
    RemoveField {
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        field: ComponentFieldId,
    },
    SetAssetRefField {
        entity: SceneEntityId,
        component: ComponentTypeId,
        component_version: ComponentSchemaVersion,
        field: ComponentFieldId,
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
                field,
                value,
            } => ScenePatchOperation::SetField {
                entity: entity.clone(),
                component: component.clone(),
                component_version: *component_version,
                field: field.clone(),
                value: value.clone(),
            },
            Self::RemoveField {
                entity,
                component,
                component_version,
                field,
            } => ScenePatchOperation::RemoveField {
                entity: entity.clone(),
                component: component.clone(),
                component_version: *component_version,
                field: field.clone(),
            },
            Self::SetAssetRefField {
                entity,
                component,
                component_version,
                field,
                asset_ref,
            } => ScenePatchOperation::SetAssetRefField {
                entity: entity.clone(),
                component: component.clone(),
                component_version: *component_version,
                field: field.clone(),
                asset_ref: asset_ref.clone(),
            },
            Self::Reparent { entity, parent } => ScenePatchOperation::Reparent {
                entity: entity.clone(),
                parent: parent.clone(),
            },
        };
        Some(ScenePatchDocument::new([operation]))
    }

    fn to_patch_for_document(
        &self,
        document: &SceneDocument,
        registry: &ComponentRegistry,
    ) -> Result<Option<ScenePatchDocument>, Diagnostic> {
        let Some(patch) = self.to_patch() else {
            return Ok(None);
        };
        let Some((entity_id, component_id)) = self.component_target() else {
            return Ok(Some(patch));
        };
        let Some(component) = document
            .entities
            .iter()
            .find(|entity| &entity.id == entity_id)
            .and_then(|entity| entity.components.get(component_id))
        else {
            return Ok(Some(patch));
        };
        let Some(schema) = registry.schema(component_id) else {
            return Ok(Some(patch));
        };
        if component.version == schema.version {
            return Ok(Some(patch));
        }

        let migrated = registry
            .migrate_component_value(component_id, component.version, &component.value)
            .map_err(|_| {
                component_migration_diagnostic(
                    entity_id,
                    component_id,
                    component.version,
                    schema.version,
                )
            })?;
        let mut operations = Vec::with_capacity(patch.operations.len() + 1);
        operations.push(ScenePatchOperation::ReplaceComponent {
            entity: entity_id.clone(),
            component: component_id.clone(),
            value: SceneComponentRecord::new(migrated.version, migrated.value),
        });
        operations.extend(patch.operations);
        Ok(Some(ScenePatchDocument::new(operations)))
    }

    fn component_target(&self) -> Option<(&SceneEntityId, &ComponentTypeId)> {
        match self {
            Self::SetField {
                entity, component, ..
            }
            | Self::RemoveField {
                entity, component, ..
            }
            | Self::SetAssetRefField {
                entity, component, ..
            } => Some((entity, component)),
            Self::SelectEntity { .. } | Self::Reparent { .. } => None,
        }
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
    world_snapshot: Option<&WorldIdentitySnapshot>,
) -> SceneInspectorModel {
    let mut diagnostics = DiagnosticReport::default();
    if !registry.is_frozen() {
        diagnostics.push(diagnostic::error(
            "tooling.inspector-registry-not-frozen",
            "inspector requires a frozen component registry",
        ));
    }
    let document = session.document();
    let entities = document
        .entities
        .iter()
        .map(|entity| SceneInspectorEntityRow {
            id: entity.id.clone(),
            parent: entity.parent.clone(),
            inspectable_component_count: inspectable_component_count(entity, registry),
            has_prefab: entity.prefab.is_some(),
            selected: selected_entity == Some(&entity.id),
            live_locator: session
                .live_instance()
                .and_then(|instance| instance.locator(&entity.id)),
        })
        .collect::<Vec<_>>();

    let selected_entity_view = selected_entity.and_then(|id| {
        let entity = document.entities.iter().find(|entity| entity.id == *id);
        if entity.is_none() {
            diagnostics.push(diagnostic::with_entity(
                diagnostic::warning(
                    "tooling.inspector-selected-entity-missing",
                    "selected entity is no longer present in the authoring document",
                ),
                id,
            ));
        }
        entity.map(|entity| build_entity_view(entity, registry, &mut diagnostics))
    });

    SceneInspectorModel {
        selected_entity: selected_entity.cloned(),
        entities,
        selected_entity_view,
        world_snapshot: world_snapshot.cloned(),
        history: session.history_status(),
        live_dirty: session.is_live_dirty(),
        diagnostics,
    }
}

fn inspectable_component_count(
    entity: &nara_scene::SceneEntityRecord,
    registry: &ComponentRegistry,
) -> usize {
    if !registry.is_frozen() {
        return 0;
    }
    entity
        .components
        .keys()
        .filter(|component_id| {
            registry
                .schema(component_id)
                .is_some_and(|schema| schema.has_capability(ComponentCapability::Inspect))
        })
        .count()
}

fn build_entity_view(
    entity: &nara_scene::SceneEntityRecord,
    registry: &ComponentRegistry,
    diagnostics: &mut DiagnosticReport,
) -> SceneInspectorEntityView {
    if !registry.is_frozen() {
        return SceneInspectorEntityView {
            id: entity.id.clone(),
            parent: entity.parent.clone(),
            has_prefab: entity.prefab.is_some(),
            components: Vec::new(),
        };
    }

    let components = entity
        .components
        .iter()
        .filter_map(|(component_id, component)| {
            let Some(schema) = registry.schema(component_id) else {
                diagnostics.push(diagnostic::with_component(
                    diagnostic::with_entity(
                        diagnostic::warning(
                            "tooling.inspector-unknown-component",
                            "inspector cannot find schema for a component on the selected entity",
                        ),
                        &entity.id,
                    ),
                    component_id,
                ));
                return None;
            };
            if !schema.has_capability(ComponentCapability::Inspect) {
                return None;
            }
            match registry.migrate_component_value(
                component_id,
                component.version,
                &component.value,
            ) {
                Ok(migrated) => Some(build_component_view(
                    component_id,
                    component,
                    schema,
                    &migrated.value,
                )),
                Err(_) => {
                    diagnostics.push(component_migration_diagnostic(
                        &entity.id,
                        component_id,
                        component.version,
                        schema.version,
                    ));
                    None
                }
            }
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
    component_id: &ComponentTypeId,
    component: &SceneComponentRecord,
    schema: &ComponentSchema,
    current_value: &ComponentValue,
) -> SceneInspectorComponentView {
    SceneInspectorComponentView {
        component: component_id.clone(),
        document_version: component.version,
        schema_version: schema.version,
        capabilities: schema.capabilities.clone(),
        fields: build_field_views(schema, current_value),
    }
}

fn component_migration_diagnostic(
    entity: &SceneEntityId,
    component: &ComponentTypeId,
    source_version: ComponentSchemaVersion,
    target_version: ComponentSchemaVersion,
) -> Diagnostic {
    diagnostic::with_public_u64(
        diagnostic::with_public_u64(
            diagnostic::with_component(
                diagnostic::with_entity(
                    diagnostic::error(
                        "tooling.inspector-component-migration-failed",
                        "inspector could not migrate a component to the current schema",
                    ),
                    entity,
                ),
                component,
            ),
            "source-version",
            u64::from(source_version.get()),
        ),
        "target-version",
        u64::from(target_version.get()),
    )
}

fn build_field_views(
    schema: &ComponentSchema,
    value: &ComponentValue,
) -> Vec<SceneInspectorFieldView> {
    schema
        .fields
        .iter()
        .filter(|field| field.has_capability(ComponentCapability::Inspect))
        .map(|field| build_field_view(field, value))
        .collect()
}

fn build_field_view(
    field: &ComponentFieldSchema,
    value: &ComponentValue,
) -> SceneInspectorFieldView {
    match value.get_path(&field.path) {
        Ok(value) => SceneInspectorFieldView {
            id: field.id.clone(),
            aliases: field.aliases.clone(),
            path: field.path.clone(),
            value_kind: field.value_kind,
            required: field.required,
            capabilities: field.capabilities.clone(),
            default_value: field.default_value.clone(),
            value: Some(value.clone()),
            state: SceneInspectorFieldState::Present,
        },
        Err(error) => SceneInspectorFieldView {
            id: field.id.clone(),
            aliases: field.aliases.clone(),
            path: field.path.clone(),
            value_kind: field.value_kind,
            required: field.required,
            capabilities: field.capabilities.clone(),
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
    code: &'static str,
    summary: &'static str,
) -> SceneInspectorCommandReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic::error(code, summary));
    SceneInspectorCommandReport {
        applied: false,
        selected_entity: None,
        patch: None,
        patch_report: None,
        diagnostics,
    }
}

fn inspector_entity_error_report(
    code: &'static str,
    summary: &'static str,
    entity: &SceneEntityId,
) -> SceneInspectorCommandReport {
    let mut diagnostics = DiagnosticReport::default();
    diagnostics.push(diagnostic::with_entity(
        diagnostic::error(code, summary),
        entity,
    ));
    SceneInspectorCommandReport {
        applied: false,
        selected_entity: None,
        patch: None,
        patch_report: None,
        diagnostics,
    }
}
