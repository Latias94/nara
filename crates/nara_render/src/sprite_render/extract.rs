use std::{cmp::Reverse, sync::Arc};

use crate::image::{ImageAsset, ImageContentSnapshot, PreparedImageSnapshot};
use crate::material::{Material2d, Material2dAsset, Material2dBinding};
use crate::sprite::Sprite;
use crate::tilemap::{TileCoord, TileIndex, TileSet, Tilemap};
use nara_asset::{AssetRef, AssetServer, AssetStates, Assets, Handle};
use nara_core::{Affine2, Color, Vec2};
use nara_ecs::bevy_ecs::{
    entity::EntityHashSet, entity_disabling::Disabled, error::BevyError, query::Allow,
};
use nara_ecs::{Entity, Or, Query, Res, ResMut, Resource, With};
use nara_scene::spatial::{ChildOf, Children, GlobalTransform2d, InheritedVisibility};

use crate::sprite_render::{
    ExtractedSprite, ExtractedSpriteKind, ExtractedSpriteMaterial, ExtractedSprites,
    PreparedMaterial2d, SpriteRenderStats, TextureUvRect,
};

#[derive(Debug, Default, Resource)]
pub(crate) struct ExtractedSpriteScratch {
    sprites: ExtractedSprites,
    stats: SpriteRenderStats,
    branches: EntityHashSet,
    pending: Vec<Entity>,
}

type SpriteSources<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        Option<&'static Sprite>,
        Option<&'static Tilemap>,
        Option<&'static Material2dBinding>,
        Option<&'static Material2d>,
        Option<&'static GlobalTransform2d>,
        Option<&'static InheritedVisibility>,
    ),
    Or<(With<Sprite>, With<Tilemap>)>,
>;

type SourceHierarchy<'w, 's> =
    Query<'w, 's, (Option<&'static ChildOf>, Option<&'static Children>), Allow<Disabled>>;

#[derive(Debug, thiserror::Error)]
#[error("sprite or tilemap entity {0:?} has no GlobalTransform2d")]
struct MissingGlobalTransform(Entity);

pub(crate) fn extract_sprites(
    mut extracted: ResMut<ExtractedSprites>,
    mut stats: ResMut<SpriteRenderStats>,
    mut scratch: ResMut<ExtractedSpriteScratch>,
    sources: SpriteSources,
    hierarchy: SourceHierarchy,
    materials: Option<Res<Assets<Material2dAsset>>>,
    tilesets: Option<Res<Assets<TileSet>>>,
    server: Option<Res<AssetServer>>,
    states: Option<Res<AssetStates>>,
    images: Option<Res<Assets<ImageAsset>>>,
) -> Result<(), BevyError> {
    let scratch = &mut *scratch;
    scratch.sprites.clear();
    scratch.stats = SpriteRenderStats::default();
    scratch.branches.clear();
    scratch.pending.clear();

    for (mut entity, _, _, _, _, global, visibility) in &sources {
        if visibility.is_some_and(|visibility| !visibility.get()) {
            continue;
        }
        if global.is_none() {
            return Err(BevyError::error(MissingGlobalTransform(entity)));
        }
        while scratch.branches.insert(entity) {
            let parent = hierarchy
                .get(entity)
                .ok()
                .and_then(|(parent, _)| parent)
                .map(ChildOf::parent)
                .filter(|parent| hierarchy.contains(*parent));
            let Some(parent) = parent else {
                scratch.pending.push(entity);
                break;
            };
            entity = parent;
        }
    }
    scratch
        .pending
        .sort_unstable_by_key(|entity| Reverse(entity.index_u32()));

    while !scratch.branches.is_empty() {
        if scratch.pending.is_empty() {
            scratch.pending.extend(scratch.branches.iter().copied());
            scratch
                .pending
                .sort_unstable_by_key(|entity| Reverse(entity.index_u32()));
        }
        let Some(entity) = scratch.pending.pop() else {
            break;
        };
        if !scratch.branches.remove(&entity) {
            continue;
        }
        if let Ok((_, Some(children))) = hierarchy.get(entity) {
            scratch.pending.extend(
                children
                    .iter()
                    .rev()
                    .copied()
                    .filter(|child| scratch.branches.contains(child)),
            );
        }
        let Ok((_, sprite, tilemap, binding, authored, Some(global), visibility)) =
            sources.get(entity)
        else {
            continue;
        };
        if visibility.is_some_and(|visibility| !visibility.get()) {
            continue;
        }
        let material = match extract_material(
            binding,
            authored,
            materials.as_deref(),
            server.as_deref(),
            states.as_deref(),
            images.as_deref(),
        ) {
            Ok(material) => material,
            Err(MaterialExtractionError::Material) => {
                scratch.stats.missing_materials = scratch.stats.missing_materials.saturating_add(1);
                continue;
            }
            Err(MaterialExtractionError::Texture) => {
                scratch.stats.unprepared_textures =
                    scratch.stats.unprepared_textures.saturating_add(1);
                continue;
            }
        };
        if let Some(sprite) = sprite {
            let source_order = scratch.sprites.len() as u64;
            let mut extracted = extract_sprite(entity, sprite, global, source_order);
            extracted.material_extension = material.clone();
            scratch.sprites.push(extracted);
            scratch.stats.extracted_sprites = scratch.stats.extracted_sprites.saturating_add(1);
        }

        let Some(tilemap) = tilemap else {
            continue;
        };
        let tileset = tilemap.tileset.and_then(|handle| {
            extract_tileset(
                handle,
                tilesets.as_deref()?,
                server.as_deref(),
                states.as_deref(),
                images.as_deref(),
            )
        });
        if tilemap.tileset.is_some() && tileset.is_none() {
            scratch.stats.missing_tilesets = scratch.stats.missing_tilesets.saturating_add(1);
            continue;
        }
        scratch
            .sprites
            .add_tile_chunks(entity, tilemap.chunk_revisions());
        let mut context = TilemapExtractionContext::new(entity, tilemap, tileset, global);
        for (coord, cell) in tilemap.cells() {
            let source_order = scratch.sprites.len() as u64;
            let Some(mut extracted_cell) =
                context.extract_cell(coord, cell.tile, cell.color, source_order)
            else {
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

    std::mem::swap(&mut *extracted, &mut scratch.sprites);
    scratch.sprites.clear();
    *stats = scratch.stats;
    Ok(())
}

fn extract_tileset<'a>(
    handle: Handle<TileSet>,
    tilesets: &'a Assets<TileSet>,
    server: Option<&AssetServer>,
    states: Option<&AssetStates>,
    images: Option<&Assets<ImageAsset>>,
) -> Option<&'a TileSet> {
    let tileset = tilesets.get(handle)?;
    let Some(image) = tileset.image_content() else {
        return Some(tileset);
    };
    let states = states?;
    states.state(handle.id())?.usable_version()?;
    if states.published_slot_revision(handle.id()) != Some(&tilesets.slot_revision(handle)) {
        return None;
    }
    image
        .is_authorized(server?, states, images?)
        .then_some(tileset)
}

enum MaterialExtractionError {
    Material,
    Texture,
}

fn extract_material(
    binding: Option<&Material2dBinding>,
    authored: Option<&Material2d>,
    materials: Option<&Assets<Material2dAsset>>,
    server: Option<&AssetServer>,
    states: Option<&AssetStates>,
    images: Option<&Assets<ImageAsset>>,
) -> Result<Option<Arc<PreparedMaterial2d>>, MaterialExtractionError> {
    match (binding, authored) {
        (Some(_), Some(_)) => Err(MaterialExtractionError::Material),
        (Some(binding), None) => capture_material(binding.clone(), |handle| {
            PreparedImageSnapshot::capture(handle, images?, states?)
        })
        .map(Some)
        .map_err(|()| MaterialExtractionError::Texture),
        (None, None) => Ok(None),
        (None, Some(authored)) => {
            let materials = materials.ok_or(MaterialExtractionError::Material)?;
            let handle = authored.handle();
            let asset = materials
                .get(handle)
                .ok_or(MaterialExtractionError::Material)?;
            let server = server.ok_or(MaterialExtractionError::Material)?;
            if server.get_product_handle::<Material2dAsset>(asset.product()) != Some(handle) {
                return Err(MaterialExtractionError::Material);
            }
            let states = states.ok_or(MaterialExtractionError::Material)?;
            if states
                .state(handle.id())
                .is_none_or(|state| state.usable_version().is_none())
                || states.published_slot_revision(handle.id())
                    != Some(&materials.slot_revision(handle))
            {
                return Err(MaterialExtractionError::Material);
            }
            if asset
                .textures()
                .values()
                .any(|reference| !matches!(reference, AssetRef::Product(_)))
            {
                return Err(MaterialExtractionError::Material);
            }
            let binding = authored
                .bind(asset, |reference| match reference {
                    AssetRef::Product(product) => server.get_product_handle::<ImageAsset>(*product),
                    AssetRef::Path(_) | AssetRef::StableId(_) => None,
                })
                .map_err(|_| MaterialExtractionError::Material)?;
            capture_material(binding, |handle| {
                ImageContentSnapshot::capture(handle, server, states, images?)
                    .ok()
                    .map(|image| image.prepared_snapshot())
            })
            .map(Some)
            .map_err(|()| MaterialExtractionError::Material)
        }
    }
}

fn capture_material(
    binding: Material2dBinding,
    mut capture: impl FnMut(Handle<ImageAsset>) -> Option<PreparedImageSnapshot>,
) -> Result<Arc<PreparedMaterial2d>, ()> {
    let declaration = binding.generation().declaration();
    let mut textures = Vec::with_capacity(declaration.textures().len());
    for field in declaration.textures() {
        let handle = *binding.textures().get(field.id()).ok_or(())?;
        textures.push((field.binding(), capture(handle).ok_or(())?));
    }
    textures.sort_unstable_by_key(|(binding, _)| *binding);
    Ok(Arc::new(PreparedMaterial2d { binding, textures }))
}

#[must_use]
fn extract_sprite(
    entity: Entity,
    sprite: &Sprite,
    global: &GlobalTransform2d,
    source_order: u64,
) -> ExtractedSprite {
    let affine = global.affine();
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
        world_center: affine.transform_point2(local_center),
        world_x_axis: affine.transform_vector2(local_x_axis),
        world_y_axis: affine.transform_vector2(local_y_axis),
        color: sprite.material.tint,
        layer: sprite.layer,
        world_z: global.z(),
        sort_key: sprite.sort_key,
        world_sort_y: sprite
            .y_sort_anchor
            .map_or(0.0, |anchor| affine.transform_point2(anchor).y),
    }
}

struct TilemapExtractionContext<'a> {
    entity: Entity,
    tilemap: &'a Tilemap,
    tileset: Option<&'a TileSet>,
    affine: Affine2,
    world_z: f32,
    material: ExtractedSpriteMaterial,
    image_content: Option<&'a ImageContentSnapshot>,
    world_x_axis: Vec2,
    world_y_axis: Vec2,
    cached_region: Option<(TileIndex, Option<TextureUvRect>)>,
}

impl<'a> TilemapExtractionContext<'a> {
    fn new(
        entity: Entity,
        tilemap: &'a Tilemap,
        tileset: Option<&'a TileSet>,
        global: &GlobalTransform2d,
    ) -> Self {
        let material = tileset
            .map(|tileset| {
                let material = tileset.material();
                ExtractedSpriteMaterial {
                    image: material.image,
                    sampler: material.sampler,
                    alpha_mode: material.alpha_mode,
                    tint: material.tint,
                }
            })
            .unwrap_or_else(|| ExtractedSpriteMaterial::from_color(Color::WHITE));
        let tile_size = tilemap.tile_size;
        let local_x_axis = Vec2::new(tile_size.x * 0.5, 0.0);
        let local_y_axis = Vec2::new(0.0, tile_size.y * 0.5);
        let affine = global.affine();

        Self {
            entity,
            tilemap,
            tileset,
            affine,
            world_z: global.z(),
            material,
            image_content: tileset.and_then(TileSet::image_content),
            world_x_axis: affine.transform_vector2(local_x_axis),
            world_y_axis: affine.transform_vector2(local_y_axis),
            cached_region: None,
        }
    }

    fn extract_cell(
        &mut self,
        coord: TileCoord,
        tile: TileIndex,
        color: Color,
        source_order: u64,
    ) -> Option<ExtractedSprite> {
        let texture_region = match self.cached_region {
            Some((cached_tile, region)) if cached_tile == tile => region,
            _ => {
                let region = self.tileset.and_then(|tileset| {
                    tileset
                        .normalized_region(tile)
                        .map(|region| TextureUvRect::new(region.min, region.size))
                });
                self.cached_region = Some((tile, region));
                region
            }
        };
        if self.material.image.is_some() && texture_region.is_none() {
            return None;
        }
        let tile_size = self.tilemap.tile_size;
        let local_center = Vec2::new(
            (coord.x as f32 + 0.5) * tile_size.x,
            (coord.y as f32 + 0.5) * tile_size.y,
        );

        Some(ExtractedSprite {
            entity: self.entity,
            source_order,
            kind: ExtractedSpriteKind::TilemapCell { coord, tile },
            material: self.material,
            image_content: self.image_content.cloned(),
            material_extension: None,
            texture_region: texture_region.unwrap_or(TextureUvRect::FULL),
            world_center: self.affine.transform_point2(local_center),
            world_x_axis: self.world_x_axis,
            world_y_axis: self.world_y_axis,
            color: multiply_color(self.material.tint, color),
            layer: self.tilemap.layer.index,
            world_z: self.world_z,
            sort_key: self.tilemap.sort_key,
            world_sort_y: self.tilemap.y_sort_anchor.map_or(0.0, |anchor| {
                self.affine.transform_point2(local_center + anchor).y
            }),
        })
    }
}

fn multiply_color(left: Color, right: Color) -> Color {
    Color::rgba(
        left.r * right.r,
        left.g * right.g,
        left.b * right.b,
        left.a * right.a,
    )
}
