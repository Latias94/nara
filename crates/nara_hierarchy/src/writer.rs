use nara_ecs::{Commands, Entity, World, entity::EntityHashSet};

use crate::{
    HierarchyError, Parent, mark_topology_dirty,
    validation::{
        HierarchyPreflightStats, HierarchyValidationScratch, validate_hierarchy_with_additions,
    },
};

/// One parent edge requested during construction of an unpublished runtime topology.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HierarchyConstructionEdge {
    child: Entity,
    parent: Entity,
}

impl HierarchyConstructionEdge {
    /// Creates one construction-only parent edge.
    #[must_use]
    pub const fn new(child: Entity, parent: Entity) -> Self {
        Self { child, parent }
    }

    /// Returns the child entity.
    #[must_use]
    pub const fn child(self) -> Entity {
        self.child
    }

    /// Returns the requested parent entity.
    #[must_use]
    pub const fn parent(self) -> Entity {
        self.parent
    }
}

/// A narrow writer for constructing runtime hierarchy edges before publication.
///
/// This is not a move, detach, or reparent API. Each insertion validates its complete ancestor
/// chain before mutating the world and marks the private topology generation dirty on success.
pub struct HierarchyConstructionWriter<'world> {
    world: &'world mut World,
}

/// Owned candidate hierarchy edges validated for a later infallible publication tail.
///
/// This proof is intentionally exposed only through the private hierarchy surface. Its caller
/// must retain exclusive control of the target World between preparation and commit.
#[must_use]
pub struct PreparedHierarchyConstructionBatch {
    edges: Vec<HierarchyConstructionEdge>,
}

pub(crate) fn prepare_hierarchy_construction_batch(
    world: &World,
    edges: &[HierarchyConstructionEdge],
) -> Result<PreparedHierarchyConstructionBatch, HierarchyError> {
    let mut scratch = HierarchyValidationScratch::default();
    validate_hierarchy_with_additions(
        world,
        edges.iter().map(|edge| (edge.child, edge.parent)),
        &mut scratch,
    )?;
    Ok(PreparedHierarchyConstructionBatch {
        edges: edges.to_vec(),
    })
}

impl PreparedHierarchyConstructionBatch {
    /// Publishes the previously validated edges without a recoverable failure branch.
    pub fn commit(self, world: &mut World) {
        let mut scratch = HierarchyValidationScratch::default();
        validate_hierarchy_with_additions(
            world,
            self.edges.iter().map(|edge| (edge.child, edge.parent)),
            &mut scratch,
        )
        .expect("prepared hierarchy construction facts must remain valid through commit");
        if self.edges.is_empty() {
            return;
        }
        world.insert_batch(
            self.edges
                .into_iter()
                .map(|edge| (edge.child, Parent::new(edge.parent))),
        );
        mark_topology_dirty(world);
    }
}

impl<'world> HierarchyConstructionWriter<'world> {
    /// Creates a construction writer over an exclusively borrowed world.
    #[must_use]
    pub fn new(world: &'world mut World) -> Self {
        Self { world }
    }

    /// Attaches an unparented child to an existing parent.
    pub fn attach(&mut self, child: Entity, parent: Entity) -> Result<(), HierarchyError> {
        validate_new_edge(self.world, child, parent)?;
        self.world.entity_mut(child).insert(Parent::new(parent));
        mark_topology_dirty(self.world);
        Ok(())
    }

    /// Atomically preflights and inserts a construction batch.
    ///
    /// One complete preflight covers the existing hierarchy and every proposed edge before the
    /// first mutation. Validation is iterative and linear in the existing topology plus the batch
    /// size. On success, Bevy's relationship-aware batch insertion queues all reverse projections
    /// and the topology generation is marked dirty exactly once. The caller must reach its normal
    /// command flush before the hierarchy completion set runs.
    pub fn attach_batch(
        &mut self,
        edges: &[HierarchyConstructionEdge],
    ) -> Result<(), HierarchyError> {
        self.attach_batch_impl(edges).map(|_| ())
    }

    fn attach_batch_impl(
        &mut self,
        edges: &[HierarchyConstructionEdge],
    ) -> Result<HierarchyPreflightStats, HierarchyError> {
        if edges.is_empty() {
            return Ok(HierarchyPreflightStats::default());
        }

        let mut scratch = HierarchyValidationScratch::default();
        let stats = validate_hierarchy_with_additions(
            self.world,
            edges.iter().map(|edge| (edge.child, edge.parent)),
            &mut scratch,
        )?;

        self.world.insert_batch(
            edges
                .iter()
                .map(|edge| (edge.child, Parent::new(edge.parent))),
        );
        mark_topology_dirty(self.world);
        Ok(stats)
    }

    #[cfg(test)]
    pub(crate) fn attach_batch_with_stats(
        &mut self,
        edges: &[HierarchyConstructionEdge],
    ) -> Result<HierarchyPreflightStats, HierarchyError> {
        self.attach_batch_impl(edges)
    }
}

/// Deferred hierarchy-construction commands.
pub trait HierarchyCommandsExt {
    /// Queues one validated construction-only parent edge.
    ///
    /// Validation happens when the command is applied, so entities spawned earlier in the same
    /// command queue are eligible. A rejected command follows the world's normal command error
    /// policy and performs no hierarchy mutation.
    fn attach_hierarchy_child(&mut self, child: Entity, parent: Entity) -> &mut Self;
}

impl HierarchyCommandsExt for Commands<'_, '_> {
    fn attach_hierarchy_child(&mut self, child: Entity, parent: Entity) -> &mut Self {
        self.queue(move |world: &mut World| {
            HierarchyConstructionWriter::new(world).attach(child, parent)
        });
        self
    }
}

fn validate_new_edge(world: &World, child: Entity, parent: Entity) -> Result<(), HierarchyError> {
    if world.get_entity(child).is_err() {
        return Err(HierarchyError::MissingChild { child });
    }
    if world.get_entity(parent).is_err() {
        return Err(HierarchyError::MissingParent { child, parent });
    }
    if child == parent {
        return Err(HierarchyError::SelfParent { entity: child });
    }
    if let Some(existing) = world.get::<Parent>(child) {
        return Err(HierarchyError::AlreadyParented {
            child,
            parent: existing.parent(),
        });
    }

    let mut visited = EntityHashSet::default();
    let mut cursor = parent;
    loop {
        if cursor == child {
            return Err(HierarchyError::Cycle { entity: child });
        }
        if !visited.insert(cursor) {
            return Err(HierarchyError::Cycle { entity: cursor });
        }

        let Some(next) = world.get::<Parent>(cursor).map(Parent::parent) else {
            break;
        };
        if world.get_entity(next).is_err() {
            return Err(HierarchyError::MissingParent {
                child: cursor,
                parent: next,
            });
        }
        cursor = next;
    }

    Ok(())
}
