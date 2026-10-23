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
    Children, HierarchyError, HierarchyRoots, Parent, hierarchy_baseline_is_valid,
    mark_hierarchy_baseline_valid, mark_topology_dirty, relation::HierarchyMember,
    validate_hierarchy,
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
    resolved: Vec<ResolvedOperation>,
}

pub(crate) fn prepare_mutation_baseline(world: &mut World) {
    world.register_component::<Parent>();
    world.register_component::<Children>();
    world.register_component::<HierarchyMember>();
    world.register_component::<HierarchyRoots>();
    world.flush();
}

pub(crate) fn prepare_hierarchy_batch(
    world: &mut World,
    operations: &[HierarchyOperation],
) -> Result<PreparedHierarchyBatch, HierarchyError> {
    let resolved = resolve_operations(world, operations, true)?;
    validate_hierarchy_mutation_lifecycle(world, &resolved)?;
    Ok(PreparedHierarchyBatch { resolved })
}

fn prepare_writer_batch(
    world: &mut World,
    operations: &[HierarchyOperation],
) -> Result<PreparedHierarchyBatch, HierarchyError> {
    let resolved = resolve_operations(world, operations, !hierarchy_baseline_is_valid(world))?;
    validate_hierarchy_mutation_lifecycle(world, &resolved)?;
    Ok(PreparedHierarchyBatch { resolved })
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
        let entity = retirement
            .iter()
            .copied()
            .find(|entity| !postorder.contains(entity))
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
        if self.resolved.is_empty() {
            return;
        }

        commit_resolved(world, self.resolved);
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
    roots: Option<Vec<Entity>>,
    children_overrides: EntityHashMap<Vec<Entity>>,
    placement_overrides: EntityHashMap<Option<HierarchyPlacementOwner>>,
}

impl<'world> CandidateTopology<'world> {
    fn read(world: &'world World) -> Self {
        Self {
            world,
            roots: None,
            children_overrides: EntityHashMap::default(),
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
                let insertion = self.resolve_insertion(world, entity, placement, false)?;
                self.insert(entity, insertion);
                Ok(ResolvedOperation::Place { entity, insertion })
            }
            HierarchyOperation::Move { entity, from, to } => {
                self.require_entity(world, entity)?;
                if !self.placement_matches(entity, from) {
                    return Err(HierarchyError::PlacementContextMismatch { entity });
                }
                let insertion = self.resolve_insertion(world, entity, to, true)?;
                let owner = self
                    .detach(entity)
                    .expect("matched placement must have one candidate owner");
                self.insert(entity, insertion);
                Ok(ResolvedOperation::Move {
                    entity,
                    from: owner,
                    insertion,
                })
            }
            HierarchyOperation::Remove { entity, from } => {
                self.require_entity(world, entity)?;
                if !self.placement_matches(entity, from) {
                    return Err(HierarchyError::PlacementContextMismatch { entity });
                }
                if !self
                    .siblings(HierarchyPlacementOwner::Parent { parent: entity })
                    .is_empty()
                {
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

    fn resolve_insertion(
        &self,
        world: &World,
        entity: Entity,
        placement: HierarchyPlacement,
        remove_entity_first: bool,
    ) -> Result<ResolvedInsertion, HierarchyError> {
        self.validate_owner(world, entity, placement.owner, remove_entity_first)?;
        let siblings = self.siblings(placement.owner);
        let index = match placement.anchor {
            HierarchyPlacementAnchor::First => 0,
            HierarchyPlacementAnchor::Last => {
                siblings.len() - usize::from(remove_entity_first && siblings.contains(&entity))
            }
            HierarchyPlacementAnchor::Before { sibling }
            | HierarchyPlacementAnchor::After { sibling }
                if sibling == entity =>
            {
                return Err(HierarchyError::SelfAnchor { entity });
            }
            HierarchyPlacementAnchor::Before { sibling }
            | HierarchyPlacementAnchor::After { sibling } => {
                let Some(anchor_index) =
                    siblings.iter().position(|candidate| *candidate == sibling)
                else {
                    return Err(HierarchyError::AnchorMissing {
                        entity,
                        anchor: sibling,
                    });
                };
                let mut index =
                    if matches!(placement.anchor, HierarchyPlacementAnchor::Before { .. }) {
                        anchor_index
                    } else {
                        anchor_index.saturating_add(1)
                    };
                if remove_entity_first
                    && siblings
                        .iter()
                        .position(|candidate| *candidate == entity)
                        .is_some_and(|entity_index| entity_index < index)
                {
                    index -= 1;
                }
                index
            }
        };
        Ok(ResolvedInsertion {
            owner: placement.owner,
            index,
        })
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

    fn siblings(&self, owner: HierarchyPlacementOwner) -> &[Entity] {
        match owner {
            HierarchyPlacementOwner::Root => match &self.roots {
                Some(roots) => roots.as_slice(),
                None => self
                    .world
                    .get_resource::<HierarchyRoots>()
                    .map_or(&[], HierarchyRoots::as_slice),
            },
            HierarchyPlacementOwner::Parent { parent } => self
                .children_overrides
                .get(&parent)
                .map(Vec::as_slice)
                .or_else(|| self.world.get::<Children>(parent).map(Children::as_slice))
                .unwrap_or(&[]),
        }
    }

    fn placement_matches(&self, entity: Entity, placement: HierarchyPlacement) -> bool {
        let Some(owner) = self.owner(entity) else {
            return false;
        };
        if owner != placement.owner {
            return false;
        }
        let siblings = self.siblings(owner);
        let Some(index) = siblings.iter().position(|candidate| *candidate == entity) else {
            return false;
        };
        match placement.anchor {
            HierarchyPlacementAnchor::First => index == 0,
            HierarchyPlacementAnchor::Last => index + 1 == siblings.len(),
            HierarchyPlacementAnchor::Before { sibling } => {
                siblings.get(index + 1) == Some(&sibling)
            }
            HierarchyPlacementAnchor::After { sibling } => {
                index > 0 && siblings.get(index - 1) == Some(&sibling)
            }
        }
    }

    fn current_placement(&self, entity: Entity) -> Option<HierarchyPlacement> {
        let owner = self.owner(entity)?;
        placement_in_siblings(owner, self.siblings(owner), entity)
    }

    fn owner(&self, entity: Entity) -> Option<HierarchyPlacementOwner> {
        if let Some(owner) = self.placement_overrides.get(&entity) {
            return *owner;
        }
        let is_root = if let Some(roots) = &self.roots {
            roots.contains(&entity)
        } else {
            self.world
                .get_resource::<HierarchyRoots>()
                .is_some_and(|roots| roots.contains(entity))
        };
        if is_root {
            return Some(HierarchyPlacementOwner::Root);
        }
        self.world
            .get::<Parent>(entity)
            .map(|parent| HierarchyPlacementOwner::Parent {
                parent: parent.parent(),
            })
    }

    fn detach(&mut self, entity: Entity) -> Option<HierarchyPlacementOwner> {
        let owner = self.owner(entity)?;
        let index = self
            .siblings(owner)
            .iter()
            .position(|candidate| *candidate == entity)
            .expect("validated placement must remain in the candidate");
        match owner {
            HierarchyPlacementOwner::Root => {
                self.roots_mut().remove(index);
            }
            HierarchyPlacementOwner::Parent { parent } => {
                self.children_mut(parent).remove(index);
            }
        }
        self.placement_overrides.insert(entity, None);
        Some(owner)
    }

    fn insert(&mut self, entity: Entity, insertion: ResolvedInsertion) {
        match insertion.owner {
            HierarchyPlacementOwner::Root => {
                self.roots_mut().insert(insertion.index, entity);
            }
            HierarchyPlacementOwner::Parent { parent } => {
                self.children_mut(parent).insert(insertion.index, entity);
            }
        }
        self.placement_overrides
            .insert(entity, Some(insertion.owner));
    }

    fn roots_mut(&mut self) -> &mut Vec<Entity> {
        if self.roots.is_none() {
            let roots = self
                .world
                .get_resource::<HierarchyRoots>()
                .map_or_else(Vec::new, |roots| roots.as_slice().to_vec());
            self.roots = Some(roots);
        }
        self.roots.as_mut().expect("roots override must exist")
    }

    fn children_mut(&mut self, parent: Entity) -> &mut Vec<Entity> {
        if !self.children_overrides.contains_key(&parent) {
            let children = self
                .world
                .get::<Children>(parent)
                .map_or_else(Vec::new, |children| children.as_slice().to_vec());
            self.children_overrides.insert(parent, children);
        }
        self.children_overrides
            .get_mut(&parent)
            .expect("children override must be inserted before mutation")
    }
}

#[derive(Debug, Clone, Copy)]
struct ResolvedInsertion {
    owner: HierarchyPlacementOwner,
    index: usize,
}

#[derive(Debug, Clone, Copy)]
enum ResolvedOperation {
    Place {
        entity: Entity,
        insertion: ResolvedInsertion,
    },
    Move {
        entity: Entity,
        from: HierarchyPlacementOwner,
        insertion: ResolvedInsertion,
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

    let mut parent_targets = EntityHashSet::default();
    let mut children_targets = EntityHashSet::default();
    for operation in operations {
        match *operation {
            ResolvedOperation::Place { entity, insertion } => {
                validate_registered_component_observers::<HierarchyMember>(world, Some(entity))
                    .map_err(|_| HierarchyError::LifecycleWork)?;
                record_component_targets(
                    entity,
                    insertion.owner,
                    &mut parent_targets,
                    &mut children_targets,
                );
            }
            ResolvedOperation::Move {
                entity,
                from,
                insertion,
            } => {
                record_component_targets(entity, from, &mut parent_targets, &mut children_targets);
                record_component_targets(
                    entity,
                    insertion.owner,
                    &mut parent_targets,
                    &mut children_targets,
                );
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

fn resolve_operations(
    world: &World,
    operations: &[HierarchyOperation],
    validate: bool,
) -> Result<Vec<ResolvedOperation>, HierarchyError> {
    if validate {
        validate_hierarchy(world)?;
    }
    let mut candidate = CandidateTopology::read(world);
    operations
        .iter()
        .copied()
        .map(|operation| candidate.apply(world, operation))
        .collect()
}

fn commit_resolved(world: &mut World, operations: Vec<ResolvedOperation>) {
    let mut roots = world
        .remove_resource::<HierarchyRoots>()
        .unwrap_or_else(HierarchyRoots::new);
    for operation in operations {
        match operation {
            ResolvedOperation::Place { entity, insertion } => {
                world.entity_mut(entity).insert(HierarchyMember);
                commit_insert(world, &mut roots, entity, insertion);
            }
            ResolvedOperation::Move {
                entity,
                from,
                insertion,
            } => {
                commit_detach(world, &mut roots, entity, from);
                commit_insert(world, &mut roots, entity, insertion);
            }
            ResolvedOperation::Remove { entity, from } => {
                commit_detach(world, &mut roots, entity, from);
                world.entity_mut(entity).remove::<HierarchyMember>();
            }
        }
    }
    roots.advance_epoch();
    world.insert_resource(roots);
}

fn commit_insert(
    world: &mut World,
    roots: &mut HierarchyRoots,
    entity: Entity,
    insertion: ResolvedInsertion,
) {
    match insertion.owner {
        HierarchyPlacementOwner::Root => roots.insert(insertion.index, entity),
        HierarchyPlacementOwner::Parent { parent } => {
            let mut children = world
                .entity_mut(parent)
                .take::<Children>()
                .unwrap_or_else(Children::new);
            children.insert(insertion.index, entity);
            world.entity_mut(parent).insert(children);
            world.entity_mut(entity).insert(Parent::new(parent));
        }
    }
}

fn commit_detach(
    world: &mut World,
    roots: &mut HierarchyRoots,
    entity: Entity,
    owner: HierarchyPlacementOwner,
) {
    match owner {
        HierarchyPlacementOwner::Root => roots.remove(entity),
        HierarchyPlacementOwner::Parent { parent } => {
            let _ = world.entity_mut(entity).take::<Parent>();
            let mut children = world
                .entity_mut(parent)
                .take::<Children>()
                .expect("prepared hierarchy parent must retain its reverse projection");
            children.remove(entity);
            if !children.is_empty() {
                world.entity_mut(parent).insert(children);
            }
        }
    }
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
