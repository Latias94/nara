//! Final hierarchy compilation for one declaring overlay's expanded fragment.

use std::collections::BTreeMap;

use crate::{SceneDocument, SceneEntityId};

use super::{
    PrefabComponentIntent, PrefabEntityTarget, PrefabExpansionFailureKind, PrefabOverlay,
    PrefabPlacementParent, PrefabPlacementSlot,
};

/// The fragment's roots have no physical outer anchor here. Under a nested anchor,
/// selected Prefab roots and containing-layer children remain separate ordering domains.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Sequence {
    Root,
    NestedRoots(usize),
    Children(usize),
}

impl Sequence {
    fn parent(self) -> Option<usize> {
        match self {
            Self::Root => None,
            Self::NestedRoots(parent) | Self::Children(parent) => Some(parent),
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
/// `nested_roots` describes actual nested-root prefixes, excluding the outer physical anchor.
/// Work uses explicit stacks bounded by the admitted entities and placement declarations;
/// this helper neither replays edit operations nor imposes a second private input limit.
pub(super) fn apply_layout(
    document: &mut SceneDocument,
    targets: &mut BTreeMap<PrefabEntityTarget, SceneEntityId>,
    overlay: &PrefabOverlay,
    nested_roots: &mut BTreeMap<SceneEntityId, Vec<SceneEntityId>>,
) -> Result<(), PrefabExpansionFailureKind> {
    let mut index = EntityIndex::new();
    for (ordinal, record) in document.entities.iter().enumerate() {
        if index.insert(record.id.clone(), ordinal).is_some() {
            return Err(PrefabExpansionFailureKind::Placement);
        }
    }
    for entity in targets.values() {
        if !index.contains_key(entity) {
            return Err(PrefabExpansionFailureKind::MissingTarget);
        }
    }
    let (mut sequences, base_order) = read_sequences(document, nested_roots, &index)?;
    let mut claimed = vec![false; document.entities.len()];
    let mut placements = Vec::with_capacity(overlay.placements.len());
    for placement in &overlay.placements {
        let entity = resolve(&placement.target, targets, &index)?;
        if std::mem::replace(&mut claimed[entity], true) {
            return Err(PrefabExpansionFailureKind::DuplicateIntent);
        }
        let sequence = match &placement.parent {
            PrefabPlacementParent::Root => Sequence::Root,
            PrefabPlacementParent::Entity(parent) => {
                let parent = resolve(parent, targets, &index)?;
                // Each entity is claimed once, so its membership still describes the input.
                // Same-parent reordering cannot cross the nested-root/child boundary.
                if sequences[entity].parent() == Some(parent) {
                    sequences[entity]
                } else {
                    Sequence::Children(parent)
                }
            }
        };
        let slot = match &placement.slot {
            PrefabPlacementSlot::First => Slot::First,
            PrefabPlacementSlot::Last => Slot::Last,
            PrefabPlacementSlot::Before(anchor) => Slot::Before(resolve(anchor, targets, &index)?),
            PrefabPlacementSlot::After(anchor) => Slot::After(resolve(anchor, targets, &index)?),
        };
        sequences[entity] = sequence;
        placements.push(Placement {
            entity,
            sequence,
            slot,
        });
    }
    let mut added = vec![false; document.entities.len()];
    for addition in &overlay.additions {
        let entity = resolve(
            &PrefabEntityTarget::addition(addition.id.clone()),
            targets,
            &index,
        )?;
        if std::mem::replace(&mut added[entity], true) {
            return Err(PrefabExpansionFailureKind::DuplicateIntent);
        }
        if !claimed[entity] {
            return Err(PrefabExpansionFailureKind::Placement);
        }
    }

    let ordered = order_sequences(&sequences, base_order, &placements, &claimed)?;
    let suppressed = suppression_mask(&sequences, &ordered, overlay, targets, &index)?;
    check_surviving_intents(overlay, targets, &index, &suppressed)?;

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
        record.children = survivors(Sequence::NestedRoots(entity));
        record
            .children
            .extend(survivors(Sequence::Children(entity)));
    }
    nested_roots.retain(|anchor, _| index.get(anchor).is_none_or(|entity| !suppressed[*entity]));
    for (anchor, roots) in nested_roots.iter_mut() {
        if let Some(entity) = index.get(anchor) {
            *roots = survivors(Sequence::NestedRoots(*entity));
        }
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
) -> Result<usize, PrefabExpansionFailureKind> {
    targets
        .get(target)
        .and_then(|entity| index.get(entity))
        .copied()
        .ok_or(PrefabExpansionFailureKind::MissingTarget)
}

fn read_sequences(
    document: &SceneDocument,
    nested_roots: &BTreeMap<SceneEntityId, Vec<SceneEntityId>>,
    index: &EntityIndex,
) -> Result<(Vec<Sequence>, Sequences), PrefabExpansionFailureKind> {
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
        let roots = nested_roots.get(&record.id);
        let prefix = roots.map_or(0, Vec::len);
        if let Some(roots) = roots {
            if !record.children.starts_with(roots) {
                return Err(PrefabExpansionFailureKind::Placement);
            }
            record_sequence(
                Sequence::NestedRoots(entity),
                roots,
                index,
                &mut membership,
                &mut ordered,
            )?;
        }
        record_sequence(
            Sequence::Children(entity),
            &record.children[prefix..],
            index,
            &mut membership,
            &mut ordered,
        )?;
    }
    let membership = membership
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or(PrefabExpansionFailureKind::Placement)?;
    Ok((membership, ordered))
}

fn record_sequence(
    sequence: Sequence,
    entities: &[SceneEntityId],
    index: &EntityIndex,
    membership: &mut [Option<Sequence>],
    ordered: &mut Sequences,
) -> Result<(), PrefabExpansionFailureKind> {
    let mut members = Vec::with_capacity(entities.len());
    for entity in entities {
        let entity = *index
            .get(entity)
            .ok_or(PrefabExpansionFailureKind::Placement)?;
        if membership[entity].replace(sequence).is_some() {
            return Err(PrefabExpansionFailureKind::Placement);
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
) -> Result<Sequences, PrefabExpansionFailureKind> {
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
                    return Err(PrefabExpansionFailureKind::Placement);
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
                            return Err(PrefabExpansionFailureKind::Placement);
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
        return Err(PrefabExpansionFailureKind::Placement);
    }
    Ok(ordered)
}

fn suppression_mask(
    sequences: &[Sequence],
    ordered: &Sequences,
    overlay: &PrefabOverlay,
    targets: &BTreeMap<PrefabEntityTarget, SceneEntityId>,
    index: &EntityIndex,
) -> Result<Vec<bool>, PrefabExpansionFailureKind> {
    let mut suppressed = vec![false; sequences.len()];
    for target in &overlay.suppressions {
        let entity = resolve(target, targets, index)?;
        if std::mem::replace(&mut suppressed[entity], true) {
            return Err(PrefabExpansionFailureKind::DuplicateIntent);
        }
    }
    let mut entered = vec![false; sequences.len()];
    let mut visited = 0;
    let mut pending = ordered.get(&Sequence::Root).cloned().unwrap_or_default();
    while let Some(entity) = pending.pop() {
        if std::mem::replace(&mut entered[entity], true) {
            return Err(PrefabExpansionFailureKind::Placement);
        }
        if let Some(parent) = sequences[entity].parent()
            && suppressed[parent]
        {
            // All explicit suppressions were marked before traversal, so ancestor overlap
            // rejects in either declaration order. Traversal visits parents before children.
            if suppressed[entity] {
                return Err(PrefabExpansionFailureKind::DuplicateIntent);
            }
            suppressed[entity] = true;
        }
        visited += 1;
        for sequence in [Sequence::Children(entity), Sequence::NestedRoots(entity)] {
            if let Some(children) = ordered.get(&sequence) {
                pending.extend(children.iter().rev().copied());
            }
        }
    }
    // Every entity has exactly one owner. Any unreachable remainder is a parent cycle,
    // including a cycle that an explicit suppression would otherwise hide.
    if visited != sequences.len() {
        return Err(PrefabExpansionFailureKind::Placement);
    }
    Ok(suppressed)
}

fn check_surviving_intents(
    overlay: &PrefabOverlay,
    targets: &BTreeMap<PrefabEntityTarget, SceneEntityId>,
    index: &EntityIndex,
    suppressed: &[bool],
) -> Result<(), PrefabExpansionFailureKind> {
    let check = |target: &PrefabEntityTarget| {
        let entity = resolve(target, targets, index)?;
        if suppressed[entity] {
            return Err(PrefabExpansionFailureKind::Placement);
        }
        Ok(())
    };
    for addition in &overlay.additions {
        check(&PrefabEntityTarget::addition(addition.id.clone()))?;
    }
    for instance in &overlay.instances {
        check(&instance.target)?;
    }
    for intent in &overlay.components {
        match intent {
            PrefabComponentIntent::Set { target, .. } => check(target)?,
            PrefabComponentIntent::Suppress { target, .. } => check(target)?,
            PrefabComponentIntent::Properties { target, .. } => check(target)?,
        }
    }
    for placement in &overlay.placements {
        check(&placement.target)?;
        if let PrefabPlacementParent::Entity(parent) = &placement.parent {
            check(parent)?;
        }
        if let PrefabPlacementSlot::Before(anchor) | PrefabPlacementSlot::After(anchor) =
            &placement.slot
        {
            check(anchor)?;
        }
    }
    Ok(())
}
