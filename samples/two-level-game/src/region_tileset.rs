//! Shared atlas and patterned floors for the Side Room and Lookout regions.

use nara::{
    asset::{AssetProductRef, ImportedProductId, StableAssetId},
    core::Color,
    material::SamplerDescriptor,
    prelude::Vec2,
    tilemap::{TileAtlasLayout, TileCell, TileCoord, TileIndex, TileSetDescriptor},
};

pub const TILESET_PATH: &str = "tilesets/region.tileset.ron";
pub const TILE_SIZE: Vec2 = Vec2::new(16.0, 20.0);
pub const SIDE_ROOM_FLOOR: &str = "side-room-floor";
pub const LOOKOUT_FLOOR: &str = "lookout-floor";
pub const SIDE_ROOM_ORIGIN: Vec2 = Vec2::new(-120.0, 48.0);
pub const LOOKOUT_ORIGIN: Vec2 = Vec2::new(-120.0, -48.0);
pub const FLOOR_LAYER: i32 = -1;

#[must_use]
pub fn tileset_product() -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str("d45a34c8-77ef-4a82-943a-63f47e7585d2")
            .expect("valid region TileSet Product identity"),
        ImportedProductId::PRIMARY,
    )
    .expect("non-nil region TileSet Product identity")
}

#[must_use]
pub fn atlas() -> TileAtlasLayout {
    TileAtlasLayout::grid(Vec2::ONE, 2, 2)
}

#[must_use]
pub fn descriptor() -> TileSetDescriptor {
    TileSetDescriptor::new(
        tileset_product(),
        crate::side_room::image_product(),
        atlas(),
    )
    .expect("valid region TileSet descriptor")
    .with_sampler(SamplerDescriptor::NEAREST_CLAMP)
}

#[must_use]
pub fn side_room_color(coord: TileCoord) -> Color {
    if coord.x.rem_euclid(2) == coord.y.rem_euclid(2) {
        Color::rgba(0.20, 0.40, 0.35, 1.0)
    } else {
        Color::rgba(0.30, 0.60, 0.50, 1.0)
    }
}

#[must_use]
pub fn lookout_color(coord: TileCoord) -> Color {
    if coord.x.rem_euclid(2) == coord.y.rem_euclid(2) {
        Color::rgba(0.25, 0.35, 0.60, 1.0)
    } else {
        Color::rgba(0.40, 0.50, 0.75, 1.0)
    }
}

#[must_use]
pub fn side_room_cells() -> [(TileCoord, TileCell); 8] {
    cells(side_room_color)
}

#[must_use]
pub fn lookout_cells() -> [(TileCoord, TileCell); 8] {
    cells(lookout_color)
}

fn cells(color: fn(TileCoord) -> Color) -> [(TileCoord, TileCell); 8] {
    std::array::from_fn(|index| {
        let coord = TileCoord::new(index as i32 / 2 - 2, index as i32 % 2 - 1);
        (
            coord,
            TileCell::new(TileIndex::new((index / 2) as u32)).with_color(color(coord)),
        )
    })
}
