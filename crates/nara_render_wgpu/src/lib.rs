//! Wgpu backend skeleton for nara render-domain data.

use std::collections::BTreeMap;

mod sprite;
mod surface;
mod texture;
mod ui;

use crate::sprite::{
    WgpuSpriteBatchBuffer, WgpuSpriteDrawStats, WgpuSpritePipeline, WgpuSpritePipelineDrawRef,
    WgpuSpritePipelineKey, create_sprite_batch_buffers, create_sprite_pipeline,
    create_sprite_texture_bind_group_layout, draw_sprite_batch_buffers_for_phase,
    sprite_batch_draw_stats,
};
use crate::surface::{WgpuSurfaceState, configure_surface, create_surface, surface_extent};
use crate::texture::WgpuSpriteTextureCache;
use crate::ui::{create_ui_batch_buffers, ui_batch_draw_stats};
use nara_app::{App, CoreStage, Plugin, PluginError};
use nara_asset::Assets;
use nara_ecs::{Query, Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use nara_image::{ImageAsset, PreparedImageResource};
use nara_material::AlphaMode2d;
use nara_render::{
    Color, Extent2d, ExtractedViews, FrameStats, PreparedRenderResources, RenderBackendState,
    RenderBackendStatus, RenderFrame, RenderFrameSkipReason, RenderPassPlan, RenderPassStep,
    RenderPassStepLabel, RenderPhaseInput, begin_render_frame, build_render_pass_plan,
};
use nara_sprite_render::{SpriteBatch, SpriteBatches};
use nara_ui_render::{UiBatch, UiBatches};
use nara_window::{
    PrimaryWindowId, Window, WindowId,
    backend::{BackendWindowHandles, RawWindowHandleProvider},
};
use thiserror::Error;

pub use crate::surface::{
    SurfaceAcquireAction, SurfaceResizeAction, SurfaceTextureStatus, choose_present_mode,
    clear_color_to_wgpu, map_present_mode, surface_acquire_policy, surface_resize_action,
};
pub use crate::texture::WgpuTextureCacheStats as WgpuRenderTextureCacheStats;

#[cfg(test)]
use nara_window::PresentMode;

const WGPU_RENDER_BACKEND: &str = "wgpu";

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuRenderPlugin;

impl Plugin for WgpuRenderPlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            nara_app::PluginId::new("nara.render-wgpu"),
            nara_app::PluginCategory::Backend,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_plugin_if_missing(nara_render::RenderPlugin)?;
        app.init_resource::<WgpuRenderBackend>();
        app.init_resource::<RenderBackendStatus>();
        app.world_mut()
            .resource_mut::<RenderBackendStatus>()
            .mark_state(WGPU_RENDER_BACKEND, RenderBackendState::Uninitialized);
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
    sprite_texture_bind_group_layout: Option<wgpu::BindGroupLayout>,
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
            sprite_texture_bind_group_layout: None,
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
        self.sprite_texture_bind_group_layout = None;
        self.sprite_pipelines.clear();
        self.sprite_textures.clear();
    }

    #[must_use]
    pub fn sprite_texture_count(&self) -> usize {
        self.sprite_textures.image_count()
    }

    #[must_use]
    pub fn sprite_texture_stats(&self) -> WgpuRenderTextureCacheStats {
        self.sprite_textures.stats()
    }

    fn render_clear_passes(
        &mut self,
        handles: Option<&BackendWindowHandles>,
        windows: &Query<&Window>,
        views: &ExtractedViews,
        sprite_batches: &SpriteBatches,
        ui_batches: &UiBatches,
        images: Option<&Assets<ImageAsset>>,
        prepared_images: Option<&PreparedRenderResources<PreparedImageResource>>,
        primary_window_id: Option<WindowId>,
        frame: &mut RenderFrame,
        stats: &mut FrameStats,
        status: &mut RenderBackendStatus,
    ) -> Result<(), WgpuRenderError> {
        stats.draw_calls = 0;
        stats.sprites = 0;
        status.mark_state(WGPU_RENDER_BACKEND, render_backend_state(self.state));
        status.clear_skip();

        if views.is_empty() {
            frame.mark_skipped();
            status.mark_skipped_with_message(
                frame.index,
                RenderFrameSkipReason::NoViews,
                "no extracted render views",
            );
            return Ok(());
        }

        self.ensure_device()?;
        status.mark_ready(WGPU_RENDER_BACKEND);
        self.sprite_textures.begin_frame(frame.index);
        let pass_plan = build_wgpu_render_pass_plan(views, sprite_batches, ui_batches);

        let mut submitted_any = false;
        for (view_index, view) in views.as_slice().iter().enumerate() {
            let Some(window_id) = crate::surface::target_window_id(view.target, primary_window_id)
            else {
                continue;
            };
            let Some(window) = windows.iter().find(|window| window.id == window_id) else {
                continue;
            };
            let Some(handles) = handles else {
                continue;
            };
            let Some(provider) = handles.get(window_id) else {
                continue;
            };

            let color = view.clear_color;
            let view_batches = sprite_batches.for_view(view_index).collect::<Vec<_>>();
            let view_ui_batches = ui_batches.for_view(view_index).collect::<Vec<_>>();
            let view_pass_steps = pass_plan.for_view(view_index).copied().collect::<Vec<_>>();
            if let Some(draw_stats) = self.render_window_clear_pass(
                window,
                provider,
                color,
                &view_batches,
                &view_ui_batches,
                &view_pass_steps,
                images,
                prepared_images,
                frame.index,
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
            let (reason, message) = if handles.is_none() {
                (
                    RenderFrameSkipReason::SurfaceUnavailable,
                    "backend window handles resource is missing",
                )
            } else {
                (
                    RenderFrameSkipReason::NoRenderableTarget,
                    "no view resolved to an available backend surface",
                )
            };
            status.mark_skipped_with_message(frame.index, reason, message);
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
        ui_batches: &[&UiBatch],
        pass_steps: &[RenderPassStep],
        images: Option<&Assets<ImageAsset>>,
        prepared_images: Option<&PreparedRenderResources<PreparedImageResource>>,
        frame_index: u64,
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
        let has_ui_work = ui_batches.iter().any(|batch| !batch.instances.is_empty());
        let alpha_modes = required_alpha_modes(sprite_batches, ui_batches);
        let sprite_draw_state = if !alpha_modes.is_empty() {
            let texture_layout = self.ensure_sprite_texture_bind_group_layout()?;
            for alpha_mode in &alpha_modes {
                self.ensure_sprite_pipeline(WgpuSpritePipelineKey {
                    format: view_format,
                    alpha_mode: *alpha_mode,
                })?;
            }
            Some((
                texture_layout,
                self.sprite_pipeline_draw_refs(view_format, &alpha_modes)?,
            ))
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
        let sprite_buffers = if let Some((texture_layout, _pipelines)) = &sprite_draw_state {
            if has_sprite_work {
                create_sprite_batch_buffers(
                    &device,
                    &queue,
                    sprite_batches,
                    texture_layout,
                    &mut self.sprite_textures,
                    images,
                    prepared_images,
                    frame_index,
                )
                .map_err(|error| WgpuRenderError::SpriteTexture {
                    message: error.to_string(),
                })?
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let ui_buffers = if let Some((texture_layout, _pipelines)) = &sprite_draw_state {
            if has_ui_work {
                create_ui_batch_buffers(
                    &device,
                    &queue,
                    ui_batches,
                    texture_layout,
                    &mut self.sprite_textures,
                    images,
                    prepared_images,
                    frame_index,
                )
                .map_err(|error| WgpuRenderError::SpriteTexture {
                    message: error.to_string(),
                })?
            } else {
                Vec::new()
            }
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
                    sprite_draw_state
                        .as_ref()
                        .map(|(_, pipelines)| pipelines.as_slice()),
                    &sprite_buffers,
                    &ui_buffers,
                    pass_steps,
                )?;
                let sprite_stats = sprite_batch_draw_stats(sprite_batches);
                let ui_stats = ui_batch_draw_stats(ui_batches);
                Ok(Some(WgpuSpriteDrawStats {
                    draw_calls: sprite_stats.draw_calls.saturating_add(ui_stats.draw_calls),
                    sprites: sprite_stats.sprites,
                }))
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

    fn ensure_sprite_texture_bind_group_layout(
        &mut self,
    ) -> Result<wgpu::BindGroupLayout, WgpuRenderError> {
        if let Some(layout) = &self.sprite_texture_bind_group_layout {
            return Ok(layout.clone());
        }

        let layout = {
            let device = self
                .device
                .as_ref()
                .ok_or(WgpuRenderError::BackendNotReady)?;
            create_sprite_texture_bind_group_layout(device)
        };
        self.sprite_texture_bind_group_layout = Some(layout.clone());
        Ok(layout)
    }

    fn ensure_sprite_pipeline(
        &mut self,
        key: WgpuSpritePipelineKey,
    ) -> Result<(), WgpuRenderError> {
        if self
            .sprite_pipelines
            .iter()
            .any(|pipeline| pipeline.key == key)
        {
            return Ok(());
        }

        let texture_layout = self.ensure_sprite_texture_bind_group_layout()?;
        let pipeline = {
            let device = self
                .device
                .as_ref()
                .ok_or(WgpuRenderError::BackendNotReady)?;
            create_sprite_pipeline(device, key, &texture_layout)
        };
        self.sprite_pipelines.push(pipeline);
        Ok(())
    }

    fn sprite_pipeline_draw_refs(
        &self,
        format: wgpu::TextureFormat,
        alpha_modes: &[AlphaMode2d],
    ) -> Result<Vec<WgpuSpritePipelineDrawRef>, WgpuRenderError> {
        let mut pipelines = Vec::with_capacity(alpha_modes.len());
        for alpha_mode in alpha_modes {
            let key = WgpuSpritePipelineKey {
                format,
                alpha_mode: *alpha_mode,
            };
            let pipeline = self
                .sprite_pipelines
                .iter()
                .find(|pipeline| pipeline.key == key)
                .ok_or_else(|| WgpuRenderError::SpritePipelineMissing {
                    format: format!("{format:?}"),
                    alpha_mode: format!("{alpha_mode:?}"),
                })?;
            pipelines.push(WgpuSpritePipelineDrawRef {
                alpha_mode: *alpha_mode,
                pipeline: pipeline.pipeline.clone(),
            });
        }
        Ok(pipelines)
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
    #[error("wgpu sprite pipeline is missing for format {format} and alpha mode {alpha_mode}")]
    SpritePipelineMissing { format: String, alpha_mode: String },
    #[error("wgpu sprite texture error: {message}")]
    SpriteTexture { message: String },
}

pub fn render_clear_passes(
    mut backend: ResMut<WgpuRenderBackend>,
    handles: Option<Res<BackendWindowHandles>>,
    windows: Query<&Window>,
    views: Res<ExtractedViews>,
    sprite_batches: Option<Res<SpriteBatches>>,
    ui_batches: Option<Res<UiBatches>>,
    images: Option<Res<Assets<ImageAsset>>>,
    prepared_images: Option<Res<PreparedRenderResources<PreparedImageResource>>>,
    primary_window_id: Option<Res<PrimaryWindowId>>,
    mut frame: ResMut<RenderFrame>,
    mut stats: ResMut<FrameStats>,
    mut status: ResMut<RenderBackendStatus>,
) {
    let primary_window_id = primary_window_id.map(|resource| resource.0);
    let empty_sprite_batches = SpriteBatches::default();
    let empty_ui_batches = UiBatches::default();
    let sprite_batches = sprite_batches.as_deref().unwrap_or(&empty_sprite_batches);
    let ui_batches = ui_batches.as_deref().unwrap_or(&empty_ui_batches);
    let result = backend.render_clear_passes(
        handles.as_deref(),
        &windows,
        &views,
        sprite_batches,
        ui_batches,
        images.as_deref(),
        prepared_images.as_deref(),
        primary_window_id,
        &mut frame,
        &mut stats,
        &mut status,
    );

    if let Err(error) = result {
        backend.mark_error(&error);
        status.mark_unavailable(WGPU_RENDER_BACKEND, error.to_string());
        status.mark_skipped_with_message(
            frame.index,
            RenderFrameSkipReason::BackendError,
            error.to_string(),
        );
        frame.mark_skipped();
    }
}

fn render_backend_state(state: WgpuBackendState) -> RenderBackendState {
    match state {
        WgpuBackendState::Uninitialized => RenderBackendState::Uninitialized,
        WgpuBackendState::Initializing => RenderBackendState::Initializing,
        WgpuBackendState::Ready => RenderBackendState::Ready,
        WgpuBackendState::Unavailable => RenderBackendState::Unavailable,
    }
}

fn required_alpha_modes(
    sprite_batches: &[&SpriteBatch],
    ui_batches: &[&UiBatch],
) -> Vec<AlphaMode2d> {
    let mut modes = Vec::new();
    for mode in sprite_batches
        .iter()
        .filter(|batch| !batch.instances.is_empty())
        .map(|batch| batch.material.alpha_mode)
        .chain(
            ui_batches
                .iter()
                .filter(|batch| !batch.instances.is_empty())
                .map(|batch| batch.material.alpha_mode),
        )
    {
        if !modes.contains(&mode) {
            modes.push(mode);
        }
    }
    modes
}

fn render_acquired_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    window_id: WindowId,
    surface_state: &WgpuSurfaceState,
    surface_texture: wgpu::SurfaceTexture,
    clear_color: Color,
    sprite_pipelines: Option<&[WgpuSpritePipelineDrawRef]>,
    sprite_buffers: &[WgpuSpriteBatchBuffer],
    ui_buffers: &[WgpuSpriteBatchBuffer],
    pass_steps: &[RenderPassStep],
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
        if let Some(sprite_pipelines) = sprite_pipelines {
            for step in pass_steps {
                match step.node.label {
                    RenderPassStepLabel::Clear => {}
                    RenderPassStepLabel::Phase(phase) => {
                        draw_sprite_batch_buffers_for_phase(
                            &mut pass,
                            sprite_pipelines,
                            sprite_buffers,
                            phase,
                        );
                        draw_sprite_batch_buffers_for_phase(
                            &mut pass,
                            sprite_pipelines,
                            ui_buffers,
                            phase,
                        );
                    }
                }
            }
        }
    }

    queue.submit([encoder.finish()]);
    queue.present(surface_texture);
    Ok(())
}

fn build_wgpu_render_pass_plan(
    views: &ExtractedViews,
    sprite_batches: &SpriteBatches,
    ui_batches: &UiBatches,
) -> RenderPassPlan {
    build_render_pass_plan(
        views,
        sprite_batches
            .as_slice()
            .iter()
            .map(|batch| RenderPhaseInput {
                view_index: batch.view_index,
                phase: batch.phase,
            })
            .chain(ui_batches.as_slice().iter().map(|batch| RenderPhaseInput {
                view_index: batch.view_index,
                phase: nara_render::RenderPhaseLabel::UI,
            })),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use nara_app::App;
    use nara_core::Vec2;
    use nara_render::{
        ExtractedView, RenderBackendStatus, RenderFrame, RenderFrameState, RenderPhaseLabel,
        RenderTarget, ViewportRect,
    };
    use nara_sprite_render::{
        ColorKey, SpriteBatches, SpriteInstance, SpriteMaterialKey, TextureUvRect,
    };
    use nara_ui_render::{
        UiBatch, UiBatches, UiClipRect, UiColorKey, UiInstance, UiMaterialKey, UiTextureRect,
    };

    #[test]
    fn backend_starts_uninitialized() {
        let backend = WgpuRenderBackend::default();

        assert_eq!(backend.state(), WgpuBackendState::Uninitialized);
        assert_eq!(backend.surface_count(), 0);
        assert_eq!(backend.sprite_texture_count(), 0);
        assert_eq!(backend.last_error(), None);
    }

    #[test]
    fn plugin_reports_skipped_frame_status_without_views() {
        let mut app = App::new();
        app.add_plugin(WgpuRenderPlugin).unwrap();

        app.run_once(Duration::ZERO).unwrap();

        let frame = app.world().resource::<RenderFrame>();
        let status = app.world().resource::<RenderBackendStatus>();
        let skip = status.last_skip().unwrap();
        assert_eq!(frame.state, RenderFrameState::Skipped);
        assert_eq!(status.backend(), Some(WGPU_RENDER_BACKEND));
        assert_eq!(status.state(), RenderBackendState::Uninitialized);
        assert_eq!(skip.frame_index(), frame.index);
        assert_eq!(skip.reason(), RenderFrameSkipReason::NoViews);
        assert!(!app.world().contains_resource::<SpriteBatches>());
        assert!(!app.world().contains_resource::<UiBatches>());
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

    #[test]
    fn backend_pass_plan_orders_world_before_ui() {
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            camera_entity: nara_ecs::Entity::PLACEHOLDER,
            target: RenderTarget::PrimaryWindow,
            viewport: ViewportRect::new(0, 0, 100, 100).unwrap(),
            world_position: Vec2::ZERO,
            viewport_height: 100.0,
            order: 0,
            clear_color: Color::BLACK,
        });
        let mut sprite_batches = SpriteBatches::default();
        sprite_batches.replace(vec![SpriteBatch {
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            phase: RenderPhaseLabel::TRANSPARENT_2D,
            layer: 0,
            sort_key: 0,
            material: sprite_material_key(),
            instances: vec![SpriteInstance {
                center: Vec2::ZERO,
                x_axis: Vec2::X,
                y_axis: Vec2::Y,
                color: Color::WHITE,
                uv: TextureUvRect::FULL,
            }],
        }]);
        let mut ui_batches = UiBatches::default();
        ui_batches.replace(vec![UiBatch {
            view_index: 0,
            view_order: 0,
            target: RenderTarget::PrimaryWindow,
            order: 0,
            z_index: 0,
            material: ui_material_key(),
            clip_rect: UiClipRect::from_rect(nara_ui::UiRect::from_origin_size(
                0.0, 0.0, 10.0, 10.0,
            )),
            instances: vec![UiInstance {
                center: Vec2::ZERO,
                x_axis: Vec2::X,
                y_axis: Vec2::Y,
                color: Color::WHITE,
                uv: UiTextureRect::FULL,
            }],
        }]);

        let plan = build_wgpu_render_pass_plan(&views, &sprite_batches, &ui_batches);

        assert_eq!(
            plan.steps()
                .iter()
                .map(|step| step.node.label)
                .collect::<Vec<_>>(),
            vec![
                RenderPassStepLabel::Clear,
                RenderPassStepLabel::Phase(RenderPhaseLabel::TRANSPARENT_2D),
                RenderPassStepLabel::Phase(RenderPhaseLabel::UI),
            ]
        );
    }

    #[test]
    fn ui_render_public_types_are_not_sprite_aliases() {
        assert_ne!(
            std::any::type_name::<UiInstance>(),
            std::any::type_name::<SpriteInstance>()
        );
        assert_ne!(
            std::any::type_name::<UiMaterialKey>(),
            std::any::type_name::<SpriteMaterialKey>()
        );
        assert_ne!(
            std::any::type_name::<UiTextureRect>(),
            std::any::type_name::<TextureUvRect>()
        );
    }

    fn sprite_material_key() -> SpriteMaterialKey {
        SpriteMaterialKey {
            image: None,
            sampler: nara_material::SamplerDescriptor::default(),
            alpha_mode: nara_material::AlphaMode2d::Blend,
            tint: ColorKey::from_color(Color::WHITE),
        }
    }

    fn ui_material_key() -> UiMaterialKey {
        UiMaterialKey {
            image: None,
            sampler: nara_material::SamplerDescriptor::default(),
            alpha_mode: nara_material::AlphaMode2d::Blend,
            tint: UiColorKey::from_color(Color::WHITE),
        }
    }
}
