use nara_app::{App, CoreStage, FixedUpdateSet, PluginError, StartupStage};
use nara_core::Mat3;
use nara_ecs::{
    Entity, FromWorld, Mut, QueryState, Ref, Resource, With, Without, World,
    change_detection::{DetectChanges, Tick},
    entity::{EntityHashMap, EntityHashSet},
    error::BevyError,
    schedule::IntoScheduleConfigs,
};
use nara_hierarchy::Parent;

use crate::{
    __private::{CompletedTransformProjection, TransformSet},
    GlobalTransform2d, Transform2d,
};

/// Failure to publish one complete runtime 2D transform projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum TransformCompletionError {
    #[error("the runtime hierarchy has not completed validation")]
    HierarchyIncomplete,
    #[error("entity {entity:?} has a non-finite local 2D transform")]
    NonFiniteLocal { entity: Entity },
    #[error(
        "entity {child:?} has a local 2D transform but its structural parent {parent:?} does not"
    )]
    MissingParentTransform { child: Entity, parent: Entity },
    #[error("entity {entity:?} produced a non-finite global 2D transform")]
    NonFiniteGlobal { entity: Entity },
    #[error(
        "the transform forest traversal visited {visited} of {expected} participating entities"
    )]
    IncompleteTraversal { expected: usize, visited: usize },
}

#[derive(Debug, Resource)]
struct TransformCompletionState {
    last_observed_tick: Tick,
    observed_participants: usize,
    observed_hierarchy_generation: Option<u64>,
    spatial_generation: u64,
    completed_spatial_generation: Option<u64>,
    completed_hierarchy_generation: Option<u64>,
    #[cfg(test)]
    change_detection_scans: u64,
    #[cfg(test)]
    change_detection_visits: u64,
    #[cfg(test)]
    propagation_scans: u64,
    #[cfg(test)]
    participant_visits: u64,
    #[cfg(test)]
    transform_edge_visits: u64,
}

impl Default for TransformCompletionState {
    fn default() -> Self {
        Self {
            last_observed_tick: Tick::new(0),
            observed_participants: 0,
            observed_hierarchy_generation: None,
            spatial_generation: 0,
            completed_spatial_generation: None,
            completed_hierarchy_generation: None,
            #[cfg(test)]
            change_detection_scans: 0,
            #[cfg(test)]
            change_detection_visits: 0,
            #[cfg(test)]
            propagation_scans: 0,
            #[cfg(test)]
            participant_visits: 0,
            #[cfg(test)]
            transform_edge_visits: 0,
        }
    }
}

#[derive(Resource)]
struct RetainedTransformPropagationScratch {
    local_transforms: QueryState<(Entity, Ref<'static, Transform2d>)>,
    stale_globals: QueryState<Entity, (With<GlobalTransform2d>, Without<Transform2d>)>,
    local_matrices: EntityHashMap<Mat3>,
    first_transform_child: EntityHashMap<usize>,
    transform_children: Vec<Entity>,
    next_transform_sibling: Vec<Option<usize>>,
    roots: Vec<Entity>,
    stack: Vec<(Entity, Mat3)>,
    visited: EntityHashSet,
    candidate_globals: Vec<(Entity, Mat3)>,
    stale_entities: Vec<Entity>,
}

impl FromWorld for RetainedTransformPropagationScratch {
    fn from_world(world: &mut World) -> Self {
        Self {
            local_transforms: world.query::<(Entity, Ref<Transform2d>)>(),
            stale_globals: world
                .query_filtered::<Entity, (With<GlobalTransform2d>, Without<Transform2d>)>(),
            local_matrices: EntityHashMap::default(),
            first_transform_child: EntityHashMap::default(),
            transform_children: Vec::new(),
            next_transform_sibling: Vec::new(),
            roots: Vec::new(),
            stack: Vec::new(),
            visited: EntityHashSet::default(),
            candidate_globals: Vec::new(),
            stale_entities: Vec::new(),
        }
    }
}

pub(crate) fn install(app: &mut App) -> Result<(), PluginError> {
    app.init_resource::<TransformCompletionState>()?;
    app.init_resource::<RetainedTransformPropagationScratch>()?;
    app.configure_sets(
        StartupStage::Tooling,
        TransformSet::Propagate.after(nara_hierarchy::__private::HierarchySet::ValidateAndComplete),
    )?;
    app.add_systems(
        StartupStage::Tooling,
        complete_transform_projection_system.in_set(TransformSet::Propagate),
    )?;
    app.configure_sets(
        CoreStage::FixedUpdate,
        TransformSet::Propagate
            .after(nara_hierarchy::__private::HierarchySet::ValidateAndComplete)
            .before(FixedUpdateSet::Finalize),
    )?;
    app.add_systems(
        CoreStage::FixedUpdate,
        complete_transform_projection_system.in_set(TransformSet::Propagate),
    )?;
    app.configure_sets(
        CoreStage::PostUpdate,
        TransformSet::Propagate.after(nara_hierarchy::__private::HierarchySet::ValidateAndComplete),
    )?;
    app.add_systems(
        CoreStage::PostUpdate,
        complete_transform_projection_system.in_set(TransformSet::Propagate),
    )?;
    app.configure_sets(
        CoreStage::Extract,
        TransformSet::Propagate.after(nara_hierarchy::__private::HierarchySet::ValidateAndComplete),
    )?;
    app.add_systems(
        CoreStage::Extract,
        complete_transform_projection_system.in_set(TransformSet::Propagate),
    )?;
    Ok(())
}

fn complete_transform_projection_system(world: &mut World) -> Result<(), BevyError> {
    complete_transform_projection(world).map_err(BevyError::error)
}

pub(crate) fn complete_transform_projection(
    world: &mut World,
) -> Result<(), TransformCompletionError> {
    let Some(hierarchy_generation) =
        nara_hierarchy::__private::completed_topology_generation(world)
    else {
        let state = &mut *world.resource_mut::<TransformCompletionState>();
        state.completed_spatial_generation = None;
        state.completed_hierarchy_generation = None;
        world.remove_resource::<CompletedTransformProjection>();
        return Err(TransformCompletionError::HierarchyIncomplete);
    };
    let current_tick = world.change_tick();
    let state_snapshot = {
        let state = world.resource::<TransformCompletionState>();
        (
            state.last_observed_tick,
            state.observed_participants,
            state.observed_hierarchy_generation,
            state.completed_spatial_generation,
        )
    };
    let projection_token_missing = !world.contains_resource::<CompletedTransformProjection>();

    world.resource_scope(
        |world, mut scratch: Mut<RetainedTransformPropagationScratch>| {
            let mut participant_count = 0usize;
            let mut local_changed = false;
            for (_, transform) in scratch.local_transforms.iter(world) {
                participant_count = participant_count.saturating_add(1);
                local_changed |= transform
                    .last_changed()
                    .is_newer_than(state_snapshot.0, current_tick);
            }
            #[cfg(test)]
            {
                let state = &mut *world.resource_mut::<TransformCompletionState>();
                state.change_detection_scans = state.change_detection_scans.saturating_add(1);
                state.change_detection_visits = state
                    .change_detection_visits
                    .saturating_add(u64::try_from(participant_count).unwrap_or(u64::MAX));
            }

            let source_changed = local_changed
                || participant_count != state_snapshot.1
                || state_snapshot.2 != Some(hierarchy_generation);
            let needs_completion =
                state_snapshot.3.is_none() || projection_token_missing || source_changed;
            if !needs_completion {
                world
                    .resource_mut::<TransformCompletionState>()
                    .last_observed_tick = current_tick;
                return Ok(());
            }

            let spatial_generation = {
                let state = &mut *world.resource_mut::<TransformCompletionState>();
                if source_changed {
                    state.spatial_generation = state.spatial_generation.saturating_add(1);
                }
                state.last_observed_tick = current_tick;
                state.observed_participants = participant_count;
                state.observed_hierarchy_generation = Some(hierarchy_generation);
                state.completed_spatial_generation = None;
                state.completed_hierarchy_generation = None;
                state.spatial_generation
            };
            world.remove_resource::<CompletedTransformProjection>();

            build_candidate_projection(world, &mut scratch, participant_count)?;
            publish_candidate_projection(world, &mut scratch);

            #[cfg(test)]
            let visited = scratch.candidate_globals.len();
            #[cfg(test)]
            let transform_edges = scratch.transform_children.len();
            let state = &mut *world.resource_mut::<TransformCompletionState>();
            state.completed_spatial_generation = Some(spatial_generation);
            state.completed_hierarchy_generation = Some(hierarchy_generation);
            #[cfg(test)]
            {
                state.propagation_scans = state.propagation_scans.saturating_add(1);
                state.participant_visits = state
                    .participant_visits
                    .saturating_add(u64::try_from(visited).unwrap_or(u64::MAX));
                state.transform_edge_visits = state
                    .transform_edge_visits
                    .saturating_add(u64::try_from(transform_edges).unwrap_or(u64::MAX));
            }
            world.insert_resource(CompletedTransformProjection::new(
                spatial_generation,
                hierarchy_generation,
            ));
            Ok(())
        },
    )
}

#[cfg(test)]
pub(crate) fn propagation_stats(world: &World) -> (u64, u64, u64, u64, u64) {
    let state = world.resource::<TransformCompletionState>();
    (
        state.change_detection_scans,
        state.change_detection_visits,
        state.propagation_scans,
        state.participant_visits,
        state.transform_edge_visits,
    )
}

fn build_candidate_projection(
    world: &World,
    scratch: &mut RetainedTransformPropagationScratch,
    participant_count: usize,
) -> Result<(), TransformCompletionError> {
    scratch.local_matrices.clear();
    scratch.first_transform_child.clear();
    scratch.transform_children.clear();
    scratch.next_transform_sibling.clear();
    scratch.roots.clear();
    scratch.stack.clear();
    scratch.visited.clear();
    scratch.candidate_globals.clear();
    scratch.stale_entities.clear();

    scratch.local_matrices.reserve(participant_count);
    scratch.first_transform_child.reserve(participant_count);
    scratch.transform_children.reserve(participant_count);
    scratch.next_transform_sibling.reserve(participant_count);
    scratch.roots.reserve(participant_count);
    scratch.visited.reserve(participant_count);
    scratch.candidate_globals.reserve(participant_count);

    for (entity, transform) in scratch.local_transforms.iter(world) {
        let matrix = transform.matrix();
        if !matrix.is_finite() {
            return Err(TransformCompletionError::NonFiniteLocal { entity });
        }
        scratch.local_matrices.insert(entity, matrix);
    }

    for &entity in scratch.local_matrices.keys() {
        if let Some(parent) = world.get::<Parent>(entity) {
            let parent = parent.parent();
            if !scratch.local_matrices.contains_key(&parent) {
                return Err(TransformCompletionError::MissingParentTransform {
                    child: entity,
                    parent,
                });
            }
            let edge_index = scratch.transform_children.len();
            let previous_head = scratch.first_transform_child.insert(parent, edge_index);
            scratch.transform_children.push(entity);
            scratch.next_transform_sibling.push(previous_head);
        } else {
            scratch.roots.push(entity);
        }
    }

    for &root in scratch.roots.iter().rev() {
        scratch.stack.push((root, Mat3::IDENTITY));
    }
    while let Some((entity, parent_global)) = scratch.stack.pop() {
        if !scratch.visited.insert(entity) {
            return Err(TransformCompletionError::IncompleteTraversal {
                expected: participant_count,
                visited: scratch.visited.len(),
            });
        }
        let local = scratch.local_matrices[&entity];
        let global = parent_global * local;
        if !global.is_finite() {
            return Err(TransformCompletionError::NonFiniteGlobal { entity });
        }
        scratch.candidate_globals.push((entity, global));

        let mut edge_index = scratch.first_transform_child.get(&entity).copied();
        while let Some(index) = edge_index {
            scratch
                .stack
                .push((scratch.transform_children[index], global));
            edge_index = scratch.next_transform_sibling[index];
        }
    }

    if scratch.visited.len() != participant_count {
        return Err(TransformCompletionError::IncompleteTraversal {
            expected: participant_count,
            visited: scratch.visited.len(),
        });
    }

    scratch
        .stale_entities
        .extend(scratch.stale_globals.iter(world));
    Ok(())
}

fn publish_candidate_projection(
    world: &mut World,
    scratch: &mut RetainedTransformPropagationScratch,
) {
    for entity in scratch.stale_entities.drain(..) {
        world.entity_mut(entity).remove::<GlobalTransform2d>();
    }

    for (entity, matrix) in scratch.candidate_globals.iter().copied() {
        if world
            .get::<GlobalTransform2d>(entity)
            .is_none_or(|current| current.matrix() != matrix)
        {
            world.entity_mut(entity).insert(GlobalTransform2d(matrix));
        }
    }
}
