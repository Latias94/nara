use std::cmp::Ordering;

use nara_core::{Color, Vec2};
use nara_ecs::{Res, ResMut};
use nara_render::{ExtractedView, ExtractedViews, RenderPhaseLabel};

use crate::{
    ExtractedSprite, QueuedSpriteItem, QueuedSpriteItems, SpriteBatch, SpriteBatches,
    SpriteInstance, SpriteRenderStats,
};

pub fn queue_sprites(
    mut queued: ResMut<QueuedSpriteItems>,
    mut stats: ResMut<SpriteRenderStats>,
    views: Res<ExtractedViews>,
    extracted: Res<crate::ExtractedSprites>,
) {
    queued.clear();
    stats.unsupported_textured_sprites = saturating_u32(
        extracted
            .as_slice()
            .iter()
            .filter(|sprite| sprite.is_textured())
            .count(),
    );

    for (view_index, view) in views.as_slice().iter().enumerate() {
        for sprite in extracted
            .as_slice()
            .iter()
            .filter(|sprite| !sprite.is_textured())
        {
            let Some(instance) = project_sprite_to_view(view, sprite) else {
                continue;
            };

            queued.push(QueuedSpriteItem {
                view_index,
                view_order: view.order,
                target: view.target,
                phase: sprite.phase,
                layer: sprite.layer,
                sort_key: sprite.sort_key,
                entity_bits: sprite.entity.to_bits(),
                source_order: sprite.source_order,
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
    world_to_clip_instance(
        view,
        sprite.world_center,
        sprite.world_x_axis,
        sprite.world_y_axis,
        sprite.color,
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
    let world_extent = view_world_extent(view)?;
    let clip_scale = Vec2::new(2.0 / world_extent.x, 2.0 / world_extent.y);

    Some(SpriteInstance {
        center: (world_center - view.world_position) * clip_scale,
        x_axis: world_x_axis * clip_scale,
        y_axis: world_y_axis * clip_scale,
        color,
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
        left.source_order,
        left.entity_bits,
    )
        .cmp(&(
            right.view_order,
            right.view_index,
            phase_order(right.phase),
            right.layer,
            right.sort_key,
            right.source_order,
            right.entity_bits,
        ))
}

#[must_use]
pub fn phase_order(phase: RenderPhaseLabel) -> u8 {
    if phase == RenderPhaseLabel::OPAQUE_2D {
        0
    } else if phase == RenderPhaseLabel::TILEMAP_2D {
        1
    } else if phase == RenderPhaseLabel::TRANSPARENT_2D {
        2
    } else if phase == RenderPhaseLabel::GIZMO {
        3
    } else if phase == RenderPhaseLabel::UI {
        4
    } else {
        u8::MAX
    }
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}
