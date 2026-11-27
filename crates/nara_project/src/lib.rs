//! Project manifests and native tools for code-first Nara games.

mod manifest;

pub use manifest::{
    EditorPlayMode, ProjectDelivery, ProjectEditor, ProjectInfo, ProjectManifest, ProjectPaths,
    ProjectWindow,
};

#[cfg(all(
    feature = "workflow",
    any(target_os = "linux", target_os = "macos", windows)
))]
pub mod play;

#[cfg(all(feature = "workflow", windows))]
mod windows_command_line;

#[cfg(all(
    feature = "workflow",
    any(target_os = "linux", target_os = "macos", windows)
))]
pub mod workflow;
