//! Resource-complete backend-neutral paint packets for immutable UI frames.

// Bevy derives resolve this path; Nara's ECS facade remains the actual dependency.
mod atlas;
mod integration;
mod limits;
mod packet;
mod planner;

pub use integration::{
    UI_RENDER_PLUGIN_DECLARATION, UI_RENDER_PLUGIN_ID, UiPaintCompileStats,
    UiPresentationIntegrationError, UiRenderPlugin, compile_ui_paint_packets,
    drain_ui_presentation_outcomes, plugin,
};
pub use limits::{UiPresentationLimits, UiPresentationLimitsError};
pub use packet::{
    OneShotPresentationCommit, UiBackendSubmissionIdentity, UiGlyphAtlasPageKey,
    UiGlyphAtlasPageSnapshot, UiPacketBorder, UiPacketFill, UiPacketImage, UiPacketPrimitive,
    UiPacketRect, UiPacketUv, UiPaintPacket, UiPaintPacketIdentity, UiPresentationAttemptId,
    UiPresentationAttemptIdentity, UiPresentationBackendFaultDomain,
    UiPresentationBackendGeneration, UiPresentationCapabilityGeneration, UiPresentationEpoch,
    UiPresentationOutcome, UiPresentationRetryIdentity, UiPresentationRuntimeGeneration,
};
pub use planner::{
    UiPreparedImages, UiPresentationContentError, UiPresentationPlanner,
    UiPresentationPlannerCreateError,
};

#[doc(hidden)]
pub mod __private {
    use std::sync::Arc;

    pub use crate::integration::{
        UiBackendOutcomePermit, UiBackendPresentationFrame, UiBackendSubmission,
        UiBackendSubmissionBatchBindError, UiBackendSubmissionReader, UiCompletedBackendOutcome,
        UiPaintPacketSnapshots, UiPresentationOutcomes, UiPresentationRoute,
        UiPresentationRouteError, UiSubmittedBackendSubmission, abort_ui_planned_packet,
        bind_backend_submission_batch, commit_ui_backend_outcome, commit_ui_backend_outcomes,
        commit_ui_backend_presentation, discard_ui_backend_acceptance, replace_ui_backend_frame,
        replace_ui_presentation_routes, snapshot_ui_paint_packets,
        take_ui_backend_presentation_frame,
    };
    pub use crate::planner::UiPlannedPacketAbortOutcome;

    pub use crate::packet::{
        UiAcceptedBackendPresentation, UiDeferredBackendPresentation, UiPreparedBackendSubmission,
        UiSubmittedBackendPresentation,
    };

    use crate::{
        UiBackendSubmissionIdentity, UiPaintPacket, UiPresentationAttemptId,
        UiPresentationAttemptIdentity, UiPresentationCapabilityGeneration, UiPresentationEpoch,
        UiPresentationOutcome, UiPresentationRuntimeGeneration,
    };

    #[must_use]
    pub fn presentation_attempt(packet: &UiPaintPacket) -> UiPresentationAttemptIdentity {
        UiPresentationAttemptIdentity::routed(packet.identity(), packet.target())
    }

    #[must_use]
    pub fn prepare_backend_submission(
        admission_id: UiPresentationAttemptId,
        packet: Arc<UiPaintPacket>,
        presentation_epoch: UiPresentationEpoch,
        runtime_generation: UiPresentationRuntimeGeneration,
        capability_generation: UiPresentationCapabilityGeneration,
    ) -> UiPreparedBackendSubmission {
        UiPreparedBackendSubmission::new(
            admission_id,
            packet,
            presentation_epoch,
            runtime_generation,
            capability_generation,
        )
    }

    #[must_use]
    pub const fn prepared_attempt(
        submission: &UiPreparedBackendSubmission,
    ) -> UiPresentationAttemptIdentity {
        UiPresentationAttemptIdentity::backend(submission.identity())
    }

    #[must_use]
    pub const fn submitted_attempt(
        identity: UiBackendSubmissionIdentity,
    ) -> UiPresentationAttemptIdentity {
        UiPresentationAttemptIdentity::backend(identity)
    }

    #[must_use]
    pub fn retry_deferred(deferred: UiDeferredBackendPresentation) -> UiPreparedBackendSubmission {
        deferred.into_retry()
    }

    #[must_use]
    pub fn discard_prepared(
        submission: UiPreparedBackendSubmission,
    ) -> UiPresentationAttemptIdentity {
        submission.into_attempt()
    }

    #[must_use]
    pub fn abort_prepared(
        submission: UiPreparedBackendSubmission,
        message: String,
    ) -> UiPresentationOutcome {
        UiPresentationOutcome::Aborted {
            attempt: submission.into_attempt(),
            message,
        }
    }

    #[must_use]
    pub fn supersede_prepared(
        submission: UiPreparedBackendSubmission,
        successor: &UiPreparedBackendSubmission,
        message: String,
    ) -> UiPresentationOutcome {
        UiPresentationOutcome::Superseded {
            attempt: submission.into_attempt(),
            successor: prepared_attempt(successor),
            message,
        }
    }
}

pub mod prelude {
    pub use crate::UiRenderPlugin;
}

/// Explicit Host/backend integration surface kept out of ordinary gameplay imports.
pub mod advanced {
    pub use crate::{
        OneShotPresentationCommit, UiGlyphAtlasPageKey, UiGlyphAtlasPageSnapshot, UiPacketBorder,
        UiPacketFill, UiPacketImage, UiPacketPrimitive, UiPacketRect, UiPacketUv,
        UiPaintCompileStats, UiPaintPacket, UiPaintPacketIdentity, UiPreparedImages,
        UiPresentationAttemptId, UiPresentationAttemptIdentity, UiPresentationBackendFaultDomain,
        UiPresentationBackendGeneration, UiPresentationCapabilityGeneration,
        UiPresentationContentError, UiPresentationEpoch, UiPresentationIntegrationError,
        UiPresentationLimits, UiPresentationLimitsError, UiPresentationOutcome,
        UiPresentationPlanner, UiPresentationPlannerCreateError, UiPresentationRetryIdentity,
        UiPresentationRuntimeGeneration, compile_ui_paint_packets, drain_ui_presentation_outcomes,
    };
}

#[cfg(test)]
mod packet_tests;
