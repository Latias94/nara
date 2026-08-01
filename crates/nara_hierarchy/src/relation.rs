use nara_ecs::{Component, Entity};

/// The authoritative structural parent relation for one runtime entity.
///
/// Ordinary Nara code creates this relation through
/// [`HierarchyConstructionWriter`](crate::HierarchyConstructionWriter). The private field prevents
/// direct construction through Nara's API; Bevy's [`Relationship`](nara_ecs::relationship::Relationship)
/// substrate remains an explicitly advanced escape hatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Component)]
#[relationship(relationship_target = Children)]
pub struct Parent(#[entities] Entity);

impl Parent {
    #[inline]
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

/// The relationship-maintained reverse projection of [`Parent`] edges.
///
/// The collection is deliberately private and exposes query-only access. It does not opt into
/// Bevy's `linked_spawn` behavior, so structure never owns entity lifetime.
#[derive(Debug, Default, PartialEq, Eq, Component)]
#[relationship_target(relationship = Parent)]
pub struct Children(Vec<Entity>);

impl Children {
    /// Iterates the children in the relationship substrate's current collection order.
    ///
    /// This order is not a persistent or product-level sibling-order contract.
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
