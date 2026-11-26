//! WGPU rendering into a window attachment or a shared offscreen image.

mod canvas;
#[cfg(all(
    feature = "winit",
    any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd"
    )
))]
pub mod desktop;
mod device;
mod error;
mod frame_buffer;
mod output;
mod pass;
mod renderer;
#[cfg(feature = "sprite-render")]
mod sprite_tile;
#[cfg(all(
    feature = "winit",
    any(
        target_os = "windows",
        target_os = "macos",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "dragonfly",
        target_os = "netbsd",
        target_os = "openbsd"
    )
))]
mod surface;
mod texture_cache;
#[cfg(feature = "hud-runtime")]
mod ui;
mod work;

pub use device::{GpuContext, GpuDeviceErrors};
pub use error::WgpuRenderError;
pub use output::WgpuImageOutput;
pub use pass::{Attachment, PassAttachments, RenderPass, RenderPassContext, RenderTargetView};
pub use renderer::{RenderInput, RenderReport, Renderer};
#[cfg(feature = "sprite-render")]
pub use sprite_tile::SpriteTilePass;
pub use texture_cache::TextureCacheStats;
#[cfg(feature = "hud-runtime")]
pub use ui::HudPass;
pub use work::WgpuNativeWorkStats;

/// Converts a linear, straight-alpha color to the storage used by stock attachments.
pub fn premultiplied_clear_color(color: crate::Color) -> wgpu::Color {
    let alpha = f64::from(color.a);
    wgpu::Color {
        r: f64::from(color.r) * alpha,
        g: f64::from(color.g) * alpha,
        b: f64::from(color.b) * alpha,
        a: alpha,
    }
}
