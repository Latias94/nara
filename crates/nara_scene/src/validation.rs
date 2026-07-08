use std::collections::{BTreeMap, BTreeSet};

use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_reflect::{
    ComponentCodecError, ComponentDecodeContext, ComponentMigrationError, ComponentRegistry,
    PreparedComponent,
};

use crate::{SceneDocument, SceneEntityId, SceneEntityRecord, document::validate_scene_entity_id};

pub(crate) struct PreparedScene {
    pub(crate) entities: Vec<PreparedSceneEntity>,
    pub(crate) diagnostics: DiagnosticReport,
}

pub(crate) struct PreparedSceneEntity {
    pub(crate) id: SceneEntityId,
    pub(crate) parent: Option<SceneEntityId>,
    pub(crate) components: Vec<PreparedComponent>,
}

pub(crate) fn preflight_scene(
    document: &SceneDocument,
    registry: &ComponentRegistry,
) -> PreparedScene {
    let mut context = ComponentDecodeContext::new();
    preflight_scene_with_context(document, registry, &mut context)
}

pub(crate) fn preflight_scene_with_context(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    context: &mut ComponentDecodeContext<'_>,
) -> PreparedScene {
    let mut diagnostics = DiagnosticReport::default();
    let mut seen = BTreeSet::<SceneEntityId>::new();
    let mut ids = BTreeSet::<SceneEntityId>::new();

    if document.format_version != SceneDocument::CURRENT_FORMAT_VERSION {
        diagnostics.push(Diagnostic::error(
            "scene.unsupported-format-version",
            format!(
                "scene format version {} is unsupported; expected {}",
                document.format_version,
                SceneDocument::CURRENT_FORMAT_VERSION
            ),
        ));
    }

    for entity in &document.entities {
        if let Err(error) = validate_scene_entity_id(entity.id.as_str()) {
            diagnostics.push(
                Diagnostic::error("scene.invalid-entity-id", error.to_string())
                    .with_entity_id(entity.id.as_str()),
            );
        }
        if !seen.insert(entity.id.clone()) {
            diagnostics.push(
                Diagnostic::error("scene.duplicate-entity-id", "duplicate scene entity id")
                    .with_entity_id(entity.id.as_str()),
            );
        }
        ids.insert(entity.id.clone());
    }

    for entity in &document.entities {
        if let Some(parent) = &entity.parent {
            if let Err(error) = validate_scene_entity_id(parent.as_str()) {
                diagnostics.push(
                    Diagnostic::error("scene.invalid-parent-id", error.to_string())
                        .with_entity_id(entity.id.as_str())
                        .with_field_path("parent"),
                );
            }
            if !ids.contains(parent) {
                diagnostics.push(
                    Diagnostic::error("scene.missing-parent", "parent entity id does not exist")
                        .with_entity_id(entity.id.as_str()),
                );
            }
        }
        if let Some(prefab) = &entity.prefab {
            diagnostics.push(
                Diagnostic::error(
                    "scene.prefab-instance-unsupported",
                    "external prefab source resolution is not implemented in this slice; instantiate PrefabDocument directly",
                )
                .with_entity_id(entity.id.as_str())
                .with_field_path("prefab.source")
                .with_asset_ref(prefab.source.to_string()),
            );
        }
    }

    detect_parent_cycles(document, &mut diagnostics);

    let mut prepared_entities = Vec::new();
    for entity in sorted_entities(document) {
        let mut prepared_components = Vec::new();
        for (component_id, component) in &entity.components {
            let Some(schema) = registry.schema(component_id) else {
                diagnostics.push(
                    Diagnostic::error(
                        "scene.unknown-component",
                        "component type is not registered",
                    )
                    .with_entity_id(entity.id.as_str())
                    .with_component_id(component_id.as_str()),
                );
                continue;
            };
            if !schema.serializable {
                diagnostics.push(
                    Diagnostic::error(
                        "scene.component-not-serializable",
                        "component is registered but not scene-serializable",
                    )
                    .with_entity_id(entity.id.as_str())
                    .with_component_id(component_id.as_str()),
                );
                continue;
            }
            let migrated = match registry.migrate_component_value(
                component_id,
                component.version,
                &component.value,
            ) {
                Ok(migrated) => migrated,
                Err(error) => {
                    diagnostics.push(component_migration_diagnostic(
                        entity.id.as_str(),
                        component_id.as_str(),
                        &error,
                    ));
                    continue;
                }
            };
            if migrated.version != schema.version {
                diagnostics.push(component_migration_diagnostic(
                    entity.id.as_str(),
                    component_id.as_str(),
                    &ComponentMigrationError::UnsupportedVersion {
                        component_id: component_id.clone(),
                        from_version: migrated.version,
                        target_version: schema.version,
                    },
                ));
                continue;
            }

            match registry.preflight_component_with_context(component_id, &migrated.value, context)
            {
                Some(Ok(prepared)) => prepared_components.push(prepared),
                Some(Err(error)) => {
                    let mut diagnostic =
                        Diagnostic::error("scene.invalid-component-payload", error.to_string())
                            .with_entity_id(entity.id.as_str())
                            .with_component_id(component_id.as_str());
                    if let Some(field_path) = codec_error_field_path(&error) {
                        diagnostic = diagnostic.with_field_path(field_path);
                    }
                    if let Some(asset_ref) = codec_error_asset_ref(&error) {
                        diagnostic = diagnostic.with_asset_ref(asset_ref);
                    }
                    diagnostics.push(diagnostic);
                }
                None => diagnostics.push(
                    Diagnostic::error(
                        "scene.missing-component-codec",
                        "component has no scene codec",
                    )
                    .with_entity_id(entity.id.as_str())
                    .with_component_id(component_id.as_str()),
                ),
            }
        }

        prepared_entities.push(PreparedSceneEntity {
            id: entity.id.clone(),
            parent: entity.parent.clone(),
            components: prepared_components,
        });
    }

    PreparedScene {
        entities: prepared_entities,
        diagnostics,
    }
}

fn component_migration_diagnostic(
    entity_id: &str,
    component_id: &str,
    error: &ComponentMigrationError,
) -> Diagnostic {
    match error {
        ComponentMigrationError::MigrationFailed { error, .. } => {
            let mut diagnostic =
                Diagnostic::error("scene.component-migration-failed", error.to_string())
                    .with_entity_id(entity_id)
                    .with_component_id(component_id);
            if let Some(field_path) = codec_error_field_path(error) {
                diagnostic = diagnostic.with_field_path(field_path);
            }
            if let Some(asset_ref) = codec_error_asset_ref(error) {
                diagnostic = diagnostic.with_asset_ref(asset_ref);
            }
            diagnostic
        }
        ComponentMigrationError::UnknownComponentId { .. }
        | ComponentMigrationError::UnsupportedVersion { .. }
        | ComponentMigrationError::MissingMigration { .. } => {
            Diagnostic::error("scene.unsupported-component-version", error.to_string())
                .with_entity_id(entity_id)
                .with_component_id(component_id)
        }
    }
}

fn codec_error_field_path(error: &ComponentCodecError) -> Option<&str> {
    match error {
        ComponentCodecError::MissingField { field }
        | ComponentCodecError::InvalidField { field, .. }
        | ComponentCodecError::InvalidAssetRef { field, .. } => Some(field.as_str()),
        ComponentCodecError::EntityMissing | ComponentCodecError::Message(_) => None,
    }
}

fn codec_error_asset_ref(error: &ComponentCodecError) -> Option<&str> {
    match error {
        ComponentCodecError::InvalidAssetRef { asset_ref, .. } => Some(asset_ref.as_str()),
        ComponentCodecError::MissingField { .. }
        | ComponentCodecError::InvalidField { .. }
        | ComponentCodecError::EntityMissing
        | ComponentCodecError::Message(_) => None,
    }
}

fn sorted_entities(document: &SceneDocument) -> Vec<&SceneEntityRecord> {
    let mut entities = document.entities.iter().collect::<Vec<_>>();
    entities.sort_by(|left, right| left.id.cmp(&right.id));
    entities
}

fn detect_parent_cycles(document: &SceneDocument, diagnostics: &mut DiagnosticReport) {
    let parents = document
        .entities
        .iter()
        .map(|entity| (entity.id.clone(), entity.parent.clone()))
        .collect::<BTreeMap<_, _>>();

    for entity in &document.entities {
        let mut visiting = BTreeSet::new();
        let mut current = Some(entity.id.clone());
        while let Some(id) = current {
            if !visiting.insert(id.clone()) {
                diagnostics.push(
                    Diagnostic::error(
                        "scene.parent-cycle",
                        "scene hierarchy contains a parent cycle",
                    )
                    .with_entity_id(entity.id.as_str()),
                );
                break;
            }
            current = parents.get(&id).and_then(Clone::clone);
        }
    }
}
