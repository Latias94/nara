use std::{
    collections::BTreeMap,
    sync::Arc,
    time::{Duration, Instant},
};

#[cfg(feature = "hud-runtime")]
use crate::hud::{
    UiBackendSubmissionIdentity, UiPaintPacketIdentity, UiPresentationBackendGeneration,
};
use crate::{Extent2d, RenderBackendState, RenderWindowPacket};
use nara_window::{
    WindowId,
    backend::{BackendWindowHandles, WindowTargetError},
};

use crate::gpu::WgpuRenderError;
use crate::gpu::device::WgpuDeviceOwner;
use crate::gpu::presented::WgpuPresentedFrameReceipt;
use crate::gpu::runtime_timing::{
    RuntimeFramePhaseCapture, WgpuNamespacePhase, WgpuNamespaceTimings, WgpuRuntimeFramePhase,
    WgpuRuntimeFrameTimings,
};
#[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
use crate::gpu::stock_quad::StockQuadResources;
use crate::gpu::surface::{
    SurfaceDropReason, WgpuSurfaceState, configure_surface, create_surface, surface_extent,
};
use crate::gpu::telemetry::{
    WgpuAdapterSummary, WgpuBackendTelemetry, WgpuBackendTelemetryConfig,
    WgpuFrameCompletionSample, WgpuFrameCompletionStats, WgpuGpuResourceStats,
};

const TELEMETRY_COMPLETION_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

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
    direct_surface_presents: u64,
    candidate_instance_buffer_demand_bytes: u64,
    submitted_instance_buffer_demand_bytes: u64,
    native_work: crate::gpu::WgpuNativeWorkStats,
    runtime_timings: Option<WgpuRuntimeFrameTimings>,
    #[cfg(feature = "hud-runtime")]
    ui_packet_count: u64,
    #[cfg(feature = "hud-runtime")]
    ui_packet_identity: Option<UiPaintPacketIdentity>,
    #[cfg(feature = "hud-runtime")]
    ui_primitives: u64,
    #[cfg(feature = "hud-runtime")]
    ui_glyph_primitives: u64,
    #[cfg(feature = "hud-runtime")]
    ui_glyph_pages: u64,
}

/// Immutable WGPU facts binding one exact UI submission to its submitted frame transaction.
///
/// UI payload counters are packet-local. Frame context, such as the submitted Sprite count, is
/// named explicitly. The record is produced only after surface submission succeeds and is retained
/// for the presentation owner to bind to the matching one-shot backend acceptance.
#[cfg(feature = "hud-runtime")]
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
    mesh_primitive_count: u64,
    submitted_mesh_count: u64,
    submitted_mesh_vertex_count: u64,
    submitted_mesh_triangle_count: u64,
    referenced_texture_binding_count: u64,
    frame_submitted_sprite_count: u64,
    frame_submitted_image_sprite_count: u64,
    submitted_texture_bytes: u64,
    submitted_instance_buffer_bytes: u64,
}

#[cfg(feature = "hud-runtime")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WgpuSubmittedFrameFacts {
    app_frame_index: u64,
    render_transaction_frame_index: u64,
    submitted_sprite_count: u32,
    submitted_image_sprite_count: u32,
    submitted_texture_bytes: u64,
    submitted_instance_buffer_bytes: u64,
}

#[cfg(feature = "hud-runtime")]
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

#[cfg(feature = "hud-runtime")]
impl WgpuSubmittedUiFacts {
    pub(crate) fn set_resource_bytes(&mut self, texture_bytes: u64, instance_buffer_bytes: u64) {
        self.submitted_texture_bytes = texture_bytes;
        self.submitted_instance_buffer_bytes = instance_buffer_bytes;
    }
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

    /// Mesh primitives in the original packet, including completely clipped geometry.
    #[must_use]
    pub const fn mesh_primitive_count(self) -> u64 {
        self.mesh_primitive_count
    }

    /// Encoded mesh draws; these are not counted as quads or sprites.
    #[must_use]
    pub const fn submitted_mesh_count(self) -> u64 {
        self.submitted_mesh_count
    }

    /// Submitted mesh vertex occurrences, including repeated immutable geometry.
    #[must_use]
    pub const fn submitted_mesh_vertex_count(self) -> u64 {
        self.submitted_mesh_vertex_count
    }

    /// Submitted indexed triangles before raster clipping.
    #[must_use]
    pub const fn submitted_mesh_triangle_count(self) -> u64 {
        self.submitted_mesh_triangle_count
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
    /// Returns opt-in wall spans for returned calls in this original Runtime transaction.
    ///
    /// `None` means phase timing was disabled at the transaction boundary. An enabled snapshot
    /// can have absent phases after early rejection or unwind. Spans include returned errors and
    /// driver waits; they are neither complete frame durations nor success receipts.
    /// Only the managed Runtime path records these phases.
    #[must_use]
    pub const fn runtime_timings(self) -> Option<WgpuRuntimeFrameTimings> {
        self.runtime_timings
    }

    /// Returns issued preparation/program work for all attempts in this transaction.
    /// Publication, failure, completion and resource retirement do not refund these counts.
    #[must_use]
    pub const fn native_work(self) -> crate::gpu::WgpuNativeWorkStats {
        self.native_work
    }

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

    /// Successful runtime presents rendered directly into their acquired surface attachments.
    #[must_use]
    pub const fn direct_surface_presents(self) -> u64 {
        self.direct_surface_presents
    }

    /// Scope demand held by prepared draws that have not been submitted yet.
    ///
    /// Dropping a candidate refunds this demand. This does not measure GPU residency.
    #[must_use]
    pub const fn candidate_instance_buffer_demand_bytes(self) -> u64 {
        self.candidate_instance_buffer_demand_bytes
    }

    /// Cumulative instance demand submitted in this transaction, until the next transaction.
    ///
    /// GPU completion does not refund this demand. Physical completion accounting is separate.
    #[must_use]
    pub const fn submitted_instance_buffer_demand_bytes(self) -> u64 {
        self.submitted_instance_buffer_demand_bytes
    }

    /// Returns the number of UI packets admitted for this exact backend frame transaction.
    #[cfg(feature = "hud-runtime")]
    #[must_use]
    pub const fn ui_packet_count(self) -> u64 {
        self.ui_packet_count
    }

    /// Returns the immutable packet identity when exactly one UI packet targeted this frame.
    #[cfg(feature = "hud-runtime")]
    #[must_use]
    pub const fn ui_packet_identity(self) -> Option<UiPaintPacketIdentity> {
        self.ui_packet_identity
    }

    /// Returns the expanded UI primitive count referenced by this exact frame transaction.
    #[cfg(feature = "hud-runtime")]
    #[must_use]
    pub const fn ui_primitives(self) -> u64 {
        self.ui_primitives
    }

    /// Returns the glyph primitive count referenced by this exact backend frame transaction.
    #[cfg(feature = "hud-runtime")]
    #[must_use]
    pub const fn ui_glyph_primitives(self) -> u64 {
        self.ui_glyph_primitives
    }

    /// Returns the glyph-atlas page reference count retained by this exact transaction.
    #[cfg(feature = "hud-runtime")]
    #[must_use]
    pub const fn ui_glyph_pages(self) -> u64 {
        self.ui_glyph_pages
    }

    #[cfg(feature = "hud-runtime")]
    pub(crate) fn record_ui_payload(&mut self, facts: crate::gpu::frame::WgpuUiPayloadFacts) {
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

/// Concrete GPU execution owner. Runners and Editor Hosts retain it outside gameplay World.
#[derive(Debug, Default)]
pub(crate) struct WgpuRenderBackend {
    // Rust drops fields in declaration order. Retire surfaces while their GPU owners are live.
    surfaces: BTreeMap<WindowId, WgpuSurfaceState>,
    frame_transaction_stats: WgpuFrameTransactionStats,
    runtime_frame_timings_enabled: bool,
    presented_frame_this_transaction: Option<WgpuPresentedFrameReceipt>,
    #[cfg(feature = "hud-runtime")]
    submitted_ui_facts: Vec<WgpuSubmittedUiFacts>,
    telemetry: WgpuBackendTelemetry,
    #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
    stock_quad: StockQuadResources,
    last_error: Option<String>,
    runtime_close_started: bool,
    runtime_retirement: Option<crate::gpu::retirement::WgpuGpuRetirement>,
    resource_budget: Arc<crate::gpu::output::OutputBudget>,
    // Keep the device domain alive through surface/cache field destruction on direct Drop too.
    device_owner: WgpuDeviceOwner,
}

pub(super) struct WgpuDeviceNamespace {
    pub(super) epoch_lost: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub(super) instance: wgpu::Instance,
    pub(super) adapter: wgpu::Adapter,
    pub(super) device: wgpu::Device,
    pub(super) queue: wgpu::Queue,
    pub(super) surface: Option<(wgpu::TextureFormat, Extent2d)>,
    pub(super) backend_instance_id: u64,
    pub(super) device_epoch: u64,
}

pub(super) struct WgpuEditorAcquiredSurface {
    pub(super) texture: wgpu::SurfaceTexture,
    pub(super) surface_generation: u64,
    pub(super) format: wgpu::TextureFormat,
    pub(super) extent: Extent2d,
    pub(super) reconfigure_after_present: bool,
}

impl WgpuRenderBackend {
    pub(super) fn with_gpu_context(
        context: crate::gpu::GpuContext,
    ) -> Result<Self, WgpuRenderError> {
        Ok(Self {
            device_owner: WgpuDeviceOwner::with_context(context)?,
            ..Self::default()
        })
    }

    pub(crate) fn resource_budget(&self) -> &Arc<crate::gpu::output::OutputBudget> {
        &self.resource_budget
    }

    pub(super) const fn runtime_close_started(&self) -> bool {
        self.runtime_close_started
    }

    /// Stops this Runtime execution owner and advances its original queue and surface retirement.
    /// A retry retains the same drain even after namespace invalidation or a native poll error.
    #[doc(hidden)]
    pub fn poll_runtime_retirement(
        &mut self,
        window_ids: &[WindowId],
    ) -> Result<bool, WgpuRenderError> {
        self.runtime_close_started = true;
        let discard = self.state() == WgpuBackendState::Unavailable;
        if let Some(retirement) = self.runtime_retirement.as_mut() {
            retirement.begin(discard);
            if !retirement
                .poll()
                .map_err(|error| WgpuRenderError::RetirementPollFailed {
                    message: error.to_string(),
                })?
            {
                return Ok(false);
            }
        }
        self.retire_targets(window_ids, SurfaceDropReason::TargetShutdown)?;
        self.clear_gpu_resources(SurfaceDropReason::BackendCleanup)?;
        Ok(true)
    }

    /// Process-local non-reused identity of this backend owner.
    ///
    /// Together with [`Self::device_epoch`], this identifies one device namespace. The value is
    /// diagnostic runtime state and must not be persisted or used as backend-neutral identity.
    #[must_use]
    pub const fn instance_id(&self) -> u64 {
        self.device_owner.instance_id()
    }

    #[must_use]
    pub const fn state(&self) -> WgpuBackendState {
        self.device_owner.state()
    }

    #[cfg(test)]
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
    pub fn frame_transaction_stats(&self) -> WgpuFrameTransactionStats {
        let stats = self.frame_transaction_stats;
        #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
        let stats = {
            let mut stats = stats;
            (
                stats.candidate_instance_buffer_demand_bytes,
                stats.submitted_instance_buffer_demand_bytes,
            ) = self.stock_quad.instance_charges();
            stats
        };
        stats
    }

    pub(crate) fn record_native_work(&mut self, work: crate::gpu::WgpuNativeWorkStats) {
        self.frame_transaction_stats.native_work.accumulate(work);
    }

    /// Transfers already admitted CPU facts to the World without exposing native execution.
    pub(crate) fn take_observation(&mut self) -> crate::gpu::WgpuRenderObservation {
        let extent = self
            .presented_frame_this_transaction
            .and_then(|receipt| self.configured_surface_extent(receipt.window_id()));
        let mut observation = crate::gpu::WgpuRenderObservation::from_parts(
            self.instance_id(),
            self.device_epoch(),
            self.state(),
            self.frame_transaction_stats(),
            self.presented_frame_this_transaction,
            #[cfg(feature = "hud-runtime")]
            std::mem::take(&mut self.submitted_ui_facts),
        );
        observation.record_presented_surface_extent(extent);
        observation
    }

    #[cfg(test)]
    pub(crate) const fn presented_frame_this_transaction(
        &self,
    ) -> Option<WgpuPresentedFrameReceipt> {
        self.presented_frame_this_transaction
    }

    /// Returns the exact submitted facts for one presentation attempt in the latest transaction.
    #[cfg(all(test, feature = "hud-runtime"))]
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
        self.device_owner.device_epoch()
    }

    /// Registers immutable Feature prerequisites before the first device acquisition.
    ///
    /// Late contributions must fit the enabled original request. A stronger contribution returns
    /// a reconstruction requirement and never replaces a healthy device or changes its epoch.
    pub(crate) fn request_device_requirements(
        &mut self,
        requirements: &crate::gpu::requirements::WgpuDeviceRequirements,
    ) -> Result<(), crate::gpu::requirements::WgpuDeviceRequirementsError> {
        self.device_owner.request_device_requirements(requirements)
    }

    /// Returns bounded diagnostic information for the currently selected adapter.
    ///
    /// This is backend-local observation data. It is not a persistent adapter identity and must
    /// not be used to select or reconstruct a device.
    #[must_use]
    pub fn adapter_summary(&self) -> Option<WgpuAdapterSummary> {
        self.device_owner.adapter_summary()
    }

    /// Enables finite GPU completion observations for this backend instance.
    ///
    /// Reconfiguration discards any pending or undrained observations and accounts them as lost.
    /// It does not discard resource-retirement records from the same device epoch. Call this
    /// before the first submitted frame when GPU resource peaks will be used as evidence; any
    /// earlier instance-buffer submission is retained conservatively and reported as a lost
    /// retirement.
    pub fn configure_telemetry(&mut self, config: WgpuBackendTelemetryConfig) {
        self.runtime_frame_timings_enabled = config.runtime_frame_timings_enabled();
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
        let Some((device, _)) = self.device_owner.device_queue() else {
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
        let (device, _) = self
            .device_owner
            .device_queue()
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

    pub(super) fn wait_for_editor_idle(&self, timeout: Duration) -> Result<(), WgpuRenderError> {
        self.fail_if_device_lost()?;
        let (_, queue) = self
            .device_owner
            .device_queue()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        // Toolkit initialization can leave write_texture/write_buffer transfers pending before
        // any rendered frame. Polling alone does not submit those transfers for completion.
        queue.submit([]);
        self.wait_for_telemetry_completions(timeout)
    }

    pub(super) fn begin_frame_transaction(
        &mut self,
        render_transaction_frame_index: u64,
        app_frame_index: Option<u64>,
    ) {
        self.frame_transaction_stats =
            WgpuFrameTransactionStats::begin(render_transaction_frame_index, app_frame_index);
        self.frame_transaction_stats.runtime_timings = self
            .runtime_frame_timings_enabled
            .then(WgpuRuntimeFrameTimings::default);
        #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
        {
            self.stock_quad.reset_instance_budget();
        }
        self.presented_frame_this_transaction = None;
        #[cfg(feature = "hud-runtime")]
        self.clear_submitted_ui_facts();
    }

    pub(crate) fn begin_runtime_frame_phase(&self) -> RuntimeFramePhaseCapture {
        RuntimeFramePhaseCapture::start(self.frame_transaction_stats.runtime_timings.is_some())
    }

    pub(crate) fn finish_runtime_frame_phase(
        &mut self,
        phase: WgpuRuntimeFramePhase,
        capture: RuntimeFramePhaseCapture,
    ) {
        capture.finish(|elapsed| {
            if let Some(timings) = &mut self.frame_transaction_stats.runtime_timings {
                timings.record(phase, elapsed);
            }
        });
    }

    pub(crate) fn record_runtime_namespace_timings(
        &mut self,
        details: Option<WgpuNamespaceTimings>,
    ) {
        if let (Some(details), Some(timings)) =
            (details, &mut self.frame_transaction_stats.runtime_timings)
        {
            *timings.namespace_mut() = details;
        }
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
        self.device_owner.invalidate_presentations();
        #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
        {
            self.stock_quad.clear();
        }
        #[cfg(feature = "hud-runtime")]
        self.clear_submitted_ui_facts();
        self.presented_frame_this_transaction = None;
        self.device_owner.clear();
        self.telemetry.retire_device_epoch();
        retirement_result
    }

    #[cfg(feature = "hud-runtime")]
    fn clear_submitted_ui_facts(&mut self) {
        self.submitted_ui_facts.clear();
    }

    #[cfg(all(test, any(feature = "sprite-render", feature = "hud-runtime")))]
    #[must_use]
    pub fn texture_stats(&self) -> crate::gpu::WgpuRenderTextureCacheStats {
        self.stock_quad.texture_stats()
    }

    #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
    pub(crate) fn stock_quad_resources(&mut self) -> &mut StockQuadResources {
        &mut self.stock_quad
    }

    pub(super) fn prepare_editor_window(
        &mut self,
        handles: &BackendWindowHandles,
        window: RenderWindowPacket,
        mut namespace_timing: Option<&mut WgpuNamespaceTimings>,
    ) -> Result<WgpuDeviceNamespace, WgpuRenderError> {
        self.fail_if_device_lost()?;
        self.retire_requested_surfaces()?;
        if self.state() == WgpuBackendState::Unavailable {
            return Err(WgpuRenderError::BackendUnavailable);
        }
        let size = surface_extent(
            window.resolution.physical_width,
            window.resolution.physical_height,
        );
        let size = if let Some(size) = size {
            let timing = RuntimeFramePhaseCapture::start(namespace_timing.is_some());
            let instance = self.ensure_instance();
            timing.finish(|elapsed| {
                if let Some(timings) = namespace_timing.as_deref_mut() {
                    timings.record(WgpuNamespacePhase::InstancePreparation, elapsed);
                }
            });
            instance?;
            let timing = RuntimeFramePhaseCapture::start(namespace_timing.is_some());
            let created = self.create_surface_if_missing(window, handles, size);
            timing.finish(|elapsed| {
                if let Some(timings) = namespace_timing.as_deref_mut() {
                    timings.record(WgpuNamespacePhase::SurfaceCreation, elapsed);
                }
            });
            if !created? {
                return Err(WgpuRenderError::SurfaceMissing {
                    window_id: window.id,
                });
            }
            self.ensure_device(window.id, namespace_timing.as_deref_mut())?;
            self.fail_if_device_lost()?;
            let timing = RuntimeFramePhaseCapture::start(namespace_timing.is_some());
            let configured = self.configure_surface_if_needed(window, size);
            timing.finish(|elapsed| {
                if let Some(timings) = namespace_timing {
                    timings.record(WgpuNamespacePhase::SurfaceConfiguration, elapsed);
                }
            });
            configured?;
            size
        } else {
            self.configured_surface_extent(window.id)
                .ok_or(WgpuRenderError::BackendNotReady)?
        };
        let surface_format = self.surface_view_format(window.id)?;
        let ready = self.device_owner.ready()?;
        Ok(WgpuDeviceNamespace {
            epoch_lost: self.device_owner.output_epoch_guard()?,
            instance: self.device_owner.instance()?.clone(),
            adapter: ready.adapter.clone(),
            device: ready.device.clone(),
            queue: ready.queue.clone(),
            surface: Some((surface_format, size)),
            backend_instance_id: self.instance_id(),
            device_epoch: self.device_epoch(),
        })
    }

    /// Uses the normal device owner without manufacturing a platform target or Surface.
    pub(super) fn prepare_image_device(&mut self) -> Result<WgpuDeviceNamespace, WgpuRenderError> {
        self.device_owner.fail_if_device_lost()?;
        if let Some(epoch) = self.device_owner.acquire(None, None)? {
            self.telemetry.begin_device_epoch(epoch);
            self.last_error = None;
        }
        self.retain_runtime_retirement()?;
        let ready = self.device_owner.ready()?;
        Ok(WgpuDeviceNamespace {
            epoch_lost: self.device_owner.output_epoch_guard()?,
            instance: self.device_owner.instance()?.clone(),
            adapter: ready.adapter.clone(),
            device: ready.device.clone(),
            queue: ready.queue.clone(),
            surface: None,
            backend_instance_id: self.instance_id(),
            device_epoch: self.device_epoch(),
        })
    }

    pub(super) fn check_device_health(&self) -> Result<(), WgpuRenderError> {
        self.device_owner.fail_if_device_lost()
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
        self.frame_transaction_stats.surface_acquire_attempts = self
            .frame_transaction_stats
            .surface_acquire_attempts
            .saturating_add(1);
        let acquired = surface.surface(window_id)?.get_current_texture();
        match acquired {
            wgpu::CurrentSurfaceTexture::Success(texture) => {
                let acquired_size = texture.texture.size();
                let extent = Extent2d {
                    width: acquired_size.width,
                    height: acquired_size.height,
                };
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
                    reconfigure_after_present: false,
                }))
            }
            wgpu::CurrentSurfaceTexture::Suboptimal(texture) => {
                let acquired_size = texture.texture.size();
                let extent = Extent2d {
                    width: acquired_size.width,
                    height: acquired_size.height,
                };
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
                    reconfigure_after_present: true,
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

    /// Completes only the exact surface attempt that reached successful native presentation.
    ///
    /// Acquisition leaves the configuration dirty until this point. Dropping an unpresented
    /// Vulkan texture does not reset its acquire semaphore in the selected WGPU implementation;
    /// refusal or unwind therefore requires reconfiguration before the next acquisition.
    pub(super) fn complete_editor_surface_presentation(
        &mut self,
        window: WindowId,
        generation: u64,
        reconfigure_after_present: bool,
    ) {
        if let Some(surface) = self.surfaces.get_mut(&window)
            && surface.surface_generation() == Some(generation)
        {
            surface.dirty = reconfigure_after_present;
        }
    }

    #[cfg(feature = "hud-runtime")]
    pub(crate) fn prepare_submitted_ui_facts(
        submissions: &[crate::hud::presentation::UiSubmittedBackendSubmission],
        lowered: &[crate::gpu::ui::WgpuLoweredUiPacketFacts],
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
                mesh_primitive_count: lowered.mesh_primitive_count,
                submitted_mesh_count: lowered.submitted_mesh_count,
                submitted_mesh_vertex_count: lowered.submitted_mesh_vertex_count,
                submitted_mesh_triangle_count: lowered.submitted_mesh_triangle_count,
                referenced_texture_binding_count: crate::gpu::saturating_u64(
                    lowered.resources.len(),
                ),
                frame_submitted_sprite_count: u64::from(frame.submitted_sprite_count),
                frame_submitted_image_sprite_count: u64::from(frame.submitted_image_sprite_count),
                submitted_texture_bytes: frame.submitted_texture_bytes,
                submitted_instance_buffer_bytes: frame.submitted_instance_buffer_bytes,
            });
        }
        Ok(submitted_facts)
    }

    #[cfg(feature = "hud-runtime")]
    pub(crate) fn publish_submitted_ui_facts(&mut self, facts: Vec<WgpuSubmittedUiFacts>) {
        // The closed Runtime transaction prepares one complete batch before native submission.
        // Move that buffer through publication; extend would allocate again after acceptance.
        debug_assert!(self.submitted_ui_facts.is_empty());
        self.submitted_ui_facts = facts;
    }

    pub(super) fn reject_editor_packet(&mut self) {
        self.frame_transaction_stats.packet_rejections = self
            .frame_transaction_stats
            .packet_rejections
            .saturating_add(1);
    }

    pub(super) fn record_editor_image_submission(&mut self) {
        self.frame_transaction_stats.packet_admissions = self
            .frame_transaction_stats
            .packet_admissions
            .saturating_add(1);
        self.frame_transaction_stats.queue_submissions = self
            .frame_transaction_stats
            .queue_submissions
            .saturating_add(1);
    }

    pub(crate) fn record_runtime_packet_admission(&mut self) {
        self.frame_transaction_stats.packet_admissions = self
            .frame_transaction_stats
            .packet_admissions
            .saturating_add(1);
    }

    pub(crate) fn record_runtime_program_submission(&mut self) {
        self.frame_transaction_stats.queue_submissions = self
            .frame_transaction_stats
            .queue_submissions
            .saturating_add(1);
    }

    /// Publishes an actual Runtime surface present after program submission was counted.
    pub(crate) fn commit_runtime_presentation(
        &mut self,
        receipt: WgpuPresentedFrameReceipt,
        direct: bool,
    ) {
        self.presented_frame_this_transaction = Some(receipt);
        self.frame_transaction_stats.presents =
            self.frame_transaction_stats.presents.saturating_add(1);
        if direct {
            self.frame_transaction_stats.direct_surface_presents = self
                .frame_transaction_stats
                .direct_surface_presents
                .saturating_add(1);
        }
    }

    #[cfg(feature = "hud-runtime")]
    pub(crate) fn record_runtime_ui_payload(
        &mut self,
        facts: crate::gpu::frame::WgpuUiPayloadFacts,
    ) {
        self.frame_transaction_stats.record_ui_payload(facts);
    }

    /// Tracks already submitted work on the original queue without submitting or counting again.
    pub(crate) fn track_runtime_submission(
        &self,
        app_frame_index: u64,
        render_transaction_frame_index: u64,
        work_started_at: Instant,
    ) -> Result<(), WgpuRenderError> {
        let (_, queue) = self
            .device_owner
            .device_queue()
            .ok_or(WgpuRenderError::BackendNotReady)?;
        if let Some(completion) = self.telemetry.begin_frame_submission(
            app_frame_index,
            render_transaction_frame_index,
            self.device_epoch(),
            work_started_at,
        ) {
            queue.on_submitted_work_done(move || completion.complete());
        }
        Ok(())
    }

    pub(crate) fn observe_program_resources(&self, texture_bytes: u64, instance_buffer_bytes: u64) {
        self.telemetry
            .observe_gpu_resources(texture_bytes, instance_buffer_bytes);
    }

    pub(crate) fn finish_program_resources(&self) {
        self.telemetry.finish_prepared_gpu_resources();
    }

    #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
    pub(super) fn begin_editor_submitter_frame(&mut self, frame_index: u64) {
        self.stock_quad.begin_frame(frame_index);
    }

    #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
    #[cfg(test)]
    pub(crate) fn reserve_instance_buffer_bytes(
        &self,
        bytes: u64,
    ) -> Result<crate::gpu::budget::InstanceReservation, crate::gpu::budget::InstanceBudgetError>
    {
        self.stock_quad.reserve_instance_buffer_bytes(bytes)
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
        self.device_owner.ensure_instance()
    }

    fn ensure_device(
        &mut self,
        window_id: WindowId,
        namespace_timing: Option<&mut WgpuNamespaceTimings>,
    ) -> Result<(), WgpuRenderError> {
        if self.state() == WgpuBackendState::Unavailable {
            return Err(WgpuRenderError::BackendUnavailable);
        }
        let surface = self
            .surfaces
            .get(&window_id)
            .ok_or(WgpuRenderError::SurfaceMissing { window_id })?
            .surface(window_id)?;
        if let Some(epoch) = self.device_owner.acquire(Some(surface), namespace_timing)? {
            self.telemetry.begin_device_epoch(epoch);
            self.last_error = None;
        }
        self.retain_runtime_retirement()
    }

    fn retain_runtime_retirement(&mut self) -> Result<(), WgpuRenderError> {
        if !self.device_owner.is_external() && self.runtime_retirement.is_none() {
            let (device, queue) = self
                .device_owner
                .device_queue()
                .ok_or(WgpuRenderError::BackendNotReady)?;
            self.runtime_retirement = Some(crate::gpu::retirement::WgpuGpuRetirement::new(
                device, queue,
            ));
        }
        Ok(())
    }

    fn fail_if_device_lost(&self) -> Result<(), WgpuRenderError> {
        self.device_owner.fail_if_device_lost()
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
            let instance = self.device_owner.instance()?;
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
        let ready = self.device_owner.ready()?;
        let surface = self
            .surfaces
            .get_mut(&window.id)
            .ok_or(WgpuRenderError::SurfaceMissing {
                window_id: window.id,
            })?;
        if surface.needs_configuration(size, window.present_mode) {
            configure_surface(
                surface,
                &ready.adapter,
                &ready.device,
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

    pub(super) fn retain_editor_world_presentation(
        &self,
        view: crate::gpu::host::WgpuRenderProducerId,
        revoker: crate::runtime_integration::WorldPresentationRevoker,
    ) -> Result<(), WgpuRenderError> {
        self.device_owner.retain_editor_presentation(view, revoker)
    }

    pub(super) fn retire_editor_world_presentation(
        &self,
        view: crate::gpu::host::WgpuRenderProducerId,
    ) {
        self.device_owner.retire_editor_presentation(view);
    }

    pub(crate) fn logical_texture_bytes(&self) -> u64 {
        #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
        {
            self.stock_quad.logical_texture_bytes()
        }
        #[cfg(not(any(feature = "sprite-render", feature = "hud-runtime")))]
        {
            0
        }
    }

    pub(super) fn mark_error(&mut self, error: &WgpuRenderError) -> String {
        let cleanup_error = self
            .clear_gpu_resources(SurfaceDropReason::SurfaceOrDeviceLost)
            .err();
        self.device_owner.mark_unavailable();
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
    use std::{num::NonZeroU64, sync::Arc};

    #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
    use crate::material::AlphaMode2d;
    use nara_app::{
        App, RuntimeCandidate, RuntimeCandidateRetirementState, RuntimeClosePolicy,
        RuntimeGeneration, RuntimeObligationLedger,
    };
    use nara_window::backend::{WindowHandleProvider, WindowTargetPhase};
    use raw_window_handle::{
        DisplayHandle, HandleError, HasDisplayHandle, HasWindowHandle, WindowHandle,
    };

    use super::*;
    #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
    use crate::gpu::quad::create_quad_texture_bind_group_layout;

    #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
    fn backend_with_probe_device(device: &wgpu::Device, queue: &wgpu::Queue) -> WgpuRenderBackend {
        let mut backend = WgpuRenderBackend::default();
        backend
            .device_owner
            .install_probe_device(device.clone(), queue.clone());
        backend
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
        backend.device_owner = WgpuDeviceOwner::with_retired_epoch(NonZeroU64::new(7).unwrap());
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
            device_owner: WgpuDeviceOwner::with_retired_epoch(NonZeroU64::new(7).unwrap()),
            ..WgpuRenderBackend::default()
        };

        backend
            .clear_gpu_resources(SurfaceDropReason::BackendCleanup)
            .unwrap();

        assert_eq!(backend.device_epoch(), 7);
        assert_eq!(backend.state(), WgpuBackendState::Uninitialized);
        assert!(backend.device_owner.ready().is_err());
    }

    #[test]
    fn backend_completion_telemetry_is_bounded_and_opt_in() {
        let mut backend = WgpuRenderBackend::default();
        assert!(!backend.frame_completion_stats().enabled());

        backend.configure_telemetry(
            crate::gpu::telemetry::WgpuBackendTelemetryConfig::new(2, 3).unwrap(),
        );

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

    #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
    #[test]
    fn cumulative_instance_budget_rejects_two_admissible_views_in_one_frame() {
        let mut backend = WgpuRenderBackend::default();
        backend.begin_frame_transaction(1, None);

        let per_view = 40 * 1024 * 1024;
        let first = backend
            .reserve_instance_buffer_bytes(per_view)
            .expect("the first view fits the cumulative frame budget");
        assert!(
            backend.reserve_instance_buffer_bytes(per_view).is_err(),
            "two individually admissible views must not exceed one 64 MiB scope"
        );
        drop(first);
        let recovered = backend
            .reserve_instance_buffer_bytes(64 * 1024 * 1024)
            .expect("dropping an uncommitted view must refund its reservation");
        drop(recovered);
    }

    #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
    #[test]
    fn replacing_frame_scope_does_not_let_old_reservation_charge_the_new_scope() {
        let mut backend = WgpuRenderBackend::default();
        backend.begin_frame_transaction(1, None);
        let mut old_reservation = backend
            .reserve_instance_buffer_bytes(40 * 1024 * 1024)
            .unwrap();
        old_reservation.commit();
        backend.begin_frame_transaction(2, None);
        drop(old_reservation);
        let new_reservation = backend
            .reserve_instance_buffer_bytes(64 * 1024 * 1024)
            .expect("the new scope starts empty even while the old holder is dropped");
        drop(new_reservation);
    }

    #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
    #[test]
    fn new_frame_demand_is_independent_of_older_physical_completion() {
        let mut backend = WgpuRenderBackend::default();
        backend.configure_telemetry(WgpuBackendTelemetryConfig::new(2, 2).unwrap());
        backend.telemetry.begin_device_epoch(7);
        backend.begin_frame_transaction(1, Some(1));
        let mut old_reservation = backend.reserve_instance_buffer_bytes(64).unwrap();
        old_reservation.commit();
        backend.telemetry.observe_gpu_resources(4, 64);
        let old_completion = backend
            .telemetry
            .begin_frame_submission(1, 1, 7, std::time::Instant::now())
            .unwrap();
        backend.telemetry.finish_prepared_gpu_resources();

        backend.begin_frame_transaction(2, Some(2));
        let scope = backend.frame_transaction_stats();
        assert_eq!(scope.candidate_instance_buffer_demand_bytes(), 0);
        assert_eq!(scope.submitted_instance_buffer_demand_bytes(), 0);
        assert_eq!(
            backend.gpu_resource_stats().current_instance_buffer_bytes(),
            64
        );
        assert_eq!(backend.gpu_resource_stats().pending_retirements(), 1);

        let mut new_reservation = backend.reserve_instance_buffer_bytes(32).unwrap();
        assert_eq!(
            backend
                .frame_transaction_stats()
                .candidate_instance_buffer_demand_bytes(),
            32
        );
        new_reservation.commit();
        backend.telemetry.observe_gpu_resources(4, 32);
        let new_completion = backend
            .telemetry
            .begin_frame_submission(2, 2, 7, std::time::Instant::now())
            .unwrap();
        backend.telemetry.finish_prepared_gpu_resources();
        assert_eq!(
            backend.gpu_resource_stats().current_instance_buffer_bytes(),
            96
        );
        assert_eq!(backend.gpu_resource_stats().pending_retirements(), 2);

        old_completion.complete();
        drop(old_reservation);
        assert_eq!(
            backend.gpu_resource_stats().current_instance_buffer_bytes(),
            32
        );
        assert_eq!(backend.gpu_resource_stats().pending_retirements(), 1);
        assert_eq!(
            backend
                .frame_transaction_stats()
                .submitted_instance_buffer_demand_bytes(),
            32
        );
        new_completion.complete();
        drop(new_reservation);
        assert_eq!(
            backend.gpu_resource_stats().current_instance_buffer_bytes(),
            0
        );
        assert_eq!(backend.gpu_resource_stats().pending_retirements(), 0);
        assert_eq!(
            backend
                .frame_transaction_stats()
                .submitted_instance_buffer_demand_bytes(),
            32,
            "physical completion cannot refund demand in the current frame"
        );
    }

    #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
    #[test]
    fn replacement_backend_renews_device_namespace_and_starts_with_an_empty_texture_cache() {
        use crate::gpu::quad::WgpuQuadMaterialKey;
        use crate::gpu::readback_tests::ReadbackGpu;
        use crate::material::SamplerDescriptor;

        let gpu = ReadbackGpu::new();
        let layout = create_quad_texture_bind_group_layout(&gpu.device).unwrap();
        let predecessor_key = WgpuQuadMaterialKey {
            texture: crate::gpu::quad::WgpuQuadTextureKey::Fallback,
            sampler: SamplerDescriptor::default(),
            alpha_mode: AlphaMode2d::Opaque,
        };

        let (predecessor_namespace, prefix) = gpu.with_preparation(|preparation| {
            let mut predecessor = backend_with_probe_device(&gpu.device, &gpu.queue);
            let _binding = predecessor.stock_quad.textures_mut().bind_group(
                preparation,
                &layout,
                predecessor_key,
                &crate::gpu::quad::WgpuQuadTexture::Fallback,
                7,
            )?;
            let predecessor_namespace = (predecessor.instance_id(), predecessor.device_epoch());
            assert_eq!(
                predecessor.texture_stats(),
                crate::gpu::WgpuRenderTextureCacheStats {
                    fallback_bindings: 1,
                    has_fallback_texture: true,
                    texture_bytes: 4,
                    ..crate::gpu::WgpuRenderTextureCacheStats::default()
                }
            );
            Ok((predecessor_namespace, None))
        });
        gpu.wait_for_prefix(&prefix);
        let predecessor_namespace = predecessor_namespace.unwrap();

        // The test-owned Device is deliberately shared. Runtime-local backend identity and cache
        // ownership must remain fresh even when a process parent outlives the predecessor.
        let replacement = backend_with_probe_device(&gpu.device, &gpu.queue);
        let replacement_namespace = (replacement.instance_id(), replacement.device_epoch());
        assert_ne!(replacement_namespace, predecessor_namespace);
        assert_eq!(
            replacement.texture_stats(),
            crate::gpu::WgpuRenderTextureCacheStats::default(),
            "the replacement cache must not contain the predecessor material key or statistics",
        );
    }

    #[cfg(any(feature = "sprite-render", feature = "hud-runtime"))]
    #[test]
    fn reconstructed_backend_readmits_exact_image_against_replacement_device_limits() {
        use crate::gpu::quad::{WgpuQuadMaterialKey, WgpuQuadTexture, WgpuQuadTextureKey};
        use crate::gpu::readback_tests::{
            image_versions, render_to_rgba, render_to_rgba_with_limits,
        };
        use crate::gpu::texture::WgpuTextureError;
        use crate::material::SamplerDescriptor;

        let [snapshot, _] = image_versions(4_096, 1);
        let material = WgpuQuadMaterialKey {
            texture: WgpuQuadTextureKey::Image(snapshot.identity().key()),
            sampler: SamplerDescriptor::NEAREST_CLAMP,
            alpha_mode: AlphaMode2d::Opaque,
        };
        let (_, (mut backend, old_binding)) =
            render_to_rgba(|device, queue, _, _, _, preparation| {
                let mut backend = backend_with_probe_device(device, queue);
                let layout = create_quad_texture_bind_group_layout(device)?;
                let binding = backend
                    .stock_quad
                    .textures_mut()
                    .bind_group(
                        preparation,
                        &layout,
                        material,
                        &WgpuQuadTexture::Image(snapshot.clone()),
                        1,
                    )
                    .unwrap();
                assert_eq!(backend.texture_stats().uploaded_images, 1);
                Ok((backend, binding))
            })
            .unwrap();
        backend
            .clear_gpu_resources(SurfaceDropReason::BackendCleanup)
            .unwrap();
        assert_eq!(backend.logical_texture_bytes(), 4_096 * 4);

        render_to_rgba_with_limits(
            wgpu::Limits {
                max_texture_dimension_2d: 2_048,
                ..Default::default()
            },
            |device, queue, _, _, _, preparation| {
                backend
                    .device_owner
                    .install_probe_device(device.clone(), queue.clone());
                let layout = create_quad_texture_bind_group_layout(device)?;
                let error = backend
                    .stock_quad
                    .textures_mut()
                    .bind_group(
                        preparation,
                        &layout,
                        material,
                        &WgpuQuadTexture::Image(snapshot),
                        2,
                    )
                    .unwrap_err();
                assert_eq!(
                    error,
                    WgpuTextureError::TextureExtentUnsupported {
                        texture: material.texture,
                        width: 4_096,
                        height: 1,
                        maximum: 2_048,
                    },
                );
                assert_eq!(backend.texture_stats().image_bindings, 0);
                assert_eq!(backend.texture_stats().uploaded_images, 0);
                assert_eq!(backend.texture_stats().reused_images, 0);
                drop(old_binding);
                assert_eq!(backend.logical_texture_bytes(), 0);
                let [legal, _] = image_versions(2_048, 1);
                backend
                    .stock_quad
                    .textures_mut()
                    .prepared_image_view(preparation, &legal, 2)
                    .unwrap();
                assert_eq!(backend.logical_texture_bytes(), 2_048 * 4);
                assert_eq!(backend.texture_stats().uploaded_images, 1);
                assert_eq!(backend.device_epoch(), 2);
                Ok(())
            },
        )
        .unwrap();
    }

    fn test_runtime_generation() -> RuntimeGeneration {
        let candidate = RuntimeCandidate::admit(
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

    #[test]
    fn backend_starts_without_surfaces_or_native_state() {
        let backend = WgpuRenderBackend::default();
        assert_eq!(backend.state(), WgpuBackendState::Uninitialized);
        assert_eq!(backend.surface_count(), 0);
        assert_eq!(backend.last_error(), None);
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
    fn device_loss_remains_rejected_until_the_owner_is_retired() {
        let backend = WgpuRenderBackend {
            device_owner: WgpuDeviceOwner::with_loss_observation(),
            ..WgpuRenderBackend::default()
        };
        assert!(matches!(
            backend.fail_if_device_lost(),
            Err(WgpuRenderError::DeviceLost)
        ));
        assert!(matches!(
            backend.fail_if_device_lost(),
            Err(WgpuRenderError::DeviceLost)
        ));
    }

    #[test]
    fn device_loss_invalidates_surfaces_without_releasing_window_authority() {
        let (mut backend, handles) = backend_with_lease_only_surface(WindowId::PRIMARY);
        backend.device_owner = WgpuDeviceOwner::with_loss_observation();

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
        assert_eq!(backend.fail_if_device_lost(), Ok(()));
        assert!(backend.device_owner.device_queue().is_none());
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
        let mut backend = WgpuRenderBackend::default();
        backend.ensure_instance().unwrap();

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
        let mut backend = WgpuRenderBackend::default();
        backend.ensure_instance().unwrap();
        assert!(backend.device_owner.instance().is_ok());
        assert!(backend.device_owner.ready().is_err());

        backend.mark_error(&WgpuRenderError::BackendNotReady);

        assert_eq!(backend.state(), WgpuBackendState::Unavailable);
        assert!(backend.device_owner.instance().is_err());
        assert!(backend.device_owner.ready().is_err());
        assert!(backend.device_owner.device_queue().is_none());
    }

    #[test]
    fn unavailable_backend_does_not_reenter_device_initialization() {
        let mut backend = WgpuRenderBackend {
            last_error: Some("device lost".to_owned()),
            ..WgpuRenderBackend::default()
        };
        backend.device_owner.mark_unavailable();

        assert_eq!(
            backend.ensure_device(WindowId::PRIMARY, None),
            Err(WgpuRenderError::BackendUnavailable)
        );
        assert_eq!(backend.state(), WgpuBackendState::Unavailable);
        assert_eq!(backend.last_error(), Some("device lost"));
        assert!(backend.device_owner.instance().is_err());
        assert!(backend.device_owner.ready().is_err());
        assert!(backend.device_owner.device_queue().is_none());
    }

    #[test]
    #[ignore = "requires an explicitly selected native GPU; acquisition failure is a failed prerequisite"]
    fn original_runtime_queue_retires_after_namespace_cleanup_without_telemetry() {
        for destroy_device in [false, true] {
            let mut backend = WgpuRenderBackend::default();
            let namespace = backend.prepare_image_device().unwrap();
            assert!(backend.runtime_retirement.is_some());
            assert!(!backend.frame_completion_stats().enabled());
            let (device, queue) = (&namespace.device, &namespace.queue);
            let buffer = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("original-runtime-retirement-work"),
                size: 4096,
                usage: wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            queue.write_buffer(&buffer, 0, &[1; 16]);
            let mut encoder =
                device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
            encoder.clear_buffer(&buffer, 16, None);
            queue.submit([encoder.finish()]);
            if destroy_device {
                device.destroy();
            }
            drop(namespace);
            backend.mark_error(&WgpuRenderError::DeviceLost);
            assert!(backend.device_owner.device_queue().is_none());
            assert!(backend.runtime_retirement.is_some());
            assert!(!backend.frame_completion_stats().enabled());

            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !backend.poll_runtime_retirement(&[]).unwrap() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "the original accepted prefix did not drain"
                );
                std::thread::sleep(Duration::from_millis(1));
            }
            assert!(backend.runtime_close_started());
            assert_eq!(backend.surface_count(), 0);
        }
    }
}
