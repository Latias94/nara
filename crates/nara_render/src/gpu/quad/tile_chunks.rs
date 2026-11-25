use std::{
    collections::{HashMap, hash_map::DefaultHasher},
    hash::{Hash, Hasher},
    sync::Arc,
};

use crate::sprite_render::SpriteTileChunk;

use crate::gpu::preparation::{NativePreparation, NativePreparationError, WgpuFeatureAllocation};

use super::WgpuQuadInstance;

const MAX_RESIDENT_TILE_CHUNKS: usize = 1024;
const UNUSED_GRACE_FRAMES: u64 = 2;

#[derive(Debug)]
pub(crate) struct WgpuTileChunkInstances {
    pub(super) instances: Box<[WgpuQuadInstance]>,
    source: SpriteTileChunk,
    fingerprint: u64,
}

impl WgpuTileChunkInstances {
    pub(crate) fn new(source: SpriteTileChunk, instances: Vec<WgpuQuadInstance>) -> Self {
        let mut hasher = DefaultHasher::new();
        bytemuck::cast_slice::<_, u8>(&instances).hash(&mut hasher);
        Self {
            instances: instances.into_boxed_slice(),
            source,
            fingerprint: hasher.finish(),
        }
    }

    pub(crate) fn byte_len(&self) -> u64 {
        super::quad_instance_byte_len(self.instances.len())
    }
}

impl PartialEq for WgpuTileChunkInstances {
    fn eq(&self, other: &Self) -> bool {
        self.source == other.source
            && self.fingerprint == other.fingerprint
            && bytemuck::cast_slice::<_, u8>(&self.instances)
                == bytemuck::cast_slice::<_, u8>(&other.instances)
    }
}

impl Eq for WgpuTileChunkInstances {}

impl Hash for WgpuTileChunkInstances {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.source.hash(state);
        self.fingerprint.hash(state);
    }
}

pub(super) type TileChunkBuffer = (wgpu::Buffer, WgpuFeatureAllocation);

#[derive(Debug)]
struct CachedTileChunk {
    buffer: TileChunkBuffer,
    last_used: u64,
}

#[derive(Debug, Default)]
pub(crate) struct WgpuTileChunkCache {
    resident: HashMap<Arc<WgpuTileChunkInstances>, CachedTileChunk>,
}

/// One preparation resolves each immutable chunk once, including when the resident cache is full.
pub(super) type PreparedTileChunks = HashMap<*const WgpuTileChunkInstances, TileChunkBuffer>;

impl WgpuTileChunkCache {
    pub(crate) fn begin_frame(&mut self, frame_index: u64) {
        self.resident
            .retain(|_, entry| frame_index.saturating_sub(entry.last_used) <= UNUSED_GRACE_FRAMES);
    }

    pub(crate) fn clear(&mut self) {
        self.resident.clear();
    }

    pub(crate) fn contains(&self, chunk: &WgpuTileChunkInstances) -> bool {
        self.resident.contains_key(chunk)
    }

    pub(super) fn prepare(
        &mut self,
        preparation: &mut NativePreparation<'_>,
        prepared: &mut PreparedTileChunks,
        chunk: &Arc<WgpuTileChunkInstances>,
        frame_index: u64,
    ) -> Result<TileChunkBuffer, NativePreparationError> {
        let identity = Arc::as_ptr(chunk);
        if let Some(buffer) = prepared.get(&identity) {
            return Ok(buffer.clone());
        }
        prepared
            .try_reserve(1)
            .map_err(|_| NativePreparationError::ResourceCapacity)?;
        let buffer = if let Some(entry) = self.resident.get_mut(chunk.as_ref()) {
            preparation.retain(entry.buffer.clone())?;
            entry.last_used = frame_index;
            entry.buffer.clone()
        } else {
            let cacheable = self.resident.len() < MAX_RESIDENT_TILE_CHUNKS
                && self.resident.try_reserve(1).is_ok();
            let buffer = preparation.create_buffer_with_data(
                "nara_wgpu_tile_chunk_instances",
                bytemuck::cast_slice(&chunk.instances),
                wgpu::BufferUsages::VERTEX,
            )?;
            if cacheable {
                self.resident.insert(
                    Arc::clone(chunk),
                    CachedTileChunk {
                        buffer: buffer.clone(),
                        last_used: frame_index,
                    },
                );
            }
            buffer
        };
        prepared.insert(identity, buffer.clone());
        Ok(buffer)
    }
}
