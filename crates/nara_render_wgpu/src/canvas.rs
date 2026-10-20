//! Fixed world raster and final nearest composite, owned by the existing target lifetime.

use std::sync::Arc;

use nara_render::{Extent2d, RenderFrameSourceGeneration, WorldCanvasFrame};

use crate::{WgpuRenderError, allocation::checked_resource as checked};

pub(crate) const WORLD_CANVAS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

#[derive(Debug, Default)]
pub(crate) struct WorldCanvasCache {
    resources: Option<Arc<WorldCanvasResources>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct CanvasKey {
    extent: Extent2d,
    destination_format: wgpu::TextureFormat,
    source: RenderFrameSourceGeneration,
    device_epoch: u64,
}

#[derive(Debug)]
struct WorldCanvasResources {
    key: CanvasKey,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}

#[derive(Debug)]
pub(crate) struct PreparedWorldCanvas {
    resources: Arc<WorldCanvasResources>,
    frame: WorldCanvasFrame,
    _mapping_buffer: wgpu::Buffer,
    binding: wgpu::BindGroup,
    pub(crate) world_view_count: usize,
}

impl WorldCanvasCache {
    pub(crate) fn clear(&mut self) {
        self.resources = None;
    }

    pub(crate) fn retain_namespace(
        &mut self,
        source: RenderFrameSourceGeneration,
        device_epoch: u64,
    ) {
        if self.resources.as_ref().is_some_and(|resources| {
            resources.key.source != source || resources.key.device_epoch != device_epoch
        }) {
            self.clear();
        }
    }

    pub(crate) fn logical_bytes(&self) -> u64 {
        self.resources.as_ref().map_or(0, |resources| {
            u64::from(resources.key.extent.width) * u64::from(resources.key.extent.height) * 4
        })
    }

    pub(crate) fn prepare(
        &mut self,
        device: &wgpu::Device,
        device_epoch: u64,
        source: RenderFrameSourceGeneration,
        destination_format: wgpu::TextureFormat,
        frame: WorldCanvasFrame,
        world_view_count: usize,
    ) -> Result<PreparedWorldCanvas, WgpuRenderError> {
        let key = CanvasKey {
            extent: frame.extent(),
            destination_format,
            source,
            device_epoch,
        };
        let resources = match self
            .resources
            .as_ref()
            .filter(|resources| resources.key == key)
        {
            Some(resources) => Arc::clone(resources),
            None => Arc::new(WorldCanvasResources::create(device, key)?),
        };
        let mapping = frame.mapping();
        let rect = mapping.content_rect();
        let ppp = mapping.pixels_per_point();
        let physical = [
            rect.min[0] * ppp,
            rect.min[1] * ppp,
            rect.max[0] * ppp,
            rect.max[1] * ppp,
        ];
        let mut bytes = [0_u8; 16];
        for (chunk, value) in bytes.chunks_exact_mut(4).zip(physical) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        // Each draw owns its immutable mapping instead of overwriting a uniform retained by
        // another prepared or submitted frame during a resize.
        let mapping_buffer = crate::allocation::create_checked_buffer_with_data(
            device,
            "nara_world_canvas_mapping",
            &bytes,
            wgpu::BufferUsages::UNIFORM,
        )?;
        let binding = checked(device, "nara_world_canvas_binding", || {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("nara_world_canvas_binding"),
                layout: &resources.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&resources.view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: mapping_buffer.as_entire_binding(),
                    },
                ],
            })
        })?;
        // The target and its frame binding must both succeed before replacing the retained
        // allocation, including a late mapping-buffer or binding allocation failure.
        self.resources = Some(Arc::clone(&resources));
        Ok(PreparedWorldCanvas {
            resources,
            frame,
            _mapping_buffer: mapping_buffer,
            binding,
            world_view_count,
        })
    }
}

impl WorldCanvasResources {
    fn create(device: &wgpu::Device, key: CanvasKey) -> Result<Self, WgpuRenderError> {
        let maximum = device.limits().max_texture_dimension_2d;
        if key.extent.is_empty() || key.extent.width > maximum || key.extent.height > maximum {
            return Err(WgpuRenderError::ContentRejected {
                message: format!(
                    "world canvas {:?} exceeds device extent {maximum}",
                    key.extent
                ),
            });
        }
        let layout = checked(device, "nara_world_canvas_layout", || {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("nara_world_canvas_layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: wgpu::BufferSize::new(16),
                        },
                        count: None,
                    },
                ],
            })
        })?;
        let texture = checked(device, "nara_world_canvas_texture", || {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some("nara_world_canvas_texture"),
                size: wgpu::Extent3d {
                    width: key.extent.width,
                    height: key.extent.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: WORLD_CANVAS_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
        })?;
        let view = checked(device, "nara_world_canvas_view", || {
            texture.create_view(&wgpu::TextureViewDescriptor::default())
        })?;
        let shader = checked(device, "nara_world_canvas_shader", || {
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("nara_world_canvas_shader"),
                source: wgpu::ShaderSource::Wgsl(include_str!("canvas.wgsl").into()),
            })
        })?;
        let pipeline_layout = checked(device, "nara_world_canvas_pipeline_layout", || {
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("nara_world_canvas_pipeline_layout"),
                bind_group_layouts: &[Some(&layout)],
                immediate_size: 0,
            })
        })?;
        let pipeline = checked(device, "nara_world_canvas_pipeline", || {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("nara_world_canvas_pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_main"),
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: key.destination_format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        })?;
        Ok(Self {
            key,
            _texture: texture,
            view,
            layout,
            pipeline,
        })
    }
}

impl PreparedWorldCanvas {
    pub(crate) fn view(&self) -> &wgpu::TextureView {
        &self.resources.view
    }

    pub(crate) fn encode_composite(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        destination: &wgpu::TextureView,
    ) {
        let mapping = self.frame.mapping();
        let content = mapping.content_rect();
        let scale = mapping.pixels_per_point();
        let min = content.min.map(|point| point * scale);
        let max = content.max.map(|point| point * scale);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("nara_world_canvas_composite"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: destination,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(crate::clear_color_to_wgpu(self.frame.bar_color())),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        // The viewport transforms the oversized triangle; the scissor independently enforces
        // the same half-open pixel-center coverage used by display mapping and world picking.
        let begin = min.map(|edge| (edge - 0.5).ceil().max(0.0) as u32);
        let end = max.map(|edge| (edge - 0.5).ceil().max(0.0) as u32);
        if end[0] <= begin[0] || end[1] <= begin[1] {
            return;
        }
        // Rasterize the full integer pixel-center coverage. The exact fractional content rect
        // remains in the uniform and owns source sampling independently of viewport coverage.
        pass.set_viewport(
            begin[0] as f32,
            begin[1] as f32,
            (end[0] - begin[0]) as f32,
            (end[1] - begin[1]) as f32,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(begin[0], begin[1], end[0] - begin[0], end[1] - begin[1]);
        pass.set_pipeline(&self.resources.pipeline);
        pass.set_bind_group(0, &self.binding, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests;
