use crate::sprite::{
    WgpuSpriteBatchBuffer, WgpuSpriteDrawStats, create_sprite_batch_buffers,
    sprite_batch_draw_stats,
};
use crate::texture::{WgpuSpriteTextureCache, WgpuSpriteTextureError};
use nara_asset::Assets;
use nara_image::{ImageAsset, PreparedImageResource};
use nara_render::{PreparedRenderResources, RenderPhaseLabel};
use nara_sprite_render::{ColorKey, SpriteBatch, SpriteInstance, SpriteMaterialKey, TextureUvRect};
use nara_ui_render::{UiBatch, UiColorKey, UiInstance, UiMaterialKey, UiTextureRect};

pub(crate) fn create_ui_batch_buffers(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    batches: &[&UiBatch],
    texture_layout: &wgpu::BindGroupLayout,
    texture_cache: &mut WgpuSpriteTextureCache,
    images: Option<&Assets<ImageAsset>>,
    prepared_images: Option<&PreparedRenderResources<PreparedImageResource>>,
    frame_index: u64,
) -> Result<Vec<WgpuSpriteBatchBuffer>, WgpuSpriteTextureError> {
    let sprite_batches = ui_batches_as_sprite_batches(batches);
    let refs = sprite_batches.iter().collect::<Vec<_>>();
    create_sprite_batch_buffers(
        device,
        queue,
        &refs,
        texture_layout,
        texture_cache,
        images,
        prepared_images,
        frame_index,
    )
}

pub(crate) fn ui_batch_draw_stats(batches: &[&UiBatch]) -> WgpuSpriteDrawStats {
    let sprite_batches = ui_batches_as_sprite_batches(batches);
    let refs = sprite_batches.iter().collect::<Vec<_>>();
    sprite_batch_draw_stats(&refs)
}

fn ui_batches_as_sprite_batches(batches: &[&UiBatch]) -> Vec<SpriteBatch> {
    batches
        .iter()
        .map(|batch| SpriteBatch {
            view_index: batch.view_index,
            view_order: batch.view_order,
            target: batch.target,
            phase: RenderPhaseLabel::UI,
            layer: batch.order,
            sort_key: batch.z_index,
            material: sprite_material_key(batch.material),
            instances: batch
                .instances
                .iter()
                .copied()
                .map(sprite_instance)
                .collect(),
        })
        .collect()
}

fn sprite_material_key(material: UiMaterialKey) -> SpriteMaterialKey {
    SpriteMaterialKey {
        image: material.image,
        sampler: material.sampler,
        alpha_mode: material.alpha_mode,
        tint: sprite_color_key(material.tint),
    }
}

fn sprite_color_key(color: UiColorKey) -> ColorKey {
    ColorKey {
        r: color.r,
        g: color.g,
        b: color.b,
        a: color.a,
    }
}

fn sprite_instance(instance: UiInstance) -> SpriteInstance {
    SpriteInstance {
        center: instance.center,
        x_axis: instance.x_axis,
        y_axis: instance.y_axis,
        color: instance.color,
        uv: sprite_texture_rect(instance.uv),
    }
}

fn sprite_texture_rect(rect: UiTextureRect) -> TextureUvRect {
    TextureUvRect::new(rect.min, rect.size)
}
