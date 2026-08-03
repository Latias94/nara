use std::collections::HashSet;

use nara_ecs::{Entity, QueryState, World, entity::EntityHashMap};

use crate::{Children, HierarchyError, Parent};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Complete,
}

#[derive(Debug, Default)]
pub(crate) struct HierarchyValidationScratch {
    parent_query: Option<QueryState<(Entity, &'static Parent)>>,
    children_query: Option<QueryState<(Entity, &'static Children)>>,
    parent_edges: Vec<(Entity, Entity)>,
    parent_by_child: EntityHashMap<Entity>,
    reverse_edges: HashSet<(Entity, Entity)>,
    visit_states: EntityHashMap<VisitState>,
    path: Vec<Entity>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HierarchyPreflightStats {
    #[cfg(test)]
    pub(crate) parent_entities_scanned: usize,
    #[cfg(test)]
    pub(crate) children_entities_scanned: usize,
    #[cfg(test)]
    pub(crate) addition_edges_scanned: usize,
    #[cfg(test)]
    pub(crate) cycle_starts_scanned: usize,
    #[cfg(test)]
    pub(crate) cycle_cursor_visits: usize,
}

impl HierarchyPreflightStats {
    fn record_parent_entity(&mut self) {
        #[cfg(test)]
        {
            self.parent_entities_scanned += 1;
        }
    }

    fn record_children_entity(&mut self) {
        #[cfg(test)]
        {
            self.children_entities_scanned += 1;
        }
    }

    fn record_addition_edge(&mut self) {
        #[cfg(test)]
        {
            self.addition_edges_scanned += 1;
        }
    }

    fn record_cycle_start(&mut self) {
        #[cfg(test)]
        {
            self.cycle_starts_scanned += 1;
        }
    }

    fn record_cycle_cursor_visit(&mut self) {
        #[cfg(test)]
        {
            self.cycle_cursor_visits += 1;
        }
    }
}

impl HierarchyValidationScratch {
    fn clear(&mut self) {
        self.parent_edges.clear();
        self.parent_by_child.clear();
        self.reverse_edges.clear();
        self.visit_states.clear();
        self.path.clear();
    }

    #[cfg(test)]
    pub(crate) fn capacities(&self) -> [usize; 5] {
        [
            self.parent_edges.capacity(),
            self.parent_by_child.capacity(),
            self.reverse_edges.capacity(),
            self.visit_states.capacity(),
            self.path.capacity(),
        ]
    }
}

/// Validates the complete runtime structural hierarchy without recursion.
///
/// Runtime is linear in the number of forward and reverse hierarchy entries. This function does
/// not consult the generation fast path and is reusable by publication preflight.
pub fn validate_hierarchy(world: &mut World) -> Result<(), HierarchyError> {
    let mut scratch = HierarchyValidationScratch::default();
    validate_hierarchy_with_scratch(world, &mut scratch)
}

pub(crate) fn validate_hierarchy_with_scratch(
    world: &mut World,
    scratch: &mut HierarchyValidationScratch,
) -> Result<(), HierarchyError> {
    validate_hierarchy_with_additions(world, core::iter::empty(), scratch).map(|_| ())
}

pub(crate) fn validate_hierarchy_with_additions(
    world: &mut World,
    additions: impl IntoIterator<Item = (Entity, Entity)>,
    scratch: &mut HierarchyValidationScratch,
) -> Result<HierarchyPreflightStats, HierarchyError> {
    scratch.clear();
    let mut stats = HierarchyPreflightStats::default();

    {
        let parent_query = scratch
            .parent_query
            .get_or_insert_with(|| world.query::<(Entity, &Parent)>());
        for (entity, parent) in parent_query.iter(world) {
            stats.record_parent_entity();
            let parent = parent.parent();
            if entity == parent {
                return Err(HierarchyError::SelfParent { entity });
            }
            if world.get_entity(parent).is_err() {
                return Err(HierarchyError::MissingParent {
                    child: entity,
                    parent,
                });
            }
            scratch.parent_edges.push((entity, parent));
            scratch.parent_by_child.insert(entity, parent);
        }
    }

    {
        let children_query = scratch
            .children_query
            .get_or_insert_with(|| world.query::<(Entity, &Children)>());
        for (entity, children) in children_query.iter(world) {
            stats.record_children_entity();
            if children.is_empty() {
                return Err(HierarchyError::ReverseEmpty { parent: entity });
            }

            for child in children.iter() {
                if world.get_entity(child).is_err() {
                    return Err(HierarchyError::ReverseChildMissing {
                        parent: entity,
                        child,
                    });
                }
                if !scratch.reverse_edges.insert((entity, child)) {
                    return Err(HierarchyError::ReverseDuplicate {
                        parent: entity,
                        child,
                    });
                }
            }
        }
    }

    for &(parent, child) in &scratch.reverse_edges {
        let actual_parent = scratch.parent_by_child.get(&child).copied();
        if actual_parent != Some(parent) {
            return Err(HierarchyError::ReverseUnexpected {
                parent,
                child,
                actual_parent,
            });
        }
    }

    for &(child, parent) in &scratch.parent_edges {
        if !scratch.reverse_edges.contains(&(parent, child)) {
            return Err(HierarchyError::ReverseMissing { parent, child });
        }
    }

    let additions = additions.into_iter();
    let (addition_lower_bound, _) = additions.size_hint();
    scratch.parent_edges.reserve(addition_lower_bound);
    scratch.parent_by_child.reserve(addition_lower_bound);

    for (child, parent) in additions {
        stats.record_addition_edge();
        if world.get_entity(child).is_err() {
            return Err(HierarchyError::MissingChild { child });
        }
        if world.get_entity(parent).is_err() {
            return Err(HierarchyError::MissingParent { child, parent });
        }
        if child == parent {
            return Err(HierarchyError::SelfParent { entity: child });
        }
        if let Some(existing_parent) = scratch.parent_by_child.insert(child, parent) {
            return Err(HierarchyError::AlreadyParented {
                child,
                parent: existing_parent,
            });
        }
        scratch.parent_edges.push((child, parent));
    }

    validate_acyclic(scratch, &mut stats)?;
    Ok(stats)
}

fn validate_acyclic(
    scratch: &mut HierarchyValidationScratch,
    stats: &mut HierarchyPreflightStats,
) -> Result<(), HierarchyError> {
    let HierarchyValidationScratch {
        parent_edges,
        parent_by_child,
        visit_states,
        path,
        ..
    } = scratch;

    for &(start, _) in parent_edges.iter() {
        stats.record_cycle_start();
        if visit_states.get(&start) == Some(&VisitState::Complete) {
            continue;
        }

        path.clear();
        let mut cursor = start;
        loop {
            stats.record_cycle_cursor_visit();
            match visit_states.get(&cursor) {
                Some(VisitState::Visiting) => {
                    return Err(HierarchyError::Cycle { entity: cursor });
                }
                Some(VisitState::Complete) => break,
                None => {}
            }

            let Some(&parent) = parent_by_child.get(&cursor) else {
                break;
            };
            visit_states.insert(cursor, VisitState::Visiting);
            path.push(cursor);
            cursor = parent;
        }

        for entity in path.drain(..) {
            visit_states.insert(entity, VisitState::Complete);
        }
    }

    Ok(())
}
