//! Persistent scene documents, authoring operations, and runtime materialization.

mod asset_bindings;
mod authoring;
#[cfg(feature = "serde")]
mod authoring_decode;
#[cfg(feature = "serde")]
mod authoring_preview;
mod candidate;
mod diagnostics;
mod document;
mod export;
#[cfg(feature = "serde")]
mod format;
#[cfg(feature = "serde")]
mod migration;
mod occurrence;
mod patch;
mod prefab;
mod product_transaction;
mod provenance;
mod runtime_hierarchy;
mod scene_components;
mod source;
mod spawn;
mod validation;

#[cfg(test)]
mod product_transaction_tests;
#[cfg(test)]
mod tests;

#[cfg(feature = "serde")]
pub use authoring::{
    AuthoringConstructionDraft, AuthoringPropertyEdit, AuthoringSessionReplacementError,
    CommittedAuthoringConstruction, ConstructionEntity, ConstructionEntityMap,
    ConstructionPlacement, LocalConstructionEntity, PreparedPrefabAuthoringConstruction,
    PreparedPrefabAuthoringEdit, PreparedSceneAuthoringConstruction, PreparedSceneAuthoringEdit,
};
pub use authoring::{
    AuthoringSessionAuthorityError, CreatedSourceEntity, PrefabAuthoringSession,
    SceneAuthoringClearReport, SceneAuthoringContentState, SceneAuthoringHistoryLimits,
    SceneAuthoringHistoryStatus, SceneAuthoringHistoryUsage, SceneAuthoringRevision,
    SceneAuthoringSession, SceneAuthoringSourceId, SceneAuthoringSyncReport,
};
#[cfg(feature = "serde")]
pub use authoring_decode::{
    AuthoringAdapterEvidence, AuthoringAdapterIdentity, AuthoringComponentView,
    AuthoringDirectMembers, AuthoringDocumentKind, AuthoringDocumentPublicationError,
    AuthoringDocumentReadiness, AuthoringDocumentReadmission, AuthoringDocumentSource,
    AuthoringEntityView, AuthoringFieldEdit, AuthoringFieldEditError, AuthoringFieldWrite,
    AuthoringPatchRecordSlot, AuthoringPatchTargetKind, AuthoringPropertyValue, AuthoringReadError,
    AuthoringRecordFact, AuthoringRecordLocation, AuthoringRecordOwnerClaim,
    AuthoringRecordReadiness, AuthoringSchemaBasis, AuthoringSessionReopenError,
    MAX_AUTHORING_EDIT_OPERATIONS, MAX_AUTHORING_FIELD_WRITES, PrefabAuthoringDocumentCandidate,
    PublishedAuthoringPrefabDocument, PublishedAuthoringSceneDocument,
    SceneAuthoringDocumentCandidate, UnavailableEntityReference, UnavailablePatchTarget,
    UnavailableRecord, UnavailableRecordManifest, UnavailableRecordManifestEntry,
    UnavailableValuePathSegment,
};
pub use candidate::{ExpandedSceneCandidate, ExpandedSceneDocument};
pub use document::{SceneComponentRecord, SceneDocument, SceneEntityRecord};
pub use export::{
    SceneExportOptions, SceneExportOutput, SceneExportRemap, SceneExportReport, export_scene,
    export_scene_with_options,
};
#[cfg(feature = "serde")]
pub use format::{
    CanonicalPrefabDocumentCandidate, CanonicalSceneDocumentCandidate, PrefabDocumentCandidate,
    PublishedPrefabDocument, PublishedSceneDocument, SceneDocumentCandidate, SceneFileBudgetError,
    SceneFileBudgetKind, SceneFileEncoding, SceneFileLimits, SceneFilePublicationError,
    SceneFormatError, ScenePatchDocumentCandidate,
};
pub use nara_identity::{SceneEntityId, SceneEntityIdError, SceneInstanceId, SpawnedSceneInstance};
pub use occurrence::{SceneOccurrenceKey, SceneOccurrenceKeyError, SceneOccurrenceLimits};
pub use patch::{
    ScenePatchApplyLimits, ScenePatchDocument, ScenePatchOperation, ScenePatchReport,
    ScenePlacement, ScenePlacementAnchor, ScenePlacementOwner,
};
pub use prefab::{
    InMemoryPrefabSourceResolver, PrefabDocument, PrefabExpansionBudgetKind, PrefabExpansionLimits,
    PrefabExpansionOptions, PrefabExpansionReport, PrefabInstance, PrefabInstantiationReport,
    PrefabSourceResolver, ResolvedPrefabSource,
};
pub use provenance::{
    PrefabProjectionPath, PrefabProjectionStep, SceneEntityProvenance, SceneEntityProvenanceMap,
};
/// Provisional APIs for advanced product integrations.
///
/// These contracts have a real reference-game consumer but are not ordinary scene-authoring
/// surface. They may change while ADR 0089 remains Proposed.
pub mod advanced {
    pub use crate::asset_bindings::SceneAssetBindings;
    pub use crate::product_transaction::{
        SceneProductOverlayWriter, SceneProductResource, SceneProductTransactionLimits,
    };
    pub use crate::spawn::{
        PreparedSceneReplacement, SceneNativeRetirementClaims,
        prepare_expanded_scene_replacement_with_claims, replace_expanded_scene_with_product,
        replace_expanded_scene_with_product_and_bindings,
    };
}
pub use scene_components::{
    Name, SCENE_COMPONENTS_PLUGIN_DECLARATION, SCENE_COMPONENTS_PLUGIN_ID,
    SCENE_COMPONENTS_SCHEMA_OWNER_ID, SCENE_COMPONENTS_SCHEMA_PROVIDER,
    SCENE_COMPONENTS_SCHEMA_PROVIDER_ID, SceneComponentsPlugin, Visibility,
    register_scene_components,
};
pub use source::{
    SourceDocumentKey, SourceDocumentKeyError, SourceDocumentKind, SourceDocumentOrigin,
};
pub use spawn::{
    PreparedScene, SceneEntityRetirementError, SceneEntitySource, SceneProjection,
    SceneSpawnReport, prepare_scene, prepare_scene_replacement, retire_and_despawn_scene_entity,
    retire_and_despawn_scene_instance, spawn_scene,
};

#[doc(hidden)]
#[cfg(feature = "serde")]
pub mod __private {
    pub use crate::authoring_decode::{
        AuthoringAdapterReceipt, AuthoringAdapterReceiptError,
        fork_published_authoring_prefab_document, fork_published_authoring_scene_document,
    };
    pub use crate::authoring_preview::{
        AuthoringPrefabPreviewDocument, AuthoringPreviewOmission, AuthoringPreviewOmissionLocation,
        AuthoringPreviewOmissionReason, AuthoringPreviewProjectionError,
        AuthoringScenePreviewDocument, project_authoring_prefab_session_preview,
        project_authoring_scene_session_preview, project_published_authoring_prefab_preview,
        project_published_authoring_scene_preview,
    };
}
