use crate::sprite::{
    WgpuSpriteBatchBuffer, WgpuSpriteDrawStats, create_sprite_batch_buffers,
    sprite_batch_draw_stats,
};
use crate::texture::{WgpuSpriteTextureCache, WgpuSpriteTextureError};
use nara_asset::Assets;
use nara_image::{ImageAsset, PreparedImageResource};
use nara_render::{PreparedRenderResources, RenderPhaseLabel};
use nara_sprite_render::SpriteBatch;
use nara_ui_render::UiBatch;

pub(crate) fn create_ui_batch_buffers(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    batches: &[&UiBatch],
    texture_layout: &wgpu::BindGroupLayout,
    texture_cache: &mut WgpuSpriteTextureCache,
    images: Option<&Assets<ImageAsset>>,
    prepared_images: Option<&PreparedRenderResources<PreparedImageResource>>,
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
            material: batch.material,
            instances: batch.instances.clone(),
        })
        .collect()
}
