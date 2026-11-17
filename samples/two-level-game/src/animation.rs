//! Game-owned explorer atlas and pure animation derived from logical time and held directions.

use nara::{
    asset::{AssetProductRef, ImportedProductId, StableAssetId},
    core::Vec2,
    sprite::TextureRegion,
};
use serde::Serialize;

use crate::resume::GameClock;

/// Assets-relative path to the shared four-frame explorer atlas.
pub const ATLAS_PATH: &str = "textures/explorer.png";
/// Stable source identity declared by the explorer Image metadata.
pub const ATLAS_SOURCE_ID: &str = "2eece9cd-6332-4868-b0d2-289879df9de8";

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

/// Selects a frame from elapsed logical nanoseconds and held left/right/up/down directions.
///
/// Opposing holds cancel independently on each axis. Any remaining direction walks, including
/// when the player's position is clamped at a wall. Walking cycles through frames 1, 2 and 3
/// every 100 ms on the existing clock; changing holds does not restart its phase.
#[must_use]
pub fn explorer_frame(clock: GameClock, held: [bool; 4]) -> ExplorerFrame {
    if held[0] == held[1] && held[2] == held[3] {
        return ExplorerFrame::Idle;
    }
    match (clock.elapsed_nanos / 100_000_000) % 3 {
        0 => ExplorerFrame::WalkOne,
        1 => ExplorerFrame::WalkTwo,
        _ => ExplorerFrame::WalkThree,
    }
}

#[cfg(test)]
mod tests;
