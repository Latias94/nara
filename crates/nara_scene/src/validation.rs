use std::collections::{BTreeMap, BTreeSet};

use nara_diagnostic::{Diagnostic, DiagnosticReport};
use nara_reflect::{
    ComponentDecodeContext, ComponentMigrationError, ComponentRegistry, ComponentTypeId,
    PreparedComponent,
};

use crate::{
    SceneDocument, SceneEntityId, SceneEntityRecord,
    diagnostics::{
        error as diagnostic_error, push_with_operation_index, with_asset_ref, with_codec_error,
        with_migration_error, with_public_locator,
    },
};

pub(crate) struct PreparedScene {
    pub(crate) entities: Vec<PreparedSceneEntity>,
    pub(crate) diagnostics: DiagnosticReport,
}

pub(crate) struct PreparedSceneEntity {
    pub(crate) id: SceneEntityId,
    pub(crate) parent: Option<SceneEntityId>,
    pub(crate) components: Vec<PreparedSceneComponent>,
}

pub(crate) struct PreparedSceneComponent {
    pub(crate) id: ComponentTypeId,
    pub(crate) prepared: PreparedComponent,
}

pub(crate) fn preflight_scene(
    document: &SceneDocument,
    registry: &ComponentRegistry,
) -> PreparedScene {
    let mut context = ComponentDecodeContext::new();
    preflight_scene_with_context(document, registry, &mut context)
}

pub(crate) fn preflight_authoring_scene(
    document: &SceneDocument,
    registry: &ComponentRegistry,
) -> PreparedScene {
    let mut context = ComponentDecodeContext::new();
    preflight_authoring_scene_with_context(document, registry, &mut context)
}

pub(crate) fn preflight_authoring_scene_for_patch(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    operation_index: usize,
) -> PreparedScene {
    let mut context = ComponentDecodeContext::new();
    preflight_scene_with_context_options(
        document,
        registry,
        &mut context,
        true,
        Some(operation_index),
    )
}

pub(crate) fn preflight_scene_with_context(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    context: &mut ComponentDecodeContext<'_>,
) -> PreparedScene {
    preflight_scene_with_context_options(document, registry, context, false, None)
}

pub(crate) fn preflight_authoring_scene_with_context(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    context: &mut ComponentDecodeContext<'_>,
) -> PreparedScene {
    preflight_scene_with_context_options(document, registry, context, true, None)
}

pub(crate) fn preflight_authoring_scene_with_context_for_patch(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    context: &mut ComponentDecodeContext<'_>,
    operation_index: usize,
) -> PreparedScene {
    preflight_scene_with_context_options(document, registry, context, true, Some(operation_index))
}

fn preflight_scene_with_context_options(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    context: &mut ComponentDecodeContext<'_>,
    allow_prefab_instances: bool,
    operation_index: Option<usize>,
) -> PreparedScene {
    let mut diagnostics = DiagnosticReport::default();
    if !registry.is_frozen() {
        diagnostics.push(diagnostic_error(
            "scene.component-registry-not-frozen",
            "Scene validation requires a frozen component registry",
        ));
        return PreparedScene {
            entities: Vec::new(),
            diagnostics,
        };
    }
    let mut seen = BTreeSet::<SceneEntityId>::new();
    let mut ids = BTreeSet::<SceneEntityId>::new();

    for entity in &document.entities {
        if !seen.insert(entity.id.clone()) {
            push_with_operation_index(
                &mut diagnostics,
                with_public_locator(
                    diagnostic_error("scene.duplicate-entity-id", "Scene entity ID is duplicated"),
                    "entity-id",
                    entity.id.as_str(),
                ),
                operation_index,
            );
        }
        ids.insert(entity.id.clone());
    }

    for entity in &document.entities {
        if let Some(parent) = &entity.parent
            && !ids.contains(parent)
        {
            push_with_operation_index(
                &mut diagnostics,
                with_public_locator(
                    diagnostic_error("scene.missing-parent", "Scene parent entity does not exist"),
                    "entity-id",
                    entity.id.as_str(),
                ),
                operation_index,
            );
        }
        if let Some(prefab) = &entity.prefab
            && !allow_prefab_instances
        {
            push_with_operation_index(
                &mut diagnostics,
                with_asset_ref(
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.prefab-instance-unsupported",
                                "Prefab instances require expansion before scene spawn",
                            ),
                            "entity-id",
                            entity.id.as_str(),
                        ),
                        "field-path",
                        "prefab.source",
                    ),
                    "asset-ref",
                    &prefab.source,
                ),
                operation_index,
            );
        }
    }

    detect_parent_cycles(document, &mut diagnostics, operation_index);

    let mut prepared_entities = Vec::new();
    for entity in sorted_entities(document) {
        let mut prepared_components = Vec::new();
        for (component_id, component) in &entity.components {
            let Some(schema) = registry.schema(component_id) else {
                push_with_operation_index(
                    &mut diagnostics,
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.unknown-component",
                                "Component type is not registered",
                            ),
                            "entity-id",
                            entity.id.as_str(),
                        ),
                        "component-id",
                        component_id.as_str(),
                    ),
                    operation_index,
                );
                continue;
            };
            if !schema.has_capability(nara_reflect::ComponentCapability::Scene) {
                push_with_operation_index(
                    &mut diagnostics,
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.component-not-scene-capable",
                                "Component is not scene-capable",
                            ),
                            "entity-id",
                            entity.id.as_str(),
                        ),
                        "component-id",
                        component_id.as_str(),
                    ),
                    operation_index,
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
                    push_with_operation_index(
                        &mut diagnostics,
                        component_migration_diagnostic(
                            entity.id.as_str(),
                            component_id.as_str(),
                            &error,
                        ),
                        operation_index,
                    );
                    continue;
                }
            };
            if migrated.version != schema.version {
                push_with_operation_index(
                    &mut diagnostics,
                    component_migration_diagnostic(
                        entity.id.as_str(),
                        component_id.as_str(),
                        &ComponentMigrationError::UnsupportedVersion {
                            component_id: component_id.clone(),
                            from_version: migrated.version,
                            target_version: schema.version,
                        },
                    ),
                    operation_index,
                );
                continue;
            }

            match registry.preflight_component_with_context(component_id, &migrated.value, context)
            {
                Some(Ok(prepared)) => prepared_components.push(PreparedSceneComponent {
                    id: component_id.clone(),
                    prepared,
                }),
                Some(Err(error)) => {
                    let diagnostic = with_codec_error(
                        with_public_locator(
                            with_public_locator(
                                diagnostic_error(
                                    "scene.invalid-component-payload",
                                    "Component payload is invalid",
                                ),
                                "entity-id",
                                entity.id.as_str(),
                            ),
                            "component-id",
                            component_id.as_str(),
                        ),
                        &error,
                    );
                    push_with_operation_index(&mut diagnostics, diagnostic, operation_index);
                }
                None => push_with_operation_index(
                    &mut diagnostics,
                    with_public_locator(
                        with_public_locator(
                            diagnostic_error(
                                "scene.missing-component-codec",
                                "Component has no scene codec",
                            ),
                            "entity-id",
                            entity.id.as_str(),
                        ),
                        "component-id",
                        component_id.as_str(),
                    ),
                    operation_index,
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
        ComponentMigrationError::MigrationFailed { .. } => with_migration_error(
            with_public_locator(
                with_public_locator(
                    diagnostic_error(
                        "scene.component-migration-failed",
                        "Component migration failed",
                    ),
                    "entity-id",
                    entity_id,
                ),
                "component-id",
                component_id,
            ),
            error,
        ),
        ComponentMigrationError::UnknownComponentId { .. }
        | ComponentMigrationError::UnsupportedVersion { .. }
        | ComponentMigrationError::MissingMigration { .. } => with_migration_error(
            with_public_locator(
                with_public_locator(
                    diagnostic_error(
                        "scene.unsupported-component-version",
                        "Component version is unsupported",
                    ),
                    "entity-id",
                    entity_id,
                ),
                "component-id",
                component_id,
            ),
            error,
        ),
    }
}

fn sorted_entities(document: &SceneDocument) -> Vec<&SceneEntityRecord> {
    let mut entities = document.entities.iter().collect::<Vec<_>>();
    entities.sort_by(|left, right| left.id.cmp(&right.id));
    entities
}

fn detect_parent_cycles(
    document: &SceneDocument,
    diagnostics: &mut DiagnosticReport,
    operation_index: Option<usize>,
) {
    let parents = document
        .entities
        .iter()
        .map(|entity| (entity.id.clone(), entity.parent.clone()))
        .collect::<BTreeMap<_, _>>();

    #[derive(Clone, Copy, PartialEq, Eq)]
    enum VisitState {
        Visiting,
        Acyclic,
        Cyclic,
    }

    let mut states = BTreeMap::<SceneEntityId, VisitState>::new();
    for entity in &document.entities {
        if matches!(
            states.get(&entity.id),
            Some(VisitState::Acyclic | VisitState::Cyclic)
        ) {
            continue;
        }

        let mut path = Vec::new();
        let mut current = Some(entity.id.clone());
        let outcome = loop {
            let Some(id) = current else {
                break VisitState::Acyclic;
            };
            match states.get(&id) {
                Some(VisitState::Acyclic) => break VisitState::Acyclic,
                Some(VisitState::Cyclic | VisitState::Visiting) => break VisitState::Cyclic,
                None => {
                    states.insert(id.clone(), VisitState::Visiting);
                    path.push(id.clone());
                    current = parents.get(&id).and_then(Clone::clone);
                }
            }
        };
        for id in path {
            states.insert(id, outcome);
        }
    }

    for entity in &document.entities {
        if states.get(&entity.id) == Some(&VisitState::Cyclic) {
            push_with_operation_index(
                diagnostics,
                with_public_locator(
                    diagnostic_error(
                        "scene.parent-cycle",
                        "Scene hierarchy contains a parent cycle",
                    ),
                    "entity-id",
                    entity.id.as_str(),
                ),
                operation_index,
            );
        }
    }
}
