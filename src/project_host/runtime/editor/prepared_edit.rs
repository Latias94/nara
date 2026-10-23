//! Public prepared authoring keeps document publication in its existing owner.

use nara_scene::{SceneAuthoringRevision, ScenePatchDocument, ScenePatchOperation};
use nara_tooling::{
    EditorActionOutcome, EditorActionRejection, EditorDocumentId, EditorDocumentKind,
    EditorWorkspaceCommandReport, PreparedEditorDocumentEdit,
};

use super::{EditorProjectSession, editor_workspace_command_rejected};

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum EditorEditPreparationError {
    #[error("no Editor workspace is available")]
    WorkspaceUnavailable,
    #[error("the Editor is closing or resolving a workspace intent")]
    Closing,
    #[error("the edit requires its specialized document-domain contract")]
    StructureContractMismatch { document: EditorDocumentId },
    #[error("the document owner rejected edit preparation")]
    Document(Box<EditorWorkspaceCommandReport>),
}

impl EditorProjectSession {
    /// Prepares an explicitly targeted data-authoring transaction without changing the frame.
    pub fn prepare_authoring_edit(
        &self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        patch: &ScenePatchDocument,
    ) -> Result<PreparedEditorDocumentEdit, EditorEditPreparationError> {
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return Err(EditorEditPreparationError::Closing);
        }
        self.verify_prepared_patch_contract(document, patch)
            .map_err(|_| EditorEditPreparationError::StructureContractMismatch { document })?;
        self.workspace
            .prepare_authoring_edit(document, base_revision, patch)
            .map_err(|report| EditorEditPreparationError::Document(Box::new(report)))
    }

    /// Consumes the accepted candidate; later project/domain changes cannot authorize it anew.
    pub fn commit_authoring_edit(
        &mut self,
        prepared: PreparedEditorDocumentEdit,
    ) -> EditorActionOutcome {
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return EditorActionOutcome::Workspace(editor_workspace_command_rejected(
                self.workspace.active_document(),
                "project.editor.workspace-intent-pending",
            ));
        }
        if let Err(rejection) =
            self.verify_prepared_patch_contract(prepared.document(), prepared.patch())
        {
            return EditorActionOutcome::Rejected(rejection);
        }
        let report = self.workspace.commit_authoring_edit(prepared);
        let _ = self.diagnostics.extend(report.diagnostics.clone());
        EditorActionOutcome::Workspace(report)
    }

    pub(super) fn verify_prepared_patch_contract(
        &self,
        document: EditorDocumentId,
        patch: &ScenePatchDocument,
    ) -> Result<(), EditorActionRejection> {
        if self.workspace.document_kind(document) == Some(EditorDocumentKind::Prefab)
            && patch.operations.iter().any(|operation| {
                !matches!(
                    operation,
                    ScenePatchOperation::SetField { .. }
                        | ScenePatchOperation::SetAssetRefField { .. }
                )
            })
            && (self.prefab_uses_ui_authoring_domain(document)
                || super::actions::prefab_patch_uses_ui_domain(patch))
        {
            return Err(EditorActionRejection::StructureContractMismatch { document });
        }
        Ok(())
    }
}
