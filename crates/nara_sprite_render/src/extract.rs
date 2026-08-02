use nara_asset::Assets;
use nara_core::{Color, Vec2};
use nara_ecs::{Entity, Query, Res, ResMut, Resource, error::BevyError};
use nara_render::RenderPhaseLabel;
use nara_sprite::Sprite;
use nara_tilemap::{TileCoord, TileIndex, TileSet, Tilemap};
use nara_transform::{GlobalTransform2d, Transform2d};

use crate::{
    ExtractedSprite, ExtractedSpriteKind, ExtractedSpriteMaterial, ExtractedSprites,
    SpriteRenderStats, TextureUvRect,
};

/// Failure to publish one complete sprite/tilemap world-space extraction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub(crate) enum SpriteExtractionError {
    #[error("the completed 2D transform projection is unavailable")]
    ProjectionIncomplete,
    #[error("sprite entity {entity:?} is missing its authored Transform2d")]
    MissingSpriteLocalTransform { entity: Entity },
    #[error("sprite entity {entity:?} is missing its completed GlobalTransform2d")]
    MissingSpriteGlobalTransform { entity: Entity },
    #[error("tilemap entity {entity:?} is missing its authored Transform2d")]
    MissingTilemapLocalTransform { entity: Entity },
    #[error("tilemap entity {entity:?} is missing its completed GlobalTransform2d")]
    MissingTilemapGlobalTransform { entity: Entity },
}

#[derive(Debug, Default, Resource)]
pub(crate) struct ExtractedSpriteScratch {
    sprites: Vec<ExtractedSprite>,
    stats: SpriteRenderStats,
}

pub(crate) fn extract_sprites(
    mut extracted: ResMut<ExtractedSprites>,
    mut stats: ResMut<SpriteRenderStats>,
    mut scratch: ResMut<ExtractedSpriteScratch>,
    completed_projection: Option<Res<nara_transform::__private::CompletedTransformProjection>>,
    completed_hierarchy: Option<Res<nara_hierarchy::__private::CompletedHierarchyProjection>>,
    sprites: Query<(
        Entity,
        &Sprite,
        Option<&Transform2d>,
        Option<&GlobalTransform2d>,
    )>,
    tilemaps: Query<(
        Entity,
        &Tilemap,
        Option<&Transform2d>,
        Option<&GlobalTransform2d>,
    )>,
    tilesets: Option<Res<Assets<TileSet>>>,
) -> Result<(), BevyError> {
    let completed_projection = completed_projection
        .ok_or(SpriteExtractionError::ProjectionIncomplete)
        .map_err(BevyError::error)?;
    let completed_hierarchy = completed_hierarchy
        .ok_or(SpriteExtractionError::ProjectionIncomplete)
        .map_err(BevyError::error)?;
    if completed_projection.hierarchy_generation() != completed_hierarchy.generation() {
        return Err(BevyError::error(
            SpriteExtractionError::ProjectionIncomplete,
        ));
    }
    scratch.sprites.clear();
    scratch.stats = SpriteRenderStats::default();

    for (entity, sprite, local, global) in sprites.iter() {
        local
            .ok_or(SpriteExtractionError::MissingSpriteLocalTransform { entity })
            .map_err(BevyError::error)?;
        let global = global
            .ok_or(SpriteExtractionError::MissingSpriteGlobalTransform { entity })
            .map_err(BevyError::error)?;
        let source_order = scratch.sprites.len() as u64;
        scratch
            .sprites
            .push(extract_sprite(entity, sprite, global, source_order));
        scratch.stats.extracted_sprites = scratch.stats.extracted_sprites.saturating_add(1);
    }

    for (entity, tilemap, local, global) in tilemaps.iter() {
        local
            .ok_or(SpriteExtractionError::MissingTilemapLocalTransform { entity })
            .map_err(BevyError::error)?;
        let global = global
            .ok_or(SpriteExtractionError::MissingTilemapGlobalTransform { entity })
            .map_err(BevyError::error)?;
        let tileset = tilemap.tileset.and_then(|handle| {
            tilesets
                .as_deref()
                .and_then(|tilesets| tilesets.get(handle))
        });
        let missing_tileset = tilemap.tileset.is_some() && tileset.is_none();
        for (coord, cell) in tilemap.cells() {
            let source_order = scratch.sprites.len() as u64;
            let Some(extracted_cell) = extract_tile_cell(
                entity,
                tilemap,
                tileset,
                global,
                coord,
                cell.tile,
                cell.color,
                source_order,
            ) else {
                scratch.stats.invalid_tile_regions =
                    scratch.stats.invalid_tile_regions.saturating_add(1);
                continue;
            };
            if missing_tileset {
                scratch.stats.missing_tilesets = scratch.stats.missing_tilesets.saturating_add(1);
            }
            scratch.sprites.push(extracted_cell);
            scratch.stats.extracted_tile_cells =
                scratch.stats.extracted_tile_cells.saturating_add(1);
        }
    }

    extracted.replace(&mut scratch.sprites);
    *stats = scratch.stats;
    Ok(())
}

#[must_use]
pub fn extract_sprite(
    entity: Entity,
    sprite: &Sprite,
    global: &GlobalTransform2d,
    source_order: u64,
) -> ExtractedSprite {
    let matrix = global.matrix();
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
    global: &GlobalTransform2d,
    coord: TileCoord,
    tile: TileIndex,
    color: Color,
    source_order: u64,
) -> Option<ExtractedSprite> {
    let matrix = global.matrix();
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
