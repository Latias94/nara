use std::{sync::Arc, time::Instant};

use nara_ecs::{Res, Resource, SystemParam, SystemSet, World};
#[cfg(feature = "stock-ui")]
use nara_render::RenderFrameState;
#[cfg(any(feature = "stock-ui", all(test, feature = "stock-sprite")))]
use nara_render::RenderTarget;
#[cfg(all(test, any(feature = "stock-sprite", feature = "stock-ui")))]
use nara_render::ViewportRect;
use nara_render::{RenderFramePacket, RenderFramePacketError};
#[cfg(all(test, feature = "stock-sprite"))]
use nara_sprite_render::__private::SpriteBatches;
#[cfg(all(test, feature = "stock-ui"))]
use nara_ui_render::__private::UiPresentationOutcomes;
#[cfg(feature = "stock-ui")]
use nara_ui_render::__private::{
    UiBackendSubmission, UiCompletedBackendOutcome, UiSubmittedBackendSubmission,
};
#[cfg(all(test, feature = "stock-ui"))]
use nara_ui_render::{UiPacketPrimitive, UiPaintPacket};
#[cfg(feature = "stock-ui")]
use nara_ui_render::{
    UiPaintPacketIdentity, UiPresentationAttemptIdentity, UiPresentationBackendFaultDomain,
    UiPresentationBackendGeneration, UiPresentationIntegrationError,
};
#[cfg(feature = "stock-ui")]
use nara_window::WindowId;

#[cfg(any(feature = "stock-ui", all(test, feature = "stock-sprite")))]
use crate::WgpuRenderError;
#[cfg(all(test, any(feature = "stock-sprite", feature = "stock-ui")))]
use crate::quad::WgpuQuadBatch;
#[cfg(all(test, feature = "stock-ui"))]
use crate::ui::WgpuLoweredUiPacketFacts;
use crate::{WgpuRenderInput, WgpuRenderProgramError, policy::CapturedPolicy};

/// Ordered CPU capture and optional native-window publication in `CoreStage::Render`.
/// Policy systems contribute their immutable input in [`Self::Capture`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub enum WgpuFrameSet {
    Reset,
    Capture,
    Seal,
    Publish,
}

/// Private legacy drawing input retained only for numerical comparison tests.
#[cfg(test)]
#[derive(Debug, Default, Clone)]
pub(crate) struct LegacyWgpuFramePayload {
    #[cfg(feature = "stock-ui")]
    pub(crate) primary_window_id: Option<WindowId>,
    #[cfg(feature = "stock-ui")]
    pub(crate) ui_logical_target: Option<RenderTarget>,
    #[cfg(feature = "stock-sprite")]
    pub(crate) sprite_batches: Option<SpriteBatches>,
    #[cfg(feature = "stock-ui")]
    pub(crate) ui_packets: Vec<Arc<UiPaintPacket>>,
}

#[cfg(feature = "stock-ui")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct WgpuUiPayloadFacts {
    pub(crate) packet_count: u64,
    pub(crate) packet_identity: Option<UiPaintPacketIdentity>,
    pub(crate) primitives: u64,
    pub(crate) glyph_primitives: u64,
    pub(crate) glyph_pages: u64,
}

#[derive(Debug)]
pub(crate) struct WgpuCapturedFrame {
    pub(crate) input: WgpuRenderInput,
    pub(crate) app_frame_index: u64,
    pub(crate) work_started_at: Instant,
}

/// Private legacy transaction retained only for numerical comparison tests.
#[cfg(test)]
#[derive(Debug)]
pub(crate) struct LegacyWgpuCapturedFrame {
    pub(crate) topology: RenderFramePacket,
    pub(crate) payload: LegacyWgpuFramePayload,
    pub(crate) app_frame_index: u64,
    pub(crate) work_started_at: Instant,
}

/// Refusal to seal one exact CPU policy capture and its neutral render topology.
#[derive(Debug, thiserror::Error)]
pub enum WgpuFrameCaptureError {
    #[error("managed runtime generation is unavailable")]
    MissingRuntimeGeneration,
    #[error(transparent)]
    Topology(#[from] RenderFramePacketError),
    #[error("a render batch does not belong to the admitted view and target")]
    BatchTopologyMismatch,
    #[error("nonempty render views require a capture from the selected policy installation")]
    MissingPolicy,
    #[error(transparent)]
    Policy(#[from] WgpuRenderProgramError),
    #[cfg(feature = "stock-ui")]
    #[error(transparent)]
    UiPresentation(#[from] UiPresentationIntegrationError),
}

#[cfg(feature = "stock-ui")]
#[derive(Debug)]
pub(crate) enum WgpuUiSubmissionState {
    Prepared(Vec<UiBackendSubmission>),
    Submitted(Vec<UiSubmittedBackendSubmission>),
}

#[cfg(feature = "stock-ui")]
impl WgpuUiSubmissionState {
    pub(crate) fn submission_count(&self) -> usize {
        match self {
            Self::Prepared(submissions) => submissions.len(),
            Self::Submitted(submissions) => submissions.len(),
        }
    }

    pub(crate) fn validate_backend_generation(
        &self,
        backend_generation: UiPresentationBackendGeneration,
    ) -> Result<(), WgpuRenderError> {
        let Self::Prepared(submissions) = self else {
            return Ok(());
        };
        let Some((admission_id, required)) = submissions.iter().find_map(|submission| {
            submission
                .submission()
                .required_backend_generation()
                .filter(|required| *required != backend_generation)
                .map(|required| {
                    (
                        submission.submission().identity().admission_id().get(),
                        required,
                    )
                })
        }) else {
            return Ok(());
        };
        Err(ui_backend_generation_mismatch(
            admission_id,
            required,
            backend_generation,
        ))
    }

    pub(crate) fn bind_backend(
        &mut self,
        backend_generation: UiPresentationBackendGeneration,
    ) -> Result<(), WgpuRenderError> {
        self.validate_backend_generation(backend_generation)?;
        let Self::Prepared(prepared) = self else {
            return Ok(());
        };
        match nara_ui_render::__private::bind_backend_submission_batch(
            std::mem::take(prepared),
            backend_generation,
        ) {
            Ok(submissions) => {
                *self = Self::Submitted(submissions);
                Ok(())
            }
            Err(error) => {
                let mismatch = ui_backend_generation_mismatch(
                    error.admission_id().get(),
                    error.required(),
                    error.actual(),
                );
                *prepared = error.into_submissions();
                Err(mismatch)
            }
        }
    }

    pub(crate) fn bound_submissions(&self) -> &[UiSubmittedBackendSubmission] {
        let Self::Submitted(submissions) = self else {
            return &[];
        };
        submissions
    }
}

#[cfg(feature = "stock-ui")]
fn ui_backend_generation_mismatch(
    admission_id: u64,
    required: UiPresentationBackendGeneration,
    actual: UiPresentationBackendGeneration,
) -> WgpuRenderError {
    WgpuRenderError::UiSubmissionRejected {
        message: format!(
            "UI backend attempt {admission_id} requires generation ({}, {}) but the live generation is ({}, {})",
            required.backend_instance_id(),
            required.device_epoch(),
            actual.backend_instance_id(),
            actual.device_epoch(),
        ),
    }
}

#[cfg(feature = "stock-ui")]
pub(crate) enum WgpuUiCompletion {
    Presented,
    Cancelled(String),
    Deferred(String),
    ContentRejected(String),
    BackendFault {
        backend_generation: Option<UiPresentationBackendGeneration>,
        domain: UiPresentationBackendFaultDomain,
        message: String,
    },
    Superseded {
        successor: UiPresentationAttemptIdentity,
        message: String,
    },
}

#[cfg(feature = "stock-ui")]
pub(crate) fn complete_ui_submissions(
    submissions: WgpuUiSubmissionState,
    completion: WgpuUiCompletion,
) -> Vec<UiCompletedBackendOutcome> {
    let mut completed = Vec::with_capacity(submissions.submission_count());
    complete_ui_submissions_into(submissions, completion, &mut completed);
    completed
}

/// Appends one outcome per original submission without growing the output allocation.
///
/// The caller must reserve room for the complete batch before calling. Runtime captures reserve
/// this storage before native execution begins.
#[cfg(feature = "stock-ui")]
pub(crate) fn complete_ui_submissions_into(
    submissions: WgpuUiSubmissionState,
    completion: WgpuUiCompletion,
    completed: &mut Vec<UiCompletedBackendOutcome>,
) {
    assert!(
        completed.capacity() - completed.len() >= submissions.submission_count(),
        "UI completion storage must be reserved for the complete batch"
    );
    match (submissions, completion) {
        (WgpuUiSubmissionState::Submitted(submissions), WgpuUiCompletion::Presented) => {
            for submission in submissions {
                completed.push(submission.presented());
            }
        }
        (WgpuUiSubmissionState::Prepared(submissions), WgpuUiCompletion::Presented) => {
            for submission in submissions {
                completed.push(submission.backend_fault(
                    None,
                    UiPresentationBackendFaultDomain::Target,
                    "the target reached presentation without binding UI to a live backend generation"
                        .to_owned(),
                ));
            }
        }
        (WgpuUiSubmissionState::Prepared(submissions), WgpuUiCompletion::Cancelled(message)) => {
            for submission in submissions {
                completed.push(submission.aborted(message.clone()));
            }
        }
        (WgpuUiSubmissionState::Submitted(submissions), WgpuUiCompletion::Cancelled(message)) => {
            for submission in submissions {
                completed.push(submission.deferred(message.clone()));
            }
        }
        (WgpuUiSubmissionState::Submitted(submissions), WgpuUiCompletion::Deferred(message)) => {
            for submission in submissions {
                completed.push(submission.deferred(message.clone()));
            }
        }
        (WgpuUiSubmissionState::Prepared(submissions), WgpuUiCompletion::Deferred(message)) => {
            for submission in submissions {
                completed.push(submission.backend_fault(
                    None,
                    UiPresentationBackendFaultDomain::Target,
                    message.clone(),
                ));
            }
        }
        (
            WgpuUiSubmissionState::Prepared(submissions),
            WgpuUiCompletion::ContentRejected(message),
        ) => {
            for submission in submissions {
                completed.push(submission.content_rejected(message.clone()));
            }
        }
        (
            WgpuUiSubmissionState::Submitted(submissions),
            WgpuUiCompletion::ContentRejected(message),
        ) => {
            for submission in submissions {
                completed.push(
                    submission
                        .backend_fault(UiPresentationBackendFaultDomain::Attempt, message.clone()),
                );
            }
        }
        (
            WgpuUiSubmissionState::Prepared(submissions),
            WgpuUiCompletion::BackendFault {
                backend_generation,
                domain,
                message,
            },
        ) => {
            for submission in submissions {
                completed.push(submission.backend_fault(
                    backend_generation,
                    domain,
                    message.clone(),
                ));
            }
        }
        (
            WgpuUiSubmissionState::Submitted(submissions),
            WgpuUiCompletion::BackendFault {
                domain, message, ..
            },
        ) => {
            for submission in submissions {
                completed.push(submission.backend_fault(domain, message.clone()));
            }
        }
        (
            WgpuUiSubmissionState::Prepared(submissions),
            WgpuUiCompletion::Superseded { successor, message },
        ) => {
            for submission in submissions {
                completed.push(submission.superseded(successor, message.clone()));
            }
        }
        (
            WgpuUiSubmissionState::Submitted(submissions),
            WgpuUiCompletion::Superseded { successor, message },
        ) => {
            for submission in submissions {
                completed.push(submission.superseded(successor, message.clone()));
            }
        }
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum CapturePhase {
    #[default]
    Idle,
    Collecting,
    Sealed,
    Closed,
}

#[derive(Default, Resource)]
pub(crate) struct WgpuFramePacketSlot {
    phase: CapturePhase,
    capture: Option<Arc<dyn CapturedPolicy>>,
    collection_error: Option<WgpuFrameCaptureError>,
    current: Option<Result<Option<WgpuCapturedFrame>, WgpuFrameCaptureError>>,
    #[cfg(feature = "stock-ui")]
    ui_submissions: Option<WgpuUiSubmissionState>,
}

impl std::fmt::Debug for WgpuFramePacketSlot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = formatter.debug_struct("WgpuFramePacketSlot");
        debug
            .field("phase", &self.phase)
            .field(
                "installation",
                &self.capture.as_ref().map(|capture| capture.admission().id),
            )
            .field("collection_error", &self.collection_error)
            .field("current", &self.current);
        #[cfg(feature = "stock-ui")]
        debug.field("ui_submissions", &self.ui_submissions);
        debug.finish()
    }
}

/// Read-only observation of the current sealed CPU policy input.
///
/// Schedule after [`crate::WgpuPolicyCaptureSet::Seal`] and before [`crate::WgpuPolicyCaptureSet::Publish`]. Cloning the
/// borrowed input retains replayable CPU meaning only: this reader never takes the original
/// transport batch, UI completion sidecar or native presentation responsibility. After the
/// publication owner takes the batch, no input remains observable here.
#[derive(SystemParam)]
pub struct WgpuPolicyFrameReader<'w> {
    slot: Res<'w, WgpuFramePacketSlot>,
}

impl WgpuPolicyFrameReader<'_> {
    pub fn read(&self) -> Result<Option<&WgpuRenderInput>, &WgpuFrameCaptureError> {
        if self.slot.phase != CapturePhase::Sealed {
            return Ok(None);
        }
        match self.slot.current.as_ref() {
            Some(Ok(Some(frame))) => Ok(Some(&frame.input)),
            Some(Err(error)) => Err(error),
            Some(Ok(None)) | None => Ok(None),
        }
    }
}

impl WgpuFramePacketSlot {
    pub(crate) fn close(&mut self) {
        // Return every original UI permission before the Runtime's logical close barrier.
        // Their original pre-admitted result storage remains alive independently of this slot.
        self.phase = CapturePhase::Closed;
        self.capture = None;
        self.current = None;
        self.collection_error = None;
        #[cfg(feature = "stock-ui")]
        {
            self.ui_submissions = None;
        }
    }

    pub(crate) fn reset(&mut self) {
        // A slow consumer owns the current batch until it takes it, including its UI sidecar.
        if self.phase == CapturePhase::Closed || self.current.is_some() {
            return;
        }
        if self.phase == CapturePhase::Collecting {
            return;
        }
        self.phase = CapturePhase::Collecting;
    }

    pub(crate) fn is_collecting(&self) -> bool {
        self.phase == CapturePhase::Collecting && self.current.is_none()
    }

    pub(crate) fn accepts_capture(&self) -> bool {
        self.is_collecting() && self.capture.is_none() && self.collection_error.is_none()
    }

    pub(crate) fn collect_policy(&mut self, capture: Arc<dyn CapturedPolicy>) -> bool {
        if !self.accepts_capture() {
            return false;
        }
        self.capture = Some(capture);
        true
    }

    #[cfg(feature = "stock-ui")]
    pub(crate) fn accepts_ui_collection(&self) -> bool {
        self.accepts_capture() && self.ui_submissions.is_none()
    }

    #[cfg(feature = "stock-ui")]
    pub(crate) fn collect_policy_with_ui(
        &mut self,
        capture: Arc<dyn CapturedPolicy>,
        ui: WgpuUiSubmissionState,
    ) -> Result<(), (Arc<dyn CapturedPolicy>, WgpuUiSubmissionState)> {
        if !self.accepts_ui_collection() {
            return Err((capture, ui));
        }
        self.capture = Some(capture);
        self.ui_submissions = Some(ui);
        Ok(())
    }

    #[cfg(feature = "stock-ui")]
    pub(crate) fn reject_ui_collection(&mut self, error: UiPresentationIntegrationError) {
        if self.accepts_ui_collection() {
            self.collection_error = Some(WgpuFrameCaptureError::UiPresentation(error));
        }
    }

    pub(crate) fn seal(
        &mut self,
        captured: Result<Option<RenderFramePacket>, RenderFramePacketError>,
        app_frame_index: u64,
        work_started_at: Instant,
    ) {
        if !self.is_collecting() {
            return;
        }
        self.phase = CapturePhase::Sealed;
        let capture = self.capture.take();
        self.current = Some(if let Some(error) = self.collection_error.take() {
            Err(error)
        } else {
            captured
                .map_err(WgpuFrameCaptureError::from)
                .and_then(|packet| {
                    let Some(packet) = packet else {
                        return Ok(None);
                    };
                    let capture = capture.ok_or(WgpuFrameCaptureError::MissingPolicy)?;
                    let input = WgpuRenderInput::from_capture(packet, capture)?;
                    Ok(Some(WgpuCapturedFrame {
                        input,
                        app_frame_index,
                        work_started_at,
                    }))
                })
        });
    }

    pub(crate) fn reject_capture(&mut self, error: WgpuFrameCaptureError) {
        if !self.is_collecting() {
            return;
        }
        self.phase = CapturePhase::Sealed;
        self.capture = None;
        self.current = Some(Err(self.collection_error.take().unwrap_or(error)));
    }

    pub(crate) fn take_batch(&mut self) -> Option<WgpuCapturedPolicyBatch> {
        let captured = self.current.take()?;
        self.phase = CapturePhase::Idle;
        Some(WgpuCapturedPolicyBatch {
            captured,
            #[cfg(feature = "stock-ui")]
            ui: self.ui_submissions.take(),
        })
    }
}

/// Move-only transport of an already sealed policy input and its original UI permissions.
#[doc(hidden)]
#[derive(Debug)]
pub struct WgpuCapturedPolicyBatch {
    pub(crate) captured: Result<Option<WgpuCapturedFrame>, WgpuFrameCaptureError>,
    #[cfg(feature = "stock-ui")]
    pub(crate) ui: Option<WgpuUiSubmissionState>,
}

impl WgpuCapturedPolicyBatch {
    #[cfg(feature = "stock-ui")]
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        Result<Option<WgpuRenderInput>, WgpuFrameCaptureError>,
        Option<crate::host::WgpuUiSubmission>,
    ) {
        (
            self.captured
                .map(|captured| captured.map(|frame| frame.input)),
            self.ui.map(crate::host::WgpuUiSubmission::from_state),
        )
    }

    #[cfg(not(feature = "stock-ui"))]
    #[must_use]
    pub fn into_parts(self) -> (Result<Option<WgpuRenderInput>, WgpuFrameCaptureError>,) {
        (self
            .captured
            .map(|captured| captured.map(|frame| frame.input)),)
    }
}

/// Takes the original sealed batch once without selecting or capturing a policy again.
#[doc(hidden)]
pub fn take_captured_policy_batch(world: &mut World) -> Option<WgpuCapturedPolicyBatch> {
    world
        .get_resource_mut::<WgpuFramePacketSlot>()?
        .take_batch()
}

#[cfg(test)]
impl LegacyWgpuFramePayload {
    #[cfg(feature = "stock-sprite")]
    pub(crate) fn for_editor_sprites(sprite_batches: SpriteBatches) -> Self {
        Self {
            sprite_batches: Some(sprite_batches),
            ..Self::default()
        }
    }

    #[cfg(feature = "stock-ui")]
    pub(crate) fn for_editor_runtime_ui(ui_packets: Vec<Arc<UiPaintPacket>>) -> Self {
        Self {
            primary_window_id: Some(WindowId::PRIMARY),
            ui_logical_target: Some(RenderTarget::Window(WindowId::PRIMARY)),
            ui_packets,
            ..Self::default()
        }
    }

    #[cfg(all(feature = "stock-sprite", feature = "stock-ui"))]
    pub(crate) fn for_editor_viewport(
        sprite_batches: SpriteBatches,
        ui_packets: Vec<Arc<UiPaintPacket>>,
    ) -> Self {
        let mut payload = Self::for_editor_runtime_ui(ui_packets);
        payload.sprite_batches = Some(sprite_batches);
        payload
    }

    #[cfg(feature = "stock-ui")]
    pub(crate) fn ui_facts(&self, target: RenderTarget) -> WgpuUiPayloadFacts {
        let packet_target = self.ui_packet_target(target);
        let mut facts = WgpuUiPayloadFacts::default();
        for packet in self.ui_packets.iter().filter(|packet| {
            crate::ui::ui_packet_targets_view(packet, packet_target, self.primary_window_id)
        }) {
            facts.packet_count = facts.packet_count.saturating_add(1);
            facts.packet_identity = if facts.packet_count == 1 {
                Some(packet.identity())
            } else {
                None
            };
            facts.primitives = facts
                .primitives
                .saturating_add(crate::saturating_u64(packet.primitives().len()));
            facts.glyph_primitives = facts.glyph_primitives.saturating_add(crate::saturating_u64(
                packet
                    .primitives()
                    .iter()
                    .filter(|primitive| matches!(primitive, UiPacketPrimitive::Glyph { .. }))
                    .count(),
            ));
            facts.glyph_pages = facts
                .glyph_pages
                .saturating_add(crate::saturating_u64(packet.glyph_pages().len()));
        }
        facts
    }

    #[cfg(feature = "stock-ui")]
    fn ui_packet_target(&self, execution_target: RenderTarget) -> RenderTarget {
        self.ui_logical_target.unwrap_or(execution_target)
    }

    /// Private legacy lowering oracle for ordered quad readback and Runtime comparison tests.
    #[cfg(all(test, any(feature = "stock-sprite", feature = "stock-ui")))]
    pub(crate) fn lower_draw(
        &self,
        target: RenderTarget,
        viewport: ViewportRect,
        view_index: Option<usize>,
    ) -> Result<WgpuLoweredFrameDraw, WgpuRenderError> {
        #[cfg(feature = "stock-sprite")]
        let content = self
            .sprite_batches
            .as_ref()
            .map_or_else(Vec::new, |batches| {
                view_index.map_or_else(Vec::new, |index| {
                    crate::sprite::collect_sprite_quad_batches(batches, index)
                })
            });
        #[cfg(not(feature = "stock-sprite"))]
        let content = Vec::new();
        #[cfg(feature = "stock-ui")]
        let ui_packet_target = self.ui_packet_target(target);
        #[cfg(feature = "stock-ui")]
        let ui = crate::ui::lower_ui_packets(
            if view_index.is_none() {
                &self.ui_packets
            } else {
                &[]
            },
            ui_packet_target,
            self.primary_window_id,
            viewport,
        )
        .map_err(|error| WgpuRenderError::UiPacketInvariant {
            message: error.to_string(),
        })?;
        #[cfg(not(feature = "stock-ui"))]
        let ui_batches = {
            let _ = (target, viewport, view_index);
            Vec::new()
        };
        #[cfg(feature = "stock-ui")]
        let ui_batches = ui.batches;

        Ok(WgpuLoweredFrameDraw {
            batches: nara_render::__private::assemble_first_party_draws(content, ui_batches),
            #[cfg(feature = "stock-ui")]
            ui_packets: ui.packet_facts,
        })
    }
}

/// Private legacy combined draw used only by the native/unit comparison oracle.
#[cfg(all(test, any(feature = "stock-sprite", feature = "stock-ui")))]
#[derive(Debug, Default)]
pub(crate) struct WgpuLoweredFrameDraw {
    pub(crate) batches: Vec<WgpuQuadBatch>,
    #[cfg(feature = "stock-ui")]
    pub(crate) ui_packets: Vec<WgpuLoweredUiPacketFacts>,
}

#[cfg(feature = "stock-ui")]
pub(crate) fn render_targets_match(
    packet: RenderTarget,
    view: RenderTarget,
    primary_window_id: Option<WindowId>,
) -> bool {
    match (packet, view) {
        (RenderTarget::Image(packet), RenderTarget::Image(view)) => packet == view,
        (RenderTarget::Image(_), _) | (_, RenderTarget::Image(_)) => false,
        (packet, view) => view
            .window_id(primary_window_id)
            .is_some_and(|window| packet.window_id(primary_window_id) == Some(window)),
    }
}

#[cfg(feature = "stock-ui")]
pub(crate) fn ui_completion(
    submissions: &WgpuUiSubmissionState,
    result: &Result<(), WgpuRenderError>,
    frame_state: RenderFrameState,
    device_fault_generation: Option<UiPresentationBackendGeneration>,
    skip_message: Option<&str>,
) -> WgpuUiCompletion {
    match (submissions, result) {
        (WgpuUiSubmissionState::Submitted(_), Ok(()))
            if frame_state == RenderFrameState::Submitted =>
        {
            WgpuUiCompletion::Presented
        }
        (WgpuUiSubmissionState::Submitted(_), Ok(())) => WgpuUiCompletion::BackendFault {
            backend_generation: None,
            domain: UiPresentationBackendFaultDomain::Target,
            message: skip_message
                .unwrap_or("the target surface did not submit the exact UI packet")
                .to_owned(),
        },
        (WgpuUiSubmissionState::Submitted(_), Err(error)) if error.is_deferred() => {
            WgpuUiCompletion::Deferred(error.to_string())
        }
        (WgpuUiSubmissionState::Submitted(_), Err(error)) if error.is_content_rejection() => {
            WgpuUiCompletion::ContentRejected(error.to_string())
        }
        (WgpuUiSubmissionState::Prepared(_), Err(error)) if error.is_content_rejection() => {
            WgpuUiCompletion::ContentRejected(error.to_string())
        }
        (WgpuUiSubmissionState::Prepared(_), Ok(())) => WgpuUiCompletion::BackendFault {
            backend_generation: None,
            domain: UiPresentationBackendFaultDomain::Target,
            message: skip_message
                .unwrap_or("the target did not reach a live device generation before submit")
                .to_owned(),
        },
        (_, Err(error)) => {
            let domain = classify_ui_backend_fault(error);
            WgpuUiCompletion::BackendFault {
                backend_generation: (domain == UiPresentationBackendFaultDomain::Device)
                    .then_some(device_fault_generation)
                    .flatten(),
                domain,
                message: error.to_string(),
            }
        }
    }
}

#[cfg(all(test, feature = "stock-ui"))]
pub(crate) fn publish_ui_outcomes(
    outcomes: &mut UiPresentationOutcomes,
    submissions: WgpuUiSubmissionState,
    result: &Result<(), WgpuRenderError>,
    frame_state: RenderFrameState,
    device_fault_generation: Option<UiPresentationBackendGeneration>,
    skip_message: Option<&str>,
) {
    let completion = ui_completion(
        &submissions,
        result,
        frame_state,
        device_fault_generation,
        skip_message,
    );
    for completed in complete_ui_submissions(submissions, completion) {
        outcomes.commit_backend(completed);
    }
}

#[cfg(feature = "stock-ui")]
pub(crate) const fn classify_ui_backend_fault(
    error: &WgpuRenderError,
) -> UiPresentationBackendFaultDomain {
    if error.is_target_fault() {
        UiPresentationBackendFaultDomain::Target
    } else if error.is_attempt_fault() {
        UiPresentationBackendFaultDomain::Attempt
    } else {
        UiPresentationBackendFaultDomain::Device
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "stock-ui")]
    fn empty_policy_capture() -> Arc<dyn CapturedPolicy> {
        crate::WgpuPolicyDefinition::new(crate::WgpuStockPolicy::new())
            .install()
            .unwrap()
            .accept(Arc::new(crate::WgpuStockCapture::new()))
            .unwrap()
    }

    #[test]
    fn sealed_reader_preserves_capture_errors_without_taking_transport() {
        let mut world = World::new();
        world.init_resource::<WgpuFramePacketSlot>();
        let mut state =
            nara_ecs::bevy_ecs::system::SystemState::<WgpuPolicyFrameReader>::new(&mut world);
        assert!(state.get(&world).unwrap().read().unwrap().is_none());
        {
            let mut slot = world.resource_mut::<WgpuFramePacketSlot>();
            slot.reset();
            slot.seal(Ok(Some(image_packet(1))), 1, Instant::now());
        }
        for _ in 0..2 {
            assert!(matches!(
                state.get(&world).unwrap().read(),
                Err(WgpuFrameCaptureError::MissingPolicy)
            ));
        }
        let batch = world
            .resource_mut::<WgpuFramePacketSlot>()
            .take_batch()
            .unwrap();
        assert!(matches!(
            batch.captured,
            Err(WgpuFrameCaptureError::MissingPolicy)
        ));
        assert!(state.get(&world).unwrap().read().unwrap().is_none());
    }

    #[cfg(feature = "stock-ui")]
    #[test]
    fn sealed_reader_clones_cpu_input_without_taking_the_original_ui_sidecar() {
        let installation = crate::WgpuPolicyDefinition::new(crate::WgpuStockPolicy::default())
            .install()
            .unwrap();
        let capture = Arc::new(crate::WgpuStockCapture::default());
        let mut outcomes = UiPresentationOutcomes::default();
        let packet = ui_packet();
        let submissions = prepare_ui_submissions(&mut outcomes, &packet, 1);
        let original_submissions = submissions.as_ptr();
        let mut slot = WgpuFramePacketSlot::default();
        slot.reset();
        assert!(
            slot.collect_policy_with_ui(
                installation.accept(Arc::clone(&capture)).unwrap(),
                WgpuUiSubmissionState::Prepared(submissions)
            )
            .is_ok()
        );
        slot.seal(Ok(Some(image_packet(1))), 7, Instant::now());
        let mut world = World::new();
        world.insert_resource(slot);
        let mut state =
            nara_ecs::bevy_ecs::system::SystemState::<WgpuPolicyFrameReader>::new(&mut world);
        let input = state.get(&world).unwrap().read().unwrap().unwrap().clone();
        assert!(std::ptr::eq(
            input.captured::<crate::WgpuStockPolicy>().unwrap(),
            capture.as_ref()
        ));
        assert_eq!(
            world.resource::<WgpuFramePacketSlot>().phase,
            CapturePhase::Sealed
        );
        world.resource_mut::<WgpuFramePacketSlot>().reset();
        assert_eq!(
            state
                .get(&world)
                .unwrap()
                .read()
                .unwrap()
                .unwrap()
                .packet()
                .frame_index(),
            1
        );

        let batch = world
            .resource_mut::<WgpuFramePacketSlot>()
            .take_batch()
            .unwrap();
        assert!(state.get(&world).unwrap().read().unwrap().is_none());
        assert!(
            world
                .resource_mut::<WgpuFramePacketSlot>()
                .take_batch()
                .is_none()
        );
        let WgpuUiSubmissionState::Prepared(submissions) = batch.ui.unwrap() else {
            panic!("the original UI sidecar must stay unsubmitted");
        };
        assert_eq!(submissions.as_ptr(), original_submissions);
        for completed in complete_ui_submissions(
            WgpuUiSubmissionState::Prepared(submissions),
            WgpuUiCompletion::ContentRejected("the original owner settled its sidecar".to_owned()),
        ) {
            outcomes.commit_backend(completed);
        }
        world.insert_resource(outcomes);
        assert!(matches!(
            nara_ui_render::drain_ui_presentation_outcomes(&mut world)
                .unwrap()
                .as_slice(),
            [nara_ui_render::UiPresentationOutcome::ContentRejected { .. }]
        ));
        assert_eq!(
            input.packet().frame_index(),
            1,
            "CPU replay remains separate from one-shot completion"
        );
        drop(input);
        assert!(
            nara_ui_render::drain_ui_presentation_outcomes(&mut world)
                .unwrap()
                .is_empty()
        );
    }

    #[cfg(feature = "stock-ui")]
    #[test]
    fn target_matching_normalizes_windows_but_keeps_images_exact() {
        use nara_render::RenderImage2d;

        assert!(render_targets_match(
            RenderTarget::Window(WindowId::PRIMARY),
            RenderTarget::PrimaryWindow,
            Some(WindowId::PRIMARY),
        ));
        assert!(!render_targets_match(
            RenderTarget::Image(RenderImage2d::allocate_handle().unwrap()),
            RenderTarget::Image(RenderImage2d::allocate_handle().unwrap()),
            Some(WindowId::PRIMARY),
        ));
    }

    #[test]
    fn policy_capture_starts_only_in_the_collection_phase() {
        let mut slot = WgpuFramePacketSlot::default();
        assert!(!slot.accepts_capture());
        slot.reset();

        assert!(slot.accepts_capture());
        assert!(slot.capture.is_none());
    }

    #[test]
    fn empty_views_do_not_require_an_implicit_policy() {
        let mut slot = WgpuFramePacketSlot::default();
        slot.reset();
        slot.seal(Ok(None), 7, Instant::now());

        assert!(matches!(slot.take_batch().unwrap().captured, Ok(None)));
        assert!(slot.take_batch().is_none());
    }

    #[test]
    fn nonempty_views_require_an_explicit_policy_capture() {
        let mut slot = WgpuFramePacketSlot::default();
        slot.reset();
        slot.seal(Ok(Some(image_packet(1))), 7, Instant::now());

        assert!(matches!(
            slot.take_batch().unwrap().captured,
            Err(WgpuFrameCaptureError::MissingPolicy)
        ));
    }

    #[test]
    fn accepted_capture_survives_withdrawal_and_reset_until_its_consumer_takes_it() {
        let definition = crate::WgpuPolicyDefinition::new(crate::WgpuStockPolicy::default());
        let installation = definition.install().unwrap();
        let identity = installation.id();
        let capture = Arc::new(crate::WgpuStockCapture::default());
        let mut slot = WgpuFramePacketSlot::default();
        slot.reset();
        assert!(slot.collect_policy(installation.accept(Arc::clone(&capture)).unwrap()));
        assert!(!slot.collect_policy(installation.accept(Arc::clone(&capture)).unwrap()));
        drop(installation);

        slot.seal(Ok(Some(image_packet(1))), 7, Instant::now());
        slot.reset();
        slot.seal(Ok(None), 8, Instant::now());
        slot.reject_capture(WgpuFrameCaptureError::MissingRuntimeGeneration);

        let captured = slot.take_batch().unwrap().captured.unwrap().unwrap();
        assert_eq!(captured.input.installation(), identity);
        assert_eq!(captured.input.packet().frame_index(), 1);
        assert_eq!(captured.app_frame_index, 7);
        assert!(std::ptr::eq(
            captured.input.captured::<crate::WgpuStockPolicy>().unwrap(),
            capture.as_ref(),
        ));
        assert!(slot.take_batch().is_none());
        slot.reset();
        assert!(slot.accepts_capture());
    }

    #[test]
    fn reset_preserves_an_unconsumed_capture_error_and_close_is_irreversible() {
        let mut slot = WgpuFramePacketSlot::default();
        slot.reset();
        slot.reject_capture(WgpuFrameCaptureError::MissingRuntimeGeneration);
        slot.reset();
        slot.seal(Ok(None), 7, Instant::now());

        assert!(matches!(
            slot.take_batch().unwrap().captured,
            Err(WgpuFrameCaptureError::MissingRuntimeGeneration)
        ));
        slot.close();
        slot.reset();
        assert!(!slot.is_collecting());
        assert!(slot.take_batch().is_none());
    }

    #[cfg(feature = "stock-ui")]
    #[test]
    fn ui_submission_state_is_absent_until_the_contribution_collects() {
        let mut slot = WgpuFramePacketSlot::default();
        slot.reset();

        assert!(slot.ui_submissions.is_none());
        assert!(slot.accepts_ui_collection());
        assert!(slot.take_batch().is_none());
    }

    #[cfg(feature = "stock-ui")]
    #[test]
    fn refused_policy_ui_pair_preserves_both_allocations_and_every_slot_field() {
        let installation = crate::WgpuPolicyDefinition::new(crate::WgpuStockPolicy::default())
            .install()
            .unwrap();
        let packet = ui_packet();
        for state in 0..6 {
            let mut outcomes = UiPresentationOutcomes::default();
            let submissions = prepare_ui_submissions(&mut outcomes, &packet, 1);
            let allocation = submissions.as_ptr();
            let capture = installation
                .accept(Arc::new(crate::WgpuStockCapture::default()))
                .unwrap();
            let mut slot = WgpuFramePacketSlot::default();
            match state {
                0 => {}
                1 => {
                    slot.reset();
                    assert!(
                        slot.collect_policy(
                            installation
                                .accept(Arc::new(crate::WgpuStockCapture::default()),)
                                .unwrap()
                        )
                    );
                }
                2 => {
                    slot.reset();
                    assert!(
                        slot.collect_policy_with_ui(
                            empty_policy_capture(),
                            WgpuUiSubmissionState::Prepared(Vec::new())
                        )
                        .is_ok()
                    );
                }
                3 => {
                    slot.reset();
                    slot.seal(Ok(None), 1, Instant::now());
                }
                4 => slot.close(),
                5 => {
                    slot.reset();
                    slot.reject_ui_collection(UiPresentationIntegrationError::Unavailable);
                }
                _ => unreachable!(),
            }
            let phase = slot.phase;
            let previous_capture = slot.capture.clone();
            let previous_ui_count = slot
                .ui_submissions
                .as_ref()
                .map(WgpuUiSubmissionState::submission_count);
            let previous_current = slot.current.is_some();
            let previous_error = slot.collection_error.as_ref().map(ToString::to_string);

            let (returned_capture, returned_ui) = slot
                .collect_policy_with_ui(
                    Arc::clone(&capture),
                    WgpuUiSubmissionState::Prepared(submissions),
                )
                .unwrap_err();

            assert!(Arc::ptr_eq(&capture, &returned_capture));
            let WgpuUiSubmissionState::Prepared(returned_submissions) = returned_ui else {
                panic!("refusal must preserve the original UI state");
            };
            assert_eq!(returned_submissions.as_ptr(), allocation);
            assert_eq!(returned_submissions.len(), 1);
            assert_eq!(slot.phase, phase);
            assert_eq!(slot.current.is_some(), previous_current);
            assert_eq!(
                slot.collection_error.as_ref().map(ToString::to_string),
                previous_error
            );
            assert_eq!(
                slot.ui_submissions
                    .as_ref()
                    .map(WgpuUiSubmissionState::submission_count),
                previous_ui_count
            );
            match (previous_capture.as_ref(), slot.capture.as_ref()) {
                (None, None) => {}
                (Some(previous), Some(current)) => assert!(Arc::ptr_eq(previous, current)),
                _ => panic!("refusal must not partially write a policy capture"),
            }
            for completed in complete_ui_submissions(
                WgpuUiSubmissionState::Prepared(returned_submissions),
                WgpuUiCompletion::ContentRejected("original pair returned".to_owned()),
            ) {
                outcomes.commit_backend(completed);
            }
            let mut world = World::new();
            world.insert_resource(outcomes);
            assert!(matches!(
                nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap().as_slice(),
                [nara_ui_render::UiPresentationOutcome::ContentRejected { attempt, .. }]
                    if attempt.admission_id().unwrap().get() == 1,
            ));
        }
    }

    #[cfg(feature = "stock-ui")]
    #[test]
    fn accepted_policy_ui_pair_survives_seal_reset_and_installation_drop() {
        let installation = crate::WgpuPolicyDefinition::new(crate::WgpuStockPolicy::default())
            .install()
            .unwrap();
        let identity = installation.id();
        let capture = Arc::new(crate::WgpuStockCapture::default());
        let packet = ui_packet();
        let mut outcomes = UiPresentationOutcomes::default();
        let submissions = prepare_ui_submissions(&mut outcomes, &packet, 1);
        let allocation = submissions.as_ptr();
        let mut slot = WgpuFramePacketSlot::default();
        slot.reset();
        assert!(
            slot.collect_policy_with_ui(
                installation.accept(Arc::clone(&capture)).unwrap(),
                WgpuUiSubmissionState::Prepared(submissions),
            )
            .is_ok()
        );
        drop(installation);
        slot.seal(Ok(Some(image_packet(1))), 7, Instant::now());
        slot.reset();
        slot.seal(Ok(None), 8, Instant::now());

        let batch = slot.take_batch().unwrap();
        assert!(slot.take_batch().is_none());
        let frame = batch.captured.unwrap().unwrap();
        assert_eq!(frame.input.installation(), identity);
        assert_eq!(frame.input.packet().frame_index(), 1);
        assert_eq!(frame.app_frame_index, 7);
        assert!(std::ptr::eq(
            frame.input.captured::<crate::WgpuStockPolicy>().unwrap(),
            capture.as_ref(),
        ));
        let WgpuUiSubmissionState::Prepared(submissions) = batch.ui.unwrap() else {
            panic!("the original unsubmitted UI state must remain attached");
        };
        assert_eq!(submissions.as_ptr(), allocation);
        for completed in complete_ui_submissions(
            WgpuUiSubmissionState::Prepared(submissions),
            WgpuUiCompletion::ContentRejected("original pair consumed".to_owned()),
        ) {
            outcomes.commit_backend(completed);
        }
        let mut world = World::new();
        world.insert_resource(outcomes);
        assert!(matches!(
            nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap().as_slice(),
            [nara_ui_render::UiPresentationOutcome::ContentRejected { attempt, .. }]
                if attempt.admission_id().unwrap().get() == 1,
        ));
    }

    #[cfg(feature = "stock-ui")]
    #[test]
    fn close_returns_policy_ui_pair_permissions_once_before_or_after_seal() {
        let installation = crate::WgpuPolicyDefinition::new(crate::WgpuStockPolicy::default())
            .install()
            .unwrap();
        let packet = ui_packet();
        for seal in [false, true] {
            let mut outcomes = UiPresentationOutcomes::default();
            let submissions = prepare_ui_submissions(&mut outcomes, &packet, 1);
            let mut slot = WgpuFramePacketSlot::default();
            slot.reset();
            assert!(
                slot.collect_policy_with_ui(
                    installation
                        .accept(Arc::new(crate::WgpuStockCapture::default()))
                        .unwrap(),
                    WgpuUiSubmissionState::Prepared(submissions),
                )
                .is_ok()
            );
            if seal {
                slot.seal(Ok(Some(image_packet(1))), 7, Instant::now());
            }
            slot.close();
            slot.close();
            slot.reset();

            assert!(slot.capture.is_none());
            assert!(slot.ui_submissions.is_none());
            assert!(slot.take_batch().is_none());
            let mut world = World::new();
            world.insert_resource(outcomes);
            assert!(matches!(
                nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap().as_slice(),
                [nara_ui_render::UiPresentationOutcome::Aborted { attempt, .. }]
                    if attempt.admission_id().unwrap().get() == 1,
            ));
            assert!(
                nara_ui_render::drain_ui_presentation_outcomes(&mut world)
                    .unwrap()
                    .is_empty()
            );
        }
    }

    #[cfg(feature = "stock-ui")]
    #[test]
    fn editor_batch_takes_the_original_ui_sidecar_once() {
        let packet = ui_packet();
        let mut outcomes = UiPresentationOutcomes::default();
        let submissions = prepare_ui_submissions(&mut outcomes, &packet, 1);
        let mut slot = WgpuFramePacketSlot::default();
        slot.reset();
        assert!(
            slot.collect_policy_with_ui(
                empty_policy_capture(),
                WgpuUiSubmissionState::Prepared(submissions)
            )
            .is_ok()
        );
        assert!(
            slot.collect_policy_with_ui(
                empty_policy_capture(),
                WgpuUiSubmissionState::Prepared(Vec::new())
            )
            .is_err()
        );
        slot.seal(Ok(None), 7, Instant::now());
        slot.reset();
        assert!(!slot.accepts_ui_collection());
        assert!(slot.collection_error.is_none());

        let mut world = World::new();
        world.insert_resource(slot);
        world.insert_resource(outcomes);
        let batch = take_captured_policy_batch(&mut world).unwrap();
        assert!(take_captured_policy_batch(&mut world).is_none());
        let (captured, ui) = batch.into_parts();
        assert!(matches!(captured, Ok(None)));
        assert!(matches!(
            nara_ui_render::drain_ui_presentation_outcomes(&mut world),
            Err(UiPresentationIntegrationError::OutstandingBackendOutcomeReservations { count: 1 })
        ));
        drop(ui);

        let published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        assert!(matches!(
            published.as_slice(),
            [nara_ui_render::UiPresentationOutcome::Aborted { attempt, .. }]
                if attempt.admission_id().unwrap().get() == 1
        ));
    }

    #[cfg(feature = "stock-ui")]
    #[test]
    fn reset_preserves_an_unsealed_atomic_policy_and_original_ui_pair() {
        let packet = ui_packet();
        let mut outcomes = UiPresentationOutcomes::default();
        let submissions = prepare_ui_submissions(&mut outcomes, &packet, 1);
        let mut slot = WgpuFramePacketSlot::default();
        slot.reset();
        assert!(
            slot.collect_policy_with_ui(
                empty_policy_capture(),
                WgpuUiSubmissionState::Prepared(submissions)
            )
            .is_ok()
        );

        slot.reset();

        assert!(!slot.accepts_ui_collection());
        assert!(slot.ui_submissions.is_some());
        assert!(slot.capture.is_some());
        assert!(slot.collection_error.is_none());
        slot.seal(Ok(None), 7, Instant::now());
        let batch = slot.take_batch().unwrap();
        assert!(matches!(batch.captured, Ok(None)));
        let submissions = batch.ui.unwrap();
        assert_eq!(submissions.submission_count(), 1);
        slot.close();

        let mut world = nara_ecs::World::new();
        world.insert_resource(outcomes);
        assert!(matches!(
            nara_ui_render::drain_ui_presentation_outcomes(&mut world),
            Err(UiPresentationIntegrationError::OutstandingBackendOutcomeReservations { count: 1 })
        ));

        drop(submissions);

        let published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        assert!(matches!(
            published.as_slice(),
            [nara_ui_render::UiPresentationOutcome::Aborted { attempt, .. }]
                if attempt.admission_id().unwrap().get() == 1
        ));
    }

    #[cfg(feature = "stock-ui")]
    #[test]
    fn close_returns_untransferred_ui_permissions_to_the_original_owner_once() {
        let packet = ui_packet();
        let mut outcomes = UiPresentationOutcomes::default();
        let submissions = prepare_ui_submissions(&mut outcomes, &packet, 2);
        let mut slot = WgpuFramePacketSlot::default();
        slot.reset();
        assert!(
            slot.collect_policy_with_ui(
                empty_policy_capture(),
                WgpuUiSubmissionState::Prepared(submissions)
            )
            .is_ok()
        );

        slot.close();
        slot.close();

        assert!(slot.ui_submissions.is_none());
        assert!(slot.take_batch().is_none());
        let mut world = nara_ecs::World::new();
        world.insert_resource(outcomes);
        let published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        assert_eq!(published.len(), 2);
        for (index, outcome) in published.iter().enumerate() {
            assert!(matches!(
                outcome,
                nara_ui_render::UiPresentationOutcome::Aborted { attempt, .. }
                    if attempt.admission_id().unwrap().get() == u64::try_from(index + 1).unwrap()
            ));
        }
        assert!(
            nara_ui_render::drain_ui_presentation_outcomes(&mut world)
                .unwrap()
                .is_empty()
        );
    }

    #[cfg(feature = "stock-ui")]
    #[test]
    fn completion_appends_to_reserved_storage_and_preserves_exact_submission_authority() {
        use nara_ui_render::UiPresentationOutcome;

        let packet = ui_packet();
        let packet_identity = packet.identity();
        let backend_generation = UiPresentationBackendGeneration::new(17, 3).unwrap();
        let mut outcomes = UiPresentationOutcomes::default();
        let submissions = prepare_ui_submissions(&mut outcomes, &packet, 3);
        let mut submitted = nara_ui_render::__private::bind_backend_submission_batch(
            submissions,
            backend_generation,
        )
        .unwrap();
        let deferred = WgpuUiSubmissionState::Submitted(submitted.split_off(1));
        let presented = WgpuUiSubmissionState::Submitted(submitted);
        let mut completed = Vec::with_capacity(3);
        let original_allocation = completed.as_ptr();
        let original_capacity = completed.capacity();

        let presentation =
            ui_completion(&presented, &Ok(()), RenderFrameState::Submitted, None, None);
        complete_ui_submissions_into(presented, presentation, &mut completed);
        let deferral = ui_completion(
            &deferred,
            &Err(WgpuRenderError::UploadDeferred {
                message: "the shared upload budget is exhausted".to_owned(),
            }),
            RenderFrameState::Skipped,
            None,
            None,
        );
        complete_ui_submissions_into(deferred, deferral, &mut completed);

        assert_eq!(completed.len(), 3);
        assert_eq!(completed.as_ptr(), original_allocation);
        assert_eq!(completed.capacity(), original_capacity);
        for completion in completed {
            outcomes.commit_backend(completion);
        }
        let mut world = nara_ecs::World::new();
        world.insert_resource(outcomes);
        let published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        assert_eq!(published.len(), 3);
        let UiPresentationOutcome::Presented(commit) = &published[0] else {
            panic!("the first reserved result must retain its original presentation authority");
        };
        assert_eq!(commit.attempt().admission_id().unwrap().get(), 1);
        assert_eq!(commit.backend_generation(), backend_generation);
        assert_eq!(commit.packet().identity(), packet_identity);
        for (index, outcome) in published.iter().enumerate().skip(1) {
            let UiPresentationOutcome::Deferred { retry, .. } = outcome else {
                panic!("appending a deferral must not replace the original completed result");
            };
            assert_eq!(
                retry.attempt().admission_id().unwrap().get(),
                u64::try_from(index + 1).unwrap()
            );
            assert_eq!(retry.backend_generation(), backend_generation);
            assert_eq!(retry.packet().identity(), packet_identity);
        }
    }

    #[cfg(feature = "stock-ui")]
    #[test]
    fn cancellation_returns_prepared_abort_and_bound_retry_while_unbound_deferral_stays_a_fault() {
        use nara_ui_render::UiPresentationOutcome;

        let packet = ui_packet();
        let generation = UiPresentationBackendGeneration::new(17, 3).unwrap();
        let mut outcomes = UiPresentationOutcomes::default();
        let mut prepared = prepare_ui_submissions(&mut outcomes, &packet, 3);
        let unbound_deferral = WgpuUiSubmissionState::Prepared(prepared.split_off(2));
        let bound_cancellation = WgpuUiSubmissionState::Submitted(
            nara_ui_render::__private::bind_backend_submission_batch(
                prepared.split_off(1),
                generation,
            )
            .unwrap(),
        );
        let prepared_cancellation = WgpuUiSubmissionState::Prepared(prepared);
        let message = "the original presentation was interrupted";
        let mut completed = Vec::with_capacity(3);
        let allocation = completed.as_ptr();
        let capacity = completed.capacity();

        complete_ui_submissions_into(
            prepared_cancellation,
            WgpuUiCompletion::Cancelled(message.to_owned()),
            &mut completed,
        );
        complete_ui_submissions_into(
            bound_cancellation,
            WgpuUiCompletion::Cancelled(message.to_owned()),
            &mut completed,
        );
        complete_ui_submissions_into(
            unbound_deferral,
            WgpuUiCompletion::Deferred("the unbound backend is unavailable".to_owned()),
            &mut completed,
        );

        assert_eq!(completed.len(), 3);
        assert_eq!(completed.as_ptr(), allocation);
        assert_eq!(completed.capacity(), capacity);
        for completion in completed {
            outcomes.commit_backend(completion);
        }
        let mut world = World::new();
        world.insert_resource(outcomes);
        let published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        assert!(matches!(
            published.as_slice(),
            [
                UiPresentationOutcome::Aborted { attempt, message: aborted_message },
                UiPresentationOutcome::Deferred { retry, message: retry_message },
                UiPresentationOutcome::BackendFault {
                    attempt: Some(fault_attempt),
                    backend_generation: None,
                    domain: UiPresentationBackendFaultDomain::Target,
                    ..
                },
            ] if attempt.admission_id().unwrap().get() == 1
                && attempt.packet() == packet.identity()
                && aborted_message == message
                && retry.attempt().admission_id().unwrap().get() == 2
                && retry.backend_generation() == generation
                && Arc::ptr_eq(retry.packet(), &packet)
                && retry_message == message
                && fault_attempt.admission_id().unwrap().get() == 3,
        ));
        assert!(
            nara_ui_render::drain_ui_presentation_outcomes(&mut world)
                .unwrap()
                .is_empty()
        );
    }

    #[cfg(feature = "stock-ui")]
    #[test]
    fn dropping_completed_storage_returns_every_original_permission_as_aborted() {
        let packet = ui_packet();
        let mut outcomes = UiPresentationOutcomes::default();
        let submissions = prepare_ui_submissions(&mut outcomes, &packet, 2);
        let mut submissions = WgpuUiSubmissionState::Prepared(submissions);
        submissions
            .bind_backend(UiPresentationBackendGeneration::new(17, 3).unwrap())
            .unwrap();
        let mut completed = Vec::with_capacity(submissions.submission_count());

        complete_ui_submissions_into(submissions, WgpuUiCompletion::Presented, &mut completed);
        drop(completed);

        let mut world = nara_ecs::World::new();
        world.insert_resource(outcomes);
        let published = nara_ui_render::drain_ui_presentation_outcomes(&mut world).unwrap();
        assert_eq!(published.len(), 2);
        for (index, outcome) in published.iter().enumerate() {
            assert!(matches!(
                outcome,
                nara_ui_render::UiPresentationOutcome::Aborted { attempt, .. }
                    if attempt.admission_id().unwrap().get() == u64::try_from(index + 1).unwrap()
            ));
        }
    }

    fn image_packet(frame_index: u64) -> RenderFramePacket {
        use nara_core::{Color, Mat3};
        use nara_ecs::Entity;
        use nara_render::{
            Extent2d, ExtractedView, ExtractedViews, RenderFrameSourceGeneration, RenderImage2d,
            RenderImagePacket, RenderTarget, ViewLoadOp, ViewMapping2d, ViewportRect,
            build_image_render_frame_packet,
        };

        let target = RenderImagePacket::new(
            RenderImage2d::allocate_handle().unwrap(),
            Extent2d::new(2, 2).unwrap(),
        )
        .unwrap();
        let viewport = ViewportRect::from_extent(target.extent()).unwrap();
        let mut views = ExtractedViews::default();
        views.push(ExtractedView {
            id: nara_render::RenderViewId::try_acquire().unwrap(),
            camera_entity: Entity::PLACEHOLDER,
            target: RenderTarget::Image(target.image()),
            viewport,
            mapping: ViewMapping2d::orthographic(Mat3::IDENTITY, 2.0, viewport).unwrap(),
            order: 0,
            load: ViewLoadOp::Clear(Color::BLACK),
        });
        build_image_render_frame_packet(
            RenderFrameSourceGeneration::try_acquire().unwrap(),
            frame_index,
            &views,
            target,
        )
        .unwrap()
        .unwrap()
    }

    #[cfg(feature = "stock-ui")]
    fn prepare_ui_submissions(
        outcomes: &mut UiPresentationOutcomes,
        packet: &Arc<UiPaintPacket>,
        count: usize,
    ) -> Vec<UiBackendSubmission> {
        use nara_ui_render::{
            UiPresentationAttemptId, UiPresentationCapabilityGeneration, UiPresentationEpoch,
            UiPresentationRuntimeGeneration,
        };

        let submissions: Vec<_> = (1..=count)
            .map(|admission_id| {
                nara_ui_render::__private::prepare_backend_submission(
                    UiPresentationAttemptId::new(u64::try_from(admission_id).unwrap()).unwrap(),
                    Arc::clone(packet),
                    UiPresentationEpoch::new(3).unwrap(),
                    UiPresentationRuntimeGeneration::new(7).unwrap(),
                    UiPresentationCapabilityGeneration::new(11).unwrap(),
                )
            })
            .collect();
        let permits = outcomes.reserve_backend(&submissions).unwrap();
        submissions
            .into_iter()
            .zip(permits)
            .map(|(submission, permit)| UiBackendSubmission::pair(submission, permit).unwrap())
            .collect()
    }

    #[cfg(feature = "stock-ui")]
    fn ui_packet() -> Arc<UiPaintPacket> {
        use nara_ui_kernel::{
            UiAuthoringSnapshotBuilder, UiColor, UiCompiler, UiEnvironment, UiLayer, UiLocalKey,
            UiNode, UiRoot, UiRootKey, UiStyle, UiSurfaceId, UiTheme, UiVisual,
        };

        let snapshot = UiAuthoringSnapshotBuilder::new(1)
            .with_root(
                UiRoot::new(
                    UiRootKey::runtime("wgpu-frame-completion", "root").unwrap(),
                    UiLayer::Base,
                    UiStyle::fill(),
                )
                .with_child(
                    UiNode::new(UiStyle::absolute(0.0, 0.0, 16.0, 16.0))
                        .with_key(UiLocalKey::runtime("wgpu-frame-completion", "panel").unwrap())
                        .with_visual(UiVisual::solid(UiColor::rgba(1.0, 1.0, 1.0, 1.0))),
                ),
            )
            .build()
            .unwrap();
        let environment =
            UiEnvironment::new(UiSurfaceId::new(1).unwrap(), 16.0, 16.0, 1.0, 1, 1, 1, 1).unwrap();
        let frame = UiCompiler::new(nara_text::bundled_font_face().unwrap(), Default::default())
            .unwrap()
            .compile(&snapshot, environment, UiTheme::default())
            .unwrap();
        nara_ui_render::UiPresentationPlanner::default()
            .plan(
                frame,
                RenderTarget::Window(WindowId::PRIMARY),
                &nara_ui_render::UiPreparedImages::default(),
            )
            .unwrap()
    }
}
