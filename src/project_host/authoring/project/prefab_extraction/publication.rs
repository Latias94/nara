//! Concrete extraction files under confirmed Pending custody.
//!
//! Live checkpoints and recovered semantic proofs keep their original owners. Successful file
//! results stay move-only here until their later checkpoint/future-project consumers accept them.
//! FilesComplete is still Pending; this owner cannot publish Ready or install a Workspace.

use nara_fs::RelativeComponent;

#[cfg(feature = "tooling")]
use super::outputs::storage::{
    LivePrefabExtractionInstallation, ProjectPrefabExtractionStorage,
    ProjectPrefabExtractionStoragePhase,
};
use super::{
    recovered_generation::{
        RecoveredPrefabExtractionGeneration, RecoveredPrefabExtractionGenerationError,
        RecoveredPrefabExtractionGenerationPhase,
    },
    recovery_data::PrefabExtractionRecovery,
    *,
};
#[cfg(feature = "tooling")]
use crate::project_host::persistence::{
    ExtractionCheckpointAcceptance, ExtractionPersistenceError,
};
use crate::project_host::{
    authoring::{
        AuthoringProjectAdmissionUsage,
        recovery_storage::{ObservedPendingRecoveryStorage, RecoveryStorageError},
        source_inventory::{SourceFileObservation, SourceInventoryError, product, release, sum},
    },
    file_publication::{
        AcceptedFilePublication, FilePublicationBinding, FilePublicationEvidence,
        FilePublicationFailureStage, FilePublicationHost, FilePublicationOutcome,
        FilePublicationRejection,
    },
};

mod checkpoints;
mod future_inventory;
mod future_project;
pub(super) use future_inventory::{
    ProjectPrefabExtractionFutureInventory, ProjectPrefabExtractionFutureInventoryError,
    ProjectPrefabExtractionFutureInventoryFailure,
};
#[cfg(feature = "tooling")]
pub(crate) use future_project::ExtractionPersistenceDestination;
pub(super) use future_project::{
    BoundPrefabExtractionProject, PrefabExtractionProjectError, PrefabExtractionProjectOutcome,
    PrefabExtractionProjectPreparationFailure, PreparedPrefabExtractionProject,
    PublishedPrefabExtractionProject,
};
#[cfg(feature = "tooling")]
pub(super) use future_project::{
    InstalledPrefabExtractionProject, PrefabExtractionInstallationError,
    PrefabExtractionInstallationFailure, PreparedPrefabExtractionWorkspaceInstallation,
    RetainedPrefabExtractionInstallation,
};

const NATIVE_SCRATCH: usize = 2 * 65_536;
const HANDLE_PEAK: usize = 16;
const ACTIVE_HANDLES: usize = 2;

pub(super) struct PublicationInputs<'a> {
    pub(super) pending: &'a ObservedPendingRecoveryStorage<PrefabExtractionRecovery>,
    pub(super) inventory: &'a SourceInventory,
    pub(super) budget: &'a mut AuthoringProjectAdmissionBudget,
    pub(super) bundle: PublicationBundle<'a>,
    pub(super) schema: PublicationSchema<'a>,
    // Only live installation has original Workspace document destinations. Recovered origins
    // carry no substitute or fabricated open set; headless builds omit this field entirely.
    #[cfg(feature = "tooling")]
    pub(super) original_open: Option<&'a [PrefabExtractionOpenInput]>,
}

/// Original live definitions or the one model admission performed by this recovery process.
/// Neither variant creates a successor basis when files reach their final names.
pub(super) enum PublicationSchema<'a> {
    #[cfg(feature = "tooling")]
    Live {
        receipt: &'a Arc<AuthoringSchemaReceipt>,
        adapters: &'a [AuthoringAdapterReceipt],
        basis: nara_scene::AuthoringSchemaBasis,
    },
    Recovered(&'a crate::project_host::authoring::schema_models::SelectedSchemaModels),
}

#[derive(Clone, Copy)]
pub(super) struct PublicationBundle<'a> {
    pub(super) path: &'a str,
    pub(super) identity: FileIdentity,
    pub(super) parent: Option<FileIdentity>,
    pub(super) digest: ContentDigest,
}

enum Origin {
    #[cfg(feature = "tooling")]
    Live(ProjectPrefabExtractionStorage),
    #[cfg(feature = "tooling")]
    LiveInstallation(LivePrefabExtractionInstallation),
    Recovered(RecoveredPrefabExtractionGeneration),
}

impl Origin {
    fn validate_handoff(&mut self) -> Result<(), ProjectPrefabExtractionPublicationError> {
        match self {
            #[cfg(feature = "tooling")]
            Self::Live(owner) => owner.validate_publication_handoff(),
            #[cfg(feature = "tooling")]
            Self::LiveInstallation(_) => Err(ProjectPrefabExtractionPublicationError::Phase),
            Self::Recovered(owner) => owner
                .validate()
                .map_err(ProjectPrefabExtractionPublicationError::Generation),
        }
    }

    fn inputs(&mut self) -> Result<PublicationInputs<'_>, ProjectPrefabExtractionPublicationError> {
        match self {
            #[cfg(feature = "tooling")]
            Self::Live(owner) => owner.publication_inputs(),
            #[cfg(feature = "tooling")]
            Self::LiveInstallation(owner) => Some(owner.publication_inputs()),
            Self::Recovered(owner) => owner.publication_inputs(),
        }
        .ok_or(ProjectPrefabExtractionPublicationError::Phase)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ProjectPrefabExtractionPublicationPhase {
    Preparing,
    Files,
    FilesComplete,
    Checkpoints,
    CheckpointsComplete,
}

/// Fixed package indices, independent of sparse source outputs and per-configuration membership.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ExtractionFileMember {
    Prefab,
    Metadata,
    Original(usize),
}

#[derive(Debug, Clone, Copy)]
enum Expected {
    Missing {
        parent: FileIdentity,
    },
    Existing {
        identity: FileIdentity,
        parent: FileIdentity,
        digest: ContentDigest,
    },
}

impl Expected {
    fn parent(self) -> FileIdentity {
        match self {
            Self::Missing { parent } | Self::Existing { parent, .. } => parent,
        }
    }

    fn binding(self) -> FilePublicationBinding {
        match self {
            Self::Missing { .. } => FilePublicationBinding::Missing,
            Self::Existing {
                identity, digest, ..
            } => FilePublicationBinding::Existing { identity, digest },
        }
    }
}

/// Reading an already-current output never creates historical replacement evidence.
#[derive(Debug)]
pub(super) enum ExtractionFileResult {
    Published(AcceptedFilePublication),
    Observed {
        file: FileCapability,
        digest: ContentDigest,
    },
    #[cfg(feature = "tooling")]
    Checkpoint(ExtractionCheckpointAcceptance),
}

impl ExtractionFileResult {
    pub(super) fn observation(&self) -> (FileIdentity, Option<FileIdentity>, ContentDigest) {
        match self {
            Self::Published(accepted) => (
                accepted.source().identity(),
                accepted.source().parent_identity(),
                accepted.source().digest(),
            ),
            Self::Observed { file, digest } => (file.identity(), file.parent_identity(), *digest),
            #[cfg(feature = "tooling")]
            Self::Checkpoint(acceptance) => acceptance.observation(),
        }
    }

    fn accepted_checkpoint(&self) -> bool {
        match self {
            #[cfg(feature = "tooling")]
            Self::Checkpoint(acceptance) => acceptance.accepted(),
            _ => false,
        }
    }
}

#[derive(Debug)]
struct Entry {
    member: ExtractionFileMember,
    expected: Expected,
    result: Option<ExtractionFileResult>,
}

#[must_use]
pub(super) struct ProjectPrefabExtractionPublication {
    origin: Origin,
    entries: Vec<Entry>,
    initialized: bool,
    position: usize,
    checkpoint_position: Option<usize>,
    active: Option<FilePublicationHost>,
    active_bytes: usize,
    uncertain: Option<FilePublicationEvidence>,
    #[cfg(test)]
    fail_next_write: Option<crate::project_host::file_publication::CleanupTestControl>,
}

impl fmt::Debug for ProjectPrefabExtractionPublication {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectPrefabExtractionPublication")
            .field("phase", &self.phase())
            .field("position", &self.position)
            .field("entries", &self.entries)
            .field("cleanup_pending", &self.cleanup_pending())
            .field("uncertain", &self.uncertain.is_some())
            .finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum ProjectPrefabExtractionPublicationError {
    #[error("extraction files require confirmed Pending and a complete saved generation")]
    Phase,
    #[error("extraction Pending storage changed")]
    Pending(#[from] RecoveryStorageError),
    #[error("extraction file admission exceeded its original allowance")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("extraction publication source inventory changed")]
    Inventory(#[from] SourceInventoryError),
    #[cfg(feature = "tooling")]
    #[error("extraction saved-checkpoint proof is stale")]
    Workspace(nara_tooling::EditorWorkspaceCommandReport),
    #[cfg(feature = "tooling")]
    #[error("extraction save checkpoint could not accept its actual file result")]
    Checkpoint(ExtractionPersistenceError),
    #[error("recovered extraction generation is stale")]
    Generation(RecoveredPrefabExtractionGenerationError),
    #[error("extraction file source or retained ancestor changed")]
    SourceChanged,
    #[cfg(feature = "tooling")]
    #[error("completed extraction installation could not detach its original owners: {0:?}")]
    Installation(super::outputs::storage::ProjectPrefabExtractionStorageError),
    #[error("extraction publication was rejected: {0:?}")]
    Rejected(FilePublicationRejection),
    #[error("extraction publication failed: {0:?}")]
    Failed(FilePublicationFailureStage),
    #[error("extraction publication has unresolved physical uncertainty")]
    Uncertain,
}

impl From<AuthoringProjectSourceAdmissionError> for ProjectPrefabExtractionPublicationError {
    fn from(error: AuthoringProjectSourceAdmissionError) -> Self {
        Self::Inventory(SourceInventoryError::Native(error))
    }
}

#[derive(Debug)]
#[must_use]
pub(super) struct ProjectPrefabExtractionPublicationFailure {
    owner: ProjectPrefabExtractionPublication,
    error: ProjectPrefabExtractionPublicationError,
}

impl ProjectPrefabExtractionPublicationFailure {
    pub(super) fn into_parts(
        self,
    ) -> (
        ProjectPrefabExtractionPublication,
        ProjectPrefabExtractionPublicationError,
    ) {
        (self.owner, self.error)
    }

    pub(super) fn owner(&self) -> &ProjectPrefabExtractionPublication {
        &self.owner
    }
    pub(super) fn error(&self) -> &ProjectPrefabExtractionPublicationError {
        &self.error
    }
    pub(super) fn retry(self) -> Result<ProjectPrefabExtractionPublication, Self> {
        self.owner.advance()
    }
    pub(super) fn into_owner(self) -> ProjectPrefabExtractionPublication {
        self.owner
    }
}

#[cfg(feature = "tooling")]
impl ProjectPrefabExtractionStorage {
    pub(super) fn into_publication(
        self,
    ) -> Result<ProjectPrefabExtractionPublication, ProjectPrefabExtractionPublicationFailure> {
        let ready = self.phase() == ProjectPrefabExtractionStoragePhase::Confirmed;
        ProjectPrefabExtractionPublication::new(Origin::Live(self), ready)
    }
}

impl RecoveredPrefabExtractionGeneration {
    pub(super) fn into_publication(
        self,
    ) -> Result<ProjectPrefabExtractionPublication, ProjectPrefabExtractionPublicationFailure> {
        let ready = self.phase() == RecoveredPrefabExtractionGenerationPhase::Complete;
        ProjectPrefabExtractionPublication::new(Origin::Recovered(self), ready)
    }
}

impl ProjectPrefabExtractionPublication {
    #[cfg(feature = "tooling")]
    pub(super) fn session_context(&self) -> &ProjectExtractionContext {
        match &self.origin {
            Origin::Live(storage) => &storage.outputs().review().context,
            Origin::LiveInstallation(owner) => owner.session_context(),
            Origin::Recovered(_) => unreachable!("a Session publication retains its live origin"),
        }
    }

    #[cfg(feature = "tooling")]
    pub(super) fn session_context_mut(&mut self) -> &mut ProjectExtractionContext {
        match &mut self.origin {
            Origin::Live(storage) => storage.outputs_mut().session_context_mut(),
            Origin::LiveInstallation(owner) => owner.session_context_mut(),
            Origin::Recovered(_) => unreachable!("a Session publication retains its live origin"),
        }
    }

    fn new(origin: Origin, ready: bool) -> Result<Self, ProjectPrefabExtractionPublicationFailure> {
        let owner = Self {
            origin,
            entries: Vec::new(),
            initialized: false,
            position: 0,
            checkpoint_position: None,
            active: None,
            active_bytes: 0,
            uncertain: None,
            #[cfg(test)]
            fail_next_write: None,
        };
        if !ready {
            return Err(ProjectPrefabExtractionPublicationFailure {
                owner,
                error: ProjectPrefabExtractionPublicationError::Phase,
            });
        }
        owner.advance()
    }

    pub(super) fn phase(&self) -> ProjectPrefabExtractionPublicationPhase {
        if !self.initialized {
            ProjectPrefabExtractionPublicationPhase::Preparing
        } else if self.position == self.entries.len() {
            match self.checkpoint_position {
                None => ProjectPrefabExtractionPublicationPhase::FilesComplete,
                Some(position) if position < self.entries.len() => {
                    ProjectPrefabExtractionPublicationPhase::Checkpoints
                }
                Some(_) => ProjectPrefabExtractionPublicationPhase::CheckpointsComplete,
            }
        } else {
            ProjectPrefabExtractionPublicationPhase::Files
        }
    }

    pub(super) fn position(&self) -> usize {
        self.position
    }
    pub(super) fn file_count(&self) -> usize {
        self.entries.len()
    }
    pub(super) fn checkpoint_position(&self) -> Option<usize> {
        self.checkpoint_position
    }
    pub(super) fn result(
        &self,
        index: usize,
    ) -> Option<(ExtractionFileMember, &ExtractionFileResult)> {
        let entry = self.entries.get(index)?;
        Some((entry.member, entry.result.as_ref()?))
    }
    #[cfg(feature = "tooling")]
    pub(super) fn live_outputs(&self) -> Option<&PreparedProjectPrefabExtractionOutputs> {
        match &self.origin {
            Origin::Live(storage) => Some(storage.outputs()),
            _ => None,
        }
    }
    pub(super) fn recovered_generation(&self) -> Option<&RecoveredPrefabExtractionGeneration> {
        match &self.origin {
            Origin::Recovered(owner) => Some(owner),
            #[cfg(feature = "tooling")]
            _ => None,
        }
    }
    pub(super) fn host_usage(
        &mut self,
    ) -> Result<AuthoringProjectAdmissionUsage, ProjectPrefabExtractionPublicationError> {
        Ok(self.origin.inputs()?.budget.usage())
    }
    pub(super) fn cleanup_pending(&self) -> bool {
        self.active
            .as_ref()
            .is_some_and(FilePublicationHost::has_cleanup)
    }
    pub(super) fn uncertain(&self) -> bool {
        self.uncertain.is_some()
            || self
                .active
                .as_ref()
                .is_some_and(FilePublicationHost::uncertain)
    }
    pub(super) fn uncertain_evidence(&self) -> Option<&FilePublicationEvidence> {
        self.uncertain.as_ref()
    }
    pub(super) fn retry_cleanup(
        &mut self,
    ) -> Result<bool, ProjectPrefabExtractionPublicationError> {
        if !self.cleanup_pending() {
            return Ok(true);
        }
        let budget = self.origin.inputs()?.budget;
        Ok(
            crate::project_host::authoring::publication_admission::with_cleanup_admission(
                budget,
                || {
                    self.active
                        .as_mut()
                        .expect("pending cleanup retains its publisher")
                        .retry_cleanup()
                },
            )?,
        )
    }
    #[cfg(test)]
    pub(super) fn test_fail_next_write(
        &mut self,
    ) -> crate::project_host::file_publication::CleanupTestControl {
        let control = crate::project_host::file_publication::CleanupTestControl::blocked();
        self.fail_next_write = Some(control.clone());
        control
    }

    pub(super) fn advance(mut self) -> Result<Self, ProjectPrefabExtractionPublicationFailure> {
        match self.step() {
            Ok(()) => Ok(self),
            Err(error) => Err(ProjectPrefabExtractionPublicationFailure { owner: self, error }),
        }
    }

    fn step(&mut self) -> Result<(), ProjectPrefabExtractionPublicationError> {
        if self.uncertain() {
            return Err(ProjectPrefabExtractionPublicationError::Uncertain);
        }
        if !self.initialized {
            self.origin.validate_handoff()?;
        }
        let inputs = self.origin.inputs()?;
        inputs.pending.validate(inputs.budget)?;
        if !self.initialized {
            let recovery = inputs.pending.recovery();
            inputs.budget.charge_source_bytes(
                0,
                0,
                product(
                    recovery.complete_original_sources().len(),
                    size_of::<usize>(),
                )?,
                0,
            )?;
            let count = sum(
                2,
                recovery
                    .complete_original_sources()
                    .iter()
                    .filter(|source| source.output().is_some())
                    .count(),
            )?;
            let bytes = sum(
                product(count, size_of::<Entry>())?,
                size_of::<Self>() + size_of::<ProjectPrefabExtractionPublicationFailure>(),
            )?;
            let retained = inputs.budget.retained_candidate_bytes;
            let result = (|| {
                inputs.budget.charge_source_bytes(0, 0, bytes, bytes)?;
                let mut entries = Vec::with_capacity(count);
                for member in [ExtractionFileMember::Prefab, ExtractionFileMember::Metadata]
                    .into_iter()
                    .chain(
                        recovery
                            .complete_original_sources()
                            .iter()
                            .enumerate()
                            .filter(|(_, source)| source.output().is_some())
                            .map(|(index, _)| ExtractionFileMember::Original(index)),
                    )
                {
                    let (path, _, _) = member_bytes(recovery, member)?;
                    // AcceptedFilePublication retains the actual leaf name as well as its fixed
                    // inline owner. Reserve that successful output before any file can change.
                    inputs
                        .budget
                        .charge_source_bytes(0, 0, path.len(), path.len())?;
                    let expected = expected(inputs.inventory, path, member, inputs.budget)?;
                    entries.push(Entry {
                        member,
                        expected,
                        result: None,
                    });
                }
                Ok::<_, ProjectPrefabExtractionPublicationError>(entries)
            })();
            match result {
                Ok(entries) => {
                    self.entries = entries;
                    self.initialized = true;
                }
                Err(error) => {
                    let dropped = inputs
                        .budget
                        .retained_candidate_bytes
                        .checked_sub(retained)
                        .ok_or(SourceInventoryError::AccountingOverflow)?;
                    release(inputs.budget, dropped)?;
                    return Err(error);
                }
            }
            return Ok(());
        }
        // Fixed output results remain authoritative only while their exact names still select
        // those retained leaves. This does not pretend the old full inventory is unchanged.
        for entry in &self.entries[..self.position] {
            let (path, _, _) = member_bytes(inputs.pending.recovery(), entry.member)?;
            let result = entry
                .result
                .as_ref()
                .ok_or(ProjectPrefabExtractionPublicationError::Phase)?;
            let (identity, parent, digest) = result.observation();
            let file = verify_target(
                inputs.pending.root(),
                inputs.inventory,
                path,
                Expected::Existing {
                    identity,
                    parent: parent.ok_or(ProjectPrefabExtractionPublicationError::SourceChanged)?,
                    digest,
                },
                inputs.budget,
            )?;
            drop(file);
        }
        if self.position == self.entries.len() {
            // End the file-validation borrow before transferring one original checkpoint.
            return self.accept_next_checkpoint();
        }
        let entry = &mut self.entries[self.position];
        let (path, bytes, digest) = member_bytes(inputs.pending.recovery(), entry.member)?;
        if matches!(entry.expected, Expected::Existing { digest: previous, .. } if previous == digest)
        {
            let retained = inputs.budget.retained_candidate_bytes;
            let handles = inputs.budget.retained_open_handles;
            let result = (|| {
                inputs.budget.retain_open_handles(1)?;
                let file = verify_target(
                    inputs.pending.root(),
                    inputs.inventory,
                    path,
                    entry.expected,
                    inputs.budget,
                )?
                .ok_or(ProjectPrefabExtractionPublicationError::SourceChanged)?;
                Ok::<_, ProjectPrefabExtractionPublicationError>(file)
            })();
            match result {
                Ok(file) => entry.result = Some(ExtractionFileResult::Observed { file, digest }),
                Err(error) => {
                    restore_failed(inputs.budget, retained, handles)?;
                    return Err(error);
                }
            }
            self.position += 1;
            return Ok(());
        }
        if self.active.is_none() {
            let retained = inputs.budget.retained_candidate_bytes;
            let handles = inputs.budget.retained_open_handles;
            let active_bytes = sum(
                size_of::<FilePublicationHost>() + 4096,
                product(path.len(), 4)?,
            )?;
            let result = with_path(path, inputs.budget, |budget| {
                budget.charge_source_bytes(0, 0, 0, active_bytes)?;
                budget.retain_open_handles(ACTIVE_HANDLES)?;
                let file = verify_target(
                    inputs.pending.root(),
                    inputs.inventory,
                    path,
                    entry.expected,
                    budget,
                )?;
                let publisher = match file {
                    Some(file) => {
                        let parent = inputs
                            .inventory
                            .shared_parent(path)
                            .ok_or(ProjectPrefabExtractionPublicationError::SourceChanged)?;
                        let leaf = RelativeComponent::new(path.rsplit('/').next().unwrap_or(path))
                            .map_err(|_| ProjectPrefabExtractionPublicationError::SourceChanged)?;
                        FilePublicationHost::from_existing(
                            parent,
                            leaf,
                            file,
                            match entry.expected {
                                Expected::Existing { digest, .. } => digest,
                                _ => return Err(ProjectPrefabExtractionPublicationError::Phase),
                            },
                            match entry.expected {
                                Expected::Existing { digest, .. } => {
                                    digest.length().max(bytes.len() as u64)
                                }
                                Expected::Missing { .. } => {
                                    return Err(ProjectPrefabExtractionPublicationError::Phase);
                                }
                            },
                        )
                    }
                    None => FilePublicationHost::bind_missing(
                        inputs.pending.root(),
                        path,
                        bytes.len() as u64,
                    )
                    .map_err(ProjectPrefabExtractionPublicationError::Rejected)?,
                };
                if publisher.parent_identity() != entry.expected.parent() {
                    return Err(ProjectPrefabExtractionPublicationError::SourceChanged);
                }
                Ok(publisher)
            });
            match result {
                Ok(publisher) => {
                    self.active = Some(publisher);
                    self.active_bytes = active_bytes;
                }
                Err(error) => {
                    restore_failed(inputs.budget, retained, handles)?;
                    return Err(error);
                }
            }
        }
        // A retained publisher is not proof that its parent is still reachable at the selected
        // path. Recheck even on cleanup/retry, before publishing into that retained directory.
        drop(verify_target(
            inputs.pending.root(),
            inputs.inventory,
            path,
            entry.expected,
            inputs.budget,
        )?);
        let publisher = self
            .active
            .as_mut()
            .ok_or(ProjectPrefabExtractionPublicationError::Phase)?;
        #[cfg(test)]
        if let Some(control) = self.fail_next_write.take() {
            publisher.test_fail_after_write_with_control(&control);
        }
        let cost = publisher
            .binding()
            .checked_content_cost(bytes.len())
            .ok_or(SourceInventoryError::AccountingOverflow)?;
        let scratch = sum(
            NATIVE_SCRATCH + 4096,
            sum(bytes.len(), product(path.len(), 4)?)?,
        )?;
        let retained = inputs.budget.retained_candidate_bytes;
        let handles = inputs.budget.retained_open_handles;
        let reservation = (|| {
            inputs.budget.retain_open_handles(1)?;
            inputs.budget.admit_handle_peak(HANDLE_PEAK)?;
            inputs.budget.charge_source_bytes(
                cost.encoded_read_bytes,
                0,
                sum(cost.content_work_bytes, path.len())?,
                scratch,
            )?;
            Ok::<_, ProjectPrefabExtractionPublicationError>(())
        })();
        if let Err(error) = reservation {
            restore_failed(inputs.budget, retained, handles)?;
            return Err(error);
        }
        let outcome = publisher.publish(bytes);
        release(inputs.budget, scratch)?;
        match outcome {
            FilePublicationOutcome::Published(accepted) => {
                entry.result = Some(ExtractionFileResult::Published(accepted));
                drop(self.active.take());
                release(inputs.budget, self.active_bytes)?;
                inputs.budget.release_open_handles(ACTIVE_HANDLES);
                self.active_bytes = 0;
                self.position += 1;
                Ok(())
            }
            FilePublicationOutcome::Rejected(reason) => {
                inputs.budget.release_open_handles(1);
                Err(ProjectPrefabExtractionPublicationError::Rejected(reason))
            }
            FilePublicationOutcome::Failed(stage) => {
                inputs.budget.release_open_handles(1);
                Err(ProjectPrefabExtractionPublicationError::Failed(stage))
            }
            FilePublicationOutcome::Uncertain(evidence) => {
                inputs.budget.release_open_handles(1);
                self.uncertain = Some(evidence);
                Err(ProjectPrefabExtractionPublicationError::Uncertain)
            }
        }
    }
}

fn member_bytes(
    recovery: &PrefabExtractionRecovery,
    member: ExtractionFileMember,
) -> Result<(&str, &[u8], ContentDigest), ProjectPrefabExtractionPublicationError> {
    let (path, blob) = match member {
        ExtractionFileMember::Prefab => (recovery.q().path(), recovery.q().output()),
        ExtractionFileMember::Metadata => {
            (recovery.q_sidecar().path(), recovery.q_sidecar().output())
        }
        ExtractionFileMember::Original(index) => {
            let source = recovery
                .complete_original_sources()
                .get(index)
                .ok_or(ProjectPrefabExtractionPublicationError::Phase)?;
            (
                source.path(),
                source
                    .output()
                    .ok_or(ProjectPrefabExtractionPublicationError::Phase)?,
            )
        }
    };
    let digest = blob.digest();
    Ok((
        path,
        blob.bytes(),
        ContentDigest::from_parts(digest.length(), *digest.as_bytes()),
    ))
}

fn expected(
    inventory: &SourceInventory,
    path: &str,
    member: ExtractionFileMember,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Expected, ProjectPrefabExtractionPublicationError> {
    budget.charge_source_bytes(
        0,
        0,
        product(
            path.len(),
            (usize::BITS - inventory.participants().len().leading_zeros()) as usize + 2,
        )?,
        0,
    )?;
    let parent = inventory
        .shared_parent(path)
        .ok_or(ProjectPrefabExtractionPublicationError::SourceChanged)?
        .identity();
    match member {
        ExtractionFileMember::Original(_) => {
            let fact = inventory
                .participant(path)
                .ok_or(ProjectPrefabExtractionPublicationError::SourceChanged)?
                .file();
            if fact.parent_identity() != Some(parent) {
                return Err(ProjectPrefabExtractionPublicationError::SourceChanged);
            }
            Ok(Expected::Existing {
                identity: fact.identity(),
                parent,
                digest: fact.digest(),
            })
        }
        ExtractionFileMember::Prefab | ExtractionFileMember::Metadata => match inventory
            .file_observation(path)
        {
            None | Some(SourceFileObservation::Missing { .. }) => Ok(Expected::Missing { parent }),
            Some(SourceFileObservation::Present { file, .. })
                if file.parent_identity() == Some(parent) =>
            {
                Ok(Expected::Existing {
                    identity: file.identity(),
                    parent,
                    digest: file.digest(),
                })
            }
            _ => Err(ProjectPrefabExtractionPublicationError::SourceChanged),
        },
    }
}

fn verify_target(
    root: &DirectoryCapability,
    inventory: &SourceInventory,
    path: &str,
    expected: Expected,
    budget: &mut AuthoringProjectAdmissionBudget,
) -> Result<Option<FileCapability>, ProjectPrefabExtractionPublicationError> {
    with_path(path, budget, |budget| {
        // Checking only the final parent's object would miss a retained subtree moved beneath
        // a replacement ancestor. Every original path prefix keeps its own custody fact.
        let depth = path.bytes().filter(|byte| *byte == b'/').count();
        budget.charge_source_bytes(0, 0, product(path.len(), sum(depth, 1)?)?, 0)?;
        for (end, _) in path.match_indices('/') {
            let retained = inventory
                .shared_parent(&path[..=end])
                .ok_or(ProjectPrefabExtractionPublicationError::SourceChanged)?;
            let prefix = RelativePath::new(&path[..end])
                .map_err(|_| ProjectPrefabExtractionPublicationError::SourceChanged)?;
            let current = root
                .open_directory_exact(&prefix)
                .map_err(|_| ProjectPrefabExtractionPublicationError::SourceChanged)?;
            if retained.identity() != current.identity() {
                return Err(ProjectPrefabExtractionPublicationError::SourceChanged);
            }
        }
        let relative = RelativePath::new(path)
            .map_err(|_| ProjectPrefabExtractionPublicationError::SourceChanged)?;
        let (parent, leaf) = root
            .resolve_file_parent(&relative)
            .map_err(|_| ProjectPrefabExtractionPublicationError::SourceChanged)?;
        let original_parent = inventory
            .shared_parent(path)
            .ok_or(ProjectPrefabExtractionPublicationError::SourceChanged)?;
        if parent.identity() != expected.parent() || parent.identity() != original_parent.identity()
        {
            return Err(ProjectPrefabExtractionPublicationError::SourceChanged);
        }
        match expected {
            Expected::Missing { .. } => match parent.open_child_file(&leaf) {
                Err(FsError::Io { source, .. })
                    if source.kind() == std::io::ErrorKind::NotFound =>
                {
                    Ok(None)
                }
                _ => Err(ProjectPrefabExtractionPublicationError::SourceChanged),
            },
            Expected::Existing {
                identity, digest, ..
            } => {
                let file = root
                    .open_file_exact(&relative)
                    .map_err(|_| ProjectPrefabExtractionPublicationError::SourceChanged)?;
                let length = usize::try_from(digest.length())
                    .map_err(|_| SourceInventoryError::AccountingOverflow)?;
                let read_bound = sum(length, 1)?;
                budget.charge_source_bytes(read_bound, 0, read_bound, 0)?;
                if file.identity() != identity
                    || file.parent_identity() != Some(parent.identity())
                    || file.verify_digest(digest, digest.length()).is_err()
                {
                    return Err(ProjectPrefabExtractionPublicationError::SourceChanged);
                }
                Ok(Some(file))
            }
        }
    })
}

fn with_path<T>(
    path: &str,
    budget: &mut AuthoringProjectAdmissionBudget,
    inspect: impl FnOnce(
        &mut AuthoringProjectAdmissionBudget,
    ) -> Result<T, ProjectPrefabExtractionPublicationError>,
) -> Result<T, ProjectPrefabExtractionPublicationError> {
    budget.charge_source_bytes(0, 0, path.len(), 0)?;
    let shape = RelativePath::preflight(path)
        .map_err(|_| ProjectPrefabExtractionPublicationError::SourceChanged)?;
    budget.charge_reference(path.len(), shape.components(), true)?;
    budget.admit_handle_peak(HANDLE_PEAK)?;
    let scratch = sum(
        NATIVE_SCRATCH,
        sum(product(path.len(), 4)?, product(shape.components(), 128)?)?,
    )?;
    let before = budget.retained_candidate_bytes;
    if let Err(error) = budget.charge_source_bytes(0, 0, 0, scratch) {
        let refused = budget
            .retained_candidate_bytes
            .checked_sub(before)
            .ok_or(SourceInventoryError::AccountingOverflow)?;
        release(budget, refused)?;
        return Err(error.into());
    }
    let result = inspect(budget);
    release(budget, scratch)?;
    result
}

fn restore_failed(
    budget: &mut AuthoringProjectAdmissionBudget,
    retained: usize,
    handles: usize,
) -> Result<(), ProjectPrefabExtractionPublicationError> {
    let dropped = budget
        .retained_candidate_bytes
        .checked_sub(retained)
        .ok_or(SourceInventoryError::AccountingOverflow)?;
    release(budget, dropped)?;
    let closed = budget
        .retained_open_handles
        .checked_sub(handles)
        .ok_or(SourceInventoryError::AccountingOverflow)?;
    budget.release_open_handles(closed);
    Ok(())
}
