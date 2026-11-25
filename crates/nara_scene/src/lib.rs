//! Persistent scene documents, authoring operations, and runtime materialization.

mod asset_reservations;
pub mod authoring;
#[cfg(feature = "serde")]
mod authoring_decode;
#[cfg(feature = "serde")]
mod authoring_preview;
mod candidate;
mod compiled;
mod component_binding_plugin;
mod component_bindings;
mod component_payload;
mod data_components;
mod diagnostics;
mod document;
mod draft;
mod effective_component_payload;
mod export;
pub mod fixed_spatial;
#[cfg(feature = "serde")]
mod format;
pub mod hierarchy;
#[cfg(feature = "serde")]
mod migration;
mod occurrence;
mod patch;
mod placement;
mod prefab;
#[cfg(feature = "serde")]
mod prefab_copy;
mod product_transaction;
mod provenance;
#[cfg(feature = "runtime-catalog")]
pub mod runtime_catalog;
#[cfg(feature = "runtime-configuration")]
pub mod runtime_configuration;
mod runtime_hierarchy;
#[cfg(feature = "serde")]
mod runtime_scene;
mod scene_components;
mod schema_authority;
mod source_hierarchy;
pub mod spatial;
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
    ConstructionPlacement, LocalConstructionEntity, LocalizedPrefabEntity, PrefabConversionError,
    PrefabConversionReceipt, PrefabConversionRequest, PrefabDeclarationError,
    PrefabDeclarationLimits, PrefabInstanceObservation, PreparedPrefabAuthoringConstruction,
    PreparedPrefabAuthoringEdit, PreparedPrefabConversion, PreparedPrefabSchemaReclassification,
    PreparedSceneAuthoringConstruction, PreparedSceneAuthoringEdit, PreparedScenePrefabConversion,
    PreparedSceneSchemaReclassification, ScenePrefabConversionRequest, SchemaReclassificationKind,
    SchemaReclassificationLimits, SchemaReclassificationUsage, SchemaReclassificationWork,
};
pub use authoring::{
    AuthoringSessionAuthorityError, CreatedSourceEntity, PrefabAuthoringSession,
    SceneAuthoringClearReport, SceneAuthoringContentState, SceneAuthoringHistoryLimits,
    SceneAuthoringHistoryStatus, SceneAuthoringHistoryUsage, SceneAuthoringRevision,
    SceneAuthoringSession, SceneAuthoringSourceId, SceneAuthoringSyncReport,
};
#[cfg(feature = "serde")]
#[doc(hidden)]
pub use authoring::{PrefabComponentPropertyClaims, PrefabPropertyClaims};
#[cfg(feature = "serde")]
pub use authoring_decode::{
    AuthoringAdapterEvidence, AuthoringAdapterIdentity, AuthoringComponentView,
    AuthoringDirectMembers, AuthoringDocumentKind, AuthoringDocumentPublicationError,
    AuthoringDocumentReadiness, AuthoringDocumentReadmission, AuthoringDocumentSource,
    AuthoringEntityView, AuthoringFieldEdit, AuthoringFieldEditError, AuthoringFieldWrite,
    AuthoringOverlayPathSegment, AuthoringOverlayRecordSlot, AuthoringOverlayTargetKind,
    AuthoringPropertyChild, AuthoringPropertyChildrenLimits, AuthoringPropertyChildrenPage,
    AuthoringPropertyChildrenPosition, AuthoringPropertyValue, AuthoringReadError,
    AuthoringRecordFact, AuthoringRecordLocation, AuthoringRecordOwnerClaim,
    AuthoringRecordReadiness, AuthoringSchemaBasis, AuthoringSessionReopenError,
    MAX_AUTHORING_EDIT_OPERATIONS, MAX_AUTHORING_FIELD_LOGICAL_BYTES,
    MAX_AUTHORING_FIELD_VALUE_NODES, MAX_AUTHORING_FIELD_WRITES, PrefabAuthoringDocumentCandidate,
    PublishedAuthoringPrefabDocument, PublishedAuthoringSceneDocument,
    SceneAuthoringDocumentCandidate, UnavailableEntityReference, UnavailableOverlayTarget,
    UnavailableRecord, UnavailableRecordManifest, UnavailableRecordManifestEntry,
    UnavailableValuePathSegment,
};
pub use candidate::ExpandedSceneDocument;
pub use compiled::{
    CompiledSceneCandidate, CompiledSceneComponent, CompiledSceneDocument, CompiledSceneEntity,
};
pub use component_binding_plugin::{
    SCENE_COMPONENT_BINDINGS_PLUGIN_DECLARATION, SCENE_COMPONENT_BINDINGS_PLUGIN_ID,
    SCENE_COMPONENT_BINDINGS_PLUGIN_REQUIREMENT, SceneComponentBindingsAuthorityError,
    SceneComponentBindingsPlugin, scene_component_bindings,
    validate_scene_component_bindings_authority,
};
pub use component_bindings::{
    SceneComponentBindingError, SceneComponentBindingLimits, SceneComponentBindings,
    SceneComponentBindingsWitness,
};
pub use data_components::{SceneDataComponentError, SceneDataComponents};
pub use document::{SceneComponentRecord, SceneDocument, SceneEntityRecord};
pub use draft::{ExpandedSceneComponent, ExpandedSceneEntity};
pub use export::{
    LiveSceneExportOptions, LiveSceneExportOutput, LiveSceneExportRemap, LiveSceneExportReport,
    export_live_scene, export_live_scene_with_options,
};
#[cfg(feature = "serde")]
pub use format::{
    CanonicalPrefabDocumentCandidate, CanonicalSceneDocumentCandidate, PrefabDocumentCandidate,
    PublishedPrefabDocument, PublishedSceneDocument, SceneDocumentCandidate, SceneFileBudgetError,
    SceneFileBudgetKind, SceneFileDecodingAdmissionError, SceneFileEncoding,
    SceneFileEncodingAdmissionError, SceneFileLimits, SceneFilePublicationError, SceneFormatError,
    ScenePatchDocumentCandidate,
};
pub use nara_core::source::{
    SourceDocumentBindingError, SourceDocumentKey, SourceDocumentKeyError, SourceDocumentKind,
    SourceDocumentOrigin,
};
pub use nara_ecs::identity::{
    SceneEntityId, SceneEntityIdError, SceneInstanceId, SpawnedSceneInstance,
};
pub use occurrence::{SceneOccurrenceKey, SceneOccurrenceKeyError, SceneOccurrenceLimits};
pub use patch::{
    DefaultPropertyEdit, ScenePatchApplyLimits, ScenePatchDocument, ScenePatchOperation,
    ScenePatchReport,
};
pub use placement::{
    SceneEntityPlacement, ScenePlacement, ScenePlacementParent, ScenePlacementSlot,
};
#[cfg(feature = "serde")]
pub use prefab::ObservedPrefabRevert;
#[cfg(feature = "serde")]
pub use prefab::inspection::{
    AuthoringPrefabComponentFact, AuthoringPrefabComponentOverride,
    AuthoringPrefabComponentOverrideKind, AuthoringPrefabComponentView,
    AuthoringPrefabInstanceSnapshot, AuthoringPrefabProperty, AuthoringPrefabPropertyClaim,
    AuthoringPrefabPropertyClaimKind, AuthoringPrefabPropertyValue, AuthoringPrefabRecordOrigin,
    AuthoringPrefabTargetView, AuthoringPrefabValue, AuthoringPrefabValueError,
};
pub use prefab::{
    InMemoryPrefabSourceResolver, PrefabAddition, PrefabComponentEdit, PrefabComponentIntent,
    PrefabComponentSetMode, PrefabDeclarationSource, PrefabDocument, PrefabEntityOrigin,
    PrefabEntityTarget, PrefabInstance, PrefabInstanceBinding, PrefabInstanceClaim,
    PrefabInstanceHop, PrefabInstanceLocation, PrefabInstancePath, PrefabInstancePathSegment,
    PrefabInstantiationReport, PrefabOverlay, PrefabOverlaySlot, PrefabPropertyClaim,
    PrefabPropertyClaimRef, PrefabPropertyIntent, PrefabReferenceArguments, PrefabRevert,
    PrefabSourceResolver, ResolvedPrefabSource, SceneCompilationBudgetKind,
    SceneCompilationFailure, SceneCompilationFailureKind, SceneCompilationLimits,
    SceneCompilationOptions, SceneCompilationReport, SceneCompilationWorkingSet,
};
#[cfg(feature = "serde")]
pub use prefab_copy::{
    PrefabCopyError, PrefabCopyLimits, PrefabCopyRequest, PrefabCopySchemaError, PrefabCopySource,
    PreparedPrefabCopy, PreparedPrefabCopySource,
};
pub use provenance::{
    PrefabProjectionPath, PrefabProjectionStep, SceneEntityProvenance, SceneEntityProvenanceMap,
};
#[cfg(feature = "serde")]
pub use runtime_scene::{
    RuntimeSceneCandidate, RuntimeSceneDocumentCandidate, RuntimeSceneError, RuntimeSceneLimits,
};
pub use source_hierarchy::SourceLocalSceneHierarchy;
/// Provisional APIs for advanced product integrations.
///
/// These contracts have a real reference-game consumer but are not ordinary scene-authoring
/// surface. They may change while ADR 0089 remains Proposed.
pub mod advanced {
    pub use crate::asset_reservations::SceneAssetReservations;
    #[cfg(feature = "serde")]
    pub use crate::occurrence::scene_instance_retired;
    pub use crate::product_transaction::{
        SceneProductOverlayWriter, SceneProductResource, SceneProductRetirement,
        SceneProductTransactionLimits,
    };
    pub use crate::spawn::{
        SceneNativeRetirementClaims, StagedSceneReplacement, replace_expanded_scene_with_product,
        replace_expanded_scene_with_product_and_reservations,
        stage_expanded_scene_replacement_with_claims,
    };
}
pub use scene_components::{
    Name, SCENE_COMPONENTS_PLUGIN_DECLARATION, SCENE_COMPONENTS_PLUGIN_ID,
    SCENE_COMPONENTS_SCHEMA_OWNER_ID, SCENE_COMPONENTS_SCHEMA_PROVIDER,
    SCENE_COMPONENTS_SCHEMA_PROVIDER_ID, SceneComponentsPlugin, register_scene_components,
};
pub use spatial::{
    SPATIAL_PLUGIN_ID, SpatialPlugin, SpatialSet, TRANSFORM_COMPONENTS_PLUGIN_DECLARATION,
    TRANSFORM_COMPONENTS_PLUGIN_ID, TRANSFORM_SCHEMA_OWNER_ID, TRANSFORM_SCHEMA_PROVIDER,
    TRANSFORM_SCHEMA_PROVIDER_ID, TransformComponentsPlugin, Visibility,
    register_transform_components,
};
pub use spawn::{
    PreparedScene, SceneEntityRetirementError, SceneEntitySource, SceneProjection,
    SceneSpawnReport, prepare_scene, prepare_scene_replacement, retire_and_despawn_scene_entity,
    retire_and_despawn_scene_instance, spawn_scene,
};
