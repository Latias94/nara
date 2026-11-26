use std::collections::HashMap;
use std::ops::Range;

use crate::gpu::{
    PassAttachments, RenderPass, RenderPassContext, WgpuRenderError, premultiplied_clear_color,
};
use crate::sprite_render::batching::SpriteBatch;
use crate::{CapturedRenderTime, ExtractedView, ViewLoadOp, ViewportRect};

use geometry::{FrameGeometry, TileBuffers};
use pipeline::{Pipeline, Pipelines, UniformBindings};
use textures::SpriteTextureCache;
use view_uniform::{VIEW_UNIFORM_BYTES, WgpuViewUniform};

mod geometry;
mod pipeline;
mod textures;
mod view_uniform;

/// Draws the packet's ordered 2D views into a scene attachment.
#[derive(Default)]
pub struct SpriteTilePass {
    native: Option<NativeScene>,
}

impl RenderPass for SpriteTilePass {
    fn attachments(&self) -> PassAttachments {
        PassAttachments::SCENE
    }

    fn execute(&mut self, ctx: &mut RenderPassContext<'_>) -> Result<(), WgpuRenderError> {
        let frame = ctx.frame;
        let packet = frame.packet();
        let views = packet.views();
        if self
            .native
            .as_ref()
            .is_some_and(|native| &native.device != ctx.device)
        {
            self.native = None;
        }
        if let Some(native) = &mut self.native {
            native.frame = native.frame.saturating_add(1);
            native.textures.prune(native.frame);
            native.tiles.prune(native.frame);
            native.uniforms.prune(native.frame);
        }
        if views.is_empty() {
            ctx.clear(frame.clear_color());
            return Ok(());
        }
        for view in views {
            let viewport = view.viewport;
            if viewport.extent().is_empty()
                || u64::from(viewport.physical_x) + u64::from(viewport.physical_width)
                    > u64::from(ctx.target.extent.width)
                || u64::from(viewport.physical_y) + u64::from(viewport.physical_height)
                    > u64::from(ctx.target.extent.height)
            {
                return Err(WgpuRenderError::InvalidTarget(
                    "scene viewport is outside its attachment",
                ));
            }
        }

        let mut geometry = FrameGeometry::collect(views, frame.sprite_batches())?;
        if geometry.views.iter().all(|view| view.batches.is_empty()) {
            for view in views {
                draw_view(ctx, view, None, &[]);
            }
            return Ok(());
        }

        let native = self
            .native
            .get_or_insert_with(|| NativeScene::new(ctx.device));
        let frame = native.frame;

        let uniform_bytes = pack_uniforms(
            &mut geometry,
            views,
            packet.captured_time(),
            ctx.device.limits().min_uniform_buffer_offset_alignment,
        )?;
        let uniforms = (!uniform_bytes.is_empty())
            .then(|| {
                ctx.upload_buffer(
                    "nara_scene_uniforms",
                    wgpu::BufferUsages::UNIFORM,
                    &uniform_bytes,
                )
            })
            .transpose()?;
        let dynamic = (!geometry.dynamic.is_empty())
            .then(|| {
                ctx.upload_buffer(
                    "nara_sprite_instances",
                    wgpu::BufferUsages::VERTEX,
                    bytemuck::cast_slice(&geometry.dynamic),
                )
            })
            .transpose()?;
        let chunks =
            native
                .tiles
                .prepare(ctx, packet.source_generation(), frame, geometry.chunks)?;

        for (view, geometry) in views.iter().zip(geometry.views) {
            let draws = geometry
                .batches
                .into_iter()
                .map(|draw| {
                    let buffer = match draw.batch.tile_chunk {
                        Some(chunk) => chunks.get(&chunk),
                        None => dynamic.as_ref(),
                    }
                    .ok_or(WgpuRenderError::Capacity(
                        "scene instance buffer is missing",
                    ))?;
                    native.prepare_draw(
                        ctx,
                        draw.batch,
                        buffer,
                        draw.instances,
                        uniforms.as_ref().zip(draw.uniform_offset),
                    )
                })
                .collect::<Result<Vec<_>, _>>()?;
            let view_binding = uniforms
                .as_ref()
                .zip(geometry.uniform_offset)
                .map(|(buffer, offset)| {
                    let layout = native.pipelines.view_layout.as_ref().ok_or_else(|| {
                        WgpuRenderError::Validation("the 2D material view layout is missing".into())
                    })?;
                    Ok::<_, WgpuRenderError>(native.uniforms.bind(
                        ctx.device,
                        layout,
                        wgpu::BufferBinding {
                            buffer,
                            offset,
                            size: wgpu::BufferSize::new(VIEW_UNIFORM_BYTES),
                        },
                        Vec::new(),
                        frame,
                    ))
                })
                .transpose()?;
            draw_view(ctx, view, view_binding.as_ref(), &draws);
        }
        Ok(())
    }
}

struct NativeScene {
    device: wgpu::Device,
    frame: u64,
    textures: SpriteTextureCache,
    pipelines: Pipelines,
    tiles: TileBuffers,
    uniforms: UniformBindings,
}

impl NativeScene {
    fn new(device: &wgpu::Device) -> Self {
        Self {
            device: device.clone(),
            frame: 0,
            textures: SpriteTextureCache::new(device),
            pipelines: Pipelines::default(),
            tiles: TileBuffers::default(),
            uniforms: UniformBindings::default(),
        }
    }

    fn prepare_draw(
        &mut self,
        ctx: &mut RenderPassContext<'_>,
        batch: &SpriteBatch,
        buffer: &wgpu::Buffer,
        instances: Range<u32>,
        uniform: Option<(&wgpu::Buffer, u64)>,
    ) -> Result<Draw, WgpuRenderError> {
        let material = batch.material_extension.as_deref();
        let Pipeline {
            native,
            material_layout,
        } = self.pipelines.get(
            ctx.device,
            &self.textures.layout,
            ctx.target.format,
            batch.material.alpha_mode,
            material.map(|material| material.binding.generation()),
            self.frame,
        )?;
        let texture = self.textures.bind(
            ctx,
            batch.image.as_ref(),
            batch.material.sampler,
            self.frame,
        )?;
        let material = match (material, material_layout, uniform) {
            (Some(material), Some(layout), Some((buffer, offset))) => {
                let views = material
                    .textures
                    .iter()
                    .map(|(binding, image)| {
                        self.textures
                            .view(ctx, image, self.frame)
                            .map(|view| (*binding, view))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let size = material.binding.uniform_bytes().len() as u64;
                Some(self.uniforms.bind(
                    ctx.device,
                    &layout,
                    wgpu::BufferBinding {
                        buffer,
                        offset,
                        size: wgpu::BufferSize::new(size),
                    },
                    views,
                    self.frame,
                ))
            }
            (None, None, None) => None,
            _ => {
                return Err(WgpuRenderError::Validation(
                    "incomplete 2D material bindings".into(),
                ));
            }
        };
        Ok(Draw {
            pipeline: native,
            buffer: buffer.clone(),
            instances,
            texture,
            material,
        })
    }
}

fn pack_uniforms(
    geometry: &mut FrameGeometry<'_>,
    views: &[ExtractedView],
    time: Option<CapturedRenderTime>,
    alignment: u32,
) -> Result<Vec<u8>, WgpuRenderError> {
    let alignment = alignment.max(4) as usize;
    let mut bytes = Vec::new();
    let mut material_offsets = HashMap::new();
    for (geometry, view) in geometry.views.iter_mut().zip(views) {
        for draw in &mut geometry.batches {
            let Some(material) = draw.batch.material_extension.as_ref() else {
                continue;
            };
            if geometry.uniform_offset.is_none() {
                let uniform = WgpuViewUniform::capture(view, time);
                geometry.uniform_offset = Some(append_uniform(
                    &mut bytes,
                    alignment,
                    bytemuck::bytes_of(&uniform),
                )?);
            }
            let offset = match material_offsets.entry(std::sync::Arc::as_ptr(material)) {
                std::collections::hash_map::Entry::Occupied(entry) => *entry.get(),
                std::collections::hash_map::Entry::Vacant(entry) => {
                    pipeline::validate_capture(material)?;
                    *entry.insert(append_uniform(
                        &mut bytes,
                        alignment,
                        material.binding.uniform_bytes(),
                    )?)
                }
            };
            draw.uniform_offset = Some(offset);
        }
    }
    Ok(bytes)
}

fn append_uniform(
    bytes: &mut Vec<u8>,
    alignment: usize,
    data: &[u8],
) -> Result<u64, WgpuRenderError> {
    let offset = bytes
        .len()
        .checked_next_multiple_of(alignment)
        .ok_or(WgpuRenderError::Capacity("scene uniforms are too large"))?;
    let end = offset
        .checked_add(data.len())
        .ok_or(WgpuRenderError::Capacity("scene uniforms are too large"))?;
    bytes
        .try_reserve(end - bytes.len())
        .map_err(|_| WgpuRenderError::Capacity("scene uniforms are too large"))?;
    bytes.resize(offset, 0);
    bytes.extend_from_slice(data);
    Ok(offset as u64)
}

struct Draw {
    pipeline: wgpu::RenderPipeline,
    buffer: wgpu::Buffer,
    instances: Range<u32>,
    texture: wgpu::BindGroup,
    material: Option<wgpu::BindGroup>,
}

fn draw_view(
    ctx: &mut RenderPassContext<'_>,
    view: &ExtractedView,
    view_uniform: Option<&wgpu::BindGroup>,
    draws: &[Draw],
) {
    if draws.is_empty() && view.load == ViewLoadOp::Load {
        return;
    }
    let mut pass = ctx.begin_color_pass(
        "nara_sprite_tile",
        match view.load {
            ViewLoadOp::Clear(color) => wgpu::LoadOp::Clear(premultiplied_clear_color(color)),
            ViewLoadOp::Load => wgpu::LoadOp::Load,
        },
    );
    let ViewportRect {
        physical_x,
        physical_y,
        physical_width,
        physical_height,
    } = view.viewport;
    pass.set_viewport(
        physical_x as f32,
        physical_y as f32,
        physical_width as f32,
        physical_height as f32,
        0.0,
        1.0,
    );
    pass.set_scissor_rect(physical_x, physical_y, physical_width, physical_height);
    let mut pipeline = None;
    for draw in draws {
        if pipeline != Some(&draw.pipeline) {
            pass.set_pipeline(&draw.pipeline);
            pipeline = Some(&draw.pipeline);
        }
        pass.set_bind_group(0, &draw.texture, &[]);
        if let Some((material, view)) = draw.material.as_ref().zip(view_uniform) {
            pass.set_bind_group(1, material, &[]);
            pass.set_bind_group(2, view, &[]);
        }
        pass.set_vertex_buffer(0, draw.buffer.slice(..));
        pass.draw(0..6, draw.instances.clone());
    }
}

#[cfg(test)]
mod tests;
