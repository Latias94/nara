//! Game-owned atlas identity and clip selection over an admitted immutable animation resource.

use nara::{
    asset::{AssetProductRef, AssetRef, ImportedProductId, StableAssetId},
    core::Vec2,
    prelude::{Component, PersistentComponent},
    project_host::ProjectDomainAssets,
    sprite::TextureRegion,
};
use nara_sprite_animation::SpriteAnimationDomain;
use serde::Serialize;

use crate::resume::GameClock;

/// Assets-relative path to the shared four-frame explorer atlas.
pub const ATLAS_PATH: &str = "textures/explorer.png";
/// Stable source identity declared by the explorer Image metadata.
pub const ATLAS_SOURCE_ID: &str = "2eece9cd-6332-4868-b0d2-289879df9de8";
/// Assets-relative path to the immutable named explorer clips.
pub const CLIPS_PATH: &str = "animations/explorer.sprite-animations.json";
/// Stable source identity declared by the clip descriptor's metadata.
pub const CLIPS_SOURCE_ID: &str = "e4c107a3-2acd-57b5-b394-7d64443ccce6";

/// Authored animation selection; the Sprite independently retains its Image and sampler.
#[derive(Debug, Clone, PartialEq, Component, PersistentComponent)]
#[nara(
    id = "two-level-game.ExplorerAnimation",
    version = 1,
    alias = "Explorer animation",
    component_capabilities(scene, inspect, edit),
    field_capabilities(scene, inspect, edit)
)]
pub struct ExplorerAnimation {
    #[nara(
        id = "clips",
        alias = "Clips",
        capabilities(scene, inspect, edit, asset_ref)
    )]
    pub clips: AssetRef,
}

/// Complete primary clip Product shared by both levels.
#[must_use]
pub fn clips_product() -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(CLIPS_SOURCE_ID).expect("fixed valid explorer clips identity"),
        ImportedProductId::PRIMARY,
    )
    .expect("fixed non-nil primary explorer clips Product")
}

/// Complete primary Image Product shared by both levels.
#[must_use]
pub fn atlas_product() -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(ATLAS_SOURCE_ID).expect("fixed valid explorer atlas identity"),
        ImportedProductId::PRIMARY,
    )
    .expect("fixed non-nil primary explorer atlas Product")
}

/// One of four adjacent 16-by-16 frames in the 64-by-16 explorer atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ExplorerFrame {
    Idle,
    WalkOne,
    WalkTwo,
    WalkThree,
}

impl ExplorerFrame {
    /// Returns the zero-based column in the atlas.
    #[must_use]
    pub const fn index(self) -> u32 {
        match self {
            Self::Idle => 0,
            Self::WalkOne => 1,
            Self::WalkTwo => 2,
            Self::WalkThree => 3,
        }
    }

    /// Returns exact normalized quarter-width UV coordinates for this frame.
    #[must_use]
    pub const fn texture_region(self) -> TextureRegion {
        TextureRegion::new(
            Vec2::new(self.index() as f32 * 0.25, 0.0),
            Vec2::new(0.25, 1.0),
        )
    }
}

fn clip_name(held: [bool; 4]) -> &'static str {
    if held[0] == held[1] && held[2] == held[3] {
        "idle"
    } else {
        "walk"
    }
}

#[derive(Debug, Clone)]
pub(crate) struct PreparedAnimation {
    assets: ProjectDomainAssets,
    reference: AssetRef,
}

impl PreparedAnimation {
    pub(crate) fn acquire(
        assets: &ProjectDomainAssets,
        reference: AssetRef,
    ) -> Result<Self, String> {
        let set = assets
            .collection::<SpriteAnimationDomain>()
            .and_then(|collection| collection.get(&reference))
            .ok_or("the explorer animation resource was not acquired")?;
        if set.product() != clips_product()
            || set.get("idle").is_none()
            || set.get("walk").is_none()
        {
            return Err(
                "the explorer requires its original clip Product with idle and walk clips".into(),
            );
        }
        Ok(Self {
            assets: assets.clone(),
            reference,
        })
    }

    pub(crate) fn sample(&self, clock: GameClock, held: [bool; 4]) -> TextureRegion {
        self.assets
            .collection::<SpriteAnimationDomain>()
            .and_then(|collection| collection.get(&self.reference))
            .and_then(|set| set.get(clip_name(held)))
            .expect("the retained immutable explorer clip was admitted")
            .sample(std::time::Duration::from_nanos(clock.elapsed_nanos))
            .region
    }

    pub(crate) fn matches(&self, component: &ExplorerAnimation) -> bool {
        self.reference == component.clips
    }
}

#[cfg(test)]
mod tests;
