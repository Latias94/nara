use std::collections::BTreeSet;

use nara_diagnostic::Diagnostic;
use nara_reflect::{
    ComponentCapability, ComponentRegistry, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
};
use nara_scene::{
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
};

use crate::diagnostic;

const TRANSFORM_COMPONENT: &str = "nara.transform.Transform2d";
const SPRITE_COMPONENT: &str = "nara.sprite.Sprite";

/// The bounded entity drafts admitted by the first Scene Composition product slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneCompositionEntityKind {
    Empty,
    ColoredSprite,
}

/// The bounded component drafts admitted by the first Scene Composition product slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneCompositionComponentKind {
    Transform2d,
    Sprite,
}

/// One UI-neutral Scene Composition command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SceneCompositionCommand {
    CreateEntity {
        kind: SceneCompositionEntityKind,
        placement: ScenePlacement,
    },
    RemoveEntity {
        entity: SceneEntityId,
    },
    AddComponent {
        entity: SceneEntityId,
        component: SceneCompositionComponentKind,
    },
    RemoveComponent {
        entity: SceneEntityId,
        component: SceneCompositionComponentKind,
    },
    MoveEntity {
        entity: SceneEntityId,
        to: ScenePlacement,
    },
}

impl SceneCompositionCommand {
    pub(crate) const fn allocates_entity(&self) -> bool {
        matches!(self, Self::CreateEntity { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SceneCompositionSelection {
    Preserve,
    Select(Option<SceneEntityId>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreparedSceneComposition {
    patch: ScenePatchDocument,
    selection: SceneCompositionSelection,
}

impl PreparedSceneComposition {
    pub(crate) fn patch(&self) -> &ScenePatchDocument {
        &self.patch
    }

    pub(crate) fn selection(&self) -> &SceneCompositionSelection {
        &self.selection
    }
}

pub(crate) fn prepare_scene_composition(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    selected: Option<&SceneEntityId>,
    allocated_entity: Option<SceneEntityId>,
    command: SceneCompositionCommand,
) -> Result<PreparedSceneComposition, Diagnostic> {
    match command {
        SceneCompositionCommand::CreateEntity { kind, placement } => {
            let entity_id = allocated_entity.ok_or_else(entity_id_exhausted)?;
            let mut entity = SceneEntityRecord::new(entity_id.clone());
            if kind == SceneCompositionEntityKind::ColoredSprite {
                let (transform_id, transform) =
                    component_draft(registry, SceneCompositionComponentKind::Transform2d)?;
                let (sprite_id, sprite) =
                    component_draft(registry, SceneCompositionComponentKind::Sprite)?;
                entity = entity
                    .with_component(transform_id, transform)
                    .with_component(sprite_id, sprite);
            }
            Ok(PreparedSceneComposition {
                patch: ScenePatchDocument::new([ScenePatchOperation::AddEntity {
                    entity,
                    placement,
                }]),
                selection: SceneCompositionSelection::Select(Some(entity_id)),
            })
        }
        SceneCompositionCommand::RemoveEntity { entity } => {
            let from = current_placement(document, &entity)?;
            let replacement_selection = selected
                .filter(|selected| subtree_contains(document, &entity, selected))
                .map(|_| match &from.owner {
                    ScenePlacementOwner::Root => None,
                    ScenePlacementOwner::Parent { parent } => Some(parent.clone()),
                });
            Ok(PreparedSceneComposition {
                patch: ScenePatchDocument::new([ScenePatchOperation::RemoveEntity {
                    entity,
                    from,
                }]),
                selection: replacement_selection.map_or(
                    SceneCompositionSelection::Preserve,
                    SceneCompositionSelection::Select,
                ),
            })
        }
        SceneCompositionCommand::AddComponent { entity, component } => {
            require_entity(document, &entity)?;
            let (component_id, value) = component_draft(registry, component)?;
            Ok(PreparedSceneComposition {
                patch: ScenePatchDocument::new([ScenePatchOperation::AddComponent {
                    entity,
                    component: component_id,
                    value,
                }]),
                selection: SceneCompositionSelection::Preserve,
            })
        }
        SceneCompositionCommand::RemoveComponent { entity, component } => {
            require_entity(document, &entity)?;
            let (component, _) = admitted_component(registry, component)?;
            Ok(PreparedSceneComposition {
                patch: ScenePatchDocument::new([ScenePatchOperation::RemoveComponent {
                    entity,
                    component,
                }]),
                selection: SceneCompositionSelection::Preserve,
            })
        }
        SceneCompositionCommand::MoveEntity { entity, to } => {
            let from = current_placement(document, &entity)?;
            Ok(PreparedSceneComposition {
                patch: ScenePatchDocument::new([ScenePatchOperation::MoveEntity {
                    entity,
                    from,
                    to,
                }]),
                selection: SceneCompositionSelection::Preserve,
            })
        }
    }
}

fn component_draft(
    registry: &ComponentRegistry,
    component: SceneCompositionComponentKind,
) -> Result<(ComponentTypeId, SceneComponentRecord), Diagnostic> {
    let (component_id, version) = admitted_component(registry, component)?;
    let value = match component {
        SceneCompositionComponentKind::Transform2d => default_transform_value(),
        SceneCompositionComponentKind::Sprite => default_sprite_value(),
    };
    Ok((component_id, SceneComponentRecord::new(version, value)))
}

fn admitted_component(
    registry: &ComponentRegistry,
    component: SceneCompositionComponentKind,
) -> Result<(ComponentTypeId, ComponentSchemaVersion), Diagnostic> {
    if !registry.is_frozen() {
        return Err(diagnostic::error(
            "tooling.composition-registry-not-frozen",
            "scene composition requires a frozen component registry",
        ));
    }
    let component_id = component_type(component);
    let Some(schema) = registry.schema(&component_id) else {
        return Err(diagnostic::with_component(
            diagnostic::error(
                "tooling.composition-component-unavailable",
                "scene composition component is not available in this project",
            ),
            &component_id,
        ));
    };
    if schema.version() != ComponentSchemaVersion::ONE
        || !ComponentCapability::SCENE_AUTHORING
            .into_iter()
            .all(|capability| schema.has_capability(capability))
    {
        return Err(diagnostic::with_component(
            diagnostic::error(
                "tooling.composition-component-not-admitted",
                "scene composition component does not match the admitted authoring contract",
            ),
            &component_id,
        ));
    }

    Ok((component_id, schema.version()))
}

fn component_type(component: SceneCompositionComponentKind) -> ComponentTypeId {
    ComponentTypeId::new(match component {
        SceneCompositionComponentKind::Transform2d => TRANSFORM_COMPONENT,
        SceneCompositionComponentKind::Sprite => SPRITE_COMPONENT,
    })
}

fn default_transform_value() -> ComponentValue {
    ComponentValue::map([
        ("translation", vec2(0.0, 0.0)),
        ("rotation", finite(0.0)),
        ("scale", vec2(1.0, 1.0)),
    ])
}

fn default_sprite_value() -> ComponentValue {
    ComponentValue::map([
        ("size", vec2(64.0, 64.0)),
        ("anchor", vec2(0.0, 0.0)),
        (
            "material",
            ComponentValue::map([
                ("image", ComponentValue::Null),
                (
                    "sampler",
                    ComponentValue::map([
                        ("min_filter", ComponentValue::String("linear".to_string())),
                        ("mag_filter", ComponentValue::String("linear".to_string())),
                        (
                            "mipmap_filter",
                            ComponentValue::String("linear".to_string()),
                        ),
                        (
                            "address_mode_u",
                            ComponentValue::String("clamp_to_edge".to_string()),
                        ),
                        (
                            "address_mode_v",
                            ComponentValue::String("clamp_to_edge".to_string()),
                        ),
                    ]),
                ),
                ("alpha_mode", ComponentValue::String("blend".to_string())),
                (
                    "tint",
                    ComponentValue::map([
                        ("r", finite(0.20)),
                        ("g", finite(0.55)),
                        ("b", finite(0.95)),
                        ("a", finite(1.0)),
                    ]),
                ),
            ]),
        ),
        ("layer", ComponentValue::I64(0)),
        ("sort_key", ComponentValue::I64(0)),
        ("texture_region", ComponentValue::Null),
    ])
}

fn vec2(x: f64, y: f64) -> ComponentValue {
    ComponentValue::map([("x", finite(x)), ("y", finite(y))])
}

fn finite(value: f64) -> ComponentValue {
    ComponentValue::f64(value).expect("composition draft constants must be finite")
}

fn require_entity(document: &SceneDocument, entity: &SceneEntityId) -> Result<(), Diagnostic> {
    if document.entities.iter().any(|record| record.id == *entity) {
        return Ok(());
    }
    Err(missing_entity(entity))
}

fn current_placement(
    document: &SceneDocument,
    entity: &SceneEntityId,
) -> Result<ScenePlacement, Diagnostic> {
    ScenePlacement::current(document, entity).ok_or_else(|| missing_entity(entity))
}

fn missing_entity(entity: &SceneEntityId) -> Diagnostic {
    diagnostic::with_entity(
        diagnostic::error(
            "tooling.composition-missing-entity",
            "scene composition targets an entity that is not in the document",
        ),
        entity,
    )
}

fn entity_id_exhausted() -> Diagnostic {
    diagnostic::error(
        "tooling.composition-entity-id-exhausted",
        "scene composition cannot allocate another stable entity identity",
    )
}

fn subtree_contains(
    document: &SceneDocument,
    root: &SceneEntityId,
    candidate: &SceneEntityId,
) -> bool {
    let mut pending = vec![root];
    let mut visited = BTreeSet::new();
    while let Some(entity) = pending.pop() {
        if entity == candidate {
            return true;
        }
        if !visited.insert(entity.clone()) {
            continue;
        }
        if let Some(record) = document.entities.iter().find(|record| record.id == *entity) {
            pending.extend(record.children.iter());
        }
    }
    false
}
