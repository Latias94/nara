//! Saved-startup project preparation under one unpublished authoring Schema successor.

use super::*;
use crate::project_host::{
    authoring::{AdmittedAuthoringSchemaOwner, PreparedAuthoringSchemaOwner},
    file_publication::FilePublicationEvidence,
};

/// A complete actual saved-root closure, still fenced by the incumbent metadata observation.
///
/// This grants neither whole-Session adoption nor executable approval. Current working roots
/// and exact source-history proofs remain distinct inputs to the Session operation.
pub(crate) struct PreparedSchemaOwnerProject {
    incumbent: Arc<AuthoringSchemaReceipt>,
    basis: AuthoringSchemaBasis,
    scene_limits: SceneFileLimits,
    startup: CapturedSavedStartup,
}

/// The bundle was already published; retain both preparation and actual leaf for reconciliation.
pub(crate) struct SchemaOwnerProjectAttachmentFailure {
    pub(crate) prepared: PreparedSchemaOwnerProject,
    pub(crate) successor: AdmittedAuthoringSchemaOwner,
}

impl PreparedAuthoringSchemaOwner {
    pub(crate) fn prepare_project(
        &mut self,
        incumbent: &AuthoringProjectSessionSnapshot,
        open_prefabs: &[AuthoringPrefabSessionSnapshot],
    ) -> Result<PreparedSchemaOwnerProject, AuthoringProjectSourceAdmissionError> {
        if !Arc::ptr_eq(self.incumbent(), &incumbent.schema) {
            return Err(AuthoringProjectSourceAdmissionError::SchemaBasisMismatch);
        }
        let mut budget = self.budget();
        let basis = self.schema_basis().clone();
        let result = (|| {
            budget.ensure_within_limits()?;
            budget.charge_source_bytes(
                0,
                0,
                0,
                checked_sum([
                    size_of::<PreparedSchemaOwnerProject>(),
                    size_of::<AuthoringProjectSnapshotInner>(),
                    adapter_receipts_retained_bytes(self.adapters())?,
                ])?,
            )?;
            let read = read_saved_startup(self.incumbent(), incumbent.scene_limits, &mut budget)?;
            let mut startup = capture_saved_startup(
                self.incumbent(),
                incumbent.scene_limits,
                read,
                &basis,
                self.bytes().len(),
                &mut budget,
                open_prefabs,
                capture::AuthoringCapturePreview::Omit,
            )?;
            // This closure may include dirty Prefab participants. Ordinary startup seeding marks
            // all participants as disk-only; current content must use the revision-aware path.
            startup.capture.usage = budget.usage();
            Ok(PreparedSchemaOwnerProject {
                incumbent: Arc::clone(self.incumbent()),
                basis,
                scene_limits: incumbent.scene_limits,
                startup,
            })
        })();
        // Failed reads, classification and acquisition performed work too.
        self.retain_capture_budget(budget);
        result
    }
}

impl PreparedSchemaOwnerProject {
    pub(crate) fn readiness(&self) -> AuthoringProjectReadiness {
        self.startup.capture.readiness()
    }

    pub(crate) fn diagnostics(&self) -> &DiagnosticReport {
        self.startup.capture.diagnostics()
    }

    pub(crate) fn composition_failure(&self) -> Option<&nara_scene::PrefabExpansionFailure> {
        self.startup.capture.composition_failure()
    }

    /// Revalidates original actual source/lookup/metadata observations before bundle publication.
    /// It does not refresh inputs or grant a new allowance for every validation attempt.
    pub(crate) fn validate(
        &self,
        owner: &mut PreparedAuthoringSchemaOwner,
    ) -> Result<(), AuthoringProjectSourceAdmissionError> {
        if &self.basis != owner.schema_basis() || !Arc::ptr_eq(&self.incumbent, owner.incumbent()) {
            return Err(AuthoringProjectSourceAdmissionError::SchemaBasisMismatch);
        }
        let mut budget = owner.budget();
        let result = budget
            .ensure_within_limits()
            .map_err(AuthoringProjectSourceAdmissionError::from)
            .and_then(|()| {
                self.startup
                    .capture
                    .source_validation()
                    .validate_with_budget(&mut budget)
            });
        owner.retain_capture_budget(budget);
        result
    }

    /// Joins to the exact published metadata owner without IO, recompilation or basis admission.
    /// The actual final operation usage covers later captures and failed attempts as well.
    pub(crate) fn attach(
        self,
        successor: AdmittedAuthoringSchemaOwner,
    ) -> Result<
        (AuthoringProjectReceipt, FilePublicationEvidence),
        SchemaOwnerProjectAttachmentFailure,
    > {
        if self.basis != successor.basis {
            return Err(SchemaOwnerProjectAttachmentFailure {
                prepared: self,
                successor,
            });
        }
        let AdmittedAuthoringSchemaOwner {
            schema,
            adapters,
            basis: _,
            publication,
            usage,
        } = successor;
        let candidate = self
            .startup
            .into_candidate(schema, adapters, self.scene_limits, usage);
        Ok((candidate.publish(), publication))
    }
}

impl Debug for PreparedSchemaOwnerProject {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedSchemaOwnerProject")
            .field("startup", &self.startup.source)
            .field("readiness", &self.readiness())
            .field("composition_failure", &self.composition_failure())
            .finish_non_exhaustive()
    }
}

impl Debug for SchemaOwnerProjectAttachmentFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchemaOwnerProjectAttachmentFailure")
            .field("prepared", &self.prepared)
            .field("published_bundle", &self.successor.schema.bundle_digest())
            .finish_non_exhaustive()
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;
