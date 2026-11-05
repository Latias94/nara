//! Private first-party Nara Editor application.

mod app;
mod frontend;
pub mod native;
pub mod panels;
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
#[cfg(feature = "imgui-reference")]
pub use frontend::{
    ImguiEditorFrontend, ImguiEditorPanel, ImguiEditorPanelContext, ImguiEditorSubmission,
};
pub use panels::{
    EditorPanel, EditorPanelContext, EditorPanelHost, EditorPanelInstanceToken,
    EditorPanelSubmission, PanelActionDispatch, PanelActionNotDispatched, PanelSubmissionId,
    PanelSubmitError,
};
pub use platform::EditorRunError;
#[cfg(feature = "imgui-reference")]
pub use platform::{
    EditorCompositionError, EditorNativeComposition, EditorNativeEvent, EditorNativeHost, run,
    run_trusted_compiled,
};
