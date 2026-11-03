//! Reviewed Prefab extraction delegates capture and operation custody to the Session.

use std::time::Duration;

use nara::{
    diagnostic::DiagnosticReport,
    project_host::{
        AuthoringSourceOperationLimits, EditorPrefabExtractionError, EditorPrefabExtractionId,
        EditorPrefabExtractionRequest, EditorPrefabExtractionReview,
        EditorPrefabExtractionSelection, EditorPrefabExtractionView, EditorProjectSession,
    },
};

use super::EditorApplication;

impl EditorApplication {
    pub fn begin_prefab_extraction(
        &mut self,
        request: EditorPrefabExtractionRequest,
    ) -> Result<EditorPrefabExtractionId, EditorPrefabExtractionError> {
        if self.session_retirement.is_some() {
            return Err(EditorPrefabExtractionError::Closing);
        }
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorPrefabExtractionError::Closing)?
            .session
            .begin_prefab_extraction(request);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    /// Selects finite source work and native custody before the Session begins capture.
    pub fn begin_prefab_extraction_with_limits(
        &mut self,
        request: EditorPrefabExtractionRequest,
        limits: AuthoringSourceOperationLimits,
    ) -> Result<EditorPrefabExtractionId, EditorPrefabExtractionError> {
        if self.session_retirement.is_some() {
            return Err(EditorPrefabExtractionError::Closing);
        }
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorPrefabExtractionError::Closing)?
            .session
            .begin_prefab_extraction_with_limits(request, limits);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    /// Borrows the retained observation while Close waits for physical operation retirement.
    pub fn prefab_extraction_view(
        &self,
    ) -> Result<&EditorPrefabExtractionView, EditorPrefabExtractionError> {
        Ok(self
            .state
            .published()
            .ok_or(EditorPrefabExtractionError::Closing)?
            .session
            .prefab_extraction_view())
    }

    /// Borrows the Session's complete prepared review without advancing or changing it.
    pub fn prefab_extraction_review(
        &self,
        operation: EditorPrefabExtractionId,
    ) -> Result<EditorPrefabExtractionReview<'_>, EditorPrefabExtractionError> {
        self.state
            .published()
            .ok_or(EditorPrefabExtractionError::Closing)?
            .session
            .prefab_extraction_review(operation)
    }

    pub fn prefab_extraction_selection(
        &self,
        operation: EditorPrefabExtractionId,
    ) -> Result<EditorPrefabExtractionSelection<'_>, EditorPrefabExtractionError> {
        self.state
            .published()
            .ok_or(EditorPrefabExtractionError::Closing)?
            .session
            .prefab_extraction_selection(operation)
    }

    pub fn revise_prefab_extraction(
        &mut self,
        operation: EditorPrefabExtractionId,
        save_current: Vec<nara::tooling::EditorDocumentId>,
    ) -> Result<(), EditorPrefabExtractionError> {
        if self.session_retirement.is_some() {
            return Err(EditorPrefabExtractionError::Closing);
        }
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorPrefabExtractionError::Closing)?
            .session
            .revise_prefab_extraction(operation, save_current);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    pub fn cancel_prefab_extraction(
        &mut self,
        operation: EditorPrefabExtractionId,
    ) -> Result<(), EditorPrefabExtractionError> {
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorPrefabExtractionError::Closing)?
            .session
            .cancel_prefab_extraction(operation);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    pub fn confirm_prefab_extraction(
        &mut self,
        operation: EditorPrefabExtractionId,
    ) -> Result<(), EditorPrefabExtractionError> {
        if self.session_retirement.is_some() {
            return Err(EditorPrefabExtractionError::Closing);
        }
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorPrefabExtractionError::Closing)?
            .session
            .confirm_prefab_extraction(operation);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    /// Close can wait for the same retained publication while the original Tasks still run.
    pub fn retry_prefab_extraction(
        &mut self,
        operation: EditorPrefabExtractionId,
    ) -> Result<(), EditorPrefabExtractionError> {
        let result = self
            .state
            .published_mut()
            .ok_or(EditorPrefabExtractionError::Closing)?
            .session
            .retry_prefab_extraction(operation);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    pub fn prefab_extraction_diagnostics(&self) -> Option<&DiagnosticReport> {
        self.state
            .published()?
            .session
            .prefab_extraction_diagnostics()
    }
}

/// Close requests cancellation only when the Session owner permits it. The Session retains its
/// write fence and captured inputs until admitted workers physically return.
pub(super) fn cancel_pre_pending_for_close(session: &mut EditorProjectSession) -> bool {
    let view = session.prefab_extraction_view();
    if !view.can_cancel() {
        return false;
    }
    let Some(operation) = view.operation else {
        return false;
    };
    session.cancel_prefab_extraction(operation).is_ok()
}
