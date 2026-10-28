//! Current-source capture before bundle publication, with nominal successor attachment.

use super::*;
use crate::project_host::authoring::{
    AdmittedAuthoringSchemaOwner, ObservedAuthoringSchemaOwner, PreparedAuthoringSchemaOwner,
};

impl PreparedAuthoringSchemaOwner {
    pub(crate) fn capture_scene(
        &mut self,
        source: &super::super::super::document::AuthoringDocumentSourceFact,
        view: nara_scene::__private::PreparedSceneSchemaSource<'_>,
        open_prefabs: &[AuthoringPrefabSessionSnapshot],
    ) -> Result<PreparedSchemaOwnerSceneCapture, AuthoringProjectSourceAdmissionError> {
        let basis = view.schema_basis();
        if &basis != self.schema_basis() {
            return Err(AuthoringProjectSourceAdmissionError::SchemaBasisMismatch);
        }
        if source.source_key().kind() != nara_scene::SourceDocumentKind::Scene
            || !source_locator_matches(Some(view.source()), source.relative_path())
        {
            return Err(AuthoringProjectSourceAdmissionError::DocumentSourceMismatch);
        }
        let mut budget = self.budget();
        let result = (|| {
            budget.charge_source_bytes(0, 0, 0, size_of::<Arc<AuthoringSchemaReceipt>>())?;
            prefabs::charge_capture_inputs(
                source,
                view.document().retained_bytes()?,
                adapter_receipts_retained_bytes(self.adapters())?,
                open_prefabs,
                &mut budget,
            )
            .and_then(|(_, containing)| {
                capture_scene(
                    self.observation(),
                    view.file_limits(),
                    view.document(),
                    &basis,
                    &containing,
                    source.native_source(),
                    AuthoringProjectReadiness::from_document(view.readiness()),
                    &mut budget,
                    open_prefabs,
                    AuthoringCapturePreview::Prepare,
                )
            })
        })();
        // Failed attempts have still performed work. Keep one allowance across all sources;
        // never grant another full budget merely because a capture was refused or discarded.
        self.retain_capture_budget(budget);
        let mut capture = result?;
        capture.root_revision = Some(view.base_revision());
        Ok(PreparedSchemaOwnerSceneCapture {
            capture,
            basis,
            incumbent: Arc::clone(self.incumbent()),
        })
    }
}

impl PreparedSchemaOwnerSceneCapture {
    pub(crate) fn readiness(&self) -> AuthoringProjectReadiness {
        self.capture.readiness()
    }
    pub(crate) fn diagnostics(&self) -> &DiagnosticReport {
        self.capture.diagnostics()
    }
    pub(crate) fn composition_failure(&self) -> Option<&nara_scene::PrefabExpansionFailure> {
        self.capture.composition_failure()
    }

    /// Revalidates the original capture under the same cumulative project preparation allowance.
    pub(crate) fn validate(
        &self,
        owner: &mut PreparedAuthoringSchemaOwner,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        if &self.basis != owner.schema_basis()
            || !Arc::ptr_eq(&self.incumbent, owner.incumbent())
            || !Arc::ptr_eq(&self.capture.authority, owner.observation())
        {
            return Err(AuthoringProjectSourceAdmissionError::SchemaBasisMismatch);
        }
        let mut budget = owner.budget();
        let result = budget
            .ensure_within_limits()
            .map_err(AuthoringProjectSourceAdmissionError::from)
            .and_then(|()| {
                self.capture
                    .source_validation()
                    .validate_with_budget(&mut budget)
            });
        owner.retain_capture_budget(budget);
        result
    }

    /// Transfers already prepared capture into the same nominal successor after leaf attachment.
    /// No IO, compilation or Schema admission occurs here.
    pub(crate) fn attach(
        self,
        successor: &AdmittedAuthoringSchemaOwner,
    ) -> Result<AuthoringDocumentCapture, Self> {
        self.attach_schema(&successor.basis, &successor.schema)
    }

    pub(crate) fn attach_observed(
        self,
        successor: &ObservedAuthoringSchemaOwner,
    ) -> Result<AuthoringDocumentCapture, Self> {
        self.attach_schema(&successor.basis, &successor.schema)
    }

    fn attach_schema(
        mut self,
        basis: &AuthoringSchemaBasis,
        schema: &Arc<AuthoringSchemaReceipt>,
    ) -> Result<AuthoringDocumentCapture, Self> {
        if &self.basis != basis {
            return Err(self);
        }
        self.capture.authority = Arc::clone(schema);
        Ok(self.capture)
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;
