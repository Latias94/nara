//! Resource-complete backend-neutral paint packets for immutable UI frames.

// Bevy derives resolve this path; Nara's ECS facade remains the actual dependency.
extern crate nara_ecs as bevy_ecs;

mod atlas;
mod integration;
mod limits;
mod packet;
mod planner;

pub use integration::{
    UI_RENDER_PLUGIN_DECLARATION, UI_RENDER_PLUGIN_ID, UiPaintCompileStats,
    UiPresentationIntegrationError, UiPresentationRoute, UiPresentationRouteError, UiRenderPlugin,
    compile_ui_paint_packets, drain_ui_presentation_outcomes, install_ui_presentation_route,
    retire_ui_presentation_route,
};
pub use limits::{UiPresentationLimits, UiPresentationLimitsError};
pub use packet::{
    OneShotPresentationCommit, UiGlyphAtlasPageKey, UiGlyphAtlasPageSnapshot, UiPacketBorder,
    UiPacketFill, UiPacketImage, UiPacketPrimitive, UiPacketRect, UiPacketUv, UiPaintPacket,
    UiPaintPacketIdentity, UiPresentationAttemptIdentity, UiPresentationBackendGeneration,
    UiPresentationOutcome, UiPresentationRetryIdentity,
};
pub use planner::{
    UiPreparedImages, UiPresentationContentError, UiPresentationPlanner,
    UiPresentationPlannerCreateError,
};

#[doc(hidden)]
pub mod __private {
    use std::sync::Arc;

    pub use crate::integration::{
        UiPaintPacketReader, UiPresentationOutcomes, replace_ui_presentation_routes,
    };

    use crate::{
        OneShotPresentationCommit, UiPaintPacket, UiPresentationAttemptIdentity,
        UiPresentationBackendGeneration, UiPresentationOutcome, UiPresentationRetryIdentity,
    };

    #[must_use]
    pub fn presentation_attempt(packet: &UiPaintPacket) -> UiPresentationAttemptIdentity {
        UiPresentationAttemptIdentity::routed(packet.identity(), packet.target())
    }

    #[must_use]
    pub fn presentation_retry(
        attempt: UiPresentationAttemptIdentity,
        backend_generation: UiPresentationBackendGeneration,
    ) -> UiPresentationRetryIdentity {
        UiPresentationRetryIdentity::new(attempt, backend_generation)
    }

    #[must_use]
    pub fn issue_presented(
        packet: Arc<UiPaintPacket>,
        presentation_epoch: u64,
        backend_generation: UiPresentationBackendGeneration,
    ) -> UiPresentationOutcome {
        UiPresentationOutcome::Presented(OneShotPresentationCommit::issue(
            packet,
            presentation_epoch,
            backend_generation,
        ))
    }
}

pub mod prelude {
    pub use crate::UiRenderPlugin;
}

/// Explicit Host/backend integration surface kept out of ordinary gameplay imports.
pub mod advanced {
    pub use crate::{
        UiGlyphAtlasPageKey, UiGlyphAtlasPageSnapshot, UiPacketBorder, UiPacketFill, UiPacketImage,
        UiPacketPrimitive, UiPacketRect, UiPacketUv, UiPaintCompileStats, UiPaintPacket,
        UiPaintPacketIdentity, UiPreparedImages, UiPresentationAttemptIdentity,
        UiPresentationBackendGeneration, UiPresentationContentError,
        UiPresentationIntegrationError, UiPresentationLimits, UiPresentationLimitsError,
        UiPresentationOutcome, UiPresentationPlanner, UiPresentationPlannerCreateError,
        UiPresentationRetryIdentity, UiPresentationRoute, UiPresentationRouteError,
        compile_ui_paint_packets, drain_ui_presentation_outcomes, install_ui_presentation_route,
        retire_ui_presentation_route,
    };
}

#[cfg(test)]
mod packet_tests;
