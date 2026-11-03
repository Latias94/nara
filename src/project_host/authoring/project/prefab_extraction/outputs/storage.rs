//! The concrete fixed-output owner through package and Pending publication.
//!
//! The original Review, nominal checkpoints, source Work and Host budget move together. No
//! source or Q is published here, and a confirmed Pending token does not establish Ready.

use super::super::recovery_data::{PrefabExtractionRecovery, PrefabExtractionRecoveryError};
use super::recovery::PreparedProjectPrefabExtractionRecovery;
use super::*;
use crate::project_host::{
    authoring::{
        publication_admission::with_cleanup_admission,
        recovery_storage::{
            BoundRecoveryPending, PendingRecoveryStorage, PreparedRecoveryStorage,
            RecoveryPendingPublication, RecoveryStorageError, RecoveryStorageHost,
            RecoveryStoragePublication, StoredRecovery,
        },
    },
    file_publication::{
        FilePublicationEvidence, FilePublicationFailureStage, FilePublicationRejection,
    },
};

mod installation;
#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;
pub(in crate::project_host::authoring::project::prefab_extraction) use installation::LivePrefabExtractionInstallation;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(in crate::project_host::authoring::project::prefab_extraction) enum ProjectPrefabExtractionStoragePhase
{
    Sharing,
    Capturing,
    Preparing,
    Recovery,
    Stored,
    Pending,
    Confirmed,
    Cancelling,
}

enum Stage {
    Share(ProjectPath),
    Capture(ProjectPath),
    Prepare(RecoveryStorageHost, ProjectPath),
    Recovery(PreparedRecoveryStorage<PrefabExtractionRecovery>),
    Stored(StoredRecovery<PrefabExtractionRecovery>),
    Pending(BoundRecoveryPending<PrefabExtractionRecovery>),
    Confirmed(PendingRecoveryStorage<PrefabExtractionRecovery>),
    Cancelling,
}

#[derive(Debug)]
pub(in crate::project_host::authoring::project::prefab_extraction) enum ProjectPrefabExtractionStorageError
{
    Output(ProjectPrefabExtractionOutputError),
    Data(PrefabExtractionRecoveryError),
    Storage(RecoveryStorageError),
    Rejected(FilePublicationRejection),
    Failed(FilePublicationFailureStage),
    Uncertain,
    IncompleteRecovery,
    PendingAttempted,
    RecoveryShared,
    CleanupPending,
    Cancelling,
    InstallationPhase,
}

impl From<RecoveryStorageError> for ProjectPrefabExtractionStorageError {
    fn from(error: RecoveryStorageError) -> Self {
        Self::Storage(error)
    }
}

/// The transport's Arc cannot outlive an independently cancellable package owner. This
/// aggregate owns both; cancellation restores exclusive data ownership before any refund.
#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct ProjectPrefabExtractionStorage
{
    owner: PreparedProjectPrefabExtractionRecovery,
    recovery: Option<Arc<PrefabExtractionRecovery>>,
    stage: Stage,
    initialized: bool,
    pending_attempted: bool,
    uncertain_publication: Option<FilePublicationEvidence>,
    orphan_publication: Option<FilePublicationEvidence>,
    retained_before: usize,
    handles_before: usize,
    path_reserved: usize,
}

impl fmt::Debug for ProjectPrefabExtractionStorage {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectPrefabExtractionStorage")
            .field("owner", &self.owner)
            .field("phase", &self.phase())
            .field("pending_attempted", &self.pending_attempted)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct ProjectPrefabExtractionStorageFailure
{
    storage: ProjectPrefabExtractionStorage,
    error: ProjectPrefabExtractionStorageError,
}

/// Cancellation can leave a confirmed package on disk while Ready remains unchanged. Its
/// actual publication witness stays owned and charged until this cancellation record ends.
#[derive(Debug)]
#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct ProjectPrefabExtractionStorageCancellation
{
    prepared: PreparedProjectPrefabExtractionRecovery,
    orphan_publication: Option<FilePublicationEvidence>,
    witness_bytes: usize,
}

impl ProjectPrefabExtractionStorageCancellation {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn session_context_mut(
        &mut self,
    ) -> &mut ProjectExtractionContext {
        &mut self.prepared.outputs.review.context
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn prepared(
        &self,
    ) -> &PreparedProjectPrefabExtractionRecovery {
        &self.prepared
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn orphan_publication(
        &self,
    ) -> Option<&FilePublicationEvidence> {
        self.orphan_publication.as_ref()
    }

    /// Ends this cancellation record and releases its witness storage after dropping the
    /// actual receipt. A durable orphan package is intentionally left at its published path.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_recovery(
        self,
    ) -> PreparedProjectPrefabExtractionRecovery {
        let Self {
            mut prepared,
            orphan_publication,
            witness_bytes,
        } = self;
        drop(orphan_publication);
        let budget = &mut prepared.outputs.review.context.budget;
        budget.retained_candidate_bytes = budget
            .retained_candidate_bytes
            .checked_sub(witness_bytes)
            .expect("the cancellation witness remains charged until its record ends");
        prepared
    }
}

impl PreparedProjectPrefabExtractionRecovery {
    /// Consumes the entire recovery owner before any immutable package sharing or physical I/O.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn prepare_storage(
        self,
        path: ProjectPath,
    ) -> Result<ProjectPrefabExtractionStorage, ProjectPrefabExtractionStorageFailure> {
        self.prepare_storage_owned_path(path, 0)
    }

    /// The Session generated this exact path only after admitting its allocation. Transfer
    /// that existing reservation into storage instead of charging a second copy of the path.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn prepare_storage_reserved_path(
        self,
        path: ProjectPath,
    ) -> Result<ProjectPrefabExtractionStorage, ProjectPrefabExtractionStorageFailure> {
        let reserved = path.as_str().len();
        self.prepare_storage_owned_path(path, reserved)
    }

    fn prepare_storage_owned_path(
        self,
        path: ProjectPath,
        path_reserved: usize,
    ) -> Result<ProjectPrefabExtractionStorage, ProjectPrefabExtractionStorageFailure> {
        let retained_before = self
            .outputs
            .review
            .context
            .budget
            .retained_candidate_bytes
            .checked_sub(path_reserved)
            .expect("an already admitted path moves with its existing live reservation");
        let handles_before = self.outputs.review.context.budget.retained_open_handles;
        ProjectPrefabExtractionStorage {
            owner: self,
            recovery: None,
            stage: Stage::Share(path),
            initialized: false,
            pending_attempted: false,
            uncertain_publication: None,
            orphan_publication: None,
            retained_before,
            handles_before,
            path_reserved,
        }
        .advance()
    }
}

impl ProjectPrefabExtractionStorageFailure {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_parts(
        self,
    ) -> (
        ProjectPrefabExtractionStorage,
        ProjectPrefabExtractionStorageError,
    ) {
        (self.storage, self.error)
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn error(
        &self,
    ) -> &ProjectPrefabExtractionStorageError {
        &self.error
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn storage(
        &self,
    ) -> &ProjectPrefabExtractionStorage {
        &self.storage
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn retry(
        self,
    ) -> Result<ProjectPrefabExtractionStorage, Self> {
        self.storage.advance()
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn cancel(
        self,
    ) -> Result<ProjectPrefabExtractionStorageCancellation, Self> {
        self.storage.cancel()
    }
}

impl ProjectPrefabExtractionStorage {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn pending_attempted(
        &self,
    ) -> bool {
        self.pending_attempted
    }

    /// External Task/installation storage survives native cancellation. Keep its original
    /// cancellation baseline synchronized with actual reservation and physical refunds.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn adjust_session_storage(
        &mut self,
        added: usize,
        released: usize,
    ) {
        self.retained_before = self
            .retained_before
            .checked_add(added)
            .and_then(|bytes| bytes.checked_sub(released))
            .expect("external storage remains part of the original cancellation baseline");
    }

    #[cfg(test)]
    pub(in crate::project_host::authoring::project::prefab_extraction) fn test_fail_recovery_write(
        &mut self,
    ) -> crate::project_host::file_publication::CleanupTestControl {
        let Stage::Recovery(prepared) = &mut self.stage else {
            panic!("the test must stop before its actual recovery package attempt")
        };
        prepared.test_fail_after_write()
    }

    #[cfg(test)]
    pub(in crate::project_host::authoring::project::prefab_extraction) fn test_fail_pending_write(
        &mut self,
    ) -> crate::project_host::file_publication::CleanupTestControl {
        let Stage::Pending(pending) = &mut self.stage else {
            panic!("the test must stop before its actual Pending attempt")
        };
        pending.test_fail_after_write()
    }

    /// The actual Pending receipt authorizes a state-only manifest transition. Reobserve the
    /// original source configuration without pretending its old Ready leaf is current.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn validate_publication_handoff(
        &mut self,
    ) -> Result<(), super::super::publication::ProjectPrefabExtractionPublicationError> {
        use super::super::publication::ProjectPrefabExtractionPublicationError as Error;
        use crate::project_host::authoring::source_inventory::{
            SourceInventoryInput, revalidate_file_facts, with_temporary_storage,
        };
        let Stage::Confirmed(pending) = &self.stage else {
            return Err(Error::Phase);
        };
        self.owner
            .outputs
            .require_saved_generation()
            .map_err(Error::Workspace)?;
        let context = &mut self.owner.outputs.review.context;
        let actual = pending.observed();
        actual.validate(&mut context.budget)?;
        let schema = &context.schema;
        let receipt = pending.pending_publication().receipt();
        context.budget.charge_source_bytes(
            0,
            0,
            checked_sum([
                schema.manifest_bytes().len(),
                actual.recovery().old_ready_manifest().bytes().len(),
            ])?,
            0,
        )?;
        if receipt.previous_identity() != Some(schema.manifest_identity())
            || receipt.published_identity() != Some(actual.capability().identity())
            || receipt.verified_content_digest() != Some(actual.digest())
            || schema.manifest_bytes() != actual.recovery().old_ready_manifest().bytes()
        {
            return Err(Error::SourceChanged);
        }
        with_temporary_storage(&mut context.budget, |budget| {
            let bytes = checked_product(context.open.len(), size_of::<OpenSource<'_>>())?;
            budget.charge_source_bytes(0, 0, bytes, bytes)?;
            let open: Vec<_> = context
                .open
                .iter()
                .map(|open| {
                    open.source
                        .as_ref()
                        .map(OpenSource::Bound)
                        .unwrap_or(OpenSource::Unbound)
                })
                .collect();
            // These are the real original admission facts. The concrete receipt above, not
            // these old fields, proves which manifest is currently authoritative.
            let root = schema.project_root();
            revalidate_file_facts(
                &root,
                schema.bundle_path().as_str(),
                schema.bundle_identity(),
                schema.bundle_parent_identity(),
                schema.bundle_digest(),
                budget,
            )?;
            context.inventory.validate_admitted_observations(
                SourceInventoryInput {
                    root: &root,
                    manifest_bytes: schema.manifest_bytes(),
                    manifest_identity: schema.manifest_identity(),
                    manifest_digest: schema.manifest_digest(),
                    bundle_path: schema.bundle_path().as_str(),
                    bundle_identity: schema.bundle_identity(),
                    bundle_digest: schema.bundle_digest(),
                    selected_settings: Some(schema.settings()),
                },
                &open,
                budget,
            )?;
            Ok::<_, Error>(())
        })?;
        actual.validate(&mut context.budget)?;
        Ok(())
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn publication_inputs(
        &mut self,
    ) -> Option<super::super::publication::PublicationInputs<'_>> {
        let Stage::Confirmed(pending) = &self.stage else {
            return None;
        };
        let context = &mut self.owner.outputs.review.context;
        Some(super::super::publication::PublicationInputs {
            pending: pending.observed(),
            inventory: &context.inventory,
            budget: &mut context.budget,
            original_open: Some(&context.open),
            bundle: super::super::publication::PublicationBundle {
                path: context.schema.bundle_path().as_str(),
                identity: context.schema.bundle_identity(),
                parent: context.schema.bundle_parent_identity(),
                digest: context.schema.bundle_digest(),
            },
            schema: super::super::publication::PublicationSchema::Live {
                receipt: &context.schema,
                adapters: &context.adapters,
                basis: context.basis.clone(),
            },
        })
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn phase(
        &self,
    ) -> ProjectPrefabExtractionStoragePhase {
        match &self.stage {
            Stage::Share(_) => ProjectPrefabExtractionStoragePhase::Sharing,
            Stage::Capture(_) => ProjectPrefabExtractionStoragePhase::Capturing,
            Stage::Prepare(..) => ProjectPrefabExtractionStoragePhase::Preparing,
            Stage::Recovery(_) => ProjectPrefabExtractionStoragePhase::Recovery,
            Stage::Stored(_) => ProjectPrefabExtractionStoragePhase::Stored,
            Stage::Pending(_) => ProjectPrefabExtractionStoragePhase::Pending,
            Stage::Confirmed(_) => ProjectPrefabExtractionStoragePhase::Confirmed,
            Stage::Cancelling => ProjectPrefabExtractionStoragePhase::Cancelling,
        }
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn outputs(
        &self,
    ) -> &PreparedProjectPrefabExtractionOutputs {
        &self.owner.outputs
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn outputs_mut(
        &mut self,
    ) -> &mut PreparedProjectPrefabExtractionOutputs {
        &mut self.owner.outputs
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn recovery(
        &self,
    ) -> Option<&PrefabExtractionRecovery> {
        self.recovery.as_deref().or(self.owner.recovery.as_ref())
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn pending(
        &self,
    ) -> Option<&PendingRecoveryStorage<PrefabExtractionRecovery>> {
        match &self.stage {
            Stage::Confirmed(pending) => Some(pending),
            _ => None,
        }
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn uncertain_publication(
        &self,
    ) -> Option<&FilePublicationEvidence> {
        self.uncertain_publication.as_ref()
    }

    /// Performs one finite physical stage, retaining the exact phase on refusal. Confirmed
    /// Pending is terminal for this slice; subsequent source publication belongs to its owner.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn advance(
        mut self,
    ) -> Result<Self, ProjectPrefabExtractionStorageFailure> {
        match self.advance_stage() {
            Ok(()) => Ok(self),
            Err(error) => Err(ProjectPrefabExtractionStorageFailure {
                storage: self,
                error,
            }),
        }
    }

    /// Ready cancellation retains any confirmed orphan receipt, discharges native cleanup and
    /// restores exclusive package ownership. A possible Pending replacement fences this path.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn cancel(
        mut self,
    ) -> Result<ProjectPrefabExtractionStorageCancellation, ProjectPrefabExtractionStorageFailure>
    {
        if self.pending_attempted {
            return Err(ProjectPrefabExtractionStorageFailure {
                storage: self,
                error: ProjectPrefabExtractionStorageError::PendingAttempted,
            });
        }
        if let Err(error) = self.release_for_cancellation() {
            return Err(ProjectPrefabExtractionStorageFailure {
                storage: self,
                error,
            });
        }
        if let Some(recovery) = self.recovery.take() {
            match Arc::try_unwrap(recovery) {
                Ok(recovery) => self.owner.recovery = Some(recovery),
                Err(recovery) => {
                    self.recovery = Some(recovery);
                    return Err(ProjectPrefabExtractionStorageFailure {
                        storage: self,
                        error: ProjectPrefabExtractionStorageError::RecoveryShared,
                    });
                }
            }
        }
        // Publishers, native observations, the path and Arc controls have actually dropped.
        // The returned original package retains its original reservation until its own cancel.
        let witness_bytes = if self.orphan_publication.is_some() {
            size_of::<FilePublicationEvidence>()
        } else {
            0
        };
        let budget = &mut self.owner.outputs.review.context.budget;
        budget.retained_candidate_bytes = self.retained_before + witness_bytes;
        budget.retained_open_handles = self.handles_before;
        Ok(ProjectPrefabExtractionStorageCancellation {
            prepared: self.owner,
            orphan_publication: self.orphan_publication,
            witness_bytes,
        })
    }

    fn release_for_cancellation(&mut self) -> Result<(), ProjectPrefabExtractionStorageError> {
        let uncertain = self.uncertain_publication.is_some()
            || match &self.stage {
                Stage::Recovery(prepared) => prepared.uncertain(),
                Stage::Pending(pending) => pending.uncertain(),
                _ => false,
            };
        if uncertain {
            return Err(ProjectPrefabExtractionStorageError::Uncertain);
        }
        self.cleanup()?;
        match std::mem::replace(&mut self.stage, Stage::Cancelling) {
            Stage::Recovery(prepared) => match prepared.release_unpublished() {
                Ok(recovery) => drop(recovery),
                Err(prepared) => {
                    self.stage = Stage::Recovery(prepared);
                    return Err(ProjectPrefabExtractionStorageError::CleanupPending);
                }
            },
            Stage::Pending(pending) => match pending.release_before_pending() {
                Ok(stored) => {
                    let (recovery, publication) = stored.release_orphan();
                    drop(recovery);
                    self.orphan_publication = Some(publication);
                }
                Err(pending) => {
                    self.stage = Stage::Pending(pending);
                    return Err(ProjectPrefabExtractionStorageError::CleanupPending);
                }
            },
            Stage::Stored(stored) => {
                let (recovery, publication) = stored.release_orphan();
                drop(recovery);
                self.orphan_publication = Some(publication);
            }
            Stage::Confirmed(pending) => {
                self.stage = Stage::Confirmed(pending);
                return Err(ProjectPrefabExtractionStorageError::PendingAttempted);
            }
            other => drop(other),
        }
        Ok(())
    }

    fn advance_stage(&mut self) -> Result<(), ProjectPrefabExtractionStorageError> {
        if self.uncertain_publication.is_some() {
            return Err(ProjectPrefabExtractionStorageError::Uncertain);
        }
        self.cleanup()?;
        let stage = std::mem::replace(&mut self.stage, Stage::Cancelling);
        match stage {
            Stage::Share(path) => {
                let path_bytes = path.as_str().len();
                self.stage = Stage::Share(path);
                if !self.initialized {
                    let bytes = checked_sum([
                        size_of::<Self>(),
                        size_of::<ProjectPrefabExtractionStorageFailure>(),
                        size_of::<ProjectPrefabExtractionStorageCancellation>(),
                        path_bytes,
                    ])
                    .map_err(ProjectPrefabExtractionOutputError::from)
                    .map_err(ProjectPrefabExtractionStorageError::Output)?;
                    self.owner
                        .outputs
                        .admit(Some(bytes))
                        .map_err(ProjectPrefabExtractionStorageError::Output)?;
                    let budget = &mut self.owner.outputs.review.context.budget;
                    let before = budget.retained_candidate_bytes;
                    let retained = bytes
                        .checked_sub(self.path_reserved)
                        .expect("storage metadata includes its already admitted path");
                    if let Err(error) = budget.charge_source_bytes(0, 0, bytes, retained) {
                        budget.retained_candidate_bytes = before;
                        return Err(RecoveryStorageError::Admission(error).into());
                    }
                    self.initialized = true;
                }
                let recovery = self
                    .owner
                    .recovery
                    .take()
                    .ok_or(ProjectPrefabExtractionStorageError::IncompleteRecovery)?;
                match recovery.into_shared(&mut self.owner.outputs.review.context.budget) {
                    Ok(recovery) => self.recovery = Some(recovery),
                    Err(failure) => {
                        self.owner.recovery = Some(failure.recovery);
                        return Err(ProjectPrefabExtractionStorageError::Data(failure.error));
                    }
                }
                let Stage::Share(path) = std::mem::replace(&mut self.stage, Stage::Cancelling)
                else {
                    unreachable!("sharing retains its path throughout admission")
                };
                self.stage = Stage::Capture(path);
            }
            Stage::Capture(path) => {
                let context = &mut self.owner.outputs.review.context;
                match RecoveryStorageHost::capture(
                    context.schema.project_root(),
                    &mut context.budget,
                ) {
                    Ok(host) => self.stage = Stage::Prepare(host, path),
                    Err(error) => {
                        self.stage = Stage::Capture(path);
                        return Err(error.into());
                    }
                }
            }
            Stage::Prepare(host, path) => {
                let recovery = Arc::clone(
                    self.recovery
                        .as_ref()
                        .expect("storage preparation follows immutable package admission"),
                );
                match host.prepare(
                    recovery,
                    path,
                    &mut self.owner.outputs.review.context.budget,
                ) {
                    Ok(prepared) => self.stage = Stage::Recovery(prepared),
                    Err(failure) => {
                        self.stage = Stage::Prepare(failure.host, failure.path);
                        return Err(failure.error.into());
                    }
                }
            }
            Stage::Recovery(prepared) => {
                match prepared.publish(&mut self.owner.outputs.review.context.budget) {
                    RecoveryStoragePublication::Published(stored) => {
                        self.stage = Stage::Stored(stored)
                    }
                    RecoveryStoragePublication::Refused { prepared, error } => {
                        self.stage = Stage::Recovery(prepared);
                        return Err(error.into());
                    }
                    RecoveryStoragePublication::Rejected { prepared, reason } => {
                        self.stage = Stage::Recovery(prepared);
                        return Err(ProjectPrefabExtractionStorageError::Rejected(reason));
                    }
                    RecoveryStoragePublication::Failed { prepared, stage } => {
                        self.stage = Stage::Recovery(prepared);
                        return Err(ProjectPrefabExtractionStorageError::Failed(stage));
                    }
                    RecoveryStoragePublication::Uncertain {
                        prepared,
                        publication,
                    } => {
                        self.stage = Stage::Recovery(prepared);
                        self.uncertain_publication = Some(publication);
                        return Err(ProjectPrefabExtractionStorageError::Uncertain);
                    }
                }
            }
            Stage::Stored(stored) => {
                match stored.bind_pending(&mut self.owner.outputs.review.context.budget) {
                    Ok(pending) => self.stage = Stage::Pending(pending),
                    Err(failure) => {
                        self.stage = Stage::Stored(failure.stored);
                        return Err(failure.error.into());
                    }
                }
            }
            Stage::Pending(pending) => {
                let previously_attempted = self.pending_attempted;
                self.pending_attempted = true;
                match pending.publish(&mut self.owner.outputs.review.context.budget) {
                    RecoveryPendingPublication::Published(pending) => {
                        self.stage = Stage::Confirmed(pending)
                    }
                    RecoveryPendingPublication::Refused { pending, error } => {
                        // No file publisher was entered. Preserve a fence from an earlier
                        // actual attempt, but do not invent one for this admission refusal.
                        self.pending_attempted = previously_attempted;
                        self.stage = Stage::Pending(pending);
                        return Err(error.into());
                    }
                    RecoveryPendingPublication::Rejected { pending, reason } => {
                        self.stage = Stage::Pending(pending);
                        return Err(ProjectPrefabExtractionStorageError::Rejected(reason));
                    }
                    RecoveryPendingPublication::Failed { pending, stage } => {
                        self.stage = Stage::Pending(pending);
                        return Err(ProjectPrefabExtractionStorageError::Failed(stage));
                    }
                    RecoveryPendingPublication::Uncertain {
                        pending,
                        publication,
                    } => {
                        self.stage = Stage::Pending(pending);
                        self.uncertain_publication = Some(publication);
                        return Err(ProjectPrefabExtractionStorageError::Uncertain);
                    }
                }
            }
            Stage::Confirmed(pending) => self.stage = Stage::Confirmed(pending),
            Stage::Cancelling => return Err(ProjectPrefabExtractionStorageError::Cancelling),
        }
        Ok(())
    }

    fn cleanup(&mut self) -> Result<(), ProjectPrefabExtractionStorageError> {
        let needed = match &self.stage {
            Stage::Recovery(prepared) => prepared.cleanup_pending(),
            Stage::Pending(pending) => pending.cleanup_pending(),
            _ => false,
        };
        if !needed {
            return Ok(());
        }
        let budget = &mut self.owner.outputs.review.context.budget;
        let cleaned = with_cleanup_admission(budget, || match &mut self.stage {
            Stage::Recovery(prepared) => prepared.retry_cleanup(),
            Stage::Pending(pending) => pending.retry_cleanup(),
            _ => true,
        })
        .map_err(RecoveryStorageError::Admission)?;
        if cleaned {
            Ok(())
        } else {
            Err(ProjectPrefabExtractionStorageError::CleanupPending)
        }
    }
}
