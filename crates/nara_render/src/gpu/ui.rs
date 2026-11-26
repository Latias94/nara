use std::{collections::HashMap, mem::size_of, sync::Arc};

use crate::gpu::{PassAttachments, RenderPass, RenderPassContext, WgpuRenderError};
use crate::hud::{
    HudDrawError, HudPresentation,
    atlas::{GlyphAtlas, GlyphError},
    paint::{Geometry, PaintError, Scissor, Vertex},
};

mod textures;
use textures::Textures;

/// Draws game HUD paint over the existing color attachment, in compiler paint order.
#[derive(Default)]
pub struct HudPass {
    atlas: GlyphAtlas,
    geometry: Geometry,
    native: Option<NativeHud>,
}

impl HudPass {
    fn clear_glyph_textures(&mut self) {
        if let Some(native) = &mut self.native {
            native.textures.clear_glyphs();
        }
    }
}

impl RenderPass for HudPass {
    fn attachments(&self) -> PassAttachments {
        PassAttachments::OVERLAY
    }

    fn execute(&mut self, ctx: &mut RenderPassContext<'_>) -> Result<(), WgpuRenderError> {
        let Some(hud) = ctx.frame.hud() else {
            return Ok(());
        };
        let extent = ctx.target.extent;
        let surface = hud
            .frame()
            .environment()
            .content_transform()
            .physical_surface_size();
        if extent.is_empty()
            || surface.width().round() as u32 != extent.width
            || surface.height().round() as u32 != extent.height
        {
            *ctx.hud = Err(HudDrawError::ExtentMismatch);
            return Ok(());
        }
        if self.atlas.set_fonts(hud.fonts()) {
            self.clear_glyph_textures();
        }
        let mut prepared = self.geometry.prepare(hud, extent, &mut self.atlas);
        if matches!(prepared, Err(PaintError::Glyph(GlyphError::Full))) {
            self.atlas.clear();
            self.clear_glyph_textures();
            prepared = self.geometry.prepare(hud, extent, &mut self.atlas);
        }
        // A rejected HUD must not prevent the scene from presenting. No hit geometry is
        // returned for this candidate because its paint was never drawn.
        if let Err(error) = prepared {
            *ctx.hud = Err(HudDrawError::Paint(error.to_string()));
            return Ok(());
        }
        let presentation = HudPresentation {
            frame: Arc::clone(hud.frame()),
            glyph_count: self.geometry.glyph_count,
        };

        if !self.geometry.draws.is_empty() {
            if self
                .native
                .as_ref()
                .is_some_and(|native| native.device != *ctx.device)
            {
                self.native = None;
            }
            let native = self.native.get_or_insert_with(|| NativeHud {
                device: ctx.device.clone(),
                textures: Textures::new(ctx.device),
                pipelines: HashMap::new(),
            });
            let pipeline = native
                .pipelines
                .entry(ctx.target.format)
                .or_insert_with(|| {
                    pipeline(ctx.device, &native.textures.layout, ctx.target.format)
                });
            native
                .textures
                .prepare(ctx, &self.geometry, &mut self.atlas)?;
            let vertices = ctx.upload_buffer(
                "nara_hud_vertices",
                wgpu::BufferUsages::VERTEX,
                bytemuck::cast_slice(&self.geometry.vertices),
            )?;
            let indices = ctx.upload_buffer(
                "nara_hud_indices",
                wgpu::BufferUsages::INDEX,
                bytemuck::cast_slice(&self.geometry.indices),
            )?;
            let mut pass = ctx.begin_color_pass("nara_hud", wgpu::LoadOp::Load);
            pass.set_pipeline(pipeline);
            pass.set_vertex_buffer(0, vertices.slice(..));
            pass.set_index_buffer(indices.slice(..), wgpu::IndexFormat::Uint32);
            for draw in &self.geometry.draws {
                let Scissor([left, top, right, bottom]) = draw.clip;
                pass.set_scissor_rect(left, top, right - left, bottom - top);
                pass.set_bind_group(0, native.textures.binding(draw.texture)?, &[]);
                pass.draw_indexed(draw.indices.clone(), 0, 0..1);
            }
        }

        *ctx.hud = Ok(Some(presentation));
        Ok(())
    }
}

struct NativeHud {
    device: wgpu::Device,
    textures: Textures,
    pipelines: HashMap<wgpu::TextureFormat, wgpu::RenderPipeline>,
}

fn pipeline(
    device: &wgpu::Device,
    textures: &wgpu::BindGroupLayout,
    format: wgpu::TextureFormat,
) -> wgpu::RenderPipeline {
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("nara_hud"),
        source: wgpu::ShaderSource::Wgsl(include_str!("ui/hud.wgsl").into()),
    });
    let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("nara_hud"),
        bind_group_layouts: &[Some(textures)],
        immediate_size: 0,
    });
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("nara_hud"),
        layout: Some(&layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[Some(wgpu::VertexBufferLayout {
                array_stride: size_of::<Vertex>() as u64,
                step_mode: wgpu::VertexStepMode::Vertex,
                attributes: &wgpu::vertex_attr_array![
                    0 => Float32x2, 1 => Float32x2, 2 => Float32x4, 3 => Float32x2,
                    4 => Float32x2, 5 => Float32x4, 6 => Float32, 7 => Uint32
                ],
            })],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: Default::default(),
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}
