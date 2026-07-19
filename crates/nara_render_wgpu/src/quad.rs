use crate::texture::{WgpuSpriteTextureCache, WgpuSpriteTextureError};
use bytemuck::{Pod, Zeroable};
use nara_asset::Assets;
use nara_image::{ImageAsset, PreparedImageResource};
use nara_material::{AlphaMode2d, SamplerDescriptor};
use nara_render::{PreparedRenderResources, RenderPhaseLabel, RenderResourceKey, ViewportRect};
use wgpu::util::DeviceExt;

const VERTICES_PER_QUAD: u32 = 6;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable)]
pub(crate) struct WgpuQuadInstance {
    pub(crate) center: [f32; 2],
    pub(crate) x_axis: [f32; 2],
    pub(crate) y_axis: [f32; 2],
    pub(crate) color: [f32; 4],
    pub(crate) uv_min: [f32; 2],
    pub(crate) uv_size: [f32; 2],
}

const QUAD_INSTANCE_ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
    0 => Float32x2,
    1 => Float32x2,
    2 => Float32x2,
    3 => Float32x4,
    4 => Float32x2,
    5 => Float32x2
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct WgpuQuadMaterialKey {
    pub(crate) image: Option<RenderResourceKey>,
    pub(crate) sampler: SamplerDescriptor,
    pub(crate) alpha_mode: AlphaMode2d,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct WgpuQuadBatch {
    pub(crate) phase: RenderPhaseLabel,
    pub(crate) material: WgpuQuadMaterialKey,
    pub(crate) scissor: Option<WgpuScissorRect>,
    pub(crate) instances: Vec<WgpuQuadInstance>,
    pub(crate) counts_as_sprites: bool,
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
}

#[derive(Debug, Clone)]
pub(crate) struct WgpuQuadPipelineDrawRef {
    pub(crate) alpha_mode: AlphaMode2d,
    pub(crate) pipeline: wgpu::RenderPipeline,
}

#[derive(Debug)]
pub(crate) struct WgpuQuadBatchBuffer {
    buffer: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    phase: RenderPhaseLabel,
    alpha_mode: AlphaMode2d,
    scissor: Option<WgpuScissorRect>,
    instance_count: u32,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct WgpuQuadDrawStats {
    pub(crate) draw_calls: u32,
    pub(crate) sprites: u32,
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
) -> WgpuQuadPipeline {
    let shader = device.create_shader_module(wgpu::include_wgsl!("sprite.wgsl"));
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("nara_wgpu_quad_pipeline_layout"),
        bind_group_layouts: &[Some(texture_bind_group_layout)],
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

    WgpuQuadPipeline { key, pipeline }
}

pub(crate) fn create_quad_batch_buffers(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    batches: &[WgpuQuadBatch],
    texture_layout: &wgpu::BindGroupLayout,
    texture_cache: &mut WgpuSpriteTextureCache,
    images: Option<&Assets<ImageAsset>>,
    prepared_images: Option<&PreparedRenderResources<PreparedImageResource>>,
    frame_index: u64,
) -> Result<Vec<WgpuQuadBatchBuffer>, WgpuSpriteTextureError> {
    batches
        .iter()
        .filter(|batch| !batch.instances.is_empty())
        .map(|batch| {
            let bind_group = match batch.material.image {
                Some(key) => texture_cache.image_bind_group(
                    device,
                    queue,
                    texture_layout,
                    batch.material,
                    images.ok_or(WgpuSpriteTextureError::MissingImageAsset { key })?,
                    prepared_images
                        .ok_or(WgpuSpriteTextureError::MissingPreparedResource { key })?,
                    frame_index,
                )?,
                None => texture_cache.fallback_bind_group(
                    device,
                    queue,
                    texture_layout,
                    batch.material,
                    frame_index,
                ),
            };
            let instance_count = saturating_u32(batch.instances.len());
            let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("nara_wgpu_quad_instances"),
                contents: bytemuck::cast_slice(&batch.instances),
                usage: wgpu::BufferUsages::VERTEX,
            });

            Ok(WgpuQuadBatchBuffer {
                buffer,
                bind_group,
                phase: batch.phase,
                alpha_mode: batch.material.alpha_mode,
                scissor: batch.scissor,
                instance_count,
            })
        })
        .collect()
}

pub(crate) fn draw_quad_batch_buffers_for_phase<'pass>(
    render_pass: &mut wgpu::RenderPass<'pass>,
    pipelines: &'pass [WgpuQuadPipelineDrawRef],
    buffers: &'pass [WgpuQuadBatchBuffer],
    phase: RenderPhaseLabel,
    viewport: ViewportRect,
) {
    if buffers.iter().all(|buffer| buffer.phase != phase) {
        return;
    }

    let mut bound_alpha_mode = None::<AlphaMode2d>;
    for buffer in buffers.iter().filter(|buffer| buffer.phase == phase) {
        let Some(pipeline) = pipelines
            .iter()
            .find(|pipeline| pipeline.alpha_mode == buffer.alpha_mode)
            .map(|pipeline| &pipeline.pipeline)
        else {
            continue;
        };
        if bound_alpha_mode != Some(buffer.alpha_mode) {
            render_pass.set_pipeline(pipeline);
            bound_alpha_mode = Some(buffer.alpha_mode);
        }
        let scissor = buffer.scissor.unwrap_or(WgpuScissorRect {
            x: viewport.physical_x,
            y: viewport.physical_y,
            width: viewport.physical_width,
            height: viewport.physical_height,
        });
        render_pass.set_scissor_rect(scissor.x, scissor.y, scissor.width, scissor.height);
        render_pass.set_bind_group(0, &buffer.bind_group, &[]);
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
        }
    }
    stats
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
        assert_eq!(layout.attributes.len(), 6);
        assert_eq!(layout.attributes[0].offset, 0);
        assert_eq!(layout.attributes[5].offset, 48);
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
}
