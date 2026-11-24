//! Shared nearest composite for graph-owned world and destination textures.

mod program;

pub(crate) use program::{CanvasCompositeCapture, CanvasCompositeFeature};

use nara_render::WorldCanvasFrame;

use crate::{WgpuRenderError, allocation::checked_resource as checked};

pub(crate) const WORLD_CANVAS_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const CANVAS_MAPPING_BYTES: u64 = 16;

/// Shared native program; the owner supplies both source and destination textures.
#[derive(Debug)]
struct CanvasCompositePipeline {
    destination_format: wgpu::TextureFormat,
    layout: wgpu::BindGroupLayout,
    pipeline: wgpu::RenderPipeline,
}

#[derive(Debug)]
struct CanvasCompositeMapping {
    frame: WorldCanvasFrame,
    buffer: wgpu::Buffer,
    binding: wgpu::BindGroup,
}

impl CanvasCompositePipeline {
    fn create(
        device: &wgpu::Device,
        destination_format: wgpu::TextureFormat,
    ) -> Result<Self, WgpuRenderError> {
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
                            min_binding_size: wgpu::BufferSize::new(CANVAS_MAPPING_BYTES),
                        },
                        count: None,
                    },
                ],
            })
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
                        format: destination_format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        })?;
        Ok(Self {
            destination_format,
            layout,
            pipeline,
        })
    }

    fn prepare_mapping(
        &self,
        device: &wgpu::Device,
        source: &wgpu::TextureView,
        frame: WorldCanvasFrame,
    ) -> Result<CanvasCompositeMapping, WgpuRenderError> {
        let mapping = frame.mapping();
        let rect = mapping.content_rect();
        let ppp = mapping.pixels_per_point();
        let physical = [
            rect.min[0] * ppp,
            rect.min[1] * ppp,
            rect.max[0] * ppp,
            rect.max[1] * ppp,
        ];
        let mut bytes = [0_u8; CANVAS_MAPPING_BYTES as usize];
        for (chunk, value) in bytes.chunks_exact_mut(4).zip(physical) {
            chunk.copy_from_slice(&value.to_le_bytes());
        }
        // Each draw owns its immutable mapping instead of overwriting a uniform retained by
        // another prepared or submitted frame during a resize.
        let buffer = crate::allocation::create_checked_buffer_with_data(
            device,
            "nara_world_canvas_mapping",
            &bytes,
            wgpu::BufferUsages::UNIFORM,
        )?;
        let binding = checked(device, "nara_world_canvas_binding", || {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("nara_world_canvas_binding"),
                layout: &self.layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(source),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: buffer.as_entire_binding(),
                    },
                ],
            })
        })?;
        Ok(CanvasCompositeMapping {
            frame,
            buffer,
            binding,
        })
    }

    fn encode_composite(
        &self,
        prepared: &CanvasCompositeMapping,
        encoder: &mut wgpu::CommandEncoder,
        destination: &wgpu::TextureView,
        load: wgpu::LoadOp<wgpu::Color>,
    ) {
        let mapping = prepared.frame.mapping();
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
                    load,
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
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &prepared.binding, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod program_tests;
