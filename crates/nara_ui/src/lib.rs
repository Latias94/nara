//! Code-first game UI, Taffy layout, and Parley/Swash text.

pub mod kernel;
pub mod text;

#[cfg(feature = "runtime")]
mod components;
#[cfg(feature = "runtime")]
mod interaction;
#[cfg(feature = "runtime")]
mod plugin;
#[cfg(feature = "runtime")]
mod primary_surface_metrics;
#[cfg(feature = "runtime")]
mod runtime;

pub use kernel::{
    UiActionId, UiAlign, UiBorder, UiBrush, UiColor, UiColorRole, UiCornerRadii, UiDimension,
    UiEdges, UiFlexDirection, UiImageId, UiJustify, UiLayer, UiOverflow, UiPositionMode,
    UiPresentationReceipt, UiSafeAreaInsets, UiSemanticRole, UiSize, UiStyle, UiStyleError,
    UiSurfaceContentTransform, UiSurfaceContentTransformError, UiSurfaceId, UiTextAlignment,
    UiTextOverflow, UiThemePalette,
};
pub use text::{FontFace, FontFaceError};

#[cfg(feature = "runtime")]
pub use components::{
    UiButton, UiButtonVisuals, UiNode, UiRoot, UiRuntimeKey, UiSemantics, UiText, UiTheme, UiVisual,
};
#[cfg(feature = "runtime")]
pub use interaction::{UiActionEvent, UiActivationSource};
#[cfg(feature = "runtime")]
pub use plugin::{UI_PLUGIN_DECLARATION, UI_PLUGIN_ID, UiPlugin, plugin};
#[cfg(feature = "runtime")]
pub use primary_surface_metrics::UiPrimarySurfaceMetrics;
#[cfg(feature = "runtime")]
pub use runtime::{
    UiAdapterDiagnostics, UiAdapterError, UiSurfaceAvailability, UiSurfaceEndpoint,
    UiSurfaceRetirement, UiSurfaceSlot, UiSurfaceSlotError, UiSurfaceSlotSnapshot,
    UiSurfaceSuspension, UiSurfaceSuspensionReason, active_ui_surface_slots, install_ui_surface,
    latest_presented_ui_frame, restore_ui_surface, retire_ui_surface, suspend_ui_surface,
    ui_candidate_frame, ui_candidate_frames, ui_surface_slot_snapshots, update_ui_surface_slot,
};

/// Font and publication data for presenters of compiled frames.
#[cfg(feature = "runtime")]
pub mod presentation {
    pub use crate::runtime::{clear_presented_ui_frame, publish_presented_frame, ui_font_face};
}

/// Components and styling for game HUDs.
#[cfg(feature = "runtime")]
pub mod prelude {
    pub use crate::{
        UiActionEvent, UiActionId, UiActivationSource, UiAlign, UiBorder, UiBrush, UiButton,
        UiButtonVisuals, UiColor, UiColorRole, UiCornerRadii, UiDimension, UiEdges,
        UiFlexDirection, UiImageId, UiJustify, UiLayer, UiNode, UiOverflow, UiPlugin,
        UiPositionMode, UiRoot, UiRuntimeKey, UiSemanticRole, UiSemantics, UiStyle, UiText,
        UiTextAlignment, UiTextOverflow, UiTheme, UiThemePalette, UiVisual,
    };
}

/// Immutable layout and paint data for presenters.
pub mod advanced {
    pub use crate::kernel::{UiFrame, UiKey, UiPaintPrimitive};
    #[cfg(feature = "runtime")]
    pub use crate::{
        UiAdapterDiagnostics, UiAdapterError, UiPresentationReceipt, UiPrimarySurfaceMetrics,
        UiSurfaceAvailability, UiSurfaceEndpoint, UiSurfaceId, UiSurfaceSlot,
        UiSurfaceSlotSnapshot, active_ui_surface_slots, latest_presented_ui_frame,
        ui_candidate_frame, ui_candidate_frames,
    };
}
