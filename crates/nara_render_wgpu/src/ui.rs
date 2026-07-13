use crate::quad::{WgpuQuadBatch, WgpuQuadInstance, WgpuQuadMaterialKey};
use nara_render::{RenderPhaseInput, RenderPhaseLabel};
use nara_ui_render::{UiBatch, UiBatches, UiInstance};

pub(crate) fn collect_ui_quad_batches(
    batches: &UiBatches,
    view_index: usize,
) -> Vec<WgpuQuadBatch> {
    batches.for_view(view_index).map(ui_quad_batch).collect()
}

pub(crate) fn append_ui_phase_inputs(batches: &UiBatches, inputs: &mut Vec<RenderPhaseInput>) {
    inputs.extend(batches.as_slice().iter().map(|batch| RenderPhaseInput {
        view_index: batch.view_index,
        phase: RenderPhaseLabel::UI,
    }));
}

fn ui_quad_batch(batch: &UiBatch) -> WgpuQuadBatch {
    WgpuQuadBatch {
        phase: RenderPhaseLabel::UI,
        material: WgpuQuadMaterialKey {
            image: batch.material.image,
            sampler: batch.material.sampler,
            alpha_mode: batch.material.alpha_mode,
        },
        instances: batch.instances.iter().map(ui_quad_instance).collect(),
        counts_as_sprites: false,
    }
}

fn ui_quad_instance(instance: &UiInstance) -> WgpuQuadInstance {
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
    use nara_render::RenderTarget;
    use nara_ui::UiRect;
    use nara_ui_render::{UiClipRect, UiColorKey, UiMaterialKey, UiTextureRect};

    #[test]
    fn ui_adapter_does_not_require_sprite_domain_types() {
        let batch = UiBatch {
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            order: 0,
            z_index: 0,
            material: UiMaterialKey {
                image: None,
                sampler: SamplerDescriptor::default(),
                alpha_mode: AlphaMode2d::Blend,
                tint: UiColorKey::from_color(Color::WHITE),
            },
            clip_rect: Some(UiClipRect::from_rect(UiRect::from_origin_size(
                Vec2::ZERO,
                Vec2::ONE,
            ))),
            instances: vec![UiInstance {
                center: Vec2::ZERO,
                x_axis: Vec2::X,
                y_axis: Vec2::Y,
                color: Color::WHITE,
                uv: UiTextureRect::FULL,
            }],
        };

        let converted = ui_quad_batch(&batch);
        assert_eq!(converted.phase, RenderPhaseLabel::UI);
        assert!(!converted.counts_as_sprites);
        assert_eq!(converted.instances.len(), 1);
    }
}
