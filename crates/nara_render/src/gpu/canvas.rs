use std::collections::HashMap;

use crate::WorldCanvasFrame;

use super::{RenderPassContext, WgpuRenderError, premultiplied_clear_color};

#[derive(Default)]
pub(super) struct CanvasPipelines(HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>);

#[derive(Clone, Copy)]
pub(super) struct CanvasViewport {
    pub(super) edges: [f32; 4],
}

impl CanvasViewport {
    pub(super) fn new(canvas: WorldCanvasFrame) -> Self {
        let mapping = canvas.mapping();
        let rect = mapping.content_rect();
        let scale = mapping.pixels_per_point();
        let min = rect.min.map(|point| point * scale);
        let max = rect.max.map(|point| point * scale);
        Self {
            edges: [min[0], min[1], max[0], max[1]],
        }
    }

    pub(super) fn apply(self, pass: &mut wgpu::RenderPass<'_>) -> bool {
        // Draw the integer pixel centers covered by the fractional canvas rectangle.
        let [left, top, right, bottom] = self.edges.map(|edge| (edge - 0.5).ceil().max(0.0) as u32);
        if right <= left || bottom <= top {
            return false;
        }
        let width = right - left;
        let height = bottom - top;
        pass.set_viewport(
            left as f32,
            top as f32,
            width as f32,
            height as f32,
            0.0,
            1.0,
        );
        pass.set_scissor_rect(left, top, width, height);
        true
    }
}

impl CanvasPipelines {
    pub(super) fn composite(
        &mut self,
        context: &mut RenderPassContext<'_>,
        source: &wgpu::TextureView,
        canvas: WorldCanvasFrame,
    ) -> Result<(), WgpuRenderError> {
        let pipeline = self
            .0
            .entry(context.target.format)
            .or_insert_with(|| create_pipeline(context.device, context.target.format));
        let viewport = CanvasViewport::new(canvas);
        let uniform = context.upload_buffer(
            "nara_canvas_rect",
            wgpu::BufferUsages::UNIFORM,
            bytemuck::bytes_of(&viewport.edges),
        )?;
        let binding = context
            .device
            .create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("nara_canvas_source"),
                layout: &pipeline.get_bind_group_layout(0),
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(source),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                            buffer: &uniform,
                            offset: 0,
                            size: wgpu::BufferSize::new(16),
                        }),
                    },
                ],
            });

        let mut pass = context.begin_color_pass(
            "nara_canvas_composite",
            wgpu::LoadOp::Clear(premultiplied_clear_color(canvas.bar_color())),
        );
        if viewport.apply(&mut pass) {
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &binding, &[]);
            pass.draw(0..3, 0..1);
        }
        Ok(())
    }
}

fn create_pipeline(device: &wgpu::Device, format: wgpu::TextureFormat) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("nara_canvas_shader"),
        source: wgpu::ShaderSource::Wgsl(include_str!("canvas.wgsl").into()),
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("nara_canvas_pipeline"),
        layout: None,
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        primitive: Default::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
