//! Authored entity references and world-scoped runtime identity.
//!
//! Enable `serde` to read and write authored and runtime reference values.

mod allocator;
pub mod domain;
mod reference;
mod remap;
mod types;

pub use allocator::IdentityAllocationError;
pub use domain::{
    EntityIdentityAxis, EntityLookup, IdentityDomainError, IdentityDomainStats, IdentityTombstone,
    IdentityTombstoneSubject, PersistentRetirementDisposition, SceneIdentitySnapshot,
    SpawnedSceneInstance, TombstoneCause, WorldEntityLocators, WorldEntityToken,
    WorldIdentityDomain, WorldIdentityDomainSettings, adopt_identity_entity,
    register_persistent_identity, register_scene_instance, resolve_in_world,
    retire_identity_entity, retire_identity_entity_with_disposition, spawn_identity_entity,
};
pub use reference::{
    AuthoredEntityReference, AuthoredReferenceTarget, EntityReferenceCost, PrefabEntityOrigin,
    PrefabEntityTarget, PrefabInstanceHop, ReferenceInputId, ReferenceInputIdError,
};
pub use remap::{EntityReferenceRemap, IdentityRemapError, WorldEntityLocatorRemap};
pub use types::{
    EntityReference, PersistentRuntimeId, PersistentRuntimeIdError, PersistentRuntimeNamespaceId,
    PersistentRuntimeNamespaceIdError, PersistentRuntimeReference, RuntimeEntityIncarnation,
    RuntimeEntityIncarnationError, RuntimeEntityReference, SceneEntityId, SceneEntityIdError,
    SceneInstanceId, WorldEntityLocator, WorldIdentityDomainId,
};

#[cfg(test)]
use allocator::MonotonicNonZeroU64Allocator;
#[cfg(test)]
use domain::allocate_world_identity_domain_id_from;

#[cfg(test)]
mod tests;
