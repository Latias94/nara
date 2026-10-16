mod egui;
mod egui_workspace;
#[cfg(test)]
mod egui_workspace_tests;

pub use egui::EguiEditorFrontend;
pub use egui_workspace::{EguiEditorPanelContext, EguiEditorPanelPainter};
