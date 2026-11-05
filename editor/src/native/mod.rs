//! Native services and concrete frontend attachment over the original Editor and graphics owners.

mod clipboard;
mod images;

pub use clipboard::{EditorClipboard, EditorClipboardAccess};
pub use images::{
    EDITOR_PANEL_IMAGE_LIMIT, EditorPanelCancelReason, EditorPanelCapability, EditorPanelImage,
    EditorPanelImageEndpoint, EditorPanelImageError, EditorPanelImageFrame, EditorPanelImageHost,
    EditorPanelImageStatus, EditorPanelImeArea, EditorPanelInput, EditorPanelInputKind,
    EditorPanelInputOutcome, EditorPanelKeyEvent, EditorPanelPlatformEvent,
    EditorPanelServiceError, EditorPanelServices, EditorPanelTextInputScope, EditorPanelWheelDelta,
    EditorPanelWindowGeometry,
};
