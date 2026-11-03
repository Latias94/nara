//! Source placement candidates for a cut, without reconstructing flattened hierarchy.
//!
//! Initial declaration ownership decides which constraints can move. This construction is not
//! an occurrence or semantic correspondence proof; the original compiler must admit P and Q.

use std::{borrow::Cow, mem::size_of};

use super::{
    ExtractionError, ExtractionWork, SourceCut, SourceDocumentInput,
    relocation::{AddressRole, BranchRelocation},
};
use crate::{
    PrefabEntityOrigin, PrefabEntityTarget, SceneEntityPlacement, ScenePlacement,
    ScenePlacementParent, ScenePlacementSlot,
};

#[cfg(test)]
mod coupling_tests;
#[cfg(test)]
mod tests;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PlacementDestination {
    Retained(usize),
    Extracted(usize),
}

#[derive(Debug)]
pub(super) struct PlacementSplit {
    pub(super) retained: Vec<SceneEntityPlacement>,
    pub(super) extracted: Vec<SceneEntityPlacement>,
    /// One destination per original source placement, recorded during construction. New Q
    /// root/baseline declarations have no original. This mapping is not a semantic proof.
    pub(super) originals: Vec<PlacementDestination>,
    /// Exact fresh Q baselines emitted by this constructor, separate from every original.
    pub(super) generated: Vec<usize>,
}

/// Returns retained P constraints and moved Q constraints in their original stored order.
///
/// `SourceCut` comes from the structural kernel, which rejects duplicate targets, missing local
/// placements and invalid source target grammar. Each cut record consequently has one original
/// local declaration. Cross-cut pins retain their complete intent in P; only direct moved records
/// need an explicit Q baseline. Inherited entities keep their base layout until P applies its pin.
pub(super) fn split_placements(
    cut: &SourceCut<'_>,
    relocation: &BranchRelocation<'_, '_>,
    work: &mut ExtractionWork,
) -> Result<PlacementSplit, ExtractionError> {
    let declarations = match cut.source() {
        SourceDocumentInput::Scene(source) => &source.placements,
        SourceDocumentInput::Prefab(source) => &source.placements,
    };
    let mut groups = placement_groups(cut, declarations, relocation, work)?;
    // Each original declaration produces at most one entry in either vector. Reserve both
    // placement buffers and the original-position table before allocating, including entries
    // duplicated as a retained pin and Q baseline.
    work.admit(
        declarations
            .len()
            .checked_mul(size_of::<SceneEntityPlacement>())
            .and_then(|bytes| bytes.checked_mul(2))
            .and_then(|bytes| {
                bytes.checked_add(
                    declarations
                        .len()
                        .checked_mul(size_of::<PlacementDestination>() + size_of::<usize>())?,
                )
            }),
    )?;
    let mut retained = Vec::with_capacity(declarations.len());
    let mut moved = Vec::with_capacity(declarations.len());
    let mut originals = Vec::with_capacity(declarations.len());
    let mut generated = Vec::with_capacity(declarations.len());
    let mut root_seen = false;
    let mut local_count = 0usize;

    for (declaration_index, original) in declarations.iter().enumerate() {
        work.placement_copy(original)?;
        let mut candidate = original.clone();
        let direct = if original.target.through.is_empty() {
            match &original.target.entity {
                PrefabEntityOrigin::SourceEntity(entity) => Some(entity),
                PrefabEntityOrigin::InstanceAddition(_) => {
                    return Err(ExtractionError::InvalidSource);
                }
            }
        } else {
            None
        };
        let is_root = if let Some(entity) = direct {
            work.admit(
                entity
                    .as_str()
                    .len()
                    .checked_add(cut.root().id.as_str().len()),
            )?;
            entity == &cut.root().id
        } else {
            false
        };
        if is_root {
            if root_seen {
                return Err(ExtractionError::InvalidSource);
            }
            root_seen = true;
            rewrite_retained(&mut candidate, relocation, work)?;
            work.visit(2, 0, 0)?;
            originals.push(PlacementDestination::Retained(retained.len()));
            retained.push(candidate);

            // P keeps its external parent and sibling intent on the retained entity anchor.
            // Q receives a fresh root declaration, never a copy of that external placement.
            work.admit(
                size_of::<SceneEntityPlacement>().checked_add(cut.root().id.as_str().len()),
            )?;
            generated.push(moved.len());
            moved.push(SceneEntityPlacement::root(cut.root().id.clone()));
            local_count = local_count
                .checked_add(1)
                .ok_or(ExtractionError::Capacity)?;
            continue;
        }

        let target = relocation.moved_target(&original.target, AddressRole::Entity, |bytes| {
            work.admit(bytes)
        })?;
        let Some(target) = target else {
            rewrite_retained(&mut candidate, relocation, work)?;
            work.visit(2, 0, 0)?;
            originals.push(PlacementDestination::Retained(retained.len()));
            retained.push(candidate);
            continue;
        };
        let parent = match &original.placement.parent {
            // Root belongs to P's containing hierarchy. An inherited target moved there must
            // remain a P pin rather than becoming another materialized root in Q.
            ScenePlacementParent::Root => None,
            ScenePlacementParent::Entity(parent) => {
                relocation.moved_target(parent, AddressRole::Entity, |bytes| work.admit(bytes))?
            }
        };
        let (internal_slot, sibling) = match &original.placement.slot {
            ScenePlacementSlot::First | ScenePlacementSlot::Last => (true, None),
            ScenePlacementSlot::Before(sibling) | ScenePlacementSlot::After(sibling) => {
                let sibling = relocation
                    .moved_target(sibling, AddressRole::Entity, |bytes| work.admit(bytes))?;
                (sibling.is_some(), sibling)
            }
        };
        let internal = !groups.retained(declaration_index);
        if !internal {
            if direct.is_none() {
                rewrite_retained(&mut candidate, relocation, work)?;
                work.visit(2, 0, 0)?;
                originals.push(PlacementDestination::Retained(retained.len()));
                retained.push(candidate);
                continue;
            }

            // Preserve the full authored constraint independently of Q's required local
            // baseline. No flattened child order can establish the meaning of Before/After.
            work.placement_copy(original)?;
            let mut pin = original.clone();
            rewrite_retained(&mut pin, relocation, work)?;
            work.visit(2, 0, 0)?;
            originals.push(PlacementDestination::Retained(retained.len()));
            retained.push(pin);
        }

        if let Cow::Owned(target) = target {
            candidate.target = target;
        }
        match if internal { parent } else { None } {
            Some(Cow::Owned(parent)) => {
                candidate.placement.parent = ScenePlacementParent::Entity(parent);
            }
            Some(Cow::Borrowed(_)) => {}
            None => {
                work.admit(
                    size_of::<PrefabEntityTarget>().checked_add(cut.root().id.as_str().len()),
                )?;
                candidate.placement.parent =
                    ScenePlacementParent::Entity(PrefabEntityTarget::source(cut.root().id.clone()));
            }
        }
        if internal && internal_slot {
            if let Some(Cow::Owned(sibling)) = sibling {
                match &mut candidate.placement.slot {
                    ScenePlacementSlot::Before(target) | ScenePlacementSlot::After(target) => {
                        *target = sibling;
                    }
                    ScenePlacementSlot::First | ScenePlacementSlot::Last => {
                        return Err(ExtractionError::InvalidSource);
                    }
                }
            }
        } else {
            candidate.placement.slot = ScenePlacementSlot::Last;
        }
        work.visit(if internal { 2 } else { 1 }, 0, 0)?;
        if internal {
            originals.push(PlacementDestination::Extracted(moved.len()));
        } else {
            generated.push(moved.len());
        }
        moved.push(candidate);
        if direct.is_some() {
            local_count = local_count
                .checked_add(1)
                .ok_or(ExtractionError::Capacity)?;
        }
    }

    if !root_seen || local_count != cut.entities().len() {
        return Err(ExtractionError::InvalidSource);
    }
    Ok(PlacementSplit {
        retained,
        extracted: moved,
        originals,
        generated,
    })
}

/// Constraints sharing a canonical entity or a parent/sibling domain must compose in one layer.
/// Facts come from this cut's original compiler snapshot, including root aliases. We group source
/// declarations; no flattened child sequence is copied back into authored intent.
struct PlacementGroups {
    parent: Vec<usize>,
    rank: Vec<u8>,
    retained: Vec<bool>,
    targets: Vec<usize>,
}

impl PlacementGroups {
    fn representative(&mut self, mut entity: usize) -> usize {
        while self.parent[entity] != entity {
            self.parent[entity] = self.parent[self.parent[entity]];
            entity = self.parent[entity];
        }
        entity
    }

    fn join(&mut self, left: usize, right: usize) {
        let left = self.representative(left);
        let right = self.representative(right);
        if left != right {
            let (left, right) = if self.rank[left] >= self.rank[right] {
                (left, right)
            } else {
                (right, left)
            };
            self.parent[right] = left;
            self.retained[left] |= self.retained[right];
            if self.rank[left] == self.rank[right] {
                self.rank[left] += 1;
            }
        }
    }

    fn retained(&mut self, index: usize) -> bool {
        let root = self.representative(self.targets[index]);
        self.retained[root]
    }
}

fn placement_groups(
    cut: &SourceCut<'_>,
    declarations: &[SceneEntityPlacement],
    relocation: &BranchRelocation<'_, '_>,
    work: &mut ExtractionWork,
) -> Result<PlacementGroups, ExtractionError> {
    let count = cut.target_fact_count();
    work.admit(
        count
            .checked_mul(size_of::<usize>() + size_of::<bool>() + size_of::<u8>())
            .and_then(|bytes| {
                bytes.checked_add(declarations.len().checked_mul(size_of::<usize>())?)
            }),
    )?;
    let mut groups = PlacementGroups {
        parent: (0..count).collect(),
        rank: vec![0; count],
        retained: vec![false; count],
        targets: Vec::with_capacity(declarations.len()),
    };
    let lookup = |target: &PrefabEntityTarget, work: &mut ExtractionWork| {
        // Account borrowed key comparisons at this immutable snapshot before BTree lookup.
        work.target_lookup(target, cut.membership.targets.len())?;
        cut.target_fact(target)
            .ok_or(ExtractionError::InvalidSource)
    };
    for declaration in declarations {
        // Union by rank bounds each representative traversal by log2 of this snapshot.
        work.visit(
            ((usize::BITS - count.leading_zeros()) as usize + 1).saturating_mul(8),
            0,
            0,
        )?;
        let target = lookup(&declaration.target, work)?;
        groups.targets.push(target);
        let is_root = declaration.target.through.is_empty()
            && matches!(&declaration.target.entity, PrefabEntityOrigin::SourceEntity(entity) if entity == &cut.root().id);
        if is_root {
            // P keeps the selected anchor's exterior constraint independently of Q's root.
            continue;
        }
        let target_internal = relocation
            .moved_target(&declaration.target, AddressRole::Entity, |bytes| {
                work.admit(bytes)
            })?
            .is_some();
        let parent_internal = match &declaration.placement.parent {
            ScenePlacementParent::Root => false,
            ScenePlacementParent::Entity(parent) => {
                groups.join(target, lookup(parent, work)?);
                relocation
                    .moved_target(parent, AddressRole::Entity, |bytes| work.admit(bytes))?
                    .is_some()
            }
        };
        let sibling_internal = match &declaration.placement.slot {
            ScenePlacementSlot::First | ScenePlacementSlot::Last => true,
            ScenePlacementSlot::Before(sibling) | ScenePlacementSlot::After(sibling) => {
                groups.join(target, lookup(sibling, work)?);
                relocation
                    .moved_target(sibling, AddressRole::Entity, |bytes| work.admit(bytes))?
                    .is_some()
            }
        };
        if !(target_internal && parent_internal && sibling_internal) {
            let root = groups.representative(target);
            groups.retained[root] = true;
        }
    }
    Ok(groups)
}

/// The caller admits the complete raw clone before any route can allocate a replacement.
fn rewrite_retained(
    declaration: &mut SceneEntityPlacement,
    relocation: &BranchRelocation<'_, '_>,
    work: &mut ExtractionWork,
) -> Result<(), ExtractionError> {
    if let Cow::Owned(target) = relocation.retained_target(
        relocation.original(),
        &declaration.target,
        AddressRole::Entity,
        |bytes| work.admit(bytes),
    )? {
        declaration.target = target;
    }
    let ScenePlacement { parent, slot } = &mut declaration.placement;
    if let ScenePlacementParent::Entity(parent) = parent
        && let Cow::Owned(target) = relocation.retained_target(
            relocation.original(),
            parent,
            AddressRole::Entity,
            |bytes| work.admit(bytes),
        )?
    {
        *parent = target;
    }
    if let ScenePlacementSlot::Before(sibling) | ScenePlacementSlot::After(sibling) = slot
        && let Cow::Owned(target) = relocation.retained_target(
            relocation.original(),
            sibling,
            AddressRole::Entity,
            |bytes| work.admit(bytes),
        )?
    {
        *sibling = target;
    }
    Ok(())
}
