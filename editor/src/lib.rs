//! Private first-party Nara Editor application.

mod app;
mod frontend;
mod platform;
mod supervised_play;

pub use app::{
    EditorApplication, EditorApplicationOpenError, EditorApplicationState, EditorApplicationUpdate,
    EditorLaunchOptions, EditorProjectAction,
};
pub use frontend::EguiEditorFrontend;
pub use platform::{
    EditorGameViewCanaryEvidence, EditorGameViewCanaryFrame, EditorGameViewCanaryRequest,
    EditorRunError, EditorSupervisedPlayCanaryEvidence, EditorUiAuthoringCanaryEvidence,
    EditorUiAuthoringCanaryFrame, EditorUiAuthoringCanaryRequest, run, run_trusted_compiled,
    run_trusted_compiled_game_view_canary,
};
