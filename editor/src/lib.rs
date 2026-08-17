//! Private first-party Nara Editor application.

mod app;
mod platform;

pub use app::{
    EditorApplication, EditorApplicationOpenError, EditorApplicationUpdate, EditorLaunchOptions,
};
pub use platform::{EditorRunError, run, run_with_intent};
