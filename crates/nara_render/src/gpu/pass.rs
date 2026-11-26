use crate::{Color, Extent2d};

use super::{RenderInput, WgpuNativeWorkStats, WgpuRenderError, frame_buffer::FrameRingBuffer};

/// Attachment shared by the scene, HUD and editor overlay in submission order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Attachment {
    Color,
}

/// Declares the attachment dependencies for one pass.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PassAttachments {
    pub reads: &'static [Attachment],
    pub writes: &'static [Attachment],
}

impl PassAttachments {
    pub const SCENE: Self = Self {
        reads: &[],
        writes: &[Attachment::Color],
    };
    pub const OVERLAY: Self = Self {
        reads: &[Attachment::Color],
        writes: &[Attachment::Color],
    };
}

#[derive(Clone, Copy, Debug)]
pub struct RenderTargetView<'a> {
    pub view: &'a wgpu::TextureView,
    pub format: wgpu::TextureFormat,
    pub extent: Extent2d,
}

/// Records GPU commands into the renderer's current frame encoder.
pub trait RenderPass {
    fn attachments(&self) -> PassAttachments;

    /// The renderer submits the encoder after every pass succeeds.
    fn execute(&mut self, context: &mut RenderPassContext<'_>) -> Result<(), WgpuRenderError>;

    /// Drops device-bound caches (such as pipelines or bind groups) after device loss.
    fn clear_caches(&mut self) {}
}

pub struct RenderPassContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub target: RenderTargetView<'a>,
    pub frame: &'a RenderInput,
    pub(crate) work: &'a mut WgpuNativeWorkStats,
    pub(super) uploads: &'a mut FrameRingBuffer,
    #[cfg(feature = "hud-runtime")]
    pub(crate) hud: &'a mut Result<Option<crate::hud::HudPresentation>, crate::hud::HudDrawError>,
}

impl RenderPassContext<'_> {
    pub fn begin_color_pass(
        &mut self,
        label: &str,
        load: wgpu::LoadOp<wgpu::Color>,
    ) -> wgpu::RenderPass<'_> {
        begin_color_pass(self.encoder, self.target.view, label, load)
    }

    pub fn clear(&mut self, color: Color) {
        self.begin_color_pass(
            "nara_scene_clear",
            wgpu::LoadOp::Clear(super::premultiplied_clear_color(color)),
        );
    }

    /// Uploads nonempty 4-byte-aligned bytes through the renderer's pooled frame ring buffer.
    pub fn upload_buffer(
        &mut self,
        label: &'static str,
        usage: wgpu::BufferUsages,
        bytes: &[u8],
    ) -> Result<wgpu::Buffer, WgpuRenderError> {
        self.uploads
            .upload(self.device, self.encoder, self.work, label, usage, bytes)
    }

    #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
    pub(super) fn create_sampled_texture(
        &mut self,
        label: &str,
        size: wgpu::Extent3d,
        format: wgpu::TextureFormat,
    ) -> wgpu::Texture {
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage: wgpu::TextureUsages::COPY_DST | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        self.work.created_texture_calls += 1;
        texture
    }

    #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
    pub(super) fn write_texture(
        &mut self,
        target: wgpu::TexelCopyTextureInfo<'_>,
        pixels: &[u8],
        layout: wgpu::TexelCopyBufferLayout,
        extent: wgpu::Extent3d,
    ) {
        self.queue.write_texture(target, pixels, layout, extent);
        self.work.issued_texture_write_calls += 1;
    }
}

#[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
pub(super) const fn texture_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Texture {
            sample_type: wgpu::TextureSampleType::Float { filterable: true },
            view_dimension: wgpu::TextureViewDimension::D2,
            multisampled: false,
        },
        count: None,
    }
}

#[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
pub(super) const fn sampler_layout_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
        count: None,
    }
}

pub(super) fn begin_color_pass<'a>(
    encoder: &'a mut wgpu::CommandEncoder,
    target: &wgpu::TextureView,
    label: &str,
    load: wgpu::LoadOp<wgpu::Color>,
) -> wgpu::RenderPass<'a> {
    encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some(label),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: target,
            depth_slice: None,
            resolve_target: None,
            ops: wgpu::Operations {
                load,
                store: wgpu::StoreOp::Store,
            },
        })],
        ..Default::default()
    })
}
