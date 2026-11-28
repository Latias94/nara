use std::{cmp::Reverse, sync::Arc};

use crate::image::{ImageAsset, ImageExtent, PreparedImageSnapshot};
use crate::material::{Material2dBinding, SamplerDescriptor};
use crate::particles::ParticleEmitter2d;
use crate::sprite::{NineSlice, Sprite, SpriteScaleMode};
use crate::tilemap::{TileCoord, TileIndex, TileSet, Tilemap};
use nara_asset::{AssetStates, Assets, Handle};
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
        Option<&'static ParticleEmitter2d>,
        Option<&'static Material2dBinding>,
        Option<&'static GlobalTransform2d>,
        Option<&'static InheritedVisibility>,
    ),
    Or<(With<Sprite>, With<Tilemap>, With<ParticleEmitter2d>)>,
>;

type SourceHierarchy<'w, 's> =
    Query<'w, 's, (Option<&'static ChildOf>, Option<&'static Children>), Allow<Disabled>>;

#[derive(Debug, thiserror::Error)]
#[error("sprite, tilemap, or particle emitter entity {0:?} has no GlobalTransform2d")]
struct MissingGlobalTransform(Entity);

pub(crate) fn extract_sprites(
    mut extracted: ResMut<ExtractedSprites>,
    mut stats: ResMut<SpriteRenderStats>,
    mut scratch: ResMut<ExtractedSpriteScratch>,
    sources: SpriteSources,
    hierarchy: SourceHierarchy,
    tilesets: Option<Res<Assets<TileSet>>>,
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
        let Ok((_, sprite, tilemap, emitter, binding, Some(global), visibility)) =
            sources.get(entity)
        else {
            continue;
        };
        if visibility.is_some_and(|visibility| !visibility.get()) {
            continue;
        }
        if let Some(emitter) = emitter {
            extract_particles(scratch, entity, emitter, global);
        }
        let mut material = None;
        if let Some(binding) = binding.filter(|_| sprite.is_some() || tilemap.is_some()) {
            let Ok(captured) = capture_material(binding.clone(), |handle| {
                PreparedImageSnapshot::capture(handle, images.as_deref()?, states.as_deref()?)
            }) else {
                scratch.stats.unprepared_textures =
                    scratch.stats.unprepared_textures.saturating_add(1);
                continue;
            };
            material = Some(captured);
        }
        if let Some(sprite) = sprite {
            let source_order = scratch.sprites.len() as u64;
            let mut extracted = extract_sprite(entity, sprite, global, source_order);
            extracted.material_extension = material.clone();
            if let SpriteScaleMode::Sliced(borders) = sprite.scale_mode
                && let Some(image) = sprite
                    .material
                    .image
                    .and_then(|id| images.as_deref()?.get(id))
                && sprite.size.is_finite()
                && sprite.size.cmpne(Vec2::ZERO).all()
                && extracted.texture_region.is_valid()
            {
                extract_slices(
                    &mut scratch.sprites,
                    extracted,
                    sprite.size,
                    image.extent(),
                    borders,
                );
            } else {
                scratch.sprites.push(extracted);
            }
            scratch.stats.extracted_sprites = scratch.stats.extracted_sprites.saturating_add(1);
        }

        let Some(tilemap) = tilemap else {
            continue;
        };
        let tileset = tilemap
            .tileset
            .and_then(|handle| tilesets.as_deref()?.get(handle))
            .or_else(|| tilemap.authored_tileset());
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

fn extract_particles(
    scratch: &mut ExtractedSpriteScratch,
    entity: Entity,
    emitter: &ParticleEmitter2d,
    global: &GlobalTransform2d,
) {
    let instances = emitter.render_instances(global);
    scratch.stats.extracted_sprites = scratch
        .stats
        .extracted_sprites
        .saturating_add(u32::try_from(instances.len()).unwrap_or(u32::MAX));
    for particle in instances {
        scratch.sprites.push(ExtractedSprite {
            entity,
            source_order: scratch.sprites.len() as u64,
            kind: ExtractedSpriteKind::Sprite,
            material: ExtractedSpriteMaterial {
                image: particle.texture,
                sampler: SamplerDescriptor::default(),
                alpha_mode: particle.alpha_mode,
                tint: particle.color,
            },
            material_extension: None,
            texture_region: TextureUvRect::FULL,
            world_center: particle.world_center,
            world_x_axis: particle.world_x_axis,
            world_y_axis: particle.world_y_axis,
            color: particle.color,
            layer: particle.layer,
            world_z: particle.world_z,
            sort_key: particle.sort_key,
            world_sort_y: 0.0,
        });
    }
}

#[must_use]
pub(super) fn extract_sprite(
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
        material_extension: None,
        texture_region: sprite
            .texture_region
            .map_or(TextureUvRect::FULL, TextureUvRect::from_texture_region)
            .flipped(sprite.flip_x, sprite.flip_y),
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

fn extract_slices(
    extracted: &mut ExtractedSprites,
    sprite: ExtractedSprite,
    size: Vec2,
    extent: ImageExtent,
    borders: NineSlice,
) {
    let borders = borders.nonnegative();
    if borders == NineSlice::default() {
        extracted.push(sprite);
        return;
    }

    let uv = sprite.texture_region;
    let x = SliceAxis::new(
        [borders.left, borders.right],
        extent.width,
        size.x,
        [uv.min.x, uv.size.x],
    );
    let y = SliceAxis::new(
        [borders.top, borders.bottom],
        extent.height,
        size.y,
        [uv.min.y, uv.size.y],
    );
    for row in 0..3 {
        let top = y.positions[row];
        let bottom = y.positions[row + 1];
        if top == bottom {
            continue;
        }
        let row_center = sprite.world_center - sprite.world_y_axis * ((top + bottom) * 0.5);
        let world_y_axis = sprite.world_y_axis * ((bottom - top) * 0.5);
        for column in 0..3 {
            let left = x.positions[column];
            let right = x.positions[column + 1];
            if left == right {
                continue;
            }

            let start = Vec2::new(x.uvs[column], y.uvs[row]);
            let end = Vec2::new(x.uvs[column + 1], y.uvs[row + 1]);
            let mut quad = sprite.clone();
            quad.source_order = extracted.len() as u64;
            quad.texture_region = TextureUvRect::new(start, end - start);
            quad.world_center = row_center + sprite.world_x_axis * ((left + right) * 0.5);
            quad.world_x_axis *= (right - left) * 0.5;
            quad.world_y_axis = world_y_axis;
            extracted.push(quad);
        }
    }
}

struct SliceAxis {
    /// Normalized positions along the original quad, increasing rightward or downward.
    positions: [f32; 4],
    uvs: [f32; 4],
}

impl SliceAxis {
    fn new(borders: [f32; 2], pixels: u32, size: f32, uv: [f32; 2]) -> Self {
        let [start, span] = uv.map(f64::from);
        let source_size = span.abs() * f64::from(pixels);
        let destination_size = f64::from(size.abs());
        let [first, last] = borders.map(f64::from);
        let borders = if span < 0.0 {
            [last, first]
        } else {
            [first, last]
        };
        let source = split_axis(borders, source_size);
        let destination = split_axis([source[1], source_size - source[2]], destination_size);

        Self {
            positions: destination.map(|point| (2.0 * (point / destination_size) - 1.0) as f32),
            uvs: source.map(|point| (start + span * (point / source_size)) as f32),
        }
    }
}

fn split_axis([first, last]: [f64; 2], length: f64) -> [f64; 4] {
    let sum = first + last;
    if sum >= length {
        let middle = length * (first / sum);
        [0.0, middle, middle, length]
    } else {
        [0.0, first, length - last, length]
    }
}

struct TilemapExtractionContext<'a> {
    entity: Entity,
    tilemap: &'a Tilemap,
    tileset: Option<&'a TileSet>,
    affine: Affine2,
    world_z: f32,
    material: ExtractedSpriteMaterial,
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
