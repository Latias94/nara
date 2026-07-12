//! Scene runtime hierarchy components and persistent scene document data.

mod authoring;
mod diagnostics;
mod document;
mod export;
#[cfg(feature = "serde")]
mod format;
mod hierarchy;
#[cfg(feature = "serde")]
mod migration;
mod patch;
mod prefab;
mod spawn;
mod validation;

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
    PrefabDocumentCandidate, SceneDocumentCandidate, SceneFileBudgetError, SceneFileBudgetKind,
    SceneFileEncoding, SceneFileLimits, SceneFilePublicationError, SceneFormatError,
    ScenePatchDocumentCandidate,
};
pub use hierarchy::{
    Children, HierarchyPlugin, Name, Parent, Transform2d, Visibility, register_scene_components,
    spawn_child, sync_children,
};
pub use nara_identity::{SceneEntityId, SceneEntityIdError, SceneInstanceId, SpawnedSceneInstance};
pub use patch::{ScenePatchApplyLimits, ScenePatchDocument, ScenePatchOperation, ScenePatchReport};
pub use prefab::{
    InMemoryPrefabSourceResolver, PrefabDocument, PrefabExpansionBudgetKind, PrefabExpansionLimits,
    PrefabExpansionOptions, PrefabExpansionReport, PrefabInstance, PrefabInstantiationReport,
    PrefabSourceResolver,
};
pub use spawn::{
    SceneEntitySource, SceneSpawnReport, SceneSpawner, spawn_prefab,
    spawn_prefab_with_asset_database, spawn_prefab_with_patch,
    spawn_prefab_with_patch_and_asset_database, spawn_scene, spawn_scene_with_asset_database,
    spawn_scene_with_prefab_resolver, spawn_scene_with_prefab_resolver_and_asset_database,
};
