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
    pub(crate) roots: Vec<SceneEntityId>,
    pub(crate) entities: Vec<PreparedSceneEntity>,
    pub(crate) diagnostics: DiagnosticReport,
}

pub(crate) struct PreparedSceneEntity {
    pub(crate) id: SceneEntityId,
    pub(crate) children: Vec<SceneEntityId>,
    pub(crate) components: Vec<PreparedSceneComponent>,
}

pub(crate) struct PreparedSceneComponent {
    pub(crate) id: ComponentTypeId,
    pub(crate) prepared: PreparedComponent,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValidatedScenePlacement {
    parents: BTreeMap<SceneEntityId, Option<SceneEntityId>>,
}

impl ValidatedScenePlacement {
    pub(crate) fn parent_of(&self, entity: &SceneEntityId) -> Option<&SceneEntityId> {
        self.parents.get(entity).and_then(Option::as_ref)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum PlacementOwner {
    Root,
    Parent(SceneEntityId),
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
    let placement = validate_scene_placement(document, &mut diagnostics, operation_index);
    if !registry.is_frozen() {
        diagnostics.push(diagnostic_error(
            "scene.component-registry-not-frozen",
            "Scene validation requires a frozen component registry",
        ));
        return PreparedScene {
            roots: Vec::new(),
            entities: Vec::new(),
            diagnostics,
        };
    }

    for entity in &document.entities {
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

    if let Some(entity_id) = placement.as_ref().and_then(|placement| {
        first_unavailable_hierarchy_projection_entity(document, placement, registry)
    }) {
        push_with_operation_index(
            &mut diagnostics,
            with_public_locator(
                diagnostic_error(
                    "scene.hierarchy-projection-unavailable",
                    "Scene hierarchy requires a runtime projection that is not implemented",
                ),
                "entity-id",
                entity_id.as_str(),
            ),
            operation_index,
        );
    }

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
            children: entity.children.clone(),
            components: prepared_components,
        });
    }

    PreparedScene {
        roots: document.roots.clone(),
        entities: prepared_entities,
        diagnostics,
    }
}

pub(crate) fn first_unavailable_hierarchy_projection_entity<'document>(
    document: &'document SceneDocument,
    placement: &ValidatedScenePlacement,
    registry: &ComponentRegistry,
) -> Option<&'document SceneEntityId> {
    let requires_unavailable_projection = |component_id: &ComponentTypeId| {
        component_id.as_str() == "nara.scene.Visibility" && registry.schema(component_id).is_some()
    };
    let by_id = document
        .entities
        .iter()
        .map(|entity| (&entity.id, entity))
        .collect::<BTreeMap<_, _>>();

    document.entities.iter().find_map(|entity| {
        let parent_id = placement.parent_of(&entity.id)?;
        let parent = by_id.get(parent_id).copied();
        let requires_unavailable_projection = entity
            .components
            .keys()
            .any(requires_unavailable_projection)
            || parent.is_some_and(|parent| {
                parent
                    .components
                    .keys()
                    .any(requires_unavailable_projection)
            });
        requires_unavailable_projection.then_some(&entity.id)
    })
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

fn validate_scene_placement(
    document: &SceneDocument,
    diagnostics: &mut DiagnosticReport,
    operation_index: Option<usize>,
) -> Option<ValidatedScenePlacement> {
    let errors_before = diagnostics.stats().observed_errors();
    let mut ids = BTreeSet::<SceneEntityId>::new();
    for entity in &document.entities {
        if !ids.insert(entity.id.clone()) {
            push_placement_diagnostic(
                diagnostics,
                operation_index,
                "scene.duplicate-entity-id",
                "Scene entity ID is duplicated",
                &entity.id,
                None,
            );
        }
    }

    let mut owners = BTreeMap::<SceneEntityId, PlacementOwner>::new();
    for root in &document.roots {
        if !ids.contains(root) {
            push_placement_diagnostic(
                diagnostics,
                operation_index,
                "scene.dangling-root",
                "Scene root does not identify an entity record",
                root,
                None,
            );
            continue;
        }
        match owners.get(root) {
            None => {
                owners.insert(root.clone(), PlacementOwner::Root);
            }
            Some(PlacementOwner::Root) => push_placement_diagnostic(
                diagnostics,
                operation_index,
                "scene.duplicate-root",
                "Scene root is listed more than once",
                root,
                None,
            ),
            Some(PlacementOwner::Parent(parent)) => push_placement_diagnostic(
                diagnostics,
                operation_index,
                "scene.root-child-overlap",
                "Scene entity cannot be both a root and a child",
                root,
                Some(parent),
            ),
        }
    }

    for parent in &document.entities {
        let mut local_children = BTreeSet::<SceneEntityId>::new();
        for child in &parent.children {
            if !local_children.insert(child.clone()) {
                push_placement_diagnostic(
                    diagnostics,
                    operation_index,
                    "scene.duplicate-child",
                    "Scene parent lists the same child more than once",
                    child,
                    Some(&parent.id),
                );
                continue;
            }
            if !ids.contains(child) {
                push_placement_diagnostic(
                    diagnostics,
                    operation_index,
                    "scene.dangling-child",
                    "Scene child does not identify an entity record",
                    child,
                    Some(&parent.id),
                );
                continue;
            }
            match owners.get(child) {
                None => {
                    owners.insert(child.clone(), PlacementOwner::Parent(parent.id.clone()));
                }
                Some(PlacementOwner::Root) => push_placement_diagnostic(
                    diagnostics,
                    operation_index,
                    "scene.root-child-overlap",
                    "Scene entity cannot be both a root and a child",
                    child,
                    Some(&parent.id),
                ),
                Some(PlacementOwner::Parent(existing)) if existing == &parent.id => {
                    push_placement_diagnostic(
                        diagnostics,
                        operation_index,
                        "scene.duplicate-child",
                        "Scene parent lists the same child more than once",
                        child,
                        Some(&parent.id),
                    );
                }
                Some(PlacementOwner::Parent(existing)) => push_placement_diagnostic(
                    diagnostics,
                    operation_index,
                    "scene.multiple-parents",
                    "Scene entity is owned by more than one parent",
                    child,
                    Some(existing),
                ),
            }
        }
    }

    for id in &ids {
        if !owners.contains_key(id) {
            push_placement_diagnostic(
                diagnostics,
                operation_index,
                "scene.missing-placement",
                "Scene entity is absent from roots and every parent child list",
                id,
                None,
            );
        }
    }

    let parents = owners
        .iter()
        .map(|(entity, owner)| {
            let parent = match owner {
                PlacementOwner::Root => None,
                PlacementOwner::Parent(parent) => Some(parent.clone()),
            };
            (entity.clone(), parent)
        })
        .collect::<BTreeMap<_, _>>();

    let mut completed = BTreeSet::<SceneEntityId>::new();
    for entity in &document.entities {
        if completed.contains(&entity.id) {
            continue;
        }

        let mut path = Vec::<SceneEntityId>::new();
        let mut positions = BTreeMap::<SceneEntityId, usize>::new();
        let mut current = Some(entity.id.clone());
        while let Some(id) = current {
            if completed.contains(&id) {
                break;
            }
            if let Some(cycle_start) = positions.get(&id).copied() {
                let witness = path[cycle_start..]
                    .iter()
                    .min()
                    .expect("cycle path contains the repeated entity")
                    .clone();
                push_placement_diagnostic(
                    diagnostics,
                    operation_index,
                    "scene.placement-cycle",
                    "Scene placement contains a cycle",
                    &witness,
                    parents.get(&witness).and_then(Option::as_ref),
                );
                break;
            }
            positions.insert(id.clone(), path.len());
            path.push(id.clone());
            current = parents.get(&id).and_then(Clone::clone);
        }
        completed.extend(path);
    }

    (diagnostics.stats().observed_errors() == errors_before)
        .then_some(ValidatedScenePlacement { parents })
}

fn push_placement_diagnostic(
    diagnostics: &mut DiagnosticReport,
    operation_index: Option<usize>,
    code: &'static str,
    summary: &'static str,
    entity: &SceneEntityId,
    parent: Option<&SceneEntityId>,
) {
    let diagnostic = with_public_locator(
        diagnostic_error(code, summary),
        "entity-id",
        entity.as_str(),
    );
    let diagnostic = match parent {
        Some(parent) => with_public_locator(diagnostic, "parent-id", parent.as_str()),
        None => diagnostic,
    };
    push_with_operation_index(diagnostics, diagnostic, operation_index);
}
