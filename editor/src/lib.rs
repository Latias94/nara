//! Private first-party Nara Editor application.

mod app;
mod platform;
mod supervised_play;

pub use app::{
    EditorApplication, EditorApplicationOpenError, EditorApplicationState, EditorApplicationUpdate,
    EditorLaunchOptions,
};
pub use platform::{
    EditorGameViewCanaryEvidence, EditorGameViewCanaryFrame, EditorGameViewCanaryRequest,
    EditorRunError, EditorUiAuthoringCanaryEvidence, EditorUiAuthoringCanaryFrame,
    EditorUiAuthoringCanaryRequest, run, run_trusted_compiled,
    run_trusted_compiled_game_view_canary,
};
