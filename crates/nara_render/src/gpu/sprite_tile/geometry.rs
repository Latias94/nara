use std::{collections::HashMap, mem::size_of, ops::Range};

use bytemuck::{Pod, Zeroable};
use nara_ecs::Entity;

use crate::gpu::{RenderPassContext, WgpuRenderError};
use crate::sprite_render::SpriteTileChunk;
use crate::sprite_render::batching::{SpriteBatch, SpriteInstance};
use crate::tilemap::TileChunkCoord;
use crate::{ExtractedView, RenderFrameSourceGeneration, RenderViewId};

const MAX_CACHED_CHUNKS: usize = 1024;
const CHUNK_GRACE_FRAMES: u64 = 2;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub(super) struct QuadInstance {
    center: [f32; 2],
    x_axis: [f32; 2],
    y_axis: [f32; 2],
    color: [f32; 4],
    uv_min: [f32; 2],
    uv_size: [f32; 2],
}

impl From<&SpriteInstance> for QuadInstance {
    fn from(sprite: &SpriteInstance) -> Self {
        let color = sprite.color;
        Self {
            center: sprite.center.to_array(),
            x_axis: sprite.x_axis.to_array(),
            y_axis: sprite.y_axis.to_array(),
            color: [color.r, color.g, color.b, color.a],
            uv_min: sprite.uv.min.to_array(),
            uv_size: sprite.uv.size.to_array(),
        }
    }
}

pub(super) fn vertex_layout() -> wgpu::VertexBufferLayout<'static> {
    const ATTRIBUTES: [wgpu::VertexAttribute; 6] = wgpu::vertex_attr_array![
        0 => Float32x2, 1 => Float32x2, 2 => Float32x2, 3 => Float32x4,
        4 => Float32x2, 5 => Float32x2
    ];
    wgpu::VertexBufferLayout {
        array_stride: size_of::<QuadInstance>() as u64,
        step_mode: wgpu::VertexStepMode::Instance,
        attributes: &ATTRIBUTES,
    }
}

pub(super) struct FrameGeometry<'a> {
    pub(super) views: Vec<ViewGeometry<'a>>,
    pub(super) dynamic: Vec<QuadInstance>,
    pub(super) chunks: HashMap<SpriteTileChunk, Vec<QuadInstance>>,
}

#[derive(Default)]
pub(super) struct ViewGeometry<'a> {
    pub(super) batches: Vec<BatchGeometry<'a>>,
    pub(super) uniform_offset: Option<u64>,
}

pub(super) struct BatchGeometry<'a> {
    pub(super) batch: &'a SpriteBatch,
    pub(super) instances: Range<u32>,
    pub(super) uniform_offset: Option<u64>,
}

impl<'a> FrameGeometry<'a> {
    pub(super) fn collect(
        views: &[ExtractedView],
        batches: &'a [SpriteBatch],
    ) -> Result<Self, WgpuRenderError> {
        let mut frame = Self {
            views: (0..views.len()).map(|_| ViewGeometry::default()).collect(),
            dynamic: Vec::new(),
            chunks: HashMap::new(),
        };
        for batch in batches {
            let valid = views.get(batch.view_index).is_some_and(|view| {
                view.target == batch.target
                    && view.order == batch.view_order
                    && batch.tile_chunk.is_none_or(|chunk| chunk.view == view.id)
            });
            if !valid {
                return Err(WgpuRenderError::InvalidTarget(
                    "a sprite batch does not belong to its captured view",
                ));
            }
            if batch.instances.is_empty() {
                continue;
            }
            let instances = match batch.tile_chunk {
                Some(chunk) => frame.chunks.entry(chunk).or_default(),
                None => &mut frame.dynamic,
            };
            let capacity = || WgpuRenderError::Capacity("scene instances are too large");
            let start = u32::try_from(instances.len()).map_err(|_| capacity())?;
            let count = u32::try_from(batch.instances.len()).map_err(|_| capacity())?;
            let end = start.checked_add(count).ok_or_else(capacity)?;
            instances
                .try_reserve(batch.instances.len())
                .map_err(|_| capacity())?;
            instances.extend(batch.instances.iter().map(QuadInstance::from));
            frame.views[batch.view_index].batches.push(BatchGeometry {
                batch,
                instances: start..end,
                uniform_offset: None,
            });
        }
        Ok(frame)
    }
}

#[derive(Default)]
pub(super) struct TileBuffers {
    resident: HashMap<TileBufferKey, TileBuffer>,
}

#[derive(PartialEq, Eq, Hash)]
struct TileBufferKey {
    source: RenderFrameSourceGeneration,
    view: RenderViewId,
    entity: Entity,
    coord: TileChunkCoord,
}

struct TileBuffer {
    revision: u64,
    instances: Vec<QuadInstance>,
    buffer: wgpu::Buffer,
    used_at: u64,
}

impl TileBuffers {
    pub(super) fn prune(&mut self, frame: u64) {
        self.resident
            .retain(|_, entry| frame.saturating_sub(entry.used_at) <= CHUNK_GRACE_FRAMES);
    }

    pub(super) fn prepare(
        &mut self,
        ctx: &mut RenderPassContext<'_>,
        source: RenderFrameSourceGeneration,
        frame: u64,
        chunks: HashMap<SpriteTileChunk, Vec<QuadInstance>>,
    ) -> Result<HashMap<SpriteTileChunk, wgpu::Buffer>, WgpuRenderError> {
        let mut prepared = HashMap::with_capacity(chunks.len());
        if chunks.is_empty() {
            return Ok(prepared);
        }
        let max_buffer_size = ctx.device.limits().max_buffer_size;
        for (chunk, instances) in chunks {
            let key = TileBufferKey {
                source,
                view: chunk.view,
                entity: chunk.entity,
                coord: chunk.change.coord,
            };
            let bytes: &[u8] = bytemuck::cast_slice(&instances);
            if let Some(entry) = self.resident.get_mut(&key)
                && entry.revision == chunk.change.revision
                && bytemuck::cast_slice::<_, u8>(&entry.instances) == bytes
            {
                entry.used_at = frame;
                prepared.insert(chunk, entry.buffer.clone());
                continue;
            }
            let size = bytes.len() as u64;
            if size > max_buffer_size {
                return Err(WgpuRenderError::Capacity(
                    "tile chunk exceeds the device buffer limit",
                ));
            }
            let buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("nara_tile_chunk_instances"),
                size,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            ctx.work.created_buffer_calls = ctx.work.created_buffer_calls.saturating_add(1);
            ctx.queue.write_buffer(&buffer, 0, bytes);
            ctx.work.issued_buffer_write_calls =
                ctx.work.issued_buffer_write_calls.saturating_add(1);
            prepared.insert(chunk, buffer.clone());
            if self.resident.len() < MAX_CACHED_CHUNKS || self.resident.contains_key(&key) {
                self.resident.insert(
                    key,
                    TileBuffer {
                        revision: chunk.change.revision,
                        instances,
                        buffer,
                        used_at: frame,
                    },
                );
            }
        }
        Ok(prepared)
    }
}
