use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use nara_diagnostic::Diagnostic;
use nara_reflect::{
    ComponentCapability, ComponentRegistry, ComponentSchemaVersion, ComponentTypeId,
};
use nara_scene::{
    PrefabDocument, SceneComponentRecord, SceneDocument, SceneEntityId, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
};

use crate::diagnostic;

const TRANSFORM_COMPONENT: &str = "nara.transform.Transform2d";
const SPRITE_COMPONENT: &str = "nara.sprite.Sprite";
const MAX_PREFAB_COMPOSITION_COMPONENTS: usize = 4;
const MAX_PREFAB_COMPOSITION_VALUE_NODES: usize = 512;
const MAX_PREFAB_COMPOSITION_LOGICAL_BYTES: usize = 16 * 1024;

/// The bounded entity drafts admitted by the first Scene Composition product slice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneCompositionEntityKind {
    Empty,
    ColoredSprite,
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
        component: ComponentTypeId,
    },
    RemoveComponent {
        entity: SceneEntityId,
        component: ComponentTypeId,
    },
    MoveEntity {
        entity: SceneEntityId,
        to: ScenePlacement,
    },
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

pub(crate) enum GenericCompositionDocument<'a> {
    Scene(&'a SceneDocument),
    Prefab(&'a PrefabDocument),
}

impl GenericCompositionDocument<'_> {
    fn current_placement(&self, entity: &SceneEntityId) -> Option<ScenePlacement> {
        match self {
            Self::Scene(document) => ScenePlacement::current(document, entity),
            Self::Prefab(document) => ScenePlacement::current_prefab(document, entity),
        }
    }

    fn subtree_contains(&self, root: &SceneEntityId, candidate: &SceneEntityId) -> bool {
        match self {
            Self::Scene(document) => subtree_contains(document, root, candidate),
            Self::Prefab(document) => prefab_subtree_contains(document, root, candidate),
        }
    }
}

pub(crate) fn prepare_generic_composition(
    document: GenericCompositionDocument<'_>,
    selected: Option<&SceneEntityId>,
    command: SceneCompositionCommand,
    initial_component: impl FnOnce(&ComponentTypeId) -> Option<SceneComponentRecord>,
) -> Result<PreparedSceneComposition, Diagnostic> {
    let (operation, selection) = match command {
        SceneCompositionCommand::AddComponent { entity, component } => {
            let value = initial_component(&component).ok_or_else(|| {
                diagnostic::with_component(
                    diagnostic::error(
                        "tooling.composition-template-unavailable",
                        "Component has no admitted complete creation template",
                    ),
                    &component,
                )
            })?;
            (
                ScenePatchOperation::AddComponent {
                    entity,
                    component,
                    value,
                },
                SceneCompositionSelection::Preserve,
            )
        }
        SceneCompositionCommand::RemoveComponent { entity, component } => (
            ScenePatchOperation::RemoveComponent { entity, component },
            SceneCompositionSelection::Preserve,
        ),
        SceneCompositionCommand::MoveEntity { entity, to } => {
            let from = document
                .current_placement(&entity)
                .ok_or_else(|| missing_entity(&entity))?;
            (
                ScenePatchOperation::MoveEntity { entity, from, to },
                SceneCompositionSelection::Preserve,
            )
        }
        SceneCompositionCommand::RemoveEntity { entity } => {
            let from = document
                .current_placement(&entity)
                .ok_or_else(|| missing_entity(&entity))?;
            let selection =
                if selected.is_some_and(|selected| document.subtree_contains(&entity, selected)) {
                    SceneCompositionSelection::Select(match &from.owner {
                        ScenePlacementOwner::Root => None,
                        ScenePlacementOwner::Parent { parent } => Some(parent.clone()),
                    })
                } else {
                    SceneCompositionSelection::Preserve
                };
            (
                ScenePatchOperation::RemoveEntity { entity, from },
                selection,
            )
        }
        _ => {
            return Err(diagnostic::error(
                "tooling.workspace-authoring-command-unsupported",
                "Generic composition creates empty entities; add components from their admitted templates",
            ));
        }
    };
    Ok(PreparedSceneComposition {
        patch: ScenePatchDocument::new([operation]),
        selection,
    })
}

pub(crate) fn prepare_scene_composition(
    document: &SceneDocument,
    registry: &ComponentRegistry,
    selected: Option<&SceneEntityId>,
    command: SceneCompositionCommand,
) -> Result<PreparedSceneComposition, Diagnostic> {
    match command {
        SceneCompositionCommand::CreateEntity { .. } => Err(creation_requires_source_owner()),
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

pub(crate) fn scene_entity_components(
    registry: &ComponentRegistry,
    kind: SceneCompositionEntityKind,
) -> Result<BTreeMap<ComponentTypeId, SceneComponentRecord>, Diagnostic> {
    match kind {
        SceneCompositionEntityKind::Empty => Ok(BTreeMap::new()),
        SceneCompositionEntityKind::ColoredSprite => Ok(BTreeMap::from([
            component_draft(registry, ComponentTypeId::new(TRANSFORM_COMPONENT))?,
            component_draft(registry, ComponentTypeId::new(SPRITE_COMPONENT))?,
        ])),
    }
}

/// Bounded component records for one new Prefab entity before identity allocation.
#[derive(Debug, Clone, PartialEq)]
pub struct PrefabCompositionEntityDraft {
    components: Arc<BTreeMap<ComponentTypeId, SceneComponentRecord>>,
}

impl PrefabCompositionEntityDraft {
    pub fn new(
        components: BTreeMap<ComponentTypeId, SceneComponentRecord>,
    ) -> Result<Self, PrefabCompositionDraftError> {
        if components.len() > MAX_PREFAB_COMPOSITION_COMPONENTS {
            return Err(PrefabCompositionDraftError::ComponentLimitExceeded);
        }
        let mut value_nodes = 0usize;
        let mut logical_bytes = 0usize;
        for (component, record) in &components {
            let cost = record.value.cost();
            value_nodes = value_nodes.saturating_add(cost.nodes());
            logical_bytes = logical_bytes
                .saturating_add(component.as_str().len())
                .saturating_add(cost.logical_bytes());
        }
        if value_nodes > MAX_PREFAB_COMPOSITION_VALUE_NODES {
            return Err(PrefabCompositionDraftError::ValueNodeLimitExceeded);
        }
        if logical_bytes > MAX_PREFAB_COMPOSITION_LOGICAL_BYTES {
            return Err(PrefabCompositionDraftError::LogicalByteLimitExceeded);
        }
        Ok(Self {
            components: Arc::new(components),
        })
    }

    #[must_use]
    pub fn components(&self) -> &BTreeMap<ComponentTypeId, SceneComponentRecord> {
        &self.components
    }
}

/// Why an ID-free Prefab entity draft exceeded the Editor composition envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabCompositionDraftError {
    ComponentLimitExceeded,
    ValueNodeLimitExceeded,
    LogicalByteLimitExceeded,
}

impl Display for PrefabCompositionDraftError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ComponentLimitExceeded => "prefab composition component limit exceeded",
            Self::ValueNodeLimitExceeded => "prefab composition value-node limit exceeded",
            Self::LogicalByteLimitExceeded => "prefab composition logical-byte limit exceeded",
        })
    }
}

impl Error for PrefabCompositionDraftError {}

/// UI-neutral structural command for one persistent Prefab source.
#[derive(Debug, Clone, PartialEq)]
pub enum PrefabCompositionCommand {
    CreateEntity {
        draft: PrefabCompositionEntityDraft,
        placement: ScenePlacement,
    },
    MoveEntity {
        entity: SceneEntityId,
        to: ScenePlacement,
    },
    RemoveEntity {
        entity: SceneEntityId,
    },
    AddComponent {
        entity: SceneEntityId,
        component: ComponentTypeId,
    },
    RemoveComponent {
        entity: SceneEntityId,
        component: ComponentTypeId,
    },
}

/// Exact structural result used by the Adapter to restore hierarchy focus.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrefabCompositionOutcome {
    Created {
        entity: SceneEntityId,
    },
    Moved {
        entity: SceneEntityId,
    },
    Removed {
        entity: SceneEntityId,
        selection: Option<SceneEntityId>,
    },
    NoOp {
        entity: SceneEntityId,
    },
    ComponentAdded {
        entity: SceneEntityId,
        component: ComponentTypeId,
    },
    ComponentRemoved {
        entity: SceneEntityId,
        component: ComponentTypeId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PrefabCompositionSelection {
    Preserve,
    Select(Option<SceneEntityId>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct PreparedPrefabComposition {
    patch: Option<ScenePatchDocument>,
    selection: PrefabCompositionSelection,
    outcome: PrefabCompositionOutcome,
}

impl PreparedPrefabComposition {
    pub(crate) const fn patch(&self) -> Option<&ScenePatchDocument> {
        self.patch.as_ref()
    }

    pub(crate) const fn selection(&self) -> &PrefabCompositionSelection {
        &self.selection
    }

    pub(crate) const fn outcome(&self) -> &PrefabCompositionOutcome {
        &self.outcome
    }
}

pub(crate) fn prepare_prefab_composition(
    document: &PrefabDocument,
    registry: &ComponentRegistry,
    selected: Option<&SceneEntityId>,
    command: PrefabCompositionCommand,
) -> Result<PreparedPrefabComposition, Diagnostic> {
    match command {
        PrefabCompositionCommand::AddComponent { entity, component } => {
            let (component, value) = component_draft(registry, component)?;
            Ok(PreparedPrefabComposition {
                patch: Some(ScenePatchDocument::new([
                    ScenePatchOperation::AddComponent {
                        entity: entity.clone(),
                        component: component.clone(),
                        value,
                    },
                ])),
                selection: PrefabCompositionSelection::Preserve,
                outcome: PrefabCompositionOutcome::ComponentAdded { entity, component },
            })
        }
        PrefabCompositionCommand::RemoveComponent { entity, component } => {
            admitted_component(registry, component.clone())?;
            Ok(PreparedPrefabComposition {
                patch: Some(ScenePatchDocument::new([
                    ScenePatchOperation::RemoveComponent {
                        entity: entity.clone(),
                        component: component.clone(),
                    },
                ])),
                selection: PrefabCompositionSelection::Preserve,
                outcome: PrefabCompositionOutcome::ComponentRemoved { entity, component },
            })
        }
        PrefabCompositionCommand::CreateEntity { .. } => Err(creation_requires_source_owner()),
        PrefabCompositionCommand::MoveEntity { entity, to } => {
            let from = ScenePlacement::current_prefab(document, &entity)
                .ok_or_else(|| missing_prefab_entity(&entity))?;
            if to.matches_current_prefab(document, &entity) {
                return Ok(PreparedPrefabComposition {
                    patch: None,
                    selection: PrefabCompositionSelection::Preserve,
                    outcome: PrefabCompositionOutcome::NoOp { entity },
                });
            }
            Ok(PreparedPrefabComposition {
                patch: Some(ScenePatchDocument::new([ScenePatchOperation::MoveEntity {
                    entity: entity.clone(),
                    from,
                    to,
                }])),
                selection: PrefabCompositionSelection::Preserve,
                outcome: PrefabCompositionOutcome::Moved { entity },
            })
        }
        PrefabCompositionCommand::RemoveEntity { entity } => {
            let from = ScenePlacement::current_prefab(document, &entity)
                .ok_or_else(|| missing_prefab_entity(&entity))?;
            let replacement_selection = selected
                .filter(|selected| prefab_subtree_contains(document, &entity, selected))
                .map(|_| match &from.owner {
                    ScenePlacementOwner::Root => None,
                    ScenePlacementOwner::Parent { parent } => Some(parent.clone()),
                });
            let selection = replacement_selection
                .clone()
                .unwrap_or_else(|| selected.cloned());
            Ok(PreparedPrefabComposition {
                patch: Some(ScenePatchDocument::new([
                    ScenePatchOperation::RemoveEntity {
                        entity: entity.clone(),
                        from,
                    },
                ])),
                selection: replacement_selection.map_or(
                    PrefabCompositionSelection::Preserve,
                    PrefabCompositionSelection::Select,
                ),
                outcome: PrefabCompositionOutcome::Removed { entity, selection },
            })
        }
    }
}

fn prefab_subtree_contains(
    document: &PrefabDocument,
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

fn missing_prefab_entity(entity: &SceneEntityId) -> Diagnostic {
    diagnostic::with_entity(
        diagnostic::error(
            "tooling.prefab-composition-missing-entity",
            "Prefab composition targets an entity that is not in the document",
        ),
        entity,
    )
}

fn creation_requires_source_owner() -> Diagnostic {
    diagnostic::error(
        "tooling.composition-creation-requires-source-owner",
        "Entity creation must be submitted to the source owner, not lowered around a reserved ID",
    )
}

fn component_draft(
    registry: &ComponentRegistry,
    component: ComponentTypeId,
) -> Result<(ComponentTypeId, SceneComponentRecord), Diagnostic> {
    let (component_id, version) = admitted_component(registry, component)?;
    let value = registry
        .schema(&component_id)
        .and_then(|schema| schema.authoring_initial_value())
        .cloned()
        .ok_or_else(|| {
            diagnostic::with_component(
                diagnostic::error(
                    "tooling.composition-template-unavailable",
                    "Component has no complete authoring creation template",
                ),
                &component_id,
            )
        })?;
    Ok((component_id, SceneComponentRecord::new(version, value)))
}

fn admitted_component(
    registry: &ComponentRegistry,
    component: ComponentTypeId,
) -> Result<(ComponentTypeId, ComponentSchemaVersion), Diagnostic> {
    if !registry.is_frozen() {
        return Err(diagnostic::error(
            "tooling.composition-registry-not-frozen",
            "scene composition requires a frozen component registry",
        ));
    }
    let component_id = component;
    let Some(schema) = registry.schema(&component_id) else {
        return Err(diagnostic::with_component(
            diagnostic::error(
                "tooling.composition-component-unavailable",
                "scene composition component is not available in this project",
            ),
            &component_id,
        ));
    };
    if !ComponentCapability::SCENE_AUTHORING
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
