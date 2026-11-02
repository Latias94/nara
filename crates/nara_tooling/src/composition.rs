use std::{
    collections::{BTreeMap, BTreeSet},
    error::Error,
    fmt::{self, Display, Formatter},
    sync::Arc,
};

use nara_diagnostic::Diagnostic;
use nara_reflect::{
    ComponentCapability, ComponentRegistrySnapshot, ComponentSchemaVersion, ComponentTypeId,
};
use nara_scene::{
    PrefabDocument, PrefabEntityOrigin, PrefabEntityTarget, SceneComponentRecord, SceneDocument,
    SceneEntityId, SceneEntityPlacement, SceneEntityRecord, ScenePatchDocument,
    ScenePatchOperation, ScenePlacement, ScenePlacementParent, ScenePlacementSlot,
    SourceLocalSceneHierarchy,
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
    /// Removes a source-local branch admitted by the shared local hierarchy query.
    RemoveEntity { entity: SceneEntityId },
    AddComponent {
        entity: SceneEntityId,
        component: ComponentTypeId,
    },
    RemoveComponent {
        entity: SceneEntityId,
        component: ComponentTypeId,
    },
    MoveEntity {
        target: PrefabEntityTarget,
        to: ScenePlacement,
    },
    /// Resumes inherited placement. Direct source entities always retain an explicit placement.
    RevertPlacement { target: PrefabEntityTarget },
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
    fn placements(&self) -> &[SceneEntityPlacement] {
        match self {
            Self::Scene(document) => &document.placements,
            Self::Prefab(document) => &document.placements,
        }
    }

    fn records(&self) -> &[SceneEntityRecord] {
        match self {
            Self::Scene(document) => &document.entities,
            Self::Prefab(document) => &document.entities,
        }
    }

    fn contains(&self, entity: &SceneEntityId) -> bool {
        match self {
            Self::Scene(document) => document.entities.iter().any(|record| &record.id == entity),
            Self::Prefab(document) => document.entities.iter().any(|record| &record.id == entity),
        }
    }

    fn local_hierarchy(&self) -> Result<SourceLocalSceneHierarchy, Diagnostic> {
        match self {
            Self::Scene(document) => document.local_hierarchy(Default::default()),
            Self::Prefab(document) => document.local_hierarchy(Default::default()),
        }
        .map_err(|_| removal_requires_local_hierarchy())
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
        SceneCompositionCommand::MoveEntity { target, to } => {
            return Ok(PreparedSceneComposition {
                patch: ScenePatchDocument::set_placement(document.placements(), target, to),
                selection: SceneCompositionSelection::Preserve,
            });
        }
        SceneCompositionCommand::RevertPlacement { target } => {
            return Ok(PreparedSceneComposition {
                patch: ScenePatchDocument::revert_placement(document.placements(), &target)?,
                selection: SceneCompositionSelection::Preserve,
            });
        }
        SceneCompositionCommand::RemoveEntity { entity } => {
            if !document.contains(&entity) {
                return Err(missing_entity(&entity));
            }
            let hierarchy = document.local_hierarchy()?;
            let (patch, replacement) = removal_patch(
                document.placements(),
                document.records(),
                &hierarchy,
                &entity,
                selected,
            )?;
            return Ok(PreparedSceneComposition {
                patch,
                selection: replacement.map_or(
                    SceneCompositionSelection::Preserve,
                    SceneCompositionSelection::Select,
                ),
            });
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
    registry: &ComponentRegistrySnapshot,
    selected: Option<&SceneEntityId>,
    command: SceneCompositionCommand,
) -> Result<PreparedSceneComposition, Diagnostic> {
    match command {
        SceneCompositionCommand::CreateEntity { .. } => Err(creation_requires_source_owner()),
        SceneCompositionCommand::RemoveEntity { entity } => {
            require_entity(document, &entity)?;
            let hierarchy = document
                .local_hierarchy(Default::default())
                .map_err(|_| removal_requires_local_hierarchy())?;
            let (patch, replacement) = removal_patch(
                &document.placements,
                &document.entities,
                &hierarchy,
                &entity,
                selected,
            )?;
            Ok(PreparedSceneComposition {
                patch,
                selection: replacement.map_or(
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
        SceneCompositionCommand::MoveEntity { target, to } => Ok(PreparedSceneComposition {
            patch: ScenePatchDocument::set_placement(&document.placements, target, to),
            selection: SceneCompositionSelection::Preserve,
        }),
        SceneCompositionCommand::RevertPlacement { target } => Ok(PreparedSceneComposition {
            patch: ScenePatchDocument::revert_placement(&document.placements, &target)?,
            selection: SceneCompositionSelection::Preserve,
        }),
    }
}

pub(crate) fn scene_entity_components(
    registry: &ComponentRegistrySnapshot,
    kind: SceneCompositionEntityKind,
) -> Result<BTreeMap<ComponentTypeId, SceneComponentRecord>, Diagnostic> {
    scene_entity_component_ids(kind)
        .iter()
        .map(|component| component_draft(registry, ComponentTypeId::new(*component)))
        .collect()
}

pub(crate) fn scene_entity_component_ids(
    kind: SceneCompositionEntityKind,
) -> &'static [&'static str] {
    match kind {
        SceneCompositionEntityKind::Empty => &[],
        SceneCompositionEntityKind::ColoredSprite => &[TRANSFORM_COMPONENT, SPRITE_COMPONENT],
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
                .saturating_add(record.owner().map_or(0, |owner| owner.as_str().len()))
                .saturating_add(cost.logical_bytes());
            for (field, value) in &record.contextual_defaults {
                let cost = value.cost();
                value_nodes = value_nodes.saturating_add(1).saturating_add(cost.nodes());
                logical_bytes = logical_bytes
                    .saturating_add(field.as_str().len())
                    .saturating_add(cost.logical_bytes());
            }
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
        target: PrefabEntityTarget,
        to: ScenePlacement,
    },
    RevertPlacement {
        target: PrefabEntityTarget,
    },
    /// Removes a source-local branch; routed topology requires actual source compilation.
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
        target: PrefabEntityTarget,
    },
    PlacementReverted {
        target: PrefabEntityTarget,
    },
    Removed {
        entity: SceneEntityId,
        selection: Option<SceneEntityId>,
    },
    NoOp {
        target: PrefabEntityTarget,
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
    registry: &ComponentRegistrySnapshot,
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
        PrefabCompositionCommand::MoveEntity { target, to } => {
            if document
                .placements
                .iter()
                .any(|entry| entry.target == target && entry.placement == to)
            {
                return Ok(PreparedPrefabComposition {
                    patch: None,
                    selection: PrefabCompositionSelection::Preserve,
                    outcome: PrefabCompositionOutcome::NoOp { target },
                });
            }
            Ok(PreparedPrefabComposition {
                patch: Some(ScenePatchDocument::set_placement(
                    &document.placements,
                    target.clone(),
                    to,
                )),
                selection: PrefabCompositionSelection::Preserve,
                outcome: PrefabCompositionOutcome::Moved { target },
            })
        }
        PrefabCompositionCommand::RevertPlacement { target } => {
            let patch = ScenePatchDocument::revert_placement(&document.placements, &target)?;
            let present = document
                .placements
                .iter()
                .any(|entry| entry.target == target);
            Ok(PreparedPrefabComposition {
                patch: present.then_some(patch),
                selection: PrefabCompositionSelection::Preserve,
                outcome: if present {
                    PrefabCompositionOutcome::PlacementReverted { target }
                } else {
                    PrefabCompositionOutcome::NoOp { target }
                },
            })
        }
        PrefabCompositionCommand::RemoveEntity { entity } => {
            if !document.entities.iter().any(|record| record.id == entity) {
                return Err(missing_prefab_entity(&entity));
            }
            let hierarchy = document
                .local_hierarchy(Default::default())
                .map_err(|_| removal_requires_local_hierarchy())?;
            let (patch, replacement) = removal_patch(
                &document.placements,
                &document.entities,
                &hierarchy,
                &entity,
                selected,
            )?;
            let selection = replacement.clone().unwrap_or_else(|| selected.cloned());
            Ok(PreparedPrefabComposition {
                patch: Some(patch),
                selection: replacement.map_or(
                    PrefabCompositionSelection::Preserve,
                    PrefabCompositionSelection::Select,
                ),
                outcome: PrefabCompositionOutcome::Removed { entity, selection },
            })
        }
    }
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
    registry: &ComponentRegistrySnapshot,
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
    registry: &ComponentRegistrySnapshot,
    component: ComponentTypeId,
) -> Result<(ComponentTypeId, ComponentSchemaVersion), Diagnostic> {
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

fn missing_entity(entity: &SceneEntityId) -> Diagnostic {
    diagnostic::with_entity(
        diagnostic::error(
            "tooling.composition-missing-entity",
            "scene composition targets an entity that is not in the document",
        ),
        entity,
    )
}

/// Selects the branch from the shared source-local observation, never declaration-vector order.
/// Repairs owned sibling intent; final admission still rejects surviving value references.
fn removal_patch(
    placements: &[SceneEntityPlacement],
    records: &[SceneEntityRecord],
    hierarchy: &SourceLocalSceneHierarchy,
    entity: &SceneEntityId,
    selected: Option<&SceneEntityId>,
) -> Result<(ScenePatchDocument, Option<Option<SceneEntityId>>), Diagnostic> {
    if hierarchy.children(entity).is_none() {
        return Err(missing_entity(entity));
    }
    let mut removed = BTreeSet::new();
    let mut pending = vec![entity.clone()];
    while let Some(entity) = pending.pop() {
        if removed.insert(entity.clone()) {
            pending.extend(hierarchy.children(&entity).into_iter().flatten().cloned());
        }
    }
    let replacement = selected
        .filter(|selected| removed.contains(*selected))
        .map(|_| {
            placements
                .iter()
                .find_map(|entry| match &entry.target.entity {
                    PrefabEntityOrigin::SourceEntity(parent)
                        if entry.target.through.is_empty()
                            && hierarchy
                                .children(parent)
                                .is_some_and(|children| children.contains(entity)) =>
                    {
                        Some(parent.clone())
                    }
                    _ => None,
                })
        });
    let value = removal_placements(placements, records, hierarchy, &removed)?;
    let mut operations = removed
        .iter()
        .cloned()
        .map(|entity| ScenePatchOperation::RemoveEntity { entity })
        .collect::<Vec<_>>();
    operations.push(ScenePatchOperation::SetPlacements {
        expected: placements.to_vec(),
        value,
    });
    Ok((ScenePatchDocument::new(operations), replacement))
}

fn source_local_entity(target: &PrefabEntityTarget) -> Option<&SceneEntityId> {
    match &target.entity {
        PrefabEntityOrigin::SourceEntity(entity) if target.through.is_empty() => Some(entity),
        _ => None,
    }
}

fn removal_placements(
    placements: &[SceneEntityPlacement],
    records: &[SceneEntityRecord],
    hierarchy: &SourceLocalSceneHierarchy,
    removed: &BTreeSet<SceneEntityId>,
) -> Result<Vec<SceneEntityPlacement>, Diagnostic> {
    let survives = |entry: &&SceneEntityPlacement| {
        !source_local_entity(&entry.target).is_some_and(|entity| removed.contains(entity))
    };
    let mut affected_parents = BTreeSet::new();
    for entry in placements.iter().filter(survives) {
        let sibling = match &entry.placement.slot {
            ScenePlacementSlot::Before(sibling) | ScenePlacementSlot::After(sibling) => sibling,
            ScenePlacementSlot::First | ScenePlacementSlot::Last => continue,
        };
        if source_local_entity(sibling).is_some_and(|entity| removed.contains(entity)) {
            let parent = match &entry.placement.parent {
                ScenePlacementParent::Root => None,
                ScenePlacementParent::Entity(parent) => {
                    Some(source_local_entity(parent).ok_or_else(removal_requires_local_hierarchy)?)
                }
            };
            affected_parents.insert(parent);
        }
    }
    for record in records {
        if record.prefab.is_some() && affected_parents.contains(&Some(&record.id)) {
            return Err(diagnostic::with_entity(
                diagnostic::error(
                    "tooling.composition-removal-requires-source-context",
                    "Repairing sibling placements below a Prefab anchor requires the actual source context",
                ),
                &record.id,
            ));
        }
    }

    // Reauthor the entire affected sequence. Reanchoring only a dangling pin can conflict with
    // another survivor's Before/After intent and create a cycle or change the surviving order.
    let mut previous_siblings = BTreeMap::new();
    for parent in affected_parents {
        let siblings = match parent {
            None => hierarchy.roots(),
            Some(parent) => hierarchy
                .children(parent)
                .ok_or_else(removal_requires_local_hierarchy)?,
        };
        let mut previous = None;
        for sibling in siblings
            .iter()
            .filter(|sibling| !removed.contains(*sibling))
        {
            previous_siblings.insert(sibling, previous);
            previous = Some(sibling);
        }
    }
    Ok(placements
        .iter()
        .filter(survives)
        .map(|entry| {
            let mut value = entry.clone();
            if let Some(previous) =
                source_local_entity(&entry.target).and_then(|entity| previous_siblings.get(entity))
            {
                value.placement.slot = previous.map_or(ScenePlacementSlot::First, |previous| {
                    ScenePlacementSlot::After(PrefabEntityTarget::source(previous.clone()))
                });
            }
            value
        })
        .collect())
}

fn removal_requires_local_hierarchy() -> Diagnostic {
    diagnostic::error(
        "tooling.composition-removal-requires-local-hierarchy",
        "Source-local branch removal requires complete local topology; projected targets require actual source compilation",
    )
}
