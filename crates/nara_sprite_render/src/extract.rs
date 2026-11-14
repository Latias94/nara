use nara_asset::{AssetRef, AssetServer, AssetStates, Assets};
use nara_core::{Color, Mat3, Vec2};
use nara_ecs::{Entity, Query, Res, ResMut, Resource, bevy_ecs::error::BevyError};
use nara_image::ImageAsset;
use nara_material::{Material2d, Material2dAsset, Material2dBinding};
use nara_sprite::Sprite;
use nara_tilemap::{TileCoord, TileIndex, TileSet, Tilemap};
use nara_transform::PresentedSpatialSnapshot2d;

use crate::{
    ExtractedSprite, ExtractedSpriteKind, ExtractedSpriteMaterial, ExtractedSprites,
    SpriteRenderStats, TextureUvRect,
};

#[derive(Debug, Default, Resource)]
pub(crate) struct ExtractedSpriteScratch {
    sprites: Vec<ExtractedSprite>,
    stats: SpriteRenderStats,
}

pub(crate) fn extract_sprites(
    mut extracted: ResMut<ExtractedSprites>,
    mut stats: ResMut<SpriteRenderStats>,
    mut scratch: ResMut<ExtractedSpriteScratch>,
    spatial: PresentedSpatialSnapshot2d,
    sprites: Query<(
        Entity,
        &Sprite,
        Option<&Material2dBinding>,
        Option<&Material2d>,
    )>,
    tilemaps: Query<(
        Entity,
        &Tilemap,
        Option<&Material2dBinding>,
        Option<&Material2d>,
    )>,
    materials: Option<Res<Assets<Material2dAsset>>>,
    tilesets: Option<Res<Assets<TileSet>>>,
    server: Option<Res<AssetServer>>,
    states: Option<Res<AssetStates>>,
    images: Option<Res<Assets<ImageAsset>>>,
) -> Result<(), BevyError> {
    let hierarchy_order = spatial.order().map_err(BevyError::error)?;
    for (entity, ..) in sprites.iter() {
        spatial.structural_index(entity).map_err(BevyError::error)?;
        spatial.global(entity).map_err(BevyError::error)?;
    }
    for (entity, ..) in tilemaps.iter() {
        spatial.structural_index(entity).map_err(BevyError::error)?;
        spatial.global(entity).map_err(BevyError::error)?;
    }
    scratch.sprites.clear();
    scratch.stats = SpriteRenderStats::default();

    for entity in hierarchy_order.iter().copied() {
        let sprite = sprites.get(entity).ok();
        let tilemap = tilemaps.get(entity).ok();
        let Some((binding, authored)) = sprite
            .map(|(_, _, binding, authored)| (binding, authored))
            .or_else(|| tilemap.map(|(_, _, binding, authored)| (binding, authored)))
        else {
            continue;
        };
        let material = match extract_material(
            binding,
            authored,
            materials.as_deref(),
            server.as_deref(),
            states.as_deref(),
            images.as_deref(),
        ) {
            Ok(material) => material,
            Err(()) => {
                scratch.stats.missing_materials = scratch.stats.missing_materials.saturating_add(1);
                continue;
            }
        };
        let matrix = spatial.global(entity).map_err(BevyError::error)?.matrix();
        if let Some((entity, sprite, ..)) = sprite {
            let source_order = scratch.sprites.len() as u64;
            let mut extracted = extract_sprite(entity, sprite, matrix, source_order);
            extracted.material_extension = material.clone();
            scratch.sprites.push(extracted);
            scratch.stats.extracted_sprites = scratch.stats.extracted_sprites.saturating_add(1);
        }

        let Some((entity, tilemap, ..)) = tilemap else {
            continue;
        };
        let tileset = tilemap.tileset.and_then(|handle| {
            tilesets
                .as_deref()
                .and_then(|tilesets| tilesets.get(handle))
        });
        let missing_coherent_content = tileset.is_some_and(|tileset| {
            let Some(image) = tileset.image_content() else {
                return false;
            };
            match (server.as_deref(), states.as_deref(), images.as_deref()) {
                (Some(server), Some(states), Some(images)) => {
                    !image.is_authorized(server, states, images)
                }
                _ => true,
            }
        });
        if missing_coherent_content {
            scratch.stats.missing_tilesets = scratch.stats.missing_tilesets.saturating_add(1);
            continue;
        }
        let missing_tileset = tilemap.tileset.is_some() && tileset.is_none();
        if missing_tileset {
            scratch.stats.missing_tilesets = scratch.stats.missing_tilesets.saturating_add(1);
            continue;
        }
        for (coord, cell) in tilemap.cells() {
            let source_order = scratch.sprites.len() as u64;
            let Some(mut extracted_cell) = extract_tile_cell(
                entity,
                tilemap,
                tileset,
                matrix,
                coord,
                cell.tile,
                cell.color,
                source_order,
            ) else {
                scratch.stats.invalid_tile_regions =
                    scratch.stats.invalid_tile_regions.saturating_add(1);
                continue;
            };
            extracted_cell.material_extension = material.clone();
            scratch.sprites.push(extracted_cell);
            scratch.stats.extracted_tile_cells =
                scratch.stats.extracted_tile_cells.saturating_add(1);
        }
    }

    extracted.replace(&mut scratch.sprites);
    *stats = scratch.stats;
    Ok(())
}

fn extract_material(
    binding: Option<&Material2dBinding>,
    authored: Option<&Material2d>,
    materials: Option<&Assets<Material2dAsset>>,
    server: Option<&AssetServer>,
    states: Option<&AssetStates>,
    images: Option<&Assets<ImageAsset>>,
) -> Result<Option<Material2dBinding>, ()> {
    match (binding, authored) {
        (Some(_), Some(_)) => Err(()),
        (Some(binding), None) => Ok(Some(binding.clone())),
        (None, None) => Ok(None),
        (None, Some(authored)) => {
            let materials = materials.ok_or(())?;
            let handle = authored.handle();
            let asset = materials.get(handle).ok_or(())?;
            let server = server.ok_or(())?;
            if server.get_product_handle::<Material2dAsset>(asset.product()) != Some(handle) {
                return Err(());
            }
            let states = states.ok_or(())?;
            if states
                .state(handle.id())
                .is_none_or(|state| state.usable_version().is_none())
                || states.published_slot_revision(handle.id())
                    != Some(&materials.slot_revision(handle))
            {
                return Err(());
            }
            if asset
                .textures()
                .values()
                .any(|reference| !matches!(reference, AssetRef::Product(_)))
            {
                return Err(());
            }
            let binding = authored
                .bind(asset, |reference| match reference {
                    AssetRef::Product(product) => server.get_product_handle::<ImageAsset>(*product),
                    AssetRef::Path(_) | AssetRef::StableId(_) => None,
                })
                .map_err(|_| ())?;
            if binding.textures().values().any(|handle| {
                let Some(images) = images else {
                    return true;
                };
                let Some(image) = images.get(*handle) else {
                    return true;
                };
                let Some(product) = image.product() else {
                    return true;
                };
                server.get_product_handle::<ImageAsset>(product) != Some(*handle)
                    || states
                        .state(handle.id())
                        .is_none_or(|state| state.usable_version().is_none())
                    || states.published_slot_revision(handle.id())
                        != Some(&images.slot_revision(*handle))
            }) {
                return Err(());
            }
            Ok(Some(binding))
        }
    }
}

#[must_use]
pub fn extract_sprite(
    entity: Entity,
    sprite: &Sprite,
    matrix: Mat3,
    source_order: u64,
) -> ExtractedSprite {
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
        image_content: None,
        material_extension: None,
        texture_region: sprite
            .texture_region
            .map(TextureUvRect::from_texture_region)
            .unwrap_or(TextureUvRect::FULL),
        world_center: matrix.transform_point2(local_center),
        world_x_axis: matrix.transform_vector2(local_x_axis),
        world_y_axis: matrix.transform_vector2(local_y_axis),
        color: sprite.material.tint,
        layer: sprite.layer,
        sort_key: sprite.sort_key,
        world_sort_y: sprite
            .y_sort_anchor
            .map_or(0.0, |anchor| matrix.transform_point2(anchor).y),
    }
}

#[must_use]
pub fn extract_tile_cell(
    entity: Entity,
    tilemap: &Tilemap,
    tileset: Option<&TileSet>,
    matrix: Mat3,
    coord: TileCoord,
    tile: TileIndex,
    color: Color,
    source_order: u64,
) -> Option<ExtractedSprite> {
    let tile_size = tilemap.tile_size;
    let local_center = Vec2::new(
        (coord.x as f32 + 0.5) * tile_size.x,
        (coord.y as f32 + 0.5) * tile_size.y,
    );
    let local_x_axis = Vec2::new(tile_size.x * 0.5, 0.0);
    let local_y_axis = Vec2::new(0.0, tile_size.y * 0.5);
    let tile_texture = tileset.and_then(|tileset| {
        Some((
            tileset.material().image?,
            tileset
                .normalized_region(tile)
                .map(|region| TextureUvRect::new(region.min, region.size))?,
        ))
    });
    if tileset
        .and_then(|tileset| tileset.material().image)
        .is_some()
        && tile_texture.is_none()
    {
        return None;
    }
    let material = tileset
        .map(|tileset| ExtractedSpriteMaterial {
            image: tileset.material().image,
            sampler: tileset.material().sampler,
            alpha_mode: tileset.material().alpha_mode,
            tint: tileset.material().tint,
        })
        .unwrap_or_else(|| ExtractedSpriteMaterial::from_color(Color::WHITE));

    Some(ExtractedSprite {
        entity,
        source_order,
        kind: ExtractedSpriteKind::TilemapCell { coord, tile },
        material,
        image_content: tileset.and_then(TileSet::image_content).cloned(),
        material_extension: None,
        texture_region: tile_texture
            .map(|(_, texture_region)| texture_region)
            .unwrap_or(TextureUvRect::FULL),
        world_center: matrix.transform_point2(local_center),
        world_x_axis: matrix.transform_vector2(local_x_axis),
        world_y_axis: matrix.transform_vector2(local_y_axis),
        color: multiply_color(material.tint, color),
        layer: tilemap.layer.index,
        sort_key: tilemap.sort_key,
        world_sort_y: tilemap.y_sort_anchor.map_or(0.0, |anchor| {
            matrix.transform_point2(local_center + anchor).y
        }),
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
