use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex, OnceLock, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

const MAX_IDLE_TEXTURES: usize = 24;
const MAX_IDLE_BYTES: u64 = 64 * 1024 * 1024;

/// Cumulative requests for native textures used by stock render passes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextureCacheStats {
    pub created: u64,
    pub reused: u64,
    pub idle: usize,
}

#[derive(Debug, PartialEq, Eq)]
struct TextureKey {
    size: wgpu::Extent3d,
    mip_count: u32,
    sample_count: u32,
    dimension: wgpu::TextureDimension,
    format: wgpu::TextureFormat,
    usage: wgpu::TextureUsages,
    view_formats: Vec<wgpu::TextureFormat>,
}

impl From<&wgpu::TextureDescriptor<'_>> for TextureKey {
    fn from(descriptor: &wgpu::TextureDescriptor<'_>) -> Self {
        Self {
            size: descriptor.size,
            mip_count: descriptor.mip_level_count,
            sample_count: descriptor.sample_count,
            dimension: descriptor.dimension,
            format: descriptor.format,
            usage: descriptor.usage,
            view_formats: descriptor.view_formats.to_vec(),
        }
    }
}

#[derive(Debug)]
struct CachedTexture {
    key: TextureKey,
    bytes: u64,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

#[derive(Debug)]
struct IdleTextures {
    textures: VecDeque<CachedTexture>,
    bytes: u64,
    limit: u64,
    epoch: u64,
    created: u64,
    reused: u64,
}

/// Bounded spare allocations. An idle texture has no published image or pending GPU use.
#[derive(Debug)]
pub(crate) struct TextureCache(Arc<Mutex<IdleTextures>>);

impl Default for TextureCache {
    fn default() -> Self {
        Self(Arc::new(Mutex::new(IdleTextures {
            textures: VecDeque::with_capacity(MAX_IDLE_TEXTURES),
            bytes: 0,
            limit: MAX_IDLE_BYTES,
            epoch: 0,
            created: 0,
            reused: 0,
        })))
    }
}

impl TextureCache {
    pub(crate) fn begin_epoch(&self, epoch: u64) {
        let mut idle = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if idle.epoch != epoch {
            idle.textures.clear();
            idle.bytes = 0;
            idle.epoch = epoch;
        }
    }

    pub(crate) fn set_limit(&self, limit: u64) {
        let mut idle = self.0.lock().unwrap_or_else(|error| error.into_inner());
        idle.limit = limit.min(MAX_IDLE_BYTES);
        while idle.bytes > idle.limit {
            let retired = idle
                .textures
                .pop_front()
                .expect("idle bytes have a texture");
            idle.bytes -= retired.bytes;
        }
    }

    pub(crate) fn stats(&self) -> TextureCacheStats {
        let idle = self.0.lock().unwrap_or_else(|error| error.into_inner());
        TextureCacheStats {
            created: idle.created,
            reused: idle.reused,
            idle: idle.textures.len(),
        }
    }

    pub(crate) fn acquire(
        &self,
        namespace: &crate::gpu::backend::WgpuDeviceNamespace,
        descriptor: &wgpu::TextureDescriptor<'_>,
        bytes: u64,
    ) -> Result<
        (wgpu::Texture, wgpu::TextureView, TextureRecycle),
        crate::gpu::WgpuRenderProgramError,
    > {
        let key = TextureKey::from(descriptor);
        let cached = {
            let mut idle = self.0.lock().unwrap_or_else(|error| error.into_inner());
            let matching = idle.textures.iter().rposition(|texture| texture.key == key);
            matching
                .and_then(|index| idle.textures.remove(index))
                .inspect(|texture| {
                    idle.bytes -= texture.bytes;
                    idle.reused = idle.reused.saturating_add(1);
                })
        };
        let cached = match cached {
            Some(texture) => texture,
            None => {
                let (texture, view) = Self::allocate(&namespace.device, descriptor)?;
                let mut idle = self.0.lock().unwrap_or_else(|error| error.into_inner());
                idle.created = idle.created.saturating_add(1);
                CachedTexture {
                    key,
                    bytes,
                    texture,
                    view,
                }
            }
        };
        Ok((
            cached.texture.clone(),
            cached.view.clone(),
            TextureRecycle {
                cache: Arc::downgrade(&self.0),
                epoch: namespace.device_epoch,
                epoch_lost: Arc::clone(&namespace.epoch_lost),
                completion: OnceLock::new(),
                cached: Some(cached),
            },
        ))
    }

    pub(crate) fn allocate(
        device: &wgpu::Device,
        descriptor: &wgpu::TextureDescriptor<'_>,
    ) -> Result<(wgpu::Texture, wgpu::TextureView), crate::gpu::WgpuRenderProgramError> {
        let texture =
            crate::gpu::allocation::checked_resource(device, "nara_program_texture", || {
                device.create_texture(descriptor)
            })?;
        let view =
            crate::gpu::allocation::checked_resource(device, "nara_program_texture_view", || {
                texture.create_view(&wgpu::TextureViewDescriptor::default())
            })?;
        Ok((texture, view))
    }
}

/// The graph, its published output and every submitted sampler share this lease.
#[derive(Debug)]
pub(crate) struct TextureRecycle {
    cache: Weak<Mutex<IdleTextures>>,
    epoch: u64,
    epoch_lost: Arc<AtomicBool>,
    completion: OnceLock<Arc<AtomicBool>>,
    cached: Option<CachedTexture>,
}

impl TextureRecycle {
    pub(crate) fn submitted(&self, completion: &Arc<AtomicBool>) {
        let _ = self.completion.set(Arc::clone(completion));
    }
}

impl Drop for TextureRecycle {
    fn drop(&mut self) {
        let complete = self
            .completion
            .get()
            .is_some_and(|complete| complete.load(Ordering::Acquire));
        if !complete || self.epoch_lost.load(Ordering::Acquire) {
            return;
        }
        let (Some(cache), Some(texture)) = (self.cache.upgrade(), self.cached.take()) else {
            return;
        };
        let mut idle = cache.lock().unwrap_or_else(|error| error.into_inner());
        if idle.epoch != self.epoch || texture.bytes > idle.limit {
            return;
        }
        while idle.textures.len() == MAX_IDLE_TEXTURES || idle.bytes + texture.bytes > idle.limit {
            let retired = idle
                .textures
                .pop_front()
                .expect("the new texture fits on its own");
            idle.bytes -= retired.bytes;
        }
        idle.bytes += texture.bytes;
        idle.textures.push_back(texture);
    }
}
