use std::sync::Arc;

use crate::Extent2d;

use super::{RenderTargetView, texture_cache::TextureLease};

/// A pooled offscreen image in sRGB with premultiplied alpha.
///
/// Sample its texture on the renderer's queue. When passing it to another renderer, retain a
/// clone as long as that renderer can sample it, including until its last submitted use completes.
/// The producer already holds its own reference until the render submission completes.
#[derive(Clone, Debug)]
pub struct WgpuImageOutput(pub(super) Arc<TextureLease>);

impl WgpuImageOutput {
    pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;

    pub fn extent(&self) -> Extent2d {
        self.0.target().extent
    }

    /// Returns the texture while its device is usable.
    pub fn native_texture(&self) -> Option<&wgpu::Texture> {
        self.0.texture()
    }

    pub fn native_view(&self) -> Option<&wgpu::TextureView> {
        self.native_texture().map(|_| {
            let RenderTargetView { view, .. } = self.0.target();
            view
        })
    }
}
