use std::cmp::Ordering;

use nara_asset::Assets;
use nara_core::{Color, Vec2};
use nara_ecs::{Res, ResMut};
use nara_image::{ImageAsset, PreparedImageResource, image_resource_key};
use nara_render::{
    ExtractedView, ExtractedViews, PreparedRenderResources, RenderPhaseLabel, render_phase_order,
};

use crate::{
    ExtractedSprite, QueuedSpriteItem, QueuedSpriteItems, SpriteBatch, SpriteBatches,
    SpriteInstance, SpriteMaterialKey, SpriteRenderStats, TextureUvRect,
};

pub fn queue_sprites(
    mut queued: ResMut<QueuedSpriteItems>,
    mut stats: ResMut<SpriteRenderStats>,
    views: Res<ExtractedViews>,
    extracted: Res<crate::ExtractedSprites>,
    images: Option<Res<Assets<ImageAsset>>>,
    prepared_images: Option<Res<PreparedRenderResources<PreparedImageResource>>>,
) {
    queued.clear();
    stats.missing_textures = 0;
    stats.unprepared_textures = 0;
    stats.invalid_texture_regions = 0;

    let queueable_sprites = resolve_queueable_sprites(
        extracted.as_slice().iter(),
        images.as_deref(),
        prepared_images.as_deref(),
        &mut stats,
    );

    for (view_index, view) in views.as_slice().iter().enumerate() {
        for queueable in &queueable_sprites {
            let Some(instance) =
                project_sprite_to_view_with_uv(view, queueable.sprite, queueable.uv)
            else {
                continue;
            };

            queued.push(QueuedSpriteItem {
                view_index,
                view_order: view.order,
                target: view.target,
                phase: queueable.sprite.phase,
                layer: queueable.sprite.layer,
                sort_key: queueable.sprite.sort_key,
                material: queueable.material,
                entity_bits: queueable.sprite.entity.to_bits(),
                source_order: queueable.sprite.source_order,
                instance,
            });
        }
    }

    stats.queued_items = saturating_u32(queued.len());
}

pub fn sort_and_batch_sprites(
    mut queued: ResMut<QueuedSpriteItems>,
    mut batches: ResMut<SpriteBatches>,
    mut stats: ResMut<SpriteRenderStats>,
) {
    queued.sort();
    let built_batches = build_sprite_batches(queued.as_slice());
    stats.batches = saturating_u32(built_batches.len());
    batches.replace(built_batches);
}

#[must_use]
pub fn project_sprite_to_view(
    view: &ExtractedView,
    sprite: &ExtractedSprite,
) -> Option<SpriteInstance> {
    project_sprite_to_view_with_uv(view, sprite, sprite.texture_region)
}

#[must_use]
pub fn project_sprite_to_view_with_uv(
    view: &ExtractedView,
    sprite: &ExtractedSprite,
    uv: TextureUvRect,
) -> Option<SpriteInstance> {
    world_to_clip_instance_with_uv(
        view,
        sprite.world_center,
        sprite.world_x_axis,
        sprite.world_y_axis,
        sprite.color,
        uv,
    )
}

#[must_use]
pub fn world_to_clip_instance(
    view: &ExtractedView,
    world_center: Vec2,
    world_x_axis: Vec2,
    world_y_axis: Vec2,
    color: Color,
) -> Option<SpriteInstance> {
    world_to_clip_instance_with_uv(
        view,
        world_center,
        world_x_axis,
        world_y_axis,
        color,
        TextureUvRect::FULL,
    )
}

#[must_use]
pub fn world_to_clip_instance_with_uv(
    view: &ExtractedView,
    world_center: Vec2,
    world_x_axis: Vec2,
    world_y_axis: Vec2,
    color: Color,
    uv: TextureUvRect,
) -> Option<SpriteInstance> {
    let world_extent = view_world_extent(view)?;
    let clip_scale = Vec2::new(2.0 / world_extent.x, 2.0 / world_extent.y);

    Some(SpriteInstance {
        center: (world_center - view.world_position) * clip_scale,
        x_axis: world_x_axis * clip_scale,
        y_axis: world_y_axis * clip_scale,
        color,
        uv,
    })
}

#[must_use]
pub fn view_world_extent(view: &ExtractedView) -> Option<Vec2> {
    if view.viewport.physical_height == 0
        || view.viewport_height <= 0.0
        || !view.viewport_height.is_finite()
    {
        return None;
    }

    let aspect = view.viewport.physical_width as f32 / view.viewport.physical_height as f32;
    let width = view.viewport_height * aspect;
    if width <= 0.0 || !width.is_finite() {
        return None;
    }

    Some(Vec2::new(width, view.viewport_height))
}

#[must_use]
pub fn build_sprite_batches(items: &[QueuedSpriteItem]) -> Vec<SpriteBatch> {
    let mut batches = Vec::<SpriteBatch>::new();

    for item in items {
        if let Some(batch) = batches.last_mut()
            && batch.view_index == item.view_index
            && batch.view_order == item.view_order
            && batch.target == item.target
            && batch.phase == item.phase
            && batch.layer == item.layer
            && batch.sort_key == item.sort_key
            && batch.material == item.material
        {
            batch.instances.push(item.instance);
            continue;
        }

        batches.push(SpriteBatch {
            view_index: item.view_index,
            view_order: item.view_order,
            target: item.target,
            phase: item.phase,
            layer: item.layer,
            sort_key: item.sort_key,
            material: item.material,
            instances: vec![item.instance],
        });
    }

    batches
}

pub fn compare_queued_sprite_items(left: &QueuedSpriteItem, right: &QueuedSpriteItem) -> Ordering {
    (
        left.view_order,
        left.view_index,
        phase_order(left.phase),
        left.layer,
        left.sort_key,
        left.material,
        left.source_order,
        left.entity_bits,
    )
        .cmp(&(
            right.view_order,
            right.view_index,
            phase_order(right.phase),
            right.layer,
            right.sort_key,
            right.material,
            right.source_order,
            right.entity_bits,
        ))
}

#[must_use]
pub fn phase_order(phase: RenderPhaseLabel) -> u8 {
    render_phase_order(phase).min(u16::from(u8::MAX)) as u8
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

struct QueueableSprite<'a> {
    sprite: &'a ExtractedSprite,
    material: SpriteMaterialKey,
    uv: TextureUvRect,
}

fn resolve_queueable_sprites<'a>(
    sprites: impl Iterator<Item = &'a ExtractedSprite>,
    images: Option<&Assets<ImageAsset>>,
    prepared_images: Option<&PreparedRenderResources<PreparedImageResource>>,
    stats: &mut SpriteRenderStats,
) -> Vec<QueueableSprite<'a>> {
    sprites
        .filter_map(|sprite| {
            resolve_sprite_material(sprite, images, prepared_images, stats).map(|(material, uv)| {
                QueueableSprite {
                    sprite,
                    material,
                    uv,
                }
            })
        })
        .collect()
}

fn resolve_sprite_material(
    sprite: &ExtractedSprite,
    images: Option<&Assets<ImageAsset>>,
    prepared_images: Option<&PreparedRenderResources<PreparedImageResource>>,
    stats: &mut SpriteRenderStats,
) -> Option<(SpriteMaterialKey, TextureUvRect)> {
    if !sprite.texture_region.is_valid() {
        stats.invalid_texture_regions = stats.invalid_texture_regions.saturating_add(1);
        return None;
    }

    let Some(texture) = sprite.material.image else {
        return Some((
            SpriteMaterialKey::from_material(sprite.material, None),
            sprite.texture_region,
        ));
    };
    let Some(images) = images else {
        stats.missing_textures = stats.missing_textures.saturating_add(1);
        return None;
    };
    if images.get(texture).is_none() {
        stats.missing_textures = stats.missing_textures.saturating_add(1);
        return None;
    }

    let key = image_resource_key(texture);
    let Some(prepared_images) = prepared_images else {
        stats.unprepared_textures = stats.unprepared_textures.saturating_add(1);
        return None;
    };
    if prepared_images.get_ready(key).is_none() {
        stats.unprepared_textures = stats.unprepared_textures.saturating_add(1);
        return None;
    }

    Some((
        SpriteMaterialKey::from_material(sprite.material, Some(key)),
        sprite.texture_region,
    ))
}
