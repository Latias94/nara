use crate::quad::{WgpuQuadBatch, WgpuQuadInstance, WgpuQuadMaterialKey};
use nara_render::RenderPhaseInput;
use nara_sprite_render::{SpriteBatch, SpriteBatches, SpriteInstance};

pub(crate) fn collect_sprite_quad_batches(
    batches: &SpriteBatches,
    view_index: usize,
) -> Vec<WgpuQuadBatch> {
    batches
        .for_view(view_index)
        .map(sprite_quad_batch)
        .collect()
}

pub(crate) fn append_sprite_phase_inputs(
    batches: &SpriteBatches,
    inputs: &mut Vec<RenderPhaseInput>,
) {
    inputs.extend(batches.as_slice().iter().map(|batch| RenderPhaseInput {
        view_index: batch.view_index,
        phase: batch.phase,
    }));
}

fn sprite_quad_batch(batch: &SpriteBatch) -> WgpuQuadBatch {
    WgpuQuadBatch {
        phase: batch.phase,
        material: WgpuQuadMaterialKey {
            image: batch.material.image,
            sampler: batch.material.sampler,
            alpha_mode: batch.material.alpha_mode,
        },
        instances: batch.instances.iter().map(sprite_quad_instance).collect(),
        counts_as_sprites: true,
    }
}

fn sprite_quad_instance(instance: &SpriteInstance) -> WgpuQuadInstance {
    WgpuQuadInstance {
        center: instance.center.to_array(),
        x_axis: instance.x_axis.to_array(),
        y_axis: instance.y_axis.to_array(),
        color: [
            instance.color.r,
            instance.color.g,
            instance.color.b,
            instance.color.a,
        ],
        uv_min: instance.uv.min.to_array(),
        uv_size: instance.uv.size.to_array(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_core::{Color, Vec2};
    use nara_material::{AlphaMode2d, SamplerDescriptor};
    use nara_render::{RenderPhaseLabel, RenderTarget};
    use nara_sprite_render::{ColorKey, SpriteMaterialKey, TextureUvRect};

    #[test]
    fn sprite_adapter_preserves_material_and_instance_geometry() {
        let batch = SpriteBatch {
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            phase: RenderPhaseLabel::TRANSPARENT_2D,
            layer: 0,
            sort_key: 0,
            material: SpriteMaterialKey {
                image: None,
                sampler: SamplerDescriptor::default(),
                alpha_mode: AlphaMode2d::Blend,
                tint: ColorKey::from_color(Color::WHITE),
            },
            instances: vec![SpriteInstance {
                center: Vec2::new(0.25, -0.5),
                x_axis: Vec2::new(0.1, 0.2),
                y_axis: Vec2::new(-0.3, 0.4),
                color: Color::rgba(0.3, 0.4, 0.5, 0.6),
                uv: TextureUvRect::new(Vec2::new(0.25, 0.5), Vec2::new(0.5, 0.25)),
            }],
        };

        let converted = sprite_quad_batch(&batch);
        assert_eq!(converted.material.alpha_mode, AlphaMode2d::Blend);
        assert_eq!(converted.instances[0].center, [0.25, -0.5]);
        assert_eq!(converted.instances[0].uv_size, [0.5, 0.25]);
        assert!(converted.counts_as_sprites);
    }
}
