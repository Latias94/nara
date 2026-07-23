use std::{
    collections::BTreeMap,
    ffi::OsStr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use nara_ecs::Resource;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_material::AlphaMode2d;
use nara_render::{
    Color, Extent2d, FrameStats, RenderBackendState, RenderBackendStatus, RenderFrame,
    RenderFrameSkipReason, RenderWindowPacket,
};
use nara_window::{
    WindowId,
    backend::{BackendWindowHandles, WindowTargetError},
};

#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::quad::{
    WgpuQuadPipeline, WgpuQuadPipelineDrawRef, WgpuQuadPipelineKey, create_quad_batch_buffers,
    create_quad_pipeline, create_quad_texture_bind_group_layout, quad_batch_draw_stats,
};
use crate::surface::{
    SurfaceDropReason, SurfaceResizeAction, WgpuSurfaceState, configure_surface, create_surface,
    surface_extent, surface_resize_action,
};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::texture::WgpuSpriteTextureCache;
use crate::{
    PreparedSubmitterDraw, RenderResourceInputs, WGPU_RENDER_BACKEND, WgpuCapturedFrame,
    WgpuFramePayload, WgpuRenderError, render_acquired_texture,
};

static NEXT_WGPU_BACKEND_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);
const WGPU_FORCE_FALLBACK_ADAPTER_ENV: &str = "NARA_WGPU_FORCE_FALLBACK";
const INVALID_FALLBACK_ADAPTER_ENV: &str = "NARA_WGPU_FORCE_FALLBACK must be either 0 or 1";

fn force_fallback_adapter_from_value(value: Option<&OsStr>) -> Result<bool, &'static str> {
    match value {
        None => Ok(false),
        Some(value) if value == OsStr::new("0") => Ok(false),
        Some(value) if value == OsStr::new("1") => Ok(true),
        Some(_) => Err(INVALID_FALLBACK_ADAPTER_ENV),
    }
}

fn force_fallback_adapter_from_environment() -> Result<bool, &'static str> {
    let value = std::env::var_os(WGPU_FORCE_FALLBACK_ADAPTER_ENV);
    force_fallback_adapter_from_value(value.as_deref())
}

#[derive(Debug)]
struct WgpuBackendInstanceId(u64);

impl Default for WgpuBackendInstanceId {
    fn default() -> Self {
        let id = NEXT_WGPU_BACKEND_INSTANCE_ID
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .unwrap_or_else(|_| std::process::abort());
        Self(id)
    }
}

#[derive(Debug, Default)]
struct DeviceLossSignal(AtomicBool);

impl DeviceLossSignal {
    fn mark_lost(&self) {
        self.0.store(true, Ordering::Release);
    }

    fn take_lost(&self) -> bool {
        self.0.swap(false, Ordering::AcqRel)
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

/// Read-only evidence for the most recent backend frame transaction.
///
/// This remains backend observation rather than a renderer abstraction. Every render stage resets
/// the counters before packet capture is consumed, so capture and admission rejection can both
/// prove that no surface was acquired, submitted, or presented.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct WgpuFrameTransactionStats {
    frame_index: Option<u64>,
    packet_admissions: u64,
    packet_rejections: u64,
    surface_acquire_attempts: u64,
    surface_acquires: u64,
    queue_submissions: u64,
    presents: u64,
}

impl WgpuFrameTransactionStats {
    #[must_use]
    pub const fn frame_index(self) -> Option<u64> {
        self.frame_index
    }

    #[must_use]
    pub const fn packet_admissions(self) -> u64 {
        self.packet_admissions
    }

    #[must_use]
    pub const fn packet_rejections(self) -> u64 {
        self.packet_rejections
    }

    #[must_use]
    pub const fn surface_acquires(self) -> u64 {
        self.surface_acquires
    }

    #[must_use]
    pub const fn surface_acquire_attempts(self) -> u64 {
        self.surface_acquire_attempts
    }

    #[must_use]
    pub const fn queue_submissions(self) -> u64 {
        self.queue_submissions
    }

    #[must_use]
    pub const fn presents(self) -> u64 {
        self.presents
    }

    fn begin(frame_index: u64) -> Self {
        Self {
            frame_index: Some(frame_index),
            ..Self::default()
        }
    }
}

#[derive(Debug, Default, Resource)]
pub struct WgpuRenderBackend {
    instance_id: WgpuBackendInstanceId,
    state: WgpuBackendState,
    instance: Option<wgpu::Instance>,
    adapter: Option<wgpu::Adapter>,
    device: Option<wgpu::Device>,
    queue: Option<wgpu::Queue>,
    device_loss_signal: Option<Arc<DeviceLossSignal>>,
    device_epoch: u64,
    surfaces: BTreeMap<WindowId, WgpuSurfaceState>,
    runtime_generation: Option<u64>,
    last_frame_index: Option<u64>,
    frame_transaction_stats: WgpuFrameTransactionStats,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    quad_texture_bind_group_layout: Option<wgpu::BindGroupLayout>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    quad_pipelines: Vec<WgpuQuadPipeline>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    quad_textures: WgpuSpriteTextureCache,
    last_error: Option<String>,
}

impl WgpuRenderBackend {
    /// Process-local non-reused identity of this backend owner.
    ///
    /// Together with [`Self::device_epoch`], this identifies one device namespace. The value is
    /// diagnostic runtime state and must not be persisted or used as backend-neutral identity.
    #[must_use]
    pub const fn instance_id(&self) -> u64 {
        self.instance_id.0
    }

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

    #[must_use]
    pub fn configured_surface_extent(&self, window_id: WindowId) -> Option<Extent2d> {
        self.surfaces
            .get(&window_id)
            .filter(|surface| surface.config.is_some())
            .map(|surface| surface.size)
    }

    #[must_use]
    pub const fn frame_transaction_stats(&self) -> WgpuFrameTransactionStats {
        self.frame_transaction_stats
    }

    /// Monotonic identity of the currently initialized device within this backend instance.
    ///
    /// Zero means no device has been initialized. The value is meaningful only with
    /// [`Self::instance_id`]. Clearing or losing a device never rewinds the epoch; a later device
    /// receives a strictly newer value.
    #[must_use]
    pub const fn device_epoch(&self) -> u64 {
        self.device_epoch
    }

    pub(super) fn begin_frame_transaction(&mut self, frame_index: u64) {
        self.frame_transaction_stats = WgpuFrameTransactionStats::begin(frame_index);
    }

    pub(super) fn reject_captured_packet(&mut self) {
        self.frame_transaction_stats.packet_admissions = 0;
        self.frame_transaction_stats.packet_rejections = 1;
    }

    pub(super) fn clear_gpu_resources(
        &mut self,
        reason: SurfaceDropReason,
    ) -> Result<(), WgpuRenderError> {
        let window_ids = self.surfaces.keys().copied().collect::<Vec<_>>();
        let retirement_result = self.retire_targets(&window_ids, reason);
        self.device_loss_signal = None;
        self.queue = None;
        self.device = None;
        self.adapter = None;
        self.instance = None;
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        {
            self.quad_texture_bind_group_layout = None;
            self.quad_pipelines.clear();
            self.quad_textures.clear();
        }
        retirement_result
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    #[must_use]
    pub fn texture_count(&self) -> usize {
        self.quad_textures.image_count()
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    #[must_use]
    pub fn texture_stats(&self) -> crate::WgpuRenderTextureCacheStats {
        self.quad_textures.stats()
    }

    pub(super) fn render_packet(
        &mut self,
        handles: Option<&BackendWindowHandles>,
        captured: WgpuCapturedFrame,
        resources: RenderResourceInputs<'_>,
        frame: &mut RenderFrame,
        stats: &mut FrameStats,
        status: &mut RenderBackendStatus,
    ) -> Result<(), WgpuRenderError> {
        let WgpuCapturedFrame {
            topology: packet,
            payload,
        } = captured;
        stats.draw_calls = 0;
        stats.sprites = 0;
        self.begin_frame_transaction(packet.frame_index());
        if let Err(error) = self.admit_packet(packet.generation().get(), packet.frame_index()) {
            self.frame_transaction_stats.packet_rejections = 1;
            return Err(error);
        }
        self.frame_transaction_stats.packet_admissions = 1;
        self.fail_if_device_lost()?;
        status.mark_state(WGPU_RENDER_BACKEND, render_backend_state(self.state));
        status.clear_skip();

        self.retire_requested_surfaces()?;

        if self.state == WgpuBackendState::Unavailable {
            let message = self
                .last_error
                .clone()
                .unwrap_or_else(|| WgpuRenderError::BackendUnavailable.to_string());
            frame.mark_skipped();
            status.mark_unavailable(WGPU_RENDER_BACKEND, message.clone());
            status.mark_skipped_with_message(
                frame.index,
                RenderFrameSkipReason::BackendError,
                message,
            );
            return Ok(());
        }

        let window = packet.window();
        let Some(size) = surface_extent(
            window.resolution.physical_width,
            window.resolution.physical_height,
        ) else {
            frame.mark_skipped();
            status.mark_skipped_with_message(
                frame.index,
                RenderFrameSkipReason::NoRenderableTarget,
                "the admitted render target is zero sized",
            );
            return Ok(());
        };
        if !self.surfaces.contains_key(&window.id) {
            let Some(handles) = handles else {
                frame.mark_skipped();
                status.mark_skipped_with_message(
                    frame.index,
                    RenderFrameSkipReason::SurfaceUnavailable,
                    "backend window handles resource is missing",
                );
                return Ok(());
            };
            self.ensure_instance()?;
            if !self.create_surface_if_missing(window, handles, size)? {
                frame.mark_skipped();
                status.mark_skipped_with_message(
                    frame.index,
                    RenderFrameSkipReason::SurfaceUnavailable,
                    "the admitted native window target is unavailable",
                );
                return Ok(());
            }
        }
        self.ensure_device(window.id)?;
        self.fail_if_device_lost()?;
        status.mark_ready(WGPU_RENDER_BACKEND);
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        self.quad_textures.begin_frame(frame.index);
        self.configure_surface_if_needed(window, size)?;
        if !self
            .surfaces
            .get(&window.id)
            .is_some_and(WgpuSurfaceState::can_acquire_frame)
        {
            frame.mark_skipped();
            status.mark_skipped_with_message(
                frame.index,
                RenderFrameSkipReason::SurfaceUnavailable,
                "the admitted surface cannot acquire a frame",
            );
            return Ok(());
        }
        let view_format = self.surface_view_format(window.id)?;
        let draw = self.prepare_submitter_draw(view_format, &payload, resources, frame.index)?;
        let submitted = self.render_window(
            window.id,
            packet.view().viewport,
            packet.view().clear_color,
            &draw,
            packet.pass_plan().steps(),
        )?;

        if submitted {
            stats.draw_calls = draw.draw_calls;
            stats.sprites = draw.sprites;
            frame.mark_submitted();
        } else {
            frame.mark_skipped();
            status.mark_skipped_with_message(
                frame.index,
                RenderFrameSkipReason::SurfaceUnavailable,
                "the admitted surface did not produce a presentable frame",
            );
        }
        Ok(())
    }

    fn admit_packet(&mut self, generation: u64, frame_index: u64) -> Result<(), WgpuRenderError> {
        if let Some(expected) = self.runtime_generation {
            if expected != generation {
                return Err(WgpuRenderError::StaleFrameGeneration {
                    expected,
                    actual: generation,
                });
            }
        } else {
            self.runtime_generation = Some(generation);
        }
        if self
            .last_frame_index
            .is_some_and(|last_frame_index| frame_index <= last_frame_index)
        {
            return Err(WgpuRenderError::FrameAlreadyConsumed { frame_index });
        }
        self.last_frame_index = Some(frame_index);
        Ok(())
    }

    fn ensure_instance(&mut self) -> Result<(), WgpuRenderError> {
        if self.state == WgpuBackendState::Unavailable {
            return Err(WgpuRenderError::BackendUnavailable);
        }
        if self.instance.is_none() {
            self.instance = Some(wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            ));
        }
        Ok(())
    }

    fn ensure_device(&mut self, window_id: WindowId) -> Result<(), WgpuRenderError> {
        if self.state == WgpuBackendState::Ready {
            return Ok(());
        }
        if self.state == WgpuBackendState::Unavailable {
            return Err(WgpuRenderError::BackendUnavailable);
        }

        self.state = WgpuBackendState::Initializing;
        self.ensure_instance()?;
        let instance = self
            .instance
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        let surface = self
            .surfaces
            .get(&window_id)
            .ok_or(WgpuRenderError::SurfaceMissing { window_id })?
            .surface(window_id)?;
        let force_fallback_adapter =
            force_fallback_adapter_from_environment().map_err(|message| {
                WgpuRenderError::AdapterUnavailable {
                    message: message.to_owned(),
                }
            })?;
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            compatible_surface: Some(surface),
            force_fallback_adapter,
            ..Default::default()
        }))
        .map_err(|error| WgpuRenderError::AdapterUnavailable {
            message: error.to_string(),
        })?;
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .map_err(|error| WgpuRenderError::DeviceUnavailable {
                    message: error.to_string(),
                })?;
        let device_loss_signal = Arc::new(DeviceLossSignal::default());
        let callback_signal = Arc::clone(&device_loss_signal);
        device.set_device_lost_callback(move |_reason, _message| callback_signal.mark_lost());
        let device_epoch = self
            .device_epoch
            .checked_add(1)
            .ok_or(WgpuRenderError::DeviceEpochExhausted)?;
        self.adapter = Some(adapter);
        self.device = Some(device);
        self.queue = Some(queue);
        self.device_loss_signal = Some(device_loss_signal);
        self.device_epoch = device_epoch;
        self.state = WgpuBackendState::Ready;
        self.last_error = None;
        Ok(())
    }

    fn fail_if_device_lost(&self) -> Result<(), WgpuRenderError> {
        if self
            .device_loss_signal
            .as_ref()
            .is_some_and(|signal| signal.take_lost())
        {
            Err(WgpuRenderError::DeviceLost)
        } else {
            Ok(())
        }
    }

    fn retire_requested_surfaces(&mut self) -> Result<(), WgpuRenderError> {
        let mut retirements = Vec::new();
        for (window_id, surface) in &self.surfaces {
            if surface.retirement_requested()? {
                retirements.push(*window_id);
            }
        }
        self.retire_targets(&retirements, SurfaceDropReason::TargetShutdown)
    }

    pub(super) fn retire_targets(
        &mut self,
        window_ids: &[WindowId],
        reason: SurfaceDropReason,
    ) -> Result<(), WgpuRenderError> {
        let mut first_error = None;
        for window_id in window_ids {
            if let Err(error) = self.retire_surface(*window_id, reason) {
                first_error.get_or_insert(error);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    fn retire_surface(
        &mut self,
        window_id: WindowId,
        reason: SurfaceDropReason,
    ) -> Result<(), WgpuRenderError> {
        let Some(surface) = self.surfaces.remove(&window_id) else {
            return Ok(());
        };
        surface.retire(reason)
    }

    fn render_window(
        &mut self,
        window_id: WindowId,
        viewport: nara_render::ViewportRect,
        clear_color: Color,
        draw: &PreparedSubmitterDraw,
        pass_steps: &[nara_render::RenderPassStep],
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

        self.frame_transaction_stats.surface_acquire_attempts += 1;
        match surface_state.surface(window_id)?.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => {
                self.frame_transaction_stats.surface_acquires += 1;
                render_acquired_texture(
                    &device,
                    &queue,
                    window_id,
                    surface_state,
                    texture,
                    viewport,
                    clear_color,
                    draw,
                    pass_steps,
                )?;
                self.frame_transaction_stats.queue_submissions += 1;
                self.frame_transaction_stats.presents += 1;
                Ok(true)
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                self.frame_transaction_stats.surface_acquires += 1;
                render_acquired_texture(
                    &device,
                    &queue,
                    window_id,
                    surface_state,
                    texture,
                    viewport,
                    clear_color,
                    draw,
                    pass_steps,
                )?;
                self.frame_transaction_stats.queue_submissions += 1;
                self.frame_transaction_stats.presents += 1;
                surface_state.dirty = true;
                Ok(true)
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                surface_state.dirty = true;
                Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.retire_surface(window_id, SurfaceDropReason::SurfaceOrDeviceLost)?;
                Ok(false)
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                Err(WgpuRenderError::SurfaceValidation { window_id })
            }
        }
    }

    fn create_surface_if_missing(
        &mut self,
        window: RenderWindowPacket,
        handles: &BackendWindowHandles,
        size: Extent2d,
    ) -> Result<bool, WgpuRenderError> {
        if !self.surfaces.contains_key(&window.id) {
            let binding = match handles.acquire_surface(window.id) {
                Ok(binding) => binding,
                Err(
                    WindowTargetError::UnknownWindow { .. }
                    | WindowTargetError::SurfaceActivationAfterRetirement { .. },
                ) => {
                    return Ok(false);
                }
                Err(error) => return Err(error.into()),
            };
            let instance = self
                .instance
                .as_ref()
                .ok_or(WgpuRenderError::BackendNotReady)?;
            let (handle_source, target) = binding.into_parts();
            let surface = create_surface(instance, handle_source, window.id)?;
            self.surfaces
                .insert(window.id, WgpuSurfaceState::new(surface, target, size));
        }

        Ok(true)
    }

    fn configure_surface_if_needed(
        &mut self,
        window: RenderWindowPacket,
        size: Extent2d,
    ) -> Result<(), WgpuRenderError> {
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
        view_format: wgpu::TextureFormat,
        payload: &WgpuFramePayload,
        resources: RenderResourceInputs<'_>,
        frame_index: u64,
    ) -> Result<PreparedSubmitterDraw, WgpuRenderError> {
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        {
            let batches = payload.quad_batches();
            let stats = quad_batch_draw_stats(&batches);
            if batches.is_empty() {
                return Ok(PreparedSubmitterDraw::default());
            }
            let alpha_modes = crate::required_alpha_modes(&batches);
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
                resources.images,
                resources.prepared_images,
                frame_index,
            )
            .map_err(|error| WgpuRenderError::QuadTexture {
                message: error.to_string(),
            })?;
            Ok(PreparedSubmitterDraw {
                pipelines,
                buffers,
                draw_calls: stats.draw_calls,
                sprites: stats.sprites,
            })
        }

        #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
        {
            let _ = (view_format, payload, resources, frame_index);
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

    pub(super) fn mark_error(&mut self, error: &WgpuRenderError) -> String {
        let cleanup_error = self
            .clear_gpu_resources(SurfaceDropReason::SurfaceOrDeviceLost)
            .err();
        self.state = WgpuBackendState::Unavailable;
        let message = match cleanup_error {
            Some(cleanup_error) => {
                format!("{error}; device invalidation cleanup failed: {cleanup_error}")
            }
            None => error.to_string(),
        };
        self.last_error = Some(message.clone());
        message
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

#[cfg(test)]
mod tests {
    use std::{ffi::OsStr, sync::Arc};

    use nara_app::{
        App, RuntimeAdmissionReservation, RuntimeCandidateRetirementState, RuntimeClosePolicy,
        RuntimeGeneration, RuntimeObligationLedger,
    };
    use nara_ecs::Entity;
    use nara_render::{
        ExtractedView, ExtractedViews, RenderTarget, ViewportRect, build_render_frame_packet,
    };
    use nara_window::Window;
    use nara_window::backend::{WindowHandleProvider, WindowTargetPhase};
    use raw_window_handle::{
        DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
    };

    use super::*;

    #[test]
    fn fallback_adapter_environment_is_absent_or_explicitly_boolean() {
        assert_eq!(force_fallback_adapter_from_value(None), Ok(false));
        assert_eq!(
            force_fallback_adapter_from_value(Some(OsStr::new("0"))),
            Ok(false)
        );
        assert_eq!(
            force_fallback_adapter_from_value(Some(OsStr::new("1"))),
            Ok(true)
        );
    }

    #[test]
    fn fallback_adapter_environment_rejects_ambiguous_values() {
        assert_eq!(
            force_fallback_adapter_from_value(Some(OsStr::new("true"))),
            Err(INVALID_FALLBACK_ADAPTER_ENV)
        );
    }

    #[derive(Debug)]
    struct TestWindowSource;

    impl HasWindowHandle for TestWindowSource {
        fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
            Err(HandleError::NotSupported)
        }
    }

    impl HasDisplayHandle for TestWindowSource {
        fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
            Err(HandleError::NotSupported)
        }
    }

    fn backend_with_lease_only_surface(
        window_id: WindowId,
    ) -> (WgpuRenderBackend, BackendWindowHandles) {
        let handles = BackendWindowHandles::default();
        handles
            .insert(
                window_id,
                WindowHandleProvider::new(Arc::new(TestWindowSource)),
            )
            .unwrap();
        let binding = handles.acquire_surface(window_id).unwrap();
        let surface = WgpuSurfaceState::lease_only(
            binding,
            Extent2d::new(320, 180).expect("non-zero test extent"),
        );
        let mut backend = WgpuRenderBackend::default();
        backend.surfaces.insert(window_id, surface);
        (backend, handles)
    }

    #[test]
    fn device_epoch_is_not_rewound_by_gpu_resource_cleanup() {
        let mut backend = WgpuRenderBackend {
            device_epoch: 7,
            ..WgpuRenderBackend::default()
        };

        backend
            .clear_gpu_resources(SurfaceDropReason::BackendCleanup)
            .unwrap();

        assert_eq!(backend.device_epoch(), 7);
    }

    #[test]
    fn backend_instances_receive_non_reused_process_local_identities() {
        let first = WgpuRenderBackend::default();
        let second = WgpuRenderBackend::default();

        assert_ne!(first.instance_id(), 0);
        assert_ne!(second.instance_id(), 0);
        assert_ne!(first.instance_id(), second.instance_id());
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    #[test]
    fn replacement_backend_renews_device_namespace_and_starts_with_an_empty_texture_cache() {
        use crate::quad::WgpuQuadMaterialKey;
        use nara_material::SamplerDescriptor;

        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::LowPower,
            compatible_surface: None,
            force_fallback_adapter: false,
            ..Default::default()
        }))
        .or_else(|_| {
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: true,
                ..Default::default()
            }))
        })
        .expect("texture-cache isolation requires a wgpu adapter");
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))
                .expect("texture-cache isolation requires a wgpu device");
        let layout = create_quad_texture_bind_group_layout(&device);
        let predecessor_key = WgpuQuadMaterialKey {
            image: None,
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::Opaque,
        };

        let predecessor_namespace;
        {
            let mut predecessor = WgpuRenderBackend {
                device_epoch: 1,
                ..WgpuRenderBackend::default()
            };
            let _binding = predecessor.quad_textures.fallback_bind_group(
                &device,
                &queue,
                &layout,
                predecessor_key,
                7,
            );
            predecessor_namespace = (predecessor.instance_id(), predecessor.device_epoch());
            assert_eq!(
                predecessor.texture_stats(),
                crate::WgpuRenderTextureCacheStats {
                    fallback_bindings: 1,
                    has_fallback_texture: true,
                    ..crate::WgpuRenderTextureCacheStats::default()
                }
            );
        }

        // The test-owned Device is deliberately shared. Runtime-local backend identity and cache
        // ownership must remain fresh even when a process parent outlives the predecessor.
        let replacement = WgpuRenderBackend {
            device_epoch: 1,
            ..WgpuRenderBackend::default()
        };
        let replacement_namespace = (replacement.instance_id(), replacement.device_epoch());
        assert_ne!(replacement_namespace, predecessor_namespace);
        assert_eq!(
            replacement.texture_stats(),
            crate::WgpuRenderTextureCacheStats::default(),
            "the replacement cache must not contain the predecessor material key or statistics",
        );
    }

    fn test_runtime_generation() -> RuntimeGeneration {
        let candidate = RuntimeAdmissionReservation::try_acquire()
            .unwrap()
            .admit(
                App::new().seal().unwrap(),
                RuntimeObligationLedger::new(),
                RuntimeClosePolicy::default(),
            )
            .unwrap();
        let ready = candidate.complete_startup().unwrap();
        let runtime = ready.promote();
        let generation = runtime.generation();
        let mut retirement = runtime.begin_retirement();
        while retirement.retirement_state() != RuntimeCandidateRetirementState::Retired {
            retirement.drive_retirement();
        }
        generation
    }

    fn test_packet(generation: RuntimeGeneration, frame_index: u64) -> WgpuCapturedFrame {
        let window = Window::default();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            camera_entity: Entity::from_raw_u32(1).unwrap(),
            target: RenderTarget::PrimaryWindow,
            viewport: ViewportRect::new(0, 0, 1280, 720).unwrap(),
            world_position: nara_core::Vec2::ZERO,
            viewport_height: 720.0,
            order: 0,
            clear_color: Color::BLACK,
        });
        let topology = build_render_frame_packet(
            generation,
            frame_index,
            Some(WindowId::PRIMARY),
            [&window],
            &views,
            [],
        )
        .unwrap()
        .unwrap();
        WgpuCapturedFrame {
            topology,
            payload: WgpuFramePayload::default(),
        }
    }

    fn render_packet_without_surface(
        backend: &mut WgpuRenderBackend,
        packet: WgpuCapturedFrame,
    ) -> Result<(), WgpuRenderError> {
        let mut frame = RenderFrame {
            index: packet.topology.frame_index(),
            state: nara_render::RenderFrameState::Rendering,
        };
        backend.render_packet(
            None,
            packet,
            RenderResourceInputs::default(),
            &mut frame,
            &mut FrameStats::default(),
            &mut RenderBackendStatus::default(),
        )
    }

    #[test]
    fn backend_starts_without_surfaces_or_native_state() {
        let backend = WgpuRenderBackend::default();
        assert_eq!(backend.state(), WgpuBackendState::Uninitialized);
        assert_eq!(backend.surface_count(), 0);
        assert_eq!(backend.last_error(), None);
    }

    #[test]
    fn render_packet_rejects_stale_generation_and_repeat_with_zero_surface_work() {
        let (mut backend, handles) = backend_with_lease_only_surface(WindowId::PRIMARY);
        let generation = test_runtime_generation();
        let stale_generation = test_runtime_generation();
        let surface_generation = handles
            .snapshot(WindowId::PRIMARY)
            .unwrap()
            .surface_generation;

        backend.runtime_generation = Some(generation.get());
        backend.last_frame_index = Some(10);
        let stale_error =
            render_packet_without_surface(&mut backend, test_packet(stale_generation, 12))
                .unwrap_err();
        assert!(matches!(
            stale_error,
            WgpuRenderError::StaleFrameGeneration { .. }
        ));
        crate::handle_wgpu_render_error(
            &mut backend,
            stale_error,
            12,
            &mut RenderBackendStatus::default(),
            None,
        );
        assert_eq!(
            backend.frame_transaction_stats(),
            WgpuFrameTransactionStats {
                frame_index: Some(12),
                packet_rejections: 1,
                ..WgpuFrameTransactionStats::default()
            }
        );
        assert_eq!(backend.surface_count(), 1);
        assert_eq!(
            handles
                .snapshot(WindowId::PRIMARY)
                .unwrap()
                .surface_generation,
            surface_generation
        );

        let repeat_error =
            render_packet_without_surface(&mut backend, test_packet(generation, 10)).unwrap_err();
        assert!(matches!(
            repeat_error,
            WgpuRenderError::FrameAlreadyConsumed { frame_index: 10 }
        ));
        crate::handle_wgpu_render_error(
            &mut backend,
            repeat_error,
            10,
            &mut RenderBackendStatus::default(),
            None,
        );
        assert_eq!(
            backend.frame_transaction_stats(),
            WgpuFrameTransactionStats {
                frame_index: Some(10),
                packet_rejections: 1,
                ..WgpuFrameTransactionStats::default()
            }
        );
        assert_eq!(backend.surface_count(), 1);
        assert_eq!(backend.state(), WgpuBackendState::Uninitialized);
        assert_eq!(backend.last_error(), None);
        assert!(backend.instance.is_none());
        assert!(backend.adapter.is_none());
        assert!(backend.device.is_none());
        let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
        assert!(snapshot.surface_active);
        assert_eq!(snapshot.surface_generation, surface_generation);
        assert!(snapshot.provider_present);
    }

    #[test]
    fn surface_loss_drops_the_live_owner_without_retiring_the_target() {
        let (mut backend, handles) = backend_with_lease_only_surface(WindowId::PRIMARY);

        backend
            .retire_surface(WindowId::PRIMARY, SurfaceDropReason::SurfaceOrDeviceLost)
            .unwrap();

        let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
        assert_eq!(snapshot.phase, WindowTargetPhase::Active);
        assert!(!snapshot.surface_active);
        assert!(snapshot.provider_present);
        assert!(handles.is_surface_target_active(WindowId::PRIMARY));
    }

    #[test]
    fn device_loss_invalidates_surfaces_without_releasing_window_authority() {
        let (mut backend, handles) = backend_with_lease_only_surface(WindowId::PRIMARY);
        let signal = Arc::new(DeviceLossSignal::default());
        backend.state = WgpuBackendState::Ready;
        backend.device_loss_signal = Some(Arc::clone(&signal));
        signal.mark_lost();

        let error = backend.fail_if_device_lost().unwrap_err();
        backend.mark_error(&error);

        let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
        assert_eq!(snapshot.phase, WindowTargetPhase::Active);
        assert!(!snapshot.surface_active);
        assert!(snapshot.provider_present);
        assert!(handles.is_surface_target_active(WindowId::PRIMARY));
        assert_eq!(backend.state(), WgpuBackendState::Unavailable);
        assert_eq!(backend.surface_count(), 0);
        assert_eq!(backend.last_error(), Some("wgpu device was lost"));
        assert!(backend.device_loss_signal.is_none());
    }

    #[test]
    fn surface_creation_error_releases_the_binding_for_retry() {
        let handles = BackendWindowHandles::default();
        handles
            .insert(
                WindowId::PRIMARY,
                WindowHandleProvider::new(Arc::new(TestWindowSource)),
            )
            .unwrap();
        let mut backend = WgpuRenderBackend {
            instance: Some(wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            )),
            ..WgpuRenderBackend::default()
        };

        assert!(matches!(
            backend.create_surface_if_missing(
                RenderWindowPacket {
                    id: WindowId::PRIMARY,
                    resolution: nara_window::WindowResolution::new(320, 180),
                    present_mode: nara_window::PresentMode::AutoVsync,
                },
                &handles,
                Extent2d::new(320, 180).unwrap(),
            ),
            Err(WgpuRenderError::SurfaceCreation {
                window_id: WindowId::PRIMARY,
                ..
            })
        ));

        let snapshot = handles.snapshot(WindowId::PRIMARY).unwrap();
        assert_eq!(backend.surface_count(), 0);
        assert!(!snapshot.surface_active);
        assert!(snapshot.provider_present);
        assert!(handles.acquire_surface(WindowId::PRIMARY).is_ok());
    }

    #[test]
    fn scoped_target_retirement_leaves_foreign_surface_live() {
        let owned = WindowId::PRIMARY;
        let foreign = WindowId::new(2);
        let handles = BackendWindowHandles::default();
        for window_id in [owned, foreign] {
            handles
                .insert(
                    window_id,
                    WindowHandleProvider::new(Arc::new(TestWindowSource)),
                )
                .unwrap();
        }
        let mut backend = WgpuRenderBackend::default();
        for window_id in [owned, foreign] {
            backend.surfaces.insert(
                window_id,
                WgpuSurfaceState::lease_only(
                    handles.acquire_surface(window_id).unwrap(),
                    Extent2d::new(320, 180).unwrap(),
                ),
            );
        }
        handles.request_retirement(owned).unwrap();

        backend
            .retire_targets(&[owned], SurfaceDropReason::TargetShutdown)
            .unwrap();

        assert_eq!(
            handles.snapshot(owned).unwrap().phase,
            WindowTargetPhase::SurfaceRetired
        );
        let foreign_snapshot = handles.snapshot(foreign).unwrap();
        assert_eq!(foreign_snapshot.phase, WindowTargetPhase::Active);
        assert!(foreign_snapshot.surface_active);
        assert_eq!(backend.surface_count(), 1);
    }

    #[test]
    fn backend_error_cleanup_discards_partial_instance_initialization() {
        let mut backend = WgpuRenderBackend {
            state: WgpuBackendState::Initializing,
            instance: Some(wgpu::Instance::new(
                wgpu::InstanceDescriptor::new_without_display_handle_from_env(),
            )),
            ..WgpuRenderBackend::default()
        };

        backend.mark_error(&WgpuRenderError::BackendNotReady);

        assert_eq!(backend.state(), WgpuBackendState::Unavailable);
        assert!(backend.instance.is_none());
        assert!(backend.adapter.is_none());
        assert!(backend.device.is_none());
        assert!(backend.queue.is_none());
    }

    #[test]
    fn unavailable_backend_does_not_reenter_device_initialization() {
        let mut backend = WgpuRenderBackend {
            state: WgpuBackendState::Unavailable,
            last_error: Some("device lost".to_owned()),
            ..WgpuRenderBackend::default()
        };

        assert_eq!(
            backend.ensure_device(WindowId::PRIMARY),
            Err(WgpuRenderError::BackendUnavailable)
        );
        assert_eq!(backend.state(), WgpuBackendState::Unavailable);
        assert_eq!(backend.last_error(), Some("device lost"));
        assert!(backend.instance.is_none());
        assert!(backend.adapter.is_none());
        assert!(backend.device.is_none());
        assert!(backend.queue.is_none());
    }
}
