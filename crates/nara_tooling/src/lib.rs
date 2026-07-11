//! Tooling-facing runtime inspection seam.

mod diagnostic;
mod inspector;
mod play;
mod snapshot;
mod workspace;

use nara_app::{App, CoreStage, Plugin, PluginError};

pub use inspector::{
    SceneInspectorCommand, SceneInspectorCommandReport, SceneInspectorComponentView,
    SceneInspectorEntityRow, SceneInspectorEntityView, SceneInspectorFieldState,
    SceneInspectorFieldView, SceneInspectorModel, SceneInspectorState,
};
pub use play::{
    SceneApplyChangesComponentReport, SceneApplyChangesComponentStatus, SceneApplyChangesReport,
    SceneApplyChangesRequest, SceneEditorMode, SceneEditorModel, SceneEditorState,
    ScenePlaySession, ScenePlayTransitionReport,
};
pub use snapshot::{DEFAULT_WORLD_IDENTITY_SNAPSHOT_LOCATOR_LIMIT, WorldIdentitySnapshot};
pub use workspace::{
    EditorDocumentId, EditorExternalReloadState, EditorSceneModel, EditorSceneSlot,
    EditorSceneTabModel, EditorSelectionSet, EditorWorkspace, EditorWorkspaceCommand,
    EditorWorkspaceCommandReport, EditorWorkspaceModel,
};

#[derive(Debug, Default)]
pub struct ToolingPlugin;

impl Plugin for ToolingPlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            nara_app::PluginId::new("nara.tooling"),
            nara_app::PluginCategory::Tooling,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<EditorWorkspace>()?;
        app.add_systems(CoreStage::Last, || {})?;
        Ok(())
    }
}
