//! Current-source capture before bundle publication, with nominal successor attachment.

use super::*;
use crate::project_host::authoring::{AdmittedAuthoringSchemaOwner, PreparedAuthoringSchemaOwner};

/// Actual selected capture under prepared metadata, still fenced by the incumbent bundle leaf.
///
/// The inner capture is unavailable as a published successor until the bundle's actual accepted
/// leaf has been attached. Historical composition and whole-project publication are separate.
pub(crate) struct PreparedSchemaOwnerSceneCapture {
    capture: AuthoringDocumentCapture,
    basis: AuthoringSchemaBasis,
}

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
            prefabs::charge_capture_inputs(
                source,
                view.document().retained_bytes()?,
                adapter_receipts_retained_bytes(self.adapters())?,
                open_prefabs,
                &mut budget,
            )
            .and_then(|(_, containing)| {
                capture_scene(
                    self.incumbent(),
                    view.file_limits(),
                    view.document(),
                    &basis,
                    &containing,
                    source.native_source(),
                    AuthoringProjectReadiness::from_document(view.readiness()),
                    &mut budget,
                    open_prefabs,
                )
            })
        })();
        // Failed attempts have still performed work. Keep one allowance across all sources;
        // never grant another full budget merely because a capture was refused or discarded.
        self.retain_capture_budget(budget);
        let mut capture = result?;
        capture.root_revision = Some(view.base_revision());
        Ok(PreparedSchemaOwnerSceneCapture { capture, basis })
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

    /// Transfers already prepared capture into the same nominal successor after leaf attachment.
    /// No IO, compilation or Schema admission occurs here.
    pub(crate) fn attach(
        mut self,
        successor: &AdmittedAuthoringSchemaOwner,
    ) -> Result<AuthoringDocumentCapture, Self> {
        if self.basis != successor.basis {
            return Err(self);
        }
        self.capture.authority = Arc::clone(&successor.schema);
        Ok(self.capture)
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;
