//! Scene runtime hierarchy components and persistent scene document data.

mod document;
mod export;
mod format;
mod hierarchy;
mod patch;
mod prefab;
mod spawn;
mod validation;

#[cfg(test)]
mod tests;

pub use document::{
    SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityIdError, SceneEntityRecord,
};
pub use export::{SceneExportOptions, SceneExportReport, export_scene, export_scene_with_options};
pub use format::SceneFormatError;
pub use hierarchy::{
    Children, HierarchyPlugin, Name, Parent, Transform2d, Visibility, register_scene_components,
    spawn_child, sync_children,
};
pub use patch::{ScenePatchDocument, ScenePatchOperation, ScenePatchReport};
pub use prefab::{PrefabDocument, PrefabInstance, PrefabInstantiationReport};
pub use spawn::{
    SceneEntityMap, SceneEntitySource, SceneInstanceId, SceneSpawnReport, SceneSpawner,
    spawn_prefab, spawn_prefab_with_asset_database, spawn_prefab_with_patch,
    spawn_prefab_with_patch_and_asset_database, spawn_scene, spawn_scene_with_asset_database,
};
