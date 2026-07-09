use nara_asset::Assets;
use nara_core::{Color, Vec2};
use nara_ecs::{Entity, Query, Res, ResMut};
use nara_render::RenderPhaseLabel;
use nara_sprite::Sprite;
use nara_tilemap::{TileCoord, TileIndex, TileSet, Tilemap};
use nara_transform::Transform2d;

use crate::{
    ExtractedSprite, ExtractedSpriteKind, ExtractedSpriteMaterial, ExtractedSprites,
    SpriteRenderStats, TextureUvRect,
};

pub fn extract_sprites(
    mut extracted: ResMut<ExtractedSprites>,
    mut stats: ResMut<SpriteRenderStats>,
    sprites: Query<(Entity, &Sprite, Option<&Transform2d>)>,
    tilemaps: Query<(Entity, &Tilemap, Option<&Transform2d>)>,
    tilesets: Option<Res<Assets<TileSet>>>,
) {
    extracted.clear();
    *stats = SpriteRenderStats::default();

    for (entity, sprite, transform) in sprites.iter() {
        let source_order = next_source_order(&extracted);
        extracted.push(extract_sprite(entity, sprite, transform, source_order));
        stats.extracted_sprites = stats.extracted_sprites.saturating_add(1);
    }

    for (entity, tilemap, transform) in tilemaps.iter() {
        let tileset = tilemap.tileset.and_then(|handle| {
            tilesets
                .as_deref()
                .and_then(|tilesets| tilesets.get(handle))
        });
        let missing_tileset = tilemap.tileset.is_some() && tileset.is_none();
        for (coord, cell) in tilemap.cells() {
            let source_order = next_source_order(&extracted);
            let Some(extracted_cell) = extract_tile_cell(
                entity,
                tilemap,
                tileset,
                transform,
                coord,
                cell.tile,
                cell.color,
                source_order,
            ) else {
                stats.invalid_tile_regions = stats.invalid_tile_regions.saturating_add(1);
                continue;
            };
            if missing_tileset {
                stats.missing_tilesets = stats.missing_tilesets.saturating_add(1);
            }
            extracted.push(extracted_cell);
            stats.extracted_tile_cells = stats.extracted_tile_cells.saturating_add(1);
        }
    }
}

#[must_use]
pub fn extract_sprite(
    entity: Entity,
    sprite: &Sprite,
    transform: Option<&Transform2d>,
    source_order: u64,
) -> ExtractedSprite {
    let transform = transform.copied().unwrap_or_default();
    let matrix = transform.matrix();
    let local_center = -sprite.anchor.normalized * sprite.size;
    let local_x_axis = Vec2::new(sprite.size.x * 0.5, 0.0);
    let local_y_axis = Vec2::new(0.0, sprite.size.y * 0.5);

    ExtractedSprite {
        entity,
        source_order,
        kind: ExtractedSpriteKind::Sprite,
        material: ExtractedSpriteMaterial {
            image: sprite.material.image,
            sampler: sprite.material.sampler,
            alpha_mode: sprite.material.alpha_mode,
            tint: sprite.material.tint,
        },
        texture_region: sprite
            .texture_region
            .map(TextureUvRect::from_texture_region)
            .unwrap_or(TextureUvRect::FULL),
        world_center: matrix.transform_point2(local_center),
        world_x_axis: matrix.transform_vector2(local_x_axis),
        world_y_axis: matrix.transform_vector2(local_y_axis),
        color: sprite.material.tint,
        phase: RenderPhaseLabel::TRANSPARENT_2D,
        layer: sprite.layer,
        sort_key: sprite.sort_key,
    }
}

#[must_use]
pub fn extract_tile_cell(
    entity: Entity,
    tilemap: &Tilemap,
    tileset: Option<&TileSet>,
    transform: Option<&Transform2d>,
    coord: TileCoord,
    tile: TileIndex,
    color: Color,
    source_order: u64,
) -> Option<ExtractedSprite> {
    let transform = transform.copied().unwrap_or_default();
    let matrix = transform.matrix();
    let tile_size = tilemap.tile_size;
    let local_center = Vec2::new(
        (coord.x as f32 + 0.5) * tile_size.x,
        (coord.y as f32 + 0.5) * tile_size.y,
    );
    let local_x_axis = Vec2::new(tile_size.x * 0.5, 0.0);
    let local_y_axis = Vec2::new(0.0, tile_size.y * 0.5);
    let tile_texture = tileset.and_then(|tileset| {
        Some((
            tileset.material.image?,
            tileset
                .normalized_region(tile)
                .map(|region| TextureUvRect::new(region.min, region.size))?,
        ))
    });
    if tileset.and_then(|tileset| tileset.material.image).is_some() && tile_texture.is_none() {
        return None;
    }
    let material = tileset
        .map(|tileset| ExtractedSpriteMaterial {
            image: tileset.material.image,
            sampler: tileset.material.sampler,
            alpha_mode: tileset.material.alpha_mode,
            tint: tileset.material.tint,
        })
        .unwrap_or_else(|| ExtractedSpriteMaterial::from_color(Color::WHITE));

    Some(ExtractedSprite {
        entity,
        source_order,
        kind: ExtractedSpriteKind::TilemapCell { coord, tile },
        material,
        texture_region: tile_texture
            .map(|(_, texture_region)| texture_region)
            .unwrap_or(TextureUvRect::FULL),
        world_center: matrix.transform_point2(local_center),
        world_x_axis: matrix.transform_vector2(local_x_axis),
        world_y_axis: matrix.transform_vector2(local_y_axis),
        color: multiply_color(material.tint, color),
        phase: RenderPhaseLabel::TILEMAP_2D,
        layer: tilemap.layer.index,
        sort_key: tilemap.sort_key,
    })
}

fn multiply_color(left: Color, right: Color) -> Color {
    Color::rgba(
        left.r * right.r,
        left.g * right.g,
        left.b * right.b,
        left.a * right.a,
    )
}

fn next_source_order(extracted: &ExtractedSprites) -> u64 {
    extracted.len() as u64
}
