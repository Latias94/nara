use std::{
    collections::BTreeMap,
    ffi::OsStr,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    time::Duration,
};

use nara_app::RuntimeGeneration;
use nara_ecs::Resource;
#[cfg(all(test, any(feature = "sprite-submitter", feature = "ui-submitter")))]
use nara_material::AlphaMode2d;
#[cfg(test)]
use nara_render::Color;
use nara_render::{
    Extent2d, FrameStats, RenderBackendState, RenderBackendStatus, RenderFrame,
    RenderFrameSkipReason, RenderFrameSourceGeneration, RenderFrameTarget, RenderTarget,
    RenderWindowPacket, ViewportRect,
};
#[cfg(feature = "ui-submitter")]
use nara_ui_render::{
    UiBackendSubmissionIdentity, UiPaintPacketIdentity, UiPresentationBackendGeneration,
};
use nara_window::{
    WindowId,
    backend::{BackendWindowHandles, WindowTargetError},
};

use crate::frame::{WgpuCapturedFrame, WgpuFramePayload};
use crate::presented::WgpuPresentedFrameReceipt;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::quad::{
    WgpuQuadPipeline, WgpuQuadPipelineDrawRef, WgpuQuadPipelineKey, admit_quad_instance_buffers,
    create_material_bind_group_layout, create_quad_batch_buffers, create_quad_pipeline,
    create_quad_texture_bind_group_layout, quad_batch_draw_stats,
};
use crate::surface::{
    SurfaceDropReason, WgpuConfiguredPresentMode, WgpuSurfaceState, configure_surface,
    create_surface, surface_extent,
};
use crate::telemetry::{
    WgpuAdapterSummary, WgpuBackendTelemetry, WgpuBackendTelemetryConfig,
    WgpuFrameCompletionSample, WgpuFrameCompletionStats, WgpuGpuResourceStats,
};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::texture::WgpuSpriteTextureCache;
use crate::{
    PreparedSubmitterDraw, PreparedTargetDraw, WGPU_RENDER_BACKEND, WgpuRenderError,
    render_acquired_texture,
};

static NEXT_WGPU_BACKEND_INSTANCE_ID: AtomicU64 = AtomicU64::new(1);
const WGPU_FORCE_FALLBACK_ADAPTER_ENV: &str = "NARA_WGPU_FORCE_FALLBACK";
const INVALID_FALLBACK_ADAPTER_ENV: &str = "NARA_WGPU_FORCE_FALLBACK must be either 0 or 1";
const TELEMETRY_COMPLETION_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

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
    app_frame_index: Option<u64>,
    render_transaction_frame_index: Option<u64>,
    packet_admissions: u64,
    packet_rejections: u64,
    surface_acquire_attempts: u64,
    surface_acquires: u64,
    queue_submissions: u64,
    presents: u64,
    #[cfg(feature = "ui-submitter")]
    ui_packet_count: u64,
    #[cfg(feature = "ui-submitter")]
    ui_packet_identity: Option<UiPaintPacketIdentity>,
    #[cfg(feature = "ui-submitter")]
    ui_primitives: u64,
    #[cfg(feature = "ui-submitter")]
    ui_glyph_primitives: u64,
    #[cfg(feature = "ui-submitter")]
    ui_glyph_pages: u64,
}

/// Immutable WGPU facts binding one exact UI submission to its submitted frame transaction.
///
/// UI payload counters are packet-local. Frame context, such as the submitted Sprite count, is
/// named explicitly. The record is produced only after surface submission succeeds and is retained
/// for the presentation owner to bind to the matching one-shot backend acceptance.
#[cfg(feature = "ui-submitter")]
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuSubmittedUiFacts {
    submission: UiBackendSubmissionIdentity,
    backend_generation: UiPresentationBackendGeneration,
    app_frame_index: u64,
    render_transaction_frame_index: u64,
    primitive_count: u64,
    image_count: u64,
    glyph_primitive_count: u64,
    glyph_page_count: u64,
    submitted_quad_count: u64,
    submitted_glyph_quad_count: u64,
    referenced_texture_binding_count: u64,
    frame_submitted_sprite_count: u64,
    frame_submitted_image_sprite_count: u64,
    submitted_texture_bytes: u64,
    submitted_instance_buffer_bytes: u64,
}

#[cfg(feature = "ui-submitter")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WgpuSubmittedFrameFacts {
    app_frame_index: u64,
    render_transaction_frame_index: u64,
    submitted_sprite_count: u32,
    submitted_image_sprite_count: u32,
    submitted_texture_bytes: u64,
    submitted_instance_buffer_bytes: u64,
}

#[cfg(feature = "ui-submitter")]
impl WgpuSubmittedFrameFacts {
    pub(crate) const fn new(
        app_frame_index: u64,
        render_transaction_frame_index: u64,
        submitted_sprite_count: u32,
        submitted_image_sprite_count: u32,
        submitted_texture_bytes: u64,
        submitted_instance_buffer_bytes: u64,
    ) -> Self {
        Self {
            app_frame_index,
            render_transaction_frame_index,
            submitted_sprite_count,
            submitted_image_sprite_count,
            submitted_texture_bytes,
            submitted_instance_buffer_bytes,
        }
    }
}

#[cfg(feature = "ui-submitter")]
impl WgpuSubmittedUiFacts {
    #[must_use]
    pub const fn submission(self) -> UiBackendSubmissionIdentity {
        self.submission
    }

    #[must_use]
    pub const fn backend_generation(self) -> UiPresentationBackendGeneration {
        self.backend_generation
    }

    #[must_use]
    pub const fn app_frame_index(self) -> u64 {
        self.app_frame_index
    }

    #[must_use]
    pub const fn render_transaction_frame_index(self) -> u64 {
        self.render_transaction_frame_index
    }

    #[must_use]
    pub const fn primitive_count(self) -> u64 {
        self.primitive_count
    }

    #[must_use]
    pub const fn image_count(self) -> u64 {
        self.image_count
    }

    #[must_use]
    pub const fn glyph_primitive_count(self) -> u64 {
        self.glyph_primitive_count
    }

    #[must_use]
    pub const fn glyph_page_count(self) -> u64 {
        self.glyph_page_count
    }

    #[must_use]
    pub const fn submitted_quad_count(self) -> u64 {
        self.submitted_quad_count
    }

    #[must_use]
    pub const fn submitted_glyph_quad_count(self) -> u64 {
        self.submitted_glyph_quad_count
    }

    #[must_use]
    pub const fn referenced_texture_binding_count(self) -> u64 {
        self.referenced_texture_binding_count
    }

    #[must_use]
    pub const fn frame_submitted_sprite_count(self) -> u64 {
        self.frame_submitted_sprite_count
    }

    #[must_use]
    pub const fn frame_submitted_image_sprite_count(self) -> u64 {
        self.frame_submitted_image_sprite_count
    }

    #[must_use]
    pub const fn submitted_texture_bytes(self) -> u64 {
        self.submitted_texture_bytes
    }

    #[must_use]
    pub const fn submitted_instance_buffer_bytes(self) -> u64 {
        self.submitted_instance_buffer_bytes
    }

    #[must_use]
    pub const fn submitted_total_bytes(self) -> u64 {
        self.submitted_texture_bytes
            .saturating_add(self.submitted_instance_buffer_bytes)
    }
}

impl WgpuFrameTransactionStats {
    #[must_use]
    pub const fn app_frame_index(self) -> Option<u64> {
        self.app_frame_index
    }

    #[must_use]
    pub const fn render_transaction_frame_index(self) -> Option<u64> {
        self.render_transaction_frame_index
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

    /// Returns the number of UI packets admitted for this exact backend frame transaction.
    #[cfg(feature = "ui-submitter")]
    #[must_use]
    pub const fn ui_packet_count(self) -> u64 {
        self.ui_packet_count
    }

    /// Returns the immutable packet identity when exactly one UI packet targeted this frame.
    #[cfg(feature = "ui-submitter")]
    #[must_use]
    pub const fn ui_packet_identity(self) -> Option<UiPaintPacketIdentity> {
        self.ui_packet_identity
    }

    /// Returns the expanded UI primitive count referenced by this exact frame transaction.
    #[cfg(feature = "ui-submitter")]
    #[must_use]
    pub const fn ui_primitives(self) -> u64 {
        self.ui_primitives
    }

    /// Returns the glyph primitive count referenced by this exact backend frame transaction.
    #[cfg(feature = "ui-submitter")]
    #[must_use]
    pub const fn ui_glyph_primitives(self) -> u64 {
        self.ui_glyph_primitives
    }

    /// Returns the glyph-atlas page reference count retained by this exact transaction.
    #[cfg(feature = "ui-submitter")]
    #[must_use]
    pub const fn ui_glyph_pages(self) -> u64 {
        self.ui_glyph_pages
    }

    #[cfg(feature = "ui-submitter")]
    pub(crate) fn record_ui_payload(&mut self, facts: crate::frame::WgpuUiPayloadFacts) {
        let previous_count = self.ui_packet_count;
        self.ui_packet_count = previous_count.saturating_add(facts.packet_count);
        self.ui_packet_identity = match (previous_count, facts.packet_count) {
            (_, 0) => self.ui_packet_identity,
            (0, 1) => facts.packet_identity,
            _ => None,
        };
        self.ui_primitives = self.ui_primitives.saturating_add(facts.primitives);
        self.ui_glyph_primitives = self
            .ui_glyph_primitives
            .saturating_add(facts.glyph_primitives);
        self.ui_glyph_pages = self.ui_glyph_pages.saturating_add(facts.glyph_pages);
    }

    fn begin(render_transaction_frame_index: u64, app_frame_index: Option<u64>) -> Self {
        Self {
            app_frame_index,
            render_transaction_frame_index: Some(render_transaction_frame_index),
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
    source_generation: Option<RenderFrameSourceGeneration>,
    last_frame_index: Option<u64>,
    frame_transaction_stats: WgpuFrameTransactionStats,
    presented_frame_this_transaction: Option<WgpuPresentedFrameReceipt>,
    #[cfg(feature = "ui-submitter")]
    submitted_ui_facts: Vec<WgpuSubmittedUiFacts>,
    telemetry: WgpuBackendTelemetry,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    quad_texture_bind_group_layout: Option<wgpu::BindGroupLayout>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    quad_material_bind_group_layout: Option<wgpu::BindGroupLayout>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    quad_pipelines: Vec<WgpuQuadPipeline>,
    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    quad_textures: WgpuSpriteTextureCache,
    last_error: Option<String>,
}

pub(super) struct WgpuEditorDeviceNamespace {
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) surface_format: wgpu::TextureFormat,
    pub(super) surface_extent: Extent2d,
    pub(super) backend_instance_id: u64,
    pub(super) device_epoch: u64,
}

pub(super) struct WgpuEditorAcquiredSurface {
    pub(super) texture: wgpu::SurfaceTexture,
    pub(super) surface_generation: u64,
    pub(super) format: wgpu::TextureFormat,
    pub(super) extent: Extent2d,
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

    /// Last requested mode accepted by surface configuration, independently of its selected fallback.
    #[must_use]
    pub fn requested_surface_present_mode(
        &self,
        window_id: WindowId,
    ) -> Option<nara_window::PresentMode> {
        self.surfaces.get(&window_id)?.requested_present_mode
    }

    #[must_use]
    pub fn configured_surface_present_mode(
        &self,
        window_id: WindowId,
    ) -> Option<WgpuConfiguredPresentMode> {
        self.surfaces
            .get(&window_id)?
            .config
            .as_ref()
            .map(|config| WgpuConfiguredPresentMode::from_wgpu(config.present_mode))
    }

    #[must_use]
    pub const fn frame_transaction_stats(&self) -> WgpuFrameTransactionStats {
        self.frame_transaction_stats
    }

    pub(crate) const fn presented_frame_this_transaction(
        &self,
    ) -> Option<WgpuPresentedFrameReceipt> {
        self.presented_frame_this_transaction
    }

    /// Returns the exact submitted facts for one presentation attempt in the latest transaction.
    #[cfg(feature = "ui-submitter")]
    #[doc(hidden)]
    #[must_use]
    pub(crate) fn submitted_ui_facts(
        &self,
        submission: UiBackendSubmissionIdentity,
        backend_generation: UiPresentationBackendGeneration,
    ) -> Option<WgpuSubmittedUiFacts> {
        self.submitted_ui_facts.iter().copied().find(|facts| {
            facts.submission == submission && facts.backend_generation == backend_generation
        })
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

    /// Returns bounded diagnostic information for the currently selected adapter.
    ///
    /// This is backend-local observation data. It is not a persistent adapter identity and must
    /// not be used to select or reconstruct a device.
    #[must_use]
    pub fn adapter_summary(&self) -> Option<WgpuAdapterSummary> {
        self.adapter.as_ref().map(|adapter| {
            let info = adapter.get_info();
            WgpuAdapterSummary::from_parts(
                &info.name,
                info.vendor,
                info.device,
                device_type_label(info.device_type),
                backend_label(info.backend),
                &info.driver,
                &info.driver_info,
            )
        })
    }

    /// Enables finite GPU completion observations for this backend instance.
    ///
    /// Reconfiguration discards any pending or undrained observations and accounts them as lost.
    /// It does not discard resource-retirement records from the same device epoch. Call this
    /// before the first submitted frame when GPU resource peaks will be used as evidence; any
    /// earlier instance-buffer submission is retained conservatively and reported as a lost
    /// retirement.
    pub fn configure_telemetry(&mut self, config: WgpuBackendTelemetryConfig) {
        self.telemetry.configure(config);
    }

    #[must_use]
    pub fn frame_completion_stats(&self) -> WgpuFrameCompletionStats {
        self.telemetry.completion_stats()
    }

    /// Drains at most the configured number of completed frame observations.
    pub fn drain_frame_completion_samples(&mut self) -> Vec<WgpuFrameCompletionSample> {
        self.telemetry.drain_completed_frames()
    }

    /// Drives completion callbacks once without blocking the render loop.
    ///
    /// Candidate probes call this at their ordinary frame boundary. Waiting for GPU completion is
    /// reserved for an explicit measurement end boundary so the probe cannot turn normal frame
    /// timing into a CPU/GPU synchronization benchmark.
    pub fn poll_telemetry_completions(&self) -> Result<(), WgpuRenderError> {
        let Some(device) = self.device.as_ref() else {
            return Ok(());
        };
        device
            .poll(wgpu::PollType::Poll)
            .map(|_| ())
            .map_err(|_| WgpuRenderError::TelemetryCompletionPollFailed)
    }

    #[must_use]
    pub fn gpu_resource_stats(&self) -> WgpuGpuResourceStats {
        self.telemetry.gpu_resource_stats()
    }

    /// Waits for the latest submitted GPU work and drives completion callbacks.
    ///
    /// This is a bounded diagnostic drain for an explicit measurement end boundary. Normal frame
    /// execution must rely on regular queue submission and polling instead of calling it.
    pub fn wait_for_telemetry_completions(&self, timeout: Duration) -> Result<(), WgpuRenderError> {
        let device = self
            .device
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        let timeout = timeout.min(TELEMETRY_COMPLETION_WAIT_TIMEOUT);
        if timeout.is_zero() {
            return Err(WgpuRenderError::TelemetryCompletionWaitTimeout);
        }
        device
            .poll(wgpu::PollType::Wait {
                submission_index: None,
                timeout: Some(timeout),
            })
            .map(|_| ())
            .map_err(|error| match error {
                wgpu::PollError::Timeout => WgpuRenderError::TelemetryCompletionWaitTimeout,
                wgpu::PollError::WrongSubmissionIndex(_, _) => {
                    WgpuRenderError::TelemetryCompletionWaitFailed
                }
            })
    }

    pub(super) fn begin_frame_transaction(
        &mut self,
        render_transaction_frame_index: u64,
        app_frame_index: Option<u64>,
    ) {
        self.frame_transaction_stats =
            WgpuFrameTransactionStats::begin(render_transaction_frame_index, app_frame_index);
        self.presented_frame_this_transaction = None;
        #[cfg(feature = "ui-submitter")]
        self.clear_submitted_ui_facts();
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
            self.quad_material_bind_group_layout = None;
            self.quad_pipelines.clear();
            self.quad_textures.clear();
        }
        #[cfg(feature = "ui-submitter")]
        self.clear_submitted_ui_facts();
        self.presented_frame_this_transaction = None;
        self.telemetry.retire_device_epoch();
        retirement_result
    }

    #[cfg(feature = "ui-submitter")]
    fn clear_submitted_ui_facts(&mut self) {
        self.submitted_ui_facts.clear();
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

    pub(super) fn prepare_editor_window(
        &mut self,
        handles: &BackendWindowHandles,
        window: RenderWindowPacket,
    ) -> Result<WgpuEditorDeviceNamespace, WgpuRenderError> {
        self.fail_if_device_lost()?;
        self.retire_requested_surfaces()?;
        if self.state == WgpuBackendState::Unavailable {
            return Err(WgpuRenderError::BackendUnavailable);
        }
        let size = surface_extent(
            window.resolution.physical_width,
            window.resolution.physical_height,
        )
        .ok_or_else(|| WgpuRenderError::ContentRejected {
            message: "the Editor window target has zero physical extent".to_owned(),
        })?;
        self.ensure_instance()?;
        if !self.create_surface_if_missing(window, handles, size)? {
            return Err(WgpuRenderError::SurfaceMissing {
                window_id: window.id,
            });
        }
        self.ensure_device(window.id)?;
        self.fail_if_device_lost()?;
        self.configure_surface_if_needed(window, size)?;
        let surface_format = self.surface_view_format(window.id)?;
        Ok(WgpuEditorDeviceNamespace {
            device: self
                .device
                .as_ref()
                .ok_or(WgpuRenderError::BackendNotReady)?
                .clone(),
            queue: self
                .queue
                .as_ref()
                .ok_or(WgpuRenderError::BackendNotReady)?
                .clone(),
            surface_format,
            surface_extent: size,
            backend_instance_id: self.instance_id(),
            device_epoch: self.device_epoch(),
        })
    }

    pub(super) fn acquire_editor_surface(
        &mut self,
        window_id: WindowId,
    ) -> Result<Option<WgpuEditorAcquiredSurface>, WgpuRenderError> {
        let surface = self
            .surfaces
            .get_mut(&window_id)
            .ok_or(WgpuRenderError::SurfaceMissing { window_id })?;
        if !surface.can_acquire_frame() {
            return Ok(None);
        }
        let surface_generation = surface
            .surface_generation()
            .ok_or(WgpuRenderError::SurfaceMissing { window_id })?;
        let config = surface
            .config
            .as_ref()
            .ok_or(WgpuRenderError::SurfaceUnconfigured { window_id })?;
        let format = config.format.add_srgb_suffix();
        let extent = surface.size;
        self.frame_transaction_stats.surface_acquire_attempts = self
            .frame_transaction_stats
            .surface_acquire_attempts
            .saturating_add(1);
        let acquired = surface.surface(window_id)?.get_current_texture();
        match acquired {
            wgpu::CurrentSurfaceTexture::Success(texture) => {
                self.frame_transaction_stats.surface_acquires = self
                    .frame_transaction_stats
                    .surface_acquires
                    .saturating_add(1);
                Ok(Some(WgpuEditorAcquiredSurface {
                    texture,
                    surface_generation,
                    format,
                    extent,
                }))
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                self.frame_transaction_stats.surface_acquires = self
                    .frame_transaction_stats
                    .surface_acquires
                    .saturating_add(1);
                surface.dirty = true;
                Ok(Some(WgpuEditorAcquiredSurface {
                    texture,
                    surface_generation,
                    format,
                    extent,
                }))
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                Ok(None)
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                surface.dirty = true;
                Ok(None)
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.retire_surface(window_id, SurfaceDropReason::SurfaceOrDeviceLost)?;
                Ok(None)
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                Err(WgpuRenderError::SurfaceValidation { window_id })
            }
        }
    }

    pub(super) fn render_packet(
        &mut self,
        handles: Option<&BackendWindowHandles>,
        captured: WgpuCapturedFrame,
        #[cfg(feature = "ui-submitter")] mut ui_submissions: Option<
            &mut crate::frame::WgpuUiSubmissionState,
        >,
        frame: &mut RenderFrame,
        stats: &mut FrameStats,
        status: &mut RenderBackendStatus,
    ) -> Result<(), WgpuRenderError> {
        let WgpuCapturedFrame {
            topology: packet,
            payload,
            app_frame_index,
            work_started_at,
        } = captured;
        let source_generation = packet.source_generation();
        stats.draw_calls = 0;
        stats.sprites = 0;
        self.begin_frame_transaction(packet.frame_index(), Some(app_frame_index));
        let runtime_generation = source_generation.runtime_generation().ok_or_else(|| {
            WgpuRenderError::ContentRejected {
                message: "the runtime WGPU backend requires a managed-runtime packet".to_owned(),
            }
        })?;
        if let Err(error) = self.admit_packet(source_generation, packet.frame_index()) {
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
            return Err(WgpuRenderError::BackendUnavailable);
        }

        let RenderFrameTarget::Window(window) = packet.target() else {
            return Err(WgpuRenderError::ContentRejected {
                message: "image render targets require the private Editor execution owner"
                    .to_owned(),
            });
        };
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
        #[cfg(feature = "ui-submitter")]
        if let Some(ui_submissions) = ui_submissions.as_deref_mut() {
            let backend_generation = nara_ui_render::UiPresentationBackendGeneration::new(
                self.instance_id(),
                self.device_epoch(),
            )
            .ok_or_else(|| WgpuRenderError::DeviceUnavailable {
                message: "the initialized WGPU device has no non-zero presentation generation"
                    .to_owned(),
            })?;
            ui_submissions.bind_backend(backend_generation)?;
        }
        status.mark_ready(WGPU_RENDER_BACKEND);
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        self.quad_textures.begin_frame(frame.index);
        self.telemetry
            .observe_gpu_resources(self.logical_texture_bytes(), 0);
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
        #[cfg(feature = "ui-submitter")]
        {
            let ui = payload.ui_facts(packet.views()[0].target);
            self.frame_transaction_stats.record_ui_payload(ui);
        }
        let draw = match self.prepare_target_draw(view_format, &packet, &payload, frame.index) {
            Ok(draw) => draw,
            Err(error) => {
                self.telemetry
                    .observe_gpu_resources(self.logical_texture_bytes(), 0);
                return Err(error);
            }
        };
        let submitted_texture_bytes = self.logical_texture_bytes();
        let submitted_instance_buffer_bytes = draw.instance_buffer_bytes();
        #[cfg(feature = "ui-submitter")]
        let submitted_frame_facts = WgpuSubmittedFrameFacts::new(
            app_frame_index,
            packet.frame_index(),
            draw.sprites,
            draw.image_sprites,
            submitted_texture_bytes,
            submitted_instance_buffer_bytes,
        );
        #[cfg(feature = "ui-submitter")]
        let submitted_ui_facts = ui_submissions
            .as_deref()
            .map(|ui_submissions| {
                Self::prepare_submitted_ui_facts(
                    ui_submissions.bound_submissions(),
                    &draw.ui_packets,
                    submitted_frame_facts,
                )
            })
            .transpose()?;
        self.telemetry
            .observe_gpu_resources(submitted_texture_bytes, submitted_instance_buffer_bytes);
        let presented = self.render_window(
            window.id,
            &draw,
            runtime_generation,
            app_frame_index,
            packet.frame_index(),
            work_started_at,
        );
        self.telemetry.finish_prepared_gpu_resources();
        let presented = presented?;

        if let Some(presented) = presented {
            self.presented_frame_this_transaction = Some(presented);
            #[cfg(feature = "ui-submitter")]
            if let Some(facts) = submitted_ui_facts {
                self.publish_submitted_ui_facts(facts);
            }
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

    #[cfg(feature = "ui-submitter")]
    pub(crate) fn prepare_submitted_ui_facts(
        submissions: &[nara_ui_render::__private::UiSubmittedBackendSubmission],
        lowered: &[crate::ui::WgpuLoweredUiPacketFacts],
        frame: WgpuSubmittedFrameFacts,
    ) -> Result<Vec<WgpuSubmittedUiFacts>, WgpuRenderError> {
        let mut submitted_facts = Vec::with_capacity(submissions.len());
        for submission in submissions {
            let submitted = submission.submission();
            let identity = submitted.identity();
            let mut matches = lowered.iter().filter(|facts| {
                facts.packet == identity.packet() && facts.target == identity.target()
            });
            let Some(lowered) = matches.next() else {
                return Err(WgpuRenderError::UiSubmissionRejected {
                    message: format!(
                        "UI backend attempt {} has no lowered packet for its exact target",
                        identity.admission_id().get()
                    ),
                });
            };
            if matches.next().is_some() {
                return Err(WgpuRenderError::UiSubmissionRejected {
                    message: format!(
                        "UI backend attempt {} matches more than one lowered packet",
                        identity.admission_id().get()
                    ),
                });
            }
            submitted_facts.push(WgpuSubmittedUiFacts {
                submission: identity,
                backend_generation: submitted.backend_generation(),
                app_frame_index: frame.app_frame_index,
                render_transaction_frame_index: frame.render_transaction_frame_index,
                primitive_count: lowered.primitive_count,
                image_count: lowered.image_count,
                glyph_primitive_count: lowered.glyph_primitive_count,
                glyph_page_count: lowered.glyph_page_count,
                submitted_quad_count: lowered.submitted_quad_count,
                submitted_glyph_quad_count: lowered.submitted_glyph_quad_count,
                referenced_texture_binding_count: crate::saturating_u64(lowered.resources.len()),
                frame_submitted_sprite_count: u64::from(frame.submitted_sprite_count),
                frame_submitted_image_sprite_count: u64::from(frame.submitted_image_sprite_count),
                submitted_texture_bytes: frame.submitted_texture_bytes,
                submitted_instance_buffer_bytes: frame.submitted_instance_buffer_bytes,
            });
        }
        Ok(submitted_facts)
    }

    #[cfg(feature = "ui-submitter")]
    pub(crate) fn publish_submitted_ui_facts(&mut self, facts: Vec<WgpuSubmittedUiFacts>) {
        self.submitted_ui_facts.extend(facts);
    }

    pub(super) fn admit_packet(
        &mut self,
        generation: RenderFrameSourceGeneration,
        frame_index: u64,
    ) -> Result<(), WgpuRenderError> {
        if let Some(expected) = self.source_generation {
            if expected != generation {
                return Err(WgpuRenderError::StaleFrameGeneration {
                    expected,
                    actual: generation,
                });
            }
        } else {
            self.source_generation = Some(generation);
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

    pub(super) const fn editor_source_generation(&self) -> Option<RenderFrameSourceGeneration> {
        self.source_generation
    }

    pub(super) fn activate_editor_source(&mut self, generation: RenderFrameSourceGeneration) {
        if self.source_generation == Some(generation) {
            return;
        }
        self.source_generation = Some(generation);
        self.last_frame_index = None;
        self.presented_frame_this_transaction = None;
        #[cfg(feature = "ui-submitter")]
        self.clear_submitted_ui_facts();
    }

    pub(super) fn record_editor_image_submission(&mut self) {
        self.frame_transaction_stats.packet_admissions = 1;
        self.frame_transaction_stats.queue_submissions = self
            .frame_transaction_stats
            .queue_submissions
            .saturating_add(1);
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    pub(super) fn begin_editor_submitter_frame(&mut self, frame_index: u64) {
        self.quad_textures.begin_frame(frame_index);
    }

    pub(super) fn record_editor_window_presentation(&mut self) {
        self.frame_transaction_stats.queue_submissions = self
            .frame_transaction_stats
            .queue_submissions
            .saturating_add(1);
        self.frame_transaction_stats.presents =
            self.frame_transaction_stats.presents.saturating_add(1);
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
        self.telemetry.begin_device_epoch(device_epoch);
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
        if self
            .presented_frame_this_transaction
            .is_some_and(|receipt| receipt.window_id() == window_id)
        {
            self.presented_frame_this_transaction = None;
        }
        let Some(surface) = self.surfaces.remove(&window_id) else {
            return Ok(());
        };
        surface.retire(reason)
    }

    fn render_window(
        &mut self,
        window_id: WindowId,
        draw: &PreparedTargetDraw,
        runtime_generation: RuntimeGeneration,
        app_frame_index: u64,
        render_transaction_frame_index: u64,
        work_started_at: std::time::Instant,
    ) -> Result<Option<WgpuPresentedFrameReceipt>, WgpuRenderError> {
        let backend_instance_id = self.instance_id();
        let device_epoch = self.device_epoch;
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
        let surface_generation = surface_state
            .surface_generation()
            .ok_or(WgpuRenderError::SurfaceMissing { window_id })?;
        let presented_receipt = || {
            WgpuPresentedFrameReceipt::new(
                runtime_generation,
                window_id,
                surface_generation,
                backend_instance_id,
                device_epoch,
                app_frame_index,
                render_transaction_frame_index,
            )
        };

        self.frame_transaction_stats.surface_acquire_attempts += 1;
        match surface_state.surface(window_id)?.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(texture) => {
                self.frame_transaction_stats.surface_acquires += 1;
                render_acquired_texture(&device, &queue, window_id, surface_state, texture, draw)?;
                if let Some(completion) = self.telemetry.begin_frame_submission(
                    app_frame_index,
                    render_transaction_frame_index,
                    self.device_epoch,
                    work_started_at,
                ) {
                    queue.on_submitted_work_done(move || completion.complete());
                }
                self.frame_transaction_stats.queue_submissions += 1;
                self.frame_transaction_stats.presents += 1;
                Ok(Some(presented_receipt()))
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                self.frame_transaction_stats.surface_acquires += 1;
                render_acquired_texture(&device, &queue, window_id, surface_state, texture, draw)?;
                if let Some(completion) = self.telemetry.begin_frame_submission(
                    app_frame_index,
                    render_transaction_frame_index,
                    self.device_epoch,
                    work_started_at,
                ) {
                    queue.on_submitted_work_done(move || completion.complete());
                }
                self.frame_transaction_stats.queue_submissions += 1;
                self.frame_transaction_stats.presents += 1;
                surface_state.dirty = true;
                Ok(Some(presented_receipt()))
            }
            wgpu::CurrentSurfaceTexture::Timeout | wgpu::CurrentSurfaceTexture::Occluded => {
                Ok(None)
            }
            wgpu::CurrentSurfaceTexture::Outdated => {
                surface_state.dirty = true;
                Ok(None)
            }
            wgpu::CurrentSurfaceTexture::Lost => {
                self.retire_surface(window_id, SurfaceDropReason::SurfaceOrDeviceLost)?;
                Ok(None)
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
        if surface.needs_configuration(size, window.present_mode) {
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

    pub(super) fn prepare_target_draw(
        &mut self,
        format: wgpu::TextureFormat,
        packet: &nara_render::RenderFramePacket,
        payload: &WgpuFramePayload,
        frame_index: u64,
    ) -> Result<PreparedTargetDraw, WgpuRenderError> {
        let mut prepared = PreparedTargetDraw::default();
        for (index, view) in packet.views().iter().enumerate() {
            let draw = self.prepare_submitter_draw(
                format,
                view.target,
                view.viewport,
                Some(index),
                payload,
                frame_index,
            )?;
            prepared.push(view.viewport, view.load, draw);
        }
        #[cfg(feature = "ui-submitter")]
        {
            let viewport = ViewportRect::from_extent(packet.target().extent())
                .expect("admitted target extent");
            let draw = self.prepare_submitter_draw(
                format,
                packet.views()[0].target,
                viewport,
                None,
                payload,
                frame_index,
            )?;
            prepared.push(viewport, nara_render::ViewLoadOp::Load, draw);
        }
        Ok(prepared)
    }

    #[cfg(all(test, any(feature = "sprite-submitter", feature = "ui-submitter")))]
    pub(crate) fn with_probe_device(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        Self {
            device: Some(device),
            queue: Some(queue),
            ..Self::default()
        }
    }

    pub(super) fn prepare_submitter_draw(
        &mut self,
        view_format: wgpu::TextureFormat,
        target: RenderTarget,
        viewport: ViewportRect,
        view_index: Option<usize>,
        payload: &WgpuFramePayload,
        frame_index: u64,
    ) -> Result<PreparedSubmitterDraw, WgpuRenderError> {
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        {
            let lowered = payload.lower_draw(target, viewport, view_index)?;
            let batches = lowered.batches;
            #[cfg(feature = "ui-submitter")]
            let ui_packets = lowered.ui_packets;
            let stats = quad_batch_draw_stats(&batches);
            if batches.is_empty() {
                return Ok(PreparedSubmitterDraw {
                    #[cfg(feature = "ui-submitter")]
                    ui_packets,
                    ..PreparedSubmitterDraw::default()
                });
            }
            admit_quad_instance_buffers(&batches).map_err(|error| {
                WgpuRenderError::ContentRejected {
                    message: error.to_string(),
                }
            })?;
            let pipeline_keys = crate::required_quad_pipeline_keys(&batches, view_format);
            let texture_layout = self.ensure_quad_texture_bind_group_layout()?;
            let material_layout = self.ensure_quad_material_bind_group_layout()?;
            for key in &pipeline_keys {
                self.ensure_quad_pipeline(*key)?;
            }
            let pipelines = self.quad_pipeline_draw_refs(&pipeline_keys)?;
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
                &material_layout,
                &mut self.quad_textures,
                frame_index,
            )
            .map_err(|error| {
                if error.is_deferred() {
                    WgpuRenderError::UploadDeferred {
                        message: error.to_string(),
                    }
                } else {
                    WgpuRenderError::ContentRejected {
                        message: error.to_string(),
                    }
                }
            })?;
            Ok(PreparedSubmitterDraw {
                pipelines,
                buffers,
                #[cfg(feature = "ui-submitter")]
                ui_packets,
                draw_calls: stats.draw_calls,
                sprites: stats.sprites,
                #[cfg(feature = "ui-submitter")]
                image_sprites: stats.image_sprites,
            })
        }

        #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
        {
            let _ = (
                view_format,
                target,
                viewport,
                view_index,
                payload,
                frame_index,
            );
            Ok(PreparedSubmitterDraw::default())
        }
    }

    fn logical_texture_bytes(&self) -> u64 {
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        {
            self.quad_textures.logical_texture_bytes()
        }
        #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
        {
            0
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
    fn ensure_quad_material_bind_group_layout(
        &mut self,
    ) -> Result<wgpu::BindGroupLayout, WgpuRenderError> {
        if let Some(layout) = &self.quad_material_bind_group_layout {
            return Ok(layout.clone());
        }
        let device = self
            .device
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        let layout = create_material_bind_group_layout(device);
        self.quad_material_bind_group_layout = Some(layout.clone());
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
        let material_layout = self.ensure_quad_material_bind_group_layout()?;
        let device = self
            .device
            .as_ref()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        self.quad_pipelines.push(create_quad_pipeline(
            device,
            key,
            &texture_layout,
            &material_layout,
        )?);
        Ok(())
    }

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
    fn quad_pipeline_draw_refs(
        &self,
        keys: &[WgpuQuadPipelineKey],
    ) -> Result<Vec<WgpuQuadPipelineDrawRef>, WgpuRenderError> {
        keys.iter()
            .map(|key| {
                let pipeline = self
                    .quad_pipelines
                    .iter()
                    .find(|pipeline| pipeline.key == *key)
                    .ok_or_else(|| WgpuRenderError::QuadPipelineMissing {
                        format: format!("{:?}", key.format),
                        alpha_mode: format!("{:?}", key.alpha_mode),
                    })?;
                Ok(WgpuQuadPipelineDrawRef {
                    alpha_mode: key.alpha_mode,
                    material: key.material,
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

const fn device_type_label(device_type: wgpu::DeviceType) -> &'static str {
    match device_type {
        wgpu::DeviceType::Other => "other",
        wgpu::DeviceType::IntegratedGpu => "integrated-gpu",
        wgpu::DeviceType::DiscreteGpu => "discrete-gpu",
        wgpu::DeviceType::VirtualGpu => "virtual-gpu",
        wgpu::DeviceType::Cpu => "cpu",
    }
}

const fn backend_label(backend: wgpu::Backend) -> &'static str {
    match backend {
        wgpu::Backend::Noop => "noop",
        wgpu::Backend::Vulkan => "vulkan",
        wgpu::Backend::Metal => "metal",
        wgpu::Backend::Dx12 => "dx12",
        wgpu::Backend::Gl => "gl",
        wgpu::Backend::BrowserWebGpu => "browser-webgpu",
    }
}

pub(super) fn render_backend_state(state: WgpuBackendState) -> RenderBackendState {
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

    fn presented_receipt(
        backend: &WgpuRenderBackend,
        runtime_generation: RuntimeGeneration,
        window_id: WindowId,
        surface_generation: u64,
    ) -> WgpuPresentedFrameReceipt {
        WgpuPresentedFrameReceipt::new(
            runtime_generation,
            window_id,
            surface_generation,
            backend.instance_id(),
            backend.device_epoch(),
            10,
            9,
        )
    }

    #[test]
    fn surface_generation_comes_from_the_retained_lease_authority() {
        let window_id = WindowId::PRIMARY;
        let handles = BackendWindowHandles::default();
        handles
            .insert(
                window_id,
                WindowHandleProvider::new(Arc::new(TestWindowSource)),
            )
            .unwrap();
        let first = WgpuSurfaceState::lease_only(
            handles.acquire_surface(window_id).unwrap(),
            Extent2d::new(320, 180).unwrap(),
        );
        assert_eq!(first.surface_generation(), Some(1));
        first
            .retire(SurfaceDropReason::SurfaceOrDeviceLost)
            .unwrap();

        let retained = WgpuSurfaceState::lease_only(
            handles.acquire_surface(window_id).unwrap(),
            Extent2d::new(320, 180).unwrap(),
        );
        assert_eq!(retained.surface_generation(), Some(2));

        let replacement_handles = BackendWindowHandles::default();
        replacement_handles
            .insert(
                window_id,
                WindowHandleProvider::new(Arc::new(TestWindowSource)),
            )
            .unwrap();
        let (_replacement_source, replacement_lease) = replacement_handles
            .acquire_surface(window_id)
            .unwrap()
            .into_parts();
        assert_eq!(replacement_lease.surface_generation(), 1);
        assert_eq!(retained.surface_generation(), Some(2));
    }

    #[test]
    fn presented_receipt_is_current_transaction_and_target_scoped() {
        let runtime_generation = test_runtime_generation();
        let (mut backend, _handles) = backend_with_lease_only_surface(WindowId::PRIMARY);
        backend.device_epoch = 7;
        let surface_generation = backend
            .surfaces
            .get(&WindowId::PRIMARY)
            .and_then(WgpuSurfaceState::surface_generation)
            .unwrap();
        let receipt = presented_receipt(
            &backend,
            runtime_generation,
            WindowId::PRIMARY,
            surface_generation,
        );
        backend.presented_frame_this_transaction = Some(receipt);
        assert_eq!(backend.presented_frame_this_transaction(), Some(receipt));

        backend.begin_frame_transaction(10, Some(11));
        assert_eq!(backend.presented_frame_this_transaction(), None);

        backend.presented_frame_this_transaction = Some(receipt);
        backend
            .retire_surface(WindowId::PRIMARY, SurfaceDropReason::SurfaceOrDeviceLost)
            .unwrap();
        assert_eq!(backend.presented_frame_this_transaction(), None);
    }

    #[test]
    fn presentation_counters_cannot_forge_a_presented_receipt() {
        let mut backend = WgpuRenderBackend::default();
        backend.frame_transaction_stats.presents = 1;

        assert_eq!(backend.frame_transaction_stats().presents(), 1);
        assert_eq!(backend.presented_frame_this_transaction(), None);
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
    fn backend_completion_telemetry_is_bounded_and_opt_in() {
        let mut backend = WgpuRenderBackend::default();
        assert!(!backend.frame_completion_stats().enabled());

        backend
            .configure_telemetry(crate::telemetry::WgpuBackendTelemetryConfig::new(2, 3).unwrap());

        let stats = backend.frame_completion_stats();
        assert!(stats.enabled());
        assert_eq!(stats.pending_samples(), 0);
        assert_eq!(stats.buffered_samples(), 0);
        assert!(backend.drain_frame_completion_samples().is_empty());
    }

    #[test]
    fn backend_cleanup_quarantines_pending_completion_and_resource_tokens() {
        let mut backend = WgpuRenderBackend::default();
        backend.configure_telemetry(WgpuBackendTelemetryConfig::new(2, 2).unwrap());
        backend.telemetry.begin_device_epoch(7);
        backend.telemetry.observe_gpu_resources(4, 64);
        let obsolete = backend
            .telemetry
            .begin_frame_submission(10, 10, 7, std::time::Instant::now())
            .unwrap();
        backend.telemetry.finish_prepared_gpu_resources();

        backend
            .clear_gpu_resources(SurfaceDropReason::BackendCleanup)
            .unwrap();
        obsolete.complete();

        let completion = backend.frame_completion_stats();
        assert_eq!(completion.pending_samples(), 0);
        assert_eq!(completion.completed_samples(), 0);
        assert_eq!(completion.lost_samples(), 1);
        let resources = backend.gpu_resource_stats();
        assert_eq!(resources.pending_retirements(), 0);
        assert_eq!(resources.lost_retirements(), 1);
        assert_eq!(resources.current_total_bytes(), 0);
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
            texture: crate::quad::WgpuQuadTextureKey::Fallback,
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::Opaque,
        };

        let predecessor_namespace;
        {
            let mut predecessor = WgpuRenderBackend {
                device_epoch: 1,
                ..WgpuRenderBackend::default()
            };
            let _binding = predecessor
                .quad_textures
                .bind_group(
                    &device,
                    &queue,
                    &layout,
                    predecessor_key,
                    &crate::quad::WgpuQuadTexture::Fallback,
                    7,
                )
                .unwrap();
            predecessor_namespace = (predecessor.instance_id(), predecessor.device_epoch());
            assert_eq!(
                predecessor.texture_stats(),
                crate::WgpuRenderTextureCacheStats {
                    fallback_bindings: 1,
                    has_fallback_texture: true,
                    texture_bytes: 4,
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
            mapping: nara_render::ViewMapping2d::orthographic(
                nara_core::Mat3::IDENTITY,
                720.0,
                ViewportRect::new(0, 0, 1280, 720).unwrap(),
            )
            .unwrap(),
            order: 0,
            load: nara_render::ViewLoadOp::Clear(Color::BLACK),
        });
        let topology = build_render_frame_packet(
            generation,
            frame_index,
            Some(WindowId::PRIMARY),
            [&window],
            &views,
        )
        .unwrap()
        .unwrap();
        WgpuCapturedFrame {
            topology,
            payload: WgpuFramePayload::default(),
            app_frame_index: frame_index,
            work_started_at: std::time::Instant::now(),
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
        #[cfg(feature = "ui-submitter")]
        let mut ui_submissions = crate::frame::WgpuUiSubmissionState::Prepared(Vec::new());
        backend.render_packet(
            None,
            packet,
            #[cfg(feature = "ui-submitter")]
            Some(&mut ui_submissions),
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

        backend.source_generation = Some(generation.into());
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
                app_frame_index: Some(12),
                render_transaction_frame_index: Some(12),
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
                app_frame_index: Some(10),
                render_transaction_frame_index: Some(10),
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

    #[test]
    fn unavailable_backend_rejects_an_admitted_frame() {
        let generation = test_runtime_generation();
        let mut backend = WgpuRenderBackend {
            state: WgpuBackendState::Unavailable,
            source_generation: Some(generation.into()),
            last_error: Some("device lost".to_owned()),
            ..WgpuRenderBackend::default()
        };

        assert_eq!(
            render_packet_without_surface(&mut backend, test_packet(generation, 1)),
            Err(WgpuRenderError::BackendUnavailable)
        );
        assert_eq!(backend.state(), WgpuBackendState::Unavailable);
        assert_eq!(backend.last_error(), Some("device lost"));
    }
}
