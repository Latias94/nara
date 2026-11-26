use std::{
    collections::VecDeque,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
};

use crate::Extent2d;

use super::{RenderTargetView, WgpuNativeWorkStats, WgpuRenderError};

const MAX_IDLE_TEXTURES: usize = 24;
const MAX_IDLE_PIXELS: u64 = 16 * 1024 * 1024;
pub(super) const MAX_ACTIVE_TEXTURES: usize = 24;

/// Cumulative checkout statistics for pooled render targets.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextureCacheStats {
    pub created: u64,
    pub reused: u64,
    pub idle: usize,
}

#[derive(Debug)]
struct CachedTexture {
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    extent: Extent2d,
    format: wgpu::TextureFormat,
}

#[derive(Debug, Default)]
struct TexturePool {
    textures: VecDeque<CachedTexture>,
    active: usize,
    created: u64,
    reused: u64,
}

#[derive(Default)]
pub(super) struct TextureCache(Arc<Mutex<TexturePool>>);

impl TextureCache {
    pub(super) fn stats(&self) -> TextureCacheStats {
        let idle = self.0.lock().unwrap_or_else(|error| error.into_inner());
        TextureCacheStats {
            created: idle.created,
            reused: idle.reused,
            idle: idle.textures.len(),
        }
    }

    pub(super) fn acquire(
        &self,
        device: &wgpu::Device,
        extent: Extent2d,
        format: wgpu::TextureFormat,
        lost: &Arc<AtomicBool>,
        work: &mut WgpuNativeWorkStats,
    ) -> Result<Arc<TextureLease>, WgpuRenderError> {
        let cached = {
            let mut idle = self.0.lock().unwrap_or_else(|error| error.into_inner());
            if idle.active == MAX_ACTIVE_TEXTURES {
                return Err(WgpuRenderError::TargetsBusy);
            }
            idle.active += 1;
            let cached = idle
                .textures
                .iter()
                .rposition(|entry| entry.extent == extent && entry.format == format)
                .and_then(|index| idle.textures.remove(index));
            if cached.is_some() {
                idle.reused += 1;
            } else {
                idle.created += 1;
            }
            cached
        };
        let cached = cached.unwrap_or_else(|| {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("nara_render_target"),
                size: wgpu::Extent3d {
                    width: extent.width,
                    height: extent.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            });
            work.created_texture_calls += 1;
            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
            CachedTexture {
                texture,
                view,
                extent,
                format,
            }
        });
        Ok(Arc::new(TextureLease {
            cache: Arc::downgrade(&self.0),
            lost: Arc::clone(lost),
            discarded: AtomicBool::new(false),
            cached: Some(cached),
        }))
    }
}

/// The producer's queue callback and any sampling consumers hold the same lease.
#[derive(Debug)]
pub(super) struct TextureLease {
    cache: Weak<Mutex<TexturePool>>,
    lost: Arc<AtomicBool>,
    discarded: AtomicBool,
    cached: Option<CachedTexture>,
}

impl TextureLease {
    pub(super) fn target(&self) -> RenderTargetView<'_> {
        let entry = self.cached.as_ref().expect("texture lease is live");
        RenderTargetView {
            view: &entry.view,
            extent: entry.extent,
            format: entry.format,
        }
    }

    pub(super) fn texture(&self) -> Option<&wgpu::Texture> {
        (!self.lost.load(Ordering::Acquire))
            .then(|| &self.cached.as_ref().expect("texture lease is live").texture)
    }

    pub(super) fn discard(&self) {
        self.discarded.store(true, Ordering::Release);
    }
}

impl Drop for TextureLease {
    fn drop(&mut self) {
        let (Some(cache), Some(texture)) = (self.cache.upgrade(), self.cached.take()) else {
            return;
        };
        let mut idle = cache.lock().unwrap_or_else(|error| error.into_inner());
        idle.active -= 1;
        if self.lost.load(Ordering::Acquire) || self.discarded.load(Ordering::Acquire) {
            return;
        }
        let pixels =
            |entry: &CachedTexture| u64::from(entry.extent.width) * u64::from(entry.extent.height);
        let new_pixels = pixels(&texture);
        if new_pixels > MAX_IDLE_PIXELS {
            return;
        }
        let mut idle_pixels = idle.textures.iter().map(pixels).sum::<u64>();
        while idle.textures.len() >= MAX_IDLE_TEXTURES || idle_pixels + new_pixels > MAX_IDLE_PIXELS
        {
            let Some(oldest) = idle.textures.pop_front() else {
                break;
            };
            idle_pixels -= pixels(&oldest);
        }
        idle.textures.push_back(texture);
    }
}
