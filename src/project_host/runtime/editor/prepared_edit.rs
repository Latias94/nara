//! Public prepared authoring keeps document publication in its existing owner.

use nara_scene::{
    AuthoringConstructionDraft, AuthoringPropertyEdit, SceneAuthoringRevision, ScenePatchDocument,
    ScenePatchOperation,
};
use nara_tooling::{
    CommittedEditorDocumentConstruction, EditorActionOutcome, EditorActionRejection,
    EditorDocumentId, EditorDocumentKind, EditorDocumentQueryBasis, EditorDocumentQueryRejection,
    EditorWorkspaceCommandReport, PreparedEditorDocumentConstruction, PreparedEditorDocumentEdit,
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
    #[error("the captured document-query basis is no longer admitted: {0}")]
    QueryBasis(EditorDocumentQueryRejection),
    #[error("the document owner rejected edit preparation")]
    Document(Box<EditorWorkspaceCommandReport>),
}

impl EditorProjectSession {
    /// Validates a retained observation before source-owned preparation, without refreshing it.
    /// Commit still revalidates the actual candidate and the current lifecycle independently.
    pub fn validate_authoring_query_basis(
        &self,
        basis: &EditorDocumentQueryBasis,
    ) -> Result<(), EditorEditPreparationError> {
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return Err(EditorEditPreparationError::Closing);
        }
        self.workspace
            .validate_document_query_basis(basis)
            .map_err(EditorEditPreparationError::QueryBasis)
    }

    /// Captures source-owned construction intent; no IDs or document state are published.
    pub fn begin_authoring_construction(
        &self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
    ) -> Result<AuthoringConstructionDraft, EditorEditPreparationError> {
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return Err(EditorEditPreparationError::Closing);
        }
        self.workspace
            .begin_authoring_construction(document, base_revision)
            .map_err(|report| EditorEditPreparationError::Document(Box::new(report)))
    }

    /// Keeps specialized Prefab admission after lowering, without a second transaction executor.
    pub fn prepare_authoring_construction(
        &self,
        document: EditorDocumentId,
        draft: &AuthoringConstructionDraft,
    ) -> Result<PreparedEditorDocumentConstruction, EditorEditPreparationError> {
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return Err(EditorEditPreparationError::Closing);
        }
        let prepared = self
            .workspace
            .prepare_authoring_construction(document, draft)
            .map_err(|report| EditorEditPreparationError::Document(Box::new(report)))?;
        self.verify_prepared_patch_contract(document, prepared.patch())
            .map_err(|_| EditorEditPreparationError::StructureContractMismatch { document })?;
        Ok(prepared)
    }

    /// A successful receipt is returned only after source publication; rejection retains its outcome.
    pub fn commit_authoring_construction(
        &mut self,
        prepared: PreparedEditorDocumentConstruction,
    ) -> Result<CommittedEditorDocumentConstruction, EditorActionOutcome> {
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return Err(EditorActionOutcome::Workspace(
                editor_workspace_command_rejected(
                    self.workspace.active_document(),
                    "project.editor.workspace-intent-pending",
                ),
            ));
        }
        self.verify_prepared_patch_contract(prepared.document(), prepared.patch())
            .map_err(EditorActionOutcome::Rejected)?;
        match self.workspace.commit_authoring_construction(prepared) {
            Ok(committed) => {
                let _ = self
                    .diagnostics
                    .extend(committed.report().diagnostics.clone());
                Ok(committed)
            }
            Err(report) => {
                let _ = self.diagnostics.extend(report.diagnostics.clone());
                Err(EditorActionOutcome::Workspace(report))
            }
        }
    }

    /// Prepares stable-property edits in one source owner, independently of selection and tabs.
    pub fn prepare_authoring_property_edit(
        &self,
        document: EditorDocumentId,
        base_revision: SceneAuthoringRevision,
        edits: &[AuthoringPropertyEdit],
    ) -> Result<PreparedEditorDocumentEdit, EditorEditPreparationError> {
        if !self.accepts_new_work() || self.pending_workspace_intent.is_some() {
            return Err(EditorEditPreparationError::Closing);
        }
        let prepared = self
            .workspace
            .prepare_authoring_property_edit(document, base_revision, edits)
            .map_err(|report| EditorEditPreparationError::Document(Box::new(report)))?;
        self.verify_prepared_patch_contract(document, prepared.patch())
            .map_err(|_| EditorEditPreparationError::StructureContractMismatch { document })?;
        Ok(prepared)
    }

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
