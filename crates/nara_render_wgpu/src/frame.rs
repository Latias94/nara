use std::time::Instant;

#[cfg(feature = "ui-submitter")]
use std::sync::Arc;

use nara_ecs::{Resource, SystemSet};
#[cfg(feature = "ui-submitter")]
use nara_render::RenderFrameState;
use nara_render::{ExtractedViews, RenderFramePacket, RenderFramePacketError};
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use nara_render::{RenderTarget, ViewportRect};
#[cfg(feature = "sprite-submitter")]
use nara_sprite_render::__private::SpriteBatches;
#[cfg(feature = "ui-submitter")]
use nara_ui_render::__private::{
    UiBackendSubmission, UiCompletedBackendOutcome, UiPresentationOutcomes,
    UiSubmittedBackendSubmission,
};
#[cfg(feature = "ui-submitter")]
use nara_ui_render::{
    UiPacketPrimitive, UiPaintPacket, UiPaintPacketIdentity, UiPresentationAttemptIdentity,
    UiPresentationBackendFaultDomain, UiPresentationBackendGeneration,
    UiPresentationIntegrationError,
};
#[cfg(feature = "ui-submitter")]
use nara_window::WindowId;

#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::WgpuRenderError;
#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
use crate::quad::WgpuQuadBatch;
#[cfg(feature = "ui-submitter")]
use crate::ui::WgpuLoweredUiPacketFacts;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, SystemSet)]
pub(crate) enum WgpuFrameSet {
    Reset,
    CollectSprite,
    CollectUi,
    Seal,
    Submit,
    CompleteUi,
}

#[derive(Debug, Default)]
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
    match (submissions, completion) {
        (WgpuUiSubmissionState::Submitted(submissions), WgpuUiCompletion::Presented) => {
            submissions
                .into_iter()
                .map(UiSubmittedBackendSubmission::presented)
                .collect()
        }
        (WgpuUiSubmissionState::Prepared(submissions), WgpuUiCompletion::Presented) => submissions
            .into_iter()
            .map(|submission| {
                submission.backend_fault(
                    None,
                    UiPresentationBackendFaultDomain::Target,
                    "the Editor window reached a composite without binding Runtime UI to a live backend generation"
                        .to_owned(),
                )
            })
            .collect(),
        (WgpuUiSubmissionState::Submitted(submissions), WgpuUiCompletion::Deferred(message)) => {
            submissions
                .into_iter()
                .map(|submission| submission.deferred(message.clone()))
                .collect()
        }
        (WgpuUiSubmissionState::Prepared(submissions), WgpuUiCompletion::Deferred(message)) => {
            submissions
                .into_iter()
                .map(|submission| {
                    submission.backend_fault(
                        None,
                        UiPresentationBackendFaultDomain::Target,
                        message.clone(),
                    )
                })
                .collect()
        }
        (
            WgpuUiSubmissionState::Prepared(submissions),
            WgpuUiCompletion::ContentRejected(message),
        ) => submissions
            .into_iter()
            .map(|submission| submission.content_rejected(message.clone()))
            .collect(),
        (
            WgpuUiSubmissionState::Submitted(submissions),
            WgpuUiCompletion::ContentRejected(message),
        ) => submissions
            .into_iter()
            .map(|submission| {
                submission.backend_fault(UiPresentationBackendFaultDomain::Attempt, message.clone())
            })
            .collect(),
        (
            WgpuUiSubmissionState::Prepared(submissions),
            WgpuUiCompletion::BackendFault {
                backend_generation,
                domain,
                message,
            },
        ) => submissions
            .into_iter()
            .map(|submission| {
                submission.backend_fault(backend_generation, domain, message.clone())
            })
            .collect(),
        (
            WgpuUiSubmissionState::Submitted(submissions),
            WgpuUiCompletion::BackendFault {
                domain, message, ..
            },
        ) => submissions
            .into_iter()
            .map(|submission| submission.backend_fault(domain, message.clone()))
            .collect(),
        (
            WgpuUiSubmissionState::Prepared(submissions),
            WgpuUiCompletion::Superseded { successor, message },
        ) => submissions
            .into_iter()
            .map(|submission| submission.superseded(successor, message.clone()))
            .collect(),
        (
            WgpuUiSubmissionState::Submitted(submissions),
            WgpuUiCompletion::Superseded { successor, message },
        ) => submissions
            .into_iter()
            .map(|submission| submission.superseded(successor, message.clone()))
            .collect(),
    }
}

#[cfg(feature = "ui-submitter")]
#[derive(Debug)]
pub(crate) struct WgpuUiTerminal {
    submissions: WgpuUiSubmissionState,
    result: Result<(), WgpuRenderError>,
    frame_state: RenderFrameState,
    device_fault_generation: Option<UiPresentationBackendGeneration>,
    skip_message: Option<String>,
}

#[derive(Debug, Default, Resource)]
pub(crate) struct WgpuFramePacketSlot {
    payload: WgpuFramePayload,
    collection_error: Option<WgpuFrameCaptureError>,
    current: Option<Result<Option<WgpuCapturedFrame>, WgpuFrameCaptureError>>,
    #[cfg(feature = "ui-submitter")]
    ui_submissions: Option<WgpuUiSubmissionState>,
    #[cfg(feature = "ui-submitter")]
    ui_terminal: Option<WgpuUiTerminal>,
}

impl WgpuFramePacketSlot {
    pub(crate) fn reset(&mut self) {
        #[cfg(feature = "ui-submitter")]
        if self.ui_submissions.is_some() || self.ui_terminal.is_some() {
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
            self.ui_terminal = None;
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
        self.collection_error.is_none()
            && self.ui_submissions.is_none()
            && self.ui_terminal.is_none()
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

    #[cfg(feature = "ui-submitter")]
    pub(crate) fn finish_ui(
        &mut self,
        submissions: WgpuUiSubmissionState,
        result: Result<(), WgpuRenderError>,
        frame_state: RenderFrameState,
        device_fault_generation: Option<UiPresentationBackendGeneration>,
        skip_message: Option<String>,
    ) {
        self.ui_terminal = Some(WgpuUiTerminal {
            submissions,
            result,
            frame_state,
            device_fault_generation,
            skip_message,
        });
    }

    #[cfg(feature = "ui-submitter")]
    pub(crate) fn complete_ui(&mut self, outcomes: &mut UiPresentationOutcomes) {
        let Some(terminal) = self.ui_terminal.take() else {
            return;
        };
        publish_ui_outcomes(
            outcomes,
            terminal.submissions,
            &terminal.result,
            terminal.frame_state,
            terminal.device_fault_generation,
            terminal.skip_message.as_deref(),
        );
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

    #[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
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

#[cfg(any(feature = "sprite-submitter", feature = "ui-submitter"))]
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
pub(crate) fn publish_ui_outcomes(
    outcomes: &mut UiPresentationOutcomes,
    submissions: WgpuUiSubmissionState,
    result: &Result<(), WgpuRenderError>,
    frame_state: RenderFrameState,
    device_fault_generation: Option<UiPresentationBackendGeneration>,
    skip_message: Option<&str>,
) {
    let completion = match (&submissions, result) {
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
    };
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
        assert!(slot.ui_terminal.is_none());
    }

    #[cfg(feature = "ui-submitter")]
    #[test]
    fn reset_rejects_without_overwriting_an_incomplete_ui_transaction() {
        let mut slot = WgpuFramePacketSlot {
            ui_submissions: Some(WgpuUiSubmissionState::Prepared(Vec::new())),
            ..Default::default()
        };

        slot.reset();

        assert!(!slot.accepts_ui_collection());
        assert!(slot.ui_submissions.is_some());
        assert!(matches!(
            slot.collection_error,
            Some(WgpuFrameCaptureError::IncompleteUiTransaction)
        ));
    }
}
