//! Wgpu backend skeleton for nara render-domain data.

use std::collections::BTreeMap;

mod sprite;
mod surface;
mod texture;

use crate::sprite::{
    WgpuSpriteBatchBuffer, WgpuSpriteDrawStats, WgpuSpritePipeline, create_sprite_batch_buffers,
    create_sprite_pipeline, draw_sprite_batch_buffers, sprite_batch_draw_stats,
};
use crate::surface::{WgpuSurfaceState, configure_surface, create_surface, surface_extent};
use crate::texture::WgpuSpriteTextureCache;
use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_asset::Assets;
use nara_ecs::{Query, Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use nara_image::{ImageAsset, PreparedImageResource};
use nara_render::{
    Color, Extent2d, ExtractedViews, FrameStats, PreparedRenderResources, RenderFrame,
    begin_render_frame,
};
use nara_sprite_render::{SpriteBatch, SpriteBatches, SpriteRenderPlugin};
use nara_window::{
    PrimaryWindowId, Window, WindowId,
    backend::{BackendWindowHandles, RawWindowHandleProvider},
};
use thiserror::Error;

pub use crate::surface::{
    SurfaceAcquireAction, SurfaceResizeAction, SurfaceTextureStatus, choose_present_mode,
    clear_color_to_wgpu, map_present_mode, surface_acquire_policy, surface_resize_action,
};

#[cfg(test)]
use nara_window::PresentMode;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuRenderPlugin;

impl Plugin for WgpuRenderPlugin {
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugin_if_missing(SpriteRenderPlugin)?;
        app.init_resource::<WgpuRenderBackend>();
        app.add_systems(
            CoreStage::Render,
            render_clear_passes.after(begin_render_frame),
        );
        Ok(())
    }

    fn cleanup(&self, app: &mut App) {
        if let Some(mut backend) = app.world_mut().get_resource_mut::<WgpuRenderBackend>() {
            backend.clear_gpu_resources();
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
    sprite_pipelines: Vec<WgpuSpritePipeline>,
    sprite_textures: WgpuSpriteTextureCache,
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
            sprite_pipelines: Vec::new(),
            sprite_textures: WgpuSpriteTextureCache::default(),
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

    pub fn clear_gpu_resources(&mut self) {
        self.clear_surfaces();
        self.sprite_pipelines.clear();
        self.sprite_textures.clear();
    }

    #[must_use]
    pub fn sprite_texture_count(&self) -> usize {
        self.sprite_textures.image_count()
    }

    fn render_clear_passes(
        &mut self,
        handles: &BackendWindowHandles,
        windows: &Query<&Window>,
        views: &ExtractedViews,
        sprite_batches: &SpriteBatches,
        images: Option<&Assets<ImageAsset>>,
        prepared_images: Option<&PreparedRenderResources<PreparedImageResource>>,
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
        self.sprite_textures.prune_unused(sprite_batches.as_slice());

        let mut submitted_any = false;
        for (view_index, view) in views.as_slice().iter().enumerate() {
            let Some(window_id) = crate::surface::target_window_id(view.target, primary_window_id)
            else {
                continue;
            };
            let Some(window) = windows.iter().find(|window| window.id == window_id) else {
                continue;
            };
            let Some(provider) = handles.get(window_id) else {
                continue;
            };

            let color = view.clear_color;
            let view_batches = sprite_batches.for_view(view_index).collect::<Vec<_>>();
            if let Some(draw_stats) = self.render_window_clear_pass(
                window,
                provider,
                color,
                &view_batches,
                images,
                prepared_images,
            )? {
                stats.draw_calls = stats.draw_calls.saturating_add(draw_stats.draw_calls);
                stats.sprites = stats.sprites.saturating_add(draw_stats.sprites);
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
        sprite_batches: &[&SpriteBatch],
        images: Option<&Assets<ImageAsset>>,
        prepared_images: Option<&PreparedRenderResources<PreparedImageResource>>,
    ) -> Result<Option<WgpuSpriteDrawStats>, WgpuRenderError> {
        let Some(size) = surface_extent(
            window.resolution.physical_width,
            window.resolution.physical_height,
        ) else {
            return Ok(None);
        };

        self.ensure_surface(window, provider, size)?;

        let view_format = self.surface_view_format(window.id)?;
        let has_sprite_work = sprite_batches
            .iter()
            .any(|batch| !batch.instances.is_empty());
        let sprite_pipeline = if has_sprite_work {
            self.ensure_sprite_pipeline(view_format)?;
            Some(self.sprite_pipeline(view_format)?.clone())
        } else {
            None
        };
        let device = self
            .device
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?
            .clone();
        let queue = self
            .queue
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?
            .clone();
        let sprite_buffers = if let Some(sprite_pipeline) = &sprite_pipeline {
            create_sprite_batch_buffers(
                &device,
                &queue,
                sprite_batches,
                &sprite_pipeline.texture_bind_group_layout,
                &mut self.sprite_textures,
                images,
                prepared_images,
            )
            .map_err(|error| WgpuRenderError::SpriteTexture {
                message: error.to_string(),
            })?
        } else {
            Vec::new()
        };
        let surface_state =
            self.surfaces
                .get_mut(&window.id)
                .ok_or(WgpuRenderError::SurfaceMissing {
                    window_id: window.id,
                })?;

        match surface_state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => {
                render_acquired_texture(
                    &device,
                    &queue,
                    window.id,
                    surface_state,
                    texture,
                    clear_color,
                    sprite_pipeline.as_ref().map(|pipeline| &pipeline.pipeline),
                    &sprite_buffers,
                )?;
                Ok(Some(sprite_batch_draw_stats(sprite_batches)))
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                drop(texture);
                surface_state.dirty = true;
                Ok(None)
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                Ok(None)
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                surface_state.dirty = true;
                Ok(None)
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.surfaces.remove(&window.id);
                Ok(None)
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

        if crate::surface::surface_resize_action(surface.size, size)
            == SurfaceResizeAction::Reconfigure(size)
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

    fn surface_view_format(
        &self,
        window_id: WindowId,
    ) -> Result<wgpu::TextureFormat, WgpuRenderError> {
        let surface = self
            .surfaces
            .get(&window_id)
            .ok_or(WgpuRenderError::SurfaceMissing { window_id })?;
        let config = surface
            .config
            .as_ref()
            .ok_or(WgpuRenderError::SurfaceUnconfigured { window_id })?;

        Ok(config.format.add_srgb_suffix())
    }

    fn ensure_sprite_pipeline(
        &mut self,
        format: wgpu::TextureFormat,
    ) -> Result<(), WgpuRenderError> {
        if self
            .sprite_pipelines
            .iter()
            .any(|pipeline| pipeline.format == format)
        {
            return Ok(());
        }

        let pipeline = {
            let device = self
                .device
                .as_ref()
                .ok_or(WgpuRenderError::BackendNotReady)?;
            create_sprite_pipeline(device, format)
        };
        self.sprite_pipelines.push(pipeline);
        Ok(())
    }

    fn sprite_pipeline(
        &self,
        format: wgpu::TextureFormat,
    ) -> Result<&WgpuSpritePipeline, WgpuRenderError> {
        self.sprite_pipelines
            .iter()
            .find(|pipeline| pipeline.format == format)
            .ok_or_else(|| WgpuRenderError::SpritePipelineMissing {
                format: format!("{format:?}"),
            })
    }

    fn mark_error(&mut self, error: &WgpuRenderError) {
        self.state = WgpuBackendState::Unavailable;
        self.last_error = Some(error.to_string());
    }
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
    #[error("wgpu sprite pipeline is missing for format {format}")]
    SpritePipelineMissing { format: String },
    #[error("wgpu sprite texture error: {message}")]
    SpriteTexture { message: String },
}

pub fn render_clear_passes(
    mut backend: ResMut<WgpuRenderBackend>,
    handles: Res<BackendWindowHandles>,
    windows: Query<&Window>,
    views: Res<ExtractedViews>,
    sprite_batches: Res<SpriteBatches>,
    images: Option<Res<Assets<ImageAsset>>>,
    prepared_images: Option<Res<PreparedRenderResources<PreparedImageResource>>>,
    primary_window_id: Option<Res<PrimaryWindowId>>,
    mut frame: ResMut<RenderFrame>,
    mut stats: ResMut<FrameStats>,
) {
    let primary_window_id = primary_window_id.map(|resource| resource.0);
    let result = backend.render_clear_passes(
        &handles,
        &windows,
        &views,
        &sprite_batches,
        images.as_deref(),
        prepared_images.as_deref(),
        primary_window_id,
        &mut frame,
        &mut stats,
    );

    if let Err(error) = result {
        backend.mark_error(&error);
        frame.mark_skipped();
    }
}

fn render_acquired_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    window_id: WindowId,
    surface_state: &WgpuSurfaceState,
    surface_texture: wgpu::SurfaceTexture,
    clear_color: Color,
    sprite_pipeline: Option<&wgpu::RenderPipeline>,
    sprite_buffers: &[WgpuSpriteBatchBuffer],
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
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("nara_wgpu_surface_pass"),
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
        if let Some(sprite_pipeline) = sprite_pipeline {
            draw_sprite_batch_buffers(&mut pass, sprite_pipeline, sprite_buffers);
        }
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
        assert_eq!(backend.sprite_texture_count(), 0);
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
