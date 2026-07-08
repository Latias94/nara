//! Wgpu backend skeleton for nara render-domain data.

use std::collections::BTreeMap;

use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_ecs::{Query, Res, ResMut, Resource};
use nara_render::{
    Color, Extent2d, ExtractedViews, FrameStats, RenderFrame, RenderPlugin, RenderTarget,
};
use nara_window::{
    PresentMode, PrimaryWindowId, Window, WindowId,
    backend::{BackendWindowHandles, RawWindowHandleProvider},
};
use thiserror::Error;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuRenderPlugin;

impl Plugin for WgpuRenderPlugin {
    fn build(&self, app: &mut App) {
        add_plugin_or_ignore_duplicate(app, RenderPlugin);
        app.init_resource::<WgpuRenderBackend>();
        app.add_systems(CoreStage::Render, render_clear_passes);
    }

    fn cleanup(&self, app: &mut App) {
        if let Some(mut backend) = app.world_mut().get_resource_mut::<WgpuRenderBackend>() {
            backend.clear_surfaces();
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum WgpuBackendState {
    #[default]
    Uninitialized,
    Initializing,
    Ready,
    Unavailable,
}

#[derive(Debug, Resource)]
pub struct WgpuRenderBackend {
    state: WgpuBackendState,
    instance: Option<wgpu::Instance>,
    adapter: Option<wgpu::Adapter>,
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,
    surfaces: BTreeMap<WindowId, WgpuSurfaceState>,
    last_error: Option<String>,
}

impl Default for WgpuRenderBackend {
    fn default() -> Self {
        Self {
            state: WgpuBackendState::Uninitialized,
            instance: None,
            adapter: None,
            device: None,
            queue: None,
            surfaces: BTreeMap::new(),
            last_error: None,
        }
    }
}

impl WgpuRenderBackend {
    #[must_use]
    pub fn state(&self) -> WgpuBackendState {
        self.state
    }

    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref()
    }

    #[must_use]
    pub fn surface_count(&self) -> usize {
        self.surfaces.len()
    }

    pub fn clear_surfaces(&mut self) {
        self.surfaces.clear();
    }

    fn render_clear_passes(
        &mut self,
        handles: &BackendWindowHandles,
        windows: &Query<&Window>,
        views: &ExtractedViews,
        primary_window_id: Option<WindowId>,
        frame: &mut RenderFrame,
        stats: &mut FrameStats,
    ) -> Result<(), WgpuRenderError> {
        stats.draw_calls = 0;
        stats.sprites = 0;

        if views.is_empty() {
            frame.mark_skipped();
            return Ok(());
        }

        self.ensure_device()?;

        let mut submitted_any = false;
        for view in views.as_slice() {
            let Some(window_id) = target_window_id(view.target, primary_window_id) else {
                continue;
            };
            let Some(window) = windows.iter().find(|window| window.id == window_id) else {
                continue;
            };
            let Some(provider) = handles.get(window_id) else {
                continue;
            };

            let color = view.clear_color;
            if self.render_window_clear_pass(window, provider, color)? {
                submitted_any = true;
            }
        }

        if submitted_any {
            frame.mark_submitted();
        } else {
            frame.mark_skipped();
        }

        Ok(())
    }

    fn ensure_device(&mut self) -> Result<(), WgpuRenderError> {
        if matches!(self.state, WgpuBackendState::Ready) {
            return Ok(());
        }

        self.state = WgpuBackendState::Initializing;
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
                .map_err(|error| WgpuRenderError::AdapterUnavailable {
                message: error.to_string(),
            })?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .map_err(|error| WgpuRenderError::DeviceUnavailable {
                    message: error.to_string(),
                })?;

        self.instance = Some(instance);
        self.adapter = Some(adapter);
        self.device = Some(device);
        self.queue = Some(queue);
        self.state = WgpuBackendState::Ready;
        self.last_error = None;
        Ok(())
    }

    fn render_window_clear_pass(
        &mut self,
        window: &Window,
        provider: &RawWindowHandleProvider,
        clear_color: Color,
    ) -> Result<bool, WgpuRenderError> {
        let Some(size) = surface_extent(
            window.resolution.physical_width,
            window.resolution.physical_height,
        ) else {
            return Ok(false);
        };

        self.ensure_surface(window, provider, size)?;

        let device = self
            .device
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        let queue = self
            .queue
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        let surface_state =
            self.surfaces
                .get_mut(&window.id)
                .ok_or(WgpuRenderError::SurfaceMissing {
                    window_id: window.id,
                })?;

        match surface_state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => {
                render_acquired_texture(
                    device,
                    queue,
                    window.id,
                    surface_state,
                    texture,
                    clear_color,
                )?;
                Ok(true)
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                drop(texture);
                surface_state.dirty = true;
                Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                surface_state.dirty = true;
                Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surfaces.remove(&window.id);
                Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Validation => Err(WgpuRenderError::SurfaceValidation {
                window_id: window.id,
            }),
        }
    }

    fn ensure_surface(
        &mut self,
        window: &Window,
        provider: &RawWindowHandleProvider,
        size: Extent2d,
    ) -> Result<(), WgpuRenderError> {
        if !self.surfaces.contains_key(&window.id) {
            let instance = self
                .instance
                .as_ref()
                .ok_or(WgpuRenderError::BackendNotReady)?;
            let surface = create_surface(instance, provider, window.id)?;
            self.surfaces.insert(
                window.id,
                WgpuSurfaceState {
                    surface,
                    config: None,
                    size,
                    dirty: true,
                },
            );
        }

        let adapter = self
            .adapter
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        let device = self
            .device
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        let surface = self
            .surfaces
            .get_mut(&window.id)
            .ok_or(WgpuRenderError::SurfaceMissing {
                window_id: window.id,
            })?;

        if surface_resize_action(surface.size, size) == SurfaceResizeAction::Reconfigure(size)
            || surface.dirty
        {
            configure_surface(
                surface,
                adapter,
                device,
                window.id,
                window.present_mode,
                size,
            )?;
        }

        Ok(())
    }

    fn mark_error(&mut self, error: &WgpuRenderError) {
        self.state = WgpuBackendState::Unavailable;
        self.last_error = Some(error.to_string());
    }
}

#[derive(Debug)]
struct WgpuSurfaceState {
    surface: wgpu::Surface<'static>,
    config: Option<wgpu::SurfaceConfiguration>,
    size: Extent2d,
    dirty: bool,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum WgpuRenderError {
    #[error("wgpu backend is not ready")]
    BackendNotReady,
    #[error("wgpu adapter unavailable: {message}")]
    AdapterUnavailable { message: String },
    #[error("wgpu device unavailable: {message}")]
    DeviceUnavailable { message: String },
    #[error("wgpu surface creation failed for window {window_id:?}: {message}")]
    SurfaceCreation {
        window_id: WindowId,
        message: String,
    },
    #[error("wgpu surface is missing for window {window_id:?}")]
    SurfaceMissing { window_id: WindowId },
    #[error("wgpu surface unsupported for window {window_id:?}")]
    SurfaceUnsupported { window_id: WindowId },
    #[error("wgpu surface for window {window_id:?} has no configuration")]
    SurfaceUnconfigured { window_id: WindowId },
    #[error("wgpu surface validation error for window {window_id:?}")]
    SurfaceValidation { window_id: WindowId },
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

pub fn render_clear_passes(
    mut backend: ResMut<WgpuRenderBackend>,
    handles: Res<BackendWindowHandles>,
    windows: Query<&Window>,
    views: Res<ExtractedViews>,
    primary_window_id: Option<Res<PrimaryWindowId>>,
    mut frame: ResMut<RenderFrame>,
    mut stats: ResMut<FrameStats>,
) {
    let primary_window_id = primary_window_id.map(|resource| resource.0);
    let result = backend.render_clear_passes(
        &handles,
        &windows,
        &views,
        primary_window_id,
        &mut frame,
        &mut stats,
    );

    if let Err(error) = result {
        backend.mark_error(&error);
        frame.mark_skipped();
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

fn add_plugin_or_ignore_duplicate(app: &mut App, plugin: impl Plugin) {
    match app.add_plugin(plugin) {
        Ok(_) | Err(PluginError::Duplicate { .. }) => {}
        Err(error) => panic!("failed to install wgpu prerequisite plugin: {error}"),
    }
}

fn target_window_id(target: RenderTarget, primary_window_id: Option<WindowId>) -> Option<WindowId> {
    target.window_id(primary_window_id)
}

fn surface_extent(width: u32, height: u32) -> Option<Extent2d> {
    Extent2d::new(width, height)
}

fn create_surface(
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

fn configure_surface(
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

fn render_acquired_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    window_id: WindowId,
    surface_state: &WgpuSurfaceState,
    surface_texture: wgpu::SurfaceTexture,
    clear_color: Color,
) -> Result<(), WgpuRenderError> {
    let config = surface_state
        .config
        .as_ref()
        .ok_or(WgpuRenderError::SurfaceUnconfigured { window_id })?;
    let view = surface_texture
        .texture
        .create_view(&wgpu::TextureViewDescriptor {
            format: Some(config.format.add_srgb_suffix()),
            ..Default::default()
        });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("nara_wgpu_clear_encoder"),
    });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("nara_wgpu_clear_pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color_to_wgpu(clear_color)),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
    }

    queue.submit([encoder.finish()]);
    queue.present(surface_texture);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_starts_uninitialized() {
        let backend = WgpuRenderBackend::default();

        assert_eq!(backend.state(), WgpuBackendState::Uninitialized);
        assert_eq!(backend.surface_count(), 0);
        assert_eq!(backend.last_error(), None);
    }

    #[test]
    fn zero_size_surfaces_skip_configuration() {
        let current = Extent2d {
            width: 1280,
            height: 720,
        };

        assert_eq!(
            surface_resize_action(
                current,
                Extent2d {
                    width: 0,
                    height: 720,
                },
            ),
            SurfaceResizeAction::SkipZeroSized
        );
        assert_eq!(
            surface_resize_action(
                current,
                Extent2d {
                    width: 1280,
                    height: 720,
                },
            ),
            SurfaceResizeAction::Unchanged
        );
        assert_eq!(
            surface_resize_action(
                current,
                Extent2d {
                    width: 640,
                    height: 480,
                },
            ),
            SurfaceResizeAction::Reconfigure(Extent2d {
                width: 640,
                height: 480,
            })
        );
    }

    #[test]
    fn surface_status_policy_covers_wgpu_30_statuses() {
        assert_eq!(
            surface_acquire_policy(SurfaceTextureStatus::Success),
            SurfaceAcquireAction::Render
        );
        assert_eq!(
            surface_acquire_policy(SurfaceTextureStatus::Suboptimal),
            SurfaceAcquireAction::Reconfigure
        );
        assert_eq!(
            surface_acquire_policy(SurfaceTextureStatus::Outdated),
            SurfaceAcquireAction::Reconfigure
        );
        assert_eq!(
            surface_acquire_policy(SurfaceTextureStatus::Timeout),
            SurfaceAcquireAction::SkipFrame
        );
        assert_eq!(
            surface_acquire_policy(SurfaceTextureStatus::Occluded),
            SurfaceAcquireAction::SkipFrame
        );
        assert_eq!(
            surface_acquire_policy(SurfaceTextureStatus::Lost),
            SurfaceAcquireAction::RecreateSurface
        );
        assert_eq!(
            surface_acquire_policy(SurfaceTextureStatus::Validation),
            SurfaceAcquireAction::Error
        );
    }

    #[test]
    fn maps_present_modes() {
        assert_eq!(
            map_present_mode(PresentMode::AutoVsync),
            wgpu::PresentMode::AutoVsync
        );
        assert_eq!(
            map_present_mode(PresentMode::AutoNoVsync),
            wgpu::PresentMode::AutoNoVsync
        );
        assert_eq!(map_present_mode(PresentMode::Fifo), wgpu::PresentMode::Fifo);
        assert_eq!(
            map_present_mode(PresentMode::Immediate),
            wgpu::PresentMode::Immediate
        );
        assert_eq!(
            map_present_mode(PresentMode::Mailbox),
            wgpu::PresentMode::Mailbox
        );
    }

    #[test]
    fn unsupported_strict_present_modes_fall_back_to_fifo() {
        assert_eq!(
            choose_present_mode(PresentMode::Immediate, &[wgpu::PresentMode::Fifo]),
            wgpu::PresentMode::Fifo
        );
        assert_eq!(
            choose_present_mode(PresentMode::Mailbox, &[wgpu::PresentMode::Immediate]),
            wgpu::PresentMode::Immediate
        );
        assert_eq!(
            choose_present_mode(PresentMode::AutoNoVsync, &[]),
            wgpu::PresentMode::AutoNoVsync
        );
    }

    #[test]
    fn clear_color_conversion_is_deterministic() {
        let color = clear_color_to_wgpu(Color::rgba(0.25, 0.5, 0.75, 1.0));

        assert_eq!(color.r, 0.25);
        assert_eq!(color.g, 0.5);
        assert_eq!(color.b, 0.75);
        assert_eq!(color.a, 1.0);
    }
}
