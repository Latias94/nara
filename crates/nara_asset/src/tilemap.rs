//! Shared tile atlas geometry and authored tile set data.

use std::collections::TryReserveError;

use nara_core::{AssetId, UuidIdError, Vec2};

use crate::image::SamplerDescriptor;

mod cooked;
#[cfg(feature = "runtime")]
mod runtime_loader;
#[cfg(feature = "authoring")]
mod source;

#[cfg(feature = "runtime")]
pub use runtime_loader::{TileSetLoader, TileSetLoaderError};

/// Maximum UTF-8 byte length of a `.tileset.ron` document.
pub const MAX_TILESET_SOURCE_BYTES: usize = 8 * 1024 * 1024;

/// Packed cells reserve zero for empty and store `TileIndex + 1` in a `u16`.
pub const MAX_TILESET_TILES: usize = u16::MAX as usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileIndex(pub u32);

impl TileIndex {
    #[must_use]
    pub const fn new(index: u32) -> Self {
        Self(index)
    }

    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileAtlasLayout {
    pub tile_size: Vec2,
    pub columns: u32,
    pub rows: u32,
    pub margin: Vec2,
    pub spacing: Vec2,
}

impl TileAtlasLayout {
    #[must_use]
    pub fn new(tile_size: Vec2, columns: u32, rows: u32) -> Option<Self> {
        let layout = Self {
            tile_size,
            columns,
            rows,
            margin: Vec2::ZERO,
            spacing: Vec2::ZERO,
        };
        layout.is_valid().then_some(layout)
    }

    #[must_use]
    pub fn grid(tile_size: Vec2, columns: u32, rows: u32) -> Self {
        Self::new(tile_size, columns, rows).expect("tile atlas layout must be valid")
    }

    #[must_use]
    pub fn with_margin(mut self, margin: Vec2) -> Self {
        self.margin = margin;
        assert!(self.is_valid(), "tile atlas margin must be valid");
        self
    }

    #[must_use]
    pub fn with_spacing(mut self, spacing: Vec2) -> Self {
        self.spacing = spacing;
        assert!(self.is_valid(), "tile atlas spacing must be valid");
        self
    }

    #[must_use]
    pub fn atlas_size(self) -> Vec2 {
        Vec2::new(
            self.margin
                .x
                .mul_add(2.0, self.tile_size.x * self.columns as f32)
                + self.spacing.x * self.columns.saturating_sub(1) as f32,
            self.margin
                .y
                .mul_add(2.0, self.tile_size.y * self.rows as f32)
                + self.spacing.y * self.rows.saturating_sub(1) as f32,
        )
    }

    #[must_use]
    pub fn normalized_region(self, tile: TileIndex) -> Option<TileAtlasRegion> {
        if !self.is_valid() {
            return None;
        }
        let raw = tile.raw();
        let x = raw % self.columns;
        let y = raw / self.columns;
        if y >= self.rows {
            return None;
        }

        let atlas_size = self.atlas_size();
        let min = Vec2::new(
            self.margin.x + (self.tile_size.x + self.spacing.x) * x as f32,
            self.margin.y + (self.tile_size.y + self.spacing.y) * y as f32,
        ) / atlas_size;
        let size = self.tile_size / atlas_size;
        Some(TileAtlasRegion { min, size })
    }

    #[must_use]
    pub fn contains_tile(self, tile: TileIndex) -> bool {
        self.is_valid() && tile.raw() / self.columns < self.rows
    }

    #[must_use]
    pub fn is_valid(self) -> bool {
        let atlas_size = self.atlas_size();
        self.columns > 0
            && self.rows > 0
            && self.tile_size.is_finite()
            && self.margin.is_finite()
            && self.spacing.is_finite()
            && self.tile_size.x > 0.0
            && self.tile_size.y > 0.0
            && self.margin.x >= 0.0
            && self.margin.y >= 0.0
            && self.spacing.x >= 0.0
            && self.spacing.y >= 0.0
            && atlas_size.is_finite()
            && atlas_size.x > 0.0
            && atlas_size.y > 0.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileAtlasRegion {
    pub min: Vec2,
    pub size: Vec2,
}

/// Collision metadata for one atlas cell.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default, deny_unknown_fields))]
pub struct TileDefinition {
    /// Enables ordinary solid collision when `one_way` is false.
    pub solid: bool,
    /// Selects one-way platform collision regardless of `solid`.
    pub one_way: bool,
    /// Membership bitmask. Zero selects the caller's default for the collision mode.
    /// Callers should use disjoint memberships for solids and one-way platforms.
    pub collision_group: u32,
}

/// An atlas and its tile definitions, with the image identified by its persistent UUID.
#[derive(Debug, Clone, PartialEq)]
pub struct TileSetAsset {
    pub image: AssetId,
    pub layout: TileAtlasLayout,
    pub sampler: SamplerDescriptor,
    /// Definitions occupy a row-major prefix of the atlas and are indexed by `TileIndex`.
    pub tiles: Vec<TileDefinition>,
}

#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum TileSetAssetError {
    #[error("tile atlas dimensions, margin, or spacing are invalid")]
    InvalidLayout,
    #[error("tile set has {count} definitions, exceeding the {MAX_TILESET_TILES}-tile limit")]
    TooManyTiles { count: usize },
    #[error("tile set has {count} definitions for an atlas with {capacity} cells")]
    TilesExceedAtlas { count: usize, capacity: u64 },
    #[error("invalid tile set image ID: {0}")]
    InvalidImageId(#[from] UuidIdError),
    #[error("invalid cooked tile set: {0}")]
    InvalidCooked(&'static str),
    #[error("unsupported cooked tile set version {0}")]
    UnsupportedVersion(u16),
    #[error("could not allocate cooked tile set data: {0}")]
    Capacity(#[from] TryReserveError),
    #[cfg(feature = "authoring")]
    #[error("tile set source exceeds the {MAX_TILESET_SOURCE_BYTES}-byte limit")]
    SourceTooLarge,
    #[cfg(feature = "authoring")]
    #[error("invalid tile set source: {0}")]
    Ron(#[from] ron::error::SpannedError),
}

fn validate_layout_and_count(
    layout: TileAtlasLayout,
    count: usize,
) -> Result<(), TileSetAssetError> {
    // Region calculation uses the stride even at index zero.
    if !layout.is_valid() || !(layout.tile_size + layout.spacing).is_finite() {
        return Err(TileSetAssetError::InvalidLayout);
    }
    if count > MAX_TILESET_TILES {
        return Err(TileSetAssetError::TooManyTiles { count });
    }
    let capacity = u64::from(layout.columns) * u64::from(layout.rows);
    if count as u64 > capacity {
        return Err(TileSetAssetError::TilesExceedAtlas { count, capacity });
    }
    Ok(())
}
