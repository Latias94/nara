//! Headless authoring state for the Nara editor.

pub mod actions;
mod document;
pub mod field_draft;
mod inspector;
mod models;
#[cfg(any(target_os = "linux", target_os = "macos", windows))]
pub mod play;
#[cfg(any(unix, windows))]
mod project;
pub mod value_draft;
pub mod viewport;

pub use document::{EditorCommand, EditorDocument, EditorError, EditorSession};
pub use inspector::{
    EditorRegistry, InspectorComponent, InspectorField, InspectorModel, InspectorValue,
};
pub use models::{ConsoleModel, HierarchyModel, HierarchyRow, SelectionMode, SelectionState};
#[cfg(any(unix, windows))]
pub use project::{AssetBrowserModel, AssetEntry, AssetKind, EditorProject, ProjectError};
