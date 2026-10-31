use std::{marker::PhantomData, sync::Arc};

use crate::{
    WgpuFeatureEncodeContext, WgpuFeaturePreflightContext, WgpuFeaturePrepareContext,
    WgpuProgramTexture, WgpuRenderFeature, WgpuRenderProgramError,
    quad::{
        WGPU_QUAD_SHAPE_PLAIN, WgpuMaterial2d, WgpuQuadBatch, WgpuQuadInstance,
        WgpuQuadMaterialKey, WgpuQuadTexture,
    },
    stock_quad::StockQuadPrepared,
};
use nara_render::ViewportRect;
use nara_sprite_render::__private::{SpriteBatch, SpriteBatches, SpriteInstance};

/// Stock Sprite drawing against the policy's exact semantic texture port.
pub(crate) struct SpriteFeature<S: 'static>(PhantomData<fn() -> S>);

impl<S: 'static> SpriteFeature<S> {
    pub(crate) const fn new() -> Self {
        Self(PhantomData)
    }
}

/// Immutable lowering shared by complete Image preflight and native preparation.
pub(crate) struct SpriteCapture<S: 'static> {
    pub(crate) target: WgpuProgramTexture<S>,
    pub(crate) viewport: ViewportRect,
    pub(crate) batches: Arc<[WgpuQuadBatch]>,
}

impl<S: 'static> SpriteCapture<S> {
    pub(crate) fn new(
        batches: &SpriteBatches,
        view_index: usize,
        target: WgpuProgramTexture<S>,
        viewport: ViewportRect,
    ) -> Self {
        Self {
            target,
            viewport,
            batches: collect_sprite_quad_batches(batches, view_index).into(),
        }
    }
}

pub(crate) struct SpritePrepared<S: 'static> {
    target: WgpuProgramTexture<S>,
    viewport: ViewportRect,
    draw: StockQuadPrepared,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SpriteReport {
    pub(crate) draw_calls: u32,
    pub(crate) sprites: u32,
    pub(crate) image_sprites: u32,
}

impl<S: 'static> WgpuRenderFeature for SpriteFeature<S> {
    type Capture = SpriteCapture<S>;
    type State = ();
    type Prepared = SpritePrepared<S>;
    type Report = SpriteReport;

    fn preflight(
        &self,
        capture: &Self::Capture,
        context: &mut WgpuFeaturePreflightContext<'_>,
    ) -> Result<(), WgpuRenderProgramError> {
        context.collect_stock_images(&capture.batches)
    }

    fn create_state(
        &self,
        _context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::State, WgpuRenderProgramError> {
        Ok(())
    }

    fn prepare(
        &self,
        _state: &mut Self::State,
        capture: &Self::Capture,
        context: &mut WgpuFeaturePrepareContext<'_>,
    ) -> Result<Self::Prepared, WgpuRenderProgramError> {
        let draw = context
            .prepare_stock_draw(&capture.batches, capture.target.descriptor().native.format)?;
        Ok(SpritePrepared {
            target: capture.target.clone(),
            viewport: capture.viewport,
            draw,
        })
    }

    fn encode(
        &self,
        prepared: &Self::Prepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Self::Report, WgpuRenderProgramError> {
        let target = context.resources().texture(&prepared.target)?;
        prepared
            .draw
            .encode(context.encoder(), target, prepared.viewport);
        Ok(SpriteReport {
            draw_calls: prepared.draw.draw_calls,
            sprites: prepared.draw.sprites,
            image_sprites: prepared.draw.image_sprites,
        })
    }
}

pub(crate) fn collect_sprite_quad_batches(
    batches: &SpriteBatches,
    view_index: usize,
) -> Vec<WgpuQuadBatch> {
    batches
        .for_view(view_index)
        .map(sprite_quad_batch)
        .collect()
}

fn sprite_quad_batch(batch: &SpriteBatch) -> WgpuQuadBatch {
    let texture = batch
        .image
        .clone()
        .map_or(WgpuQuadTexture::Fallback, WgpuQuadTexture::Image);
    WgpuQuadBatch {
        material: WgpuQuadMaterialKey {
            texture: texture.key(),
            sampler: batch.material.sampler,
            alpha_mode: batch.material.alpha_mode,
        },
        texture,
        scissor: None,
        instances: batch.instances.iter().map(sprite_quad_instance).collect(),
        counts_as_sprites: true,
        material_extension: batch
            .material_extension
            .as_ref()
            .map(|material| WgpuMaterial2d {
                definition: material.instance.definition(),
                parameters: material.instance.parameters(),
                texture: material.texture.clone(),
            }),
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
        texture_mode: 0,
        shape_mode: WGPU_QUAD_SHAPE_PLAIN,
        rect_size: [0.0; 2],
        corner_radius: 0.0,
        border_width: 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AccessRole, GraphLimits, TextureAlpha, TextureTransfer, WgpuFeatureDefinition,
        WgpuProgramTextureDescriptor, WgpuRenderHost, WgpuRenderProgramBuilder,
    };
    use nara_core::{Color, Vec2};
    use nara_material::{AlphaMode2d, SamplerDescriptor};
    use nara_render::{RenderFrameSourceGeneration, RenderTarget};
    use nara_sprite_render::__private::{SpriteBatchKind, SpriteMaterialKey, TextureUvRect};

    #[test]
    fn sprite_capture_keeps_the_exact_port_without_requiring_a_send_semantic() {
        type SceneColor = std::rc::Rc<()>;
        let mut host = WgpuRenderHost::offscreen();
        let producer = host.create_producer().unwrap();
        let definition = WgpuFeatureDefinition::new(SpriteFeature::<SceneColor>::new());
        let instance = host
            .create_feature_instance(&producer, &definition)
            .unwrap();
        let mut builder = WgpuRenderProgramBuilder::new(
            RenderFrameSourceGeneration::try_acquire().unwrap(),
            1,
            GraphLimits::default(),
        )
        .unwrap();
        let target = builder
            .texture::<SceneColor>(WgpuProgramTextureDescriptor {
                native: wgpu::TextureDescriptor {
                    label: Some("sprite-capture"),
                    size: wgpu::Extent3d {
                        width: 64,
                        height: 64,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                    view_formats: &[],
                },
                transfer: TextureTransfer::Linear,
                alpha: TextureAlpha::Premultiplied,
            })
            .unwrap();
        let viewport = ViewportRect::new(4, 8, 32, 16).unwrap();
        let capture = SpriteCapture::new(&SpriteBatches::default(), 3, target.clone(), viewport);
        assert_eq!(capture.viewport, viewport);
        assert!(capture.batches.is_empty());
        let writes = [capture
            .target
            .clear(wgpu::Color::TRANSPARENT, AccessRole::Attachment)];
        builder
            .operation(1, &instance, Arc::new(capture), &[], &writes)
            .unwrap();
        builder.finish(&[target.output()]).unwrap();
    }

    #[test]
    fn sprite_adapter_preserves_material_and_instance_geometry() {
        let batch = SpriteBatch {
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            kind: SpriteBatchKind::Sprite,
            layer: 0,
            sort_key: 0,
            material: SpriteMaterialKey {
                sampler: SamplerDescriptor::default(),
                alpha_mode: AlphaMode2d::Blend,
            },
            image: None,
            material_extension: None,
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
