//! Tilemap authoring data for 2D scenes.

use std::collections::BTreeMap;

use nara_app::{App, Plugin};
use nara_asset::Handle;
use nara_core::{Color, Vec2};
use nara_ecs::Component;

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

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct TileSet {
    pub tile_size: Vec2,
}

impl TileSet {
    #[must_use]
    pub const fn new(tile_size: Vec2) -> Self {
        Self { tile_size }
    }
}

impl Default for TileSet {
    fn default() -> Self {
        Self::new(DEFAULT_TILE_SIZE)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct DirtyTileChunk {
    pub coord: TileChunkCoord,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Component)]
pub struct Tilemap {
    pub tileset: Option<Handle<TileSet>>,
    pub tile_size: Vec2,
    pub layer: TileLayer,
    pub sort_key: i32,
    cells: BTreeMap<TileCoord, TileCell>,
    dirty_chunks: BTreeMap<TileChunkCoord, u64>,
    next_dirty_revision: u64,
}

impl Tilemap {
    #[must_use]
    pub fn new(tile_size: Vec2) -> Self {
        Self {
            tileset: None,
            tile_size,
            layer: TileLayer::default(),
            sort_key: 0,
            cells: BTreeMap::new(),
            dirty_chunks: BTreeMap::new(),
            next_dirty_revision: 1,
        }
    }

    #[must_use]
    pub fn with_tileset(mut self, tileset: Handle<TileSet>) -> Self {
        self.tileset = Some(tileset);
        self
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

    pub fn set_cell(&mut self, coord: TileCoord, cell: TileCell) -> Option<TileCell> {
        let previous = self.cells.insert(coord, cell);
        self.mark_dirty(coord);
        previous
    }

    pub fn remove_cell(&mut self, coord: TileCoord) -> Option<TileCell> {
        let previous = self.cells.remove(&coord);
        if previous.is_some() {
            self.mark_dirty(coord);
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
        self.dirty_chunks
            .iter()
            .map(|(coord, revision)| DirtyTileChunk {
                coord: *coord,
                revision: *revision,
            })
    }

    pub fn clear_dirty_chunks(&mut self) {
        self.dirty_chunks.clear();
    }

    fn mark_dirty(&mut self, coord: TileCoord) {
        let revision = self.next_dirty_revision;
        self.next_dirty_revision = self.next_dirty_revision.saturating_add(1);
        self.dirty_chunks.insert(coord.chunk(), revision);
    }
}

impl Default for Tilemap {
    fn default() -> Self {
        Self::new(DEFAULT_TILE_SIZE)
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct TilemapPlugin;

impl Plugin for TilemapPlugin {
    fn build(&self, _app: &mut App) {}
}

pub mod prelude {
    pub use crate::{
        DEFAULT_CHUNK_SIZE, DEFAULT_TILE_SIZE, DirtyTileChunk, TileCell, TileChunkCoord, TileCoord,
        TileIndex, TileLayer, TileSet, Tilemap, TilemapPlugin,
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_asset::AssetId;

    #[test]
    fn tile_coordinates_floor_divide_negative_chunks() {
        assert_eq!(TileCoord::new(0, 0).chunk(), TileChunkCoord::new(0, 0));
        assert_eq!(TileCoord::new(31, 31).chunk(), TileChunkCoord::new(0, 0));
        assert_eq!(TileCoord::new(32, 32).chunk(), TileChunkCoord::new(1, 1));
        assert_eq!(TileCoord::new(-1, -1).chunk(), TileChunkCoord::new(-1, -1));
        assert_eq!(
            TileCoord::new(-32, -32).chunk(),
            TileChunkCoord::new(-1, -1)
        );
        assert_eq!(
            TileCoord::new(-33, -33).chunk(),
            TileChunkCoord::new(-2, -2)
        );
    }

    #[test]
    fn setting_same_coordinate_replaces_cell_and_marks_chunk_dirty() {
        let mut tilemap = Tilemap::default();
        let coord = TileCoord::new(2, 3);
        let first = TileCell::new(TileIndex::new(1));
        let second = TileCell::new(TileIndex::new(2));

        assert_eq!(tilemap.set_cell(coord, first), None);
        assert_eq!(tilemap.set_cell(coord, second), Some(first));

        assert_eq!(tilemap.get_cell(coord), Some(&second));
        assert_eq!(
            tilemap.dirty_chunks().collect::<Vec<_>>(),
            vec![DirtyTileChunk {
                coord: TileChunkCoord::new(0, 0),
                revision: 2,
            }]
        );
    }

    #[test]
    fn removing_cell_marks_affected_chunk_dirty() {
        let mut tilemap = Tilemap::default();
        let coord = TileCoord::new(-1, 0);
        let cell = TileCell::new(TileIndex::new(7));

        tilemap.set_cell(coord, cell);
        tilemap.clear_dirty_chunks();

        assert_eq!(tilemap.remove_cell(coord), Some(cell));
        assert!(tilemap.get_cell(coord).is_none());
        assert_eq!(
            tilemap.dirty_chunks().collect::<Vec<_>>(),
            vec![DirtyTileChunk {
                coord: TileChunkCoord::new(-1, 0),
                revision: 2,
            }]
        );
    }

    #[test]
    fn empty_tilemaps_are_allowed() {
        let tilemap = Tilemap::default();

        assert!(tilemap.is_empty());
        assert_eq!(tilemap.cell_count(), 0);
        assert_eq!(tilemap.cells().count(), 0);
        assert_eq!(tilemap.dirty_chunks().count(), 0);
    }

    #[test]
    fn iteration_order_is_deterministic() {
        let mut tilemap = Tilemap::default();
        tilemap.set_cell(TileCoord::new(2, 0), TileCell::new(TileIndex::new(2)));
        tilemap.set_cell(TileCoord::new(-1, 3), TileCell::new(TileIndex::new(1)));
        tilemap.set_cell(TileCoord::new(0, 0), TileCell::new(TileIndex::new(0)));

        let coords = tilemap.cells().map(|(coord, _)| coord).collect::<Vec<_>>();

        assert_eq!(
            coords,
            vec![
                TileCoord::new(-1, 3),
                TileCoord::new(0, 0),
                TileCoord::new(2, 0),
            ]
        );
    }

    #[test]
    fn records_tileset_layer_and_sort_controls() {
        let tileset = Handle::new(AssetId::from_raw(11));
        let tilemap = Tilemap::new(Vec2::new(8.0, 8.0))
            .with_tileset(tileset)
            .with_layer(4)
            .with_sort_key(-9);

        assert_eq!(tilemap.tileset, Some(tileset));
        assert_eq!(tilemap.tile_size, Vec2::new(8.0, 8.0));
        assert_eq!(tilemap.layer, TileLayer::new(4));
        assert_eq!(tilemap.sort_key, -9);
    }
}
