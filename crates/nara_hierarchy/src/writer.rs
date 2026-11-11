use nara_ecs::{
    __private::LifecycleFreeDespawnError,
    __private::{
        PersistentLifecycleEvent, validate_entity_despawn_with_allowed_component_hooks,
        validate_entity_despawn_with_claimed_component_hooks,
        validate_registered_component_metadata, validate_registered_component_observers,
        validate_resource_insertion, validate_resource_scope,
    },
    Commands, Entity, World,
    bevy_ecs::entity::{EntityHashMap, EntityHashSet},
};

use crate::{
    Children, HierarchyError, HierarchyGenerationState, HierarchyRoots, Parent,
    hierarchy_baseline_is_valid, mark_hierarchy_baseline_valid, mark_topology_dirty,
    relation::HierarchyMember, validate_hierarchy,
};

/// The ordered collection that owns one runtime hierarchy entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HierarchyPlacementOwner {
    Root,
    Parent { parent: Entity },
}

/// A stable insertion point within one runtime placement owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HierarchyPlacementAnchor {
    First,
    Last,
    Before { sibling: Entity },
    After { sibling: Entity },
}

/// A root-or-parent owner plus an ordered insertion anchor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HierarchyPlacement {
    pub owner: HierarchyPlacementOwner,
    pub anchor: HierarchyPlacementAnchor,
}

impl HierarchyPlacement {
    #[must_use]
    pub const fn first(owner: HierarchyPlacementOwner) -> Self {
        Self {
            owner,
            anchor: HierarchyPlacementAnchor::First,
        }
    }

    #[must_use]
    pub const fn last(owner: HierarchyPlacementOwner) -> Self {
        Self {
            owner,
            anchor: HierarchyPlacementAnchor::Last,
        }
    }

    #[must_use]
    pub const fn before(owner: HierarchyPlacementOwner, sibling: Entity) -> Self {
        Self {
            owner,
            anchor: HierarchyPlacementAnchor::Before { sibling },
        }
    }

    #[must_use]
    pub const fn after(owner: HierarchyPlacementOwner, sibling: Entity) -> Self {
        Self {
            owner,
            anchor: HierarchyPlacementAnchor::After { sibling },
        }
    }

    /// Returns the exact current placement of a registered runtime hierarchy entity.
    #[must_use]
    pub fn current(world: &World, entity: Entity) -> Option<Self> {
        let owner = placement_owner(world, entity)?;
        let siblings = siblings_for_owner(world, owner)?;
        placement_in_siblings(owner, siblings, entity)
    }
}

/// One ordered runtime hierarchy mutation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HierarchyOperation {
    /// Registers an entity that is not currently part of the hierarchy closure.
    Place {
        entity: Entity,
        placement: HierarchyPlacement,
    },
    /// Moves an entity from an exact current placement to a new ordered placement.
    Move {
        entity: Entity,
        from: HierarchyPlacement,
        to: HierarchyPlacement,
    },
    /// Removes a childless entity from the hierarchy closure without despawning it.
    Remove {
        entity: Entity,
        from: HierarchyPlacement,
    },
}

/// The sole supported mutation boundary for Nara's ordered runtime hierarchy.
pub struct HierarchyWriter<'world> {
    world: &'world mut World,
}

/// An owned, prevalidated hierarchy transaction for an infallible publication tail.
///
/// This proof is exposed only through the private hierarchy integration surface. Its caller must
/// retain exclusive control of the target World between preparation and commit.
#[must_use]
pub struct PreparedHierarchyBatch {
    projection: Option<HierarchyProjection>,
}

pub(crate) fn prepare_mutation_baseline(world: &mut World) {
    world.register_component::<Parent>();
    world.register_component::<Children>();
    world.register_component::<HierarchyMember>();
    world.register_component::<HierarchyRoots>();
    world.register_component::<HierarchyGenerationState>();
    world.flush();
}

pub(crate) fn prepare_hierarchy_batch(
    world: &World,
    operations: &[HierarchyOperation],
) -> Result<PreparedHierarchyBatch, HierarchyError> {
    prepare_batch(world, operations, true)
}

fn prepare_writer_batch(
    world: &mut World,
    operations: &[HierarchyOperation],
) -> Result<PreparedHierarchyBatch, HierarchyError> {
    prepare_batch(world, operations, !hierarchy_baseline_is_valid(world))
}

pub(crate) fn hierarchy_retirement_operations(
    world: &World,
    entities: &[Entity],
) -> Result<Vec<HierarchyOperation>, HierarchyError> {
    let retirement = entities.iter().copied().collect::<EntityHashSet>();
    if retirement.is_empty() {
        return Ok(Vec::new());
    }

    validate_hierarchy(world)?;
    let mut candidate = CandidateTopology::read(world);
    for entity in retirement.iter().copied() {
        candidate.require_entity(world, entity)?;
        if !candidate.is_placed(entity) {
            return Err(HierarchyError::Unplaced { entity });
        }
    }
    let mut operations = Vec::new();
    let mut preorder = Vec::new();
    let mut postorder = Vec::with_capacity(retirement.len());
    let roots = world
        .get_resource::<HierarchyRoots>()
        .map_or_else(Vec::new, |roots| roots.as_slice().to_vec());
    let mut stack = roots
        .iter()
        .rev()
        .copied()
        .map(|entity| (entity, false))
        .collect::<Vec<_>>();
    while let Some((entity, visited)) = stack.pop() {
        if visited {
            if retirement.contains(&entity) {
                postorder.push(entity);
            }
            continue;
        }
        preorder.push(entity);
        stack.push((entity, true));
        if let Some(children) = world.get::<Children>(entity) {
            stack.extend(children.iter().rev().map(|child| (child, false)));
        }
    }
    if postorder.len() != retirement.len() {
        let visited = postorder.iter().copied().collect::<EntityHashSet>();
        let entity = retirement
            .iter()
            .copied()
            .find(|entity| !visited.contains(entity))
            .expect("retirement cardinality mismatch must identify one missing entity");
        return Err(HierarchyError::Unplaced { entity });
    }

    // Preserve subtrees outside the retirement set by promoting the surviving frontier in its
    // original depth-first structural order before former owners are removed.
    for child in preorder.into_iter().filter(|entity| {
        !retirement.contains(entity)
            && world
                .get::<Parent>(*entity)
                .is_some_and(|parent| retirement.contains(&parent.parent()))
    }) {
        let from = candidate
            .current_placement(child)
            .expect("validated surviving child must remain placed");
        let operation = HierarchyOperation::Move {
            entity: child,
            from,
            to: HierarchyPlacement::last(HierarchyPlacementOwner::Root),
        };
        candidate.apply(world, operation)?;
        operations.push(operation);
    }

    for entity in postorder {
        let from = candidate
            .current_placement(entity)
            .expect("validated retirement entity must remain placed until removal");
        let operation = HierarchyOperation::Remove { entity, from };
        candidate.apply(world, operation)?;
        operations.push(operation);
    }

    Ok(operations)
}

pub(crate) fn validate_hierarchy_retirement_despawn(
    world: &World,
    entities: &[Entity],
    affected_entities: &[Entity],
) -> Result<(), LifecycleFreeDespawnError> {
    validate_hierarchy_retirement_despawn_with_claimed_components(
        world,
        entities,
        affected_entities,
        &[],
    )
}

pub(crate) fn validate_hierarchy_retirement_despawn_with_claimed_components(
    world: &World,
    entities: &[Entity],
    affected_entities: &[Entity],
    claimed_component_ids: &[nara_ecs::bevy_ecs::component::ComponentId],
) -> Result<(), LifecycleFreeDespawnError> {
    let allowed_hooks = [
        PersistentLifecycleEvent::Insert,
        PersistentLifecycleEvent::Discard,
    ];
    let parent_id = validate_registered_component_metadata::<Parent>(world, &allowed_hooks)
        .map_err(|_| LifecycleFreeDespawnError::LifecycleWork)?;
    let children_id = validate_registered_component_metadata::<Children>(world, &allowed_hooks)
        .map_err(|_| LifecycleFreeDespawnError::LifecycleWork)?;
    let member_id = validate_registered_component_metadata::<HierarchyMember>(
        world,
        &[PersistentLifecycleEvent::Discard],
    )
    .map_err(|_| LifecycleFreeDespawnError::LifecycleWork)?;

    for entity in affected_entities.iter().copied() {
        validate_registered_component_observers::<Parent>(world, Some(entity))
            .map_err(|_| LifecycleFreeDespawnError::LifecycleWork)?;
        validate_registered_component_observers::<Children>(world, Some(entity))
            .map_err(|_| LifecycleFreeDespawnError::LifecycleWork)?;
        validate_registered_component_observers::<HierarchyMember>(world, Some(entity))
            .map_err(|_| LifecycleFreeDespawnError::LifecycleWork)?;
    }

    let hierarchy_component_ids = [parent_id, children_id, member_id];
    let claimed_component_ids = claimed_component_ids
        .iter()
        .copied()
        .filter(|component_id| !hierarchy_component_ids.contains(component_id))
        .collect::<Vec<_>>();
    let mut admission_component_ids = hierarchy_component_ids.to_vec();
    admission_component_ids.extend(claimed_component_ids.iter().copied());
    let mut unique = EntityHashSet::default();
    for entity in entities.iter().copied() {
        if !unique.insert(entity) {
            return Err(LifecycleFreeDespawnError::DuplicateEntity);
        }
        let entity_ref = world
            .get_entity(entity)
            .map_err(|_| LifecycleFreeDespawnError::EntityMissing)?;
        if claimed_component_ids.is_empty() {
            validate_entity_despawn_with_allowed_component_hooks(
                world,
                entity,
                entity_ref.archetype().components(),
                &hierarchy_component_ids,
            )
        } else {
            validate_entity_despawn_with_claimed_component_hooks(
                world,
                entity,
                entity_ref.archetype().components(),
                &admission_component_ids,
            )
        }
        .map_err(|_| LifecycleFreeDespawnError::LifecycleWork)?;
    }
    Ok(())
}

impl PreparedHierarchyBatch {
    /// Publishes the prevalidated ordered transaction without a recoverable failure branch.
    pub fn commit(self, world: &mut World) {
        let Some(projection) = self.projection else {
            return;
        };

        commit_projection(world, projection);
        mark_topology_dirty(world);
        mark_hierarchy_baseline_valid(world);
    }
}

impl<'world> HierarchyWriter<'world> {
    /// Creates a writer over an exclusively borrowed World.
    #[must_use]
    pub fn new(world: &'world mut World) -> Self {
        Self { world }
    }

    /// Registers one previously unplaced entity at an exact ordered placement.
    pub fn place(
        &mut self,
        entity: Entity,
        placement: HierarchyPlacement,
    ) -> Result<(), HierarchyError> {
        self.apply(HierarchyOperation::Place { entity, placement })
    }

    /// Moves one registered entity between exact ordered placements.
    pub fn move_entity(
        &mut self,
        entity: Entity,
        from: HierarchyPlacement,
        to: HierarchyPlacement,
    ) -> Result<(), HierarchyError> {
        self.apply(HierarchyOperation::Move { entity, from, to })
    }

    /// Removes one childless entity from the hierarchy closure.
    pub fn remove(
        &mut self,
        entity: Entity,
        from: HierarchyPlacement,
    ) -> Result<(), HierarchyError> {
        self.apply(HierarchyOperation::Remove { entity, from })
    }

    /// Applies one failure-atomic ordered hierarchy operation.
    pub fn apply(&mut self, operation: HierarchyOperation) -> Result<(), HierarchyError> {
        self.apply_batch(&[operation])
    }

    /// Preflights a sequential candidate and commits the complete batch atomically.
    pub fn apply_batch(&mut self, operations: &[HierarchyOperation]) -> Result<(), HierarchyError> {
        prepare_mutation_baseline(self.world);
        prepare_writer_batch(self.world, operations)?.commit(self.world);
        Ok(())
    }
}

/// Deferred ordered hierarchy commands.
pub trait HierarchyCommandsExt {
    /// Queues one ordered hierarchy operation.
    fn apply_hierarchy_operation(&mut self, operation: HierarchyOperation) -> &mut Self;

    /// Queues one failure-atomic ordered hierarchy batch.
    fn apply_hierarchy_batch(&mut self, operations: Vec<HierarchyOperation>) -> &mut Self;
}

impl HierarchyCommandsExt for Commands<'_, '_> {
    fn apply_hierarchy_operation(&mut self, operation: HierarchyOperation) -> &mut Self {
        self.queue(move |world: &mut World| HierarchyWriter::new(world).apply(operation));
        self
    }

    fn apply_hierarchy_batch(&mut self, operations: Vec<HierarchyOperation>) -> &mut Self {
        self.queue(move |world: &mut World| HierarchyWriter::new(world).apply_batch(&operations));
        self
    }
}

struct CandidateTopology<'world> {
    world: &'world World,
    roots: Option<CandidateSiblings>,
    children_overrides: EntityHashMap<CandidateSiblings>,
    links: EntityHashMap<SiblingLinks>,
    placement_overrides: EntityHashMap<Option<HierarchyPlacementOwner>>,
}

/// Only sibling sets reached by the batch are indexed. Public ordered Vec projections are built
/// once after all sequential edits and lifecycle admission, then the candidate index is dropped.
#[derive(Clone, Copy)]
struct CandidateSiblings {
    first: Option<Entity>,
    last: Option<Entity>,
    len: usize,
}

#[derive(Clone, Copy)]
struct SiblingLinks {
    previous: Option<Entity>,
    next: Option<Entity>,
}

struct HierarchyProjection {
    roots: Option<Vec<Entity>>,
    children: Vec<(Entity, Vec<Entity>)>,
    placements: EntityHashMap<Option<HierarchyPlacementOwner>>,
}

impl<'world> CandidateTopology<'world> {
    fn read(world: &'world World) -> Self {
        Self {
            world,
            roots: None,
            children_overrides: EntityHashMap::default(),
            links: EntityHashMap::default(),
            placement_overrides: EntityHashMap::default(),
        }
    }

    fn apply(
        &mut self,
        world: &World,
        operation: HierarchyOperation,
    ) -> Result<ResolvedOperation, HierarchyError> {
        match operation {
            HierarchyOperation::Place { entity, placement } => {
                self.require_entity(world, entity)?;
                if self.is_placed(entity) {
                    return Err(HierarchyError::AlreadyPlaced { entity });
                }
                self.validate_insertion(world, entity, placement, false)?;
                self.insert(entity, placement);
                Ok(ResolvedOperation::Place {
                    entity,
                    owner: placement.owner,
                })
            }
            HierarchyOperation::Move { entity, from, to } => {
                self.require_entity(world, entity)?;
                if !self.placement_matches(entity, from) {
                    return Err(HierarchyError::PlacementContextMismatch { entity });
                }
                self.validate_insertion(world, entity, to, true)?;
                let owner = self
                    .detach(entity)
                    .expect("matched placement must have one candidate owner");
                self.insert(entity, to);
                Ok(ResolvedOperation::Move {
                    entity,
                    from: owner,
                    to: to.owner,
                })
            }
            HierarchyOperation::Remove { entity, from } => {
                self.require_entity(world, entity)?;
                if !self.placement_matches(entity, from) {
                    return Err(HierarchyError::PlacementContextMismatch { entity });
                }
                if self.has_children(entity) {
                    return Err(HierarchyError::HasChildren { entity });
                }
                let owner = self
                    .detach(entity)
                    .expect("matched placement must have one candidate owner");
                Ok(ResolvedOperation::Remove {
                    entity,
                    from: owner,
                })
            }
        }
    }

    fn require_entity(&self, world: &World, entity: Entity) -> Result<(), HierarchyError> {
        if world.get_entity(entity).is_err() {
            return Err(HierarchyError::MissingEntity { entity });
        }
        Ok(())
    }

    fn is_placed(&self, entity: Entity) -> bool {
        self.owner(entity).is_some()
    }

    fn validate_insertion(
        &self,
        world: &World,
        entity: Entity,
        placement: HierarchyPlacement,
        check_cycle: bool,
    ) -> Result<(), HierarchyError> {
        self.validate_owner(world, entity, placement.owner, check_cycle)?;
        match placement.anchor {
            HierarchyPlacementAnchor::First | HierarchyPlacementAnchor::Last => {}
            HierarchyPlacementAnchor::Before { sibling }
            | HierarchyPlacementAnchor::After { sibling }
                if sibling == entity =>
            {
                return Err(HierarchyError::SelfAnchor { entity });
            }
            HierarchyPlacementAnchor::Before { sibling }
            | HierarchyPlacementAnchor::After { sibling } => {
                if self.owner(sibling) != Some(placement.owner) {
                    return Err(HierarchyError::AnchorMissing {
                        entity,
                        anchor: sibling,
                    });
                }
            }
        }
        Ok(())
    }

    fn validate_owner(
        &self,
        world: &World,
        entity: Entity,
        owner: HierarchyPlacementOwner,
        check_cycle: bool,
    ) -> Result<(), HierarchyError> {
        let HierarchyPlacementOwner::Parent { parent } = owner else {
            return Ok(());
        };
        if parent == entity {
            return Err(HierarchyError::SelfParent { entity });
        }
        if world.get_entity(parent).is_err() {
            return Err(HierarchyError::MissingParent {
                child: entity,
                parent,
            });
        }
        if !self.is_placed(parent) {
            return Err(HierarchyError::OwnerUnplaced { entity, parent });
        }

        if check_cycle {
            let mut cursor = parent;
            loop {
                if cursor == entity {
                    return Err(HierarchyError::Cycle { entity });
                }
                let Some(HierarchyPlacementOwner::Parent { parent }) = self.owner(cursor) else {
                    break;
                };
                cursor = parent;
            }
        }
        Ok(())
    }

    fn has_children(&self, parent: Entity) -> bool {
        self.children_overrides.get(&parent).map_or_else(
            || {
                self.world
                    .get::<Children>(parent)
                    .is_some_and(|children| !children.is_empty())
            },
            |children| children.len != 0,
        )
    }

    fn siblings(&mut self, owner: HierarchyPlacementOwner) -> CandidateSiblings {
        let indexed = match owner {
            HierarchyPlacementOwner::Root => self.roots,
            HierarchyPlacementOwner::Parent { parent } => {
                self.children_overrides.get(&parent).copied()
            }
        };
        if let Some(indexed) = indexed {
            return indexed;
        }

        let entities = siblings_for_owner(self.world, owner).unwrap_or(&[]);
        self.links.reserve(entities.len());
        for (index, entity) in entities.iter().copied().enumerate() {
            self.links.insert(
                entity,
                SiblingLinks {
                    previous: index.checked_sub(1).map(|index| entities[index]),
                    next: entities.get(index + 1).copied(),
                },
            );
        }
        let siblings = CandidateSiblings {
            first: entities.first().copied(),
            last: entities.last().copied(),
            len: entities.len(),
        };
        self.set_siblings(owner, siblings);
        siblings
    }

    fn set_siblings(&mut self, owner: HierarchyPlacementOwner, siblings: CandidateSiblings) {
        match owner {
            HierarchyPlacementOwner::Root => self.roots = Some(siblings),
            HierarchyPlacementOwner::Parent { parent } => {
                self.children_overrides.insert(parent, siblings);
            }
        }
    }

    fn placement_matches(&mut self, entity: Entity, placement: HierarchyPlacement) -> bool {
        let Some(owner) = self.owner(entity) else {
            return false;
        };
        if owner != placement.owner {
            return false;
        }
        self.siblings(owner);
        let Some(links) = self.links.get(&entity) else {
            return false;
        };
        match placement.anchor {
            HierarchyPlacementAnchor::First => links.previous.is_none(),
            HierarchyPlacementAnchor::Last => links.next.is_none(),
            HierarchyPlacementAnchor::Before { sibling } => links.next == Some(sibling),
            HierarchyPlacementAnchor::After { sibling } => links.previous == Some(sibling),
        }
    }

    fn current_placement(&mut self, entity: Entity) -> Option<HierarchyPlacement> {
        let owner = self.owner(entity)?;
        self.siblings(owner);
        let links = self.links.get(&entity)?;
        Some(HierarchyPlacement {
            owner,
            anchor: links
                .previous
                .map_or(HierarchyPlacementAnchor::First, |sibling| {
                    HierarchyPlacementAnchor::After { sibling }
                }),
        })
    }

    fn owner(&self, entity: Entity) -> Option<HierarchyPlacementOwner> {
        if let Some(owner) = self.placement_overrides.get(&entity) {
            return *owner;
        }
        placement_owner(self.world, entity)
    }

    fn detach(&mut self, entity: Entity) -> Option<HierarchyPlacementOwner> {
        let owner = self.owner(entity)?;
        let mut siblings = self.siblings(owner);
        let links = self
            .links
            .remove(&entity)
            .expect("validated placement must remain in the candidate");
        if let Some(previous) = links.previous {
            self.links
                .get_mut(&previous)
                .expect("previous sibling must be indexed")
                .next = links.next;
        } else {
            siblings.first = links.next;
        }
        if let Some(next) = links.next {
            self.links
                .get_mut(&next)
                .expect("next sibling must be indexed")
                .previous = links.previous;
        } else {
            siblings.last = links.previous;
        }
        siblings.len -= 1;
        self.set_siblings(owner, siblings);
        self.placement_overrides.insert(entity, None);
        Some(owner)
    }

    fn insert(&mut self, entity: Entity, placement: HierarchyPlacement) {
        let mut siblings = self.siblings(placement.owner);
        let (previous, next) = match placement.anchor {
            HierarchyPlacementAnchor::First => (None, siblings.first),
            HierarchyPlacementAnchor::Last => (siblings.last, None),
            HierarchyPlacementAnchor::Before { sibling } => (
                self.links
                    .get(&sibling)
                    .expect("validated anchor must be indexed")
                    .previous,
                Some(sibling),
            ),
            HierarchyPlacementAnchor::After { sibling } => (
                Some(sibling),
                self.links
                    .get(&sibling)
                    .expect("validated anchor must be indexed")
                    .next,
            ),
        };
        self.links.insert(entity, SiblingLinks { previous, next });
        if let Some(previous) = previous {
            self.links
                .get_mut(&previous)
                .expect("previous sibling must be indexed")
                .next = Some(entity);
        } else {
            siblings.first = Some(entity);
        }
        if let Some(next) = next {
            self.links
                .get_mut(&next)
                .expect("next sibling must be indexed")
                .previous = Some(entity);
        } else {
            siblings.last = Some(entity);
        }
        siblings.len += 1;
        self.set_siblings(placement.owner, siblings);
        self.placement_overrides
            .insert(entity, Some(placement.owner));
    }

    fn materialize(&self, siblings: CandidateSiblings) -> Vec<Entity> {
        let mut entities = Vec::with_capacity(siblings.len);
        let mut cursor = siblings.first;
        while let Some(entity) = cursor {
            entities.push(entity);
            cursor = self
                .links
                .get(&entity)
                .expect("final sibling must be indexed")
                .next;
        }
        debug_assert_eq!(entities.len(), siblings.len);
        entities
    }

    fn into_projection(self) -> HierarchyProjection {
        let roots = self.roots.map(|siblings| self.materialize(siblings));
        let children = self
            .children_overrides
            .iter()
            .map(|(&parent, &siblings)| (parent, self.materialize(siblings)))
            .collect();
        HierarchyProjection {
            roots,
            children,
            placements: self.placement_overrides,
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ResolvedOperation {
    Place {
        entity: Entity,
        owner: HierarchyPlacementOwner,
    },
    Move {
        entity: Entity,
        from: HierarchyPlacementOwner,
        to: HierarchyPlacementOwner,
    },
    Remove {
        entity: Entity,
        from: HierarchyPlacementOwner,
    },
}

fn validate_hierarchy_mutation_lifecycle(
    world: &World,
    operations: &[ResolvedOperation],
) -> Result<(), HierarchyError> {
    if operations.is_empty() {
        return Ok(());
    }

    validate_hierarchy_mutation_support(world)?;

    let mut parent_targets = EntityHashSet::default();
    let mut children_targets = EntityHashSet::default();
    for operation in operations {
        match *operation {
            ResolvedOperation::Place { entity, owner } => {
                validate_registered_component_observers::<HierarchyMember>(world, Some(entity))
                    .map_err(|_| HierarchyError::LifecycleWork)?;
                record_component_targets(entity, owner, &mut parent_targets, &mut children_targets);
            }
            ResolvedOperation::Move { entity, from, to } => {
                record_component_targets(entity, from, &mut parent_targets, &mut children_targets);
                record_component_targets(entity, to, &mut parent_targets, &mut children_targets);
            }
            ResolvedOperation::Remove { entity, from } => {
                validate_registered_component_observers::<HierarchyMember>(world, Some(entity))
                    .map_err(|_| HierarchyError::LifecycleWork)?;
                record_component_targets(entity, from, &mut parent_targets, &mut children_targets);
            }
        }
    }

    for entity in parent_targets {
        validate_registered_component_observers::<Parent>(world, Some(entity))
            .map_err(|_| HierarchyError::LifecycleWork)?;
    }
    for entity in children_targets {
        validate_registered_component_observers::<Children>(world, Some(entity))
            .map_err(|_| HierarchyError::LifecycleWork)?;
    }
    Ok(())
}

pub(crate) fn validate_fresh_construction_topology(
    world: &World,
    has_parent_edges: bool,
) -> Result<(), HierarchyError> {
    validate_hierarchy_mutation_support(world)?;
    validate_registered_component_observers::<HierarchyMember>(world, None)
        .map_err(|_| HierarchyError::LifecycleWork)?;
    if has_parent_edges {
        validate_registered_component_observers::<Parent>(world, None)
            .and_then(|()| validate_registered_component_observers::<Children>(world, None))
            .map_err(|_| HierarchyError::LifecycleWork)?;
    }
    Ok(())
}

fn validate_hierarchy_mutation_support(world: &World) -> Result<(), HierarchyError> {
    let roots_present = world.contains_resource::<HierarchyRoots>();
    let allowed_hooks = [
        PersistentLifecycleEvent::Insert,
        PersistentLifecycleEvent::Discard,
    ];
    validate_registered_component_metadata::<Parent>(world, &allowed_hooks)
        .map_err(|_| HierarchyError::LifecycleWork)?;
    validate_registered_component_metadata::<Children>(world, &allowed_hooks)
        .map_err(|_| HierarchyError::LifecycleWork)?;
    validate_registered_component_metadata::<HierarchyMember>(
        world,
        &[PersistentLifecycleEvent::Discard],
    )
    .map_err(|_| HierarchyError::LifecycleWork)?;
    if roots_present {
        validate_resource_scope::<HierarchyRoots>(world)
            .map_err(|_| HierarchyError::LifecycleWork)?;
    } else {
        validate_resource_insertion::<HierarchyRoots>(world)
            .map_err(|_| HierarchyError::LifecycleWork)?;
    }
    if !world.contains_resource::<HierarchyGenerationState>() {
        validate_resource_insertion::<HierarchyGenerationState>(world)
            .map_err(|_| HierarchyError::LifecycleWork)?;
    }
    if world.contains_resource::<crate::__private::CompletedHierarchyProjection>() {
        validate_resource_scope::<crate::__private::CompletedHierarchyProjection>(world)
            .map_err(|_| HierarchyError::LifecycleWork)?;
    }

    Ok(())
}

fn record_component_targets(
    entity: Entity,
    owner: HierarchyPlacementOwner,
    parent_targets: &mut EntityHashSet,
    children_targets: &mut EntityHashSet,
) {
    if let HierarchyPlacementOwner::Parent { parent } = owner {
        parent_targets.insert(entity);
        children_targets.insert(parent);
    }
}

fn prepare_batch(
    world: &World,
    operations: &[HierarchyOperation],
    validate: bool,
) -> Result<PreparedHierarchyBatch, HierarchyError> {
    if validate {
        validate_hierarchy(world)?;
    }
    let mut candidate = CandidateTopology::read(world);
    let resolved = operations
        .iter()
        .copied()
        .map(|operation| candidate.apply(world, operation))
        .collect::<Result<Vec<_>, _>>()?;
    validate_hierarchy_mutation_lifecycle(world, &resolved)?;
    Ok(PreparedHierarchyBatch {
        projection: (!resolved.is_empty()).then(|| candidate.into_projection()),
    })
}

fn commit_projection(world: &mut World, projection: HierarchyProjection) {
    let mut roots = world
        .remove_resource::<HierarchyRoots>()
        .unwrap_or_else(HierarchyRoots::new);
    if let Some(values) = projection.roots {
        roots.replace(values);
    }

    for (entity, owner) in projection.placements {
        let parent = match owner {
            Some(HierarchyPlacementOwner::Parent { parent }) => Some(parent),
            Some(HierarchyPlacementOwner::Root) | None => None,
        };
        let mut entity = world.entity_mut(entity);
        if entity.get::<Parent>().map(Parent::parent) != parent {
            if let Some(parent) = parent {
                entity.insert(Parent::new(parent));
            } else {
                entity.remove::<Parent>();
            }
        }
        if owner.is_some() {
            if entity.get::<HierarchyMember>().is_none() {
                entity.insert(HierarchyMember);
            }
        } else {
            entity.remove::<HierarchyMember>();
        }
    }

    for (parent, children) in projection.children {
        if children.is_empty() {
            world.entity_mut(parent).remove::<Children>();
        } else {
            world
                .entity_mut(parent)
                .insert(Children::from_entities(children));
        }
    }
    roots.advance_epoch();
    world.insert_resource(roots);
}

fn placement_owner(world: &World, entity: Entity) -> Option<HierarchyPlacementOwner> {
    if world
        .get_resource::<HierarchyRoots>()
        .is_some_and(|roots| roots.contains(entity))
    {
        return Some(HierarchyPlacementOwner::Root);
    }
    world
        .get::<Parent>(entity)
        .map(|parent| HierarchyPlacementOwner::Parent {
            parent: parent.parent(),
        })
}

fn siblings_for_owner(world: &World, owner: HierarchyPlacementOwner) -> Option<&[Entity]> {
    match owner {
        HierarchyPlacementOwner::Root => world
            .get_resource::<HierarchyRoots>()
            .map(HierarchyRoots::as_slice),
        HierarchyPlacementOwner::Parent { parent } => {
            world.get::<Children>(parent).map(Children::as_slice)
        }
    }
}

fn placement_in_siblings(
    owner: HierarchyPlacementOwner,
    siblings: &[Entity],
    entity: Entity,
) -> Option<HierarchyPlacement> {
    let index = siblings.iter().position(|candidate| *candidate == entity)?;
    let anchor = if index == 0 {
        HierarchyPlacementAnchor::First
    } else {
        HierarchyPlacementAnchor::After {
            sibling: siblings[index - 1],
        }
    };
    Some(HierarchyPlacement { owner, anchor })
}

#[cfg(test)]
mod tests;
