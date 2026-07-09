use std::cmp::Ordering;

use nara_asset::Assets;
use nara_core::{Color, Vec2};
use nara_ecs::{Res, ResMut};
use nara_image::{ImageAsset, PreparedImageResource, image_resource_key};
use nara_render::{ExtractedViews, PreparedRenderResources, ViewportRect};
use nara_ui::UiRect;

use crate::{
    ExtractedUiItem, ExtractedUiItems, QueuedUiItem, QueuedUiItems, UiBatch, UiBatches, UiClipRect,
    UiInstance, UiRenderStats, UiTextureRect, material_key,
};

pub fn queue_ui(
    mut queued: ResMut<QueuedUiItems>,
    mut stats: ResMut<UiRenderStats>,
    views: Res<ExtractedViews>,
    extracted: Res<ExtractedUiItems>,
    images: Option<Res<Assets<ImageAsset>>>,
    prepared_images: Option<Res<PreparedRenderResources<PreparedImageResource>>>,
) {
    queued.clear();
    stats.missing_images = 0;
    stats.unprepared_images = 0;

    for item in extracted.as_slice() {
        let Some(view) = views.as_slice().get(item.view_index) else {
            continue;
        };
        let Some(instance) = ui_rect_to_clip_instance(view.viewport, item.rect, item.material.tint)
        else {
            continue;
        };
        let Some(clip_rect) = resolve_clip_rect(item) else {
            continue;
        };
        let material = resolve_ui_material(
            item,
            images.as_deref(),
            prepared_images.as_deref(),
            &mut stats,
        );

        queued.push(QueuedUiItem {
            view_index: item.view_index,
            view_order: view.order,
            target: item.target,
            order: item.order,
            z_index: item.z_index,
            material,
            clip_rect,
            entity_bits: item.entity.to_bits(),
            source_order: item.source_order,
            instance,
        });
    }

    stats.queued_items = saturating_u32(queued.len());
}

pub fn sort_and_batch_ui(
    mut queued: ResMut<QueuedUiItems>,
    mut batches: ResMut<UiBatches>,
    mut stats: ResMut<UiRenderStats>,
) {
    queued.sort();
    let built_batches = build_ui_batches(queued.as_slice());
    stats.batches = saturating_u32(built_batches.len());
    batches.replace(built_batches);
}

#[must_use]
pub fn build_ui_batches(items: &[QueuedUiItem]) -> Vec<UiBatch> {
    let mut batches = Vec::<UiBatch>::new();

    for item in items {
        if let Some(batch) = batches.last_mut()
            && batch.view_index == item.view_index
            && batch.view_order == item.view_order
            && batch.target == item.target
            && batch.order == item.order
            && batch.z_index == item.z_index
            && batch.material == item.material
            && batch.clip_rect == item.clip_rect
        {
            batch.instances.push(item.instance);
            continue;
        }

        batches.push(UiBatch {
            view_index: item.view_index,
            view_order: item.view_order,
            target: item.target,
            order: item.order,
            z_index: item.z_index,
            material: item.material,
            clip_rect: item.clip_rect,
            instances: vec![item.instance],
        });
    }

    batches
}

pub fn compare_queued_ui_items(left: &QueuedUiItem, right: &QueuedUiItem) -> Ordering {
    (
        left.view_order,
        left.view_index,
        left.order,
        left.z_index,
        left.source_order,
        left.entity_bits,
        left.material,
        left.clip_rect,
    )
        .cmp(&(
            right.view_order,
            right.view_index,
            right.order,
            right.z_index,
            right.source_order,
            right.entity_bits,
            right.material,
            right.clip_rect,
        ))
}

#[must_use]
pub fn ui_rect_to_clip_instance(
    viewport: ViewportRect,
    rect: UiRect,
    color: Color,
) -> Option<UiInstance> {
    if !rect.is_non_empty() || viewport.physical_width == 0 || viewport.physical_height == 0 {
        return None;
    }
    let viewport_min = Vec2::new(viewport.physical_x as f32, viewport.physical_y as f32);
    let viewport_size = Vec2::new(
        viewport.physical_width as f32,
        viewport.physical_height as f32,
    );
    let rect_center = rect.min + rect.size * 0.5;
    let local_center = (rect_center - viewport_min) / viewport_size;
    if !local_center.is_finite() {
        return None;
    }

    Some(UiInstance {
        center: Vec2::new(local_center.x * 2.0 - 1.0, 1.0 - local_center.y * 2.0),
        x_axis: Vec2::new(rect.size.x / viewport_size.x, 0.0),
        y_axis: Vec2::new(0.0, -rect.size.y / viewport_size.y),
        color,
        uv: UiTextureRect::new(Vec2::new(0.0, 1.0), Vec2::new(1.0, -1.0)),
    })
}

fn resolve_ui_material(
    item: &ExtractedUiItem,
    images: Option<&Assets<ImageAsset>>,
    prepared_images: Option<&PreparedRenderResources<PreparedImageResource>>,
    stats: &mut UiRenderStats,
) -> crate::UiMaterialKey {
    let image = match item.material.image {
        Some(handle) => {
            if images.and_then(|images| images.get(handle)).is_none() {
                stats.missing_images = stats.missing_images.saturating_add(1);
                None
            } else {
                let key = image_resource_key(handle);
                if prepared_images
                    .and_then(|prepared_images| prepared_images.get_ready(key))
                    .is_some()
                {
                    Some(key)
                } else {
                    stats.unprepared_images = stats.unprepared_images.saturating_add(1);
                    None
                }
            }
        }
        None => None,
    };
    material_key(item.material, image)
}

fn resolve_clip_rect(item: &ExtractedUiItem) -> Option<Option<UiClipRect>> {
    match item.clip_rect {
        None => Some(None),
        Some(clip_rect) => {
            clip_rect.intersect(item.rect)?;
            Some(UiClipRect::from_rect(clip_rect))
        }
    }
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}
