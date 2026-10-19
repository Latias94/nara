use crate::texture::{
    WgpuCachedImageView, WgpuCachedTextureBinding, WgpuSpriteTextureCache, WgpuTextureError,
};
use bytemuck::{Pod, Zeroable};
use nara_image::PreparedImageSnapshot;
use nara_material::{AlphaMode2d, Material2dDefinition, SamplerDescriptor};
use nara_render::{RenderResourceKey, ViewportRect};
#[cfg(feature = "ui-submitter")]
use nara_ui_render::{UiGlyphAtlasPageKey, UiGlyphAtlasPageSnapshot};
use thiserror::Error;
use wgpu::util::DeviceExt;

const VERTICES_PER_QUAD: u32 = 6;
const MAX_QUAD_INSTANCE_BUFFER_BYTES_PER_FRAME: u64 = 64 * 1024 * 1024;
pub(crate) const WGPU_QUAD_SHAPE_PLAIN: u32 = 0;
#[cfg(feature = "ui-submitter")]
pub(crate) const WGPU_QUAD_SHAPE_ROUNDED_FILL: u32 = 1;
#[cfg(feature = "ui-submitter")]
pub(crate) const WGPU_QUAD_SHAPE_ROUNDED_BORDER: u32 = 2;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct WgpuQuadInstance {
    pub(crate) center: [f32; 2],
    pub(crate) x_axis: [f32; 2],
    pub(crate) y_axis: [f32; 2],
    pub(crate) color: [f32; 4],
    pub(crate) uv_min: [f32; 2],
    pub(crate) uv_size: [f32; 2],
    pub(crate) texture_mode: u32,
    pub(crate) shape_mode: u32,
    pub(crate) rect_size: [f32; 2],
    pub(crate) corner_radius: f32,
    pub(crate) border_width: f32,
}

const QUAD_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 11] = wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x2,
    2 => Float32x2,
    3 => Float32x4,
    4 => Float32x2,
    5 => Float32x2,
    6 => Uint32,
    7 => Uint32,
    8 => Float32x2,
    9 => Float32,
    10 => Float32
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum WgpuQuadTextureKey {
    Fallback,
    Image(RenderResourceKey),
    #[cfg(feature = "ui-submitter")]
    Glyph(UiGlyphAtlasPageKey),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum WgpuQuadTexture {
    Fallback,
    Image(PreparedImageSnapshot),
    #[cfg(feature = "ui-submitter")]
    Glyph(UiGlyphAtlasPageSnapshot),
}

impl WgpuQuadTexture {
    pub(crate) fn key(&self) -> WgpuQuadTextureKey {
        match self {
            Self::Fallback => WgpuQuadTextureKey::Fallback,
            Self::Image(snapshot) => WgpuQuadTextureKey::Image(snapshot.identity().key()),
            #[cfg(feature = "ui-submitter")]
            Self::Glyph(page) => WgpuQuadTextureKey::Glyph(page.key()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct WgpuQuadMaterialKey {
    pub(crate) texture: WgpuQuadTextureKey,
    pub(crate) sampler: SamplerDescriptor,
    pub(crate) alpha_mode: AlphaMode2d,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WgpuQuadBatch {
    pub(crate) material: WgpuQuadMaterialKey,
    pub(crate) texture: WgpuQuadTexture,
    pub(crate) scissor: Option<WgpuScissorRect>,
    pub(crate) instances: Vec<WgpuQuadInstance>,
    pub(crate) counts_as_sprites: bool,
    pub(crate) material_extension: Option<WgpuMaterial2d>,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WgpuMaterial2d {
    pub(crate) definition: Material2dDefinition,
    pub(crate) parameters: [f32; 4],
    pub(crate) texture: PreparedImageSnapshot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WgpuScissorRect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct WgpuQuadPipeline {
    pub(crate) key: WgpuQuadPipelineKey,
    pub(crate) pipeline: wgpu::RenderPipeline,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WgpuQuadPipelineKey {
    pub(crate) format: wgpu::TextureFormat,
    pub(crate) alpha_mode: AlphaMode2d,
    pub(crate) material: Option<Material2dDefinition>,
}

#[derive(Debug, Clone)]
pub(crate) struct WgpuQuadPipelineDrawRef {
    pub(crate) alpha_mode: AlphaMode2d,
    pub(crate) material: Option<Material2dDefinition>,
    pub(crate) pipeline: wgpu::RenderPipeline,
}

#[derive(Debug)]
pub(crate) struct WgpuQuadBatchBuffer {
    buffer: wgpu::Buffer,
    bind_group: WgpuCachedTextureBinding,
    material_bind_group: Option<wgpu::BindGroup>,
    material_image: Option<WgpuCachedImageView>,
    material: Option<Material2dDefinition>,
    alpha_mode: AlphaMode2d,
    scissor: Option<WgpuScissorRect>,
    instance_count: u32,
    logical_bytes: u64,
}

pub(crate) fn create_material_bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("nara_material_2d_bind_group_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: wgpu::BufferSize::new(16),
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
        ],
    })
}

impl WgpuQuadBatchBuffer {
    pub(crate) const fn logical_bytes(&self) -> u64 {
        self.logical_bytes
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WgpuQuadDrawStats {
    pub(crate) draw_calls: u32,
    pub(crate) sprites: u32,
    pub(crate) image_sprites: u32,
}

#[derive(Debug, Clone, Copy, Error, PartialEq, Eq)]
#[error("quad instances require {required} bytes; per-frame instance-buffer maximum is {maximum}")]
pub(crate) struct WgpuQuadInstanceBudgetError {
    required: u64,
    maximum: u64,
}

pub(crate) fn create_quad_texture_bind_group_layout(
    device: &wgpu::Device,
) -> wgpu::BindGroupLayout {
    device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("nara_wgpu_quad_texture_bind_group_layout"),
        entries: &[
            wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: true },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            },
            wgpu::BindGroupLayoutEntry {
                binding: 1,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                count: None,
            },
        ],
    })
}

pub(crate) fn create_quad_pipeline(
    device: &wgpu::Device,
    key: WgpuQuadPipelineKey,
    texture_bind_group_layout: &wgpu::BindGroupLayout,
    material_bind_group_layout: &wgpu::BindGroupLayout,
) -> Result<WgpuQuadPipeline, crate::WgpuRenderError> {
    let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);
    let fragment = key.material.map_or(
        "fn material_fragment(sampled: vec4<f32>, tint: vec4<f32>, uv: vec2<f32>) -> vec4<f32> { return sampled * tint; }",
        |material| material.fragment_shader,
    );
    let bindings = if key.material.is_some() {
        "@group(1) @binding(0) var<uniform> material_parameters: vec4<f32>;\n@group(1) @binding(1) var material_texture: texture_2d<f32>;"
    } else {
        ""
    };
    let source = format!("{}\n{bindings}\n{fragment}", include_str!("sprite.wgsl"));
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("nara_wgpu_quad_shader"),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    let mut layouts = vec![Some(texture_bind_group_layout)];
    if key.material.is_some() {
        layouts.push(Some(material_bind_group_layout));
    }
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("nara_wgpu_quad_pipeline_layout"),
        bind_group_layouts: &layouts,
        immediate_size: 0,
    });
    let vertex_buffer_layout = quad_instance_buffer_layout();
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("nara_wgpu_quad_pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Some(vertex_buffer_layout)],
        },
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            ..Default::default()
        },
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(quad_color_target_state(key.format, key.alpha_mode))],
        }),
        multiview_mask: None,
        cache: None,
    });

    if let Some(error) = pollster::block_on(error_scope.pop()) {
        return Err(crate::WgpuRenderError::ContentRejected {
            message: format!(
                "2D material {:?}: {error}",
                key.material.map(|material| material.id)
            ),
        });
    }
    Ok(WgpuQuadPipeline { key, pipeline })
}

pub(crate) fn create_quad_batch_buffers(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    batches: &[WgpuQuadBatch],
    texture_layout: &wgpu::BindGroupLayout,
    material_layout: &wgpu::BindGroupLayout,
    texture_cache: &mut WgpuSpriteTextureCache,
    frame_index: u64,
) -> Result<Vec<WgpuQuadBatchBuffer>, WgpuTextureError> {
    batches
        .iter()
        .filter(|batch| !batch.instances.is_empty())
        .map(|batch| {
            let bind_group = texture_cache.bind_group(
                device,
                queue,
                texture_layout,
                batch.material,
                &batch.texture,
                frame_index,
            )?;
            let (material_bind_group, material_image) =
                if let Some(material) = &batch.material_extension {
                    let view = texture_cache.prepared_image_view(
                        device,
                        queue,
                        &material.texture,
                        frame_index,
                    )?;
                    let parameters = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                        label: Some("nara_material_2d_parameters"),
                        contents: bytemuck::cast_slice(&material.parameters),
                        usage: wgpu::BufferUsages::UNIFORM,
                    });
                    let binding = device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("nara_material_2d_bind_group"),
                        layout: material_layout,
                        entries: &[
                            wgpu::BindGroupEntry {
                                binding: 0,
                                resource: parameters.as_entire_binding(),
                            },
                            wgpu::BindGroupEntry {
                                binding: 1,
                                resource: wgpu::BindingResource::TextureView(&view.view),
                            },
                        ],
                    });
                    (Some(binding), Some(view))
                } else {
                    (None, None)
                };
            let instance_count = saturating_u32(batch.instances.len());
            let logical_bytes = quad_instance_byte_len(batch.instances.len());
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("nara_wgpu_quad_instances"),
                contents: bytemuck::cast_slice(&batch.instances),
                usage: wgpu::BufferUsages::VERTEX,
            });

            Ok(WgpuQuadBatchBuffer {
                buffer,
                bind_group,
                material_bind_group,
                material_image,
                material: batch
                    .material_extension
                    .as_ref()
                    .map(|material| material.definition),
                alpha_mode: batch.material.alpha_mode,
                scissor: batch.scissor,
                instance_count,
                logical_bytes,
            })
        })
        .collect()
}

impl WgpuQuadBatchBuffer {
    pub(crate) fn retain_images_until_submission_complete(&self, queue: &wgpu::Queue) {
        self.bind_group.retain_until_submission_complete(queue);
        if let Some(image) = &self.material_image {
            image.retain_until_submission_complete(queue);
        }
    }
}

pub(crate) fn draw_quad_batch_buffers<'pass>(
    render_pass: &mut wgpu::RenderPass<'pass>,
    pipelines: &'pass [WgpuQuadPipelineDrawRef],
    buffers: &'pass [WgpuQuadBatchBuffer],
    viewport: ViewportRect,
) {
    let mut bound_pipeline = None;
    for buffer in buffers {
        let Some(pipeline) = pipelines
            .iter()
            .find(|pipeline| {
                pipeline.alpha_mode == buffer.alpha_mode && pipeline.material == buffer.material
            })
            .map(|pipeline| &pipeline.pipeline)
        else {
            continue;
        };
        if bound_pipeline != Some((buffer.alpha_mode, buffer.material)) {
            render_pass.set_pipeline(pipeline);
            bound_pipeline = Some((buffer.alpha_mode, buffer.material));
        }
        let scissor = buffer.scissor.unwrap_or(WgpuScissorRect {
            x: viewport.physical_x,
            y: viewport.physical_y,
            width: viewport.physical_width,
            height: viewport.physical_height,
        });
        render_pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
        render_pass.set_bind_group(0, &buffer.bind_group.bind_group, &[]);
        if let Some(material) = &buffer.material_bind_group {
            render_pass.set_bind_group(1, material, &[]);
        }
        render_pass.set_vertex_buffer(0, buffer.buffer.slice(..));
        render_pass.draw(0..VERTICES_PER_QUAD, 0..buffer.instance_count);
    }
}

pub(crate) fn quad_batch_draw_stats(batches: &[WgpuQuadBatch]) -> WgpuQuadDrawStats {
    let mut stats = WgpuQuadDrawStats::default();
    for batch in batches {
        let instances = saturating_u32(batch.instances.len());
        if instances == 0 {
            continue;
        }
        stats.draw_calls = stats.draw_calls.saturating_add(1);
        if batch.counts_as_sprites {
            stats.sprites = stats.sprites.saturating_add(instances);
            if matches!(batch.material.texture, WgpuQuadTextureKey::Image(_)) {
                stats.image_sprites = stats.image_sprites.saturating_add(instances);
            }
        }
    }
    stats
}

pub(crate) fn admit_quad_instance_buffers(
    batches: &[WgpuQuadBatch],
) -> Result<u64, WgpuQuadInstanceBudgetError> {
    let required = batches.iter().try_fold(0_u64, |total, batch| {
        total.checked_add(quad_instance_byte_len(batch.instances.len()))
    });
    admit_quad_instance_bytes(required.unwrap_or(u64::MAX))
}

fn admit_quad_instance_bytes(required: u64) -> Result<u64, WgpuQuadInstanceBudgetError> {
    if required > MAX_QUAD_INSTANCE_BUFFER_BYTES_PER_FRAME {
        return Err(WgpuQuadInstanceBudgetError {
            required,
            maximum: MAX_QUAD_INSTANCE_BUFFER_BYTES_PER_FRAME,
        });
    }
    Ok(required)
}

fn quad_instance_buffer_layout<'a>() -> wgpu::VertexBufferLayout<'a> {
    wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<WgpuQuadInstance>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &QUAD_INSTANCE_ATTRIBUTES,
    }
}

fn quad_color_target_state(
    format: wgpu::TextureFormat,
    alpha_mode: AlphaMode2d,
) -> wgpu::ColorTargetState {
    wgpu::ColorTargetState {
        format,
        blend: match alpha_mode {
            AlphaMode2d::Opaque => None,
            AlphaMode2d::Blend => Some(wgpu::BlendState::ALPHA_BLENDING),
        },
        write_mask: wgpu::ColorWrites::ALL,
    }
}

fn saturating_u32(value: usize) -> u32 {
    value.min(u32::MAX as usize) as u32
}

fn quad_instance_byte_len(instance_count: usize) -> wgpu::BufferAddress {
    u64::try_from(instance_count)
        .unwrap_or(u64::MAX)
        .saturating_mul(std::mem::size_of::<WgpuQuadInstance>() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quad_instance_layout_matches_shader_contract() {
        let layout = quad_instance_buffer_layout();
        assert_eq!(
            layout.array_stride,
            std::mem::size_of::<WgpuQuadInstance>() as wgpu::BufferAddress
        );
        assert_eq!(layout.step_mode, wgpu::VertexStepMode::Instance);
        assert_eq!(layout.attributes.len(), 11);
        assert_eq!(layout.attributes[0].offset, 0);
        assert_eq!(layout.attributes[5].offset, 48);
        assert_eq!(layout.attributes[6].offset, 56);
        assert_eq!(layout.attributes[7].offset, 60);
        assert_eq!(layout.attributes[8].offset, 64);
        assert_eq!(layout.attributes[9].offset, 72);
        assert_eq!(layout.attributes[10].offset, 76);
        assert_eq!(quad_instance_byte_len(3), 3 * layout.array_stride);
    }

    #[test]
    fn color_target_respects_alpha_mode() {
        assert_eq!(
            quad_color_target_state(wgpu::TextureFormat::Bgra8UnormSrgb, AlphaMode2d::Opaque).blend,
            None
        );
        assert_eq!(
            quad_color_target_state(wgpu::TextureFormat::Bgra8UnormSrgb, AlphaMode2d::Blend).blend,
            Some(wgpu::BlendState::ALPHA_BLENDING)
        );
    }

    #[test]
    fn instance_buffer_budget_is_checked_without_allocating_the_payload() {
        assert_eq!(
            admit_quad_instance_bytes(MAX_QUAD_INSTANCE_BUFFER_BYTES_PER_FRAME),
            Ok(MAX_QUAD_INSTANCE_BUFFER_BYTES_PER_FRAME)
        );
        assert_eq!(
            admit_quad_instance_bytes(MAX_QUAD_INSTANCE_BUFFER_BYTES_PER_FRAME + 1),
            Err(WgpuQuadInstanceBudgetError {
                required: MAX_QUAD_INSTANCE_BUFFER_BYTES_PER_FRAME + 1,
                maximum: MAX_QUAD_INSTANCE_BUFFER_BYTES_PER_FRAME,
            })
        );
    }

    #[test]
    fn draw_stats_distinguish_image_sprites_from_fallback_quads() {
        let fallback = WgpuQuadBatch {
            material: WgpuQuadMaterialKey {
                texture: WgpuQuadTextureKey::Fallback,
                sampler: SamplerDescriptor::default(),
                alpha_mode: AlphaMode2d::Blend,
            },
            texture: WgpuQuadTexture::Fallback,
            scissor: None,
            instances: vec![WgpuQuadInstance::zeroed(); 2],
            counts_as_sprites: true,
            material_extension: None,
        };
        let image = WgpuQuadBatch {
            material: WgpuQuadMaterialKey {
                texture: WgpuQuadTextureKey::Image(RenderResourceKey::new(
                    nara_asset::allocate_anonymous_handle::<()>().unwrap().id(),
                    nara_render::RenderResourceKind::IMAGE_2D,
                )),
                ..fallback.material
            },
            instances: vec![WgpuQuadInstance::zeroed(); 3],
            ..fallback.clone()
        };
        let ui = WgpuQuadBatch {
            counts_as_sprites: false,
            ..image.clone()
        };

        let stats = quad_batch_draw_stats(&[fallback, image, ui]);

        assert_eq!(stats.draw_calls, 3);
        assert_eq!(stats.sprites, 5);
        assert_eq!(stats.image_sprites, 3);
    }
}
