//! Final hierarchy compilation for one declaring overlay's expanded fragment.

use std::collections::BTreeMap;

use crate::draft::SceneDraft;
use crate::{SceneEntityId, SceneEntityPlacement, ScenePlacementParent, ScenePlacementSlot};

use super::{
    PrefabComponentIntent, PrefabEntityTarget, PrefabOverlay, SceneCompilationFailureKind,
};

impl crate::SceneDocument {
    /// Observes direct source topology without loading instances or interpreting components.
    /// Projected placement targets require the ordinary compiler and are refused here.
    pub fn local_hierarchy(
        &self,
        limits: crate::SceneCompilationLimits,
    ) -> Result<crate::SourceLocalSceneHierarchy, SceneCompilationFailureKind> {
        local_hierarchy(&self.placements, &self.entities, limits)
    }

    pub(crate) fn validate_local_layout_constraints(
        &self,
    ) -> Result<(), SceneCompilationFailureKind> {
        validate_local_constraints(&self.placements, &self.entities)
    }
}

impl crate::PrefabDocument {
    /// Observes direct source topology under the same desired-layout rules as Scene compilation.
    pub fn local_hierarchy(
        &self,
        limits: crate::SceneCompilationLimits,
    ) -> Result<crate::SourceLocalSceneHierarchy, SceneCompilationFailureKind> {
        local_hierarchy(&self.placements, &self.entities, limits)
    }

    pub(crate) fn validate_local_layout_constraints(
        &self,
    ) -> Result<(), SceneCompilationFailureKind> {
        validate_local_constraints(&self.placements, &self.entities)
    }
}

fn admit_local_layout(
    declarations: &[SceneEntityPlacement],
    entities: &[crate::SceneEntityRecord],
    limits: crate::SceneCompilationLimits,
) -> Result<(), SceneCompilationFailureKind> {
    use crate::provenance::provenance_string_bytes;

    if entities.len() > limits.materialized_entities().get() {
        return Err(SceneCompilationFailureKind::WorkLimit);
    }
    let maximum = limits.working_bytes().get();
    let cost = super::cost::scene_placements_cost(
        declarations,
        super::cost::PrefabOverlayCostLimits {
            nodes: maximum / 128,
            logical_bytes: maximum,
            ..super::cost::PrefabOverlayCostLimits::UNBOUNDED
        },
    )
    .map_err(|_| SceneCompilationFailureKind::WorkLimit)?;
    let bytes = cost
        .nodes
        .checked_mul(128)
        .and_then(|bytes| bytes.checked_add(cost.structural_logical_bytes))
        .and_then(|bytes| {
            entities.iter().try_fold(bytes, |bytes, entity| {
                bytes
                    .checked_add(2048)?
                    .checked_add(provenance_string_bytes(entity.id.as_str())?.checked_mul(8)?)
            })
        });
    if bytes.is_none_or(|bytes| bytes > maximum) {
        return Err(SceneCompilationFailureKind::WorkLimit);
    }
    if crate::validation::validate_source_structure(declarations, entities).has_errors() {
        return Err(SceneCompilationFailureKind::OwnerAdmission);
    }
    Ok(())
}

fn local_hierarchy(
    declarations: &[SceneEntityPlacement],
    entities: &[crate::SceneEntityRecord],
    limits: crate::SceneCompilationLimits,
) -> Result<crate::SourceLocalSceneHierarchy, SceneCompilationFailureKind> {
    use crate::draft::ExpandedSceneEntity;

    admit_local_layout(declarations, entities, limits)?;
    let mut document = SceneDraft {
        roots: entities.iter().map(|entity| entity.id.clone()).collect(),
        entities: entities
            .iter()
            .map(|entity| ExpandedSceneEntity::new(entity.id.clone()))
            .collect(),
    };
    let mut targets = entities
        .iter()
        .map(|entity| {
            (
                PrefabEntityTarget::source(entity.id.clone()),
                entity.id.clone(),
            )
        })
        .collect();
    apply_layout(&mut document, &mut targets, declarations, None)
        .map_err(|failure| failure.reason)?;
    Ok(crate::SourceLocalSceneHierarchy {
        roots: document.roots,
        children: document
            .entities
            .into_iter()
            .map(|entity| (entity.id, entity.children))
            .collect(),
    })
}

/// Proves the local constraints that do not require any projected occurrence to be loaded.
/// Unknown parent membership cuts a parent chain, but cannot erase a separate local cycle.
fn validate_local_constraints(
    declarations: &[SceneEntityPlacement],
    entities: &[crate::SceneEntityRecord],
) -> Result<(), SceneCompilationFailureKind> {
    admit_local_layout(declarations, entities, Default::default())?;
    let index = entities
        .iter()
        .enumerate()
        .map(|(index, entity)| (&entity.id, index))
        .collect::<BTreeMap<_, _>>();
    let direct = |target: &PrefabEntityTarget| {
        if !target.through.is_empty() {
            return None;
        }
        match &target.entity {
            crate::PrefabEntityOrigin::SourceEntity(entity) => index.get(entity).copied(),
            crate::PrefabEntityOrigin::InstanceAddition(_) => None,
        }
    };
    #[derive(Clone, Copy)]
    enum KnownParent {
        Root,
        LocalEntity(usize),
        ProjectedEntity,
    }
    let mut membership = vec![KnownParent::Root; entities.len()];
    let mut placements = Vec::with_capacity(entities.len());
    for declaration in declarations {
        let Some(entity) = direct(&declaration.target) else {
            continue;
        };
        membership[entity] = match &declaration.placement.parent {
            ScenePlacementParent::Root => KnownParent::Root,
            ScenePlacementParent::Entity(parent) => {
                direct(parent).map_or(KnownParent::ProjectedEntity, KnownParent::LocalEntity)
            }
        };
        let slot = match &declaration.placement.slot {
            ScenePlacementSlot::First => Slot::First,
            ScenePlacementSlot::Last => Slot::Last,
            ScenePlacementSlot::Before(anchor) => direct(anchor).map_or(Slot::Last, Slot::Before),
            ScenePlacementSlot::After(anchor) => direct(anchor).map_or(Slot::Last, Slot::After),
        };
        placements.push(Placement {
            entity,
            sequence: Sequence::Root,
            slot,
        });
    }
    let parent_membership = membership
        .iter()
        .map(|parent| match parent {
            KnownParent::LocalEntity(parent) => Sequence::Children(*parent),
            KnownParent::Root | KnownParent::ProjectedEntity => Sequence::Root,
        })
        .collect::<Vec<_>>();
    if parent_cycle(&parent_membership).is_some() {
        return Err(SceneCompilationFailureKind::Placement);
    }
    for placement in &placements {
        if let Slot::Before(anchor) | Slot::After(anchor) = placement.slot {
            let different = match (membership[placement.entity], membership[anchor]) {
                (KnownParent::Root, KnownParent::Root) => false,
                (KnownParent::Root, _) | (_, KnownParent::Root) => true,
                (KnownParent::LocalEntity(parent), KnownParent::LocalEntity(anchor_parent)) => {
                    parent != anchor_parent
                }
                _ => false,
            };
            if different {
                return Err(SceneCompilationFailureKind::Placement);
            }
        }
    }
    // Direct relative edges must be acyclic even when their eventual parent is unknown.
    // Use the ordinary order kernel in one constraint domain; this does not assert that those
    // entities are effective siblings or publish an approximate hierarchy.
    order_sequences(
        &vec![Sequence::Root; entities.len()],
        Sequences::new(),
        &placements,
        &vec![true; entities.len()],
    )?;
    Ok(())
}

/// Every materialized parent has one ordering domain, regardless of declaration ownership.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Sequence {
    Root,
    Children(usize),
}

impl Sequence {
    fn parent(self) -> Option<usize> {
        match self {
            Self::Root => None,
            Self::Children(parent) => Some(parent),
        }
    }
}

#[derive(Clone, Copy)]
enum Slot {
    First,
    Last,
    Before(usize),
    After(usize),
}

struct Placement {
    entity: usize,
    sequence: Sequence,
    slot: Slot,
}

type EntityIndex = BTreeMap<SceneEntityId, usize>;
type Sequences = BTreeMap<Sequence, Vec<usize>>;

/// Applies desired placement, then suppression, to an unpublished expanded fragment.
///
/// Additions already have their allocated IDs, complete records and initial root placement.
/// Work uses explicit stacks bounded by the admitted entities and placement declarations;
/// this helper neither replays edit operations nor imposes a second private input limit.
pub(super) fn apply_layout(
    document: &mut SceneDraft,
    targets: &mut BTreeMap<PrefabEntityTarget, SceneEntityId>,
    declarations: &[SceneEntityPlacement],
    overlay: Option<&PrefabOverlay>,
) -> Result<(), crate::SceneCompilationFailure> {
    let mut index = EntityIndex::new();
    for (ordinal, record) in document.entities.iter().enumerate() {
        if index.insert(record.id.clone(), ordinal).is_some() {
            return Err(SceneCompilationFailureKind::Placement.into());
        }
    }
    for entity in targets.values() {
        if !index.contains_key(entity) {
            return Err(SceneCompilationFailureKind::MissingTarget.into());
        }
    }
    let (mut sequences, base_order) = read_sequences(document, &index)?;
    let mut claimed = vec![false; document.entities.len()];
    let mut placements = Vec::with_capacity(declarations.len());
    for declaration in declarations {
        let entity = resolve(&declaration.target, targets, &index)?;
        if std::mem::replace(&mut claimed[entity], true) {
            return Err(SceneCompilationFailureKind::DuplicateIntent.into());
        }
        let sequence = match &declaration.placement.parent {
            ScenePlacementParent::Root => Sequence::Root,
            ScenePlacementParent::Entity(parent) => {
                let parent = resolve(parent, targets, &index)?;
                Sequence::Children(parent)
            }
        };
        let slot = match &declaration.placement.slot {
            ScenePlacementSlot::First => Slot::First,
            ScenePlacementSlot::Last => Slot::Last,
            ScenePlacementSlot::Before(anchor) => Slot::Before(resolve(anchor, targets, &index)?),
            ScenePlacementSlot::After(anchor) => Slot::After(resolve(anchor, targets, &index)?),
        };
        sequences[entity] = sequence;
        placements.push(Placement {
            entity,
            sequence,
            slot,
        });
    }
    let mut added = vec![false; document.entities.len()];
    for addition in overlay.into_iter().flat_map(|overlay| &overlay.additions) {
        let entity = resolve(
            &PrefabEntityTarget::addition(addition.id.clone()),
            targets,
            &index,
        )?;
        if std::mem::replace(&mut added[entity], true) {
            return Err(SceneCompilationFailureKind::DuplicateIntent.into());
        }
        if !claimed[entity] {
            return Err(SceneCompilationFailureKind::Placement.into());
        }
    }

    if let Some(entity) = parent_cycle(&sequences) {
        let mut failure: crate::SceneCompilationFailure =
            SceneCompilationFailureKind::Placement.into();
        failure.entity = Some(document.entities[entity].id.clone());
        return Err(failure);
    }
    let ordered = order_sequences(&sequences, base_order, &placements, &claimed)?;
    let suppressed = suppression_mask(&sequences, &ordered, overlay, targets, &index)?;
    check_surviving_intents(declarations, overlay, targets, &index, &suppressed)?;

    // All validation above observes the final graph. Parent rotations never need a valid
    // intermediate hierarchy, and descendants moved out before suppression remain alive.
    let ids = document
        .entities
        .iter()
        .map(|record| record.id.clone())
        .collect::<Vec<_>>();
    let survivors = |sequence| {
        ordered
            .get(&sequence)
            .into_iter()
            .flatten()
            .filter(|entity| !suppressed[**entity])
            .map(|entity| ids[*entity].clone())
            .collect::<Vec<_>>()
    };
    document.roots = survivors(Sequence::Root);
    for (entity, record) in document.entities.iter_mut().enumerate() {
        if suppressed[entity] {
            continue;
        }
        record.children = survivors(Sequence::Children(entity));
    }
    document
        .entities
        .retain(|record| !suppressed[index[&record.id]]);
    targets.retain(|_, entity| !suppressed[index[&*entity]]);
    Ok(())
}

fn resolve(
    target: &PrefabEntityTarget,
    targets: &BTreeMap<PrefabEntityTarget, SceneEntityId>,
    index: &EntityIndex,
) -> Result<usize, SceneCompilationFailureKind> {
    targets
        .get(target)
        .and_then(|entity| index.get(entity))
        .copied()
        .ok_or(SceneCompilationFailureKind::MissingTarget)
}

/// Follows the resolved final parent function, reporting a cycle member rather than its ingress.
fn parent_cycle(sequences: &[Sequence]) -> Option<usize> {
    let mut visited = vec![None; sequences.len()];
    for start in 0..sequences.len() {
        let mut current = Some(start);
        while let Some(entity) = current {
            match visited[entity] {
                Some(previous) if previous == start => return Some(entity),
                Some(_) => break,
                None => visited[entity] = Some(start),
            }
            current = sequences[entity].parent();
        }
    }
    None
}

fn read_sequences(
    document: &SceneDraft,
    index: &EntityIndex,
) -> Result<(Vec<Sequence>, Sequences), SceneCompilationFailureKind> {
    let mut membership = vec![None; document.entities.len()];
    let mut ordered = Sequences::new();
    record_sequence(
        Sequence::Root,
        &document.roots,
        index,
        &mut membership,
        &mut ordered,
    )?;
    for (entity, record) in document.entities.iter().enumerate() {
        record_sequence(
            Sequence::Children(entity),
            &record.children,
            index,
            &mut membership,
            &mut ordered,
        )?;
    }
    let membership = membership
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(SceneCompilationFailureKind::Placement)?;
    Ok((membership, ordered))
}

fn record_sequence(
    sequence: Sequence,
    entities: &[SceneEntityId],
    index: &EntityIndex,
    membership: &mut [Option<Sequence>],
    ordered: &mut Sequences,
) -> Result<(), SceneCompilationFailureKind> {
    let mut members = Vec::with_capacity(entities.len());
    for entity in entities {
        let entity = *index
            .get(entity)
            .ok_or(SceneCompilationFailureKind::Placement)?;
        if membership[entity].replace(sequence).is_some() {
            return Err(SceneCompilationFailureKind::Placement);
        }
        members.push(entity);
    }
    ordered.insert(sequence, members);
    Ok(())
}

fn order_sequences(
    sequences: &[Sequence],
    base_order: Sequences,
    placements: &[Placement],
    claimed: &[bool],
) -> Result<Sequences, SceneCompilationFailureKind> {
    let mut seeds = Sequences::new();
    let mut before = vec![Vec::new(); sequences.len()];
    let mut after = vec![Vec::new(); sequences.len()];
    // Cohorts follow declaration order. Relations are resolved against final membership,
    // without inferring a chronological sequence of moves from the declarations.
    for placement in placements {
        match placement.slot {
            Slot::First => seeds
                .entry(placement.sequence)
                .or_default()
                .push(placement.entity),
            Slot::Last => {}
            Slot::Before(anchor) | Slot::After(anchor) => {
                if sequences[anchor] != placement.sequence {
                    return Err(SceneCompilationFailureKind::Placement);
                }
                let relations = match placement.slot {
                    Slot::Before(_) => &mut before,
                    _ => &mut after,
                };
                relations[anchor].push(placement.entity);
            }
        }
    }
    for (sequence, members) in base_order {
        seeds
            .entry(sequence)
            .or_default()
            .extend(members.into_iter().filter(|entity| !claimed[*entity]));
    }
    for placement in placements {
        if matches!(placement.slot, Slot::Last) {
            seeds
                .entry(placement.sequence)
                .or_default()
                .push(placement.entity);
        }
    }

    enum Step {
        Visit(usize),
        Emit(usize),
    }
    let mut entered = vec![false; sequences.len()];
    let mut emitted = 0;
    let mut ordered = Sequences::new();
    let mut pending = Vec::new();
    for (sequence, seeds) in seeds {
        let output = ordered.entry(sequence).or_default();
        for seed in seeds {
            pending.push(Step::Visit(seed));
            while let Some(step) = pending.pop() {
                match step {
                    Step::Visit(entity) => {
                        if std::mem::replace(&mut entered[entity], true) {
                            return Err(SceneCompilationFailureKind::Placement);
                        }
                        pending.extend(after[entity].iter().rev().copied().map(Step::Visit));
                        pending.push(Step::Emit(entity));
                        pending.extend(before[entity].iter().rev().copied().map(Step::Visit));
                    }
                    Step::Emit(entity) => {
                        output.push(entity);
                        emitted += 1;
                    }
                }
            }
        }
    }
    // A relative-attachment cycle has no seed. It must reject rather than disappear.
    if emitted != sequences.len() {
        return Err(SceneCompilationFailureKind::Placement);
    }
    Ok(ordered)
}

fn suppression_mask(
    sequences: &[Sequence],
    ordered: &Sequences,
    overlay: Option<&PrefabOverlay>,
    targets: &BTreeMap<PrefabEntityTarget, SceneEntityId>,
    index: &EntityIndex,
) -> Result<Vec<bool>, SceneCompilationFailureKind> {
    let mut suppressed = vec![false; sequences.len()];
    for target in overlay
        .into_iter()
        .flat_map(|overlay| &overlay.suppressions)
    {
        let entity = resolve(target, targets, index)?;
        if std::mem::replace(&mut suppressed[entity], true) {
            return Err(SceneCompilationFailureKind::DuplicateIntent);
        }
    }
    let mut entered = vec![false; sequences.len()];
    let mut visited = 0;
    let mut pending = ordered.get(&Sequence::Root).cloned().unwrap_or_default();
    while let Some(entity) = pending.pop() {
        if std::mem::replace(&mut entered[entity], true) {
            return Err(SceneCompilationFailureKind::Placement);
        }
        if let Some(parent) = sequences[entity].parent()
            && suppressed[parent]
        {
            // All explicit suppressions were marked before traversal, so ancestor overlap
            // rejects in either declaration order. Traversal visits parents before children.
            if suppressed[entity] {
                return Err(SceneCompilationFailureKind::DuplicateIntent);
            }
            suppressed[entity] = true;
        }
        visited += 1;
        if let Some(children) = ordered.get(&Sequence::Children(entity)) {
            pending.extend(children.iter().rev().copied());
        }
    }
    // Every entity has exactly one owner. Any unreachable remainder is a parent cycle,
    // including a cycle that an explicit suppression would otherwise hide.
    if visited != sequences.len() {
        return Err(SceneCompilationFailureKind::Placement);
    }
    Ok(suppressed)
}

fn check_surviving_intents(
    declarations: &[SceneEntityPlacement],
    overlay: Option<&PrefabOverlay>,
    targets: &BTreeMap<PrefabEntityTarget, SceneEntityId>,
    index: &EntityIndex,
    suppressed: &[bool],
) -> Result<(), SceneCompilationFailureKind> {
    let check = |target: &PrefabEntityTarget| {
        let entity = resolve(target, targets, index)?;
        if suppressed[entity] {
            return Err(SceneCompilationFailureKind::Placement);
        }
        Ok(())
    };
    for addition in overlay.into_iter().flat_map(|overlay| &overlay.additions) {
        check(&PrefabEntityTarget::addition(addition.id.clone()))?;
    }
    for instance in overlay.into_iter().flat_map(|overlay| &overlay.instances) {
        check(&instance.target)?;
    }
    for intent in overlay.into_iter().flat_map(|overlay| &overlay.components) {
        match intent {
            PrefabComponentIntent::Set { target, .. } => check(target)?,
            PrefabComponentIntent::Suppress { target, .. } => check(target)?,
            PrefabComponentIntent::Properties { target, .. } => check(target)?,
        }
    }
    for declaration in declarations {
        check(&declaration.target)?;
        if let ScenePlacementParent::Entity(parent) = &declaration.placement.parent {
            check(parent)?;
        }
        if let ScenePlacementSlot::Before(anchor) | ScenePlacementSlot::After(anchor) =
            &declaration.placement.slot
        {
            check(anchor)?;
        }
    }
    Ok(())
}
