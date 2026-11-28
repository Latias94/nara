//! Scene data and 2D spatial propagation over the Bevy hierarchy.

#[cfg(any(feature = "authoring", feature = "cooked"))]
pub mod animation;
#[cfg(feature = "authoring")]
pub mod authoring;
#[cfg(any(feature = "authoring", feature = "cooked"))]
pub mod components;
#[cfg(feature = "cook")]
pub mod cook;
#[cfg(feature = "cooked")]
pub mod cooked;
#[cfg(feature = "runtime")]
mod runtime;
#[cfg(all(feature = "save", any(unix, windows)))]
pub mod save;
pub mod spatial;

#[cfg(feature = "runtime")]
pub use runtime::{
    RegisterSceneComponent, ResolveTilemapAssets, SCENE_PLUGIN_ID, SceneAsset, SceneAssetError,
    SceneAssetLoader, SceneError, SceneEvent, SceneInstanceId, SceneLoadState, SceneManager,
    SceneMember, ScenePlugin, SceneRegistry, SceneUpdateSet, TilemapAsset, switch_scene_asset_sync,
    switch_scene_sync,
};
pub use spatial::{SPATIAL_PLUGIN_ID, SpatialPlugin, SpatialSet, Visibility};

use bevy_ecs::component::Component;

/// A human-readable label for an entity.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Component)]
pub struct Name(pub String);

impl Name {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
