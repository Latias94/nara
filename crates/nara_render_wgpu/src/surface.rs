use nara_render::{Color, Extent2d};
use nara_window::{
    PresentMode, WindowId,
    backend::{WindowSurfaceHandleSource, WindowSurfaceLease},
};

#[cfg(test)]
use nara_window::backend::WindowSurfaceBinding;

use crate::WgpuRenderError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SurfaceDropReason {
    TargetShutdown,
    BackendCleanup,
    SurfaceOrDeviceLost,
}

impl SurfaceDropReason {
    pub(crate) const fn requests_target_retirement(self) -> bool {
        matches!(self, Self::TargetShutdown)
    }
}

#[derive(Debug)]
enum SurfaceOwner {
    Wgpu(wgpu::Surface<'static>),
    #[cfg(test)]
    LeaseOnly {
        _handle_source: WindowSurfaceHandleSource,
    },
}

#[derive(Debug)]
struct LiveSurface {
    owner: SurfaceOwner,
    lease: WindowSurfaceLease,
}

#[derive(Debug)]
pub(crate) struct WgpuSurfaceState {
    live: Option<LiveSurface>,
    pub(crate) config: Option<wgpu::SurfaceConfiguration>,
    pub(crate) size: Extent2d,
    pub(crate) dirty: bool,
}

impl WgpuSurfaceState {
    pub(crate) fn new(
        surface: wgpu::Surface<'static>,
        target: WindowSurfaceLease,
        size: Extent2d,
    ) -> Self {
        Self {
            live: Some(LiveSurface {
                owner: SurfaceOwner::Wgpu(surface),
                lease: target,
            }),
            config: None,
            size,
            dirty: true,
        }
    }

    #[cfg(test)]
    pub(crate) fn lease_only(binding: WindowSurfaceBinding, size: Extent2d) -> Self {
        let (handle_source, target) = binding.into_parts();
        Self {
            live: Some(LiveSurface {
                owner: SurfaceOwner::LeaseOnly {
                    _handle_source: handle_source,
                },
                lease: target,
            }),
            config: None,
            size,
            dirty: true,
        }
    }

    pub(crate) fn can_acquire_frame(&self) -> bool {
        self.live
            .as_ref()
            .is_some_and(|live| live.lease.can_acquire_frame())
    }

    pub(crate) fn retirement_requested(&self) -> Result<bool, WgpuRenderError> {
        self.live
            .as_ref()
            .map(|live| live.lease.retirement_requested())
            .transpose()
            .map(Option::unwrap_or_default)
            .map_err(Into::into)
    }

    pub(crate) fn surface(
        &self,
        window_id: WindowId,
    ) -> Result<&wgpu::Surface<'static>, WgpuRenderError> {
        match self.live.as_ref().map(|live| &live.owner) {
            Some(SurfaceOwner::Wgpu(surface)) => Ok(surface),
            #[cfg(test)]
            Some(SurfaceOwner::LeaseOnly { .. }) => {
                Err(WgpuRenderError::SurfaceMissing { window_id })
            }
            None => Err(WgpuRenderError::SurfaceMissing { window_id }),
        }
    }

    pub(crate) fn retire(mut self, reason: SurfaceDropReason) -> Result<(), WgpuRenderError> {
        self.retire_inner(reason.requests_target_retirement())
    }

    fn retire_inner(&mut self, request_target_retirement: bool) -> Result<(), WgpuRenderError> {
        let Some(LiveSurface { owner, lease }) = self.live.take() else {
            return Ok(());
        };

        let request_result = if request_target_retirement {
            lease.request_retirement().map(|_| ()).map_err(Into::into)
        } else {
            Ok(())
        };

        drop(owner);
        let acknowledgement = lease
            .confirm_owner_dropped()
            .map(|_| ())
            .map_err(Into::into);
        request_result.and(acknowledgement)
    }
}

impl Drop for WgpuSurfaceState {
    fn drop(&mut self) {
        let _ = self.retire_inner(false);
    }
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

/// Present mode stored in the active wgpu surface configuration.
///
/// This reports the backend's configured mode rather than the project request. Auto modes may
/// still select a platform-dependent fallback internally; evidence that requires disabled VSync
/// should require [`Self::Immediate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WgpuConfiguredPresentMode {
    AutoVsync,
    AutoNoVsync,
    Fifo,
    FifoRelaxed,
    Immediate,
    Mailbox,
}

impl WgpuConfiguredPresentMode {
    pub(crate) const fn from_wgpu(mode: wgpu::PresentMode) -> Self {
        match mode {
            wgpu::PresentMode::AutoVsync => Self::AutoVsync,
            wgpu::PresentMode::AutoNoVsync => Self::AutoNoVsync,
            wgpu::PresentMode::Fifo => Self::Fifo,
            wgpu::PresentMode::FifoRelaxed => Self::FifoRelaxed,
            wgpu::PresentMode::Immediate => Self::Immediate,
            wgpu::PresentMode::Mailbox => Self::Mailbox,
        }
    }

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AutoVsync => "auto-vsync",
            Self::AutoNoVsync => "auto-no-vsync",
            Self::Fifo => "fifo",
            Self::FifoRelaxed => "fifo-relaxed",
            Self::Immediate => "immediate",
            Self::Mailbox => "mailbox",
        }
    }
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
    handle_source: WindowSurfaceHandleSource,
    window_id: WindowId,
) -> Result<wgpu::Surface<'static>, WgpuRenderError> {
    instance
        .create_surface(handle_source)
        .map_err(|error| WgpuRenderError::SurfaceCreation {
            window_id,
            message: error.to_string(),
        })
}

pub(crate) fn configure_surface(
    surface_state: &mut WgpuSurfaceState,
    adapter: &wgpu::Adapter,
    device: &wgpu::Device,
    window_id: WindowId,
    present_mode: PresentMode,
    size: Extent2d,
) -> Result<(), WgpuRenderError> {
    let surface = surface_state.surface(window_id)?;
    let capabilities = surface.get_capabilities(adapter);
    let Some(mut config) = surface.get_default_config(adapter, size.width, size.height) else {
        return Err(WgpuRenderError::SurfaceUnsupported { window_id });
    };

    config.present_mode = choose_present_mode(present_mode, &capabilities.present_modes);
    config.view_formats = vec![config.format.add_srgb_suffix()];
    surface.configure(device, &config);
    surface_state.size = size;
    surface_state.config = Some(config);
    surface_state.dirty = false;
    Ok(())
}
