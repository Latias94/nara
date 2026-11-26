use nara_window::{
    PresentMode, WindowId,
    backend::{BackendWindowHandles, WindowSurfaceLease, WindowTargetError},
};

use crate::Extent2d;

use super::GpuContext;

#[derive(Debug, thiserror::Error)]
pub(super) enum SurfaceError {
    #[error(transparent)]
    Target(#[from] WindowTargetError),
    #[error("could not create the WGPU surface: {0}")]
    Create(#[from] wgpu::CreateSurfaceError),
    #[error("the WGPU surface does not support the requested extent or format")]
    Unsupported,
    #[error("WGPU could not validate surface acquisition")]
    Validation,
}

pub(super) struct DesktopSurface {
    pub window: WindowId,
    pub handle: wgpu::Surface<'static>,
    lease: WindowSurfaceLease,
    configured: Option<ConfiguredSurface>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ConfiguredSurface {
    extent: Extent2d,
    requested: PresentMode,
    format: wgpu::TextureFormat,
    present_mode: wgpu::PresentMode,
}

pub(super) enum SurfaceAcquisition {
    Ready(SurfaceFrame),
    Retry,
    Occluded,
    Reconfigure,
    Lost,
}

pub(super) struct SurfaceFrame {
    pub texture: wgpu::SurfaceTexture,
    configured: ConfiguredSurface,
}

impl SurfaceFrame {
    pub fn format(&self) -> wgpu::TextureFormat {
        self.configured.format
    }

    pub fn present_mode(&self) -> wgpu::PresentMode {
        self.configured.present_mode
    }
}

impl DesktopSurface {
    pub fn new(
        instance: &wgpu::Instance,
        handles: &BackendWindowHandles,
        window: WindowId,
    ) -> Result<Self, SurfaceError> {
        let (source, lease) = handles.acquire_surface(window)?.into_parts();
        Ok(Self {
            window,
            handle: instance.create_surface(source)?,
            lease,
            configured: None,
        })
    }

    pub fn generation(&self) -> u64 {
        self.lease.surface_generation()
    }

    pub fn configure(
        &mut self,
        context: &GpuContext,
        extent: Extent2d,
        requested: PresentMode,
    ) -> Result<bool, SurfaceError> {
        if self
            .configured
            .is_some_and(|config| config.extent == extent && config.requested == requested)
        {
            return Ok(false);
        }
        let maximum = context.device().limits().max_texture_dimension_2d;
        let caps = self.handle.get_capabilities(context.adapter());
        let config =
            configuration(&caps, extent, requested, maximum).ok_or(SurfaceError::Unsupported)?;
        self.handle.configure(context.device(), &config);
        self.configured = Some(ConfiguredSurface {
            extent,
            requested,
            format: config.format.add_srgb_suffix(),
            present_mode: config.present_mode,
        });
        Ok(true)
    }

    pub fn acquire(&mut self) -> Result<SurfaceAcquisition, SurfaceError> {
        use wgpu::CurrentSurfaceTexture;

        if !self.lease.can_acquire_frame() {
            return Ok(SurfaceAcquisition::Lost);
        }
        let Some(configured) = self.configured else {
            return Ok(SurfaceAcquisition::Reconfigure);
        };
        let texture = match self.handle.get_current_texture() {
            CurrentSurfaceTexture::Success(texture) => texture,
            CurrentSurfaceTexture::Suboptimal(texture) => {
                self.invalidate();
                texture
            }
            CurrentSurfaceTexture::Timeout => return Ok(SurfaceAcquisition::Retry),
            CurrentSurfaceTexture::Occluded => return Ok(SurfaceAcquisition::Occluded),
            CurrentSurfaceTexture::Outdated => {
                self.invalidate();
                return Ok(SurfaceAcquisition::Reconfigure);
            }
            CurrentSurfaceTexture::Lost => return Ok(SurfaceAcquisition::Lost),
            CurrentSurfaceTexture::Validation => return Err(SurfaceError::Validation),
        };
        Ok(SurfaceAcquisition::Ready(SurfaceFrame {
            texture,
            configured,
        }))
    }

    pub fn invalidate(&mut self) {
        self.configured = None;
    }

    pub fn release(self) -> Result<(), WindowTargetError> {
        drop(self.handle);
        self.lease.confirm_owner_dropped()
    }
}

fn configuration(
    caps: &wgpu::SurfaceCapabilities,
    extent: Extent2d,
    requested: PresentMode,
    maximum: u32,
) -> Option<wgpu::SurfaceConfiguration> {
    if extent.is_empty() || extent.width > maximum || extent.height > maximum {
        return None;
    }
    let format = *caps.formats.first()?;
    let first_mode = *caps.present_modes.first()?;
    let requested = match requested {
        PresentMode::AutoVsync => wgpu::PresentMode::AutoVsync,
        PresentMode::AutoNoVsync => wgpu::PresentMode::AutoNoVsync,
        PresentMode::Fifo => wgpu::PresentMode::Fifo,
        PresentMode::Immediate => wgpu::PresentMode::Immediate,
        PresentMode::Mailbox => wgpu::PresentMode::Mailbox,
    };
    let present_mode = if matches!(
        requested,
        wgpu::PresentMode::AutoVsync | wgpu::PresentMode::AutoNoVsync
    ) || caps.present_modes.contains(&requested)
    {
        requested
    } else if caps.present_modes.contains(&wgpu::PresentMode::Fifo) {
        wgpu::PresentMode::Fifo
    } else {
        first_mode
    };
    Some(wgpu::SurfaceConfiguration {
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        format,
        color_space: wgpu::SurfaceColorSpace::Auto,
        width: extent.width,
        height: extent.height,
        desired_maximum_frame_latency: 2,
        present_mode,
        alpha_mode: wgpu::CompositeAlphaMode::Auto,
        view_formats: vec![format.add_srgb_suffix()],
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_supported_presentation_and_uses_an_srgb_render_view() {
        let caps = wgpu::SurfaceCapabilities {
            formats: vec![
                wgpu::TextureFormat::Bgra8Unorm,
                wgpu::TextureFormat::Bgra8UnormSrgb,
            ],
            present_modes: vec![wgpu::PresentMode::Immediate, wgpu::PresentMode::Fifo],
            ..Default::default()
        };
        let extent = Extent2d::new(320, 180).unwrap();
        for (requested, expected) in [
            (PresentMode::Immediate, wgpu::PresentMode::Immediate),
            (PresentMode::Mailbox, wgpu::PresentMode::Fifo),
            (PresentMode::AutoNoVsync, wgpu::PresentMode::AutoNoVsync),
        ] {
            let config = configuration(&caps, extent, requested, 320).unwrap();
            assert_eq!(config.present_mode, expected);
            assert_eq!(config.format, wgpu::TextureFormat::Bgra8Unorm);
            assert_eq!(config.view_formats, [wgpu::TextureFormat::Bgra8UnormSrgb]);
            assert_eq!(config.usage, wgpu::TextureUsages::RENDER_ATTACHMENT);
        }
    }

    #[test]
    fn zero_oversized_and_unsupported_targets_cannot_configure() {
        let caps = wgpu::SurfaceCapabilities {
            formats: vec![wgpu::TextureFormat::Bgra8UnormSrgb],
            present_modes: vec![wgpu::PresentMode::Fifo],
            ..Default::default()
        };
        for extent in [
            Extent2d {
                width: 0,
                height: 1,
            },
            Extent2d {
                width: 1,
                height: 0,
            },
            Extent2d {
                width: 320,
                height: 321,
            },
        ] {
            assert!(configuration(&caps, extent, PresentMode::Fifo, 320).is_none());
        }
        let extent = Extent2d::new(320, 180).unwrap();
        assert!(configuration(&Default::default(), extent, PresentMode::Fifo, 320).is_none());
    }
}
