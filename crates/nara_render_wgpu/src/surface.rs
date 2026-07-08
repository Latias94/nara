use nara_render::{Color, Extent2d};
use nara_window::{PresentMode, WindowId, backend::RawWindowHandleProvider};

use crate::WgpuRenderError;

#[derive(Debug)]
pub(crate) struct WgpuSurfaceState {
    pub(crate) surface: wgpu::Surface<'static>,
    pub(crate) config: Option<wgpu::SurfaceConfiguration>,
    pub(crate) size: Extent2d,
    pub(crate) dirty: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceResizeAction {
    SkipZeroSized,
    Unchanged,
    Reconfigure(Extent2d),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceTextureStatus {
    Success,
    Suboptimal,
    Timeout,
    Occluded,
    Outdated,
    Lost,
    Validation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SurfaceAcquireAction {
    Render,
    Reconfigure,
    SkipFrame,
    RecreateSurface,
    Error,
}

#[must_use]
pub fn surface_resize_action(current: Extent2d, next: Extent2d) -> SurfaceResizeAction {
    if next.is_empty() {
        SurfaceResizeAction::SkipZeroSized
    } else if current == next {
        SurfaceResizeAction::Unchanged
    } else {
        SurfaceResizeAction::Reconfigure(next)
    }
}

#[must_use]
pub fn surface_acquire_policy(status: SurfaceTextureStatus) -> SurfaceAcquireAction {
    match status {
        SurfaceTextureStatus::Success => SurfaceAcquireAction::Render,
        SurfaceTextureStatus::Suboptimal | SurfaceTextureStatus::Outdated => {
            SurfaceAcquireAction::Reconfigure
        }
        SurfaceTextureStatus::Timeout | SurfaceTextureStatus::Occluded => {
            SurfaceAcquireAction::SkipFrame
        }
        SurfaceTextureStatus::Lost => SurfaceAcquireAction::RecreateSurface,
        SurfaceTextureStatus::Validation => SurfaceAcquireAction::Error,
    }
}

#[must_use]
pub fn map_present_mode(mode: PresentMode) -> wgpu::PresentMode {
    match mode {
        PresentMode::AutoVsync => wgpu::PresentMode::AutoVsync,
        PresentMode::AutoNoVsync => wgpu::PresentMode::AutoNoVsync,
        PresentMode::Fifo => wgpu::PresentMode::Fifo,
        PresentMode::Immediate => wgpu::PresentMode::Immediate,
        PresentMode::Mailbox => wgpu::PresentMode::Mailbox,
    }
}

#[must_use]
pub fn choose_present_mode(
    requested: PresentMode,
    supported: &[wgpu::PresentMode],
) -> wgpu::PresentMode {
    let requested = map_present_mode(requested);
    if matches!(
        requested,
        wgpu::PresentMode::AutoVsync | wgpu::PresentMode::AutoNoVsync
    ) || supported.contains(&requested)
    {
        return requested;
    }

    if supported.contains(&wgpu::PresentMode::Fifo) {
        wgpu::PresentMode::Fifo
    } else {
        supported
            .first()
            .copied()
            .unwrap_or(wgpu::PresentMode::AutoVsync)
    }
}

#[must_use]
pub fn clear_color_to_wgpu(color: Color) -> wgpu::Color {
    wgpu::Color {
        r: color.r as f64,
        g: color.g as f64,
        b: color.b as f64,
        a: color.a as f64,
    }
}

pub(crate) fn surface_extent(width: u32, height: u32) -> Option<Extent2d> {
    Extent2d::new(width, height)
}

pub(crate) fn create_surface(
    instance: &wgpu::Instance,
    provider: &RawWindowHandleProvider,
    window_id: WindowId,
) -> Result<wgpu::Surface<'static>, WgpuRenderError> {
    // SAFETY: `provider` stores a strong guard for the platform window object.
    // `WgpuRenderPlugin::cleanup` drops surfaces before app/world teardown drops
    // the backend handle providers.
    unsafe {
        instance
            .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::RawHandle {
                raw_display_handle: Some(provider.display_handle()),
                raw_window_handle: provider.window_handle(),
            })
            .map_err(|error| WgpuRenderError::SurfaceCreation {
                window_id,
                message: error.to_string(),
            })
    }
}

pub(crate) fn configure_surface(
    surface_state: &mut WgpuSurfaceState,
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    window_id: WindowId,
    present_mode: PresentMode,
    size: Extent2d,
) -> Result<(), WgpuRenderError> {
    let capabilities = surface_state.surface.get_capabilities(adapter);
    let Some(mut config) =
        surface_state
            .surface
            .get_default_config(adapter, size.width, size.height)
    else {
        return Err(WgpuRenderError::SurfaceUnsupported { window_id });
    };

    config.present_mode = choose_present_mode(present_mode, &capabilities.present_modes);
    config.view_formats = vec![config.format.add_srgb_suffix()];
    surface_state.surface.configure(device, &config);
    surface_state.size = size;
    surface_state.config = Some(config);
    surface_state.dirty = false;
    Ok(())
}

pub(crate) fn target_window_id(
    target: nara_render::RenderTarget,
    primary_window_id: Option<WindowId>,
) -> Option<WindowId> {
    target.window_id(primary_window_id)
}
