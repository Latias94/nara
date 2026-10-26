//! Private comparison of declarative structure with history-free complete-value admission.

use std::collections::{BTreeMap, BTreeSet};

use nara_asset::{AssetPath, AssetRef, StableAssetId};
use nara_core::SourceUniverseId;
use nara_reflect::{ComponentSchemaVersion, ComponentTypeId, ComponentValue};
use serde::{Deserialize, Serialize};

use crate::{
    ExpandedSceneDocument, SceneAuthoringSession, SceneDocument, SceneEntityId, SceneEntityRecord,
    ScenePatchDocument, ScenePatchOperation, ScenePlacement, ScenePlacementOwner,
    SourceDocumentKey, SourceDocumentOrigin,
};

mod properties;
mod references;
mod tests;

use properties::{PropertyClaim, PropertyIntent, admit_claims, apply_claims};
use references::{ReferenceBinding, bind_property_references, bind_references};

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

    fn logical_bytes(&self) -> usize {
        match self {
            Self::Named { name, .. } => 16usize.saturating_add(name.len()),
            Self::ProjectLocation { path, .. } => 16usize.saturating_add(path.as_str().len()),
            Self::StableAsset { .. } => 16,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    content = "entity",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum EntityOrigin {
    Source(SceneEntityId),
    Addition(SceneEntityId),
}

impl EntityOrigin {
    fn of(step: &crate::PrefabProjectionStep) -> Self {
        match step {
            crate::PrefabProjectionStep::SourceEntity { entity, .. } => {
                Self::Source(entity.clone())
            }
            crate::PrefabProjectionStep::InstanceAddition { entity } => {
                Self::Addition(entity.clone())
            }
        }
    }
    fn entity(&self) -> &SceneEntityId {
        match self {
            Self::Source(entity) | Self::Addition(entity) => entity,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct InstanceHop {
    anchor: EntityOrigin,
    expected_base: SourceIdentity,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Target {
    through: Vec<InstanceHop>,
    entity: EntityOrigin,
}

impl Target {
    fn added(entity: SceneEntityId) -> Self {
        Self {
            through: Vec::new(),
            entity: EntityOrigin::Addition(entity),
        }
    }

    fn logical_bytes(&self) -> Result<usize, Conflict> {
        if self.through.len() > 32 {
            return Err(Conflict::WorkLimit);
        }
        Ok(self
            .through
            .iter()
            .fold(self.entity.entity().as_str().len(), |bytes, hop| {
                bytes
                    .saturating_add(hop.anchor.entity().as_str().len())
                    .saturating_add(hop.expected_base.logical_bytes())
            }))
    }
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
#[serde(rename_all = "snake_case")]
enum ComponentSetMode {
    Add,
    Replace,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct BoundPropertyClaim {
    claim: PropertyClaim,
    bindings: Vec<ReferenceBinding>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ComponentIntent {
    Set {
        mode: ComponentSetMode,
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
    Properties {
        target: Target,
        component: ComponentTypeId,
        version: ComponentSchemaVersion,
        claims: Vec<BoundPropertyClaim>,
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
    instance_bases: BTreeMap<SceneEntityId, SourceDocumentKey>,
}

impl Projection {
    fn expand<R: super::PrefabSourceResolver + ?Sized>(
        anchor: SceneEntityId,
        scene: &SceneDocument,
        registry: &nara_reflect::ComponentRegistry,
        resolver: &R,
    ) -> Self {
        let mut context =
            super::PrefabExpansionContext::new(registry, resolver, Default::default(), None);
        let report = context.expand_scene_document(scene);
        assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
        let source = SourceIdentity::of(&context.resolved_instance_bases[&anchor]);
        Self {
            anchor,
            source,
            expanded: report.candidate.unwrap(),
            instance_bases: context.resolved_instance_bases,
        }
    }

    fn targets(&self) -> Result<TargetMap, Conflict> {
        let paths = self
            .expanded
            .provenance()
            .iter()
            .filter_map(|(entity, origin)| {
                let path = origin.projection_path()?;
                (path.root_anchor() == &self.anchor).then(|| {
                    (
                        path.steps()
                            .iter()
                            .map(EntityOrigin::of)
                            .collect::<Vec<_>>(),
                        entity,
                    )
                })
            })
            .collect::<BTreeMap<_, _>>();
        let mut targets = TargetMap::new();
        for (origins, entity) in &paths {
            let (last, anchors) = origins.split_last().ok_or(Conflict::OwnerAdmission)?;
            let mut through = Vec::new();
            for (index, anchor) in anchors.iter().enumerate() {
                let materialized_anchor = paths
                    .get(&origins[..=index])
                    .ok_or(Conflict::OwnerAdmission)?;
                let base = self
                    .instance_bases
                    .get(*materialized_anchor)
                    .ok_or(Conflict::OwnerAdmission)?;
                through.push(InstanceHop {
                    anchor: anchor.clone(),
                    expected_base: SourceIdentity::of(base),
                });
            }
            if targets
                .insert(
                    Target {
                        through,
                        entity: last.clone(),
                    },
                    (*entity).clone(),
                )
                .is_some()
            {
                return Err(Conflict::OwnerAdmission);
            }
        }
        Ok(targets)
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
    operation_count: &mut usize,
    operation: ScenePatchOperation,
) -> Result<(), Conflict> {
    crate::patch::apply_authoring_structure_operation(working, &operation, *operation_count)
        .map_err(|_| Conflict::OwnerAdmission)?;
    *operation_count += 1;
    Ok(())
}

fn prepare(
    projection: &Projection,
    base: &SceneAuthoringSession,
    overlay: &Overlay,
) -> Result<(Option<ExpandedSceneDocument>, TargetMap), Conflict> {
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
    let limits = crate::SceneFileLimits::default();
    crate::format::validate_scene_value_budget(base.document(), limits)
        .map_err(|_| Conflict::WorkLimit)?;
    let mut targets = projection.targets()?;
    let mut working = base.document().clone();
    let mut operation_count = 0;
    let mut provenance = projection.expanded.provenance().clone();
    let mut seen = BTreeSet::new();
    for added in &overlay.additions {
        if !seen.insert(added) {
            return Err(Conflict::DuplicateIntent);
        }
        let entity = crate::authoring::allocate_entity_id(|id| {
            working.entities.iter().any(|record| &record.id == id)
        })
        .map_err(|_| Conflict::OwnerAdmission)?;
        emit(
            &mut working,
            &mut operation_count,
            ScenePatchOperation::AddEntity {
                entity: SceneEntityRecord::new(entity.clone()),
                placement: ScenePlacement::last(ScenePlacementOwner::Parent {
                    parent: projection.anchor.clone(),
                }),
            },
        )?;
        provenance.insert(
            entity.clone(),
            crate::SceneEntityProvenance::projected(
                added.clone(),
                crate::PrefabProjectionPath::from_resolved_step(
                    None,
                    &projection.anchor,
                    crate::PrefabProjectionStep::InstanceAddition {
                        entity: added.clone(),
                    },
                ),
            ),
        );
        if targets
            .insert(Target::added(added.clone()), entity)
            .is_some()
        {
            return Err(Conflict::DuplicateIntent);
        }
    }

    for entity in &overlay.additions {
        if !overlay
            .placements
            .iter()
            .any(|placement| placement.target == Target::added(entity.clone()))
        {
            return Err(Conflict::Placement);
        }
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
                    &mut operation_count,
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

    compile_components(base, &mut working, &targets, &overlay.components)?;
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
            &mut operation_count,
            ScenePatchOperation::RemoveEntity { entity, from },
        )?;
    }
    // A declaration must not vanish as a side effect of deleting its effective parent.
    for target in overlay
        .additions
        .iter()
        .map(|id| Target::added(id.clone()))
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
            ComponentIntent::Set { target, .. }
            | ComponentIntent::Suppress { target, .. }
            | ComponentIntent::Properties { target, .. } => target.clone(),
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
    let basis = base
        .authoring_schema_basis()
        .ok_or(Conflict::OwnerAdmission)?;
    basis
        .validate_effective_document(&working, limits)
        .map_err(|_| Conflict::OwnerAdmission)?;
    if &working == base.document() {
        return Ok((None, targets));
    }
    let survivors = working
        .entities
        .iter()
        .map(|record| &record.id)
        .collect::<BTreeSet<_>>();
    provenance.retain(|id, _| survivors.contains(id));
    targets.retain(|_, id| survivors.contains(id));
    let candidate = ExpandedSceneDocument::from_expansion(working, provenance)
        .map_err(|_| Conflict::OwnerAdmission)?;
    Ok((Some(candidate), targets))
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

fn compile_components(
    base: &SceneAuthoringSession,
    working: &mut SceneDocument,
    targets: &TargetMap,
    intents: &[ComponentIntent],
) -> Result<(), Conflict> {
    let mut touched = BTreeSet::new();
    for intent in intents {
        let (target, component) = match intent {
            ComponentIntent::Set {
                target, component, ..
            }
            | ComponentIntent::Suppress { target, component }
            | ComponentIntent::Properties {
                target, component, ..
            } => (target, component),
        };
        if !touched.insert((target, component)) {
            return Err(Conflict::DuplicateIntent);
        }
        let entity = resolve(targets, target)?;
        let record = working
            .entities
            .iter_mut()
            .find(|record| record.id == entity)
            .ok_or(Conflict::Component)?;
        let existing = record.components.get(component);
        match intent {
            ComponentIntent::Set {
                mode: ComponentSetMode::Add,
                ..
            } if existing.is_some() => return Err(Conflict::Component),
            ComponentIntent::Set {
                mode: ComponentSetMode::Replace,
                ..
            }
            | ComponentIntent::Properties { .. }
            | ComponentIntent::Suppress { .. }
                if existing.is_none() =>
            {
                return Err(Conflict::Component);
            }
            _ => {}
        }
        if matches!(intent, ComponentIntent::Suppress { .. }) {
            record.components.remove(component);
            continue;
        }
        let schema = base
            .authoring_catalog_components()
            .find_map(|(schema, _)| (schema.id() == component).then_some(schema))
            .ok_or(Conflict::Component)?;
        if let ComponentIntent::Properties {
            version, claims, ..
        } = intent
        {
            if claims.len() > 64 {
                return Err(Conflict::WorkLimit);
            }
            admit_claims(&claims.iter().map(|claim| &claim.claim).collect::<Vec<_>>())?;
            let original = existing.ok_or(Conflict::Component)?;
            if original.version != *version || schema.version() != *version {
                return Err(Conflict::Component);
            }
            let effective = nara_reflect::effective_component_value(
                schema,
                &original.value,
                Default::default(),
            )
            .map_err(|_| Conflict::Component)?;
            let mut bound = Vec::new();
            for claim in claims {
                let field = schema
                    .fields()
                    .iter()
                    .find(|field| field.id() == &claim.claim.path.root)
                    .ok_or(Conflict::Component)?;
                let intent = match &claim.claim.intent {
                    PropertyIntent::Set(value) => {
                        let shape = if claim.claim.path.steps.is_empty() {
                            field.shape()
                        } else {
                            nara_reflect::resolve_component_property(
                                schema,
                                *version,
                                &effective,
                                &claim.claim.path,
                            )
                            .map_err(|_| Conflict::Component)?
                            .shape
                        };
                        PropertyIntent::Set(bind_property_references(
                            field,
                            shape,
                            value,
                            &claim.bindings,
                            targets,
                        )?)
                    }
                    PropertyIntent::Absent
                        if !claim.bindings.is_empty() && field.default_value().is_none() =>
                    {
                        return Err(Conflict::Reference);
                    }
                    PropertyIntent::Absent => PropertyIntent::Absent,
                };
                bound.push(PropertyClaim {
                    path: claim.claim.path.clone(),
                    intent,
                });
            }
            let mut value = apply_claims(schema, *version, &original.value, &bound)?;
            // Absence may expose a Schema default containing local references. Bind that new
            // effective payload explicitly; existing inherited references stay untouched.
            for claim in claims {
                if matches!(&claim.claim.intent, PropertyIntent::Absent) {
                    let field = schema
                        .fields()
                        .iter()
                        .find(|field| field.id() == &claim.claim.path.root)
                        .ok_or(Conflict::Component)?;
                    if let Some(default) = field.default_value() {
                        let bound = bind_property_references(
                            field,
                            field.shape(),
                            default,
                            &claim.bindings,
                            targets,
                        )?;
                        // Only the derived candidate gains default storage. Reuse Reflect's
                        // admitted default plan to create lawful missing Map ancestors before
                        // replacing this default's reference slots; the retained claim stays Absent.
                        value = nara_reflect::effective_component_value_owned(
                            schema,
                            value,
                            Default::default(),
                        )
                        .map_err(|_| Conflict::Component)?;
                        value
                            .set_path(field.path(), bound)
                            .map_err(|_| Conflict::Component)?;
                    }
                }
            }
            let mut replacement = original.clone();
            replacement.value = value;
            record.components.insert(component.clone(), replacement);
            continue;
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
        record.components.insert(
            component.clone(),
            crate::SceneComponentRecord::new(*version, value),
        );
    }
    Ok(())
}
