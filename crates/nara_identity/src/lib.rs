//! Stable semantic identity and world-scoped runtime lookup for nara.

mod allocator;
mod domain;
mod remap;
mod types;

pub use allocator::IdentityAllocationError;
pub use domain::{
    EntityIdentityAxis, EntityLookup, IdentityDomainError, IdentityDomainStats, IdentityTombstone,
    IdentityTombstoneSubject, SceneIdentitySnapshot, SpawnedSceneInstance, TombstoneCause,
    WorldEntityLocators, WorldEntityToken, WorldIdentityDomain, WorldIdentityDomainSettings,
    WorldIdentitySettingsError, resolve_in_world, spawn_identity_entity,
};
pub use remap::{EntityReferenceRemap, IdentityRemapError, WorldEntityLocatorRemap};
pub use types::{
    EntityReference, PersistentRuntimeId, PersistentRuntimeIdError, PersistentRuntimeNamespaceId,
    PersistentRuntimeNamespaceIdError, PersistentRuntimeReference, RuntimeEntityReference,
    SceneEntityId, SceneEntityIdError, SceneInstanceId, WorldEntityLocator, WorldIdentityDomainId,
};

#[doc(hidden)]
pub mod __private {
    pub use crate::domain::{IdentitySupportTopologyError, validate_identity_support_topology};
}

#[cfg(test)]
pub(crate) use allocator::MonotonicNonZeroU64Allocator;
#[cfg(test)]
pub(crate) use domain::allocate_world_identity_domain_id_from;

#[cfg(test)]
mod tests;
