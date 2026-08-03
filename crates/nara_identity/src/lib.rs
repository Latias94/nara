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

/// Workspace-internal Scene identity transaction support.
///
/// Rust has no friend-crate visibility. These symbols are public only so the separately compiled
/// scene owner can compose them; they are excluded from the root product facade and carry no
/// external compatibility promise.
#[doc(hidden)]
pub mod __private {
    pub use crate::domain::{
        AdditionalRetirementIdentityError, IdentitySupportTopologyError,
        PreparedSceneInstanceRegistration, PreparedSceneInstanceReplacement,
        PreparedSceneInstanceRetirement, prepare_exact_scene_instance_registration,
        prepare_exact_scene_instance_replacement, prepare_exact_scene_instance_retirement,
        validate_additional_retirement_identity_axes, validate_identity_support_topology,
    };
}

#[cfg(test)]
pub(crate) use allocator::MonotonicNonZeroU64Allocator;
#[cfg(test)]
pub(crate) use domain::allocate_world_identity_domain_id_from;

#[cfg(test)]
mod tests;
