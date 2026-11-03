//! A saved Project prepared under the extraction's actual Pending and unchanged nominal basis.
//!
//! The complete operation must bind Workspace, persistence and Session installation guards
//! before invoking the final publisher. No ordinary Ready receipt escapes before publication.

use nara_scene::AuthoringSchemaBasis;

use super::*;
use crate::project_host::authoring::{
    AuthoringProjectAdmissionHost, AuthoringProjectAdmissionLimits,
    AuthoringProjectAttemptGeneration,
    source_inventory::{with_candidate_storage, with_temporary_storage},
};

mod ready;
use ready::PreparedExtractionReady;
#[cfg(feature = "tooling")]
mod workspace_installation;
#[cfg(feature = "tooling")]
pub(crate) use workspace_installation::ExtractionPersistenceDestination;
#[cfg(feature = "tooling")]
pub(in crate::project_host::authoring::project::prefab_extraction) use workspace_installation::{
    BoundPrefabExtractionWorkspaceInstallation, InstalledPrefabExtractionProject,
    PrefabExtractionInstallationError, PrefabExtractionInstallationFailure,
    PrefabExtractionWorkspaceInstallationFailure, PreparedPrefabExtractionWorkspaceInstallation,
    RetainedPrefabExtractionInstallation,
};

#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct PreparedPrefabExtractionProject
{
    future: ProjectPrefabExtractionFutureInventory,
    session: AuthoringProjectSessionSnapshot,
    ready: PreparedExtractionReady,
    continuing: AuthoringProjectAdmissionUsage,
}

#[derive(Debug, thiserror::Error)]
pub(in crate::project_host::authoring::project::prefab_extraction) enum PrefabExtractionProjectError
{
    #[error("the retained future extraction inventory changed")]
    Inventory(#[from] ProjectPrefabExtractionFutureInventoryError),
    #[error("the extraction's original publication custody changed")]
    Publication(#[from] ProjectPrefabExtractionPublicationError),
    #[error("future extraction Project admission exceeded its original allowance")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("future extraction saved startup preparation failed")]
    Source(#[from] AuthoringProjectSourceAdmissionError),
    #[error("the extraction's already admitted Schema models were refused")]
    Models(#[from] crate::project_host::authoring::schema_models::SchemaModelsError),
    #[error("the future extraction source inventory could not be retained")]
    NativeInventory(#[from] SourceInventoryError),
    #[error("the extraction's actual file observation changed")]
    File(#[from] nara_fs::FsError),
    #[error("the extraction's actual relative file path is invalid")]
    Path(#[from] nara_fs::PathValidationError),
    #[error("the future Project lost exclusive private candidate storage")]
    PrivateStorage,
    #[error("the extraction's unchanged Schema or complete configuration differs")]
    Configuration,
    #[error("the final extraction publisher could not bind Pending: {0:?}")]
    Publisher(FilePublicationFailureStage),
}

#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct PrefabExtractionProjectPreparationFailure
{
    pub(super) future: ProjectPrefabExtractionFutureInventory,
    pub(super) error: PrefabExtractionProjectError,
}

impl fmt::Debug for PreparedPrefabExtractionProject {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedPrefabExtractionProject")
            .field("basis", self.ready.basis())
            .field("continuing", &self.continuing)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for PrefabExtractionProjectPreparationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionProjectPreparationFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl PrefabExtractionProjectPreparationFailure {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_parts(
        self,
    ) -> (
        ProjectPrefabExtractionFutureInventory,
        PrefabExtractionProjectError,
    ) {
        (self.future, self.error)
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn owner(
        &self,
    ) -> &ProjectPrefabExtractionFutureInventory {
        &self.future
    }
    pub(in crate::project_host::authoring::project::prefab_extraction) fn error(
        &self,
    ) -> &PrefabExtractionProjectError {
        &self.error
    }
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_owner(
        self,
    ) -> ProjectPrefabExtractionFutureInventory {
        self.future
    }
    pub(in crate::project_host::authoring::project::prefab_extraction) fn retry(
        self,
        host: &mut AuthoringProjectAdmissionHost,
    ) -> Result<PreparedPrefabExtractionProject, Self> {
        self.future.prepare_project(host)
    }
}

impl ProjectPrefabExtractionFutureInventory {
    /// Reuses the live basis, or the unique basis already admitted by this recovery process.
    /// A new process obtains its attempt from the actual Host, not serialized Ready bytes.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn prepare_project(
        mut self,
        host: &mut AuthoringProjectAdmissionHost,
    ) -> Result<PreparedPrefabExtractionProject, PrefabExtractionProjectPreparationFailure> {
        let result = (|| {
            let inputs = self.publication.origin.inputs()?;
            let attempt = match &inputs.schema {
                #[cfg(feature = "tooling")]
                PublicationSchema::Live { receipt, .. } => receipt.attempt_generation(),
                PublicationSchema::Recovered(_) => host.allocate_attempt()?,
            };
            let PublicationInputs {
                pending,
                bundle,
                schema,
                budget,
                ..
            } = inputs;
            with_candidate_storage(budget, |budget| {
                prepare_session(
                    &self.inventory,
                    pending,
                    bundle,
                    schema,
                    attempt,
                    host.limits,
                    budget,
                )
            })
        })();
        match result {
            Ok((session, ready, continuing)) => Ok(PreparedPrefabExtractionProject {
                future: self,
                session,
                ready,
                continuing,
            }),
            Err(error) => Err(PrefabExtractionProjectPreparationFailure {
                future: self,
                error,
            }),
        }
    }
}

fn prepare_session(
    inventory: &SourceInventory,
    pending: &ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    bundle: PublicationBundle<'_>,
    schema: PublicationSchema<'_>,
    attempt: AuthoringProjectAttemptGeneration,
    ordinary_limits: AuthoringProjectAdmissionLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<
    (
        AuthoringProjectSessionSnapshot,
        PreparedExtractionReady,
        AuthoringProjectAdmissionUsage,
    ),
    PrefabExtractionProjectError,
> {
    future_inventory::validate_inventory(inventory, pending, bundle, budget)?;
    let (ready, adapters) = PreparedExtractionReady::prepare(
        pending,
        bundle,
        schema,
        attempt,
        ordinary_limits,
        budget,
    )?;
    let before = budget.retained_candidate_bytes;
    let handles_before = budget.retained_open_handles;
    budget.charge_source_bytes(
        0,
        0,
        0,
        checked_sum([
            size_of::<PreparedPrefabExtractionProject>(),
            size_of::<AuthoringProjectSnapshotInner>(),
            size_of::<AuthoringProjectSessionSnapshot>(),
        ])?,
    )?;
    let schema = ready.pending_schema();
    let scene_limits = inventory.scene_limits();
    let read = read_saved_startup(schema, scene_limits, budget)?;
    let mut startup = capture_saved_startup(
        schema,
        scene_limits,
        read,
        ready.basis(),
        schema.bundle_bytes().len(),
        budget,
        &[],
        &[],
        capture::AuthoringCapturePreview::Prepare,
    )?;
    #[cfg(feature = "presentation-2d")]
    reserve_preview_identity_store(&startup.capture, budget)?;
    let index_bytes = ProjectSourceIndexState::checked_retained_bytes_for_schema(schema)?;
    budget.charge_source_bytes(0, 0, index_bytes, index_bytes)?;
    startup.capture.usage = budget.usage();
    let candidate =
        startup.into_candidate(Arc::clone(schema), adapters, scene_limits, budget.usage());
    let session = candidate
        .publish()
        .into_session_snapshot()
        .map_err(|_| PrefabExtractionProjectError::PrivateStorage)?;
    if session.startup_scene.schema_basis() != *ready.basis() {
        return Err(PrefabExtractionProjectError::Configuration);
    }
    let session_bytes = budget
        .retained_candidate_bytes
        .checked_sub(before)
        .and_then(|bytes| bytes.checked_sub(size_of::<PreparedPrefabExtractionProject>()))
        .ok_or(SourceInventoryError::AccountingOverflow)?;
    let session_handles = budget
        .retained_open_handles
        .checked_sub(handles_before)
        .ok_or(SourceInventoryError::AccountingOverflow)?;
    let mut continuing = ready.ordinary_budget();
    continuing.charge_source_bytes(0, 0, 0, session_bytes)?;
    continuing.retain_open_handles(session_handles)?;
    inventory.shared_custody().admit_shared(&mut continuing)?;
    session
        .startup_source_validation()
        .validate_with_budget(budget)?;
    future_inventory::validate_inventory(inventory, pending, bundle, budget)?;
    Ok((session, ready, continuing.usage()))
}

impl PreparedPrefabExtractionProject {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn publication(
        &self,
    ) -> &ProjectPrefabExtractionPublication {
        &self.future.publication
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn publication_mut(
        &mut self,
    ) -> &mut ProjectPrefabExtractionPublication {
        &mut self.future.publication
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn basis(
        &self,
    ) -> &AuthoringSchemaBasis {
        self.ready.basis()
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn validate(
        &mut self,
    ) -> Result<(), PrefabExtractionProjectError> {
        self.future.validate()?;
        let inputs = self.future.publication.origin.inputs()?;
        self.session
            .startup_source_validation()
            .validate_with_budget(inputs.budget)?;
        Ok(())
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn host_usage(
        &mut self,
    ) -> Result<AuthoringProjectAdmissionUsage, PrefabExtractionProjectError> {
        Ok(self.future.host_usage()?)
    }

    /// This binds only the final file owner. The enclosing operation separately holds all
    /// Workspace, persistence-leaf and Session installation guards across publication.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn bind(
        mut self,
    ) -> Result<BoundPrefabExtractionProject, PrefabExtractionProjectBindFailure> {
        let result = (|| {
            self.validate()?;
            let budget = self.future.publication.origin.inputs()?.budget;
            with_candidate_storage(budget, |budget| self.ready.bind_publisher(budget))
        })();
        match result {
            Ok(publisher) => Ok(BoundPrefabExtractionProject {
                project: self,
                publisher,
            }),
            Err(error) => Err(PrefabExtractionProjectBindFailure {
                project: self,
                error,
            }),
        }
    }

    #[cfg(test)]
    pub(in crate::project_host::authoring::project::prefab_extraction) fn pending_project(
        &self,
    ) -> &AuthoringProjectSessionSnapshot {
        &self.session
    }

    #[cfg(test)]
    pub(in crate::project_host::authoring::project::prefab_extraction) fn future_inventory(
        &self,
    ) -> &ProjectPrefabExtractionFutureInventory {
        &self.future
    }
}

#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct BoundPrefabExtractionProject
{
    project: PreparedPrefabExtractionProject,
    publisher: FilePublicationHost,
}

#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct PrefabExtractionProjectBindFailure
{
    pub(super) project: PreparedPrefabExtractionProject,
    pub(super) error: PrefabExtractionProjectError,
}

impl fmt::Debug for PrefabExtractionProjectBindFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionProjectBindFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl PrefabExtractionProjectBindFailure {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_parts(
        self,
    ) -> (
        PreparedPrefabExtractionProject,
        PrefabExtractionProjectError,
    ) {
        (self.project, self.error)
    }
}

impl BoundPrefabExtractionProject {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn publication(
        &self,
    ) -> &ProjectPrefabExtractionPublication {
        self.project.publication()
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn publication_mut(
        &mut self,
    ) -> &mut ProjectPrefabExtractionPublication {
        self.project.publication_mut()
    }

    #[cfg(all(test, feature = "tooling"))]
    pub(in crate::project_host::authoring::project::prefab_extraction) fn live_outputs(
        &self,
    ) -> Option<&PreparedProjectPrefabExtractionOutputs> {
        self.project.future.publication.live_outputs()
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn validate(
        &mut self,
    ) -> Result<(), PrefabExtractionProjectError> {
        self.project.validate()
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn host_usage(
        &mut self,
    ) -> Result<AuthoringProjectAdmissionUsage, PrefabExtractionProjectError> {
        self.project.host_usage()
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn cleanup_pending(
        &self,
    ) -> bool {
        self.publisher.has_cleanup()
    }
    pub(in crate::project_host::authoring::project::prefab_extraction) fn uncertain(&self) -> bool {
        self.publisher.uncertain()
    }
    pub(in crate::project_host::authoring::project::prefab_extraction) fn retry_cleanup(
        &mut self,
    ) -> Result<bool, PrefabExtractionProjectError> {
        if !self.cleanup_pending() {
            return Ok(true);
        }
        let budget = self.project.future.publication.origin.inputs()?.budget;
        Ok(
            crate::project_host::authoring::publication_admission::with_cleanup_admission(
                budget,
                || self.publisher.retry_cleanup(),
            )?,
        )
    }

    #[cfg(test)]
    pub(in crate::project_host::authoring::project::prefab_extraction) fn test_fail_next_write(
        &mut self,
    ) -> crate::project_host::file_publication::CleanupTestControl {
        self.publisher.test_fail_after_write()
    }

    /// Every semantic installation guard must already be held by the enclosing operation.
    /// Confirmed publication performs only a private slot fill, Copy updates and owned moves.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn publish(
        mut self,
    ) -> Result<PrefabExtractionProjectOutcome, PrefabExtractionProjectPublicationRefusal> {
        let result = (|| {
            self.project.validate()?;
            let budget = self.project.future.publication.origin.inputs()?.budget;
            with_candidate_storage(budget, |budget| {
                self.project
                    .ready
                    .reserve_publication(&self.publisher, budget)
            })
        })();
        let reservation = match result {
            Ok(reservation) => reservation,
            Err(error) => {
                return Err(PrefabExtractionProjectPublicationRefusal { bound: self, error });
            }
        };
        self.project.ready.assert_private();
        let outcome = self.publisher.publish(self.project.ready.bytes());
        let budget = self
            .project
            .future
            .publication
            .origin
            .inputs()
            .expect("complete extraction retains its original Pending owner")
            .budget;
        budget.retained_candidate_bytes = budget
            .retained_candidate_bytes
            .checked_sub(reservation)
            .expect("publication scratch remains reserved until its physical owner returns");
        match outcome {
            FilePublicationOutcome::Published(accepted) => {
                let Self {
                    mut project,
                    publisher,
                } = self;
                // One retained publisher slot becomes the actual Ready leaf; the old Pending
                // read ends here. The original operation's package/corpus remain separately owned.
                drop(publisher);
                let budget = project
                    .future
                    .publication
                    .origin
                    .inputs()
                    .expect("complete origin")
                    .budget;
                budget.release_open_handles(1);
                let evidence = project
                    .ready
                    .install(accepted, &mut project.session, budget);
                project
                    .session
                    .install_source_observation_custody(project.future.inventory.shared_custody());
                project.session.usage = AuthoringProjectAdmissionUsage {
                    retained_candidate_bytes: project.continuing.retained_candidate_bytes,
                    retained_open_handles: project.continuing.retained_open_handles,
                    ..budget.usage()
                };
                Ok(PrefabExtractionProjectOutcome::Published(
                    PublishedPrefabExtractionProject {
                        session: project.session,
                        files: project.future.publication,
                        inventory: project.future.inventory,
                        evidence,
                    },
                ))
            }
            FilePublicationOutcome::Rejected(reason) => {
                Ok(PrefabExtractionProjectOutcome::Rejected {
                    bound: self,
                    reason,
                })
            }
            FilePublicationOutcome::Failed(stage) => {
                Ok(PrefabExtractionProjectOutcome::Failed { bound: self, stage })
            }
            FilePublicationOutcome::Uncertain(evidence) => {
                Ok(PrefabExtractionProjectOutcome::Uncertain {
                    bound: self,
                    evidence,
                })
            }
        }
    }
}

#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct PublishedPrefabExtractionProject
{
    pub(in crate::project_host::authoring::project::prefab_extraction) session:
        AuthoringProjectSessionSnapshot,
    // These continuing move-only owners must reach the original Workspace/checkpoint guards.
    pub(in crate::project_host::authoring::project::prefab_extraction) files:
        ProjectPrefabExtractionPublication,
    pub(in crate::project_host::authoring::project::prefab_extraction) inventory: SourceInventory,
    pub(in crate::project_host::authoring::project::prefab_extraction) evidence:
        FilePublicationEvidence,
}

#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) enum PrefabExtractionProjectOutcome
{
    Published(PublishedPrefabExtractionProject),
    Rejected {
        bound: BoundPrefabExtractionProject,
        reason: FilePublicationRejection,
    },
    Failed {
        bound: BoundPrefabExtractionProject,
        stage: FilePublicationFailureStage,
    },
    Uncertain {
        bound: BoundPrefabExtractionProject,
        evidence: FilePublicationEvidence,
    },
}

#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct PrefabExtractionProjectPublicationRefusal
{
    pub(super) bound: BoundPrefabExtractionProject,
    pub(super) error: PrefabExtractionProjectError,
}

impl fmt::Debug for PrefabExtractionProjectPublicationRefusal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionProjectPublicationRefusal")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl PrefabExtractionProjectPublicationRefusal {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_parts(
        self,
    ) -> (BoundPrefabExtractionProject, PrefabExtractionProjectError) {
        (self.bound, self.error)
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn error(
        &self,
    ) -> &PrefabExtractionProjectError {
        &self.error
    }
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_bound(
        self,
    ) -> BoundPrefabExtractionProject {
        self.bound
    }
}
