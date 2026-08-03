//! Persistent scene documents, authoring operations, and runtime materialization.

mod authoring;
mod diagnostics;
mod document;
mod export;
#[cfg(feature = "serde")]
mod format;
#[cfg(feature = "serde")]
mod migration;
mod patch;
mod prefab;
mod product_transaction;
mod scene_components;
mod spawn;
mod validation;

#[cfg(test)]
mod product_transaction_tests;
#[cfg(test)]
mod tests;

pub use authoring::{
    SceneAuthoringClearReport, SceneAuthoringHistoryStatus, SceneAuthoringRevision,
    SceneAuthoringSession, SceneAuthoringSourceId, SceneAuthoringSyncReport,
};
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
pub use patch::{ScenePatchApplyLimits, ScenePatchDocument, ScenePatchOperation, ScenePatchReport};
pub use prefab::{
    InMemoryPrefabSourceResolver, PrefabDocument, PrefabExpansionBudgetKind, PrefabExpansionLimits,
    PrefabExpansionOptions, PrefabExpansionReport, PrefabInstance, PrefabInstantiationReport,
    PrefabSourceResolver,
};
pub use product_transaction::{
    SceneProductOverlayWriter, SceneProductResource, SceneProductTransactionLimits,
};
pub use scene_components::{
    Name, SCENE_COMPONENTS_PLUGIN_DECLARATION, SCENE_COMPONENTS_PLUGIN_ID,
    SCENE_COMPONENTS_SCHEMA_OWNER_ID, SCENE_COMPONENTS_SCHEMA_PROVIDER,
    SCENE_COMPONENTS_SCHEMA_PROVIDER_ID, SceneComponentsPlugin, Visibility,
    register_scene_components,
};
pub use spawn::{
    SceneEntityRetirementError, SceneEntitySource, SceneSpawnReport, SceneSpawner,
    replace_scene_with_product, retire_and_despawn_scene_entity, spawn_prefab,
    spawn_prefab_with_asset_database, spawn_prefab_with_patch,
    spawn_prefab_with_patch_and_asset_database, spawn_scene, spawn_scene_with_asset_database,
    spawn_scene_with_prefab_resolver, spawn_scene_with_prefab_resolver_and_asset_database,
};
