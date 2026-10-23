use nara_app::{
    App, CoreStage, FixedTime, FixedUpdateSet, PluginError, RealTime, RuntimeGeneration,
    StartupStage,
};
use nara_ecs::{
    Entity, FromWorld, Mut, QueryState, Resource, World, schedule::IntoScheduleConfigs,
    upstream::error::BevyError, upstream::schedule::ApplyDeferred,
};
use nara_hierarchy::{HierarchyOrder, Parent};

use crate::{
    __private::TransformSet,
    GlobalTransform2d, SpatialSnapshotPhase, SpatialSnapshotStamp, Transform2d,
    snapshot::{
        PresentationSnapshot, SimulationSnapshot, SpatialPayload, SpatialSource, next_publication,
    },
};

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
}

#[derive(Resource)]
struct TransformPropagation {
    locals: QueryState<(Entity, &'static Transform2d, Option<&'static Parent>)>,
    observed: Vec<(Entity, SpatialSource)>,
    observed_publication: Option<SpatialSnapshotStamp>,
    candidate: SpatialPayload,
    #[cfg(test)]
    stats: (u64, u64, u64, u64, u64),
}

impl FromWorld for TransformPropagation {
    fn from_world(world: &mut World) -> Self {
        Self {
            locals: world.query(),
            observed: Vec::new(),
            observed_publication: None,
            candidate: SpatialPayload::default(),
            #[cfg(test)]
            stats: (0, 0, 0, 0, 0),
        }
    }
}

pub(crate) fn install(app: &mut App) -> Result<(), PluginError> {
    app.init_resource::<SimulationSnapshot>()?;
    app.init_resource::<PresentationSnapshot>()?;
    app.init_resource::<TransformPropagation>()?;
    app.configure_sets(
        StartupStage::Tooling,
        TransformSet::Propagate.after(nara_hierarchy::__private::HierarchySet::ValidateAndComplete),
    )?;
    app.add_systems(
        StartupStage::Tooling,
        (ApplyDeferred, complete_startup)
            .chain()
            .in_set(TransformSet::Propagate),
    )?;
    app.configure_sets(
        CoreStage::FixedUpdate,
        TransformSet::Propagate
            .after(nara_hierarchy::__private::HierarchySet::ValidateAndComplete)
            .before(FixedUpdateSet::Finalize),
    )?;
    app.add_systems(
        CoreStage::FixedUpdate,
        (ApplyDeferred, complete_fixed)
            .chain()
            .in_set(TransformSet::Propagate),
    )?;
    for (stage, phase) in [
        (CoreStage::PostUpdate, SpatialSnapshotPhase::PostUpdate),
        (CoreStage::Extract, SpatialSnapshotPhase::Extract),
    ] {
        app.configure_sets(
            stage,
            TransformSet::Propagate
                .after(nara_hierarchy::__private::HierarchySet::ValidateAndComplete),
        )?;
        app.add_systems(
            stage,
            (ApplyDeferred, move |world: &mut World| {
                complete_at(world, phase).map_err(BevyError::error)
            })
                .chain()
                .in_set(TransformSet::Propagate),
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
    let previous = world.resource_mut::<SimulationSnapshot>().stamp.take();
    let hierarchy_generation = nara_hierarchy::__private::completed_topology_generation(world)
        .ok_or(TransformCompletionError::HierarchyIncomplete)?;
    let hierarchy_publication = nara_hierarchy::__private::completed_topology_publication(world)
        .ok_or(TransformCompletionError::HierarchyIncomplete)?;
    let world_id = world.id();
    let generation = world.get_resource::<RuntimeGeneration>().copied();
    let tick = world.resource::<FixedTime>().tick();
    let frame = world.resource::<RealTime>().frame;
    world.resource_scope(|world, mut propagation: Mut<TransformPropagation>| {
        world.resource_scope(|world, mut slot: Mut<SimulationSnapshot>| {
            let mut count = 0usize;
            let mut changed = previous.is_none_or(|stamp| {
                stamp.world != world_id
                    || stamp.runtime_generation != generation
                    || stamp.hierarchy_generation != hierarchy_generation
            });
            changed |= slot.hierarchy_publication != Some(hierarchy_publication);
            changed |= propagation.observed_publication != previous;
            let TransformPropagation {
                locals, observed, ..
            } = &mut *propagation;
            for (entity, local, parent) in locals.iter(world) {
                count += 1;
                // Finite-value validation precedes the equality shortcut. Source equality does
                // not depend on Bevy change ticks, which direct callers may share.
                if !local.translation.is_finite()
                    || !local.rotation.is_finite()
                    || !local.scale.is_finite()
                {
                    return Err(TransformCompletionError::NonFiniteLocal { entity });
                }
                let source = SpatialSource {
                    local: *local,
                    parent: parent.map(Parent::parent),
                };
                let row = (entity, source);
                if let Some(previous) = observed.get_mut(count - 1) {
                    changed |= *previous != row;
                    *previous = row;
                } else {
                    observed.push(row);
                    changed = true;
                }
            }
            changed |= count != observed.len();
            observed.truncate(count);
            changed |= count != slot.payload.sources.len();
            #[cfg(test)]
            {
                propagation.stats.0 += 1;
                propagation.stats.1 += count as u64;
            }
            let publication =
                next_publication().ok_or(TransformCompletionError::PublicationExhausted)?;
            if changed {
                let edges = propagation.build(world)?;
                std::mem::swap(&mut slot.payload, &mut propagation.candidate);
                #[cfg(test)]
                {
                    propagation.stats.2 += 1;
                    propagation.stats.3 += count as u64;
                    propagation.stats.4 += edges as u64;
                }
                #[cfg(not(test))]
                let _ = edges;
            }
            slot.hierarchy_publication = Some(hierarchy_publication);
            slot.stamp = Some(SpatialSnapshotStamp {
                world: world_id,
                runtime_generation: generation,
                phase,
                tick,
                frame,
                publication,
                hierarchy_generation,
            });
            propagation.observed_publication = slot.stamp;
            Ok(())
        })
    })
}

impl TransformPropagation {
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
                .insert(entity, GlobalTransform2d(global));
            self.candidate.spatial_order.push(entity);
        }
        // Unplaced spatial entities are independent roots, but have no structural index.
        for &(entity, source) in &self.observed {
            if self.candidate.structural_indices.contains_key(&entity) {
                continue;
            }
            if source.parent.is_some() {
                return Err(TransformCompletionError::IncompleteTraversal {
                    expected: count,
                    visited: self.candidate.globals.len(),
                });
            }
            let global = source.local.matrix();
            if !global.is_finite() {
                return Err(TransformCompletionError::NonFiniteGlobal { entity });
            }
            self.candidate
                .globals
                .insert(entity, GlobalTransform2d(global));
            self.candidate.spatial_order.push(entity);
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
