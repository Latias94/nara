//! Wgpu backend for backend-neutral nara render data.

use std::{collections::BTreeMap, marker::PhantomData};

#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
mod quad;
#[cfg(feature = "sprite-submitter")]
mod sprite;
mod surface;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
mod texture;
#[cfg(feature = "ui-submitter")]
mod ui;

#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::quad::{
    WgpuQuadBatch, WgpuQuadBatchBuffer, WgpuQuadPipeline, WgpuQuadPipelineDrawRef,
    WgpuQuadPipelineKey, create_quad_batch_buffers, create_quad_pipeline,
    create_quad_texture_bind_group_layout, draw_quad_batch_buffers_for_phase,
    quad_batch_draw_stats,
};
use crate::surface::{WgpuSurfaceState, configure_surface, create_surface, surface_extent};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::texture::WgpuSpriteTextureCache;
use nara_app::{App, CoreStage, Plugin, PluginCleanupContext, PluginError};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_asset::Assets;
use nara_ecs::{Query, Res, ResMut, Resource, schedule::IntoScheduleConfigs};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_image::{ImageAsset, PreparedImageResource};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_material::AlphaMode2d;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_render::PreparedRenderResources;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_render::RenderPassStepLabel;
use nara_render::{
    Color, Extent2d, ExtractedViews, FrameStats, RenderBackendState, RenderBackendStatus,
    RenderFrame, RenderFrameSkipReason, RenderPassPlan, RenderPassStep, RenderPhaseInput,
    begin_render_frame, build_render_pass_plan,
};
#[cfg(feature = "sprite-submitter")]
use nara_sprite_render::SpriteBatches;
#[cfg(feature = "ui-submitter")]
use nara_ui_render::UiBatches;
use nara_window::{
    PrimaryWindowId, Window, WindowId,
    backend::{BackendWindowHandles, RawWindowHandleProvider},
};
use thiserror::Error;

pub use crate::surface::{
    SurfaceAcquireAction, SurfaceResizeAction, SurfaceTextureStatus, choose_present_mode,
    clear_color_to_wgpu, map_present_mode, surface_acquire_policy, surface_resize_action,
};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
pub use crate::texture::WgpuTextureCacheStats as WgpuRenderTextureCacheStats;

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
        app.init_resource::<WgpuRenderBackend>()?;
        app.init_resource::<RenderBackendStatus>()?;
        app.world_mut()?
            .resource_mut::<RenderBackendStatus>()
            .mark_state(WGPU_RENDER_BACKEND, RenderBackendState::Uninitialized);
        app.add_systems(
            CoreStage::Render,
            render_wgpu_surfaces.after(begin_render_frame),
        )?;
        Ok(())
    }

    fn cleanup(&self, context: &mut PluginCleanupContext<'_>) -> Result<(), PluginError> {
        if let Some(mut backend) = context.world_mut().get_resource_mut::<WgpuRenderBackend>() {
            backend.clear_gpu_resources();
        }
        Ok(())
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

#[derive(Debug, nara_ecs::Component)]
pub struct WgpuRenderBackend {
    state: WgpuBackendState,
    instance: Option<wgpu::Instance>,
    adapter: Option<wgpu::Adapter>,
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,
    surfaces: BTreeMap<WindowId, WgpuSurfaceState>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    quad_texture_bind_group_layout: Option<wgpu::BindGroupLayout>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    quad_pipelines: Vec<WgpuQuadPipeline>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    quad_textures: WgpuSpriteTextureCache,
    last_error: Option<String>,
}

impl Resource for WgpuRenderBackend {}

impl Default for WgpuRenderBackend {
    fn default() -> Self {
        Self {
            state: WgpuBackendState::Uninitialized,
            instance: None,
            adapter: None,
            device: None,
            queue: None,
            surfaces: BTreeMap::new(),
            #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
            quad_texture_bind_group_layout: None,
            #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
            quad_pipelines: Vec::new(),
            #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
            quad_textures: WgpuSpriteTextureCache::default(),
            last_error: None,
        }
    }
}

impl WgpuRenderBackend {
    #[must_use]
    pub const fn state(&self) -> WgpuBackendState {
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
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        {
            self.quad_texture_bind_group_layout = None;
            self.quad_pipelines.clear();
            self.quad_textures.clear();
        }
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    #[must_use]
    pub fn texture_count(&self) -> usize {
        self.quad_textures.image_count()
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    #[must_use]
    pub fn texture_stats(&self) -> WgpuRenderTextureCacheStats {
        self.quad_textures.stats()
    }

    fn render_surfaces(
        &mut self,
        handles: Option<&BackendWindowHandles>,
        windows: &Query<&Window>,
        views: &ExtractedViews,
        submitters: SubmitterInputs<'_>,
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
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        self.quad_textures.begin_frame(frame.index);
        let pass_plan = build_wgpu_render_pass_plan(views, submitters);

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
            let Some(size) = surface_extent(
                window.resolution.physical_width,
                window.resolution.physical_height,
            ) else {
                continue;
            };

            self.ensure_surface(window, provider, size)?;
            let view_format = self.surface_view_format(window.id)?;

            let draw =
                self.prepare_submitter_draw(view_index, view_format, submitters, frame.index)?;
            let pass_steps = pass_plan.for_view(view_index).copied().collect::<Vec<_>>();
            if self.render_window(window.id, view.clear_color, &draw, &pass_steps)? {
                stats.draw_calls = stats.draw_calls.saturating_add(draw.draw_calls);
                stats.sprites = stats.sprites.saturating_add(draw.sprites);
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
        if self.state == WgpuBackendState::Ready {
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

    fn render_window(
        &mut self,
        window_id: WindowId,
        clear_color: Color,
        draw: &PreparedSubmitterDraw,
        pass_steps: &[RenderPassStep],
    ) -> Result<bool, WgpuRenderError> {
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
        let surface_state = self
            .surfaces
            .get_mut(&window_id)
            .ok_or(WgpuRenderError::SurfaceMissing { window_id })?;

        match surface_state.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => {
                render_acquired_texture(
                    &device,
                    &queue,
                    window_id,
                    surface_state,
                    texture,
                    clear_color,
                    draw,
                    pass_steps,
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
                self.surfaces.remove(&window_id);
                Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                Err(WgpuRenderError::SurfaceValidation { window_id })
            }
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

    fn prepare_submitter_draw(
        &mut self,
        view_index: usize,
        view_format: wgpu::TextureFormat,
        submitters: SubmitterInputs<'_>,
        frame_index: u64,
    ) -> Result<PreparedSubmitterDraw, WgpuRenderError> {
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        {
            let batches = submitters.quad_batches(view_index);
            let stats = quad_batch_draw_stats(&batches);
            if batches.is_empty() {
                return Ok(PreparedSubmitterDraw::default());
            }
            let alpha_modes = required_alpha_modes(&batches);
            let texture_layout = self.ensure_quad_texture_bind_group_layout()?;
            for alpha_mode in &alpha_modes {
                self.ensure_quad_pipeline(WgpuQuadPipelineKey {
                    format: view_format,
                    alpha_mode: *alpha_mode,
                })?;
            }
            let pipelines = self.quad_pipeline_draw_refs(view_format, &alpha_modes)?;
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
            let buffers = create_quad_batch_buffers(
                &device,
                &queue,
                &batches,
                &texture_layout,
                &mut self.quad_textures,
                submitters.images,
                submitters.prepared_images,
                frame_index,
            )
            .map_err(|error| WgpuRenderError::QuadTexture {
                message: error.to_string(),
            })?;
            return Ok(PreparedSubmitterDraw {
                pipelines,
                buffers,
                draw_calls: stats.draw_calls,
                sprites: stats.sprites,
            });
        }

        #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
        {
            let _ = (view_index, view_format, submitters, frame_index);
            Ok(PreparedSubmitterDraw::default())
        }
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    fn ensure_quad_texture_bind_group_layout(
        &mut self,
    ) -> Result<wgpu::BindGroupLayout, WgpuRenderError> {
        if let Some(layout) = &self.quad_texture_bind_group_layout {
            return Ok(layout.clone());
        }
        let device = self
            .device
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        let layout = create_quad_texture_bind_group_layout(device);
        self.quad_texture_bind_group_layout = Some(layout.clone());
        Ok(layout)
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    fn ensure_quad_pipeline(&mut self, key: WgpuQuadPipelineKey) -> Result<(), WgpuRenderError> {
        if self
            .quad_pipelines
            .iter()
            .any(|pipeline| pipeline.key == key)
        {
            return Ok(());
        }
        let texture_layout = self.ensure_quad_texture_bind_group_layout()?;
        let device = self
            .device
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        self.quad_pipelines
            .push(create_quad_pipeline(device, key, &texture_layout));
        Ok(())
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    fn quad_pipeline_draw_refs(
        &self,
        format: wgpu::TextureFormat,
        alpha_modes: &[AlphaMode2d],
    ) -> Result<Vec<WgpuQuadPipelineDrawRef>, WgpuRenderError> {
        alpha_modes
            .iter()
            .map(|alpha_mode| {
                let key = WgpuQuadPipelineKey {
                    format,
                    alpha_mode: *alpha_mode,
                };
                let pipeline = self
                    .quad_pipelines
                    .iter()
                    .find(|pipeline| pipeline.key == key)
                    .ok_or_else(|| WgpuRenderError::QuadPipelineMissing {
                        format: format!("{format:?}"),
                        alpha_mode: format!("{alpha_mode:?}"),
                    })?;
                Ok(WgpuQuadPipelineDrawRef {
                    alpha_mode: *alpha_mode,
                    pipeline: pipeline.pipeline.clone(),
                })
            })
            .collect()
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
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    #[error("wgpu quad pipeline is missing for format {format} and alpha mode {alpha_mode}")]
    QuadPipelineMissing { format: String, alpha_mode: String },
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    #[error("wgpu quad texture error: {message}")]
    QuadTexture { message: String },
}

#[derive(Clone, Copy)]
struct SubmitterInputs<'a> {
    _lifetime: PhantomData<&'a ()>,
    #[cfg(feature = "sprite-submitter")]
    sprite_batches: Option<&'a SpriteBatches>,
    #[cfg(feature = "ui-submitter")]
    ui_batches: Option<&'a UiBatches>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    images: Option<&'a Assets<ImageAsset>>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    prepared_images: Option<&'a PreparedRenderResources<PreparedImageResource>>,
}

impl SubmitterInputs<'_> {
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    fn quad_batches(self, view_index: usize) -> Vec<WgpuQuadBatch> {
        let mut batches = Vec::new();
        #[cfg(feature = "sprite-submitter")]
        if let Some(sprite_batches) = self.sprite_batches {
            batches.extend(sprite::collect_sprite_quad_batches(
                sprite_batches,
                view_index,
            ));
        }
        #[cfg(feature = "ui-submitter")]
        if let Some(ui_batches) = self.ui_batches {
            batches.extend(ui::collect_ui_quad_batches(ui_batches, view_index));
        }
        batches
    }

    fn phase_inputs(self) -> Vec<RenderPhaseInput> {
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        let mut inputs = Vec::new();
        #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
        let inputs = Vec::new();
        #[cfg(feature = "sprite-submitter")]
        if let Some(sprite_batches) = self.sprite_batches {
            sprite::append_sprite_phase_inputs(sprite_batches, &mut inputs);
        }
        #[cfg(feature = "ui-submitter")]
        if let Some(ui_batches) = self.ui_batches {
            ui::append_ui_phase_inputs(ui_batches, &mut inputs);
        }
        inputs
    }
}

#[derive(Debug, Default)]
struct PreparedSubmitterDraw {
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    pipelines: Vec<WgpuQuadPipelineDrawRef>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    buffers: Vec<WgpuQuadBatchBuffer>,
    draw_calls: u32,
    sprites: u32,
}

pub fn render_wgpu_surfaces(
    mut backend: ResMut<WgpuRenderBackend>,
    handles: Option<Res<BackendWindowHandles>>,
    windows: Query<&Window>,
    views: Res<ExtractedViews>,
    #[cfg(feature = "sprite-submitter")] sprite_batches: Option<Res<SpriteBatches>>,
    #[cfg(feature = "ui-submitter")] ui_batches: Option<Res<UiBatches>>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))] images: Option<
        Res<Assets<ImageAsset>>,
    >,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))] prepared_images: Option<
        Res<PreparedRenderResources<PreparedImageResource>>,
    >,
    primary_window_id: Option<Res<PrimaryWindowId>>,
    mut frame: ResMut<RenderFrame>,
    mut stats: ResMut<FrameStats>,
    mut status: ResMut<RenderBackendStatus>,
) {
    let submitters = SubmitterInputs {
        _lifetime: PhantomData,
        #[cfg(feature = "sprite-submitter")]
        sprite_batches: sprite_batches.as_deref(),
        #[cfg(feature = "ui-submitter")]
        ui_batches: ui_batches.as_deref(),
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        images: images.as_deref(),
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        prepared_images: prepared_images.as_deref(),
    };
    let result = backend.render_surfaces(
        handles.as_deref(),
        &windows,
        &views,
        submitters,
        primary_window_id.map(|resource| resource.0),
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

#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
fn required_alpha_modes(batches: &[WgpuQuadBatch]) -> Vec<AlphaMode2d> {
    let mut modes = Vec::new();
    for mode in batches
        .iter()
        .filter(|batch| !batch.instances.is_empty())
        .map(|batch| batch.material.alpha_mode)
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
    draw: &PreparedSubmitterDraw,
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
        label: Some("nara_wgpu_surface_encoder"),
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
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        for step in pass_steps {
            if let RenderPassStepLabel::Phase(phase) = step.node.label {
                draw_quad_batch_buffers_for_phase(&mut pass, &draw.pipelines, &draw.buffers, phase);
            }
        }
        #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
        let _ = (&mut pass, draw, pass_steps);
    }
    queue.submit([encoder.finish()]);
    queue.present(surface_texture);
    Ok(())
}

fn build_wgpu_render_pass_plan(
    views: &ExtractedViews,
    submitters: SubmitterInputs<'_>,
) -> RenderPassPlan {
    build_render_pass_plan(views, submitters.phase_inputs())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backend_starts_without_surfaces_or_native_state() {
        let backend = WgpuRenderBackend::default();
        assert_eq!(backend.state(), WgpuBackendState::Uninitialized);
        assert_eq!(backend.surface_count(), 0);
        assert_eq!(backend.last_error(), None);
    }

    #[test]
    fn base_submitter_input_has_no_phase_work() {
        let inputs = SubmitterInputs {
            _lifetime: PhantomData,
            #[cfg(feature = "sprite-submitter")]
            sprite_batches: None,
            #[cfg(feature = "ui-submitter")]
            ui_batches: None,
            #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
            images: None,
            #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
            prepared_images: None,
        };
        assert!(inputs.phase_inputs().is_empty());
    }
}
