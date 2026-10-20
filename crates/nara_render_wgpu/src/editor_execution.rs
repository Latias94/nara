use std::collections::BTreeMap;
#[cfg(feature = "editor-imgui")]
mod imgui;
#[cfg(feature = "editor-imgui")]
pub use imgui::{WgpuEditorImguiFrame, WgpuEditorImguiRenderer};
#[cfg(feature = "ui-submitter")]
use std::sync::Arc;

use nara_render::{
    Extent2d, RenderFramePacket, RenderFrameSourceGeneration, RenderFrameTarget, RenderImagePacket,
    RenderWindowPacket,
};
#[cfg(feature = "sprite-submitter")]
use nara_sprite_render::__private::SpriteBatches;
#[cfg(feature = "ui-submitter")]
use nara_ui_render::__private::{UiBackendPresentationFrame, UiCompletedBackendOutcome};
#[cfg(feature = "ui-submitter")]
use nara_ui_render::{
    UiPaintPacket, UiPresentationAttemptIdentity, UiPresentationBackendFaultDomain,
    UiPresentationBackendGeneration,
};
use nara_window::{
    WindowId,
    backend::{BackendWindowHandles, WindowHandleProvider, WindowTargetError},
};
use thiserror::Error;

#[cfg(feature = "ui-submitter")]
use crate::frame::{WgpuUiCompletion, WgpuUiSubmissionState, complete_ui_submissions};
use crate::{
    WgpuFrameTransactionStats, WgpuRenderBackend, WgpuRenderError,
    backend::{WgpuEditorAcquiredSurface, WgpuEditorDeviceNamespace},
    frame::WgpuFramePayload,
    presented::{
        WgpuEditorCompositeReceipt, WgpuEditorWindowPresentedReceipt, WgpuSubmittedImageReceipt,
    },
};

const EDITOR_IMAGE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[cfg(test)]
std::thread_local! {
    // Fail the selected texture/view allocation check without exhausting GPU memory.
    static VIEWPORT_ALLOCATION_FAILURE: std::cell::Cell<Option<u8>> = const {
        std::cell::Cell::new(None)
    };
}

fn finish_viewport_allocation(
    scope: wgpu::ErrorScopeGuard,
    extent: Extent2d,
) -> Result<(), WgpuEditorExecutionError> {
    let error = pollster::block_on(scope.pop()).map(|error| error.to_string());
    #[cfg(test)]
    let error = error.or_else(|| {
        VIEWPORT_ALLOCATION_FAILURE.with(|failure| match failure.get() {
            Some(1) => {
                failure.set(None);
                Some("injected viewport allocation failure".to_owned())
            }
            Some(remaining) => {
                failure.set(Some(remaining - 1));
                None
            }
            None => None,
        })
    });
    match error {
        Some(message) => Err(WgpuEditorExecutionError::ViewportTargetAllocationDeferred {
            width: extent.width,
            height: extent.height,
            message,
        }),
        None => Ok(()),
    }
}

/// Host-assigned, process-local identity of one non-reused Editor render view.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WgpuEditorViewId(u64);

impl WgpuEditorViewId {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Failure emitted by the private first-party Editor GPU execution owner.
#[doc(hidden)]
#[derive(Debug, Error)]
pub enum WgpuEditorExecutionError {
    #[error(transparent)]
    Backend(#[from] WgpuRenderError),
    #[error(transparent)]
    WindowTarget(#[from] WindowTargetError),
    #[error("the Editor viewport submission requires an image-target render packet")]
    ExpectedImageTarget,
    #[error("the Editor viewport target generation was exhausted")]
    TargetGenerationExhausted,
    #[error("the Editor viewport source-handoff generation was exhausted")]
    SourceHandoffGenerationExhausted,
    #[error("the Editor composite presentation generation was exhausted")]
    PresentationGenerationExhausted,
    #[error("the Editor viewport submission no longer belongs to the live target")]
    StaleImageSubmission,
    #[error("an Editor viewport target must be retired before another reservation is activated")]
    ViewportTargetAlreadyActive,
    #[error("an Editor viewport target must be admitted before image submission")]
    ViewportTargetUnavailable,
    #[error(
        "the Editor viewport target extent {width}x{height} exceeds the live GPU 2D texture limit {maximum}"
    )]
    ViewportTargetExtentUnsupported {
        width: u32,
        height: u32,
        maximum: u32,
    },
    #[error("the Editor viewport target allocation {width}x{height} was deferred: {message}")]
    ViewportTargetAllocationDeferred {
        width: u32,
        height: u32,
        message: String,
    },
    #[error("the Editor viewport reservation no longer matches the live source or GPU namespace")]
    StaleViewportReservation,
    #[error("an Editor Host render transaction must begin before image submission or presentation")]
    FrameTransactionUnavailable,
    #[error("the Editor toolkit paint callback failed: {message}")]
    ToolkitPaint { message: String },
}

impl WgpuEditorExecutionError {
    #[must_use]
    pub fn toolkit_paint(message: impl Into<String>) -> Self {
        Self::ToolkitPaint {
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn requires_owner_replacement(&self) -> bool {
        matches!(
            self,
            Self::Backend(
                WgpuRenderError::BackendUnavailable
                    | WgpuRenderError::DeviceLost
                    | WgpuRenderError::DeviceEpochExhausted
            )
        )
    }

    #[must_use]
    pub const fn is_deferred(&self) -> bool {
        matches!(self, Self::ViewportTargetAllocationDeferred { .. })
            || matches!(self, Self::Backend(error) if error.is_deferred())
    }

    #[must_use]
    pub const fn is_content_rejected(&self) -> bool {
        matches!(self, Self::Backend(error) if error.is_content_rejection())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WgpuEditorViewportReservationStamp {
    view_id: WgpuEditorViewId,
    target: RenderImagePacket,
    generation: u64,
    source_generation: RenderFrameSourceGeneration,
    source_handoff_generation: u64,
    backend_instance_id: u64,
    device_epoch: u64,
}

/// Copyable WGPU-owned identity for one admitted Editor viewport target.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuEditorViewportIdentity {
    stamp: WgpuEditorViewportReservationStamp,
}

impl WgpuEditorViewportIdentity {
    #[must_use]
    pub const fn view_id(self) -> WgpuEditorViewId {
        self.stamp.view_id
    }

    #[must_use]
    pub fn matches_submission(self, receipt: WgpuSubmittedImageReceipt) -> bool {
        self.stamp.view_id == receipt.view_id()
            && self.stamp.target == receipt.target()
            && self.stamp.generation == receipt.target_generation()
            && self.stamp.source_generation == receipt.source_generation()
            && self.stamp.source_handoff_generation == receipt.source_handoff_generation()
            && self.stamp.backend_instance_id == receipt.backend_instance_id()
            && self.stamp.device_epoch == receipt.device_epoch()
    }
}

struct WgpuEditorViewportTarget {
    stamp: WgpuEditorViewportReservationStamp,
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    last_submitted_frame_index: Option<u64>,
}

impl WgpuEditorViewportTarget {
    fn matches_namespace(&self, namespace: &WgpuEditorDeviceNamespace) -> bool {
        self.stamp.backend_instance_id == namespace.backend_instance_id
            && self.stamp.device_epoch == namespace.device_epoch
    }

    fn matches_receipt(&self, receipt: WgpuSubmittedImageReceipt) -> bool {
        self.last_submitted_frame_index == Some(receipt.render_transaction_frame_index())
            && WgpuEditorViewportIdentity { stamp: self.stamp }.matches_submission(receipt)
    }
}

/// Move-only inactive viewport target prepared for one coordinated Host publication.
#[doc(hidden)]
pub struct WgpuEditorViewportTargetReservation {
    stamp: WgpuEditorViewportReservationStamp,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
}

/// Fully validated target activation that can participate in one infallible Host commit.
#[doc(hidden)]
pub struct WgpuEditorViewportTargetActivation {
    reservation: WgpuEditorViewportTargetReservation,
}

impl std::fmt::Debug for WgpuEditorViewportTargetActivation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuEditorViewportTargetActivation")
            .field("stamp", &self.reservation.stamp)
            .finish_non_exhaustive()
    }
}

impl WgpuEditorViewportTargetActivation {
    #[must_use]
    pub const fn identity(&self) -> WgpuEditorViewportIdentity {
        WgpuEditorViewportIdentity {
            stamp: self.reservation.stamp,
        }
    }
}

impl std::fmt::Debug for WgpuEditorViewportTargetReservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuEditorViewportTargetReservation")
            .field("stamp", &self.stamp)
            .finish_non_exhaustive()
    }
}

impl WgpuEditorViewportTargetReservation {
    #[must_use]
    pub const fn identity(&self) -> WgpuEditorViewportIdentity {
        WgpuEditorViewportIdentity { stamp: self.stamp }
    }
}

/// Exact Runtime UI completion authority retained by the private Editor GPU transaction.
#[cfg(feature = "ui-submitter")]
#[doc(hidden)]
#[derive(Debug)]
pub struct WgpuEditorUiSubmission {
    state: WgpuUiSubmissionState,
}

#[cfg(feature = "ui-submitter")]
impl WgpuEditorUiSubmission {
    fn validate_backend_generation(
        &self,
        backend_generation: UiPresentationBackendGeneration,
    ) -> Result<(), WgpuEditorExecutionError> {
        self.state
            .validate_backend_generation(backend_generation)
            .map_err(Into::into)
    }

    fn bind_backend(
        &mut self,
        backend_generation: UiPresentationBackendGeneration,
    ) -> Result<(), WgpuEditorExecutionError> {
        self.state
            .bind_backend(backend_generation)
            .map_err(Into::into)
    }

    #[must_use]
    pub fn presented(self) -> Vec<UiCompletedBackendOutcome> {
        complete_ui_submissions(self.state, WgpuUiCompletion::Presented)
    }

    #[must_use]
    pub fn deferred(self, message: String) -> Vec<UiCompletedBackendOutcome> {
        complete_ui_submissions(self.state, WgpuUiCompletion::Deferred(message))
    }

    #[must_use]
    pub fn content_rejected(self, message: String) -> Vec<UiCompletedBackendOutcome> {
        complete_ui_submissions(self.state, WgpuUiCompletion::ContentRejected(message))
    }

    #[must_use]
    pub fn backend_fault(
        self,
        backend_generation: Option<UiPresentationBackendGeneration>,
        domain: UiPresentationBackendFaultDomain,
        message: String,
    ) -> Vec<UiCompletedBackendOutcome> {
        complete_ui_submissions(
            self.state,
            WgpuUiCompletion::BackendFault {
                backend_generation,
                domain,
                message,
            },
        )
    }

    #[must_use]
    pub fn superseded(
        self,
        successor: UiPresentationAttemptIdentity,
        message: String,
    ) -> Vec<UiCompletedBackendOutcome> {
        complete_ui_submissions(
            self.state,
            WgpuUiCompletion::Superseded { successor, message },
        )
    }

    fn failed(self, error: &WgpuEditorExecutionError) -> Vec<UiCompletedBackendOutcome> {
        if error.is_deferred() {
            return self.deferred(error.to_string());
        }
        if error.is_content_rejected() {
            return self.content_rejected(error.to_string());
        }
        let domain = match error {
            WgpuEditorExecutionError::Backend(error) => {
                crate::frame::classify_ui_backend_fault(error)
            }
            WgpuEditorExecutionError::WindowTarget(_)
            | WgpuEditorExecutionError::StaleImageSubmission
            | WgpuEditorExecutionError::ViewportTargetAlreadyActive
            | WgpuEditorExecutionError::ViewportTargetUnavailable
            | WgpuEditorExecutionError::ViewportTargetExtentUnsupported { .. }
            | WgpuEditorExecutionError::ViewportTargetAllocationDeferred { .. }
            | WgpuEditorExecutionError::StaleViewportReservation => {
                UiPresentationBackendFaultDomain::Target
            }
            WgpuEditorExecutionError::ExpectedImageTarget
            | WgpuEditorExecutionError::TargetGenerationExhausted
            | WgpuEditorExecutionError::SourceHandoffGenerationExhausted
            | WgpuEditorExecutionError::PresentationGenerationExhausted
            | WgpuEditorExecutionError::FrameTransactionUnavailable
            | WgpuEditorExecutionError::ToolkitPaint { .. } => {
                UiPresentationBackendFaultDomain::Attempt
            }
        };
        self.backend_fault(None, domain, error.to_string())
    }
}

/// One submitted viewport image retaining the Runtime UI completion authority until composite.
#[cfg(feature = "ui-submitter")]
#[doc(hidden)]
#[derive(Debug)]
pub struct WgpuEditorUiImageSubmission {
    image: WgpuSubmittedImageReceipt,
    ui: WgpuEditorUiSubmission,
}

#[cfg(feature = "ui-submitter")]
impl WgpuEditorUiImageSubmission {
    #[must_use]
    pub const fn image(&self) -> WgpuSubmittedImageReceipt {
        self.image
    }

    pub fn presented(
        self,
        composite: WgpuEditorCompositeReceipt,
    ) -> Result<Vec<UiCompletedBackendOutcome>, Self> {
        if composite.image() != self.image {
            return Err(self);
        }
        Ok(self.ui.presented())
    }

    #[must_use]
    pub fn deferred(self, message: String) -> Vec<UiCompletedBackendOutcome> {
        self.ui.deferred(message)
    }

    #[must_use]
    pub fn backend_fault(
        self,
        backend_generation: Option<UiPresentationBackendGeneration>,
        domain: UiPresentationBackendFaultDomain,
        message: String,
    ) -> Vec<UiCompletedBackendOutcome> {
        self.ui.backend_fault(backend_generation, domain, message)
    }

    #[must_use]
    pub fn superseded(
        self,
        successor: UiPresentationAttemptIdentity,
        message: String,
    ) -> Vec<UiCompletedBackendOutcome> {
        self.ui.superseded(successor, message)
    }

    #[must_use]
    pub fn failed(self, error: &WgpuEditorExecutionError) -> Vec<UiCompletedBackendOutcome> {
        self.ui.failed(error)
    }
}

/// Failed image submission retaining the exact Runtime UI authority for terminal completion.
#[cfg(feature = "ui-submitter")]
#[doc(hidden)]
#[derive(Debug)]
pub struct WgpuEditorUiSubmitFailure {
    error: WgpuEditorExecutionError,
    ui: WgpuEditorUiSubmission,
}

#[cfg(feature = "ui-submitter")]
impl WgpuEditorUiSubmitFailure {
    #[must_use]
    pub const fn error(&self) -> &WgpuEditorExecutionError {
        &self.error
    }

    #[must_use]
    pub fn into_parts(self) -> (WgpuEditorExecutionError, WgpuEditorUiSubmission) {
        (self.error, self.ui)
    }

    #[must_use]
    pub fn complete(self) -> (WgpuEditorExecutionError, Vec<UiCompletedBackendOutcome>) {
        let outcomes = self.ui.failed(&self.error);
        (self.error, outcomes)
    }
}

/// One scoped view of the device namespace used to initialize the private Editor toolkit renderer.
#[doc(hidden)]
pub struct WgpuEditorDeviceScope<'frame> {
    instance: &'frame wgpu::Instance,
    adapter: &'frame wgpu::Adapter,
    device: &'frame wgpu::Device,
    queue: &'frame wgpu::Queue,
    surface_format: wgpu::TextureFormat,
    surface_extent: Extent2d,
    backend_instance_id: u64,
    device_epoch: u64,
}

impl<'frame> WgpuEditorDeviceScope<'frame> {
    #[must_use]
    pub const fn instance(&self) -> &'frame wgpu::Instance {
        self.instance
    }

    #[must_use]
    pub const fn adapter(&self) -> &'frame wgpu::Adapter {
        self.adapter
    }

    #[must_use]
    pub const fn device(&self) -> &'frame wgpu::Device {
        self.device
    }

    #[must_use]
    pub const fn queue(&self) -> &'frame wgpu::Queue {
        self.queue
    }

    #[must_use]
    pub const fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_format
    }

    #[must_use]
    /// Returns the retained primary configuration extent, including while that surface is
    /// suspended. This is a device-setup fact; presentation uses the acquired texture extent.
    pub const fn surface_extent(&self) -> Extent2d {
        self.surface_extent
    }

    #[must_use]
    pub const fn backend_instance_id(&self) -> u64 {
        self.backend_instance_id
    }

    #[must_use]
    pub const fn device_epoch(&self) -> u64 {
        self.device_epoch
    }
}

/// Scoped inactive viewport texture facts available only while one reservation is live.
#[doc(hidden)]
#[derive(Clone, Copy)]
pub struct WgpuEditorReservedViewportTexture<'frame> {
    view: &'frame wgpu::TextureView,
}

impl<'frame> WgpuEditorReservedViewportTexture<'frame> {
    #[must_use]
    pub const fn view(self) -> &'frame wgpu::TextureView {
        self.view
    }
}

#[derive(Clone, Copy)]
struct WgpuEditorViewportSample {
    image: WgpuSubmittedImageReceipt,
}

fn join_composite_receipt(
    last_presentation_generation: &mut u64,
    submitted_viewport: Option<WgpuSubmittedImageReceipt>,
    sampled_viewport: Option<WgpuEditorViewportSample>,
    window: WgpuEditorWindowPresentedReceipt,
) -> Result<Option<WgpuEditorCompositeReceipt>, WgpuEditorExecutionError> {
    let sampled_viewport = sampled_viewport.filter(|sample| {
        submitted_viewport == Some(sample.image)
            && sample.image.backend_instance_id() == window.backend_instance_id()
            && sample.image.device_epoch() == window.device_epoch()
    });
    let Some(sample) = sampled_viewport else {
        return Ok(None);
    };
    let generation = last_presentation_generation
        .checked_add(1)
        .ok_or(WgpuEditorExecutionError::PresentationGenerationExhausted)?;
    *last_presentation_generation = generation;
    Ok(Some(WgpuEditorCompositeReceipt::new(
        sample.image,
        window,
        generation,
    )))
}

fn join_composite_receipts(
    last_presentation_generation: &mut u64,
    submitted: &[WgpuSubmittedImageReceipt],
    sampled: &[WgpuEditorViewportSample],
    window: WgpuEditorWindowPresentedReceipt,
) -> Result<Vec<WgpuEditorCompositeReceipt>, WgpuEditorExecutionError> {
    let mut composites = Vec::with_capacity(sampled.len());
    for sample in sampled {
        if composites
            .iter()
            .any(|composite: &WgpuEditorCompositeReceipt| composite.image() == sample.image)
        {
            continue;
        }
        let candidate = submitted
            .iter()
            .copied()
            .find(|image| *image == sample.image);
        if let Some(composite) = join_composite_receipt(
            last_presentation_generation,
            candidate,
            Some(*sample),
            window,
        )? {
            composites.push(composite);
        }
    }
    Ok(composites)
}

fn next_source_handoff_generation(
    last_generation: u64,
    current_source: Option<RenderFrameSourceGeneration>,
    next_source: RenderFrameSourceGeneration,
) -> Result<u64, WgpuEditorExecutionError> {
    if current_source == Some(next_source) {
        return Ok(last_generation);
    }
    last_generation
        .checked_add(1)
        .ok_or(WgpuEditorExecutionError::SourceHandoffGenerationExhausted)
}

fn next_viewport_reservation_stamp(
    view_id: WgpuEditorViewId,
    last_target_generation: u64,
    last_source_handoff_generation: u64,
    current_source: Option<RenderFrameSourceGeneration>,
    backend_instance_id: u64,
    device_epoch: u64,
    source_generation: RenderFrameSourceGeneration,
    target: RenderImagePacket,
) -> Result<WgpuEditorViewportReservationStamp, WgpuEditorExecutionError> {
    let generation = last_target_generation
        .checked_add(1)
        .ok_or(WgpuEditorExecutionError::TargetGenerationExhausted)?;
    let source_handoff_generation = next_source_handoff_generation(
        last_source_handoff_generation,
        current_source,
        source_generation,
    )?;
    Ok(WgpuEditorViewportReservationStamp {
        view_id,
        target,
        generation,
        source_generation,
        source_handoff_generation,
        backend_instance_id,
        device_epoch,
    })
}

/// Borrowed command-encoding scope for one Editor-window paint.
#[doc(hidden)]
pub struct WgpuEditorWindowFrame<'frame> {
    device: &'frame wgpu::Device,
    queue: &'frame wgpu::Queue,
    encoder: &'frame mut wgpu::CommandEncoder,
    target_view: &'frame wgpu::TextureView,
    surface_format: wgpu::TextureFormat,
    surface_extent: Extent2d,
    viewports: &'frame [WgpuSubmittedImageReceipt],
    #[cfg(feature = "editor-imgui")]
    viewport_textures: &'frame [(WgpuSubmittedImageReceipt, wgpu::TextureView)],
    leading_command_buffers: &'frame mut Vec<wgpu::CommandBuffer>,
    sampled_viewports: &'frame mut Vec<WgpuEditorViewportSample>,
}

impl<'frame> WgpuEditorWindowFrame<'frame> {
    #[must_use]
    pub const fn surface_format(&self) -> wgpu::TextureFormat {
        self.surface_format
    }

    #[must_use]
    pub const fn surface_extent(&self) -> Extent2d {
        self.surface_extent
    }

    pub fn encode<R>(
        &mut self,
        operation: impl for<'encode> FnOnce(
            &'encode wgpu::Device,
            &'encode wgpu::Queue,
            &'encode mut wgpu::CommandEncoder,
            &'encode wgpu::TextureView,
        ) -> R,
    ) -> R {
        operation(self.device, self.queue, self.encoder, self.target_view)
    }

    pub fn prepend_command_buffers(
        &mut self,
        command_buffers: impl IntoIterator<Item = wgpu::CommandBuffer>,
    ) {
        self.leading_command_buffers.extend(command_buffers);
    }

    pub fn record_viewport_sample(
        &mut self,
        receipt: WgpuSubmittedImageReceipt,
    ) -> Result<(), WgpuEditorExecutionError> {
        if !self.viewports.contains(&receipt) {
            return Err(WgpuEditorExecutionError::StaleImageSubmission);
        }
        self.sampled_viewports
            .push(WgpuEditorViewportSample { image: receipt });
        Ok(())
    }
}

/// Terminal facts from one attempted Editor-window paint.
#[doc(hidden)]
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WgpuEditorFrameOutcome {
    window: Option<WgpuEditorWindowPresentedReceipt>,
    composites: Vec<WgpuEditorCompositeReceipt>,
}

impl WgpuEditorFrameOutcome {
    #[must_use]
    pub const fn window(&self) -> Option<WgpuEditorWindowPresentedReceipt> {
        self.window
    }

    #[must_use]
    pub fn composites(&self) -> &[WgpuEditorCompositeReceipt] {
        &self.composites
    }
}

#[derive(Default)]
struct WgpuEditorViewAdmission {
    source_generation: Option<RenderFrameSourceGeneration>,
    last_frame_index: Option<u64>,
    last_target_generation: u64,
    last_source_handoff_generation: u64,
}

impl WgpuEditorViewAdmission {
    fn activate(&mut self, stamp: WgpuEditorViewportReservationStamp) {
        if self.source_generation != Some(stamp.source_generation) {
            self.last_frame_index = None;
        }
        self.source_generation = Some(stamp.source_generation);
        self.last_target_generation = stamp.generation;
        self.last_source_handoff_generation = stamp.source_handoff_generation;
    }

    fn admit(
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
        }
        if self
            .last_frame_index
            .is_some_and(|last| frame_index <= last)
        {
            return Err(WgpuRenderError::FrameAlreadyConsumed { frame_index });
        }
        self.last_frame_index = Some(frame_index);
        Ok(())
    }
}

#[derive(Default)]
struct WgpuEditorView {
    admission: WgpuEditorViewAdmission,
    target: Option<WgpuEditorViewportTarget>,
    world_canvas: crate::canvas::WorldCanvasCache,
    world_presentation: Option<nara_render::__private::WorldPresentationRevoker>,
}

impl Drop for WgpuEditorView {
    fn drop(&mut self) {
        if let Some(presentation) = self.world_presentation.take() {
            presentation.invalidate();
        }
    }
}

/// Private GPU owner used by the first-party Editor Host.
///
/// This type intentionally exposes no general render graph. Raw WGPU values are
/// borrowed only through scoped first-party callbacks.
#[doc(hidden)]
pub struct WgpuEditorExecution {
    #[cfg(feature = "editor-imgui")]
    primary_window_id: WindowId,
    backend: WgpuRenderBackend,
    handles: BackendWindowHandles,
    views: BTreeMap<WgpuEditorViewId, WgpuEditorView>,
    host_frame_index: Option<u64>,
    last_presentation_generation: u64,
}

impl std::fmt::Debug for WgpuEditorExecution {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuEditorExecution")
            .field("backend_instance_id", &self.backend.instance_id())
            .field("device_epoch", &self.backend.device_epoch())
            .field("view_count", &self.views.len())
            .finish_non_exhaustive()
    }
}

impl WgpuEditorExecution {
    pub fn new(
        window_id: WindowId,
        provider: WindowHandleProvider,
    ) -> Result<Self, WgpuEditorExecutionError> {
        let handles = BackendWindowHandles::default();
        handles.insert(window_id, provider)?;
        Ok(Self {
            #[cfg(feature = "editor-imgui")]
            primary_window_id: window_id,
            backend: WgpuRenderBackend::default(),
            handles,
            views: BTreeMap::new(),
            host_frame_index: None,
            last_presentation_generation: 0,
        })
    }

    #[cfg(feature = "ui-submitter")]
    #[must_use]
    pub fn reject_editor_ui_frame(
        ui_frame: UiBackendPresentationFrame,
        message: impl Into<String>,
    ) -> Vec<UiCompletedBackendOutcome> {
        let (_, submissions) = ui_frame.into_parts();
        WgpuEditorUiSubmission {
            state: WgpuUiSubmissionState::Prepared(submissions),
        }
        .content_rejected(message.into())
    }

    #[must_use]
    pub const fn backend_instance_id(&self) -> u64 {
        self.backend.instance_id()
    }

    #[must_use]
    pub const fn device_epoch(&self) -> u64 {
        self.backend.device_epoch()
    }

    #[must_use]
    pub fn frame_transaction_stats(&self) -> WgpuFrameTransactionStats {
        self.backend.frame_transaction_stats()
    }

    /// Begins the Host transaction once before submitting any of its independent view packets.
    pub fn begin_editor_frame(
        &mut self,
        window: RenderWindowPacket,
        host_frame_index: u64,
    ) -> Result<(), WgpuEditorExecutionError> {
        self.backend.prepare_editor_window(&self.handles, window)?;
        self.backend.begin_frame_transaction(host_frame_index, None);
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        self.backend.begin_editor_submitter_frame(host_frame_index);
        self.host_frame_index = Some(host_frame_index);
        Ok(())
    }

    pub fn with_device<R>(
        &mut self,
        window: RenderWindowPacket,
        operation: impl for<'frame> FnOnce(WgpuEditorDeviceScope<'frame>) -> R,
    ) -> Result<R, WgpuEditorExecutionError> {
        let namespace = self.backend.prepare_editor_window(&self.handles, window)?;
        Ok(operation(WgpuEditorDeviceScope {
            instance: &namespace.instance,
            adapter: &namespace.adapter,
            device: &namespace.device,
            queue: &namespace.queue,
            surface_format: namespace.surface_format,
            surface_extent: namespace.surface_extent,
            backend_instance_id: namespace.backend_instance_id,
            device_epoch: namespace.device_epoch,
        }))
    }

    pub fn reserve_viewport_target_with_texture<R>(
        &mut self,
        view_id: WgpuEditorViewId,
        window: RenderWindowPacket,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
        operation: impl for<'frame> FnOnce(
            WgpuEditorDeviceScope<'frame>,
            WgpuEditorReservedViewportTexture<'frame>,
        ) -> R,
    ) -> Result<(WgpuEditorViewportTargetReservation, R), WgpuEditorExecutionError> {
        let namespace = self.backend.prepare_editor_window(&self.handles, window)?;
        let reservation = self.reserve_viewport_target_in_namespace(
            view_id,
            &namespace,
            source_generation,
            target,
        )?;
        let result = operation(
            WgpuEditorDeviceScope {
                instance: &namespace.instance,
                adapter: &namespace.adapter,
                device: &namespace.device,
                queue: &namespace.queue,
                surface_format: namespace.surface_format,
                surface_extent: namespace.surface_extent,
                backend_instance_id: namespace.backend_instance_id,
                device_epoch: namespace.device_epoch,
            },
            WgpuEditorReservedViewportTexture {
                view: &reservation.view,
            },
        );
        Ok((reservation, result))
    }

    pub fn activate_viewport_target(
        &mut self,
        reservation: WgpuEditorViewportTargetReservation,
    ) -> Result<(), WgpuEditorExecutionError> {
        let activation = self.prepare_viewport_target_activation(reservation)?;
        self.commit_viewport_target_activation(activation);
        Ok(())
    }

    pub fn prepare_viewport_target_activation(
        &self,
        reservation: WgpuEditorViewportTargetReservation,
    ) -> Result<WgpuEditorViewportTargetActivation, WgpuEditorExecutionError> {
        self.validate_viewport_reservation(
            &reservation,
            self.backend.instance_id(),
            self.backend.device_epoch(),
        )?;
        Ok(WgpuEditorViewportTargetActivation { reservation })
    }

    pub fn commit_viewport_target_activation(
        &mut self,
        activation: WgpuEditorViewportTargetActivation,
    ) {
        let reservation = activation.reservation;
        debug_assert!(
            self.validate_viewport_reservation(
                &reservation,
                self.backend.instance_id(),
                self.backend.device_epoch(),
            )
            .is_ok()
        );
        let view = self.views.entry(reservation.stamp.view_id).or_default();
        if view.admission.source_generation != Some(reservation.stamp.source_generation) {
            if let Some(presentation) = view.world_presentation.take() {
                presentation.invalidate();
            }
            self.backend
                .retire_editor_world_presentation(reservation.stamp.view_id);
        }
        view.world_canvas.retain_namespace(
            reservation.stamp.source_generation,
            reservation.stamp.device_epoch,
        );
        view.admission.activate(reservation.stamp);
        view.target = Some(WgpuEditorViewportTarget {
            stamp: reservation.stamp,
            _texture: reservation.texture,
            view: reservation.view,
            last_submitted_frame_index: None,
        });
    }

    #[must_use]
    pub fn viewport_target_matches(
        &self,
        view_id: WgpuEditorViewId,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> bool {
        self.views
            .get(&view_id)
            .and_then(|view| view.target.as_ref())
            .is_some_and(|active| {
                active.stamp.source_generation == source_generation
                    && active.stamp.target == target
                    && active.stamp.backend_instance_id == self.backend.instance_id()
                    && active.stamp.device_epoch == self.backend.device_epoch()
            })
    }

    pub fn submit_image(
        &mut self,
        view_id: WgpuEditorViewId,
        window: RenderWindowPacket,
        packet: RenderFramePacket,
    ) -> Result<WgpuSubmittedImageReceipt, WgpuEditorExecutionError> {
        self.submit_image_payload(view_id, window, packet, WgpuFramePayload::default(), None)
    }

    /// Attaches invalidate-only Runtime camera state to an existing Editor view's GPU lifetime.
    /// This grants no image submission, composite or presentation authority.
    pub fn retain_game_world_presentation(
        &mut self,
        view_id: WgpuEditorViewId,
        window: RenderWindowPacket,
        revoker: nara_render::__private::WorldPresentationRevoker,
    ) -> Result<(), WgpuEditorExecutionError> {
        if self
            .views
            .get(&view_id)
            .is_none_or(|view| view.target.is_none())
        {
            revoker.invalidate();
            return Err(WgpuEditorExecutionError::ViewportTargetUnavailable);
        }
        if let Err(error) = self.backend.prepare_editor_window(&self.handles, window) {
            revoker.invalidate();
            return Err(error.into());
        }
        if let Err(error) = self
            .backend
            .retain_editor_world_presentation(view_id, revoker.clone())
        {
            revoker.invalidate();
            return Err(error.into());
        }
        let view = self
            .views
            .get_mut(&view_id)
            .expect("the registered view remains owned");
        if let Some(previous) = view.world_presentation.replace(revoker) {
            previous.invalidate();
        }
        Ok(())
    }

    #[cfg(feature = "sprite-submitter")]
    pub fn submit_sprite_image(
        &mut self,
        view_id: WgpuEditorViewId,
        window: RenderWindowPacket,
        packet: RenderFramePacket,
        sprite_batches: SpriteBatches,
    ) -> Result<WgpuSubmittedImageReceipt, WgpuEditorExecutionError> {
        self.submit_image_payload(
            view_id,
            window,
            packet,
            WgpuFramePayload::for_editor_sprites(sprite_batches),
            None,
        )
    }

    #[cfg(feature = "ui-submitter")]
    pub fn submit_ui_image(
        &mut self,
        view_id: WgpuEditorViewId,
        window: RenderWindowPacket,
        packet: RenderFramePacket,
        ui_frame: UiBackendPresentationFrame,
    ) -> Result<WgpuEditorUiImageSubmission, WgpuEditorUiSubmitFailure> {
        let (ui_packets, submissions) = ui_frame.into_parts();
        let mut ui = WgpuEditorUiSubmission {
            state: WgpuUiSubmissionState::Prepared(submissions),
        };
        let payload = WgpuFramePayload::for_editor_runtime_ui(ui_packets);
        match self.submit_image_payload(view_id, window, packet, payload, Some(&mut ui)) {
            Ok(image) => Ok(WgpuEditorUiImageSubmission { image, ui }),
            Err(error) => Err(WgpuEditorUiSubmitFailure { error, ui }),
        }
    }

    /// Submits an authoring-only UI packet without entering the Runtime UI outcome protocol.
    #[cfg(feature = "ui-submitter")]
    pub fn submit_authoring_ui_image(
        &mut self,
        view_id: WgpuEditorViewId,
        window: RenderWindowPacket,
        packet: RenderFramePacket,
        ui_packet: Arc<UiPaintPacket>,
    ) -> Result<WgpuSubmittedImageReceipt, WgpuEditorExecutionError> {
        self.submit_image_payload(
            view_id,
            window,
            packet,
            WgpuFramePayload::for_editor_authoring_ui(vec![ui_packet]),
            None,
        )
    }

    #[cfg(all(feature = "sprite-submitter", feature = "ui-submitter"))]
    pub fn submit_sprite_ui_image(
        &mut self,
        view_id: WgpuEditorViewId,
        window: RenderWindowPacket,
        packet: RenderFramePacket,
        sprite_batches: SpriteBatches,
        ui_frame: UiBackendPresentationFrame,
    ) -> Result<WgpuEditorUiImageSubmission, WgpuEditorUiSubmitFailure> {
        let (ui_packets, submissions) = ui_frame.into_parts();
        let mut ui = WgpuEditorUiSubmission {
            state: WgpuUiSubmissionState::Prepared(submissions),
        };
        let payload = WgpuFramePayload::for_editor_viewport(sprite_batches, ui_packets);
        match self.submit_image_payload(view_id, window, packet, payload, Some(&mut ui)) {
            Ok(image) => Ok(WgpuEditorUiImageSubmission { image, ui }),
            Err(error) => Err(WgpuEditorUiSubmitFailure { error, ui }),
        }
    }

    fn submit_image_payload(
        &mut self,
        view_id: WgpuEditorViewId,
        window: RenderWindowPacket,
        packet: RenderFramePacket,
        payload: WgpuFramePayload,
        #[cfg(feature = "ui-submitter")] mut ui: Option<&mut WgpuEditorUiSubmission>,
        #[cfg(not(feature = "ui-submitter"))] _ui: Option<&mut ()>,
    ) -> Result<WgpuSubmittedImageReceipt, WgpuEditorExecutionError> {
        let RenderFrameTarget::Image(target) = packet.target() else {
            return Err(WgpuEditorExecutionError::ExpectedImageTarget);
        };
        let host_frame_index = self
            .host_frame_index
            .ok_or(WgpuEditorExecutionError::FrameTransactionUnavailable)?;
        let namespace = self.backend.prepare_editor_window(&self.handles, window)?;
        self.require_viewport_target(view_id, packet.source_generation(), target)?;
        if let Err(error) = self
            .views
            .get_mut(&view_id)
            .expect("a required target belongs to a registered view")
            .admission
            .admit(packet.source_generation(), packet.frame_index())
        {
            self.backend.reject_editor_packet();
            return Err(error.into());
        }
        #[cfg(feature = "ui-submitter")]
        let ui_backend_generation = if let Some(ui) = ui.as_deref_mut() {
            let generation = UiPresentationBackendGeneration::new(
                namespace.backend_instance_id,
                namespace.device_epoch,
            )
            .ok_or_else(|| {
                WgpuEditorExecutionError::Backend(WgpuRenderError::DeviceUnavailable {
                    message: "the Editor GPU namespace has no non-zero UI presentation generation"
                        .to_owned(),
                })
            })?;
            ui.validate_backend_generation(generation)?;
            Some(generation)
        } else {
            None
        };
        #[cfg(feature = "ui-submitter")]
        if let (Some(ui), Some(backend_generation)) = (ui.as_deref_mut(), ui_backend_generation) {
            ui.bind_backend(backend_generation)?;
        }
        let canvas_cache = &mut self
            .views
            .get_mut(&view_id)
            .expect("a required target belongs to a registered view")
            .world_canvas;
        let mut draw = self.backend.prepare_target_draw_with_canvas_cache(
            EDITOR_IMAGE_FORMAT,
            &packet,
            &payload,
            host_frame_index,
            canvas_cache,
        )?;
        let viewport_target = self
            .views
            .get_mut(&view_id)
            .and_then(|view| view.target.as_mut())
            .expect("ensuring an Editor viewport target installs it");
        let mut encoder =
            namespace
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("nara_editor_viewport_encoder"),
                });
        crate::encode_target_draw(&mut encoder, &viewport_target.view, &draw);
        namespace.queue.submit([encoder.finish()]);
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        draw.commit_instance_reservations();
        draw.retain_images_until_submission_complete(&namespace.queue);
        viewport_target.last_submitted_frame_index = Some(packet.frame_index());
        self.backend.record_editor_image_submission();
        Ok(WgpuSubmittedImageReceipt::new(
            view_id,
            target,
            viewport_target.stamp.generation,
            viewport_target.stamp.source_generation,
            viewport_target.stamp.source_handoff_generation,
            namespace.backend_instance_id,
            namespace.device_epoch,
            packet.frame_index(),
        ))
    }

    pub fn present_window(
        &mut self,
        window: RenderWindowPacket,
        editor_frame_index: u64,
        viewport_submissions: &[WgpuSubmittedImageReceipt],
        paint: impl for<'frame> FnOnce(
            &mut WgpuEditorWindowFrame<'frame>,
        ) -> Result<(), WgpuEditorExecutionError>,
        pre_present: impl FnOnce(),
    ) -> Result<WgpuEditorFrameOutcome, WgpuEditorExecutionError> {
        self.host_frame_index
            .ok_or(WgpuEditorExecutionError::FrameTransactionUnavailable)?;
        let namespace = self.backend.prepare_editor_window(&self.handles, window)?;
        for &submission in viewport_submissions {
            self.validate_viewport_submission(&namespace, submission)?;
        }
        if window.resolution.physical_width == 0 || window.resolution.physical_height == 0 {
            return Ok(WgpuEditorFrameOutcome::default());
        }
        let Some(acquired) = self.backend.acquire_editor_surface(window.id)? else {
            return Ok(WgpuEditorFrameOutcome::default());
        };
        let WgpuEditorAcquiredSurface {
            texture,
            surface_generation,
            format,
            extent,
        } = acquired;
        let target_view = texture.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(format),
            ..Default::default()
        });
        let mut encoder =
            namespace
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("nara_editor_window_encoder"),
                });
        let mut leading_command_buffers = Vec::new();
        let mut sampled_viewports = Vec::new();
        #[cfg(feature = "editor-imgui")]
        let viewport_textures = viewport_submissions
            .iter()
            .copied()
            .filter_map(|image| {
                self.views
                    .get(&image.view_id())
                    .and_then(|view| view.target.as_ref())
                    .map(|target| (image, target.view.clone()))
            })
            .collect::<Vec<_>>();
        paint(&mut WgpuEditorWindowFrame {
            device: &namespace.device,
            queue: &namespace.queue,
            encoder: &mut encoder,
            target_view: &target_view,
            surface_format: format,
            surface_extent: extent,
            viewports: viewport_submissions,
            #[cfg(feature = "editor-imgui")]
            viewport_textures: &viewport_textures,
            leading_command_buffers: &mut leading_command_buffers,
            sampled_viewports: &mut sampled_viewports,
        })?;
        namespace.queue.submit(
            leading_command_buffers
                .into_iter()
                .chain([encoder.finish()]),
        );
        pre_present();
        namespace.queue.present(texture);
        self.backend.record_editor_window_presentation();
        let window_receipt = WgpuEditorWindowPresentedReceipt::new(
            window.id,
            surface_generation,
            namespace.backend_instance_id,
            namespace.device_epoch,
            editor_frame_index,
        );
        let composites = join_composite_receipts(
            &mut self.last_presentation_generation,
            viewport_submissions,
            &sampled_viewports,
            window_receipt,
        )?;
        Ok(WgpuEditorFrameOutcome {
            window: Some(window_receipt),
            composites,
        })
    }

    pub fn retire_viewport_target(&mut self, view_id: WgpuEditorViewId) {
        if let Some(view) = self.views.get_mut(&view_id) {
            view.target = None;
            if let Some(presentation) = view.world_presentation.take() {
                presentation.invalidate();
            }
        }
        self.backend.retire_editor_world_presentation(view_id);
    }

    /// Flushes pending uploads and drains work at the explicit native Host shutdown boundary.
    pub fn wait_for_idle(&self) -> Result<(), WgpuEditorExecutionError> {
        self.backend
            .wait_for_editor_idle(std::time::Duration::from_secs(5))?;
        Ok(())
    }

    /// Releases a final view lifetime. The Host never reuses its identity after this call.
    pub fn retire_view(&mut self, view_id: WgpuEditorViewId) {
        self.views.remove(&view_id);
        self.backend.retire_editor_world_presentation(view_id);
    }

    fn validate_viewport_submission(
        &self,
        namespace: &WgpuEditorDeviceNamespace,
        submission: WgpuSubmittedImageReceipt,
    ) -> Result<(), WgpuEditorExecutionError> {
        if !self
            .views
            .get(&submission.view_id())
            .and_then(|view| view.target.as_ref())
            .is_some_and(|target| {
                target.matches_namespace(namespace) && target.matches_receipt(submission)
            })
        {
            return Err(WgpuEditorExecutionError::StaleImageSubmission);
        }
        Ok(())
    }

    fn require_viewport_target(
        &self,
        view_id: WgpuEditorViewId,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> Result<(), WgpuEditorExecutionError> {
        if self.viewport_target_matches(view_id, source_generation, target) {
            return Ok(());
        }
        if self
            .views
            .get(&view_id)
            .is_some_and(|view| view.target.is_some())
        {
            return Err(WgpuEditorExecutionError::ViewportTargetAlreadyActive);
        }
        Err(WgpuEditorExecutionError::ViewportTargetUnavailable)
    }

    fn reserve_viewport_target_in_namespace(
        &self,
        view_id: WgpuEditorViewId,
        namespace: &WgpuEditorDeviceNamespace,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> Result<WgpuEditorViewportTargetReservation, WgpuEditorExecutionError> {
        let stamp =
            self.next_viewport_reservation_stamp(view_id, namespace, source_generation, target)?;
        let extent = target.extent();
        let maximum = namespace.device.limits().max_texture_dimension_2d;
        if extent.width > maximum || extent.height > maximum {
            return Err(WgpuEditorExecutionError::ViewportTargetExtentUnsupported {
                width: extent.width,
                height: extent.height,
                maximum,
            });
        }
        let allocation_scope = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let texture = namespace.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("nara_editor_viewport_target"),
            size: wgpu::Extent3d {
                width: extent.width,
                height: extent.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: EDITOR_IMAGE_FORMAT,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[EDITOR_IMAGE_FORMAT],
        });
        // A failed texture is an invalid WGPU handle. Reject it before attempting a view,
        // otherwise the expected allocation failure also produces a validation failure.
        finish_viewport_allocation(allocation_scope, extent)?;
        let allocation_scope = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(EDITOR_IMAGE_FORMAT),
            ..Default::default()
        });
        finish_viewport_allocation(allocation_scope, extent)?;
        Ok(WgpuEditorViewportTargetReservation {
            stamp,
            texture,
            view,
        })
    }

    fn next_viewport_reservation_stamp(
        &self,
        view_id: WgpuEditorViewId,
        namespace: &WgpuEditorDeviceNamespace,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> Result<WgpuEditorViewportReservationStamp, WgpuEditorExecutionError> {
        self.next_viewport_reservation_stamp_for_namespace(
            view_id,
            namespace.backend_instance_id,
            namespace.device_epoch,
            source_generation,
            target,
        )
    }

    fn validate_viewport_reservation(
        &self,
        reservation: &WgpuEditorViewportTargetReservation,
        backend_instance_id: u64,
        device_epoch: u64,
    ) -> Result<(), WgpuEditorExecutionError> {
        let expected = self.next_viewport_reservation_stamp_for_namespace(
            reservation.stamp.view_id,
            backend_instance_id,
            device_epoch,
            reservation.stamp.source_generation,
            reservation.stamp.target,
        )?;
        if reservation.stamp != expected {
            return Err(WgpuEditorExecutionError::StaleViewportReservation);
        }
        Ok(())
    }

    fn next_viewport_reservation_stamp_for_namespace(
        &self,
        view_id: WgpuEditorViewId,
        backend_instance_id: u64,
        device_epoch: u64,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> Result<WgpuEditorViewportReservationStamp, WgpuEditorExecutionError> {
        let admission = self.views.get(&view_id).map(|view| &view.admission);
        next_viewport_reservation_stamp(
            view_id,
            admission.map_or(0, |view| view.last_target_generation),
            admission.map_or(0, |view| view.last_source_handoff_generation),
            admission.and_then(|view| view.source_generation),
            backend_instance_id,
            device_epoch,
            source_generation,
            target,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nara_ecs::Entity;
    use nara_render::{
        __private::{EditorRenderGeneration, build_editor_render_frame_packet},
        Color, ExtractedView, ExtractedViews, RenderImage2d, RenderTarget, ViewportRect,
    };
    #[cfg(feature = "ui-submitter")]
    use nara_ui_render::__private::UiBackendSubmission;
    #[cfg(feature = "ui-submitter")]
    use nara_ui_render::{
        UiPaintPacket, UiPresentationAttemptId, UiPresentationCapabilityGeneration,
        UiPresentationEpoch, UiPresentationOutcome, UiPresentationRuntimeGeneration,
    };

    #[test]
    fn viewport_allocation_failure_preserves_publication_and_smaller_retry() {
        let instance =
            wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let force_fallback_adapter = match std::env::var("NARA_WGPU_FORCE_FALLBACK").as_deref() {
            Ok("1") => true,
            Ok("0") | Err(std::env::VarError::NotPresent) => false,
            value => panic!("invalid NARA_WGPU_FORCE_FALLBACK: {value:?}"),
        };
        let adapter =
            match pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter,
                ..Default::default()
            })) {
                Ok(adapter) => adapter,
                Err(error)
                    if std::env::var("DEAR_IMGUI_REQUIRE_WGPU_ADAPTER").as_deref() != Ok("1") =>
                {
                    eprintln!("skipping viewport allocation test without a WGPU adapter: {error}");
                    return;
                }
                Err(error) => panic!("the required WGPU adapter is unavailable: {error}"),
            };
        let (device, queue) =
            pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default())).unwrap();
        let mut execution = test_execution();
        let namespace = WgpuEditorDeviceNamespace {
            instance,
            adapter,
            device,
            queue,
            surface_format: EDITOR_IMAGE_FORMAT,
            surface_extent: Extent2d::new(64, 64).unwrap(),
            backend_instance_id: execution.backend.instance_id(),
            device_epoch: execution.backend.device_epoch(),
        };
        let frame = test_image_frame();
        let target = image_target(&frame);
        let source = frame.source_generation();
        let view_id = WgpuEditorViewId::new(1);
        let initial = execution
            .reserve_viewport_target_in_namespace(view_id, &namespace, source, target)
            .unwrap();
        let initial_stamp = initial.stamp;
        execution.activate_viewport_target(initial).unwrap();
        execution
            .views
            .get_mut(&view_id)
            .unwrap()
            .target
            .as_mut()
            .unwrap()
            .last_submitted_frame_index = Some(7);
        let receipt = WgpuSubmittedImageReceipt::new(
            view_id,
            target,
            initial_stamp.generation,
            source,
            initial_stamp.source_handoff_generation,
            namespace.backend_instance_id,
            namespace.device_epoch,
            7,
        );
        let larger = nara_render::__private::editor_viewport_image_target(
            target.image(),
            Extent2d::new(128, 128).unwrap(),
        )
        .unwrap();
        // Check both texture allocation and view allocation failures. Neither candidate may
        // consume a generation, replace the live target or create a new registered view.
        for failure_stage in [1, 2] {
            VIEWPORT_ALLOCATION_FAILURE.with(|failure| failure.set(Some(failure_stage)));
            let error = execution
                .reserve_viewport_target_in_namespace(view_id, &namespace, source, larger)
                .unwrap_err();
            assert!(error.is_deferred());
            assert!(!error.is_content_rejected());
            assert!(!error.requires_owner_replacement());
            assert_eq!(execution.views.len(), 1);
            assert_eq!(
                execution.views[&view_id].target.as_ref().unwrap().stamp,
                initial_stamp
            );
            assert_eq!(
                execution.views[&view_id].admission.last_target_generation,
                1
            );
            execution
                .validate_viewport_submission(&namespace, receipt)
                .unwrap();
        }
        let unregistered = WgpuEditorViewId::new(2);
        VIEWPORT_ALLOCATION_FAILURE.with(|failure| failure.set(Some(1)));
        assert!(
            execution
                .reserve_viewport_target_in_namespace(unregistered, &namespace, source, larger,)
                .is_err()
        );
        assert!(!execution.views.contains_key(&unregistered));

        let smaller = nara_render::__private::editor_viewport_image_target(
            target.image(),
            Extent2d::new(32, 32).unwrap(),
        )
        .unwrap();
        let retry = execution
            .reserve_viewport_target_in_namespace(view_id, &namespace, source, smaller)
            .unwrap();
        assert_eq!(retry.stamp.generation, initial_stamp.generation + 1);
        execution
            .validate_viewport_submission(&namespace, receipt)
            .unwrap();
        execution.retire_viewport_target(view_id);
        execution.activate_viewport_target(retry).unwrap();
        assert_eq!(
            execution.views[&view_id]
                .target
                .as_ref()
                .unwrap()
                .stamp
                .target,
            smaller
        );
        assert!(
            execution
                .validate_viewport_submission(&namespace, receipt)
                .is_err()
        );
        execution.retire_view(view_id);
        assert!(execution.views.is_empty());
    }

    #[test]
    fn editor_independent_producers_keep_their_own_admission_clock() {
        let scene = test_image_frame().source_generation();
        let game = test_image_frame().source_generation();
        let mut execution = test_execution();
        let scene_view = WgpuEditorViewId::new(1);
        let game_view = WgpuEditorViewId::new(2);
        let target = image_target(&test_image_frame());
        activate_test_view(&mut execution, scene_view, scene, target);
        execution
            .views
            .get_mut(&scene_view)
            .unwrap()
            .admission
            .admit(scene, 1)
            .unwrap();
        activate_test_view(&mut execution, game_view, game, target);
        execution
            .views
            .get_mut(&game_view)
            .unwrap()
            .admission
            .admit(game, 1)
            .unwrap();

        assert!(matches!(
            execution
                .views
                .get_mut(&game_view)
                .unwrap()
                .admission
                .admit(game, 1),
            Err(WgpuRenderError::FrameAlreadyConsumed { frame_index: 1 })
        ));
        execution
            .views
            .get_mut(&scene_view)
            .unwrap()
            .admission
            .admit(scene, 2)
            .expect("a Game submission must not invalidate the independent Scene producer");
        assert!(matches!(
            execution
                .views
                .get_mut(&scene_view)
                .unwrap()
                .admission
                .admit(scene, 2),
            Err(WgpuRenderError::FrameAlreadyConsumed { frame_index: 2 })
        ));
        execution
            .views
            .get_mut(&game_view)
            .unwrap()
            .admission
            .admit(game, 2)
            .unwrap();
    }

    #[test]
    fn editor_host_transaction_accumulates_all_image_submissions() {
        let mut backend = WgpuRenderBackend::default();
        backend.begin_frame_transaction(23, None);
        backend.record_editor_image_submission();
        backend.record_editor_image_submission();

        let facts = backend.frame_transaction_stats();
        assert_eq!(facts.render_transaction_frame_index(), Some(23));
        assert_eq!(facts.packet_admissions(), 2);
        assert_eq!(facts.queue_submissions(), 2);
    }

    #[test]
    fn independent_reservations_survive_another_view_activation_and_retirement() {
        let mut execution = test_execution();
        let scene = test_image_frame();
        let game = test_image_frame();
        let scene_view = WgpuEditorViewId::new(1);
        let game_view = WgpuEditorViewId::new(2);
        let namespace = (
            execution.backend.instance_id(),
            execution.backend.device_epoch(),
        );
        let reserve = |execution: &WgpuEditorExecution, view_id, packet: &RenderFramePacket| {
            execution
                .next_viewport_reservation_stamp_for_namespace(
                    view_id,
                    namespace.0,
                    namespace.1,
                    packet.source_generation(),
                    image_target(packet),
                )
                .unwrap()
        };
        let scene_reservation = reserve(&execution, scene_view, &scene);
        let game_reservation = reserve(&execution, game_view, &game);
        execution
            .views
            .entry(scene_view)
            .or_default()
            .admission
            .activate(scene_reservation);

        assert_eq!(reserve(&execution, game_view, &game), game_reservation);
        execution
            .views
            .entry(game_view)
            .or_default()
            .admission
            .activate(game_reservation);
        execution
            .views
            .get_mut(&game_view)
            .unwrap()
            .admission
            .admit(game.source_generation(), 1)
            .unwrap();
        let next_game = reserve(&execution, game_view, &game);
        execution.retire_viewport_target(scene_view);
        assert_eq!(reserve(&execution, game_view, &game), next_game);
        assert_eq!(
            execution.views[&game_view].admission.last_frame_index,
            Some(1)
        );
        assert_eq!(
            next_game.source_handoff_generation,
            game_reservation.source_handoff_generation
        );
        assert_eq!(next_game.generation, game_reservation.generation + 1);
    }

    #[test]
    fn closing_non_reused_views_releases_their_admission_metadata() {
        let mut execution = test_execution();
        let frame = test_image_frame();
        let target = image_target(&frame);
        let active = WgpuEditorViewId::new(1);
        activate_test_view(&mut execution, active, frame.source_generation(), target);
        for id in 2..1026 {
            let closed = WgpuEditorViewId::new(id);
            activate_test_view(&mut execution, closed, frame.source_generation(), target);
            execution.retire_view(closed);
        }
        assert_eq!(execution.views.len(), 1);
        assert!(execution.views.contains_key(&active));
    }

    #[test]
    fn resizing_one_view_preserves_both_producer_admission_and_other_view_identity() {
        let mut execution = test_execution();
        let scene = test_image_frame();
        let game = test_image_frame();
        let scene_view = WgpuEditorViewId::new(1);
        let game_view = WgpuEditorViewId::new(2);
        let first = activate_test_view(
            &mut execution,
            scene_view,
            scene.source_generation(),
            image_target(&scene),
        );
        let game_stamp = activate_test_view(
            &mut execution,
            game_view,
            game.source_generation(),
            image_target(&game),
        );
        execution
            .views
            .get_mut(&scene_view)
            .unwrap()
            .admission
            .admit(scene.source_generation(), 1)
            .unwrap();
        let resized_target = nara_render::__private::editor_viewport_image_target(
            image_target(&scene).image(),
            Extent2d::new(128, 96).unwrap(),
        )
        .unwrap();
        let resized = activate_test_view(
            &mut execution,
            scene_view,
            scene.source_generation(),
            resized_target,
        );

        assert_eq!(resized.generation, first.generation + 1);
        assert_eq!(
            resized.source_handoff_generation,
            first.source_handoff_generation
        );
        assert!(matches!(
            execution
                .views
                .get_mut(&scene_view)
                .unwrap()
                .admission
                .admit(scene.source_generation(), 1),
            Err(WgpuRenderError::FrameAlreadyConsumed { frame_index: 1 })
        ));
        let game_after_resize = &execution.views[&game_view].admission;
        assert_eq!(
            game_after_resize.last_target_generation,
            game_stamp.generation
        );
        assert_eq!(
            game_after_resize.source_generation,
            Some(game.source_generation())
        );
        assert_eq!(game_after_resize.last_frame_index, None);
    }

    #[test]
    fn multiple_composites_certify_only_exact_sampled_images_in_one_window() {
        let frame = test_image_frame();
        let target = image_target(&frame);
        let first = WgpuSubmittedImageReceipt::new(
            WgpuEditorViewId::new(1),
            target,
            1,
            frame.source_generation(),
            1,
            21,
            4,
            1,
        );
        let second = WgpuSubmittedImageReceipt::new(
            WgpuEditorViewId::new(2),
            target,
            1,
            frame.source_generation(),
            1,
            21,
            4,
            1,
        );
        let unsampled = WgpuSubmittedImageReceipt::new(
            WgpuEditorViewId::new(3),
            target,
            1,
            frame.source_generation(),
            1,
            21,
            4,
            1,
        );
        let foreign = WgpuSubmittedImageReceipt::new(
            WgpuEditorViewId::new(4),
            target,
            1,
            frame.source_generation(),
            1,
            21,
            4,
            1,
        );
        let window = WgpuEditorWindowPresentedReceipt::new(WindowId::PRIMARY, 3, 21, 4, 19);
        let mut generation = 0;
        let composites = join_composite_receipts(
            &mut generation,
            &[first, second, unsampled],
            &[
                WgpuEditorViewportSample { image: second },
                WgpuEditorViewportSample { image: first },
                WgpuEditorViewportSample { image: foreign },
                WgpuEditorViewportSample { image: first },
            ],
            window,
        )
        .unwrap();

        assert_eq!(composites.len(), 2);
        assert_eq!(composites[0].image(), second);
        assert_eq!(composites[1].image(), first);
        assert!(
            composites
                .iter()
                .all(|composite| composite.window() == window)
        );
        assert_eq!(composites[0].presentation_generation(), 1);
        assert_eq!(composites[1].presentation_generation(), 2);
        assert_eq!(generation, 2);
    }

    #[test]
    fn viewport_identity_does_not_authenticate_another_view_with_equal_generations() {
        let frame = test_image_frame();
        let target = image_target(&frame);
        let stamp = next_viewport_reservation_stamp(
            WgpuEditorViewId::new(1),
            0,
            0,
            None,
            21,
            4,
            frame.source_generation(),
            target,
        )
        .unwrap();
        let receipt = WgpuSubmittedImageReceipt::new(
            WgpuEditorViewId::new(2),
            target,
            stamp.generation,
            frame.source_generation(),
            stamp.source_handoff_generation,
            21,
            4,
            1,
        );

        assert!(!WgpuEditorViewportIdentity { stamp }.matches_submission(receipt));
    }

    #[test]
    fn composite_receipt_requires_the_exact_submitted_sample_and_window_namespace() {
        let frame = test_image_frame();
        let image = image_target(&frame);
        let submitted = WgpuSubmittedImageReceipt::new(
            WgpuEditorViewId::new(1),
            image,
            4,
            frame.source_generation(),
            6,
            10,
            2,
            7,
        );
        let window = WgpuEditorWindowPresentedReceipt::new(
            WindowId::PRIMARY,
            3,
            submitted.backend_instance_id(),
            submitted.device_epoch(),
            9,
        );
        let mut presentation_generation = 0;

        assert_eq!(
            join_composite_receipt(&mut presentation_generation, Some(submitted), None, window,)
                .unwrap(),
            None,
        );
        assert_eq!(presentation_generation, 0);

        let foreign_window = WgpuEditorWindowPresentedReceipt::new(
            WindowId::PRIMARY,
            3,
            submitted.backend_instance_id().saturating_add(1),
            submitted.device_epoch(),
            9,
        );
        let sample = WgpuEditorViewportSample { image: submitted };
        assert_eq!(
            join_composite_receipt(
                &mut presentation_generation,
                Some(submitted),
                Some(sample),
                foreign_window,
            )
            .unwrap(),
            None,
        );
        assert_eq!(presentation_generation, 0);

        let composite = join_composite_receipt(
            &mut presentation_generation,
            Some(submitted),
            Some(sample),
            window,
        )
        .unwrap()
        .unwrap();
        assert_eq!(composite.presentation_generation(), 1);
        assert_eq!(
            composite.image().source_generation(),
            frame.source_generation()
        );
        assert_eq!(composite.image().source_handoff_generation(), 6);
        assert_eq!(presentation_generation, 1);
    }

    #[test]
    fn viewport_source_handoff_advances_only_when_the_producer_changes() {
        let scene = test_image_frame().source_generation();
        let successor = test_image_frame().source_generation();

        assert_eq!(next_source_handoff_generation(0, None, scene).unwrap(), 1);
        assert_eq!(
            next_source_handoff_generation(1, Some(scene), scene).unwrap(),
            1
        );
        assert_eq!(
            next_source_handoff_generation(1, Some(scene), successor).unwrap(),
            2
        );
    }

    #[test]
    fn stale_inactive_viewport_reservation_cannot_replace_a_committed_target() {
        let first_frame = test_image_frame();
        let source = first_frame.source_generation();
        let first_target = image_target(&first_frame);
        let first = next_viewport_reservation_stamp(
            WgpuEditorViewId::new(1),
            0,
            0,
            None,
            11,
            2,
            source,
            first_target,
        )
        .unwrap();
        let duplicate = first;
        let resized_target = nara_render::__private::editor_viewport_image_target(
            first_target.image(),
            nara_render::Extent2d::new(128, 64).unwrap(),
        )
        .unwrap();
        let successor = next_viewport_reservation_stamp(
            WgpuEditorViewId::new(1),
            first.generation,
            first.source_handoff_generation,
            Some(first.source_generation),
            11,
            2,
            source,
            resized_target,
        )
        .unwrap();

        assert_ne!(duplicate, successor);
        assert_eq!(successor.generation, first.generation + 1);
        assert_eq!(
            successor.source_handoff_generation,
            first.source_handoff_generation
        );
    }

    #[test]
    fn matching_runtime_and_target_reservations_publish_together_or_not_at_all() {
        #[derive(Debug)]
        struct RuntimeReservation {
            source: RenderFrameSourceGeneration,
            target: RenderImagePacket,
        }

        #[derive(Default)]
        struct Publication {
            runtime: Option<RenderFrameSourceGeneration>,
            target: Option<WgpuEditorViewportReservationStamp>,
        }

        impl Publication {
            fn commit(
                &mut self,
                runtime: RuntimeReservation,
                target: WgpuEditorViewportReservationStamp,
            ) -> Result<(), (RuntimeReservation, WgpuEditorViewportReservationStamp)> {
                if self.runtime.is_some()
                    || self.target.is_some()
                    || runtime.source != target.source_generation
                    || runtime.target != target.target
                {
                    return Err((runtime, target));
                }
                self.runtime = Some(runtime.source);
                self.target = Some(target);
                Ok(())
            }
        }

        let frame = test_image_frame();
        let source = frame.source_generation();
        let target = image_target(&frame);
        let target_reservation = next_viewport_reservation_stamp(
            WgpuEditorViewId::new(1),
            0,
            0,
            None,
            11,
            2,
            source,
            target,
        )
        .unwrap();
        let mut publication = Publication::default();
        let mismatched_runtime = RuntimeReservation {
            source: test_image_frame().source_generation(),
            target,
        };
        let (_runtime, target_reservation) = publication
            .commit(mismatched_runtime, target_reservation)
            .expect_err("mismatched reservations must remain unpublished");
        assert!(publication.runtime.is_none());
        assert!(publication.target.is_none());

        publication
            .commit(RuntimeReservation { source, target }, target_reservation)
            .unwrap();
        assert_eq!(publication.runtime, Some(source));
        assert_eq!(publication.target, Some(target_reservation));
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn editor_ui_completion_maps_every_terminal_path_without_stranding_permits() {
        let backend_generation = UiPresentationBackendGeneration::new(21, 4).unwrap();

        let (outcomes, mut presented) = editor_ui_submission(31);
        presented.bind_backend(backend_generation).unwrap();
        assert!(matches!(
            publish_editor_ui_outcomes(outcomes, presented.presented()).remove(0),
            UiPresentationOutcome::Presented(_)
        ));

        let (outcomes, mut deferred) = editor_ui_submission(32);
        deferred.bind_backend(backend_generation).unwrap();
        let upload_deferred = WgpuEditorExecutionError::Backend(WgpuRenderError::UploadDeferred {
            message: "shared upload budget exhausted".to_owned(),
        });
        assert!(matches!(
            publish_editor_ui_outcomes(outcomes, deferred.failed(&upload_deferred)).remove(0),
            UiPresentationOutcome::Deferred { .. }
        ));

        let (outcomes, rejected) = editor_ui_submission(33);
        assert!(matches!(
            publish_editor_ui_outcomes(
                outcomes,
                rejected.content_rejected("packet lowering rejected".to_owned())
            )
            .remove(0),
            UiPresentationOutcome::ContentRejected { .. }
        ));

        let (outcomes, mut target_fault) = editor_ui_submission(34);
        target_fault.bind_backend(backend_generation).unwrap();
        assert!(matches!(
            publish_editor_ui_outcomes(
                outcomes,
                target_fault.backend_fault(
                    Some(backend_generation),
                    UiPresentationBackendFaultDomain::Target,
                    "viewport target retired".to_owned(),
                )
            )
            .remove(0),
            UiPresentationOutcome::BackendFault {
                domain: UiPresentationBackendFaultDomain::Target,
                ..
            }
        ));

        let (outcomes, mut device_fault) = editor_ui_submission(35);
        device_fault.bind_backend(backend_generation).unwrap();
        assert!(matches!(
            publish_editor_ui_outcomes(
                outcomes,
                device_fault.backend_fault(
                    Some(backend_generation),
                    UiPresentationBackendFaultDomain::Device,
                    "device lost".to_owned(),
                )
            )
            .remove(0),
            UiPresentationOutcome::BackendFault {
                backend_generation: Some(generation),
                domain: UiPresentationBackendFaultDomain::Device,
                ..
            } if generation == backend_generation
        ));

        let (outcomes, superseded) = editor_ui_submission(36);
        let successor =
            nara_ui_render::__private::prepared_attempt(&test_ui_prepared_submission(37));
        assert!(matches!(
            publish_editor_ui_outcomes(
                outcomes,
                superseded.superseded(successor, "new viewport candidate".to_owned())
            )
            .remove(0),
            UiPresentationOutcome::Superseded {
                successor: actual,
                ..
            } if actual == successor
        ));
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn editor_ui_presented_completion_requires_the_exact_final_composite() {
        let frame = test_image_frame();
        let image = image_target(&frame);
        let submitted = WgpuSubmittedImageReceipt::new(
            WgpuEditorViewId::new(1),
            image,
            8,
            frame.source_generation(),
            3,
            21,
            4,
            12,
        );
        let window = WgpuEditorWindowPresentedReceipt::new(
            WindowId::PRIMARY,
            5,
            submitted.backend_instance_id(),
            submitted.device_epoch(),
            17,
        );
        let (outcomes, mut ui) = editor_ui_submission(38);
        ui.bind_backend(UiPresentationBackendGeneration::new(21, 4).unwrap())
            .unwrap();
        let submission = WgpuEditorUiImageSubmission {
            image: submitted,
            ui,
        };
        let foreign = WgpuEditorCompositeReceipt::new(
            WgpuSubmittedImageReceipt::new(
                WgpuEditorViewId::new(1),
                image,
                9,
                frame.source_generation(),
                3,
                21,
                4,
                12,
            ),
            window,
            1,
        );
        let submission = submission
            .presented(foreign)
            .expect_err("a foreign image receipt must not commit Runtime UI presentation");
        let exact = WgpuEditorCompositeReceipt::new(submitted, window, 1);

        assert!(matches!(
            publish_editor_ui_outcomes(outcomes, submission.presented(exact).unwrap()).remove(0),
            UiPresentationOutcome::Presented(_)
        ));
    }

    fn test_execution() -> WgpuEditorExecution {
        WgpuEditorExecution {
            #[cfg(feature = "editor-imgui")]
            primary_window_id: WindowId::PRIMARY,
            backend: WgpuRenderBackend::default(),
            handles: BackendWindowHandles::default(),
            views: BTreeMap::new(),
            host_frame_index: None,
            last_presentation_generation: 0,
        }
    }

    fn activate_test_view(
        execution: &mut WgpuEditorExecution,
        view_id: WgpuEditorViewId,
        source: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> WgpuEditorViewportReservationStamp {
        let stamp = execution
            .next_viewport_reservation_stamp_for_namespace(
                view_id,
                execution.backend.instance_id(),
                execution.backend.device_epoch(),
                source,
                target,
            )
            .unwrap();
        execution
            .views
            .entry(view_id)
            .or_default()
            .admission
            .activate(stamp);
        stamp
    }

    fn test_image_frame() -> RenderFramePacket {
        let generation = EditorRenderGeneration::try_acquire().unwrap();
        let image = RenderImage2d::allocate_handle().unwrap();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            camera_entity: Entity::PLACEHOLDER,
            target: RenderTarget::Image(image),
            viewport: ViewportRect::new(0, 0, 64, 64).unwrap(),
            mapping: nara_render::ViewMapping2d::orthographic(
                nara_core::Mat3::IDENTITY,
                64.0,
                ViewportRect::new(0, 0, 64, 64).unwrap(),
            )
            .unwrap(),
            order: 0,
            load: nara_render::ViewLoadOp::Clear(Color::BLACK),
        });
        build_editor_render_frame_packet(
            generation,
            1,
            &views,
            nara_render::__private::editor_viewport_image_target(
                image,
                nara_render::Extent2d::new(64, 64).unwrap(),
            )
            .unwrap(),
        )
        .unwrap()
        .unwrap()
    }

    fn image_target(packet: &RenderFramePacket) -> RenderImagePacket {
        match packet.target() {
            RenderFrameTarget::Image(image) => image,
            RenderFrameTarget::Window(_) => panic!("test packet must target an image"),
        }
    }

    #[cfg(feature = "ui-submitter")]
    fn editor_ui_submission(
        admission_id: u64,
    ) -> (
        nara_ui_render::__private::UiPresentationOutcomes,
        WgpuEditorUiSubmission,
    ) {
        let prepared = test_ui_prepared_submission(admission_id);
        let mut outcomes = nara_ui_render::__private::UiPresentationOutcomes::default();
        let permit = outcomes
            .reserve_backend(std::slice::from_ref(&prepared))
            .unwrap()
            .remove(0);
        let submission = UiBackendSubmission::pair(prepared, permit).unwrap();
        (
            outcomes,
            WgpuEditorUiSubmission {
                state: WgpuUiSubmissionState::Prepared(vec![submission]),
            },
        )
    }

    #[cfg(feature = "ui-submitter")]
    fn test_ui_prepared_submission(
        admission_id: u64,
    ) -> nara_ui_render::__private::UiPreparedBackendSubmission {
        nara_ui_render::__private::prepare_backend_submission(
            UiPresentationAttemptId::new(admission_id).unwrap(),
            test_ui_packet(),
            UiPresentationEpoch::new(admission_id + 100).unwrap(),
            UiPresentationRuntimeGeneration::new(7).unwrap(),
            UiPresentationCapabilityGeneration::new(9).unwrap(),
        )
    }

    #[cfg(feature = "ui-submitter")]
    fn publish_editor_ui_outcomes(
        mut outcomes: nara_ui_render::__private::UiPresentationOutcomes,
        completed: Vec<UiCompletedBackendOutcome>,
    ) -> Vec<UiPresentationOutcome> {
        for outcome in completed {
            outcomes.commit_backend(outcome);
        }
        let mut world = nara_ecs::World::new();
        world.insert_resource(outcomes);
        nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap()
    }

    #[cfg(feature = "ui-submitter")]
    fn test_ui_packet() -> std::sync::Arc<UiPaintPacket> {
        use nara_text::bundled_font_face;
        use nara_ui_kernel::{
            UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiLayer, UiLocalKey,
            UiNode, UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiTheme, UiVisual,
        };

        let snapshot = UiAuthoringSnapshotBuilder::new(1)
            .with_root(
                UiRoot::new(
                    UiRootKey::runtime("editor-game-view-ui", "root").unwrap(),
                    UiLayer::Base,
                    UiStyle::fill(),
                )
                .with_child(
                    UiNode::new(UiStyle::absolute(0.0, 0.0, 32.0, 32.0))
                        .with_key(UiLocalKey::runtime("editor-game-view-ui", "panel").unwrap())
                        .with_visual(UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0))),
                ),
            )
            .build()
            .unwrap();
        let environment =
            UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 64.0, 64.0, 1.0, 1, 1, 1, 1).unwrap();
        let frame = UiCompiler::new(bundled_font_face().unwrap(), Default::default())
            .unwrap()
            .compile(&snapshot, environment, UiTheme::default())
            .unwrap();
        nara_ui_render::UiPresentationPlanner::default()
            .plan(
                frame,
                nara_render::RenderTarget::Window(WindowId::PRIMARY),
                &nara_ui_render::UiPreparedImages::default(),
            )
            .unwrap()
    }
}
