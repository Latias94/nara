use nara_ecs::{Component, Entity, Resource, entity::EntityHashMap};

/// Tracks lifetime invalidation for every placed entity, including childless roots.
#[derive(Debug, Component)]
#[component(immutable, on_discard = crate::hierarchy_component_changed)]
pub(crate) struct HierarchyMember;

/// The ordered runtime roots registered with Nara's structural hierarchy.
///
/// The collection is maintained only by [`HierarchyWriter`](crate::HierarchyWriter). It is not a
/// list of every parentless ECS entity: runtime domains opt entities into the structural closure
/// explicitly, and consumers read this sequence instead of reconstructing root order from queries
/// or entity allocation.
#[derive(Debug, PartialEq, Eq, Resource)]
pub struct HierarchyRoots(Vec<Entity>);

impl HierarchyRoots {
    pub(crate) const fn new() -> Self {
        Self(Vec::new())
    }

    pub(crate) fn insert(&mut self, index: usize, entity: Entity) {
        self.0.insert(index, entity);
    }

    pub(crate) fn remove(&mut self, entity: Entity) {
        let index = self
            .0
            .iter()
            .position(|candidate| *candidate == entity)
            .expect("prepared hierarchy root must remain registered through commit");
        self.0.remove(index);
    }

    /// Iterates registered roots in structural order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = Entity> + DoubleEndedIterator + '_ {
        self.0.iter().copied()
    }

    /// Returns registered roots as a read-only slice.
    #[must_use]
    pub fn as_slice(&self) -> &[Entity] {
        &self.0
    }

    /// Returns the number of registered roots.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether no runtime roots are registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Returns whether `entity` is a registered root.
    #[must_use]
    pub fn contains(&self, entity: Entity) -> bool {
        self.0.contains(&entity)
    }
}

/// The validated depth-first structural sequence for one completed hierarchy generation.
///
/// Consumers use this projection as a stable tie-breaker instead of query iteration or entity
/// allocation. The hierarchy completion system rebuilds it only after a dirty topology validates.
#[derive(Debug, Default, PartialEq, Eq, Resource)]
pub struct HierarchyOrder {
    generation: u64,
    roots: Vec<Entity>,
    entities: Vec<Entity>,
    indices: EntityHashMap<usize>,
}

impl HierarchyOrder {
    pub(crate) fn replace(&mut self, generation: u64, roots: Vec<Entity>, entities: Vec<Entity>) {
        self.generation = generation;
        self.roots = roots;
        self.entities = entities;
        self.indices.clear();
        self.indices.reserve(self.entities.len());
        self.indices.extend(
            self.entities
                .iter()
                .copied()
                .enumerate()
                .map(|(index, entity)| (entity, index)),
        );
    }

    /// Returns the hierarchy generation represented by this sequence.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns registered roots in their validated order.
    #[must_use]
    pub fn roots(&self) -> &[Entity] {
        &self.roots
    }

    /// Returns all hierarchy participants in depth-first structural order.
    #[must_use]
    pub fn as_slice(&self) -> &[Entity] {
        &self.entities
    }

    /// Returns one entity's stable index within this completed generation.
    #[must_use]
    pub fn index(&self, entity: Entity) -> Option<usize> {
        self.indices.get(&entity).copied()
    }

    /// Returns the number of validated hierarchy participants.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entities.len()
    }

    /// Returns whether the completed hierarchy is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty()
    }
}

/// The authoritative structural parent relation for one runtime entity.
///
/// The component is immutable and has no public constructor. Every insertion, replacement, or
/// removal immediately invalidates the completed topology generation, including advanced raw ECS
/// operations. Ordinary mutation goes through [`HierarchyWriter`](crate::HierarchyWriter).
#[derive(Debug, PartialEq, Eq, Hash, Component)]
#[component(
    immutable,
    on_insert = crate::hierarchy_component_changed,
    on_discard = crate::hierarchy_component_changed
)]
pub struct Parent(Entity);

impl Parent {
    pub(crate) const fn new(parent: Entity) -> Self {
        Self(parent)
    }

    /// Returns the structural parent entity.
    #[must_use]
    #[inline]
    pub const fn parent(&self) -> Entity {
        self.0
    }
}

/// The writer-maintained ordered reverse projection of [`Parent`] edges.
///
/// The collection is immutable, deliberately private, and exposes query-only access. Structure
/// does not own entity lifetime: raw despawn invalidates the hierarchy instead of cascading.
#[derive(Debug, PartialEq, Eq, Component)]
#[component(
    immutable,
    on_insert = crate::hierarchy_component_changed,
    on_discard = crate::hierarchy_component_changed
)]
pub struct Children(Vec<Entity>);

impl Children {
    pub(crate) const fn new() -> Self {
        Self(Vec::new())
    }

    pub(crate) fn insert(&mut self, index: usize, entity: Entity) {
        self.0.insert(index, entity);
    }

    pub(crate) fn remove(&mut self, entity: Entity) {
        let index = self
            .0
            .iter()
            .position(|candidate| *candidate == entity)
            .expect("prepared hierarchy child must remain in its reverse projection");
        self.0.remove(index);
    }

    /// Iterates children in the order committed by Nara's hierarchy writer.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = Entity> + DoubleEndedIterator + '_ {
        self.0.iter().copied()
    }

    /// Returns the children as a read-only slice.
    #[must_use]
    pub fn as_slice(&self) -> &[Entity] {
        &self.0
    }

    /// Returns the number of children in the reverse projection.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns whether the reverse projection is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
