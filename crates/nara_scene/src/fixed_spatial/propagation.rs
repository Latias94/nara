use crate::hierarchy::{ChildOf, HierarchyOrder};
use nara_app::{
    App, CoreStage, FixedTime, FixedUpdateSet, PluginError, RealTime, RuntimeGeneration,
    StartupStage,
};
use nara_ecs::{
    Entity, FromWorld, Mut, QueryState, Resource, World,
    bevy_ecs::{
        entity_disabling::DefaultQueryFilters,
        error::BevyError,
        query::{FilteredAccess, QueryBuilder},
        schedule::ApplyDeferred,
    },
    schedule::IntoScheduleConfigs,
};

use crate::fixed_spatial::{
    CapturedTransform2d, FixedSpatialSet, SpatialSnapshotPhase, SpatialSnapshotStamp, Transform2d,
    snapshot::{
        FixedInputSnapshot, PresentationSnapshot, SimulationSnapshot, SpatialPayload,
        SpatialSource, next_publication,
    },
};

pub(super) type LocalTransformQuery =
    QueryState<(Entity, &'static Transform2d, Option<&'static ChildOf>)>;

pub(super) fn stored_local_transforms(world: &mut World) -> LocalTransformQuery {
    let mut query = QueryBuilder::new(world);
    if let Some(filters) = query.world().get_resource::<DefaultQueryFilters>() {
        let mut allowed = FilteredAccess::default();
        for component in filters.disabling_ids() {
            allowed.access_mut().add_archetypal(component);
        }
        query.extend_access(allowed);
    }
    query.build()
}

/// Failure to publish one complete runtime 2D spatial cut.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum TransformCompletionError {
    #[error("the runtime hierarchy has not completed validation")]
    HierarchyIncomplete,
    #[error("entity {entity:?} has a non-finite local 2D transform")]
    NonFiniteLocal { entity: Entity },
    #[error("entity {entity:?} produced a non-finite global 2D transform")]
    NonFiniteGlobal { entity: Entity },
    #[error(
        "the transform forest traversal visited {visited} of {expected} participating entities"
    )]
    IncompleteTraversal { expected: usize, visited: usize },
    #[error("the spatial publication sequence is exhausted")]
    PublicationExhausted,
    #[error("the spatial publication clocks are unavailable")]
    MissingClock,
}

#[derive(Resource)]
struct TransformPropagation {
    locals: LocalTransformQuery,
    observed: Vec<(Entity, SpatialSource)>,
    observed_publication: Option<SpatialSnapshotStamp>,
    candidate: SpatialPayload,
    unordered_ancestors: Vec<Entity>,
    #[cfg(test)]
    stats: (u64, u64, u64, u64, u64),
}

impl FromWorld for TransformPropagation {
    fn from_world(world: &mut World) -> Self {
        Self {
            locals: stored_local_transforms(world),
            observed: Vec::new(),
            observed_publication: None,
            candidate: SpatialPayload::default(),
            unordered_ancestors: Vec::new(),
            #[cfg(test)]
            stats: (0, 0, 0, 0, 0),
        }
    }
}

#[derive(Resource)]
struct FixedInputPropagation(TransformPropagation);

impl FromWorld for FixedInputPropagation {
    fn from_world(world: &mut World) -> Self {
        Self(TransformPropagation::from_world(world))
    }
}

pub(crate) fn install_fixed_input(app: &mut App) -> Result<(), PluginError> {
    if app.world().contains_resource::<FixedInputPropagation>() {
        return Ok(());
    }
    app.init_resource::<FixedInputSnapshot>()?;
    app.init_resource::<FixedInputPropagation>()?;
    app.configure_sets(
        CoreStage::FixedUpdate,
        FixedSpatialSet::PrepareInput
            .after(FixedUpdateSet::Prepare)
            .before(FixedUpdateSet::Simulate),
    )?;
    app.add_systems(
        CoreStage::FixedUpdate,
        (ApplyDeferred, publish_fixed_input)
            .chain()
            .in_set(FixedSpatialSet::PrepareInput),
    )?;
    Ok(())
}

pub(crate) fn publish_fixed_input(world: &mut World) -> Result<(), BevyError> {
    let previous = world.resource_mut::<FixedInputSnapshot>().stamp.take();
    crate::hierarchy::complete_fixed_input_hierarchy(world)?;
    world.resource_mut::<FixedInputSnapshot>().stamp = previous;
    world
        .resource_scope(|world, mut propagation: Mut<FixedInputPropagation>| {
            world.resource_scope(|world, mut slot: Mut<FixedInputSnapshot>| {
                let FixedInputSnapshot {
                    stamp,
                    payload,
                    hierarchy_publication,
                } = &mut *slot;
                propagation.0.complete(
                    world,
                    SpatialSnapshotPhase::FixedInput,
                    stamp,
                    payload,
                    hierarchy_publication,
                )
            })
        })
        .map_err(BevyError::error)
}

pub(crate) fn install(app: &mut App) -> Result<(), PluginError> {
    app.init_resource::<SimulationSnapshot>()?;
    app.init_resource::<PresentationSnapshot>()?;
    app.init_resource::<TransformPropagation>()?;
    app.configure_sets(
        StartupStage::Tooling,
        FixedSpatialSet::Propagate.after(crate::hierarchy::HierarchySet::ValidateAndComplete),
    )?;
    app.add_systems(
        StartupStage::Tooling,
        (ApplyDeferred, complete_startup)
            .chain()
            .in_set(FixedSpatialSet::Propagate),
    )?;
    app.configure_sets(
        CoreStage::FixedUpdate,
        FixedSpatialSet::Propagate
            .after(crate::hierarchy::HierarchySet::ValidateAndComplete)
            .before(FixedUpdateSet::Finalize),
    )?;
    app.add_systems(
        CoreStage::FixedUpdate,
        (ApplyDeferred, complete_fixed)
            .chain()
            .in_set(FixedSpatialSet::Propagate),
    )?;
    for (stage, phase) in [
        (CoreStage::PostUpdate, SpatialSnapshotPhase::PostUpdate),
        (CoreStage::Extract, SpatialSnapshotPhase::Extract),
    ] {
        app.configure_sets(
            stage,
            FixedSpatialSet::Propagate.after(crate::hierarchy::HierarchySet::ValidateAndComplete),
        )?;
        app.add_systems(
            stage,
            (ApplyDeferred, move |world: &mut World| {
                complete_at(world, phase).map_err(BevyError::error)
            })
                .chain()
                .in_set(FixedSpatialSet::Propagate),
        )?;
    }
    Ok(())
}

fn complete_startup(world: &mut World) -> Result<(), BevyError> {
    complete_at(world, SpatialSnapshotPhase::Startup).map_err(BevyError::error)
}

fn complete_fixed(world: &mut World) -> Result<(), BevyError> {
    complete_at(world, SpatialSnapshotPhase::Fixed).map_err(BevyError::error)
}

#[cfg(test)]
pub(crate) fn complete_transform_projection(
    world: &mut World,
) -> Result<(), TransformCompletionError> {
    complete_at(world, SpatialSnapshotPhase::Extract)
}

pub(crate) fn complete_at(
    world: &mut World,
    phase: SpatialSnapshotPhase,
) -> Result<(), TransformCompletionError> {
    // Invalidate the publication before every attempt, even when hierarchy validation failed.
    world.resource_mut::<PresentationSnapshot>().stamp = None;
    world.resource_scope(|world, mut propagation: Mut<TransformPropagation>| {
        world.resource_scope(|world, mut slot: Mut<SimulationSnapshot>| {
            let SimulationSnapshot {
                stamp,
                payload,
                hierarchy_publication,
            } = &mut *slot;
            propagation.complete(world, phase, stamp, payload, hierarchy_publication)
        })
    })
}

impl TransformPropagation {
    fn complete(
        &mut self,
        world: &World,
        phase: SpatialSnapshotPhase,
        stamp: &mut Option<SpatialSnapshotStamp>,
        payload: &mut SpatialPayload,
        previous_hierarchy: &mut Option<crate::hierarchy::HierarchyPublication>,
    ) -> Result<(), TransformCompletionError> {
        let previous = stamp.take();
        let hierarchy_publication = crate::hierarchy::completed_topology_publication(world)
            .ok_or(TransformCompletionError::HierarchyIncomplete)?;
        let hierarchy_generation = world.resource::<HierarchyOrder>().generation();
        let world_id = world.id();
        let generation = world.get_resource::<RuntimeGeneration>().copied();
        let tick = world
            .get_resource::<FixedTime>()
            .ok_or(TransformCompletionError::MissingClock)?
            .tick();
        let frame = world
            .get_resource::<RealTime>()
            .ok_or(TransformCompletionError::MissingClock)?
            .frame;
        let mut count = 0usize;
        let mut changed = previous.is_none_or(|stamp| {
            stamp.world != world_id
                || stamp.runtime_generation != generation
                || stamp.hierarchy_generation != hierarchy_generation
        });
        changed |= *previous_hierarchy != Some(hierarchy_publication);
        changed |= self.observed_publication != previous;
        for (entity, local, parent) in self.locals.iter(world) {
            count += 1;
            if !local.translation.is_finite()
                || !local.rotation.is_finite()
                || !local.scale.is_finite()
            {
                return Err(TransformCompletionError::NonFiniteLocal { entity });
            }
            let source = SpatialSource {
                local: *local,
                parent: parent.map(ChildOf::parent),
            };
            let row = (entity, source);
            if let Some(previous) = self.observed.get_mut(count - 1) {
                changed |= *previous != row;
                *previous = row;
            } else {
                self.observed.push(row);
                changed = true;
            }
        }
        changed |= count != self.observed.len();
        self.observed.truncate(count);
        changed |= count != payload.sources.len();
        #[cfg(test)]
        {
            self.stats.0 += 1;
            self.stats.1 += count as u64;
        }
        let publication =
            next_publication().ok_or(TransformCompletionError::PublicationExhausted)?;
        if changed {
            let edges = self.build(world)?;
            std::mem::swap(payload, &mut self.candidate);
            #[cfg(test)]
            {
                self.stats.2 += 1;
                self.stats.3 += count as u64;
                self.stats.4 += edges as u64;
            }
            #[cfg(not(test))]
            let _ = edges;
        }
        *previous_hierarchy = Some(hierarchy_publication);
        *stamp = Some(SpatialSnapshotStamp {
            world: world_id,
            runtime_generation: generation,
            phase,
            tick,
            frame,
            publication,
            hierarchy_generation,
        });
        self.observed_publication = *stamp;
        Ok(())
    }

    fn build(&mut self, world: &World) -> Result<usize, TransformCompletionError> {
        let count = self.observed.len();
        self.candidate.sources.clear();
        self.candidate.globals.clear();
        self.candidate.order.clear();
        self.candidate.structural_indices.clear();
        self.candidate.spatial_order.clear();
        self.candidate.sources.reserve(count);
        self.candidate.globals.reserve(count);
        self.candidate.spatial_order.reserve(count);
        self.candidate.sources.extend(self.observed.iter().copied());
        self.candidate
            .order
            .extend_from_slice(world.resource::<HierarchyOrder>().as_slice());
        self.candidate
            .structural_indices
            .reserve(self.candidate.order.len());
        self.candidate.structural_indices.extend(
            self.candidate
                .order
                .iter()
                .copied()
                .enumerate()
                .map(|(index, entity)| (entity, index)),
        );
        // Hierarchy already validated a parent-before-child order for this exact publication.
        // Reuse it instead of retaining a second adjacency graph and traversal visited set.
        let mut edges = 0usize;
        for &entity in &self.candidate.order {
            let Some(source) = self.candidate.sources.get(&entity).copied() else {
                continue;
            };
            let parent = source.parent.and_then(|parent| {
                self.candidate
                    .sources
                    .contains_key(&parent)
                    .then_some(parent)
            });
            let parent = parent
                .map(|parent| {
                    self.candidate.globals.get(&parent).copied().ok_or(
                        TransformCompletionError::IncompleteTraversal {
                            expected: count,
                            visited: self.candidate.globals.len(),
                        },
                    )
                })
                .transpose()?;
            edges += usize::from(parent.is_some());
            let global = parent.map_or_else(
                || source.local.matrix(),
                |parent| parent.matrix() * source.local.matrix(),
            );
            if !global.is_finite() {
                return Err(TransformCompletionError::NonFiniteGlobal { entity });
            }
            self.candidate
                .globals
                .insert(entity, CapturedTransform2d(global));
            self.candidate.spatial_order.push(entity);
        }
        // Native trees outside the ordered roots still have immutable spatial facts. Follow
        // their captured spatial parents without assigning those entities scene order indices.
        for &(entity, _) in &self.observed {
            let mut cursor = entity;
            while !self.candidate.globals.contains_key(&cursor) {
                let Some(source) = self.candidate.sources.get(&cursor) else {
                    break;
                };
                self.unordered_ancestors.push(cursor);
                let Some(parent) = source.parent else {
                    break;
                };
                cursor = parent;
            }
            while let Some(entity) = self.unordered_ancestors.pop() {
                let source = self.candidate.sources[&entity];
                let parent = source
                    .parent
                    .and_then(|parent| self.candidate.globals.get(&parent));
                edges += usize::from(parent.is_some());
                let matrix = source.local.matrix();
                let global = parent.map_or(matrix, |parent| parent.matrix() * matrix);
                if !global.is_finite() {
                    self.unordered_ancestors.clear();
                    return Err(TransformCompletionError::NonFiniteGlobal { entity });
                }
                self.candidate
                    .globals
                    .insert(entity, CapturedTransform2d(global));
                self.candidate.spatial_order.push(entity);
            }
        }
        if self.candidate.globals.len() != count {
            return Err(TransformCompletionError::IncompleteTraversal {
                expected: count,
                visited: self.candidate.globals.len(),
            });
        }
        Ok(edges)
    }
}

#[cfg(test)]
pub(crate) fn propagation_stats(world: &World) -> (u64, u64, u64, u64, u64) {
    world.resource::<TransformPropagation>().stats
}

#[cfg(test)]
pub(crate) fn fixed_input_propagation_stats(world: &World) -> Option<(u64, u64, u64, u64, u64)> {
    world
        .get_resource::<FixedInputPropagation>()
        .map(|input| input.0.stats)
}
