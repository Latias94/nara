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
    #[error("the Editor viewport reservation no longer matches the live source or GPU namespace")]
    StaleViewportReservation,
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
        matches!(self, Self::Backend(error) if error.is_deferred())
    }

    #[must_use]
    pub const fn is_content_rejected(&self) -> bool {
        matches!(self, Self::Backend(error) if error.is_content_rejection())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WgpuEditorViewportReservationStamp {
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
    pub fn matches_submission(self, receipt: WgpuSubmittedImageReceipt) -> bool {
        self.stamp.target == receipt.target()
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
}

impl WgpuEditorViewportTarget {
    fn matches_namespace(&self, namespace: &WgpuEditorDeviceNamespace) -> bool {
        self.stamp.backend_instance_id == namespace.backend_instance_id
            && self.stamp.device_epoch == namespace.device_epoch
    }

    fn matches_receipt(&self, receipt: WgpuSubmittedImageReceipt) -> bool {
        WgpuEditorViewportIdentity { stamp: self.stamp }.matches_submission(receipt)
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
            | WgpuEditorExecutionError::StaleViewportReservation => {
                UiPresentationBackendFaultDomain::Target
            }
            WgpuEditorExecutionError::ExpectedImageTarget
            | WgpuEditorExecutionError::TargetGenerationExhausted
            | WgpuEditorExecutionError::SourceHandoffGenerationExhausted
            | WgpuEditorExecutionError::PresentationGenerationExhausted
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
    device: &'frame wgpu::Device,
    queue: &'frame wgpu::Queue,
    surface_format: wgpu::TextureFormat,
    surface_extent: Extent2d,
    backend_instance_id: u64,
    device_epoch: u64,
}

impl<'frame> WgpuEditorDeviceScope<'frame> {
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
struct WgpuEditorViewportTexture {
    receipt: WgpuSubmittedImageReceipt,
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
    viewport: Option<WgpuEditorViewportTexture>,
    leading_command_buffers: &'frame mut Vec<wgpu::CommandBuffer>,
    sampled_viewport: &'frame mut Option<WgpuEditorViewportSample>,
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

    pub fn record_viewport_sample(&mut self) -> Result<(), WgpuEditorExecutionError> {
        let Some(viewport) = self.viewport else {
            return Err(WgpuEditorExecutionError::StaleImageSubmission);
        };
        *self.sampled_viewport = Some(WgpuEditorViewportSample {
            image: viewport.receipt,
        });
        Ok(())
    }
}

/// Terminal facts from one attempted Editor-window paint.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WgpuEditorFrameOutcome {
    window: Option<WgpuEditorWindowPresentedReceipt>,
    composite: Option<WgpuEditorCompositeReceipt>,
}

impl WgpuEditorFrameOutcome {
    #[must_use]
    pub const fn window(self) -> Option<WgpuEditorWindowPresentedReceipt> {
        self.window
    }

    #[must_use]
    pub const fn composite(self) -> Option<WgpuEditorCompositeReceipt> {
        self.composite
    }
}

/// Private fixed-topology GPU owner used by the first-party Editor Host.
///
/// This type intentionally exposes no target registry or general render graph. Raw WGPU values are
/// borrowed only through scoped first-party callbacks.
#[doc(hidden)]
pub struct WgpuEditorExecution {
    backend: WgpuRenderBackend,
    handles: BackendWindowHandles,
    viewport_target: Option<WgpuEditorViewportTarget>,
    last_target_generation: u64,
    last_source_handoff_generation: u64,
    last_presentation_generation: u64,
}

impl std::fmt::Debug for WgpuEditorExecution {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WgpuEditorExecution")
            .field("backend_instance_id", &self.backend.instance_id())
            .field("device_epoch", &self.backend.device_epoch())
            .field("viewport_target_active", &self.viewport_target.is_some())
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
            backend: WgpuRenderBackend::default(),
            handles,
            viewport_target: None,
            last_target_generation: 0,
            last_source_handoff_generation: 0,
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
    pub const fn frame_transaction_stats(&self) -> WgpuFrameTransactionStats {
        self.backend.frame_transaction_stats()
    }

    pub fn with_device<R>(
        &mut self,
        window: RenderWindowPacket,
        operation: impl for<'frame> FnOnce(WgpuEditorDeviceScope<'frame>) -> R,
    ) -> Result<R, WgpuEditorExecutionError> {
        let namespace = self.backend.prepare_editor_window(&self.handles, window)?;
        Ok(operation(WgpuEditorDeviceScope {
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
        window: RenderWindowPacket,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
        operation: impl for<'frame> FnOnce(
            WgpuEditorDeviceScope<'frame>,
            WgpuEditorReservedViewportTexture<'frame>,
        ) -> R,
    ) -> Result<(WgpuEditorViewportTargetReservation, R), WgpuEditorExecutionError> {
        let namespace = self.backend.prepare_editor_window(&self.handles, window)?;
        let reservation =
            self.reserve_viewport_target_in_namespace(&namespace, source_generation, target)?;
        let result = operation(
            WgpuEditorDeviceScope {
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
        self.backend
            .activate_editor_source(reservation.stamp.source_generation);
        self.last_target_generation = reservation.stamp.generation;
        self.last_source_handoff_generation = reservation.stamp.source_handoff_generation;
        self.viewport_target = Some(WgpuEditorViewportTarget {
            stamp: reservation.stamp,
            _texture: reservation.texture,
            view: reservation.view,
        });
    }

    #[must_use]
    pub fn viewport_target_matches(
        &self,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> bool {
        self.viewport_target.as_ref().is_some_and(|active| {
            active.stamp.source_generation == source_generation
                && active.stamp.target == target
                && active.stamp.backend_instance_id == self.backend.instance_id()
                && active.stamp.device_epoch == self.backend.device_epoch()
        })
    }

    pub fn submit_image(
        &mut self,
        window: RenderWindowPacket,
        packet: RenderFramePacket,
    ) -> Result<WgpuSubmittedImageReceipt, WgpuEditorExecutionError> {
        self.submit_image_payload(window, packet, WgpuFramePayload::default(), None)
    }

    #[cfg(feature = "sprite-submitter")]
    pub fn submit_sprite_image(
        &mut self,
        window: RenderWindowPacket,
        packet: RenderFramePacket,
        sprite_batches: SpriteBatches,
    ) -> Result<WgpuSubmittedImageReceipt, WgpuEditorExecutionError> {
        self.submit_image_payload(
            window,
            packet,
            WgpuFramePayload::for_editor_sprites(sprite_batches),
            None,
        )
    }

    #[cfg(feature = "ui-submitter")]
    pub fn submit_ui_image(
        &mut self,
        window: RenderWindowPacket,
        packet: RenderFramePacket,
        ui_frame: UiBackendPresentationFrame,
    ) -> Result<WgpuEditorUiImageSubmission, WgpuEditorUiSubmitFailure> {
        let (ui_packets, submissions) = ui_frame.into_parts();
        let mut ui = WgpuEditorUiSubmission {
            state: WgpuUiSubmissionState::Prepared(submissions),
        };
        let payload = WgpuFramePayload::for_editor_runtime_ui(ui_packets);
        match self.submit_image_payload(window, packet, payload, Some(&mut ui)) {
            Ok(image) => Ok(WgpuEditorUiImageSubmission { image, ui }),
            Err(error) => Err(WgpuEditorUiSubmitFailure { error, ui }),
        }
    }

    /// Submits an authoring-only UI packet without entering the Runtime UI outcome protocol.
    #[cfg(feature = "ui-submitter")]
    pub fn submit_authoring_ui_image(
        &mut self,
        window: RenderWindowPacket,
        packet: RenderFramePacket,
        ui_packet: Arc<UiPaintPacket>,
    ) -> Result<WgpuSubmittedImageReceipt, WgpuEditorExecutionError> {
        self.submit_image_payload(
            window,
            packet,
            WgpuFramePayload::for_editor_authoring_ui(vec![ui_packet]),
            None,
        )
    }

    #[cfg(all(feature = "sprite-submitter", feature = "ui-submitter"))]
    pub fn submit_sprite_ui_image(
        &mut self,
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
        match self.submit_image_payload(window, packet, payload, Some(&mut ui)) {
            Ok(image) => Ok(WgpuEditorUiImageSubmission { image, ui }),
            Err(error) => Err(WgpuEditorUiSubmitFailure { error, ui }),
        }
    }

    fn submit_image_payload(
        &mut self,
        window: RenderWindowPacket,
        packet: RenderFramePacket,
        payload: WgpuFramePayload,
        #[cfg(feature = "ui-submitter")] mut ui: Option<&mut WgpuEditorUiSubmission>,
        #[cfg(not(feature = "ui-submitter"))] _ui: Option<&mut ()>,
    ) -> Result<WgpuSubmittedImageReceipt, WgpuEditorExecutionError> {
        let RenderFrameTarget::Image(target) = packet.target() else {
            return Err(WgpuEditorExecutionError::ExpectedImageTarget);
        };
        let namespace = self.backend.prepare_editor_window(&self.handles, window)?;
        self.require_viewport_target(packet.source_generation(), target)?;
        self.backend
            .begin_frame_transaction(packet.frame_index(), None);
        if let Err(error) = self
            .backend
            .admit_packet(packet.source_generation(), packet.frame_index())
        {
            self.backend.reject_captured_packet();
            return Err(error.into());
        }
        #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
        self.backend
            .begin_editor_submitter_frame(packet.frame_index());
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
        let draw = self.backend.prepare_target_draw(
            EDITOR_IMAGE_FORMAT,
            &packet,
            &payload,
            packet.frame_index(),
        )?;
        let viewport_target = self
            .viewport_target
            .as_ref()
            .expect("ensuring an Editor viewport target installs it");
        let mut encoder =
            namespace
                .device
                .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("nara_editor_viewport_encoder"),
                });
        crate::encode_target_draw(&mut encoder, &viewport_target.view, &draw);
        namespace.queue.submit([encoder.finish()]);
        self.backend.record_editor_image_submission();
        Ok(WgpuSubmittedImageReceipt::new(
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
        viewport_submission: Option<WgpuSubmittedImageReceipt>,
        paint: impl for<'frame> FnOnce(
            &mut WgpuEditorWindowFrame<'frame>,
        ) -> Result<(), WgpuEditorExecutionError>,
        pre_present: impl FnOnce(),
    ) -> Result<WgpuEditorFrameOutcome, WgpuEditorExecutionError> {
        let namespace = self.backend.prepare_editor_window(&self.handles, window)?;
        let Some(acquired) = self.backend.acquire_editor_surface(window.id)? else {
            return Ok(WgpuEditorFrameOutcome::default());
        };
        let viewport = match viewport_submission {
            Some(submission) => Some(self.viewport_texture(&namespace, submission)?),
            None => None,
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
        let mut sampled_viewport = None;
        paint(&mut WgpuEditorWindowFrame {
            device: &namespace.device,
            queue: &namespace.queue,
            encoder: &mut encoder,
            target_view: &target_view,
            surface_format: format,
            surface_extent: extent,
            viewport,
            leading_command_buffers: &mut leading_command_buffers,
            sampled_viewport: &mut sampled_viewport,
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
        let composite = join_composite_receipt(
            &mut self.last_presentation_generation,
            viewport_submission,
            sampled_viewport,
            window_receipt,
        )?;
        Ok(WgpuEditorFrameOutcome {
            window: Some(window_receipt),
            composite,
        })
    }

    pub fn retire_viewport_target(&mut self) {
        self.viewport_target = None;
    }

    fn viewport_texture(
        &self,
        namespace: &WgpuEditorDeviceNamespace,
        submission: WgpuSubmittedImageReceipt,
    ) -> Result<WgpuEditorViewportTexture, WgpuEditorExecutionError> {
        if !self.viewport_target.as_ref().is_some_and(|target| {
            target.matches_namespace(namespace) && target.matches_receipt(submission)
        }) {
            return Err(WgpuEditorExecutionError::StaleImageSubmission);
        }
        Ok(WgpuEditorViewportTexture {
            receipt: submission,
        })
    }

    fn require_viewport_target(
        &self,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> Result<(), WgpuEditorExecutionError> {
        if self.viewport_target_matches(source_generation, target) {
            return Ok(());
        }
        if self.viewport_target.is_some() {
            return Err(WgpuEditorExecutionError::ViewportTargetAlreadyActive);
        }
        Err(WgpuEditorExecutionError::ViewportTargetUnavailable)
    }

    fn reserve_viewport_target_in_namespace(
        &self,
        namespace: &WgpuEditorDeviceNamespace,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> Result<WgpuEditorViewportTargetReservation, WgpuEditorExecutionError> {
        let stamp = self.next_viewport_reservation_stamp(namespace, source_generation, target)?;
        let extent = target.extent();
        let maximum = namespace.device.limits().max_texture_dimension_2d;
        if extent.width > maximum || extent.height > maximum {
            return Err(WgpuEditorExecutionError::ViewportTargetExtentUnsupported {
                width: extent.width,
                height: extent.height,
                maximum,
            });
        }
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
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(EDITOR_IMAGE_FORMAT),
            ..Default::default()
        });
        Ok(WgpuEditorViewportTargetReservation {
            stamp,
            texture,
            view,
        })
    }

    fn next_viewport_reservation_stamp(
        &self,
        namespace: &WgpuEditorDeviceNamespace,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> Result<WgpuEditorViewportReservationStamp, WgpuEditorExecutionError> {
        self.next_viewport_reservation_stamp_for_namespace(
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
        backend_instance_id: u64,
        device_epoch: u64,
        source_generation: RenderFrameSourceGeneration,
        target: RenderImagePacket,
    ) -> Result<WgpuEditorViewportReservationStamp, WgpuEditorExecutionError> {
        next_viewport_reservation_stamp(
            self.last_target_generation,
            self.last_source_handoff_generation,
            self.backend.editor_source_generation(),
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
    use nara_asset::{AssetId, Handle};
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
    fn composite_receipt_requires_the_exact_submitted_sample_and_window_namespace() {
        let frame = test_image_frame();
        let image = image_target(&frame);
        let submitted =
            WgpuSubmittedImageReceipt::new(image, 4, frame.source_generation(), 6, 10, 2, 7);
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
        let first =
            next_viewport_reservation_stamp(0, 0, None, 11, 2, source, first_target).unwrap();
        let duplicate = first;
        let resized_target = image_target(&test_image_frame());
        let successor = next_viewport_reservation_stamp(
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
        let target_reservation =
            next_viewport_reservation_stamp(0, 0, None, 11, 2, source, target).unwrap();
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
        let submitted =
            WgpuSubmittedImageReceipt::new(image, 8, frame.source_generation(), 3, 21, 4, 12);
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
            WgpuSubmittedImageReceipt::new(image, 9, frame.source_generation(), 3, 21, 4, 12),
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

    fn test_image_frame() -> RenderFramePacket {
        let generation = EditorRenderGeneration::try_acquire().unwrap();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            camera_entity: Entity::PLACEHOLDER,
            target: RenderTarget::Image(Handle::<RenderImage2d>::new(AssetId::from_raw(17))),
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
                Handle::<RenderImage2d>::new(AssetId::from_raw(17)),
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
