//! Stable semantic identity and world-scoped runtime lookup for nara.
//!
//! Value identities and references are available without App or ECS dependencies. The `runtime`
//! feature enables ECS components, World identity domains, and Scene transaction support.
//! The independent `serde` feature enables value serialization without runtime integration.

mod allocator;
#[cfg(feature = "runtime")]
mod domain;
mod reference;
mod remap;
mod source;
mod types;

pub use allocator::IdentityAllocationError;
#[cfg(feature = "runtime")]
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
pub use source::{
    AssetPathError, AssetSourcePath, SourceDocumentBindingError, SourceDocumentKey,
    SourceDocumentKeyError, SourceDocumentKind, SourceDocumentOrigin, StableAssetId,
    StableAssetIdError, StableAssetIdGenerationError,
};
pub use types::{
    EntityReference, PersistentRuntimeId, PersistentRuntimeIdError, PersistentRuntimeNamespaceId,
    PersistentRuntimeNamespaceIdError, PersistentRuntimeReference, RuntimeEntityIncarnation,
    RuntimeEntityIncarnationError, RuntimeEntityReference, SceneEntityId, SceneEntityIdError,
    SceneInstanceId, WorldEntityLocator, WorldIdentityDomainId,
};

/// Workspace-internal Scene identity transaction support.
///
/// Rust has no friend-crate visibility. These symbols are public only so the separately compiled
/// scene owner can compose them; they are excluded from the root product facade and carry no
/// external compatibility promise.
#[doc(hidden)]
#[cfg(feature = "runtime")]
pub mod __private {
    pub use crate::domain::{
        AdditionalRetirementIdentityError, IdentitySupportTopologyError, PreparedEntityRetirement,
        PreparedSceneInstanceRegistration, PreparedSceneInstanceReplacement,
        PreparedSceneInstanceRetirement, prepare_exact_entity_retirement,
        prepare_exact_scene_instance_registration, prepare_exact_scene_instance_replacement,
        prepare_exact_scene_instance_retirement, prepare_registered_scene_instance_replacement,
        prepare_registered_scene_instance_retirement,
        prepare_registered_scene_projection_epoch_retirement,
        prepare_scene_projection_epoch_retirement, validate_additional_retirement_identity_axes,
        validate_identity_support_topology, validate_registered_identity_support_topology,
    };
}

#[cfg(test)]
pub(crate) use allocator::MonotonicNonZeroU64Allocator;
#[cfg(all(test, feature = "runtime"))]
pub(crate) use domain::allocate_world_identity_domain_id_from;

#[cfg(test)]
mod tests;
