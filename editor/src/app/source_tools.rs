//! Finite source tools delegate execution and document authority to the Project Session.

use std::{collections::BTreeMap, time::Duration};

use nara::{
    project_host::{
        EditorProjectSession, EditorSourceToolDefinition, EditorSourceToolInstallation,
        EditorSourceToolManagedHost, PreparedEditorSourceToolContent,
        PreparedEditorSourceToolInstallations, PreparedEditorSourceToolSelection,
        PreparedEditorSourceToolWithdrawal,
    },
    reflect::ComponentValue,
    tooling::{
        EditorDocumentQueryBasis, EditorSourceToolActivationId, EditorSourceToolAttemptId,
        EditorSourceToolCommit, EditorSourceToolContentBasis, EditorSourceToolContentError,
        EditorSourceToolContentView, EditorSourceToolError, EditorSourceToolInstallationRequest,
        EditorSourceToolIntent, EditorSourceToolRequestId, EditorSourceToolReview,
        EditorSourceToolSelectionBasis, EditorSourceToolSelectionError,
        EditorSourceToolSelectionView, EditorSourceToolView, EditorSourceToolWithdrawalMode,
    },
};

use super::{EditorApplication, EditorApplicationActionNotDispatched, EditorApplicationUpdate};

#[cfg(all(test, windows))]
mod content_tests;
#[cfg(all(test, windows))]
mod csharp_tests;

/// Exact tool acceptance result and subsequent application drive.
///
/// The result remains associated with this call after later requests or project replacement.
#[derive(Debug)]
pub struct EditorApplicationSourceToolSubmission {
    result: Result<EditorSourceToolCommit, EditorSourceToolError>,
    update: EditorApplicationUpdate,
}

impl EditorApplicationSourceToolSubmission {
    pub const fn result(&self) -> &Result<EditorSourceToolCommit, EditorSourceToolError> {
        &self.result
    }

    #[must_use]
    pub const fn update(&self) -> EditorApplicationUpdate {
        self.update
    }

    pub fn into_result(self) -> Result<EditorSourceToolCommit, EditorSourceToolError> {
        self.result
    }
}

impl EditorApplication {
    /// Observes protection facts separately from executable selection and graph withdrawal.
    pub fn source_tool_content_view(
        &self,
    ) -> Result<EditorSourceToolContentView, EditorSourceToolError> {
        Ok(self
            .state
            .published()
            .ok_or(EditorSourceToolError::Closing)?
            .session
            .source_tool_content_view())
    }

    pub fn prepare_source_tool_content_management(
        &self,
        basis: &EditorSourceToolContentBasis,
        directory: &str,
        installation: std::sync::Arc<EditorSourceToolInstallation>,
    ) -> Result<PreparedEditorSourceToolContent, EditorSourceToolContentError> {
        self.authoring_edit_session()
            .map_err(|_| EditorSourceToolContentError::Closing)?
            .prepare_source_tool_content_management(basis, directory, installation)
    }

    pub fn prepare_source_tool_content_adoption(
        &self,
        basis: &EditorSourceToolContentBasis,
        installation: &[u8; 32],
        paths: &[&str],
    ) -> Result<PreparedEditorSourceToolContent, EditorSourceToolContentError> {
        self.authoring_edit_session()
            .map_err(|_| EditorSourceToolContentError::Closing)?
            .prepare_source_tool_content_adoption(basis, installation, paths)
    }

    pub fn prepare_source_tool_content_observation(
        &self,
        basis: &EditorSourceToolContentBasis,
        installation: &[u8; 32],
    ) -> Result<PreparedEditorSourceToolContent, EditorSourceToolContentError> {
        self.authoring_edit_session()
            .map_err(|_| EditorSourceToolContentError::Closing)?
            .prepare_source_tool_content_observation(basis, installation)
    }

    pub fn prepare_source_tool_content_protection(
        &self,
        basis: &EditorSourceToolContentBasis,
    ) -> Result<PreparedEditorSourceToolContent, EditorSourceToolContentError> {
        self.state
            .published()
            .ok_or(EditorSourceToolContentError::Closing)?
            .session
            .prepare_source_tool_content_protection(basis)
    }

    pub fn commit_source_tool_content(
        &mut self,
        prepared: PreparedEditorSourceToolContent,
    ) -> Result<EditorSourceToolAttemptId, EditorSourceToolContentError> {
        let result = self
            .state
            .published_mut()
            .ok_or(EditorSourceToolContentError::Closing)?
            .session
            .commit_source_tool_content(prepared);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    pub fn reconcile_source_tool_content(
        &mut self,
        basis: &EditorSourceToolContentBasis,
    ) -> Result<EditorSourceToolAttemptId, EditorSourceToolContentError> {
        let result = self
            .state
            .published_mut()
            .ok_or(EditorSourceToolContentError::Closing)?
            .session
            .reconcile_source_tool_content(basis);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    /// Prepares an additive installed set against the original project/selection observation.
    pub fn prepare_source_tool_installations(
        &self,
        basis: &EditorSourceToolSelectionBasis,
        project: nara::fs::DirectoryCapability,
        requests: &[EditorSourceToolInstallationRequest],
    ) -> Result<PreparedEditorSourceToolInstallations, EditorSourceToolSelectionError> {
        self.authoring_edit_session()
            .map_err(|_| EditorSourceToolSelectionError::Closing)?
            .prepare_source_tool_installations(basis, project, requests)
    }

    pub fn register_source_tool_installations(
        &mut self,
        prepared: PreparedEditorSourceToolInstallations,
    ) -> Result<EditorSourceToolAttemptId, EditorSourceToolSelectionError> {
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorSourceToolSelectionError::Closing)?
            .session
            .register_source_tool_installations(prepared);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    pub fn prepare_source_tool_withdrawal(
        &self,
        basis: &EditorSourceToolSelectionBasis,
        installation: &[u8; 32],
        mode: EditorSourceToolWithdrawalMode,
    ) -> Result<PreparedEditorSourceToolWithdrawal, EditorSourceToolSelectionError> {
        self.authoring_edit_session()
            .map_err(|_| EditorSourceToolSelectionError::Closing)?
            .prepare_source_tool_withdrawal(basis, installation, mode)
    }

    /// Confirms graph withdrawal; actual files and admitted Schema retain their owners.
    pub fn withdraw_source_tool_installations(
        &mut self,
        prepared: PreparedEditorSourceToolWithdrawal,
    ) -> Result<EditorSourceToolAttemptId, EditorSourceToolSelectionError> {
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorSourceToolSelectionError::Closing)?
            .session
            .withdraw_source_tool_installations(prepared);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    /// Bounded code-free Host preparation; this does not probe or enable the tool.
    pub fn prepare_installed_source_tool_selection(
        &self,
        project: nara::fs::DirectoryCapability,
        directory: &str,
        receipt: nara::fs::ContentDigest,
        host: &EditorSourceToolManagedHost,
    ) -> Result<PreparedEditorSourceToolSelection, EditorSourceToolSelectionError> {
        self.authoring_edit_session()
            .map_err(|_| EditorSourceToolSelectionError::Closing)?
            .prepare_installed_source_tool_selection(project, directory, receipt, host)
    }

    /// Observes desired disk data and actual routing without granting execution approval.
    pub fn source_tool_selection_view(
        &self,
    ) -> Result<EditorSourceToolSelectionView, EditorSourceToolError> {
        Ok(self.authoring_edit_session()?.source_tool_selection_view())
    }

    pub fn enable_installed_source_tool(
        &mut self,
        basis: &EditorSourceToolSelectionBasis,
        prepared: PreparedEditorSourceToolSelection,
    ) -> Result<EditorSourceToolAttemptId, EditorSourceToolSelectionError> {
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorSourceToolSelectionError::Closing)?
            .session
            .enable_installed_source_tool(basis, prepared);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    pub fn disable_installed_source_tool(
        &mut self,
        basis: &EditorSourceToolSelectionBasis,
    ) -> Result<EditorSourceToolAttemptId, EditorSourceToolSelectionError> {
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorSourceToolSelectionError::Closing)?
            .session
            .disable_installed_source_tool(basis);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    pub fn reconcile_source_tool_selection(
        &mut self,
        basis: &EditorSourceToolSelectionBasis,
    ) -> Result<EditorSourceToolAttemptId, EditorSourceToolSelectionError> {
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorSourceToolSelectionError::Closing)?
            .session
            .reconcile_source_tool_selection(basis);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    /// Observes the current project's tool without executing it or refreshing its request.
    pub fn source_tool_view(&self) -> Result<EditorSourceToolView, EditorSourceToolError> {
        Ok(self.authoring_edit_session()?.source_tool_view())
    }

    /// Borrows the exact Session-prepared proposal retained for explicit acceptance.
    pub fn source_tool_review(
        &self,
    ) -> Result<Option<EditorSourceToolReview<'_>>, EditorSourceToolError> {
        Ok(self.authoring_edit_session()?.source_tool_review())
    }

    /// Explicitly probes a trusted command; ordinary project Open never calls this entry.
    pub fn enable_source_tool(
        &mut self,
        definition: EditorSourceToolDefinition,
    ) -> Result<EditorSourceToolAttemptId, EditorSourceToolError> {
        let result = self
            .source_tool_session_mut()?
            .enable_source_tool(definition);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    /// Revokes routing and review while the Session retains physical child/IO custody.
    pub fn disable_source_tool(
        &mut self,
        activation: &EditorSourceToolActivationId,
    ) -> Result<(), EditorSourceToolError> {
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorSourceToolError::WorkspaceUnavailable)?
            .session
            .disable_source_tool(activation);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    /// Cancels exactly one pending probe without disabling its active predecessor.
    pub fn cancel_source_tool_attempt(
        &mut self,
        attempt: &EditorSourceToolAttemptId,
    ) -> Result<(), EditorSourceToolError> {
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorSourceToolError::WorkspaceUnavailable)?
            .session
            .cancel_source_tool_attempt(attempt);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    /// Captures the original source observation for one finite copied-value proposal.
    pub fn request_source_tool(
        &mut self,
        activation: &EditorSourceToolActivationId,
        basis: &EditorDocumentQueryBasis,
        intent: EditorSourceToolIntent,
        parameters: BTreeMap<String, ComponentValue>,
    ) -> Result<EditorSourceToolRequestId, EditorSourceToolError> {
        let result = self
            .source_tool_session_mut()?
            .request_source_tool(activation, basis, intent, parameters);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    /// Publishes only the retained reviewed candidate and returns its actual owner receipt.
    pub fn accept_source_tool_review(
        &mut self,
        request: &EditorSourceToolRequestId,
        real_delta: Duration,
    ) -> EditorApplicationSourceToolSubmission {
        let result = self
            .source_tool_session_mut()
            .and_then(|session| session.accept_source_tool_review(request));
        let update = self.drive_editor_frame(real_delta, true, true);
        EditorApplicationSourceToolSubmission { result, update }
    }

    fn source_tool_session_mut(
        &mut self,
    ) -> Result<&mut EditorProjectSession, EditorSourceToolError> {
        self.authoring_edit_session_mut()
            .map_err(|reason| match reason {
                EditorApplicationActionNotDispatched::WorkspaceUnavailable => {
                    EditorSourceToolError::WorkspaceUnavailable
                }
                _ => EditorSourceToolError::Closing,
            })
    }
}
