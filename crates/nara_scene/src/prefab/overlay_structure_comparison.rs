//! Private comparison of declarative structure against exact source-owned operations.

use std::collections::{BTreeMap, BTreeSet};

use nara_asset::{AssetPath, AssetRef, StableAssetId};
use nara_core::SourceUniverseId;
use nara_reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue};
use serde::{Deserialize, Serialize};

use crate::{
    ExpandedSceneDocument, PreparedSceneAuthoringEdit, SceneAuthoringSession, SceneDocument,
    SceneEntityId, SceneEntityRecord, ScenePatchDocument, ScenePatchOperation, ScenePlacement,
    ScenePlacementOwner, SourceDocumentKey, SourceDocumentOrigin,
};

mod references;
mod tests;

use references::{ReferenceBinding, bind_references};

// A private wire comparison, not the source owner's selected persistent codec.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "origin", rename_all = "snake_case", deny_unknown_fields)]
enum SourceIdentity {
    Named {
        universe: SourceUniverseId,
        name: String,
    },
    ProjectLocation {
        universe: SourceUniverseId,
        path: AssetPath,
    },
    StableAsset {
        stable_id: StableAssetId,
    },
}

impl SourceIdentity {
    fn of(key: &SourceDocumentKey) -> Self {
        match key.origin() {
            SourceDocumentOrigin::Named { universe, name } => Self::Named {
                universe: *universe,
                name: name.to_string(),
            },
            SourceDocumentOrigin::ProjectLocation { universe, path } => Self::ProjectLocation {
                universe: *universe,
                path: path.clone(),
            },
            SourceDocumentOrigin::StableAsset { stable_id } => Self::StableAsset {
                stable_id: *stable_id,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceEntity {
    source: SourceIdentity,
    entity: SceneEntityId,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "target",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Target {
    Inherited(Vec<SourceEntity>),
    Added(SceneEntityId),
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "target",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Parent {
    Root,
    Entity(Target),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "target",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Slot {
    First,
    Last,
    Before(Target),
    After(Target),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Placement {
    target: Target,
    parent: Parent,
    slot: Slot,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ComponentIntent {
    Set {
        target: Target,
        component: ComponentTypeId,
        version: ComponentSchemaVersion,
        value: ComponentValue,
        bindings: Vec<ReferenceBinding>,
    },
    Suppress {
        target: Target,
        component: ComponentTypeId,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Overlay {
    source: SourceIdentity,
    additions: Vec<SceneEntityId>,
    placements: Vec<Placement>,
    suppressions: Vec<Target>,
    components: Vec<ComponentIntent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "target",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum Conflict {
    SourceChanged,
    MissingTarget(Target),
    DuplicateIntent,
    Placement,
    Component,
    Reference,
    OwnerAdmission,
    WorkLimit,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct UnresolvedOverlay {
    overlay: Overlay,
    conflict: Conflict,
}

type TargetMap = BTreeMap<Target, SceneEntityId>;

struct Projection {
    anchor: SceneEntityId,
    source: SourceIdentity,
    expanded: ExpandedSceneDocument,
}

impl Projection {
    fn targets(&self) -> TargetMap {
        self.expanded
            .provenance()
            .iter()
            .filter_map(|(entity, provenance)| {
                let path = provenance.projection_path()?;
                if path.root_anchor() != &self.anchor {
                    return None;
                }
                let chain = path
                    .source_chain()
                    .iter()
                    .map(|step| SourceEntity {
                        source: SourceIdentity::of(step.canonical_source()),
                        entity: step.source_entity().clone(),
                    })
                    .collect();
                Some((Target::Inherited(chain), entity.clone()))
            })
            .collect()
    }
}

fn resolve(targets: &TargetMap, target: &Target) -> Result<SceneEntityId, Conflict> {
    targets
        .get(target)
        .cloned()
        .ok_or_else(|| Conflict::MissingTarget(target.clone()))
}

fn emit(
    working: &mut SceneDocument,
    operations: &mut Vec<ScenePatchOperation>,
    operation: ScenePatchOperation,
) -> Result<(), Conflict> {
    crate::patch::apply_authoring_structure_operation(working, &operation, operations.len())
        .map_err(|_| Conflict::OwnerAdmission)?;
    operations.push(operation);
    Ok(())
}

fn prepare(
    projection: &Projection,
    base: &SceneAuthoringSession,
    overlay: &Overlay,
) -> Result<(Option<PreparedSceneAuthoringEdit>, TargetMap), Conflict> {
    if projection.source != overlay.source {
        return Err(Conflict::SourceChanged);
    }
    if base.document() != projection.expanded.document() {
        return Err(Conflict::OwnerAdmission);
    }
    if overlay.additions.len()
        + overlay.placements.len()
        + overlay.suppressions.len()
        + overlay.components.len()
        > 64
    {
        return Err(Conflict::WorkLimit);
    }
    let mut targets = projection.targets();
    let mut working = base.document().clone();
    let mut operations = Vec::new();
    if !overlay.additions.is_empty() {
        let mut draft = base
            .begin_authoring_construction(base.revision())
            .map_err(|_| Conflict::OwnerAdmission)?;
        let mut locals = Vec::new();
        let mut seen = BTreeSet::new();
        for added in &overlay.additions {
            if !seen.insert(added) {
                return Err(Conflict::DuplicateIntent);
            }
            let local = draft
                .create_entity(ScenePlacement::last(ScenePlacementOwner::Parent {
                    parent: projection.anchor.clone(),
                }))
                .map_err(|_| Conflict::OwnerAdmission)?;
            locals.push((Target::Added(added.clone()), local));
        }
        let constructed = base
            .prepare_authoring_construction(&draft)
            .map_err(|_| Conflict::OwnerAdmission)?;
        for (target, local) in locals {
            targets.insert(target, constructed.entity(&local).unwrap().clone());
        }
        working = constructed.document().clone();
        operations.extend(constructed.patch().operations.iter().cloned());
    }

    let layout = layout(projection, &working, &targets, &overlay.placements)?;
    let projected_entities: BTreeSet<_> = targets.values().cloned().collect();
    let mut pending = vec![(Parent::Root, projection.anchor.clone())];
    let mut visited = BTreeSet::new();
    while let Some((parent, parent_id)) = pending.pop() {
        let mut next = None;
        // Order only this projection's subsequence; containing-owner children are not targets.
        for target in layout.get(&parent).into_iter().flatten().rev() {
            if !visited.insert(target.clone()) {
                return Err(Conflict::Placement);
            }
            let entity = resolve(&targets, target)?;
            let owner = ScenePlacementOwner::Parent {
                parent: parent_id.clone(),
            };
            let siblings = working
                .entities
                .iter()
                .find(|record| record.id == parent_id)
                .ok_or(Conflict::Placement)?
                .children
                .iter()
                .filter(|id| projected_entities.contains(*id))
                .cloned()
                .collect::<Vec<_>>();
            let in_place = match &next {
                Some(next) => siblings
                    .iter()
                    .position(|sibling| sibling == &entity)
                    .is_some_and(|index| siblings.get(index + 1) == Some(next)),
                None => siblings.last() == Some(&entity),
            };
            if !in_place {
                let to = match &next {
                    Some(next) => ScenePlacement::before(owner, next.clone()),
                    None => siblings.last().map_or_else(
                        || ScenePlacement::first(owner.clone()),
                        |last| ScenePlacement::after(owner.clone(), last.clone()),
                    ),
                };
                let from = ScenePlacement::current(&working, &entity).ok_or(Conflict::Placement)?;
                emit(
                    &mut working,
                    &mut operations,
                    ScenePatchOperation::MoveEntity {
                        entity: entity.clone(),
                        from,
                        to,
                    },
                )?;
            }
            next = Some(entity.clone());
            pending.push((Parent::Entity(target.clone()), entity));
        }
    }
    if visited.len() != targets.len() {
        return Err(Conflict::Placement);
    }

    let expected_components = lower_components(
        base,
        &mut working,
        &targets,
        &overlay.components,
        &mut operations,
    )?;
    let removed = overlay
        .suppressions
        .iter()
        .map(|target| resolve(&targets, target))
        .collect::<Result<BTreeSet<_>, _>>()?;
    if removed.len() != overlay.suppressions.len() {
        return Err(Conflict::DuplicateIntent);
    }
    for entity in &removed {
        let mut current = entity.clone();
        while let Some(ScenePlacement {
            owner: ScenePlacementOwner::Parent { parent },
            ..
        }) = ScenePlacement::current(&working, &current)
        {
            if removed.contains(&parent) {
                return Err(Conflict::DuplicateIntent);
            }
            current = parent;
        }
    }
    let mut suppressed = BTreeSet::new();
    for target in &overlay.suppressions {
        if !suppressed.insert(target) {
            return Err(Conflict::DuplicateIntent);
        }
        let entity = resolve(&targets, target)?;
        let from = ScenePlacement::current(&working, &entity)
            .ok_or_else(|| Conflict::MissingTarget(target.clone()))?;
        emit(
            &mut working,
            &mut operations,
            ScenePatchOperation::RemoveEntity { entity, from },
        )?;
    }
    // A declaration must not vanish as a side effect of deleting its effective parent.
    for target in overlay
        .additions
        .iter()
        .map(|id| Target::Added(id.clone()))
        .chain(
            overlay
                .placements
                .iter()
                .map(|placement| placement.target.clone()),
        )
        .chain(overlay.placements.iter().flat_map(|placement| {
            let parent = match &placement.parent {
                Parent::Root => None,
                Parent::Entity(target) => Some(target),
            };
            let anchor = match &placement.slot {
                Slot::Before(target) | Slot::After(target) => Some(target),
                _ => None,
            };
            parent.into_iter().chain(anchor).cloned()
        }))
        .chain(overlay.components.iter().map(|intent| match intent {
            ComponentIntent::Set { target, .. } | ComponentIntent::Suppress { target, .. } => {
                target.clone()
            }
        }))
    {
        if !working
            .entities
            .iter()
            .any(|record| record.id == targets[&target])
        {
            return Err(Conflict::Placement);
        }
    }
    if operations.is_empty() {
        return Ok((None, targets));
    }
    let prepared = base
        .prepare_authoring_edit(base.revision(), &ScenePatchDocument::new(operations))
        .map_err(|_| Conflict::OwnerAdmission)?;
    // Field lowering is not a full-value codec: optional ancestor containers can differ.
    for ((entity, component), value) in expected_components {
        let actual = prepared
            .document()
            .entities
            .iter()
            .find(|record| record.id == entity)
            .and_then(|record| record.components.get(&component));
        if actual.is_none_or(|record| record.value != value) {
            return Err(Conflict::Component);
        }
    }
    targets.retain(|_, id| {
        prepared
            .document()
            .entities
            .iter()
            .any(|record| record.id == *id)
    });
    Ok((Some(prepared), targets))
}

fn layout(
    projection: &Projection,
    document: &SceneDocument,
    targets: &TargetMap,
    placements: &[Placement],
) -> Result<BTreeMap<Parent, Vec<Target>>, Conflict> {
    let reverse: BTreeMap<_, _> = targets
        .iter()
        .map(|(target, entity)| (entity, target))
        .collect();
    let mut parents = BTreeMap::new();
    let mut base_order = BTreeMap::<Parent, Vec<Target>>::new();
    for record in &document.entities {
        let parent = if record.id == projection.anchor {
            Parent::Root
        } else if let Some(target) = reverse.get(&record.id) {
            Parent::Entity((*target).clone())
        } else {
            continue;
        };
        for child in &record.children {
            let Some(target) = reverse.get(child) else {
                continue;
            };
            parents.insert((*target).clone(), parent.clone());
            base_order
                .entry(parent.clone())
                .or_default()
                .push((*target).clone());
        }
    }
    let mut claims = BTreeMap::new();
    for placement in placements {
        resolve(targets, &placement.target)?;
        if let Parent::Entity(parent) = &placement.parent {
            resolve(targets, parent)?;
        }
        if claims.insert(placement.target.clone(), placement).is_some() {
            return Err(Conflict::DuplicateIntent);
        }
        parents.insert(placement.target.clone(), placement.parent.clone());
    }
    for target in targets
        .keys()
        .filter(|target| matches!(target, Target::Added(_)))
    {
        if !claims.contains_key(target) {
            return Err(Conflict::Placement);
        }
    }
    let mut before = BTreeMap::<Target, Vec<Target>>::new();
    let mut after = BTreeMap::<Target, Vec<Target>>::new();
    let mut first = BTreeMap::<Parent, Vec<Target>>::new();
    let mut last = BTreeMap::<Parent, Vec<Target>>::new();
    for placement in placements {
        match &placement.slot {
            Slot::First => first
                .entry(placement.parent.clone())
                .or_default()
                .push(placement.target.clone()),
            Slot::Last => last
                .entry(placement.parent.clone())
                .or_default()
                .push(placement.target.clone()),
            Slot::Before(anchor) | Slot::After(anchor) => {
                resolve(targets, anchor)?;
                if parents.get(anchor) != Some(&placement.parent) {
                    return Err(Conflict::Placement);
                }
                let entries = if matches!(placement.slot, Slot::Before(_)) {
                    &mut before
                } else {
                    &mut after
                };
                entries
                    .entry(anchor.clone())
                    .or_default()
                    .push(placement.target.clone());
            }
        }
    }
    let mut result = BTreeMap::new();
    let mut emitted = BTreeSet::new();
    for parent in parents.values().cloned().collect::<BTreeSet<_>>() {
        let mut ordered = Vec::new();
        for target in first
            .get(&parent)
            .into_iter()
            .flatten()
            .chain(
                base_order
                    .get(&parent)
                    .into_iter()
                    .flatten()
                    .filter(|target| !claims.contains_key(*target)),
            )
            .chain(last.get(&parent).into_iter().flatten())
        {
            append_slot(target, &before, &after, &mut emitted, &mut ordered)?;
        }
        result.insert(parent, ordered);
    }
    if emitted.len() != targets.len() {
        return Err(Conflict::Placement);
    }
    Ok(result)
}

fn append_slot(
    target: &Target,
    before: &BTreeMap<Target, Vec<Target>>,
    after: &BTreeMap<Target, Vec<Target>>,
    emitted: &mut BTreeSet<Target>,
    output: &mut Vec<Target>,
) -> Result<(), Conflict> {
    if !emitted.insert(target.clone()) {
        return Err(Conflict::Placement);
    }
    for child in before.get(target).into_iter().flatten() {
        append_slot(child, before, after, emitted, output)?;
    }
    output.push(target.clone());
    for child in after.get(target).into_iter().flatten() {
        append_slot(child, before, after, emitted, output)?;
    }
    Ok(())
}

fn lower_components(
    base: &SceneAuthoringSession,
    working: &mut SceneDocument,
    targets: &TargetMap,
    intents: &[ComponentIntent],
    operations: &mut Vec<ScenePatchOperation>,
) -> Result<BTreeMap<(SceneEntityId, ComponentTypeId), ComponentValue>, Conflict> {
    let mut touched = BTreeSet::new();
    let mut expected = BTreeMap::new();
    for intent in intents {
        let (target, component) = match intent {
            ComponentIntent::Set {
                target, component, ..
            }
            | ComponentIntent::Suppress { target, component } => (target, component),
        };
        if !touched.insert((target, component)) {
            return Err(Conflict::DuplicateIntent);
        }
        let entity = resolve(targets, target)?;
        if working
            .entities
            .iter()
            .find(|record| record.id == entity)
            .is_some_and(|record| record.components.contains_key(component))
        {
            emit(
                working,
                operations,
                ScenePatchOperation::RemoveComponent {
                    entity: entity.clone(),
                    component: component.clone(),
                },
            )?;
        } else if matches!(intent, ComponentIntent::Suppress { .. }) {
            return Err(Conflict::Component);
        }
        let ComponentIntent::Set {
            version,
            value,
            bindings,
            ..
        } = intent
        else {
            continue;
        };
        let schema = base
            .authoring_catalog_components()
            .find_map(|(schema, available)| {
                (available && schema.id() == component).then_some(schema)
            })
            .ok_or(Conflict::Component)?;
        if schema.version() != *version {
            return Err(Conflict::Component);
        }
        nara_reflect::validate_component_value(schema, value).map_err(|_| Conflict::Component)?;
        // Defaults can introduce references too; none may inherit the containing Scene namespace.
        let effective = nara_reflect::effective_component_value(
            schema,
            value,
            nara_reflect::ComponentValueMaterializationLimits::default(),
        )
        .map_err(|_| Conflict::Component)?;
        let value = bind_references(schema, effective.as_ref(), bindings, targets)?;
        let template = base
            .authoring_initial_component(component)
            .ok_or(Conflict::Component)?;
        emit(
            working,
            operations,
            ScenePatchOperation::AddComponent {
                entity: entity.clone(),
                component: component.clone(),
                value: template,
            },
        )?;
        for field in schema.fields() {
            operations.push(match value.get_path(field.path()) {
                Ok(value) => ScenePatchOperation::SetField {
                    entity: entity.clone(),
                    component: component.clone(),
                    component_version: *version,
                    field: field.id().clone(),
                    value: value.clone(),
                },
                Err(_) => ScenePatchOperation::RemoveField {
                    entity: entity.clone(),
                    component: component.clone(),
                    component_version: *version,
                    field: field.id().clone(),
                },
            });
        }
        expected.insert((entity, component.clone()), value);
    }
    Ok(expected)
}
