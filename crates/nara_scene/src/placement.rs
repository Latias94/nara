//! Source-owned desired locations shared by Scene, Prefab and instance overlays.

use nara_identity::{PrefabEntityTarget, SceneEntityId};

/// Parent in the actual declaring source or overlay's effective hierarchy.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "kind",
        content = "target",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
pub enum ScenePlacementParent {
    Root,
    Entity(PrefabEntityTarget),
}

/// Desired sibling order, without a snapshot of the preceding effective hierarchy.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "kind",
        content = "target",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
pub enum ScenePlacementSlot {
    First,
    Last,
    Before(PrefabEntityTarget),
    After(PrefabEntityTarget),
}

/// A desired location interpreted in its real declaration scope.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct ScenePlacement {
    pub parent: ScenePlacementParent,
    pub slot: ScenePlacementSlot,
}

impl ScenePlacement {
    #[must_use]
    pub const fn first(parent: ScenePlacementParent) -> Self {
        Self {
            parent,
            slot: ScenePlacementSlot::First,
        }
    }

    #[must_use]
    pub const fn last(parent: ScenePlacementParent) -> Self {
        Self {
            parent,
            slot: ScenePlacementSlot::Last,
        }
    }

    #[must_use]
    pub const fn before(parent: ScenePlacementParent, sibling: PrefabEntityTarget) -> Self {
        Self {
            parent,
            slot: ScenePlacementSlot::Before(sibling),
        }
    }

    #[must_use]
    pub const fn after(parent: ScenePlacementParent, sibling: PrefabEntityTarget) -> Self {
        Self {
            parent,
            slot: ScenePlacementSlot::After(sibling),
        }
    }
}

/// One ordered source declaration assigning a desired location to a structural entity target.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct SceneEntityPlacement {
    pub target: PrefabEntityTarget,
    pub placement: ScenePlacement,
}

impl SceneEntityPlacement {
    #[must_use]
    pub const fn new(target: PrefabEntityTarget, placement: ScenePlacement) -> Self {
        Self { target, placement }
    }

    #[must_use]
    pub const fn root(entity: SceneEntityId) -> Self {
        Self::new(
            PrefabEntityTarget::source(entity),
            ScenePlacement::last(ScenePlacementParent::Root),
        )
    }
}
