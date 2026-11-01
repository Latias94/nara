//! Reviewed authored upgrades delegate document and publication authority to the Session.

use std::time::Duration;

use nara::{
    diagnostic::DiagnosticReport,
    project_host::{
        AuthoredSchemaUpgradeHostLimits, AuthoredSchemaUpgradeProposal,
        AuthoredSchemaUpgradeReview, EditorAuthoredSchemaUpgradeError,
        EditorAuthoredSchemaUpgradeId, EditorAuthoredSchemaUpgradeInputs,
        EditorAuthoredSchemaUpgradeView, EditorProjectSession,
    },
};

use super::EditorApplication;

impl EditorApplication {
    pub fn begin_authored_schema_upgrade(
        &mut self,
    ) -> Result<EditorAuthoredSchemaUpgradeId, EditorAuthoredSchemaUpgradeError> {
        if self.session_retirement.is_some() {
            return Err(EditorAuthoredSchemaUpgradeError::Closing);
        }
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorAuthoredSchemaUpgradeError::Closing)?
            .session
            .begin_authored_schema_upgrade();
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    /// Selects finite Host work and native custody before capturing the complete upgrade set.
    pub fn begin_authored_schema_upgrade_with_limits(
        &mut self,
        limits: AuthoredSchemaUpgradeHostLimits,
    ) -> Result<EditorAuthoredSchemaUpgradeId, EditorAuthoredSchemaUpgradeError> {
        if self.session_retirement.is_some() {
            return Err(EditorAuthoredSchemaUpgradeError::Closing);
        }
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorAuthoredSchemaUpgradeError::Closing)?
            .session
            .begin_authored_schema_upgrade_with_limits(limits);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    /// Retains read-only operation observations while Close waits for upgrade retirement.
    pub fn authored_schema_upgrade_view(
        &self,
    ) -> Result<&EditorAuthoredSchemaUpgradeView, EditorAuthoredSchemaUpgradeError> {
        Ok(self
            .state
            .published()
            .ok_or(EditorAuthoredSchemaUpgradeError::Closing)?
            .session
            .authored_schema_upgrade_view())
    }

    pub fn authored_schema_upgrade_inputs(
        &self,
        operation: EditorAuthoredSchemaUpgradeId,
    ) -> Result<EditorAuthoredSchemaUpgradeInputs<'_>, EditorAuthoredSchemaUpgradeError> {
        self.authoring_edit_session()
            .map_err(|_| EditorAuthoredSchemaUpgradeError::Closing)?
            .authored_schema_upgrade_inputs(operation)
    }

    /// Starts finite preparation of the explicit proposal against the original captured inputs.
    pub fn prepare_authored_schema_upgrade(
        &mut self,
        operation: EditorAuthoredSchemaUpgradeId,
        proposal: AuthoredSchemaUpgradeProposal,
    ) -> Result<(), EditorAuthoredSchemaUpgradeError> {
        if self.session_retirement.is_some() {
            return Err(EditorAuthoredSchemaUpgradeError::Closing);
        }
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorAuthoredSchemaUpgradeError::Closing)?
            .session
            .prepare_authored_schema_upgrade(operation, proposal);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    /// Borrows the Session's retained complete review without advancing preparation.
    pub fn authored_schema_upgrade_review(
        &self,
        operation: EditorAuthoredSchemaUpgradeId,
    ) -> Result<AuthoredSchemaUpgradeReview<'_>, EditorAuthoredSchemaUpgradeError> {
        self.authoring_edit_session()
            .map_err(|_| EditorAuthoredSchemaUpgradeError::Closing)?
            .authored_schema_upgrade_review(operation)
    }

    /// Confirms the Session's retained complete transition, including its explicit history policy.
    pub fn confirm_authored_schema_upgrade(
        &mut self,
        operation: EditorAuthoredSchemaUpgradeId,
    ) -> Result<(), EditorAuthoredSchemaUpgradeError> {
        if self.session_retirement.is_some() {
            return Err(EditorAuthoredSchemaUpgradeError::Closing);
        }
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorAuthoredSchemaUpgradeError::Closing)?
            .session
            .confirm_authored_schema_upgrade(operation);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    pub fn cancel_authored_schema_upgrade(
        &mut self,
        operation: EditorAuthoredSchemaUpgradeId,
    ) -> Result<(), EditorAuthoredSchemaUpgradeError> {
        let result = self
            .state
            .workspace_mut()
            .ok_or(EditorAuthoredSchemaUpgradeError::Closing)?
            .session
            .cancel_authored_schema_upgrade(operation);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    /// Retries the Session's exact retained publication under its original finite allowance.
    pub fn retry_authored_schema_upgrade(
        &mut self,
        operation: EditorAuthoredSchemaUpgradeId,
    ) -> Result<(), EditorAuthoredSchemaUpgradeError> {
        let result = self
            .state
            .published_mut()
            .ok_or(EditorAuthoredSchemaUpgradeError::Closing)?
            .session
            .retry_authored_schema_upgrade(operation);
        self.drive_editor_frame(Duration::ZERO, true, false);
        result
    }

    pub fn authored_schema_upgrade_diagnostics(&self) -> Option<&DiagnosticReport> {
        self.state
            .published()?
            .session
            .authored_schema_upgrade_diagnostics()
    }
}

/// Close requests cancellation only when the Session owner permits it. The Session retains its
/// write fence until admitted workers physically return and owns later publication retirement.
pub(super) fn cancel_pre_pending_for_close(session: &mut EditorProjectSession) -> bool {
    let view = session.authored_schema_upgrade_view();
    if !view.can_cancel() {
        return false;
    }
    let Some(operation) = view.operation else {
        return false;
    };
    session.cancel_authored_schema_upgrade(operation).is_ok()
}
