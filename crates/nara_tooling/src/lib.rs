//! Tooling-facing runtime inspection seam.

mod diagnostic;
mod inspector;
mod persistence;
mod play;
mod snapshot;
mod workspace;

use nara_app::{App, CoreStage, Plugin, PluginError};

pub use inspector::{
    SceneInspectorCommand, SceneInspectorCommandReport, SceneInspectorComponentView,
    SceneInspectorEntityRow, SceneInspectorEntityView, SceneInspectorFieldState,
    SceneInspectorFieldView, SceneInspectorModel, SceneInspectorState,
};
pub use persistence::{
    EditorCloseDecision, EditorPersistenceCommand, EditorPersistenceFailureStage,
    EditorPersistenceOperation, EditorPersistenceRejection, EditorPersistenceRequestResult,
    EditorPersistenceResult, EditorPersistenceView, EditorProjectView, EditorWorkspaceIntent,
    EditorWorkspaceIntentPhase, EditorWorkspaceIntentRejection, EditorWorkspaceIntentRequestResult,
    EditorWorkspaceIntentResult, EditorWorkspaceIntentView,
};
#[doc(hidden)]
pub use play::__export_apply_changes_from_world;
pub use play::{
    EditorApplyChangesRejection, EditorApplyChangesResult, EditorPlayCommand, EditorPlayFailure,
    EditorPlayOperation, EditorPlayOperationResult, EditorPlayRejection, EditorPlayRequestResult,
    EditorPlayState, EditorPlayView, EditorRuntimeEditRejection, EditorRuntimeEditRequest,
    EditorRuntimeEditResult, SceneApplyChangesComponentReport, SceneApplyChangesComponentStatus,
    SceneApplyChangesReport, SceneApplyChangesRequest, SceneEditorModel, SceneEditorState,
};
pub use snapshot::{DEFAULT_WORLD_IDENTITY_SNAPSHOT_LOCATOR_LIMIT, WorldIdentitySnapshot};
pub use workspace::{
    EditorDocumentDigest, EditorDocumentId, EditorExternalReloadState, EditorPersistenceCheckpoint,
    EditorSceneModel, EditorSceneSessionPublicationError, EditorSceneSlot, EditorSceneTabModel,
    EditorSelectionSet, EditorWorkspace, EditorWorkspaceCommand, EditorWorkspaceCommandReport,
    EditorWorkspaceModel,
};

#[derive(Debug, Default)]
pub struct ToolingPlugin;

pub const TOOLING_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.tooling");
const TOOLING_PRODUCT_REQUIREMENTS: &[nara_app::PluginProductCapability] =
    &[nara_app::PluginProductCapability::new("tooling")];
pub const TOOLING_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(TOOLING_PLUGIN_ID, nara_app::PluginCategory::Tooling)
        .requires_product_capabilities(TOOLING_PRODUCT_REQUIREMENTS);

impl Plugin for ToolingPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &TOOLING_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<EditorWorkspace>()?;
        app.add_systems(CoreStage::Last, || {})?;
        Ok(())
    }
}
