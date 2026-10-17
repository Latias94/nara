//! Private first-party Nara Editor application.

mod app;
mod frontend;
mod platform;
mod supervised_play;
pub mod workspace;

pub use app::{
    EditorApplication, EditorApplicationOpenError, EditorApplicationState, EditorApplicationUpdate,
    EditorLaunchOptions, EditorProjectAction,
};
pub use frontend::{ImguiEditorFrontend, ImguiEditorPanelContext, ImguiEditorPanelPainter};
pub use platform::{EditorRunError, run, run_trusted_compiled};
