//! Saved-startup project preparation under one unpublished authoring Schema successor.

use super::*;
use crate::project_host::{
    authoring::{
        AdmittedAuthoringSchemaOwner, AuthoringSchemaOwnerError, AuthoringSchemaOwnerPreparation,
        AuthoringSchemaOwnerReconciliation, ObservedAuthoringSchemaOwner,
        PreparedAuthoringSchemaOwner,
    },
    file_publication::FilePublicationEvidence,
};

/// Exact project authority needed by background preparation, without retaining the live Session.
#[derive(Clone)]
pub(crate) struct AuthoringSchemaOwnerProjectInput {
    incumbent: Arc<AuthoringSchemaReceipt>,
    basis: AuthoringSchemaBasis,
    scene_limits: SceneFileLimits,
}

impl AuthoringProjectSessionSnapshot {
    pub(crate) fn schema_owner_input(&self) -> AuthoringSchemaOwnerProjectInput {
        AuthoringSchemaOwnerProjectInput {
            incumbent: Arc::clone(&self.schema),
            basis: self.startup_scene.schema_basis(),
            scene_limits: self.scene_limits,
        }
    }
}

impl AuthoringSchemaOwnerProjectInput {
    pub(crate) fn project_root(&self) -> DirectoryCapability {
        self.incumbent.project_root()
    }

    pub(crate) fn schema_basis(&self) -> &AuthoringSchemaBasis {
        &self.basis
    }

    pub(crate) fn prepare_owner(
        &self,
        owner: &nara_reflect::ComponentSchemaOwnerSnapshot,
        change: nara_reflect::ComponentSchemaOwnerChange,
    ) -> Result<AuthoringSchemaOwnerPreparation, AuthoringSchemaOwnerError> {
        self.incumbent.prepare_owner(owner, change)
    }

    pub(crate) fn prepare_observed_owner(
        &self,
        owner: &nara_reflect::ComponentSchemaOwnerSnapshot,
        change: nara_reflect::ComponentSchemaOwnerChange,
    ) -> Result<AuthoringSchemaOwnerReconciliation, AuthoringSchemaOwnerError> {
        self.incumbent.prepare_observed_owner(owner, change)
    }

    /// Revalidation compares actual authority and nominal basis, never just equal file bytes.
    pub(crate) fn matches(&self, project: &AuthoringProjectSessionSnapshot) -> bool {
        Arc::ptr_eq(&self.incumbent, &project.schema)
            && self.basis == project.startup_scene.schema_basis()
    }
}

impl Debug for AuthoringSchemaOwnerProjectInput {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoringSchemaOwnerProjectInput")
            .finish_non_exhaustive()
    }
}

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

/// Actual selected capture under prepared metadata, still fenced by the incumbent bundle leaf.
///
/// The inner capture is unavailable as a published successor until the bundle's actual accepted
/// leaf has been attached. Source-history proof and whole-project publication are separate.
pub(crate) struct PreparedSchemaOwnerSceneCapture {
    pub(super) capture: AuthoringDocumentCapture,
    pub(super) basis: AuthoringSchemaBasis,
    pub(super) incumbent: Arc<AuthoringSchemaReceipt>,
}

/// The bundle was already published; retain both preparation and actual leaf for reconciliation.
pub(crate) struct SchemaOwnerProjectAttachmentFailure {
    pub(crate) prepared: PreparedSchemaOwnerProject,
    pub(crate) successor: AdmittedAuthoringSchemaOwner,
}

/// The observed leaf remains owned when it cannot join this nominal project preparation.
pub(crate) struct SchemaOwnerObservedProjectAttachmentFailure {
    pub(crate) prepared: PreparedSchemaOwnerProject,
    pub(crate) successor: ObservedAuthoringSchemaOwner,
}

impl PreparedAuthoringSchemaOwner {
    pub(crate) fn prepare_project(
        &mut self,
        incumbent: &AuthoringProjectSessionSnapshot,
        open_prefabs: &[AuthoringPrefabSessionSnapshot],
    ) -> Result<PreparedSchemaOwnerProject, AuthoringProjectSourceAdmissionError> {
        self.prepare_project_input(&incumbent.schema_owner_input(), open_prefabs)
    }

    pub(crate) fn prepare_project_input(
        &mut self,
        input: &AuthoringSchemaOwnerProjectInput,
        open_prefabs: &[AuthoringPrefabSessionSnapshot],
    ) -> Result<PreparedSchemaOwnerProject, AuthoringProjectSourceAdmissionError> {
        if !Arc::ptr_eq(self.incumbent(), &input.incumbent) {
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
            let read = read_saved_startup(self.observation(), input.scene_limits, &mut budget)?;
            let mut startup = capture_saved_startup(
                self.observation(),
                input.scene_limits,
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
                scene_limits: input.scene_limits,
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
        if &self.basis != owner.schema_basis()
            || !Arc::ptr_eq(&self.incumbent, owner.incumbent())
            || !Arc::ptr_eq(&self.startup.capture.authority, owner.observation())
        {
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

    /// Joins the exact observed leaf without writing or manufacturing publication evidence.
    pub(crate) fn attach_observed(
        self,
        successor: ObservedAuthoringSchemaOwner,
    ) -> Result<AuthoringProjectReceipt, SchemaOwnerObservedProjectAttachmentFailure> {
        if self.basis != successor.basis {
            return Err(SchemaOwnerObservedProjectAttachmentFailure {
                prepared: self,
                successor,
            });
        }
        let ObservedAuthoringSchemaOwner {
            schema,
            adapters,
            basis: _,
            usage,
        } = successor;
        Ok(self
            .startup
            .into_candidate(schema, adapters, self.scene_limits, usage)
            .publish())
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

impl Debug for SchemaOwnerObservedProjectAttachmentFailure {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SchemaOwnerObservedProjectAttachmentFailure")
            .field("prepared", &self.prepared)
            .field("observed_bundle", &self.successor.schema.bundle_digest())
            .finish_non_exhaustive()
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod observed_owner_tests;
#[cfg(all(test, any(windows, target_os = "linux")))]
mod owned_input_tests;
#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;
