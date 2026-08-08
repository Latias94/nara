//! Resource-complete backend-neutral paint packets for immutable UI frames.

// Bevy derives resolve this path; Nara's ECS facade remains the actual dependency.
extern crate nara_ecs as bevy_ecs;

mod atlas;
mod compiler;
mod integration;
mod packet;

pub use atlas::{UiRenderLimits, UiRenderLimitsError};
pub use compiler::{
    UiPaintPacketBuildError, UiPaintPacketBuilder, UiPaintPacketBuilderCreateError,
    UiPreparedImages,
};
pub use integration::{
    UI_RENDER_PLUGIN_DECLARATION, UI_RENDER_PLUGIN_ID, UiPaintCompileStats,
    UiPresentationIntegrationError, UiPresentationRoute, UiPresentationRouteError, UiRenderPlugin,
    compile_ui_paint_packets, drain_ui_presentation_outcomes, install_ui_presentation_route,
    retire_ui_presentation_route,
};
pub use packet::{
    UiGlyphAtlasPageKey, UiGlyphAtlasPageSnapshot, UiPacketImage, UiPacketPrimitive, UiPacketRect,
    UiPacketUv, UiPaintDiagnostic, UiPaintPacket, UiPaintPacketIdentity, UiPresentationOutcome,
};

#[doc(hidden)]
pub mod __private {
    pub use crate::integration::{
        UiPaintPacketReader, UiPresentationOutcomes, replace_ui_presentation_routes,
    };
}

pub mod prelude {
    pub use crate::UiRenderPlugin;
}

/// Explicit Host/backend integration surface kept out of ordinary gameplay imports.
pub mod advanced {
    pub use crate::{
        UiGlyphAtlasPageKey, UiGlyphAtlasPageSnapshot, UiPacketImage, UiPacketPrimitive,
        UiPacketRect, UiPacketUv, UiPaintCompileStats, UiPaintDiagnostic, UiPaintPacket,
        UiPaintPacketBuildError, UiPaintPacketBuilder, UiPaintPacketBuilderCreateError,
        UiPaintPacketIdentity, UiPreparedImages, UiPresentationIntegrationError,
        UiPresentationOutcome, UiPresentationRoute, UiPresentationRouteError, UiRenderLimits,
        UiRenderLimitsError, compile_ui_paint_packets, drain_ui_presentation_outcomes,
        install_ui_presentation_route, retire_ui_presentation_route,
    };
}

#[cfg(test)]
mod packet_tests;
