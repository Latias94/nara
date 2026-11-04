use nara::{
    project_host::EditorEditPreparationError,
    scene::{
        AuthoringConstructionDraft, AuthoringPropertyEdit, PrefabDeclarationLimits,
        PrefabInstanceObservation, PrefabInstancePath, PrefabRevert, SceneCompilationFailure,
        ScenePatchDocument,
    },
    tooling::{
        EditorAction, EditorAuthoringPage, EditorAuthoringQuery, EditorDocumentId,
        EditorDocumentQueryBasis, EditorDocumentQueryRejection, EditorEntityPage,
        EditorEntityQuery, EditorPropertyObservation, EditorPropertyRead, EditorSourceToolAction,
        EditorSourceToolError, EditorSourceToolReview, PreparedEditorDocumentConstruction,
        PreparedEditorDocumentEdit,
    },
};

use crate::{EditorApplication, workspace::EditorPanelModel};

use super::submission::{
    self, PanelOrigin, PanelSubmissionId, PanelSubmitError, PendingPanelSubmission,
};

/// Read-only application access and bounded action submission for one live panel instance.
///
/// Source preparation delegates to the existing Application and Session owners. Queuing an action
/// does not publish a source edit. The originating live panel receives its exact dispatch result
/// after the Host submits the prepared batch and drives the application.
pub struct EditorPanelContext<'scope, 'frame> {
    pub(super) application: &'scope EditorApplication,
    pub(super) model: EditorPanelModel<'frame>,
    pub(super) origin: PanelOrigin,
    pub(super) actions: &'scope mut Vec<EditorAction>,
    pub(super) submissions: &'scope mut Vec<PendingPanelSubmission<()>>,
    pub(super) next_submission: &'scope mut u64,
}

impl EditorPanelContext<'_, '_> {
    /// Borrows the current admitted frame and this panel's workspace binding.
    #[must_use]
    pub fn model(&self) -> EditorPanelModel<'_> {
        self.model
    }

    /// Captures an open source document's exact query basis without changing navigation.
    pub fn document_query_basis(
        &self,
        document: EditorDocumentId,
    ) -> Result<EditorDocumentQueryBasis, EditorDocumentQueryRejection> {
        self.application.document_query_basis(document)
    }

    /// Enumerates bounded direct source entities, including in a non-active document.
    pub fn query_document_entities(
        &self,
        query: EditorEntityQuery,
    ) -> Result<EditorEntityPage, EditorDocumentQueryRejection> {
        self.application.query_document_entities(query)
    }

    /// Discovers bounded component and property metadata without copying source values.
    pub fn query_document_authoring(
        &self,
        query: EditorAuthoringQuery,
    ) -> Result<EditorAuthoringPage, EditorDocumentQueryRejection> {
        self.application.query_document_authoring(query)
    }

    /// Reads one bounded source property through the document's original owner.
    pub fn read_document_property(
        &self,
        request: EditorPropertyRead,
    ) -> Result<EditorPropertyObservation, EditorDocumentQueryRejection> {
        self.application.read_document_property(request)
    }

    /// Observes bounded stored Prefab intent at the exact retained basis.
    pub fn observe_prefab_instance(
        &self,
        basis: &EditorDocumentQueryBasis,
        path: &PrefabInstancePath,
        limits: PrefabDeclarationLimits,
    ) -> Result<PrefabInstanceObservation, EditorDocumentQueryRejection> {
        self.application
            .observe_prefab_instance(basis, path, limits)
    }

    /// Returns a verified composition failure for this exact document basis.
    pub fn prefab_composition_failure(
        &self,
        basis: &EditorDocumentQueryBasis,
    ) -> Result<Option<&SceneCompilationFailure>, EditorDocumentQueryRejection> {
        self.application.prefab_composition_failure(basis)
    }

    /// Prepares a source-owned Revert without publishing or changing the instance's source kind.
    pub fn prepare_prefab_revert(
        &self,
        basis: &EditorDocumentQueryBasis,
        path: &PrefabInstancePath,
        action: PrefabRevert,
        limits: PrefabDeclarationLimits,
    ) -> Result<PreparedEditorDocumentEdit, EditorEditPreparationError> {
        self.application
            .prepare_prefab_revert(basis, path, action, limits)
    }

    /// Prepares an unpublished edit against the complete source, revision and Schema basis.
    pub fn prepare_authoring_edit(
        &self,
        basis: &EditorDocumentQueryBasis,
        patch: &ScenePatchDocument,
    ) -> Result<PreparedEditorDocumentEdit, EditorEditPreparationError> {
        self.application.prepare_authoring_edit(basis, patch)
    }

    /// Prepares stable-property replacements without changing selection or active tabs.
    pub fn prepare_authoring_property_edit(
        &self,
        basis: &EditorDocumentQueryBasis,
        edits: &[AuthoringPropertyEdit],
    ) -> Result<PreparedEditorDocumentEdit, EditorEditPreparationError> {
        self.application
            .prepare_authoring_property_edit(basis, edits)
    }

    /// Starts source-owned construction without allocating live entity IDs.
    pub fn begin_authoring_construction(
        &self,
        basis: &EditorDocumentQueryBasis,
    ) -> Result<AuthoringConstructionDraft, EditorEditPreparationError> {
        self.application.begin_authoring_construction(basis)
    }

    /// Prepares one owned construction candidate with its original fixed local-ID mapping.
    pub fn prepare_authoring_construction(
        &self,
        basis: &EditorDocumentQueryBasis,
        draft: &AuthoringConstructionDraft,
    ) -> Result<PreparedEditorDocumentConstruction, EditorEditPreparationError> {
        self.application
            .prepare_authoring_construction(basis, draft)
    }

    /// Queues an action in this Host's ordered batch, subject to a 1,024-action cycle limit.
    ///
    /// Admission does not make the batch atomic or imply the domain accepts the action. Immediate
    /// refusal creates neither an action slot nor a completion callback.
    pub fn submit(&mut self, action: EditorAction) -> Result<PanelSubmissionId, PanelSubmitError> {
        let id = self.enqueue()?;
        self.actions.push(action);
        Ok(id)
    }

    /// Queues the exact prepared edit; capacity refusal returns the original owned candidate.
    pub fn commit_authoring_edit(
        &mut self,
        prepared: PreparedEditorDocumentEdit,
    ) -> Result<PanelSubmissionId, (PanelSubmitError, PreparedEditorDocumentEdit)> {
        match self.enqueue() {
            Ok(id) => {
                self.actions
                    .push(EditorAction::CommitAuthoringEdit(prepared));
                Ok(id)
            }
            Err(error) => Err((error, prepared)),
        }
    }

    /// Queues the exact construction candidate; refusal returns it without allocating a slot.
    pub fn commit_authoring_construction(
        &mut self,
        prepared: PreparedEditorDocumentConstruction,
    ) -> Result<PanelSubmissionId, (PanelSubmitError, PreparedEditorDocumentConstruction)> {
        match self.enqueue() {
            Ok(id) => {
                self.actions
                    .push(EditorAction::CommitAuthoringConstruction(prepared));
                Ok(id)
            }
            Err(error) => Err((error, prepared)),
        }
    }

    /// Queues a finite source-tool operation in the same exact-result ledger.
    pub fn submit_source_tool(
        &mut self,
        action: EditorSourceToolAction,
    ) -> Result<PanelSubmissionId, PanelSubmitError> {
        self.submit(EditorAction::SourceTool(action))
    }

    /// Borrows the original Session-owned review without preparing or executing another proposal.
    pub fn source_tool_review(
        &self,
    ) -> Result<Option<EditorSourceToolReview<'_>>, EditorSourceToolError> {
        self.application.source_tool_review()
    }

    fn enqueue(&mut self) -> Result<PanelSubmissionId, PanelSubmitError> {
        submission::enqueue(
            self.next_submission,
            self.submissions,
            self.origin.clone(),
            self.actions.len(),
            (),
        )
    }
}
