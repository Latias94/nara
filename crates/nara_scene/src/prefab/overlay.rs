//! Durable instance intent, separate from revision-bound Scene transactions.

use std::collections::BTreeMap;

use nara_reflect::{
    ComponentPropertyPath, ComponentSchemaVersion, ComponentTypeId, ComponentValue,
};

pub use nara_identity::{PrefabEntityOrigin, PrefabEntityTarget, PrefabInstanceHop};

use crate::{
    PrefabInstance, SceneCompilationFailureKind, SceneComponentRecord, SceneEntityId,
    SourceDocumentKey,
};

pub(super) mod compile;
mod components;
pub(crate) mod cost;
mod layout;
mod properties;
mod references;
#[cfg(feature = "serde")]
pub(crate) use properties::validate_claim_paths;

#[cfg(all(test, feature = "serde"))]
mod regression_tests;

/// Complete source data for an overlay-local entity.
///
/// Placements are the sole hierarchy representation. Components and an optional nested instance
/// belong here rather than in duplicate current-layer component or instance claims.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PrefabAddition {
    pub id: SceneEntityId,
    #[cfg_attr(feature = "serde", serde(default))]
    pub components: BTreeMap<ComponentTypeId, SceneComponentRecord>,
    #[cfg_attr(
        feature = "serde",
        serde(default, skip_serializing_if = "Option::is_none")
    )]
    pub prefab: Option<PrefabInstance>,
}

impl PrefabAddition {
    #[must_use]
    pub fn new(id: SceneEntityId) -> Self {
        Self {
            id,
            components: BTreeMap::new(),
            prefab: None,
        }
    }
}

/// Pins the complete optional declaration without an old declaration precondition.
///
/// Removing the claim resumes inheritance. `None` explicitly detaches; it does not convert the
/// selected instance's projection into local records. Exact comparison belongs to the containing
/// source transaction, not this durable intent.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PrefabInstanceClaim {
    pub target: PrefabEntityTarget,
    pub value: Option<PrefabInstance>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PrefabComponentSetMode {
    Add,
    Replace,
}

/// An explicit property pin or authored absence; removing it resumes inheritance.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "kind",
        content = "value",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
pub enum PrefabPropertyIntent {
    Set(ComponentValue),
    Absent,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PrefabPropertyClaim {
    pub path: ComponentPropertyPath,
    pub intent: PrefabPropertyIntent,
}

/// One non-overlapping component declaration for a target in this overlay.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(
    feature = "serde",
    serde(
        tag = "kind",
        content = "value",
        rename_all = "snake_case",
        deny_unknown_fields
    )
)]
pub enum PrefabComponentIntent {
    Set {
        mode: PrefabComponentSetMode,
        target: PrefabEntityTarget,
        component: ComponentTypeId,
        record: SceneComponentRecord,
    },
    Suppress {
        target: PrefabEntityTarget,
        component: ComponentTypeId,
    },
    Properties {
        target: PrefabEntityTarget,
        component: ComponentTypeId,
        version: ComponentSchemaVersion,
        claims: Vec<PrefabPropertyClaim>,
    },
}

/// Durable instance intent bound to a canonical base, independent of exact edit history.
///
/// Empty overlays still bind their base. Only an explicit source edit removes that binding.
/// Decoding these values does not admit a source, validate targets or publish a projection.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(deny_unknown_fields))]
pub struct PrefabOverlay {
    pub base: SourceDocumentKey,
    #[cfg_attr(feature = "serde", serde(default))]
    pub additions: Vec<PrefabAddition>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub instances: Vec<PrefabInstanceClaim>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub components: Vec<PrefabComponentIntent>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub placements: Vec<crate::SceneEntityPlacement>,
    #[cfg_attr(feature = "serde", serde(default))]
    pub suppressions: Vec<PrefabEntityTarget>,
}

impl PrefabOverlay {
    #[must_use]
    pub const fn new(base: SourceDocumentKey) -> Self {
        Self {
            base,
            additions: Vec::new(),
            instances: Vec::new(),
            components: Vec::new(),
            placements: Vec::new(),
            suppressions: Vec::new(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.additions.is_empty()
            && self.instances.is_empty()
            && self.components.is_empty()
            && self.placements.is_empty()
            && self.suppressions.is_empty()
    }
}
