//! Metadata adoption delegates source, publication and recovery authority to the Session.

use std::time::Duration;

use nara::{
    diagnostic::DiagnosticReport,
    fs::{ContentDigest, ReplaceReceipt},
    project_host::{
        EditorSchemaOwnerAdoptionError, EditorSchemaOwnerAdoptionId, EditorSchemaOwnerAdoptionView,
    },
    scene::PrefabExpansionFailure,
    tooling::EditorDocumentId,
};

use super::EditorApplication;

impl EditorApplication {
    pub fn schema_owner_adoption_view(
        &self,
    ) -> Result<&EditorSchemaOwnerAdoptionView, EditorSchemaOwnerAdoptionError> {
        Ok(self
            .authoring_edit_session()
            .map_err(|_| EditorSchemaOwnerAdoptionError::Closing)?
            .schema_owner_adoption_view())
    }

    /// Starts finite code-free preparation; completion is observed in the operation view.
    pub fn prepare_installed_schema_owner(
        &mut self,
        directory: &str,
        receipt: ContentDigest,
    ) -> Result<EditorSchemaOwnerAdoptionId, EditorSchemaOwnerAdoptionError> {
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorSchemaOwnerAdoptionError::Closing)?
            .session
            .prepare_installed_schema_owner(directory, receipt);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    /// Confirms the retained review. Successful admission does not yet mean publication or adoption.
    pub fn adopt_schema_owner(
        &mut self,
        operation: EditorSchemaOwnerAdoptionId,
    ) -> Result<(), EditorSchemaOwnerAdoptionError> {
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorSchemaOwnerAdoptionError::Closing)?
            .session
            .adopt_schema_owner(operation);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    pub fn cancel_schema_owner_adoption(
        &mut self,
        operation: EditorSchemaOwnerAdoptionId,
    ) -> Result<(), EditorSchemaOwnerAdoptionError> {
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorSchemaOwnerAdoptionError::Closing)?
            .session
            .cancel_schema_owner_adoption(operation);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    pub fn reconcile_schema_owner_adoption(
        &mut self,
        operation: EditorSchemaOwnerAdoptionId,
    ) -> Result<EditorSchemaOwnerAdoptionId, EditorSchemaOwnerAdoptionError> {
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorSchemaOwnerAdoptionError::Closing)?
            .session
            .reconcile_schema_owner_adoption(operation);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    pub fn acknowledge_schema_owner_adoption(
        &mut self,
        operation: EditorSchemaOwnerAdoptionId,
    ) -> Result<(), EditorSchemaOwnerAdoptionError> {
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorSchemaOwnerAdoptionError::Closing)?
            .session
            .acknowledge_schema_owner_adoption(operation);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    pub fn schema_owner_adoption_diagnostics(
        &self,
        document: Option<EditorDocumentId>,
    ) -> Option<&DiagnosticReport> {
        self.authoring_edit_session()
            .ok()?
            .schema_owner_adoption_diagnostics(document)
    }

    pub fn schema_owner_adoption_composition_failure(
        &self,
        document: Option<EditorDocumentId>,
    ) -> Option<&PrefabExpansionFailure> {
        self.authoring_edit_session()
            .ok()?
            .schema_owner_adoption_composition_failure(document)
    }

    pub fn schema_owner_adoption_publication(&self) -> Option<&ReplaceReceipt> {
        self.authoring_edit_session()
            .ok()?
            .schema_owner_adoption_publication()
    }
}
