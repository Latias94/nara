//! Code-first tilemaps and atlas layouts.

#[cfg(feature = "tilemap-source-hydration")]
mod hydration;

// Bevy derives resolve this path; Nara's ECS facade remains the actual dependency.
use std::collections::BTreeMap;

use crate::image::ImageAsset;
use crate::material::{AlphaMode2d, SamplerDescriptor};
use nara_asset::Handle;
use nara_core::{Color, Vec2};
use nara_ecs::Component;

#[cfg(feature = "tilemap-source-hydration")]
pub use hydration::TilemapSourceHydrationPlugin;

pub use nara_asset::tilemap::{
    MAX_TILESET_SOURCE_BYTES, MAX_TILESET_TILES, TileAtlasLayout, TileAtlasRegion, TileDefinition,
    TileIndex, TileSetAsset, TileSetAssetError, TileSetLoader, TileSetLoaderError,
};

pub const DEFAULT_TILE_SIZE: Vec2 = Vec2::new(16.0, 16.0);
pub const DEFAULT_CHUNK_SIZE: i32 = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileCoord {
    pub x: i32,
    pub y: i32,
}

impl TileCoord {
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn chunk(self) -> TileChunkCoord {
        self.chunk_with_size(DEFAULT_CHUNK_SIZE)
    }

    #[must_use]
    pub fn chunk_with_size(self, chunk_size: i32) -> TileChunkCoord {
        TileChunkCoord::from_tile_coord(self, chunk_size)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileChunkCoord {
    pub x: i32,
    pub y: i32,
}

impl TileChunkCoord {
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }

    #[must_use]
    pub fn from_tile_coord(coord: TileCoord, chunk_size: i32) -> Self {
        assert!(chunk_size > 0, "tile chunk size must be positive");
        Self {
            x: coord.x.div_euclid(chunk_size),
            y: coord.y.div_euclid(chunk_size),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileCell {
    pub tile: TileIndex,
    pub color: Color,
}

impl TileCell {
    #[must_use]
    pub const fn new(tile: TileIndex) -> Self {
        Self {
            tile,
            color: Color::WHITE,
        }
    }

    #[must_use]
    pub const fn with_color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Component)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileLayer {
    pub index: i32,
}

impl TileLayer {
    #[must_use]
    pub const fn new(index: i32) -> Self {
        Self { index }
    }
}

impl Default for TileLayer {
    fn default() -> Self {
        Self::new(0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TileSet {
    tile_size: Vec2,
    material: TileSetMaterial,
    atlas: Option<TileAtlasLayout>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TileSetMaterial {
    pub image: Option<Handle<ImageAsset>>,
    pub sampler: SamplerDescriptor,
    pub alpha_mode: AlphaMode2d,
    pub tint: Color,
}

impl TileSetMaterial {
    #[must_use]
    pub fn from_color(tint: Color) -> Self {
        Self {
            image: None,
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::Blend,
            tint,
        }
    }

    #[must_use]
    pub fn from_image(image: Handle<ImageAsset>) -> Self {
        Self {
            image: Some(image),
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::Blend,
            tint: Color::WHITE,
        }
    }

    #[must_use]
    pub const fn with_sampler(mut self, sampler: SamplerDescriptor) -> Self {
        self.sampler = sampler;
        self
    }

    #[must_use]
    pub const fn with_alpha_mode(mut self, alpha_mode: AlphaMode2d) -> Self {
        self.alpha_mode = alpha_mode;
        self
    }

    #[must_use]
    pub const fn with_tint(mut self, tint: Color) -> Self {
        self.tint = tint;
        self
    }
}

impl Default for TileSetMaterial {
    fn default() -> Self {
        Self::from_color(Color::WHITE)
    }
}

impl TileSet {
    #[must_use]
    pub fn new(tile_size: Vec2) -> Self {
        Self {
            tile_size,
            material: TileSetMaterial::default(),
            atlas: None,
        }
    }

    #[must_use]
    pub fn from_image(image: Handle<ImageAsset>, atlas: TileAtlasLayout) -> Self {
        Self {
            tile_size: atlas.tile_size,
            material: TileSetMaterial::from_image(image),
            atlas: Some(atlas),
        }
    }

    #[must_use]
    pub fn with_image(mut self, image: Handle<ImageAsset>, atlas: TileAtlasLayout) -> Self {
        self.tile_size = atlas.tile_size;
        self.material.image = Some(image);
        self.atlas = Some(atlas);
        self
    }

    #[must_use]
    pub fn with_sampler(mut self, sampler: SamplerDescriptor) -> Self {
        self.material.sampler = sampler;
        self
    }

    #[must_use]
    pub fn with_alpha_mode(mut self, alpha_mode: AlphaMode2d) -> Self {
        self.material.alpha_mode = alpha_mode;
        self
    }

    #[must_use]
    pub fn with_tint(mut self, tint: Color) -> Self {
        self.material.tint = tint;
        self
    }

    #[must_use]
    pub fn normalized_region(&self, tile: TileIndex) -> Option<TileAtlasRegion> {
        let _image = self.material.image?;
        self.atlas?.normalized_region(tile)
    }

    #[must_use]
    pub const fn tile_size(&self) -> Vec2 {
        self.tile_size
    }

    #[must_use]
    pub const fn material(&self) -> TileSetMaterial {
        self.material
    }

    #[must_use]
    pub const fn atlas(&self) -> Option<TileAtlasLayout> {
        self.atlas
    }
}

impl Default for TileSet {
    fn default() -> Self {
        Self::new(DEFAULT_TILE_SIZE)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DirtyTileChunk {
    pub coord: TileChunkCoord,
    pub revision: u64,
}

#[derive(Debug, Default, Clone)]
struct TileChunkState {
    revision: u64,
    cell_count: usize,
    dirty: bool,
}

#[derive(Debug, Clone, Component)]
pub struct Tilemap {
    pub tileset: Option<Handle<TileSet>>,
    #[cfg(feature = "tilemap-source-hydration")]
    authored_tileset: Option<TileSet>,
    pub tile_size: Vec2,
    pub layer: TileLayer,
    /// Explicit sort group within a layer. Y sorting never crosses this key.
    pub sort_key: i32,
    /// Local offset from each cell center used for descending world-Y order.
    /// `None` participates at world Y = 0 and keeps authored order with equal anchors.
    pub y_sort_anchor: Option<Vec2>,
    cells: BTreeMap<TileCoord, TileCell>,
    chunks: BTreeMap<TileChunkCoord, TileChunkState>,
    next_dirty_revision: u64,
}

impl PartialEq for Tilemap {
    fn eq(&self, other: &Self) -> bool {
        self.tileset == other.tileset
            && self.authored_tileset() == other.authored_tileset()
            && self.tile_size == other.tile_size
            && self.layer == other.layer
            && self.sort_key == other.sort_key
            && self.y_sort_anchor == other.y_sort_anchor
            && self.cells == other.cells
            && self.next_dirty_revision == other.next_dirty_revision
            && self.dirty_chunks().eq(other.dirty_chunks())
    }
}

impl Tilemap {
    #[must_use]
    pub fn new(tile_size: Vec2) -> Self {
        Self {
            tileset: None,
            #[cfg(feature = "tilemap-source-hydration")]
            authored_tileset: None,
            tile_size,
            layer: TileLayer::default(),
            sort_key: 0,
            y_sort_anchor: None,
            cells: BTreeMap::new(),
            chunks: BTreeMap::new(),
            next_dirty_revision: 1,
        }
    }

    #[must_use]
    pub fn with_tileset(mut self, tileset: Handle<TileSet>) -> Self {
        self.tileset = Some(tileset);
        self
    }

    pub(crate) fn authored_tileset(&self) -> Option<&TileSet> {
        #[cfg(feature = "tilemap-source-hydration")]
        return self.authored_tileset.as_ref();
        #[cfg(not(feature = "tilemap-source-hydration"))]
        None
    }

    #[cfg(feature = "tilemap-source-hydration")]
    pub(super) fn set_authored_tileset(&mut self, tileset: TileSet) {
        self.tileset = None;
        self.authored_tileset = Some(tileset);
    }

    #[must_use]
    pub fn with_layer(mut self, layer: i32) -> Self {
        self.layer = TileLayer::new(layer);
        self
    }

    #[must_use]
    pub fn with_sort_key(mut self, sort_key: i32) -> Self {
        self.sort_key = sort_key;
        self
    }

    #[must_use]
    pub fn with_y_sort_anchor(mut self, anchor: Vec2) -> Self {
        self.y_sort_anchor = Some(anchor);
        self
    }

    pub fn set_cell(&mut self, coord: TileCoord, cell: TileCell) -> Option<TileCell> {
        let previous = self.cells.insert(coord, cell);
        let chunk = self.mark_dirty(coord.chunk());
        chunk.cell_count += usize::from(previous.is_none());
        previous
    }

    pub fn remove_cell(&mut self, coord: TileCoord) -> Option<TileCell> {
        let previous = self.cells.remove(&coord);
        if previous.is_some() {
            self.mark_dirty(coord.chunk()).cell_count -= 1;
        }
        previous
    }

    #[must_use]
    pub fn get_cell(&self, coord: TileCoord) -> Option<&TileCell> {
        self.cells.get(&coord)
    }

    pub fn cells(&self) -> impl Iterator<Item = (TileCoord, &TileCell)> + '_ {
        self.cells.iter().map(|(coord, cell)| (*coord, cell))
    }

    #[must_use]
    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    pub fn dirty_chunks(&self) -> impl Iterator<Item = DirtyTileChunk> + '_ {
        self.chunks
            .iter()
            .filter(|(_, chunk)| chunk.dirty)
            .map(|(&coord, chunk)| DirtyTileChunk {
                coord,
                revision: chunk.revision,
            })
    }

    /// Returns the latest revision of every occupied chunk, even after its dirty flag is cleared.
    pub fn chunk_revisions(&self) -> impl Iterator<Item = DirtyTileChunk> + '_ {
        self.chunks
            .iter()
            .filter(|(_, chunk)| chunk.cell_count != 0)
            .map(|(&coord, chunk)| DirtyTileChunk {
                coord,
                revision: chunk.revision,
            })
    }

    pub fn clear_dirty_chunks(&mut self) {
        self.chunks.retain(|_, chunk| {
            chunk.dirty = false;
            chunk.cell_count != 0
        });
    }

    fn mark_dirty(&mut self, coord: TileChunkCoord) -> &mut TileChunkState {
        let revision = self.next_dirty_revision;
        self.next_dirty_revision = self.next_dirty_revision.saturating_add(1);
        let chunk = self.chunks.entry(coord).or_default();
        chunk.revision = revision;
        chunk.dirty = true;
        chunk
    }
}

impl Default for Tilemap {
    fn default() -> Self {
        Self::new(DEFAULT_TILE_SIZE)
    }
}
