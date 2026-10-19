use std::cmp::Ordering;

use nara_core::{Color, Vec2};
use nara_ecs::{Res, ResMut};
use nara_image::{PreparedImageResource, PreparedImageSnapshot, image_resource_key};
use nara_render::{ExtractedView, ExtractedViews, PreparedRenderResources, RenderPrepareStatus};

use crate::{
    ExtractedSprite, PreparedMaterial2d, QueuedSpriteItem, QueuedSpriteItems, SpriteBatch,
    SpriteBatchKind, SpriteBatches, SpriteInstance, SpriteMaterialKey, SpriteRenderStats,
    TextureUvRect,
};

pub fn queue_sprites(
    mut queued: ResMut<QueuedSpriteItems>,
    mut stats: ResMut<SpriteRenderStats>,
    views: Res<ExtractedViews>,
    extracted: Res<crate::ExtractedSprites>,
    prepared_images: Option<Res<PreparedRenderResources<PreparedImageResource>>>,
) {
    queued.clear();
    stats.unprepared_textures = 0;
    stats.invalid_texture_regions = 0;

    let queueable_sprites = resolve_queueable_sprites(
        extracted.as_slice().iter(),
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
                entity: queueable.sprite.entity,
                view_index,
                view_order: view.order,
                target: view.target,
                kind: SpriteBatchKind::from_extracted(queueable.sprite.kind),
                layer: queueable.sprite.layer,
                sort_key: queueable.sprite.sort_key,
                world_sort_y: queueable.sprite.world_sort_y,
                material: queueable.material,
                image: queueable.image.clone(),
                material_extension: queueable.material_extension.clone(),
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
#[cfg(test)]
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
    Some(SpriteInstance {
        center: view.mapping.world_to_clip(world_center),
        x_axis: view.mapping.world_vector_to_clip(world_x_axis),
        y_axis: view.mapping.world_vector_to_clip(world_y_axis),
        color,
        uv,
    })
}

#[must_use]
pub fn build_sprite_batches(items: &[QueuedSpriteItem]) -> Vec<SpriteBatch> {
    let mut batches = Vec::<SpriteBatch>::new();

    for item in items {
        if let Some(batch) = batches.last_mut()
            && batch.view_index == item.view_index
            && batch.view_order == item.view_order
            && batch.target == item.target
            && batch.kind == item.kind
            && batch.layer == item.layer
            && batch.sort_key == item.sort_key
            && batch.material == item.material
            && batch.image == item.image
            && batch.material_extension == item.material_extension
        {
            batch.instances.push(item.instance);
            continue;
        }

        batches.push(SpriteBatch {
            view_index: item.view_index,
            view_order: item.view_order,
            target: item.target,
            kind: item.kind,
            layer: item.layer,
            sort_key: item.sort_key,
            material: item.material,
            image: item.image.clone(),
            material_extension: item.material_extension.clone(),
            instances: vec![item.instance],
        });
    }

    batches
}

pub fn compare_queued_sprite_items(left: &QueuedSpriteItem, right: &QueuedSpriteItem) -> Ordering {
    (left.view_order, left.view_index, left.layer, left.sort_key)
        .cmp(&(
            right.view_order,
            right.view_index,
            right.layer,
            right.sort_key,
        ))
        .then_with(|| {
            let left_y = if left.world_sort_y == 0.0 {
                0.0
            } else {
                left.world_sort_y
            };
            let right_y = if right.world_sort_y == 0.0 {
                0.0
            } else {
                right.world_sort_y
            };
            right_y.total_cmp(&left_y)
        })
        .then_with(|| left.source_order.cmp(&right.source_order))
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

struct QueueableSprite<'a> {
    sprite: &'a ExtractedSprite,
    material: SpriteMaterialKey,
    image: Option<PreparedImageSnapshot>,
    material_extension: Option<PreparedMaterial2d>,
    uv: TextureUvRect,
}

fn resolve_queueable_sprites<'a>(
    sprites: impl Iterator<Item = &'a ExtractedSprite>,
    prepared_images: Option<&PreparedRenderResources<PreparedImageResource>>,
    stats: &mut SpriteRenderStats,
) -> Vec<QueueableSprite<'a>> {
    sprites
        .filter_map(|sprite| {
            resolve_sprite_material(sprite, prepared_images, stats).map(
                |(material, image, material_extension, uv)| QueueableSprite {
                    sprite,
                    material,
                    image,
                    material_extension,
                    uv,
                },
            )
        })
        .collect()
}

fn resolve_sprite_material(
    sprite: &ExtractedSprite,
    prepared_images: Option<&PreparedRenderResources<PreparedImageResource>>,
    stats: &mut SpriteRenderStats,
) -> Option<(
    SpriteMaterialKey,
    Option<PreparedImageSnapshot>,
    Option<PreparedMaterial2d>,
    TextureUvRect,
)> {
    if !sprite.texture_region.is_valid() {
        stats.invalid_texture_regions = stats.invalid_texture_regions.saturating_add(1);
        return None;
    }

    let image = match sprite.material.image {
        Some(texture) => Some(match &sprite.image_content {
            Some(image) if image.handle() == texture => image.prepared_snapshot(),
            Some(_) => return None,
            None => resolve_image(texture, prepared_images, stats)?,
        }),
        None => None,
    };
    let material_extension = match sprite.material_extension {
        Some(instance) => Some(PreparedMaterial2d {
            texture: resolve_image(instance.texture(), prepared_images, stats)?,
            instance,
        }),
        None => None,
    };
    let mut material = SpriteMaterialKey::from_material(sprite.material);
    if let Some(extension) = &material_extension {
        material.alpha_mode = extension.instance.definition().alpha_mode;
    }
    Some((material, image, material_extension, sprite.texture_region))
}

fn resolve_image(
    texture: nara_asset::Handle<nara_image::ImageAsset>,
    prepared_images: Option<&PreparedRenderResources<PreparedImageResource>>,
    stats: &mut SpriteRenderStats,
) -> Option<PreparedImageSnapshot> {
    let key = image_resource_key(texture);
    let Some(prepared_images) = prepared_images else {
        stats.unprepared_textures = stats.unprepared_textures.saturating_add(1);
        return None;
    };
    let Some(record) = prepared_images.get(key) else {
        stats.unprepared_textures = stats.unprepared_textures.saturating_add(1);
        return None;
    };
    let resource = match record.status() {
        RenderPrepareStatus::Ready => record.resource(),
        RenderPrepareStatus::Failed(_) => None,
    };
    let Some(resource) = resource else {
        stats.unprepared_textures = stats.unprepared_textures.saturating_add(1);
        return None;
    };
    Some(PreparedImageSnapshot::new(
        record.snapshot().clone(),
        resource.clone(),
    ))
}
