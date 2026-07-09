//! Tooling-facing runtime inspection seam.

mod inspector;
mod play;
mod snapshot;

use nara_app::{App, CoreStage, Plugin, PluginError};

pub use inspector::{
    SceneInspectorCommand, SceneInspectorCommandReport, SceneInspectorComponentView,
    SceneInspectorEntityRow, SceneInspectorEntityView, SceneInspectorFieldState,
    SceneInspectorFieldView, SceneInspectorModel, SceneInspectorState,
};
pub use play::{
    SceneApplyChangesReport, SceneEditorMode, SceneEditorModel, SceneEditorState, ScenePlaySession,
    ScenePlayTransitionReport,
};
pub use snapshot::WorldSnapshot;

#[derive(Debug, Default)]
pub struct ToolingPlugin;

impl Plugin for ToolingPlugin {
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.add_systems(CoreStage::Last, || {});
        Ok(())
    }
}
