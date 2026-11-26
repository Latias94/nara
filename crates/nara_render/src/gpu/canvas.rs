use std::collections::HashMap;

use crate::WorldCanvasFrame;

use super::{RenderPassContext, WgpuRenderError, premultiplied_clear_color};

#[derive(Default)]
pub(super) struct CanvasPipelines(HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>);

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
        let mapping = canvas.mapping();
        let rect = mapping.content_rect();
        let scale = mapping.pixels_per_point();
        let min = rect.min.map(|point| point * scale);
        let max = rect.max.map(|point| point * scale);
        let edges = [min[0], min[1], max[0], max[1]];
        let uniform = context.upload_buffer(
            "nara_canvas_rect",
            wgpu::BufferUsages::UNIFORM,
            bytemuck::bytes_of(&edges),
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
        // Draw the integer pixel centers covered by the fractional canvas rectangle.
        let begin = min.map(|edge| (edge - 0.5).ceil().max(0.0) as u32);
        let end = max.map(|edge| (edge - 0.5).ceil().max(0.0) as u32);
        if end[0] > begin[0] && end[1] > begin[1] {
            let width = end[0] - begin[0];
            let height = end[1] - begin[1];
            pass.set_viewport(
                begin[0] as f32,
                begin[1] as f32,
                width as f32,
                height as f32,
                0.0,
                1.0,
            );
            pass.set_scissor_rect(begin[0], begin[1], width, height);
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
