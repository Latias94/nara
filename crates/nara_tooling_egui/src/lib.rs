//! egui Adapter for Nara's UI-neutral Editor frame and ordered action seam.
//!
//! This crate renders one immutable [`nara_tooling::EditorFrameModel`] and returns one
//! [`nara_tooling::EditorActionBatch`]. It owns only transient toolkit state; document,
//! selection, history, persistence, Play, window, and GPU authority remain elsewhere.

mod editor;
mod fields;
mod game_view;
mod inspector;
mod scene_view;

#[cfg(test)]
mod tests;

pub use editor::{
    EguiCentralViewportInput, EguiCentralViewportOutput, EguiEditorViewport,
    EguiEditorViewportIntent, EguiSceneEditorOutput, EguiSceneEditorPanel,
};
pub use game_view::{EguiGameViewInput, EguiGameViewOutput};
pub use scene_view::{EguiSceneViewInput, EguiSceneViewOutput};
