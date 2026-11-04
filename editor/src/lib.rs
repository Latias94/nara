//! Private first-party Nara Editor application.

mod app;
mod frontend;
mod platform;
mod supervised_play;
pub mod workspace;

#[cfg(all(test, feature = "ui-work-allocation-probes"))]
mod allocation_probe;

pub use app::{
    EditorApplication, EditorApplicationActionDispatch, EditorApplicationActionNotDispatched,
    EditorApplicationConstructionSubmission, EditorApplicationOpenError,
    EditorApplicationSourceToolSubmission, EditorApplicationState, EditorApplicationSubmission,
    EditorApplicationUpdate, EditorLaunchOptions, EditorProjectAction,
};
pub use frontend::{
    ImguiEditorFrontend, ImguiEditorPanel, ImguiEditorPanelContext, ImguiEditorSubmission,
    PanelActionDispatch, PanelActionNotDispatched, PanelSubmissionId, PanelSubmitError,
};
pub use platform::{
    EditorCompositionError, EditorNativeComposition, EditorNativeEvent, EditorNativeHost,
    EditorRunError, run, run_trusted_compiled,
};
