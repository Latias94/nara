//! Private first-party Nara Editor application.

mod app;
mod frontend;
mod platform;
mod supervised_play;
pub mod workspace;

pub use app::{
    EditorApplication, EditorApplicationActionDispatch, EditorApplicationActionNotDispatched,
    EditorApplicationConstructionSubmission, EditorApplicationOpenError, EditorApplicationState,
    EditorApplicationSubmission, EditorApplicationUpdate, EditorLaunchOptions, EditorProjectAction,
};
pub use frontend::{
    ImguiEditorFrontend, ImguiEditorPanel, ImguiEditorPanelContext, ImguiEditorSubmission,
    PanelActionDispatch, PanelActionNotDispatched, PanelSubmissionId, PanelSubmitError,
};
pub use platform::{
    EditorCompositionError, EditorNativeComposition, EditorRunError, run, run_trusted_compiled,
};
