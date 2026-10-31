use std::collections::BTreeMap;
mod runtime;
pub use runtime::{WgpuRuntimeHostFacts, WgpuRuntimeMaintenance};
#[cfg(feature = "editor-imgui")]
mod imgui;
#[cfg(feature = "editor-imgui")]
pub use imgui::{WgpuEditorImguiFrame, WgpuEditorImguiRenderer, WgpuEditorImguiRendererFailure};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

static NEXT_RENDER_PRODUCER: AtomicU64 = AtomicU64::new(1);

use nara_render::{
    Extent2d, RenderFramePacket, RenderFrameSourceGeneration, RenderFrameTarget, RenderImagePacket,
    RenderWindowPacket,
};
#[cfg(feature = "stock-ui")]
use nara_ui_render::__private::{UiBackendPresentationFrame, UiCompletedBackendOutcome};
#[cfg(feature = "stock-ui")]
use nara_ui_render::{
    UiPresentationAttemptIdentity, UiPresentationBackendFaultDomain,
    UiPresentationBackendGeneration,
};
use nara_window::{
    WindowId,
    backend::{BackendWindowHandles, WindowHandleProvider, WindowTargetError},
};
use thiserror::Error;

#[cfg(feature = "stock-ui")]
use crate::frame::{WgpuUiCompletion, WgpuUiSubmissionState, complete_ui_submissions};
use crate::{
    WgpuFrameTransactionStats, WgpuRenderBackend, WgpuRenderError,
    backend::{WgpuDeviceNamespace, WgpuEditorAcquiredSurface},
    output::{OutputBudget, OutputReservation},
    output::{WgpuDisplayOccurrence, WgpuImageDisplay, WgpuImageOutput, WgpuImageStorage},
    policy::{BuiltPolicy, PolicyExecutionCustody, PolicyInstances},
    presented::{
        WgpuEditorCompositeReceipt, WgpuEditorWindowPresentedReceipt, WgpuSubmittedImageReceipt,
    },
    retirement::WgpuGpuRetirement,
    submission::{WgpuSubmissionPrefix, WgpuSubmissionReservation},
};

const EDITOR_IMAGE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const MAX_PENDING_NATIVE_SUBMISSIONS: usize = 256;

struct ProgramSurfaceTarget<'a> {
    acquired: &'a WgpuEditorAcquiredSurface,
    final_version: &'a crate::graph::ResourceVersion,
    instance_buffer_bytes: u64,
    app_frame_index: u64,
    work_started_at: std::time::Instant,
    submitted_texture_bytes: &'a mut u64,
}

#[cfg(test)]
std::thread_local! {
    // Fail the selected texture/view allocation check without exhausting GPU memory.
    static VIEWPORT_ALLOCATION_FAILURE: std::cell::Cell<Option<u8>> = const {
        std::cell::Cell::new(None)
    };
    // Destroy an encoded attachment immediately before the real native submit validation.
    static IMAGE_SUBMISSION_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    static PROGRAM_SUBMISSION_FAILURE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn finish_viewport_allocation(
    scope: wgpu::ErrorScopeGuard,
    extent: Extent2d,
) -> Result<(), WgpuRenderHostError> {
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
        Some(message) => Err(WgpuRenderHostError::ViewportTargetAllocationDeferred {
            width: extent.width,
            height: extent.height,
            message,
        }),
        None => Ok(()),
    }
}

/// Copyable process-local producer facts. Admission requires its owning [`WgpuRenderProducer`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct WgpuRenderProducerId(u64);

impl WgpuRenderProducerId {
    /// Allocates a process-local, non-reused producer identity.
    fn try_acquire() -> Option<Self> {
        NEXT_RENDER_PRODUCER
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                value.checked_add(1)
            })
            .ok()
            .map(Self)
    }

    #[cfg(test)]
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Admission authority for one producer. Closing or dropping it is irreversible.
#[derive(Debug)]
pub struct WgpuRenderProducer {
    id: WgpuRenderProducerId,
    backend_instance_id: u64,
    closed: Arc<AtomicBool>,
    world_presentation: Arc<WorldPresentationSlot>,
}

impl WgpuRenderProducer {
    #[must_use]
    pub const fn id(&self) -> WgpuRenderProducerId {
        self.id
    }
}

impl Drop for WgpuRenderProducer {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Release);
        self.world_presentation.invalidate();
    }
}

/// Producer Drop revokes interaction synchronously, independently of native maintenance.
#[derive(Debug, Default)]
struct WorldPresentationSlot(
    std::sync::Mutex<Option<nara_render::__private::WorldPresentationRevoker>>,
);

impl WorldPresentationSlot {
    fn replace(
        &self,
        closed: &AtomicBool,
        revoker: nara_render::__private::WorldPresentationRevoker,
    ) -> Result<(), WgpuRenderHostError> {
        let mut current = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if closed.load(Ordering::Acquire) {
            revoker.invalidate();
            return Err(WgpuRenderHostError::ProducerClosed);
        }
        if let Some(previous) = current.replace(revoker) {
            previous.invalidate();
        }
        Ok(())
    }

    fn invalidate(&self) {
        if let Some(current) = self
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        {
            current.invalidate();
        }
    }
}

/// A failed target, content or native execution attempt on the shared WGPU Host.
#[derive(Debug, Error)]
pub enum WgpuRenderHostError {
    #[error(transparent)]
    Backend(#[from] WgpuRenderError),
    #[error(transparent)]
    Program(crate::WgpuRenderProgramError),
    #[error("render Feature requirements were refused: {0}")]
    FeatureRequirements(crate::WgpuDeviceRequirementsError),
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
    #[error("retained image output storage exceeds the Host byte limit")]
    OutputCapacity,
    #[error("the render producer is closed or belongs to another Host")]
    ProducerClosed,
    #[error("native render execution failed: {message}")]
    NativeExecution { message: String },
}

impl From<crate::WgpuRenderProgramError> for WgpuRenderHostError {
    fn from(error: crate::WgpuRenderProgramError) -> Self {
        match error {
            crate::WgpuRenderProgramError::Backend(error) => Self::Backend(error),
            error => Self::Program(error),
        }
    }
}

impl WgpuRenderHostError {
    const fn backend_error(&self) -> Option<&WgpuRenderError> {
        match self {
            Self::Backend(error) | Self::Program(crate::WgpuRenderProgramError::Backend(error)) => {
                Some(error)
            }
            _ => None,
        }
    }
    #[must_use]
    pub fn toolkit_paint(message: impl Into<String>) -> Self {
        Self::ToolkitPaint {
            message: message.into(),
        }
    }

    #[must_use]
    pub const fn requires_owner_replacement(&self) -> bool {
        matches!(
            self.backend_error(),
            Some(
                WgpuRenderError::BackendUnavailable
                    | WgpuRenderError::DeviceLost
                    | WgpuRenderError::DeviceEpochExhausted
            )
        ) || matches!(self, Self::NativeExecution { .. })
    }

    #[must_use]
    pub const fn is_deferred(&self) -> bool {
        matches!(
            self,
            Self::ViewportTargetAllocationDeferred { .. }
                | Self::OutputCapacity
                | Self::Program(crate::WgpuRenderProgramError::ResourceCapacity)
        ) || matches!(self.backend_error(), Some(error) if error.is_deferred())
    }

    #[must_use]
    pub const fn is_content_rejected(&self) -> bool {
        matches!(self.backend_error(), Some(error) if error.is_content_rejection())
            || matches!(self, Self::Program(error) if !matches!(error, crate::WgpuRenderProgramError::ResourceCapacity | crate::WgpuRenderProgramError::Backend(_)))
            || matches!(self, Self::FeatureRequirements(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ImageTargetStamp {
    view_id: WgpuRenderProducerId,
    target: RenderImagePacket,
    generation: u64,
    source_generation: RenderFrameSourceGeneration,
    source_handoff_generation: u64,
    backend_instance_id: u64,
    device_epoch: u64,
}

/// Copyable configuration identity, independent of immutable pixel output versions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WgpuImageTargetIdentity {
    stamp: ImageTargetStamp,
}

impl WgpuImageTargetIdentity {
    #[must_use]
    pub const fn producer_id(self) -> WgpuRenderProducerId {
        self.stamp.view_id
    }

    #[must_use]
    pub fn matches_submission(self, receipt: WgpuSubmittedImageReceipt) -> bool {
        self.stamp.view_id == receipt.producer_id()
            && self.stamp.target == receipt.target()
            && self.stamp.generation == receipt.target_generation()
            && self.stamp.source_generation == receipt.source_generation()
            && self.stamp.source_handoff_generation == receipt.source_handoff_generation()
            && self.stamp.backend_instance_id == receipt.backend_instance_id()
            && self.stamp.device_epoch == receipt.device_epoch()
    }
}

struct ImageTarget {
    stamp: ImageTargetStamp,
    initial_storage: Option<(wgpu::Texture, wgpu::TextureView, Arc<OutputReservation>)>,
}

/// Move-only inactive image target prepared for one coordinated Host publication.
pub struct WgpuImageTargetReservation {
    stamp: ImageTargetStamp,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    storage_reservation: OutputReservation,
}

/// Final target validation retaining exclusive Host and producer borrows until commit.
pub struct WgpuImageTargetActivation<'host> {
    host: &'host mut WgpuRenderHost,
    _producer: &'host WgpuRenderProducer,
    reservation: WgpuImageTargetReservation,
}

impl std::fmt::Debug for WgpuImageTargetActivation<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuImageTargetActivation")
            .field("stamp", &self.reservation.stamp)
            .finish_non_exhaustive()
    }
}

impl WgpuImageTargetActivation<'_> {
    #[must_use]
    pub const fn identity(&self) -> WgpuImageTargetIdentity {
        WgpuImageTargetIdentity {
            stamp: self.reservation.stamp,
        }
    }

    /// Publishes under the same exclusive Host borrow used for final validation.
    pub fn commit(self) {
        self.host.commit_image_target_activation(self.reservation);
    }
}

impl std::fmt::Debug for WgpuImageTargetReservation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuImageTargetReservation")
            .field("stamp", &self.stamp)
            .finish_non_exhaustive()
    }
}

impl WgpuImageTargetReservation {
    #[must_use]
    pub const fn identity(&self) -> WgpuImageTargetIdentity {
        WgpuImageTargetIdentity { stamp: self.stamp }
    }
}

/// Original move-only UI completion responsibility, separate from replayable drawing packets.
#[cfg(feature = "stock-ui")]
#[derive(Debug)]
pub struct WgpuUiSubmission {
    state: WgpuUiSubmissionState,
}

#[cfg(feature = "stock-ui")]
impl WgpuUiSubmission {
    pub(crate) fn from_state(state: WgpuUiSubmissionState) -> Self {
        Self { state }
    }

    pub(crate) fn into_state(self) -> WgpuUiSubmissionState {
        self.state
    }

    /// Splits original drawing packets from their one-shot completion obligations.
    pub fn capture(
        frame: UiBackendPresentationFrame,
    ) -> (Vec<Arc<nara_ui_render::UiPaintPacket>>, Self) {
        let (packets, submissions) = frame.into_parts();
        (
            packets,
            Self::from_state(WgpuUiSubmissionState::Prepared(submissions)),
        )
    }

    /// Transfers the untouched attempts to a trusted backend before native binding.
    ///
    /// This moves the original permissions without reconstructing a frame or asserting pixels.
    /// Once bound, refusal returns this same sidecar. Ordinary image consumers keep it in
    /// [`WgpuRenderHost::submit_for_display`] and complete the designated display occurrence.
    pub fn into_prepared_submissions(
        self,
    ) -> Result<Vec<nara_ui_render::advanced::UiBackendSubmission>, Self> {
        match self.state {
            WgpuUiSubmissionState::Prepared(submissions) => Ok(submissions),
            state => Err(Self { state }),
        }
    }

    fn validate_backend_generation(
        &self,
        backend_generation: UiPresentationBackendGeneration,
    ) -> Result<(), WgpuRenderHostError> {
        self.state
            .validate_backend_generation(backend_generation)
            .map_err(Into::into)
    }

    fn bind_backend(
        &mut self,
        backend_generation: UiPresentationBackendGeneration,
    ) -> Result<(), WgpuRenderHostError> {
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

    /// Cancels an unsubmitted capture, or defers its original bound backend attempt.
    #[must_use]
    pub fn cancelled(self, message: String) -> Vec<UiCompletedBackendOutcome> {
        complete_ui_submissions(self.state, WgpuUiCompletion::Cancelled(message))
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

    fn failed(self, error: &WgpuRenderHostError) -> Vec<UiCompletedBackendOutcome> {
        if error.is_deferred() {
            return self.deferred(error.to_string());
        }
        if error.is_content_rejected() {
            return self.content_rejected(error.to_string());
        }
        let domain = match error {
            WgpuRenderHostError::Backend(error)
            | WgpuRenderHostError::Program(crate::WgpuRenderProgramError::Backend(error)) => {
                crate::frame::classify_ui_backend_fault(error)
            }
            WgpuRenderHostError::WindowTarget(_)
            | WgpuRenderHostError::StaleImageSubmission
            | WgpuRenderHostError::ViewportTargetAlreadyActive
            | WgpuRenderHostError::ViewportTargetUnavailable
            | WgpuRenderHostError::ViewportTargetExtentUnsupported { .. }
            | WgpuRenderHostError::ViewportTargetAllocationDeferred { .. }
            | WgpuRenderHostError::OutputCapacity
            | WgpuRenderHostError::ProducerClosed
            | WgpuRenderHostError::StaleViewportReservation => {
                UiPresentationBackendFaultDomain::Target
            }
            WgpuRenderHostError::ExpectedImageTarget
            | WgpuRenderHostError::Program(_)
            | WgpuRenderHostError::FeatureRequirements(_)
            | WgpuRenderHostError::TargetGenerationExhausted
            | WgpuRenderHostError::SourceHandoffGenerationExhausted
            | WgpuRenderHostError::PresentationGenerationExhausted
            | WgpuRenderHostError::FrameTransactionUnavailable
            | WgpuRenderHostError::ToolkitPaint { .. } => UiPresentationBackendFaultDomain::Attempt,
            WgpuRenderHostError::NativeExecution { .. } => UiPresentationBackendFaultDomain::Device,
        };
        self.backend_fault(None, domain, error.to_string())
    }
}

/// One submitted image retaining UI completion authority until its designated display completes.
#[cfg(feature = "stock-ui")]
#[derive(Debug)]
pub struct WgpuUiImageSubmission {
    display: WgpuImageDisplay,
    interaction: Arc<AtomicBool>,
    producer_closed: Arc<AtomicBool>,
    ui: WgpuUiSubmission,
}

#[cfg(feature = "stock-ui")]
impl WgpuUiImageSubmission {
    #[must_use]
    pub fn image(&self) -> WgpuSubmittedImageReceipt {
        self.display.receipt()
    }

    #[must_use]
    pub fn output(&self) -> &WgpuImageOutput {
        self.display.output()
    }

    /// The sole display occurrence allowed to complete this original UI submission.
    #[must_use]
    pub const fn display(&self) -> &WgpuImageDisplay {
        &self.display
    }

    pub fn presented(
        self,
        composite: WgpuEditorCompositeReceipt,
    ) -> Result<Vec<UiCompletedBackendOutcome>, Self> {
        if composite.image() != self.image()
            || composite.occurrence() != self.display.occurrence()
            || !self.output().is_current()
            || !self.interaction.load(Ordering::Acquire)
            || self.producer_closed.load(Ordering::Acquire)
        {
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
    pub fn failed(self, error: &WgpuRenderHostError) -> Vec<UiCompletedBackendOutcome> {
        self.ui.failed(error)
    }
}

/// Failed image submission retaining the exact Runtime UI authority for terminal completion.
#[cfg(feature = "stock-ui")]
#[derive(Debug)]
pub struct WgpuUiSubmitFailure {
    error: WgpuRenderHostError,
    ui: WgpuUiSubmission,
}

#[cfg(feature = "stock-ui")]
impl WgpuUiSubmitFailure {
    #[must_use]
    pub const fn error(&self) -> &WgpuRenderHostError {
        &self.error
    }

    #[must_use]
    pub fn into_parts(self) -> (WgpuRenderHostError, WgpuUiSubmission) {
        (self.error, self.ui)
    }

    #[must_use]
    pub fn complete(self) -> (WgpuRenderHostError, Vec<UiCompletedBackendOutcome>) {
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

    /// Retains the original device and queue before renderer initialization can queue uploads.
    #[must_use]
    pub fn gpu_retirement(&self) -> WgpuGpuRetirement {
        WgpuGpuRetirement::new(self.device, self.queue)
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

#[derive(Clone, Copy)]
struct ImageSample {
    image: WgpuSubmittedImageReceipt,
    occurrence: WgpuDisplayOccurrence,
}

fn join_composite_receipt(
    last_presentation_generation: &mut u64,
    submitted_viewport: Option<WgpuSubmittedImageReceipt>,
    sampled_viewport: Option<ImageSample>,
    window: WgpuEditorWindowPresentedReceipt,
) -> Result<Option<WgpuEditorCompositeReceipt>, WgpuRenderHostError> {
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
        .ok_or(WgpuRenderHostError::PresentationGenerationExhausted)?;
    *last_presentation_generation = generation;
    Ok(Some(WgpuEditorCompositeReceipt::new(
        sample.image,
        sample.occurrence,
        window,
        generation,
    )))
}

fn join_composite_receipts(
    last_presentation_generation: &mut u64,
    submitted: &[WgpuImageDisplay],
    sampled: &[ImageSample],
    window: WgpuEditorWindowPresentedReceipt,
) -> Result<Vec<WgpuEditorCompositeReceipt>, WgpuRenderHostError> {
    let mut composites = Vec::with_capacity(sampled.len());
    for sample in sampled {
        if composites
            .iter()
            .any(|composite: &WgpuEditorCompositeReceipt| {
                composite.occurrence() == sample.occurrence
            })
        {
            continue;
        }
        let candidate = submitted
            .iter()
            .find(|display| {
                display.receipt() == sample.image && display.occurrence() == sample.occurrence
            })
            .map(WgpuImageDisplay::receipt);
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
) -> Result<u64, WgpuRenderHostError> {
    if current_source == Some(next_source) {
        return Ok(last_generation);
    }
    last_generation
        .checked_add(1)
        .ok_or(WgpuRenderHostError::SourceHandoffGenerationExhausted)
}

fn next_viewport_reservation_stamp(
    view_id: WgpuRenderProducerId,
    last_target_generation: u64,
    last_source_handoff_generation: u64,
    current_source: Option<RenderFrameSourceGeneration>,
    backend_instance_id: u64,
    device_epoch: u64,
    source_generation: RenderFrameSourceGeneration,
    target: RenderImagePacket,
) -> Result<ImageTargetStamp, WgpuRenderHostError> {
    let generation = last_target_generation
        .checked_add(1)
        .ok_or(WgpuRenderHostError::TargetGenerationExhausted)?;
    let source_handoff_generation = next_source_handoff_generation(
        last_source_handoff_generation,
        current_source,
        source_generation,
    )?;
    Ok(ImageTargetStamp {
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
    viewports: &'frame [WgpuImageDisplay],
    leading_command_buffers: &'frame mut Vec<wgpu::CommandBuffer>,
    sampled_viewports: &'frame mut Vec<ImageSample>,
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
        display: &WgpuImageDisplay,
    ) -> Result<(), WgpuRenderHostError> {
        if !self.viewports.contains(display) {
            return Err(WgpuRenderHostError::StaleImageSubmission);
        }
        self.sampled_viewports.push(ImageSample {
            image: display.receipt(),
            occurrence: display.occurrence(),
        });
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
struct ProducerAdmission {
    source_generation: Option<RenderFrameSourceGeneration>,
    last_frame_index: Option<u64>,
    last_target_generation: u64,
    last_source_handoff_generation: u64,
}

impl ProducerAdmission {
    fn activate(&mut self, stamp: ImageTargetStamp) {
        if self.source_generation != Some(stamp.source_generation) {
            self.last_frame_index = None;
        }
        self.source_generation = Some(stamp.source_generation);
        self.last_target_generation = stamp.generation;
        self.last_source_handoff_generation = stamp.source_handoff_generation;
    }

    fn admit(
        &self,
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
        Ok(())
    }
}

#[derive(Default)]
struct ProducerState {
    closed: Arc<AtomicBool>,
    admission: ProducerAdmission,
    target: Option<ImageTarget>,
    policies: PolicyInstances,
    world_presentation: Arc<WorldPresentationSlot>,
    runtime_presentation: Option<runtime::RuntimeWorldPresentation>,
    submitted_prefix: Option<WgpuSubmissionPrefix>,
    interaction: Arc<AtomicBool>,
}

impl Drop for ProducerState {
    fn drop(&mut self) {
        self.interaction.store(false, Ordering::Release);
        self.world_presentation.invalidate();
    }
}

/// Serialized native WGPU owner shared by windowed Editor and image-only consumers.
///
/// Producers own admission; immutable outputs own pixels; submitted prefixes own their actual
/// resources. A retained output does not grant interaction or reconstruction authority.
/// Call [`Self::poll_retirement`] to finish the Host after stopping its consumers.
pub struct WgpuRenderHost {
    #[cfg(feature = "editor-imgui")]
    primary_window_id: WindowId,
    backend: WgpuRenderBackend,
    handles: Option<BackendWindowHandles>,
    presentation: crate::presentation::SurfacePresentationCache,
    views: BTreeMap<WgpuRenderProducerId, ProducerState>,
    host_frame_index: Option<u64>,
    last_presentation_generation: u64,
    output_budget: Arc<OutputBudget>,
    feature_states: crate::program::NativeFeatureStates,
    native_prefix: Option<WgpuSubmissionPrefix>,
    retiring_prefixes: Vec<WgpuSubmissionPrefix>,
    abandoned_submission_count: u64,
    retired: bool,
}

impl std::fmt::Debug for WgpuRenderHost {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuRenderHost")
            .field("backend_instance_id", &self.backend.instance_id())
            .field("device_epoch", &self.backend.device_epoch())
            .field("view_count", &self.views.len())
            .finish_non_exhaustive()
    }
}

impl Drop for WgpuRenderHost {
    fn drop(&mut self) {
        if self.retired {
            return;
        }
        // Use the original retained pair, including after loss or namespace cleanup. The budget
        // bounds our retirement drive, not arbitrary WGPU callbacks or native destructors.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| self.poll_retirement()))
            {
                Ok(Ok(true)) => return,
                Err(_) => std::process::abort(),
                Ok(_) => {}
            }
            if std::time::Instant::now() >= deadline {
                // Match the native Session policy: an incomplete owner cannot drop dependencies
                // as if physical retirement succeeded. Explicit close keeps its retryable owner.
                std::process::abort();
            }
            std::thread::yield_now();
        }
    }
}

impl WgpuRenderHost {
    pub fn create_producer(&mut self) -> Result<WgpuRenderProducer, WgpuRenderHostError> {
        self.maintain_producers()?;
        if self.backend.runtime_close_started() {
            return Err(WgpuRenderHostError::ProducerClosed);
        }
        let id = WgpuRenderProducerId::try_acquire()
            .ok_or(WgpuRenderHostError::TargetGenerationExhausted)?;
        let closed = Arc::new(AtomicBool::new(false));
        let mut view = ProducerState::default();
        view.closed = Arc::clone(&closed);
        let world_presentation = Arc::clone(&view.world_presentation);
        self.views.insert(id, view);
        Ok(WgpuRenderProducer {
            id,
            backend_instance_id: self.backend.instance_id(),
            closed,
            world_presentation,
        })
    }

    fn producer_id(
        &self,
        producer: &WgpuRenderProducer,
    ) -> Result<WgpuRenderProducerId, WgpuRenderHostError> {
        if producer.backend_instance_id != self.backend.instance_id()
            || producer.closed.load(Ordering::Acquire)
            || !self.views.contains_key(&producer.id)
            || self.backend.runtime_close_started()
        {
            return Err(WgpuRenderHostError::ProducerClosed);
        }
        Ok(producer.id)
    }
    pub fn new(
        window_id: WindowId,
        provider: WindowHandleProvider,
    ) -> Result<Self, WgpuRenderHostError> {
        let handles = BackendWindowHandles::default();
        handles.insert(window_id, provider)?;
        let mut backend = WgpuRenderBackend::default();
        backend.begin_runtime_execution();
        let output_budget = Arc::clone(backend.resource_budget());
        Ok(Self {
            #[cfg(feature = "editor-imgui")]
            primary_window_id: window_id,
            backend,
            handles: Some(handles),
            presentation: crate::presentation::SurfacePresentationCache::default(),
            views: BTreeMap::new(),
            host_frame_index: None,
            last_presentation_generation: 0,
            output_budget,
            feature_states: crate::program::NativeFeatureStates::default(),
            native_prefix: None,
            retiring_prefixes: Vec::new(),
            abandoned_submission_count: 0,
            retired: false,
        })
    }

    /// Creates the same execution owner without a window, platform handle or hidden Surface.
    #[must_use]
    pub fn offscreen() -> Self {
        let mut backend = WgpuRenderBackend::default();
        backend.begin_runtime_execution();
        let output_budget = Arc::clone(backend.resource_budget());
        Self {
            #[cfg(feature = "editor-imgui")]
            primary_window_id: WindowId::PRIMARY,
            backend,
            handles: None,
            presentation: crate::presentation::SurfacePresentationCache::default(),
            views: BTreeMap::new(),
            host_frame_index: None,
            last_presentation_generation: 0,
            output_budget,
            feature_states: crate::program::NativeFeatureStates::default(),
            native_prefix: None,
            retiring_prefixes: Vec::new(),
            abandoned_submission_count: 0,
            retired: false,
        }
    }

    /// Adds a real native target to this device owner. Its surface is checked against the
    /// already selected Adapter on first use; incompatibility never replaces a healthy device.
    pub fn register_window(
        &mut self,
        window_id: WindowId,
        provider: WindowHandleProvider,
    ) -> Result<(), WgpuRenderHostError> {
        if self.backend.runtime_close_started() {
            return Err(WgpuRenderError::BackendUnavailable.into());
        }
        self.handles
            .get_or_insert_with(BackendWindowHandles::default)
            .insert(window_id, provider)?;
        Ok(())
    }

    /// Sets the Host's shared logical native payload budget.
    ///
    /// Charges include inactive output reservations, retained outputs, graph allocations, and
    /// prepared Image, glyph, fallback, instance and material allocations. Each allocation is
    /// charged once until its last owner or submitted-use responsibility releases it.
    /// Lowering the limit preserves existing allocations and refuses growth until they retire.
    /// Native pipeline/driver overhead and physical driver residency are outside this budget.
    #[must_use]
    pub fn with_resource_byte_limit(self, bytes: u64) -> Self {
        self.output_budget.set_limit(bytes);
        self
    }

    /// Returns retained logical native payload bytes under the Host's shared resource budget.
    ///
    /// Reservations and allocations remain charged until their last responsibility releases,
    /// including cached state, output leases and submitted uses. Shared guards count once.
    /// This excludes native pipeline/driver overhead and does not measure physical driver residency.
    #[must_use]
    pub fn retained_resource_bytes(&self) -> u64 {
        self.output_budget.retained()
    }

    fn prepare_namespace(
        &mut self,
        window: Option<RenderWindowPacket>,
    ) -> Result<WgpuDeviceNamespace, WgpuRenderHostError> {
        let handles = self.handles.clone();
        self.prepare_namespace_with_authority(window, handles.as_ref())
    }

    fn prepare_namespace_with_authority(
        &mut self,
        window: Option<RenderWindowPacket>,
        handles: Option<&BackendWindowHandles>,
    ) -> Result<WgpuDeviceNamespace, WgpuRenderHostError> {
        self.maintain_producers()?;
        if self.backend.runtime_close_started() {
            return Err(WgpuRenderError::BackendUnavailable.into());
        }
        if self.retiring_prefixes.len() >= MAX_PENDING_NATIVE_SUBMISSIONS {
            return Err(WgpuRenderError::UploadDeferred {
                message: "the Host's original native submission custody is at capacity".to_owned(),
            }
            .into());
        }
        self.retiring_prefixes.try_reserve_exact(1).map_err(|_| {
            WgpuRenderError::UploadDeferred {
                message: "the Host's original prefix observation could not reserve storage".into(),
            }
        })?;
        Ok(match window {
            Some(window) => self.backend.prepare_editor_window(
                handles.ok_or(WgpuRenderError::SurfaceMissing {
                    window_id: window.id,
                })?,
                window,
            )?,
            None => self.backend.prepare_image_device()?,
        })
    }

    fn maintain_producers(&mut self) -> Result<(), WgpuRenderHostError> {
        for view in self.views.values_mut() {
            view.policies.maintain();
        }
        self.feature_states.maintain(self.backend.device_epoch());
        let closed = self
            .views
            .iter()
            .filter_map(|(id, view)| view.closed.load(Ordering::Acquire).then_some(*id))
            .collect::<Vec<_>>();
        for id in closed {
            if let Some(mut view) = self.views.remove(&id) {
                let _ = view.submitted_prefix.take();
            }
            self.backend.retire_editor_world_presentation(id);
        }
        for prefix in &self.retiring_prefixes {
            let result = if prefix.callback_abandoned() {
                prefix.recover_abandoned()
            } else {
                prefix.poll()
            };
            match result {
                Ok(_) => {}
                Err(crate::submission::WgpuSubmissionPrefixError::CallbackAbandoned) => {
                    prefix.recover_abandoned().map_err(|error| {
                        WgpuRenderError::RetirementPollFailed {
                            message: error.to_string(),
                        }
                    })?;
                }
                Err(error) => {
                    return Err(WgpuRenderError::RetirementPollFailed {
                        message: error.to_string(),
                    }
                    .into());
                }
            }
        }
        self.abandoned_submission_count = self.abandoned_submission_count.saturating_add(
            self.retiring_prefixes
                .iter()
                .filter(|prefix| prefix.is_complete() && prefix.callback_abandoned())
                .count() as u64,
        );
        self.retiring_prefixes
            .retain(|prefix| !prefix.is_complete());
        Ok(())
    }

    /// Counts resource callbacks recovered after abandonment; it never authorizes output success.
    #[must_use]
    pub const fn abandoned_submission_count(&self) -> u64 {
        self.abandoned_submission_count
    }

    /// Begins an independently paced image-only transaction using the normal device owner.
    pub fn begin_frame(&mut self, host_frame_index: u64) -> Result<(), WgpuRenderHostError> {
        self.prepare_namespace(None)?;
        self.backend.begin_frame_transaction(host_frame_index, None);
        #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
        self.backend.begin_editor_submitter_frame(host_frame_index);
        self.host_frame_index = Some(host_frame_index);
        Ok(())
    }

    pub fn reserve_image_target(
        &mut self,
        producer: &WgpuRenderProducer,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> Result<WgpuImageTargetReservation, WgpuRenderHostError> {
        let view_id = self.producer_id(producer)?;
        let namespace = self.prepare_namespace(None)?;
        self.reserve_viewport_target_in_namespace(view_id, &namespace, source_generation, target)
    }

    /// Realizes pinned CPU facts. Failure leaves the successful producer watermark unchanged.
    pub fn submit(
        &mut self,
        producer: &WgpuRenderProducer,
        input: &crate::WgpuRenderInput,
    ) -> Result<WgpuImageOutput, WgpuRenderHostError> {
        let view_id = self.producer_id(producer)?;
        self.admit_policy_requirements(input.requirements())?;
        let namespace = self.prepare_namespace(None)?;
        self.submit_image_in_namespace(view_id, &namespace, input, None)
    }

    /// Admits a policy's original request before any frame/target operation acquires a device.
    /// A late request must fit the enabled device or explicitly require reconstruction.
    pub fn prepare_policy<P: crate::WgpuRenderPolicy>(
        &mut self,
        producer: &WgpuRenderProducer,
        definition: &crate::WgpuPolicyDefinition<P>,
    ) -> Result<(), WgpuRenderHostError> {
        self.producer_id(producer)?;
        self.admit_policy_requirements(definition.device_requirements())
    }

    fn admit_policy_requirements(
        &mut self,
        requirements: &crate::WgpuDeviceRequirements,
    ) -> Result<(), WgpuRenderHostError> {
        // A lost original device is an execution failure, not a healthy device's
        // refusal to broaden its enabled request. Preserve that structured cause.
        self.backend.check_device_health()?;
        self.backend
            .request_device_requirements(requirements)
            .map_err(|error| match error {
                crate::WgpuDeviceRequirementsError::BackendUnavailable => {
                    WgpuRenderHostError::Backend(WgpuRenderError::BackendUnavailable)
                }
                error => WgpuRenderHostError::FeatureRequirements(error),
            })
    }

    fn build_policy(
        &mut self,
        id: WgpuRenderProducerId,
        input: &crate::WgpuRenderInput,
        format: wgpu::TextureFormat,
    ) -> Result<BuiltPolicy, WgpuRenderHostError> {
        let view = self
            .views
            .get_mut(&id)
            .ok_or(WgpuRenderHostError::ProducerClosed)?;
        Ok(view
            .policies
            .build(input, self.backend.instance_id(), id, &view.closed, format)?)
    }

    /// Installs an exact capture/native pair for one producer-owned instance. Requirements join
    /// the device request before acquisition; late requirements cannot silently replace it.
    pub fn create_feature_instance<F: crate::WgpuRenderFeature>(
        &mut self,
        producer: &WgpuRenderProducer,
        definition: &crate::WgpuFeatureDefinition<F>,
    ) -> Result<crate::WgpuFeatureInstance<F>, WgpuRenderHostError> {
        self.maintain_producers()?;
        let id = self.producer_id(producer)?;
        let instance = crate::WgpuFeatureInstance::new(
            self.backend.instance_id(),
            id,
            Arc::clone(&producer.closed),
            definition,
        )?;
        self.admit_policy_requirements(definition.device_requirements())?;
        Ok(instance)
    }

    /// Executes a finite texture/buffer program through this Host's original device, queue,
    /// producer admission and submission custody. Failure publishes none of its output group.
    pub fn submit_program(
        &mut self,
        producer: &WgpuRenderProducer,
        program: &crate::WgpuRenderProgram,
    ) -> Result<crate::WgpuProgramOutputs, WgpuRenderHostError> {
        let id = self.producer_id(producer)?;
        self.validate_program_instances(id, program)?;
        self.host_frame_index
            .ok_or(WgpuRenderHostError::FrameTransactionUnavailable)?;
        self.views[&id]
            .admission
            .admit(program.source_generation(), program.frame_index())?;
        let namespace = self.prepare_namespace(None)?;
        self.execute_program_in_namespace(id, &namespace, program, None, None, None)
    }

    /// Output publication and stock image publication use the same original native transaction.
    fn execute_program_in_namespace(
        &mut self,
        id: WgpuRenderProducerId,
        namespace: &WgpuDeviceNamespace,
        program: &crate::WgpuRenderProgram,
        local_target: Option<&crate::program::HostProgramTexture>,
        mut surface: Option<ProgramSurfaceTarget<'_>>,
        policy: Option<(
            &crate::WgpuPolicyProgram,
            Arc<PolicyExecutionCustody>,
            usize,
        )>,
    ) -> Result<crate::WgpuProgramOutputs, WgpuRenderHostError> {
        let host_frame_index = self
            .host_frame_index
            .ok_or(WgpuRenderHostError::FrameTransactionUnavailable)?;
        let mut outputs = Vec::new();
        outputs
            .try_reserve_exact(program.0.graph.outputs().len())
            .map_err(|_| crate::GraphError::AllocationFailed)
            .map_err(crate::WgpuRenderProgramError::from)?;
        if let Some(surface) = &surface {
            let resource = program
                .0
                .graph
                .resource(surface.final_version)
                .map_err(crate::WgpuRenderProgramError::from)?;
            let crate::graph::ResourceDescriptor::Texture {
                native, meaning, ..
            } = &resource.desc
            else {
                return Err(crate::WgpuRenderProgramError::ResourceKind.into());
            };
            if !program.0.graph.outputs().contains(surface.final_version)
                || native.format != surface.acquired.format
                || native.size.width != surface.acquired.extent.width
                || native.size.height != surface.acquired.extent.height
                || !native.usage.contains(wgpu::TextureUsages::TEXTURE_BINDING)
                || (meaning.transfer == crate::TextureTransfer::Srgb) != native.format.is_srgb()
            {
                return Err(crate::WgpuRenderProgramError::feature(
                    "the final program image does not match its acquired surface",
                )
                .into());
            }
        }

        let policy_metadata = policy.as_ref().map_or(0, |(_, _, bytes)| *bytes);
        #[cfg(feature = "stock-ui")]
        let ui_binding = policy
            .as_ref()
            .and_then(|(output, _, _)| output.ui.as_ref());
        let finalization_cost = if surface.is_some() {
            crate::program::NativePreparationCost::new(
                1,
                policy_metadata
                    .checked_add(
                        crate::presentation::SurfacePresentationCache::modeled_metadata_bytes()
                            + std::mem::size_of::<ProgramSurfaceTarget<'_>>(),
                    )
                    .ok_or(crate::WgpuRenderProgramError::PolicyCapacity)?,
            )
        } else {
            crate::program::NativePreparationCost::new(0, policy_metadata)
        };
        let validation = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let internal = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::Internal);
        let allocation = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let mut reservation = match policy {
            Some((_, owner, _)) => WgpuSubmissionReservation::new(
                &namespace.device,
                &namespace.queue,
                (program.clone(), owner),
            ),
            None => {
                WgpuSubmissionReservation::new(&namespace.device, &namespace.queue, program.clone())
            }
        };
        let mut feature_attempt = crate::program::FeaturePreparationAttempt::default();
        let presentation = &mut self.presentation;
        let prepared = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            crate::program::prepare_and_encode(
                program,
                namespace,
                id,
                &mut self.feature_states,
                &self.output_budget,
                &mut reservation,
                &mut feature_attempt,
                host_frame_index,
                #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
                self.backend.stock_quad_resources(),
                local_target,
                finalization_cost,
                |resources, encoder, preparation| {
                    if let Some(surface) = &surface {
                        let storage = resources
                            .get(surface.final_version.index())
                            .ok_or(crate::WgpuRenderProgramError::UnpublishedResource)?;
                        let crate::program::NativeResource::Texture { texture, view } =
                            &storage.native
                        else {
                            return Err(crate::WgpuRenderProgramError::ResourceKind);
                        };
                        let prepared = presentation.prepare(
                            &namespace.device,
                            namespace.device_epoch,
                            surface.acquired.format,
                            view,
                            Extent2d::new(texture.width(), texture.height())
                                .expect("admitted texture extent"),
                            surface.acquired.extent,
                        )?;
                        let target = crate::allocation::checked_resource(
                            &namespace.device,
                            "nara_program_surface_view",
                            || {
                                surface.acquired.texture.texture.create_view(
                                    &wgpu::TextureViewDescriptor {
                                        format: Some(surface.acquired.format),
                                        ..Default::default()
                                    },
                                )
                            },
                        )?;
                        prepared.encode(encoder, &target);
                        preparation.retain(prepared)?;
                    }
                    Ok(())
                },
            )
        }))
        .unwrap_or(Err(crate::WgpuRenderProgramError::FeaturePanicked));
        if let Some(surface) = &mut surface {
            let texture_bytes =
                self.backend
                    .logical_texture_bytes()
                    .saturating_add(if prepared.is_ok() {
                        program.modeled_resource_bytes()
                    } else {
                        0
                    });
            *surface.submitted_texture_bytes = texture_bytes;
            self.backend.observe_program_resources(
                texture_bytes,
                if prepared.is_ok() {
                    surface.instance_buffer_bytes
                } else {
                    0
                },
            );
        }

        #[cfg(test)]
        PROGRAM_SUBMISSION_FAILURE.with(|failure| {
            if failure.replace(false) {
                if let Ok(encoded) = &prepared {
                    if let Some(local) = program
                        .0
                        .graph
                        .resources()
                        .iter()
                        .find(|resource| !resource.is_imported())
                    {
                        match &encoded.resources[local.version.index()].native {
                            crate::program::NativeResource::Texture { texture, .. } => {
                                texture.destroy()
                            }
                            crate::program::NativeResource::Buffer(buffer) => buffer.destroy(),
                        }
                    }
                }
            }
        });
        #[cfg(test)]
        IMAGE_SUBMISSION_FAILURE.with(|failure| {
            if failure.replace(false) {
                if let (Ok(encoded), Some(version)) = (&prepared, program.0.graph.outputs().first())
                {
                    if let crate::program::NativeResource::Texture { texture, .. } =
                        &encoded.resources[version.index()].native
                    {
                        texture.destroy();
                    }
                }
            }
        });

        // Even failed preparation may have enqueued uploads. Bind this attempt's original
        // prefix before another producer can write, including a native submit that unwinds.
        let (commands, staged) = match prepared {
            Ok(encoded) => {
                feature_attempt.commit_instance_reservations();
                (
                    Some(encoded.commands),
                    Ok((encoded.resources, encoded.reports)),
                )
            }
            Err(error) => (None, Err(error)),
        };
        let submitted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            namespace.queue.submit(commands)
        }));
        let (prefix, submit_error) = match submitted {
            Ok(index) => (reservation.submitted(index), None),
            Err(_) => (
                reservation.bind_current_prefix(),
                Some(crate::WgpuRenderProgramError::FeaturePanicked),
            ),
        };
        self.retiring_prefixes.push(prefix.clone());
        self.views
            .get_mut(&id)
            .expect("the producer is retained during this submission")
            .submitted_prefix = Some(prefix.clone());

        let allocation_error = pollster::block_on(allocation.pop());
        let internal_error = pollster::block_on(internal.pop());
        let validation_error = pollster::block_on(validation.pop());
        let result = (|| {
            self.backend.check_device_health()?;
            self.validate_program_instances(id, program)?;
            if let Some(error) = allocation_error.or(internal_error).or(validation_error) {
                return Err(Self::native_error(error));
            }
            if let Some(error) = submit_error {
                return Err(error.into());
            }
            let (resources, reports) = staged?;
            for version in program.0.graph.outputs() {
                outputs.push((version.clone(), Arc::clone(&resources[version.index()])));
            }
            let outputs = crate::WgpuProgramOutputs {
                outputs,
                reports,
                prefix,
            };
            // Actual UI encoding must match the original final-image binding before this
            // group consumes publication admission. Completion storage was admitted earlier.
            #[cfg(feature = "stock-ui")]
            if let Some(binding) = ui_binding {
                binding.submitted_facts(&outputs)?;
            }
            if let Some(surface) = &surface {
                self.backend.track_runtime_submission(
                    surface.app_frame_index,
                    program.frame_index(),
                    surface.work_started_at,
                )?;
            }
            let admission = &mut self
                .views
                .get_mut(&id)
                .expect("admitted producer")
                .admission;
            admission.source_generation = Some(program.source_generation());
            admission.last_frame_index = Some(program.frame_index());
            if surface.is_some() {
                self.backend.record_runtime_program_submission();
            } else {
                self.backend.record_editor_image_submission();
            }
            Ok(outputs)
        })();
        if surface.is_some() {
            self.backend.finish_program_resources();
        }
        if local_target.is_some() && feature_attempt.native_started {
            // A pure preflight/capacity refusal preserves separately admitted initial storage.
            // Once native work starts, the original prefix owns the attempt, including failure.
            if let Some(target) = self
                .views
                .get_mut(&id)
                .and_then(|view| view.target.as_mut())
            {
                target.initial_storage.take();
            }
        }
        if result.is_err() {
            self.feature_states
                .discard(feature_attempt.touched_instances.iter().copied());
        }
        result
    }

    fn validate_program_instances(
        &self,
        producer: WgpuRenderProducerId,
        program: &crate::WgpuRenderProgram,
    ) -> Result<(), WgpuRenderHostError> {
        for operation in &program.0.operations {
            if let Some(capture) = &operation.capture {
                let identity = capture.identity();
                if identity.is_closed()
                    || identity.backend_instance_id != self.backend.instance_id()
                    || identity.producer != producer
                {
                    return Err(crate::WgpuRenderProgramError::FeatureClosed.into());
                }
            }
        }
        Ok(())
    }

    /// Native interoperation borrows the exact retained output, never its producer's latest target.
    ///
    /// This is a trusted WGPU adapter seam: native handle clones must not write or destroy the
    /// imported immutable texture. Retain the output lease for every imported native handle and
    /// its submitted use; a native clone alone cannot preserve the Host's logical byte accounting.
    /// The adapter owns any extra submission's completion. Ordinary composition retains
    /// [`WgpuImageDisplay`] instead.
    pub fn with_image<R>(
        &mut self,
        output: &WgpuImageOutput,
        operation: impl FnOnce(&wgpu::Device, &wgpu::Queue, &wgpu::TextureView) -> R,
    ) -> Result<R, WgpuRenderHostError> {
        let namespace = self.prepare_namespace(None)?;
        self.validate_viewport_submission(&namespace, output)?;
        Ok(operation(
            &namespace.device,
            &namespace.queue,
            &output.0.view,
        ))
    }

    /// Encodes a native read-only use of exact outputs with Host-owned submission custody.
    ///
    /// The callback may enqueue uploads and encode commands, but must not submit independently,
    /// mutate imported outputs, or retain their native handles beyond the output leases. The Host
    /// retains all imported output leases through this exact submitted prefix, including a failed
    /// callback's queued uploads. The returned result does not imply physical GPU completion.
    pub fn submit_image_use<R>(
        &mut self,
        outputs: &[WgpuImageOutput],
        operation: impl FnOnce(
            &wgpu::Device,
            &wgpu::Queue,
            &mut wgpu::CommandEncoder,
            &[&wgpu::TextureView],
        ) -> Result<R, WgpuRenderHostError>,
    ) -> Result<(R, WgpuSubmissionPrefix), WgpuRenderHostError> {
        let namespace = self.prepare_namespace(None)?;
        for output in outputs {
            self.validate_viewport_submission(&namespace, output)?;
        }
        let retention =
            WgpuSubmissionReservation::new(&namespace.device, &namespace.queue, outputs.to_vec());
        let validation = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let internal = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::Internal);
        let allocation = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let mut encoder =
            namespace
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("nara_image_use"),
                });
        let views = outputs
            .iter()
            .map(|output| &output.0.view)
            .collect::<Vec<_>>();
        let result = operation(&namespace.device, &namespace.queue, &mut encoder, &views);
        let prefix = if result.is_ok() {
            retention.submitted(namespace.queue.submit([encoder.finish()]))
        } else {
            retention.submitted(namespace.queue.submit([]))
        };
        self.native_prefix = Some(prefix.clone());
        self.retiring_prefixes.push(prefix.clone());
        let allocation_error = pollster::block_on(allocation.pop());
        let internal_error = pollster::block_on(internal.pop());
        let validation_error = pollster::block_on(validation.pop());
        self.backend.check_device_health()?;
        if let Some(error) = allocation_error.or(internal_error).or(validation_error) {
            return Err(Self::native_error(error));
        }
        Ok((result?, prefix))
    }

    /// Stops this Host and polls its final original queue drain without waiting for later work.
    /// Producer-local close uses the frozen prefix returned by [`Self::retire_producer`] instead.
    pub fn poll_retirement(&mut self) -> Result<bool, WgpuRenderHostError> {
        self.poll_retirement_for_targets(&[])
    }

    fn poll_retirement_for_targets(
        &mut self,
        window_ids: &[WindowId],
    ) -> Result<bool, WgpuRenderHostError> {
        if self.retired {
            return Ok(true);
        }
        for view in self.views.values_mut() {
            view.closed.store(true, Ordering::Release);
            view.interaction.store(false, Ordering::Release);
            view.world_presentation.invalidate();
        }
        self.presentation.clear();
        self.maintain_producers()?;
        // Platform surfaces are retired by their retained native session, after this queue drain.
        let queue_complete = self.backend.poll_runtime_retirement(window_ids)?;
        self.maintain_producers()?;
        self.retired = queue_complete && self.retiring_prefixes.is_empty();
        Ok(self.retired)
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
    ) -> Result<(), WgpuRenderHostError> {
        self.prepare_namespace(Some(window))?;
        self.backend.begin_frame_transaction(host_frame_index, None);
        #[cfg(any(feature = "stock-sprite", feature = "stock-ui"))]
        self.backend.begin_editor_submitter_frame(host_frame_index);
        self.host_frame_index = Some(host_frame_index);
        Ok(())
    }

    pub fn with_device<R>(
        &mut self,
        window: RenderWindowPacket,
        operation: impl for<'frame> FnOnce(WgpuEditorDeviceScope<'frame>) -> R,
    ) -> Result<R, WgpuRenderHostError> {
        let namespace = self.prepare_namespace(Some(window))?;
        let (surface_format, surface_extent) =
            namespace.surface.ok_or(WgpuRenderError::BackendNotReady)?;
        Ok(operation(WgpuEditorDeviceScope {
            instance: &namespace.instance,
            adapter: &namespace.adapter,
            device: &namespace.device,
            queue: &namespace.queue,
            surface_format,
            surface_extent,
            backend_instance_id: namespace.backend_instance_id,
            device_epoch: namespace.device_epoch,
        }))
    }

    pub fn reserve_viewport_target(
        &mut self,
        producer: &WgpuRenderProducer,
        window: RenderWindowPacket,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> Result<WgpuImageTargetReservation, WgpuRenderHostError> {
        let view_id = self.producer_id(producer)?;
        let namespace = self.prepare_namespace(Some(window))?;
        self.reserve_viewport_target_in_namespace(view_id, &namespace, source_generation, target)
    }

    pub fn activate_image_target(
        &mut self,
        producer: &WgpuRenderProducer,
        reservation: WgpuImageTargetReservation,
    ) -> Result<(), WgpuRenderHostError> {
        let activation = self.prepare_image_target_activation(producer, reservation)?;
        activation.commit();
        Ok(())
    }

    pub fn prepare_image_target_activation<'host>(
        &'host mut self,
        producer: &'host WgpuRenderProducer,
        reservation: WgpuImageTargetReservation,
    ) -> Result<WgpuImageTargetActivation<'host>, WgpuRenderHostError> {
        if self.producer_id(producer)? != reservation.stamp.view_id {
            return Err(WgpuRenderHostError::ProducerClosed);
        }
        self.validate_viewport_reservation(
            &reservation,
            self.backend.instance_id(),
            self.backend.device_epoch(),
        )?;
        Ok(WgpuImageTargetActivation {
            host: self,
            _producer: producer,
            reservation,
        })
    }

    fn commit_image_target_activation(&mut self, reservation: WgpuImageTargetReservation) {
        debug_assert!(
            self.validate_viewport_reservation(
                &reservation,
                self.backend.instance_id(),
                self.backend.device_epoch(),
            )
            .is_ok()
        );
        let view = self.views.entry(reservation.stamp.view_id).or_default();
        view.interaction.store(false, Ordering::Release);
        view.interaction = Arc::new(AtomicBool::new(true));
        if view.admission.source_generation != Some(reservation.stamp.source_generation) {
            view.world_presentation.invalidate();
            view.runtime_presentation = None;
            self.backend
                .retire_editor_world_presentation(reservation.stamp.view_id);
        }
        view.admission.activate(reservation.stamp);
        view.target = Some(ImageTarget {
            stamp: reservation.stamp,
            initial_storage: Some((
                reservation.texture,
                reservation.view,
                Arc::new(reservation.storage_reservation),
            )),
        });
    }

    #[must_use]
    pub fn image_target_matches(
        &self,
        producer: &WgpuRenderProducer,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> bool {
        let Ok(view_id) = self.producer_id(producer) else {
            return false;
        };
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

    /// Attaches invalidate-only Runtime camera state to an existing Editor view's GPU lifetime.
    /// This grants no image submission, composite or presentation authority.
    pub fn retain_game_world_presentation(
        &mut self,
        producer: &WgpuRenderProducer,
        window: RenderWindowPacket,
        revoker: nara_render::__private::WorldPresentationRevoker,
    ) -> Result<(), WgpuRenderHostError> {
        let view_id = match self.producer_id(producer) {
            Ok(id) => id,
            Err(error) => {
                revoker.invalidate();
                return Err(error);
            }
        };
        if self
            .views
            .get(&view_id)
            .is_none_or(|view| view.target.is_none())
        {
            revoker.invalidate();
            return Err(WgpuRenderHostError::ViewportTargetUnavailable);
        }
        if let Err(error) = self.prepare_namespace(Some(window)) {
            revoker.invalidate();
            return Err(error);
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
        view.world_presentation.replace(&view.closed, revoker)?;
        view.runtime_presentation = None;
        Ok(())
    }

    #[cfg(feature = "stock-ui")]
    pub fn submit_for_display(
        &mut self,
        producer: &WgpuRenderProducer,
        input: &crate::WgpuRenderInput,
        mut ui: WgpuUiSubmission,
    ) -> Result<WgpuUiImageSubmission, WgpuUiSubmitFailure> {
        let view_id = match self.producer_id(producer) {
            Ok(id) => id,
            Err(error) => return Err(WgpuUiSubmitFailure { error, ui }),
        };
        let submitted = (|| {
            self.admit_policy_requirements(input.requirements())?;
            let namespace = self.prepare_namespace(None)?;
            self.submit_image_in_namespace(view_id, &namespace, input, Some(&mut ui))
        })();
        match submitted {
            Ok(image) => Ok(WgpuUiImageSubmission {
                display: WgpuImageDisplay::issue(image),
                interaction: Arc::clone(&self.views[&view_id].interaction),
                producer_closed: Arc::clone(&producer.closed),
                ui,
            }),
            Err(error) => Err(WgpuUiSubmitFailure { error, ui }),
        }
    }

    fn submit_image_in_namespace(
        &mut self,
        view_id: WgpuRenderProducerId,
        namespace: &WgpuDeviceNamespace,
        input: &crate::WgpuRenderInput,
        #[cfg(feature = "stock-ui")] mut ui: Option<&mut WgpuUiSubmission>,
        #[cfg(not(feature = "stock-ui"))] _ui: Option<&mut ()>,
    ) -> Result<WgpuImageOutput, WgpuRenderHostError> {
        let packet = input.packet();
        let RenderFrameTarget::Image(target) = packet.target() else {
            return Err(WgpuRenderHostError::ExpectedImageTarget);
        };
        self.host_frame_index
            .ok_or(WgpuRenderHostError::FrameTransactionUnavailable)?;
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
        #[cfg(feature = "stock-ui")]
        let ui_backend_generation = if let Some(ui) = ui.as_deref_mut() {
            let generation = UiPresentationBackendGeneration::new(
                namespace.backend_instance_id,
                namespace.device_epoch,
            )
            .ok_or_else(|| {
                WgpuRenderHostError::Backend(WgpuRenderError::DeviceUnavailable {
                    message: "the Editor GPU namespace has no non-zero UI presentation generation"
                        .to_owned(),
                })
            })?;
            ui.validate_backend_generation(generation)?;
            Some(generation)
        } else {
            None
        };
        #[cfg(feature = "stock-ui")]
        if let (Some(ui), Some(backend_generation)) = (ui.as_deref_mut(), ui_backend_generation) {
            ui.bind_backend(backend_generation)?;
        }
        let built = self.build_policy(view_id, input, EDITOR_IMAGE_FORMAT)?;
        let policy = &built.output;
        Self::validate_policy_target(policy, packet, EDITOR_IMAGE_FORMAT, true)?;
        #[cfg(feature = "stock-ui")]
        if let Some(ui) = ui.as_deref() {
            let facts = policy
                .ui
                .as_ref()
                .map_or(&[][..], |binding| binding.facts());
            crate::ui::validate_submission_packets(ui.state.bound_submissions(), facts)?;
        }
        self.validate_program_instances(view_id, &policy.program)?;
        let stamp = self.views[&view_id]
            .target
            .as_ref()
            .expect("admitted target")
            .stamp;
        let initial_storage = self
            .views
            .get(&view_id)
            .and_then(|view| view.target.as_ref())
            .and_then(|target| target.initial_storage.as_ref())
            .map(|(texture, view, reservation)| {
                (texture.clone(), view.clone(), Arc::clone(reservation))
            });
        let descriptor = policy
            .program
            .0
            .graph
            .resource(&policy.final_version)
            .map_err(crate::WgpuRenderProgramError::from)?
            .desc
            .clone();
        // Reserved target storage realizes a normal local output. It has no initialized-import
        // authority; the graph still requires its writer and complete initialization.
        let local_target = initial_storage.map(|(texture, view, reservation)| {
            crate::program::HostProgramTexture {
                version: policy.final_version.clone(),
                storage: Arc::new(crate::program::ProgramResourceStorage {
                    backend_instance_id: namespace.backend_instance_id,
                    device_epoch: namespace.device_epoch,
                    epoch_lost: Arc::clone(&namespace.epoch_lost),
                    producer: view_id,
                    source: packet.source_generation(),
                    frame_index: packet.frame_index(),
                    descriptor,
                    native: crate::program::NativeResource::Texture { texture, view },
                    _reservation: reservation,
                }),
            }
        });
        // Only the separately admitted target reservation can precede this candidate's
        // preflight. Successors let the program allocate its final version after that barrier.
        let outputs = self.execute_program_in_namespace(
            view_id,
            namespace,
            &policy.program,
            local_target.as_ref(),
            None,
            Some((policy, built.custody, built.metadata_bytes)),
        )?;
        let storage = outputs
            .outputs
            .iter()
            .find(|(version, _)| *version == policy.final_version)
            .map(|(_, storage)| storage)
            .expect("the complete stock output group contains its final version");
        let crate::program::NativeResource::Texture { texture, view } = &storage.native else {
            unreachable!("the stock policy declares a texture output")
        };
        Ok(WgpuImageOutput(Arc::new(WgpuImageStorage {
            epoch_lost: Arc::clone(&namespace.epoch_lost),
            receipt: WgpuSubmittedImageReceipt::new(
                view_id,
                target,
                stamp.generation,
                stamp.source_generation,
                stamp.source_handoff_generation,
                namespace.backend_instance_id,
                namespace.device_epoch,
                packet.frame_index(),
            ),
            texture: texture.clone(),
            view: view.clone(),
            _reservation: Arc::clone(&storage._reservation),
        })))
    }

    fn validate_policy_target(
        policy: &crate::WgpuPolicyProgram,
        packet: &RenderFramePacket,
        format: wgpu::TextureFormat,
        readback: bool,
    ) -> Result<(), WgpuRenderHostError> {
        let resource = policy
            .program
            .0
            .graph
            .resource(&policy.final_version)
            .map_err(crate::WgpuRenderProgramError::from)?;
        let crate::graph::ResourceDescriptor::Texture {
            native, meaning, ..
        } = &resource.desc
        else {
            return Err(crate::WgpuRenderProgramError::ResourceKind.into());
        };
        let extent = packet.target().extent();
        let mut usage = wgpu::TextureUsages::TEXTURE_BINDING;
        if readback {
            usage |= wgpu::TextureUsages::COPY_SRC;
        }
        if native.format != format
            || native.size.width != extent.width
            || native.size.height != extent.height
            || !native.usage.contains(usage)
            || (meaning.transfer == crate::TextureTransfer::Srgb) != format.is_srgb()
            || !matches!(
                meaning.alpha,
                crate::TextureAlpha::Premultiplied | crate::TextureAlpha::Opaque
            )
        {
            return Err(crate::WgpuRenderProgramError::feature(
                "the policy final image does not match the physical target, transfer or alpha contract",
            ).into());
        }
        Ok(())
    }

    fn native_error(error: wgpu::Error) -> WgpuRenderHostError {
        match error {
            wgpu::Error::OutOfMemory { .. } => WgpuRenderError::UploadDeferred {
                message: error.to_string(),
            }
            .into(),
            wgpu::Error::Internal { .. } => WgpuRenderHostError::NativeExecution {
                message: error.to_string(),
            },
            wgpu::Error::Validation { .. } => WgpuRenderError::ContentRejected {
                message: error.to_string(),
            }
            .into(),
        }
    }

    /// Selects exact pixels for a new placement. A mirror receives a distinct occurrence.
    pub fn display_image(
        &self,
        output: WgpuImageOutput,
    ) -> Result<WgpuImageDisplay, WgpuRenderHostError> {
        self.backend.check_device_health()?;
        if !output.is_current()
            || output.receipt().backend_instance_id() != self.backend.instance_id()
            || output.receipt().device_epoch() != self.backend.device_epoch()
        {
            return Err(WgpuRenderHostError::StaleImageSubmission);
        }
        Ok(WgpuImageDisplay::issue(output))
    }

    pub fn present_window(
        &mut self,
        window: RenderWindowPacket,
        editor_frame_index: u64,
        viewport_submissions: &[WgpuImageDisplay],
        paint: impl for<'frame> FnOnce(
            &mut WgpuEditorWindowFrame<'frame>,
        ) -> Result<(), WgpuRenderHostError>,
        pre_present: impl FnOnce(),
    ) -> Result<WgpuEditorFrameOutcome, WgpuRenderHostError> {
        self.host_frame_index
            .ok_or(WgpuRenderHostError::FrameTransactionUnavailable)?;
        let namespace = self.prepare_namespace(Some(window))?;
        for display in viewport_submissions {
            self.validate_viewport_submission(&namespace, display.output())?;
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
        let validation = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        let internal = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::Internal);
        let allocation = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
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
        let retention = WgpuSubmissionReservation::new(
            &namespace.device,
            &namespace.queue,
            viewport_submissions.to_vec(),
        );
        let paint_result = paint(&mut WgpuEditorWindowFrame {
            device: &namespace.device,
            queue: &namespace.queue,
            encoder: &mut encoder,
            target_view: &target_view,
            surface_format: format,
            surface_extent: extent,
            viewports: viewport_submissions,
            leading_command_buffers: &mut leading_command_buffers,
            sampled_viewports: &mut sampled_viewports,
        });
        let paint_result = paint_result.and_then(|()| {
            self.last_presentation_generation
                .checked_add(sampled_viewports.len() as u64)
                .ok_or(WgpuRenderHostError::PresentationGenerationExhausted)
                .map(|_| ())
        });
        let prefix = if paint_result.is_ok() {
            let native_commands = leading_command_buffers
                .into_iter()
                .chain([encoder.finish()]);
            retention.submitted(namespace.queue.submit(native_commands))
        } else {
            retention.submitted(namespace.queue.submit([]))
        };
        self.retiring_prefixes.push(prefix.clone());
        self.native_prefix = Some(prefix);
        let allocation_error = pollster::block_on(allocation.pop());
        let internal_error = pollster::block_on(internal.pop());
        let validation_error = pollster::block_on(validation.pop());
        self.backend.check_device_health()?;
        if let Some(error) = allocation_error.or(internal_error).or(validation_error) {
            return Err(Self::native_error(error));
        }
        paint_result?;
        pre_present();
        let presentation_validation = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::Validation);
        namespace.queue.present(texture);
        let presentation_error = pollster::block_on(presentation_validation.pop());
        self.backend.check_device_health()?;
        if let Some(error) = presentation_error {
            return Err(Self::native_error(error));
        }
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

    pub fn retire_image_target(&mut self, producer: &WgpuRenderProducer) {
        let Ok(view_id) = self.producer_id(producer) else {
            return;
        };
        if let Some(view) = self.views.get_mut(&view_id) {
            view.interaction.store(false, Ordering::Release);
            view.target = None;
            view.world_presentation.invalidate();
            view.runtime_presentation = None;
        }
        self.backend.retire_editor_world_presentation(view_id);
    }

    /// Flushes pending uploads and drains work at the explicit native Host shutdown boundary.
    pub fn wait_for_idle(&self) -> Result<(), WgpuRenderHostError> {
        self.backend
            .wait_for_editor_idle(std::time::Duration::from_secs(5))?;
        Ok(())
    }

    /// Releases a final view lifetime. The Host never reuses its identity after this call.
    pub fn retire_producer(
        &mut self,
        producer: &WgpuRenderProducer,
    ) -> Option<WgpuSubmissionPrefix> {
        if producer.backend_instance_id != self.backend.instance_id() {
            return None;
        }
        producer.closed.store(true, Ordering::Release);
        let view_id = producer.id;
        let prefix = self
            .views
            .remove(&view_id)
            .and_then(|mut view| view.submitted_prefix.take());
        self.backend.retire_editor_world_presentation(view_id);
        prefix
    }

    fn validate_viewport_submission(
        &self,
        namespace: &WgpuDeviceNamespace,
        output: &WgpuImageOutput,
    ) -> Result<(), WgpuRenderHostError> {
        if !output.is_current()
            || output.receipt().backend_instance_id() != namespace.backend_instance_id
            || output.receipt().device_epoch() != namespace.device_epoch
        {
            return Err(WgpuRenderHostError::StaleImageSubmission);
        }
        Ok(())
    }

    fn require_viewport_target(
        &self,
        view_id: WgpuRenderProducerId,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> Result<(), WgpuRenderHostError> {
        if self
            .views
            .get(&view_id)
            .filter(|view| !view.closed.load(Ordering::Acquire))
            .and_then(|view| view.target.as_ref())
            .is_some_and(|active| {
                active.stamp.source_generation == source_generation
                    && active.stamp.target == target
                    && active.stamp.backend_instance_id == self.backend.instance_id()
                    && active.stamp.device_epoch == self.backend.device_epoch()
            })
        {
            return Ok(());
        }
        if self
            .views
            .get(&view_id)
            .is_some_and(|view| view.target.is_some())
        {
            return Err(WgpuRenderHostError::ViewportTargetAlreadyActive);
        }
        Err(WgpuRenderHostError::ViewportTargetUnavailable)
    }

    fn reserve_viewport_target_in_namespace(
        &self,
        view_id: WgpuRenderProducerId,
        namespace: &WgpuDeviceNamespace,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> Result<WgpuImageTargetReservation, WgpuRenderHostError> {
        let stamp =
            self.next_viewport_reservation_stamp(view_id, namespace, source_generation, target)?;
        let (texture, view, storage_reservation) =
            self.allocate_output_storage(namespace, target)?;
        Ok(WgpuImageTargetReservation {
            stamp,
            texture,
            view,
            storage_reservation,
        })
    }

    fn allocate_output_storage(
        &self,
        namespace: &WgpuDeviceNamespace,
        target: RenderImagePacket,
    ) -> Result<(wgpu::Texture, wgpu::TextureView, OutputReservation), WgpuRenderHostError> {
        let extent = target.extent();
        let maximum = namespace.device.limits().max_texture_dimension_2d;
        if extent.width > maximum || extent.height > maximum {
            return Err(WgpuRenderHostError::ViewportTargetExtentUnsupported {
                width: extent.width,
                height: extent.height,
                maximum,
            });
        }
        let bytes = u64::from(extent.width)
            .checked_mul(u64::from(extent.height))
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(WgpuRenderHostError::OutputCapacity)?;
        let reservation = self
            .output_budget
            .reserve(bytes)
            .ok_or(WgpuRenderHostError::OutputCapacity)?;
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
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC
                | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        // A failed texture is an invalid WGPU handle. Reject it before attempting a view,
        // otherwise the expected allocation failure also produces a validation failure.
        let allocation_result = finish_viewport_allocation(allocation_scope, extent);
        self.backend.check_device_health()?;
        allocation_result?;
        let allocation_scope = namespace
            .device
            .push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(EDITOR_IMAGE_FORMAT),
            ..Default::default()
        });
        let allocation_result = finish_viewport_allocation(allocation_scope, extent);
        self.backend.check_device_health()?;
        allocation_result?;
        Ok((texture, view, reservation))
    }

    fn next_viewport_reservation_stamp(
        &self,
        view_id: WgpuRenderProducerId,
        namespace: &WgpuDeviceNamespace,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> Result<ImageTargetStamp, WgpuRenderHostError> {
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
        reservation: &WgpuImageTargetReservation,
        backend_instance_id: u64,
        device_epoch: u64,
    ) -> Result<(), WgpuRenderHostError> {
        if self.backend.runtime_close_started()
            || self
                .views
                .get(&reservation.stamp.view_id)
                .is_none_or(|view| view.closed.load(Ordering::Acquire))
        {
            return Err(WgpuRenderHostError::ProducerClosed);
        }
        let expected = self.next_viewport_reservation_stamp_for_namespace(
            reservation.stamp.view_id,
            backend_instance_id,
            device_epoch,
            reservation.stamp.source_generation,
            reservation.stamp.target,
        )?;
        if reservation.stamp != expected {
            return Err(WgpuRenderHostError::StaleViewportReservation);
        }
        Ok(())
    }

    fn next_viewport_reservation_stamp_for_namespace(
        &self,
        view_id: WgpuRenderProducerId,
        backend_instance_id: u64,
        device_epoch: u64,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> Result<ImageTargetStamp, WgpuRenderHostError> {
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
mod program_tests;
#[cfg(test)]
mod tests;
