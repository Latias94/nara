use std::time::Instant;

#[cfg(feature = "ui-submitter")]
use std::sync::Arc;

use nara_ecs::{Resource, SystemSet};
#[cfg(feature = "ui-submitter")]
use nara_render::RenderFrameState;
#[cfg(any(feature = "ui-submitter", all(test, feature = "sprite-submitter")))]
use nara_render::RenderTarget;
#[cfg(all(test, any(feature = "sprite-submitter", feature = "ui-submitter")))]
use nara_render::ViewportRect;
use nara_render::{ExtractedViews, RenderFramePacket, RenderFramePacketError};
#[cfg(feature = "sprite-submitter")]
use nara_sprite_render::__private::SpriteBatches;
#[cfg(all(test, feature = "ui-submitter"))]
use nara_ui_render::__private::UiPresentationOutcomes;
#[cfg(feature = "ui-submitter")]
use nara_ui_render::__private::{
    UiBackendSubmission, UiCompletedBackendOutcome, UiSubmittedBackendSubmission,
};
#[cfg(feature = "ui-submitter")]
use nara_ui_render::{
    UiPacketPrimitive, UiPaintPacket, UiPaintPacketIdentity, UiPresentationAttemptIdentity,
    UiPresentationBackendFaultDomain, UiPresentationBackendGeneration,
    UiPresentationIntegrationError,
};
#[cfg(feature = "ui-submitter")]
use nara_window::WindowId;

#[cfg(any(feature = "ui-submitter", all(test, feature = "sprite-submitter")))]
use crate::WgpuRenderError;
#[cfg(all(test, any(feature = "sprite-submitter", feature = "ui-submitter")))]
use crate::quad::WgpuQuadBatch;
#[cfg(all(test, feature = "ui-submitter"))]
use crate::ui::WgpuLoweredUiPacketFacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub(crate) enum WgpuFrameSet {
    Reset,
    CollectSprite,
    CollectUi,
    Seal,
}

#[derive(Debug, Default, Clone)]
pub(crate) struct WgpuFramePayload {
    #[cfg(feature = "ui-submitter")]
    pub(crate) primary_window_id: Option<WindowId>,
    #[cfg(feature = "ui-submitter")]
    pub(crate) ui_logical_target: Option<RenderTarget>,
    #[cfg(feature = "sprite-submitter")]
    pub(crate) sprite_batches: Option<SpriteBatches>,
    #[cfg(feature = "ui-submitter")]
    pub(crate) ui_packets: Vec<Arc<UiPaintPacket>>,
}

#[cfg(feature = "ui-submitter")]
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
    pub(crate) topology: RenderFramePacket,
    pub(crate) payload: WgpuFramePayload,
    pub(crate) app_frame_index: u64,
    pub(crate) work_started_at: Instant,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum WgpuFrameCaptureError {
    #[error("managed runtime generation is unavailable")]
    MissingRuntimeGeneration,
    #[cfg(feature = "ui-submitter")]
    #[error("the previous UI frame transaction did not reach terminal completion")]
    IncompleteUiTransaction,
    #[error(transparent)]
    Topology(#[from] RenderFramePacketError),
    #[error("a render batch does not belong to the admitted view and target")]
    BatchTopologyMismatch,
    #[cfg(feature = "ui-submitter")]
    #[error(transparent)]
    UiPresentation(#[from] UiPresentationIntegrationError),
}

#[cfg(feature = "ui-submitter")]
#[derive(Debug)]
pub(crate) enum WgpuUiSubmissionState {
    Prepared(Vec<UiBackendSubmission>),
    Submitted(Vec<UiSubmittedBackendSubmission>),
}

#[cfg(feature = "ui-submitter")]
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

#[cfg(feature = "ui-submitter")]
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

#[cfg(feature = "ui-submitter")]
pub(crate) enum WgpuUiCompletion {
    Presented,
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

#[cfg(feature = "ui-submitter")]
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
#[cfg(feature = "ui-submitter")]
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

#[derive(Debug, Default, Resource)]
pub(crate) struct WgpuFramePacketSlot {
    payload: WgpuFramePayload,
    collection_error: Option<WgpuFrameCaptureError>,
    current: Option<Result<Option<WgpuCapturedFrame>, WgpuFrameCaptureError>>,
    #[cfg(feature = "ui-submitter")]
    ui_submissions: Option<WgpuUiSubmissionState>,
}

impl WgpuFramePacketSlot {
    pub(crate) fn close(&mut self) {
        // Return every original UI permission before the Runtime's logical close barrier.
        // Their original pre-admitted result storage remains alive independently of this slot.
        self.payload = WgpuFramePayload::default();
        self.current = None;
        self.collection_error = None;
        #[cfg(feature = "ui-submitter")]
        {
            self.ui_submissions = None;
        }
    }

    pub(crate) fn reset(&mut self) {
        #[cfg(feature = "ui-submitter")]
        if self.ui_submissions.is_some() {
            self.payload = WgpuFramePayload::default();
            self.current = None;
            self.collection_error = Some(WgpuFrameCaptureError::IncompleteUiTransaction);
            return;
        }
        self.payload = WgpuFramePayload::default();
        self.collection_error = None;
        self.current = None;
        #[cfg(feature = "ui-submitter")]
        {
            self.ui_submissions = None;
        }
    }

    #[cfg(feature = "sprite-submitter")]
    pub(crate) fn collect_sprite(&mut self, batches: SpriteBatches) {
        self.payload.sprite_batches = Some(batches);
    }

    #[cfg(all(test, feature = "sprite-submitter"))]
    pub(crate) fn collected_sprite_instances(&self) -> Option<usize> {
        self.payload
            .sprite_batches
            .as_ref()
            .map(SpriteBatches::total_instances)
    }

    #[cfg(feature = "ui-submitter")]
    pub(crate) fn accepts_ui_collection(&self) -> bool {
        self.collection_error.is_none() && self.ui_submissions.is_none()
    }

    #[cfg(feature = "ui-submitter")]
    pub(crate) fn collect_ui(
        &mut self,
        packets: Vec<Arc<UiPaintPacket>>,
        submissions: Vec<UiBackendSubmission>,
    ) {
        self.payload.ui_packets = packets;
        self.ui_submissions = Some(WgpuUiSubmissionState::Prepared(submissions));
    }

    #[cfg(feature = "ui-submitter")]
    pub(crate) fn reject_ui_collection(&mut self, error: UiPresentationIntegrationError) {
        self.collection_error = Some(WgpuFrameCaptureError::UiPresentation(error));
    }

    pub(crate) fn seal(
        &mut self,
        #[cfg(feature = "ui-submitter")] primary_window_id: Option<WindowId>,
        captured: Result<Option<RenderFramePacket>, RenderFramePacketError>,
        app_frame_index: u64,
        work_started_at: Instant,
    ) {
        if let Some(error) = self.collection_error.take() {
            self.current = Some(Err(error));
            return;
        }
        #[cfg(feature = "ui-submitter")]
        {
            self.payload.primary_window_id = primary_window_id;
        }
        let payload = std::mem::take(&mut self.payload);
        self.current = Some(
            captured
                .map(|topology| {
                    topology.map(|topology| WgpuCapturedFrame {
                        topology,
                        payload,
                        app_frame_index,
                        work_started_at,
                    })
                })
                .map_err(WgpuFrameCaptureError::from),
        );
    }

    pub(crate) fn reject_capture(&mut self, error: WgpuFrameCaptureError) {
        self.current = Some(Err(error));
    }

    pub(crate) fn take_capture(
        &mut self,
    ) -> Option<Result<Option<WgpuCapturedFrame>, WgpuFrameCaptureError>> {
        self.current.take()
    }

    pub(crate) fn payload_matches_topology(
        &self,
        views: &ExtractedViews,
        #[cfg(feature = "ui-submitter")] primary_window_id: Option<WindowId>,
    ) -> bool {
        payload_batches_match_topology(
            &self.payload,
            views,
            #[cfg(feature = "ui-submitter")]
            primary_window_id,
        )
    }

    #[cfg(feature = "ui-submitter")]
    pub(crate) fn take_ui_submissions(&mut self) -> Option<WgpuUiSubmissionState> {
        self.ui_submissions.take()
    }
}

impl WgpuFramePayload {
    #[cfg(feature = "ui-submitter")]
    pub(crate) fn for_editor_authoring_ui(ui_packets: Vec<Arc<UiPaintPacket>>) -> Self {
        Self {
            ui_packets,
            ..Self::default()
        }
    }

    #[cfg(feature = "sprite-submitter")]
    pub(crate) fn for_editor_sprites(sprite_batches: SpriteBatches) -> Self {
        Self {
            sprite_batches: Some(sprite_batches),
            ..Self::default()
        }
    }

    #[cfg(feature = "ui-submitter")]
    pub(crate) fn for_editor_runtime_ui(ui_packets: Vec<Arc<UiPaintPacket>>) -> Self {
        Self {
            primary_window_id: Some(WindowId::PRIMARY),
            ui_logical_target: Some(RenderTarget::Window(WindowId::PRIMARY)),
            ui_packets,
            ..Self::default()
        }
    }

    #[cfg(all(feature = "sprite-submitter", feature = "ui-submitter"))]
    pub(crate) fn for_editor_viewport(
        sprite_batches: SpriteBatches,
        ui_packets: Vec<Arc<UiPaintPacket>>,
    ) -> Self {
        let mut payload = Self::for_editor_runtime_ui(ui_packets);
        payload.sprite_batches = Some(sprite_batches);
        payload
    }

    #[cfg(feature = "ui-submitter")]
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

    #[cfg(feature = "ui-submitter")]
    fn ui_packet_target(&self, execution_target: RenderTarget) -> RenderTarget {
        self.ui_logical_target.unwrap_or(execution_target)
    }

    /// Private legacy lowering oracle for ordered quad readback and Runtime comparison tests.
    #[cfg(all(test, any(feature = "sprite-submitter", feature = "ui-submitter")))]
    pub(crate) fn lower_draw(
        &self,
        target: RenderTarget,
        viewport: ViewportRect,
        view_index: Option<usize>,
    ) -> Result<WgpuLoweredFrameDraw, WgpuRenderError> {
        #[cfg(feature = "sprite-submitter")]
        let content = self
            .sprite_batches
            .as_ref()
            .map_or_else(Vec::new, |batches| {
                view_index.map_or_else(Vec::new, |index| {
                    crate::sprite::collect_sprite_quad_batches(batches, index)
                })
            });
        #[cfg(not(feature = "sprite-submitter"))]
        let content = Vec::new();
        #[cfg(feature = "ui-submitter")]
        let ui_packet_target = self.ui_packet_target(target);
        #[cfg(feature = "ui-submitter")]
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
        #[cfg(not(feature = "ui-submitter"))]
        let ui_batches = {
            let _ = (target, viewport, view_index);
            Vec::new()
        };
        #[cfg(feature = "ui-submitter")]
        let ui_batches = ui.batches;

        Ok(WgpuLoweredFrameDraw {
            batches: nara_render::__private::assemble_first_party_draws(content, ui_batches),
            #[cfg(feature = "ui-submitter")]
            ui_packets: ui.packet_facts,
        })
    }
}

/// Private legacy combined draw used only by the native/unit comparison oracle.
#[cfg(all(test, any(feature = "sprite-submitter", feature = "ui-submitter")))]
#[derive(Debug, Default)]
pub(crate) struct WgpuLoweredFrameDraw {
    pub(crate) batches: Vec<WgpuQuadBatch>,
    #[cfg(feature = "ui-submitter")]
    pub(crate) ui_packets: Vec<WgpuLoweredUiPacketFacts>,
}

fn payload_batches_match_topology(
    payload: &WgpuFramePayload,
    views: &ExtractedViews,
    #[cfg(feature = "ui-submitter")] primary_window_id: Option<WindowId>,
) -> bool {
    let Some(view) = views.as_slice().first() else {
        return true;
    };
    #[cfg(not(any(feature = "sprite-submitter", feature = "ui-submitter")))]
    let _ = (payload, view);
    #[cfg(feature = "sprite-submitter")]
    if payload.sprite_batches.as_ref().is_some_and(|batches| {
        batches.as_slice().iter().any(|batch| {
            views
                .as_slice()
                .get(batch.view_index)
                .is_none_or(|view| batch.target != view.target || batch.view_order != view.order)
        })
    }) {
        return false;
    }
    #[cfg(feature = "ui-submitter")]
    {
        let ui_packet_target = payload.ui_packet_target(view.target);
        if payload.ui_packets.iter().any(|packet| {
            !render_targets_match(packet.target(), ui_packet_target, primary_window_id)
        }) {
            return false;
        }
    }
    true
}

#[cfg(feature = "ui-submitter")]
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

#[cfg(feature = "ui-submitter")]
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

#[cfg(all(test, feature = "ui-submitter"))]
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

#[cfg(feature = "ui-submitter")]
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

    #[cfg(feature = "ui-submitter")]
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

    #[cfg(feature = "sprite-submitter")]
    #[test]
    fn sprite_payload_is_absent_until_the_contribution_collects() {
        let mut slot = WgpuFramePacketSlot::default();
        slot.reset();

        assert!(slot.payload.sprite_batches.is_none());
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn ui_submission_state_is_absent_until_the_contribution_collects() {
        let mut slot = WgpuFramePacketSlot::default();
        slot.reset();

        assert!(slot.take_ui_submissions().is_none());
        assert!(slot.accepts_ui_collection());
        assert!(slot.payload.ui_packets.is_empty());
        assert!(slot.take_capture().is_none());
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn reset_rejects_without_overwriting_an_incomplete_ui_transaction() {
        let packet = ui_packet();
        let mut outcomes = UiPresentationOutcomes::default();
        let submissions = prepare_ui_submissions(&mut outcomes, &packet, 1);
        let mut slot = WgpuFramePacketSlot::default();
        slot.collect_ui(vec![packet], submissions);

        slot.reset();

        assert!(!slot.accepts_ui_collection());
        assert!(slot.ui_submissions.is_some());
        assert!(matches!(
            slot.collection_error,
            Some(WgpuFrameCaptureError::IncompleteUiTransaction)
        ));
        let submissions = slot.take_ui_submissions().unwrap();
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

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn close_returns_untransferred_ui_permissions_to_the_original_owner_once() {
        let packet = ui_packet();
        let mut outcomes = UiPresentationOutcomes::default();
        let submissions = prepare_ui_submissions(&mut outcomes, &packet, 2);
        let mut slot = WgpuFramePacketSlot::default();
        slot.collect_ui(vec![packet], submissions);

        slot.close();
        slot.close();

        assert!(slot.take_ui_submissions().is_none());
        assert!(slot.take_capture().is_none());
        assert!(slot.payload.ui_packets.is_empty());
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

    #[cfg(feature = "ui-submitter")]
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

    #[cfg(feature = "ui-submitter")]
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

    #[cfg(feature = "ui-submitter")]
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

    #[cfg(feature = "ui-submitter")]
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
