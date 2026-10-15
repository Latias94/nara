use std::collections::BTreeMap;

use nara_ecs::{Entity, LifecycleFreeDespawnError, World};
use nara_hierarchy::{
    __private::{retirement_operations, validate_retirement_despawn},
    Children, HierarchyOperation, HierarchyPlacement, HierarchyPlacementOwner, HierarchyRoots,
    Parent,
};

use crate::{SceneEntityId, validation::PreparedScene};

#[derive(Debug)]
pub(crate) struct SceneHierarchyRetirementError;

pub(crate) struct PreparedSceneHierarchyRetirement {
    operations: Vec<HierarchyOperation>,
    affected_entities: Vec<Entity>,
}

impl PreparedSceneHierarchyRetirement {
    pub(crate) fn operations(&self) -> &[HierarchyOperation] {
        &self.operations
    }

    pub(crate) fn affected_entities(&self) -> &[Entity] {
        &self.affected_entities
    }
}

pub(crate) struct PreparedSceneEntityRetirement {
    entities: Vec<Entity>,
}

impl PreparedSceneEntityRetirement {
    pub(crate) fn prepare(
        world: &mut World,
        entities: &[Entity],
        hierarchy: &PreparedSceneHierarchyRetirement,
    ) -> Result<Self, LifecycleFreeDespawnError> {
        validate_retirement_despawn(world, entities, hierarchy.affected_entities())?;
        Ok(Self {
            entities: entities.to_vec(),
        })
    }

    pub(crate) fn commit(self, world: &mut World) {
        for entity in self.entities {
            // The enclosing exclusive Scene kernel performs no lifecycle registration or entity
            // mutation between preparation and this exact retirement.
            world.entity_mut(entity).despawn();
        }
    }
}

pub(crate) fn prepare_scene_hierarchy_retirement(
    world: &World,
    retirements: &[Entity],
) -> Result<PreparedSceneHierarchyRetirement, SceneHierarchyRetirementError> {
    let roots = world.get_resource::<HierarchyRoots>();
    let structural_retirements = retirements
        .iter()
        .copied()
        .filter(|entity| {
            world.get::<Parent>(*entity).is_some()
                || world.get::<Children>(*entity).is_some()
                || roots.is_some_and(|roots| roots.contains(*entity))
        })
        .collect::<Vec<_>>();
    let operations = retirement_operations(world, &structural_retirements)
        .map_err(|_| SceneHierarchyRetirementError)?;
    let mut affected_entities = Vec::new();
    for entity in structural_retirements {
        affected_entities.push(entity);
        if let Some(parent) = world.get::<Parent>(entity).map(Parent::parent) {
            affected_entities.push(parent);
        }
        if let Some(children) = world.get::<Children>(entity) {
            for child in children.iter() {
                affected_entities.push(child);
            }
        }
    }
    affected_entities.sort_unstable_by_key(|entity| entity.to_bits());
    affected_entities.dedup();

    Ok(PreparedSceneHierarchyRetirement {
        operations,
        affected_entities,
    })
}

pub(crate) fn build_scene_hierarchy_operations(
    preflight: &PreparedScene,
    spawned_by_id: &BTreeMap<SceneEntityId, Entity>,
) -> Result<Vec<HierarchyOperation>, SceneEntityId> {
    let by_id = preflight
        .entities
        .iter()
        .map(|entity| (&entity.id, entity))
        .collect::<BTreeMap<_, _>>();
    let mut operations = Vec::with_capacity(preflight.entities.len());
    let mut pending = preflight
        .roots
        .iter()
        .rev()
        .map(|root| (root.clone(), HierarchyPlacementOwner::Root))
        .collect::<Vec<_>>();

    while let Some((id, owner)) = pending.pop() {
        let Some(runtime_entity) = spawned_by_id.get(&id).copied() else {
            return Err(id);
        };
        operations.push(HierarchyOperation::Place {
            entity: runtime_entity,
            placement: HierarchyPlacement::last(owner),
        });
        let Some(record) = by_id.get(&id) else {
            return Err(id);
        };
        let child_owner = HierarchyPlacementOwner::Parent {
            parent: runtime_entity,
        };
        pending.extend(
            record
                .children
                .iter()
                .rev()
                .cloned()
                .map(|child| (child, child_owner)),
        );
    }

    Ok(operations)
}

pub(crate) fn combine_scene_hierarchy_operations(
    world: &World,
    retirement: &PreparedSceneHierarchyRetirement,
    new_operations: &[HierarchyOperation],
    retiring_entities: &[Entity],
) -> Vec<HierarchyOperation> {
    let replacement_root_placement = world
        .get_resource::<HierarchyRoots>()
        .and_then(|roots| roots.iter().find(|root| retiring_entities.contains(root)))
        .and_then(|root| HierarchyPlacement::current(world, root));

    let mut operations = Vec::with_capacity(retirement.operations().len() + new_operations.len());
    operations.extend(retirement.operations().iter().copied());
    let mut previous_new_root = None;
    let mut first_new_root_placement = replacement_root_placement;
    for operation in new_operations.iter().copied() {
        let operation = match operation {
            HierarchyOperation::Place {
                entity,
                placement:
                    HierarchyPlacement {
                        owner: HierarchyPlacementOwner::Root,
                        ..
                    },
            } => {
                let placement = if let Some(previous) = previous_new_root {
                    HierarchyPlacement::after(HierarchyPlacementOwner::Root, previous)
                } else {
                    first_new_root_placement
                        .take()
                        .unwrap_or_else(|| HierarchyPlacement::last(HierarchyPlacementOwner::Root))
                };
                previous_new_root = Some(entity);
                HierarchyOperation::Place { entity, placement }
            }
            operation => operation,
        };
        operations.push(operation);
    }
    operations
}
