use std::{
    collections::hash_map::RandomState, hash::BuildHasher, marker::PhantomData, ops::Range,
    sync::Arc,
};

use crate::{
    WgpuFeatureEncodeContext, WgpuFeaturePreflightContext, WgpuFeaturePrepareContext,
    WgpuProgramTexture, WgpuRenderFeature, WgpuRenderProgramError,
    quad::{
        WGPU_QUAD_SHAPE_PLAIN, WgpuMaterial2d, WgpuMaterialTexture, WgpuQuadBatch,
        WgpuQuadInstance, WgpuQuadMaterialKey, WgpuQuadTexture,
    },
    stock_quad::StockQuadPrepared,
    view_uniform::WgpuViewUniform,
};
use nara_render::{CapturedRenderTime, ExtractedView, ViewLoadOp, ViewportRect};
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
    pub(crate) batches: Arc<[WgpuQuadBatch]>,
    views: Arc<[SpriteViewDraw]>,
    _captured_time: Option<CapturedRenderTime>,
}

#[derive(Clone, Debug)]
struct SpriteViewDraw {
    viewport: ViewportRect,
    load: ViewLoadOp,
    buffers: Range<usize>,
}

/// Every Sprite operation can own this bounded View table, including a clear-only capture.
/// The program admits this capacity before its typed operation/capture allocation.
pub(crate) const fn ordered_view_metadata_bytes() -> usize {
    nara_render::MAX_VIEWS_PER_TARGET * std::mem::size_of::<SpriteViewDraw>()
        + 2 * std::mem::size_of::<usize>()
}

pub(crate) fn material_texture_metadata_bytes(
    batches: usize,
    slots: usize,
    materials: usize,
    selections: usize,
) -> Result<usize, WgpuRenderProgramError> {
    let metadata = materials
        .checked_mul(std::mem::size_of::<Vec<(u32, WgpuMaterialTexture)>>())
        .and_then(|bytes| {
            batches
                .checked_mul(std::mem::size_of::<WgpuQuadBatch>())
                .and_then(|batches| bytes.checked_add(batches))
        })
        .and_then(|bytes| {
            slots
                .checked_mul(std::mem::size_of::<(u32, WgpuMaterialTexture)>())
                .and_then(|slots| bytes.checked_add(slots))
        })
        .ok_or(crate::GraphError::LimitExceeded(
            crate::LimitKind::MetadataBytes,
        ))?;
    if selections == 0 {
        return Ok(metadata);
    }
    let scratch_per_selection = std::mem::size_of::<(u64, usize)>()
        + std::mem::size_of::<crate::WgpuProgramRead>()
        + std::mem::size_of::<bool>();
    selections
        .checked_mul(scratch_per_selection)
        .and_then(|bytes| bytes.checked_add(std::mem::size_of::<Vec<(u64, usize)>>()))
        .and_then(|bytes| bytes.checked_add(std::mem::size_of::<Vec<crate::WgpuProgramRead>>()))
        .and_then(|bytes| bytes.checked_add(std::mem::size_of::<Vec<bool>>()))
        .and_then(|bytes| bytes.checked_add(std::mem::size_of::<RandomState>()))
        .and_then(|bytes| bytes.checked_add(metadata))
        .ok_or_else(|| crate::GraphError::LimitExceeded(crate::LimitKind::MetadataBytes).into())
}

impl<S: 'static> SpriteCapture<S> {
    pub(crate) fn bind_material_textures(
        &mut self,
        bindings: &[crate::WgpuMaterialTextureBinding],
    ) -> Result<Vec<crate::WgpuProgramRead>, WgpuRenderProgramError> {
        if bindings.is_empty() {
            return Ok(Vec::new());
        }
        let slots = self
            .batches
            .iter()
            .try_fold(0_usize, |total, batch| {
                total.checked_add(
                    batch
                        .material_extension
                        .as_ref()
                        .map_or(0, |material| material.textures.len()),
                )
            })
            .ok_or(crate::GraphError::LimitExceeded(
                crate::LimitKind::MetadataBytes,
            ))?;
        if bindings.len() > slots {
            return Err(WgpuRenderProgramError::feature(
                "material texture selections exceed the captured texture slots",
            ));
        }
        let hasher = RandomState::new();
        let mut selections = Vec::new();
        selections
            .try_reserve_exact(bindings.len())
            .map_err(|_| WgpuRenderProgramError::ResourceCapacity)?;
        for (index, binding) in bindings.iter().enumerate() {
            selections.push((hasher.hash_one(binding.selection()), index));
        }
        selections.sort_unstable();
        for (index, &(hash, selected)) in selections.iter().enumerate() {
            let first = selections[..index].partition_point(|(other, _)| *other < hash);
            if selections[first..index].iter().any(|&(_, other)| {
                bindings[other].binding() == bindings[selected].binding()
                    && bindings[other].selection() == bindings[selected].selection()
            }) {
                return Err(WgpuRenderProgramError::feature(
                    "duplicate material program texture selection",
                ));
            }
        }
        let mut matched = Vec::new();
        matched
            .try_reserve_exact(bindings.len())
            .map_err(|_| WgpuRenderProgramError::ResourceCapacity)?;
        matched.resize(bindings.len(), false);
        let batches = Arc::get_mut(&mut self.batches).ok_or_else(|| {
            WgpuRenderProgramError::feature(
                "material sources must be bound before sharing the lowered capture",
            )
        })?;
        for batch in batches {
            if batch.instances.is_empty() {
                continue;
            }
            let Some(material) = &mut batch.material_extension else {
                continue;
            };
            let selection = crate::material_textures::MaterialSelection(&material.binding);
            let hash = hasher.hash_one(selection);
            let first = selections.partition_point(|(other, _)| *other < hash);
            let last = selections.partition_point(|(other, _)| *other <= hash);
            for &(_, index) in &selections[first..last] {
                let replacement = &bindings[index];
                if replacement.selection()
                    != crate::material_textures::MaterialSelection(&material.binding)
                {
                    continue;
                }
                let Some((_, texture)) = material
                    .textures
                    .iter_mut()
                    .find(|(slot, _)| *slot == replacement.binding())
                else {
                    return Err(WgpuRenderProgramError::feature(
                        "the captured material texture slot is unavailable",
                    ));
                };
                let source = replacement.source();
                *texture = WgpuMaterialTexture::Program(source);
                matched[index] = true;
            }
        }
        if matched.iter().any(|matched| !matched) {
            return Err(WgpuRenderProgramError::feature(
                "material program texture selection has no matching captured draw",
            ));
        }
        let mut reads = Vec::new();
        reads
            .try_reserve_exact(bindings.len())
            .map_err(|_| WgpuRenderProgramError::ResourceCapacity)?;
        reads.extend(bindings.iter().map(|binding| binding.source().read()));
        reads.sort_unstable_by(|left, right| left.version().cmp(right.version()));
        reads.dedup_by(|left, right| left.version() == right.version());
        Ok(reads)
    }

    #[cfg(test)]
    pub(crate) fn new(
        batches: &SpriteBatches,
        view_index: usize,
        target: WgpuProgramTexture<S>,
        viewport: ViewportRect,
    ) -> Self {
        Self::from_lowered(
            target,
            viewport,
            collect_sprite_quad_batches(batches, view_index, WgpuViewUniform::manual(viewport))
                .into(),
        )
    }

    #[cfg(test)]
    pub(crate) fn from_lowered(
        target: WgpuProgramTexture<S>,
        viewport: ViewportRect,
        batches: Arc<[WgpuQuadBatch]>,
    ) -> Self {
        let buffers = batches
            .iter()
            .filter(|batch| !batch.instances.is_empty())
            .count();
        Self {
            target,
            batches,
            views: Arc::from([SpriteViewDraw {
                viewport,
                load: ViewLoadOp::Load,
                buffers: 0..buffers,
            }]),
            _captured_time: None,
        }
    }

    /// Keeps the admitted world's View order and load semantics inside its one graph writer.
    /// UI and later effects consume a separate version selected by the renderer policy.
    pub(crate) fn for_views(
        batches: Option<&SpriteBatches>,
        views: &[ExtractedView],
        captured_time: Option<CapturedRenderTime>,
        target: WgpuProgramTexture<S>,
    ) -> Result<Self, WgpuRenderProgramError> {
        if views.is_empty() || views.len() > nara_render::MAX_VIEWS_PER_TARGET {
            return Err(WgpuRenderProgramError::feature(
                "invalid stock world View count",
            ));
        }
        if batches.is_some_and(|batches| {
            batches.as_slice().iter().any(|batch| {
                views.get(batch.view_index).is_none_or(|view| {
                    batch.target != view.target || batch.view_order != view.order
                })
            })
        }) {
            return Err(WgpuRenderProgramError::feature(
                "a Sprite batch does not belong to the captured View and logical target",
            ));
        }
        let batch_count = batches.map_or(0, |batches| {
            batches
                .as_slice()
                .iter()
                .filter(|batch| batch.view_index < views.len())
                .count()
        });
        let mut lowered = Vec::new();
        lowered
            .try_reserve_exact(batch_count)
            .map_err(|_| WgpuRenderProgramError::ResourceCapacity)?;
        let mut draws = Vec::new();
        draws
            .try_reserve_exact(views.len())
            .map_err(|_| WgpuRenderProgramError::ResourceCapacity)?;
        let mut next_buffer = 0;
        for (index, view) in views.iter().enumerate() {
            let first_buffer = next_buffer;
            let uniform = WgpuViewUniform::capture(view, captured_time);
            if let Some(batches) = batches {
                for batch in batches.for_view(index) {
                    if !batch.instances.is_empty() {
                        next_buffer += 1;
                    }
                    lowered.push(sprite_quad_batch(batch, uniform));
                }
            }
            draws.push(SpriteViewDraw {
                viewport: view.viewport,
                load: view.load,
                buffers: first_buffer..next_buffer,
            });
        }
        Ok(Self {
            target,
            batches: lowered.into(),
            views: draws.into(),
            _captured_time: captured_time,
        })
    }
}

pub(crate) struct SpritePrepared<S: 'static> {
    target: WgpuProgramTexture<S>,
    views: Arc<[SpriteViewDraw]>,
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
            views: Arc::clone(&capture.views),
            draw,
        })
    }

    fn encode(
        &self,
        prepared: &Self::Prepared,
        context: &mut WgpuFeatureEncodeContext<'_>,
    ) -> Result<Self::Report, WgpuRenderProgramError> {
        let target = context.resources().texture(&prepared.target)?;
        prepared.draw.encode_views(
            context.encoder(),
            target,
            prepared
                .views
                .iter()
                .map(|view| (view.viewport, view.load, view.buffers.clone())),
        );
        Ok(SpriteReport {
            draw_calls: prepared.draw.draw_calls,
            sprites: prepared.draw.sprites,
            image_sprites: prepared.draw.image_sprites,
        })
    }
}

#[cfg(test)]
pub(crate) fn collect_sprite_quad_batches(
    batches: &SpriteBatches,
    view_index: usize,
    view: WgpuViewUniform,
) -> Vec<WgpuQuadBatch> {
    batches
        .for_view(view_index)
        .map(|batch| sprite_quad_batch(batch, view))
        .collect()
}

fn sprite_quad_batch(batch: &SpriteBatch, view: WgpuViewUniform) -> WgpuQuadBatch {
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
                binding: material.binding.clone(),
                view,
                textures: material
                    .textures
                    .iter()
                    .map(|(binding, image)| (*binding, image.clone().into()))
                    .collect(),
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
        AccessRole, GraphLimits, TextureAlpha, TextureInterpretation, TextureTransfer,
        WgpuFeatureDefinition, WgpuProgramTextureDescriptor, WgpuRenderHost,
        WgpuRenderProgramBuilder,
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
                interpretation: TextureInterpretation::Color {
                    transfer: TextureTransfer::Linear,
                    alpha: TextureAlpha::Premultiplied,
                },
            })
            .unwrap();
        let viewport = ViewportRect::new(4, 8, 32, 16).unwrap();
        let capture = SpriteCapture::new(&SpriteBatches::default(), 3, target.clone(), viewport);
        assert_eq!(capture.views[0].viewport, viewport);
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

        let converted = sprite_quad_batch(
            &batch,
            WgpuViewUniform::manual(ViewportRect::new(0, 0, 64, 64).unwrap()),
        );
        assert_eq!(converted.material.alpha_mode, AlphaMode2d::Blend);
        assert_eq!(converted.instances[0].center, [0.25, -0.5]);
        assert_eq!(converted.instances[0].uv_size, [0.5, 0.25]);
        assert!(converted.counts_as_sprites);
    }

    #[test]
    fn ordered_lowering_freezes_each_original_view_and_retains_exact_cpu_time() {
        let declaration = nara_material::Material2dDeclaration::from_parameters::<
            crate::readback_tests::TestMaterialParameters,
        >("test.view-capture", 1, AlphaMode2d::Opaque, Vec::new())
        .unwrap();
        let generation = nara_material::Material2dGeneration::from_shader_source(
            declaration,
            "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return vec4<f32>(nara_view.viewport.x / 32.0, f32(nara_view.clock_available), nara_view.real_elapsed, 1.0); }",
        )
        .unwrap();
        let binding = generation
            .bind(
                &crate::readback_tests::TestMaterialParameters {
                    value: nara_material::Vec4::ONE,
                },
                std::collections::BTreeMap::new(),
            )
            .unwrap();
        let captured = crate::view_probe::captured_split_frame_with_material(Some(binding));
        let packet = &captured.topology;
        let mut builder = WgpuRenderProgramBuilder::new(
            packet.source_generation(),
            packet.frame_index(),
            GraphLimits::default(),
        )
        .unwrap();
        let target = builder
            .texture::<()>(WgpuProgramTextureDescriptor {
                native: wgpu::TextureDescriptor {
                    label: Some("captured-views"),
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
                interpretation: TextureInterpretation::Color {
                    transfer: TextureTransfer::Linear,
                    alpha: TextureAlpha::Premultiplied,
                },
            })
            .unwrap();
        let lower = |views: &[ExtractedView], time: Option<CapturedRenderTime>| {
            SpriteCapture::for_views(
                captured.payload.sprite_batches.as_ref(),
                views,
                time,
                target.clone(),
            )
            .unwrap()
        };
        let original = lower(packet.views(), packet.captured_time());
        assert_eq!(original.batches.len(), 2);
        assert_eq!(original._captured_time, packet.captured_time());
        for (batch, view) in original.batches.iter().zip(packet.views()) {
            let uniform = batch.material_extension.as_ref().unwrap().view;
            assert_eq!(
                uniform,
                WgpuViewUniform::capture(view, packet.captured_time())
            );
            assert_eq!(uniform.clock_available, 1);
            assert_eq!(uniform.frame, [1, 0]);
        }
        assert_ne!(
            original.batches[0]
                .material_extension
                .as_ref()
                .unwrap()
                .view,
            original.batches[1]
                .material_extension
                .as_ref()
                .unwrap()
                .view,
        );
        let mut later_views = packet.views().to_vec();
        later_views[0].viewport.physical_width -= 1;
        let later = lower(&later_views, None);
        assert_eq!(later._captured_time, None);
        let original_uniform = original.batches[0]
            .material_extension
            .as_ref()
            .unwrap()
            .view;
        let later_uniform = later.batches[0].material_extension.as_ref().unwrap().view;
        assert_ne!(original_uniform, later_uniform);
        assert_eq!(later_uniform.clock_available, 0);
        assert_eq!(original_uniform.clock_available, 1);
        assert_eq!(original_uniform.viewport, [0.0, 0.0, 32.0, 64.0]);
        assert_eq!(
            crate::required_quad_pipeline_keys(&original.batches, wgpu::TextureFormat::Rgba8Unorm),
            crate::required_quad_pipeline_keys(&later.batches, wgpu::TextureFormat::Rgba8Unorm),
        );
    }
}
