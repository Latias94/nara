//! Private first-party Nara Editor application.

mod app;
mod platform;

pub use app::{
    EditorApplication, EditorApplicationOpenError, EditorApplicationUpdate, EditorLaunchOptions,
};
pub use platform::{
    EditorGameViewCanaryEvidence, EditorGameViewCanaryFrame, EditorGameViewCanaryRequest,
    EditorRunError, run, run_game_view_canary_with_intent, run_with_intent,
};
