//! One finite Session publication over the exact reviewed extraction owners.
//!
//! Workers retain the original corpus, allowances, native cleanup and failure witnesses.
//! Only the final all-bound Workspace/persistence owner may publish Ready.

use nara_tooling::{EditorPersistenceAuthority, EditorWorkspace};

use super::{
    outputs::{
        recovery::PreparedProjectPrefabExtractionRecovery,
        storage::{
            ProjectPrefabExtractionStorage, ProjectPrefabExtractionStorageCancellation,
            ProjectPrefabExtractionStoragePhase,
        },
    },
    publication::{
        BoundPrefabExtractionProject, PreparedPrefabExtractionProject,
        PreparedPrefabExtractionWorkspaceInstallation, ProjectPrefabExtractionFutureInventory,
        ProjectPrefabExtractionPublication, ProjectPrefabExtractionPublicationPhase,
        RetainedPrefabExtractionInstallation,
    },
    *,
};

mod admission;
mod error;
mod installation;
use error::Failure;
pub(crate) use error::PrefabExtractionSessionPublicationError;
pub(crate) use installation::{
    PrefabExtractionPersistenceDestination, PrefabExtractionSessionInstallation,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PrefabExtractionSessionPublicationPhase {
    Confirming,
    Recovery,
    Pending,
    Sources,
    PreparingReady,
    Ready,
}

enum Stage {
    Confirm(PreparedProjectPrefabExtractionRecovery),
    StorageInput(PreparedProjectPrefabExtractionRecovery),
    Storage(ProjectPrefabExtractionStorage),
    Files(ProjectPrefabExtractionPublication),
    Inventory(ProjectPrefabExtractionPublication),
    Project(ProjectPrefabExtractionFutureInventory),
    BindProject(PreparedPrefabExtractionProject),
    Workspace(BoundPrefabExtractionProject),
    Ready(PreparedPrefabExtractionWorkspaceInstallation),
    CancelledReview(PreparedProjectPrefabExtractionRecovery),
    CancelledStorage(ProjectPrefabExtractionStorageCancellation),
    Installed(RetainedPrefabExtractionInstallation),
}

/// This is a move-only finite operation, not a fresh job or publication transaction factory.
#[must_use]
pub(crate) struct PrefabExtractionSessionPublication {
    stage: Option<Stage>,
    failure: Option<PrefabExtractionSessionPublicationError>,
    retry_requested: bool,
    cancelling: bool,
    pending_attempted: bool,
    task_storage: usize,
    installation_storage: usize,
    sources: Vec<(EditorDocumentId, AuthoringDocumentSourceFact)>,
    originals: Vec<usize>,
}

impl fmt::Debug for PrefabExtractionSessionPublication {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionSessionPublication")
            .field("phase", &self.phase())
            .field("failure", &self.failure)
            .field("pending_attempted", &self.pending_attempted)
            .field("cancelling", &self.cancelling)
            .finish_non_exhaustive()
    }
}

impl PrefabExtractionSessionPublication {
    #[cfg(test)]
    pub(super) fn test_usage(&self) -> AuthoringProjectAdmissionUsage {
        self.context().budget.usage()
    }

    #[cfg(test)]
    pub(super) fn test_original_inventory(&self) -> usize {
        self.context()
            .inventory
            .participants()
            .next()
            .map_or(0, |source| source as *const _ as usize)
    }

    #[cfg(test)]
    pub(super) fn test_exhaust_work(&mut self) {
        let context = self.context_mut();
        context.budget.limits.decode_work =
            nara_core::ItemLimit::new(context.budget.decode_work.max(1)).unwrap();
    }

    #[cfg(test)]
    pub(super) fn test_fail_pending_write(
        &mut self,
    ) -> crate::project_host::file_publication::CleanupTestControl {
        let Some(Stage::Storage(owner)) = self.stage.as_mut() else {
            panic!("expected the original storage owner")
        };
        owner.test_fail_pending_write()
    }

    #[cfg(test)]
    pub(crate) fn test_fail_recovery_write(
        &mut self,
    ) -> Option<crate::project_host::file_publication::CleanupTestControl> {
        let Some(Stage::Storage(owner)) = self.stage.as_mut() else {
            return None;
        };
        (owner.phase() == ProjectPrefabExtractionStoragePhase::Recovery)
            .then(|| owner.test_fail_recovery_write())
    }

    pub(super) fn new(recovery: PreparedProjectPrefabExtractionRecovery) -> Self {
        Self {
            stage: Some(Stage::Confirm(recovery)),
            failure: None,
            retry_requested: false,
            cancelling: false,
            pending_attempted: false,
            task_storage: 0,
            installation_storage: 0,
            sources: Vec::new(),
            originals: Vec::new(),
        }
    }

    pub(crate) fn phase(&self) -> PrefabExtractionSessionPublicationPhase {
        use PrefabExtractionSessionPublicationPhase as P;
        match self.stage.as_ref().expect("one exact publication owner") {
            Stage::Confirm(_) => P::Confirming,
            Stage::StorageInput(_) | Stage::CancelledReview(_) | Stage::CancelledStorage(_) => {
                P::Recovery
            }
            Stage::Storage(storage) => match storage.phase() {
                ProjectPrefabExtractionStoragePhase::Pending => P::Pending,
                ProjectPrefabExtractionStoragePhase::Confirmed => P::Sources,
                _ => P::Recovery,
            },
            Stage::Files(_) => P::Sources,
            Stage::Inventory(_)
            | Stage::Project(_)
            | Stage::BindProject(_)
            | Stage::Workspace(_) => P::PreparingReady,
            Stage::Ready(_) | Stage::Installed(_) => P::Ready,
        }
    }

    pub(crate) fn cancellation_allowed(&self) -> bool {
        !self.pending_attempted
    }

    /// The Session fences cancellation before dispatching the Task that could enter Pending.
    pub(crate) fn pending_attempt_is_next(&self) -> bool {
        !self.cancelling
            && matches!(self.stage.as_ref(), Some(Stage::Storage(storage))
            if storage.phase() == ProjectPrefabExtractionStoragePhase::Pending)
    }

    pub(crate) fn failure(&self) -> Option<&PrefabExtractionSessionPublicationError> {
        self.failure.as_ref()
    }

    pub(crate) fn record_failure(&mut self, error: PrefabExtractionSessionPublicationError) {
        self.failure = Some(error);
    }

    pub(crate) fn can_retry(&self) -> bool {
        self.failure.is_some()
            && !self.retry_requested
            && self.context().budget.ensure_within_limits().is_ok()
            && !self
                .failure
                .as_ref()
                .is_some_and(PrefabExtractionSessionPublicationError::terminal)
            && !self.uncertain()
            && !self.settle_cancellation()
    }

    pub(crate) fn retry_requested(&self) -> bool {
        self.retry_requested
    }

    pub(crate) fn request_retry(&mut self) -> bool {
        if !self.can_retry() {
            return false;
        }
        self.retry_requested = true;
        true
    }

    pub(crate) fn defer_retry(&mut self) {
        self.retry_requested = false;
        if self.failure.is_none() && !self.settle_cancellation() {
            self.failure = Some(Failure::TaskSuspended.into());
        }
    }

    pub(crate) fn request_cancellation(&mut self) {
        if self.cancellation_allowed() && !self.cancelling {
            self.cancelling = true;
            // No native publisher exists in these phases. Move, rather than drop or clone,
            // the exact original owner; cancellation needs no new Task or allowance.
            let stage = self
                .stage
                .take()
                .expect("cancellation retains its exact owner");
            self.stage = Some(match stage {
                Stage::Confirm(owner) | Stage::StorageInput(owner) => Stage::CancelledReview(owner),
                other => other,
            });
            self.retry_requested = !self.settle_cancellation() && !self.uncertain();
        }
    }

    /// Pure observation. Native cleanup and exclusive package release run only in `advance`.
    pub(crate) fn settle_cancellation(&self) -> bool {
        self.cancelling
            && matches!(
                self.stage,
                Some(Stage::CancelledReview(_)) | Some(Stage::CancelledStorage(_))
            )
    }

    /// One finite stage, including a requested native cleanup attempt. All refusals retain the
    /// exact input owner; a caller cannot manufacture another allowance through Retry.
    pub(crate) fn advance(mut self) -> Self {
        let retry = std::mem::take(&mut self.retry_requested);
        if self.failure.is_some() && !retry {
            return self;
        }
        if self.uncertain() {
            return self;
        }
        if self.cancelling {
            if !self.cancellation_allowed() || self.settle_cancellation() {
                return self;
            }
            self.failure = None;
            let stage = self
                .stage
                .take()
                .expect("cancellation retains its exact owner");
            match stage {
                Stage::Confirm(owner) | Stage::StorageInput(owner) => {
                    self.stage = Some(Stage::CancelledReview(owner));
                }
                Stage::Storage(owner) => match owner.cancel() {
                    Ok(cancelled) => self.stage = Some(Stage::CancelledStorage(cancelled)),
                    Err(failure) => {
                        let (owner, error) = failure.into_parts();
                        self.stage = Some(Stage::Storage(owner));
                        self.failure = Some(Failure::Storage(error).into());
                    }
                },
                other => {
                    self.stage = Some(other);
                    self.failure = Some(Failure::Phase.into());
                }
            }
            return self;
        }
        if retry
            && self
                .failure
                .as_ref()
                .is_some_and(PrefabExtractionSessionPublicationError::terminal)
        {
            return self;
        }
        if retry {
            if let Err(error) = self.cleanup_for_retry() {
                self.failure = Some(error);
                return self;
            }
            self.failure = None;
        }
        let stage = self.stage.take().expect("one exact publication owner");
        let (stage, failure) = self.advance_stage(stage);
        self.stage = Some(stage);
        self.failure = failure;
        self
    }

    fn advance_stage(
        &mut self,
        stage: Stage,
    ) -> (Stage, Option<PrefabExtractionSessionPublicationError>) {
        match stage {
            Stage::Confirm(mut owner) => {
                let result = validate_original(&mut owner).and_then(|()| {
                    owner
                        .review_mut()
                        .require_saved_generation()
                        .map_err(Failure::Workspace)
                });
                match result {
                    Ok(()) => (Stage::StorageInput(owner), None),
                    Err(error) => (Stage::Confirm(owner), Some(error.into())),
                }
            }
            Stage::StorageInput(mut owner) => {
                let path = match recovery_path(&mut owner) {
                    Ok(path) => path,
                    Err(error) => return (Stage::StorageInput(owner), Some(error)),
                };
                match owner.prepare_storage_reserved_path(path) {
                    Ok(owner) => (Stage::Storage(owner), None),
                    Err(failure) => {
                        let (owner, error) = failure.into_parts();
                        (Stage::Storage(owner), Some(Failure::Storage(error).into()))
                    }
                }
            }
            Stage::Storage(owner)
                if owner.phase() == ProjectPrefabExtractionStoragePhase::Confirmed =>
            {
                match owner.into_publication() {
                    Ok(owner) => (Stage::Files(owner), None),
                    Err(failure) => {
                        let (owner, error) = failure.into_parts();
                        (Stage::Files(owner), Some(Failure::Files(error).into()))
                    }
                }
            }
            Stage::Storage(owner) => {
                let (owner, error) = match owner.advance() {
                    Ok(owner) => (owner, None),
                    Err(failure) => {
                        let (owner, error) = failure.into_parts();
                        (owner, Some(Failure::Storage(error).into()))
                    }
                };
                self.pending_attempted |= owner.pending_attempted();
                (Stage::Storage(owner), error)
            }
            Stage::Files(owner)
                if owner.phase()
                    == ProjectPrefabExtractionPublicationPhase::CheckpointsComplete =>
            {
                (Stage::Inventory(owner), None)
            }
            Stage::Files(owner) => match owner.advance() {
                Ok(owner) => (Stage::Files(owner), None),
                Err(failure) => {
                    let (owner, error) = failure.into_parts();
                    (Stage::Files(owner), Some(Failure::Files(error).into()))
                }
            },
            Stage::Inventory(owner) => match owner.into_future_inventory() {
                Ok(owner) => (Stage::Project(owner), None),
                Err(failure) => {
                    let (owner, error) = failure.into_parts();
                    (
                        Stage::Inventory(owner),
                        Some(Failure::Inventory(error).into()),
                    )
                }
            },
            Stage::Project(mut owner) => {
                if let Err(error) = installation::prepare_sources(
                    &mut self.sources,
                    &mut self.originals,
                    &mut owner,
                ) {
                    return (Stage::Project(owner), Some(error));
                }
                let limits = owner.publication().session_context().schema.budget.limits;
                let mut host = super::super::super::AuthoringProjectAdmissionHost::new(limits);
                match owner.prepare_project(&mut host) {
                    Ok(owner) => (Stage::BindProject(owner), None),
                    Err(failure) => {
                        let (owner, error) = failure.into_parts();
                        (Stage::Project(owner), Some(Failure::Project(error).into()))
                    }
                }
            }
            Stage::BindProject(owner) => match owner.bind() {
                Ok(owner) => (Stage::Workspace(owner), None),
                Err(failure) => {
                    let (owner, error) = failure.into_parts();
                    (
                        Stage::BindProject(owner),
                        Some(Failure::Project(error).into()),
                    )
                }
            },
            Stage::Workspace(owner) => match owner.into_workspace_installation() {
                Ok(owner) => (Stage::Ready(owner), None),
                Err(failure) => {
                    let (owner, error) = failure.into_parts();
                    (
                        Stage::Workspace(owner),
                        Some(Failure::Project(error).into()),
                    )
                }
            },
            other => (other, None),
        }
    }

    fn cleanup_for_retry(&mut self) -> Result<(), PrefabExtractionSessionPublicationError> {
        // Storage advances/cancels through its own preadmitted cleanup path. File and Ready
        // owners expose explicit cleanup which must physically finish before retrying writes.
        match self
            .stage
            .as_mut()
            .expect("cleanup retains its exact owner")
        {
            Stage::Files(owner) | Stage::Inventory(owner) => {
                if !owner.retry_cleanup().map_err(Failure::Files)? {
                    return Err(Failure::CleanupPending.into());
                }
            }
            Stage::Workspace(owner) => {
                if !owner.retry_cleanup().map_err(Failure::Project)? {
                    return Err(Failure::CleanupPending.into());
                }
            }
            Stage::Ready(owner) => {
                if !owner
                    .project_mut()
                    .retry_cleanup()
                    .map_err(Failure::Project)?
                {
                    return Err(Failure::CleanupPending.into());
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn uncertain(&self) -> bool {
        if self
            .failure
            .as_ref()
            .is_some_and(PrefabExtractionSessionPublicationError::uncertain)
        {
            return true;
        }
        match self
            .stage
            .as_ref()
            .expect("uncertainty retains its exact owner")
        {
            Stage::Storage(owner) => owner.uncertain_publication().is_some(),
            Stage::Files(owner) | Stage::Inventory(owner) => owner.uncertain(),
            Stage::Project(owner) => owner.publication().uncertain(),
            Stage::BindProject(owner) => owner.publication().uncertain(),
            Stage::Workspace(owner) => owner.uncertain(),
            Stage::Ready(owner) => owner.project().uncertain(),
            _ => false,
        }
    }
}

fn validate_original(owner: &mut PreparedProjectPrefabExtractionRecovery) -> Result<(), Failure> {
    use crate::project_host::authoring::source_inventory::with_temporary_storage;
    let context = &mut owner.review_mut().context;
    context
        .destination
        .validate(&context.schema, &context.inventory, &mut context.budget)
        .map_err(Failure::Original)?;
    with_temporary_storage(&mut context.budget, |budget| {
        let bytes = checked_product(context.open.len(), size_of::<OpenSource<'_>>())
            .map_err(PrefabExtractionProjectError::from)
            .map_err(Failure::Original)?;
        budget
            .charge_source_bytes(0, 0, bytes, bytes)
            .map_err(PrefabExtractionProjectError::from)
            .map_err(Failure::Original)?;
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
        context
            .inventory
            .validate(&context.schema, &open, budget)
            .map_err(PrefabExtractionProjectError::from)
            .map_err(Failure::Original)
    })
}

fn recovery_path(
    owner: &mut PreparedProjectPrefabExtractionRecovery,
) -> Result<ProjectPath, PrefabExtractionSessionPublicationError> {
    let length = ".nara-prefab-extraction-".len() + 64 + 1 + 32 + ".json".len();
    let digest = owner
        .recovery()
        .expect("complete Review retains recovery data")
        .digest();
    let budget = &mut owner.review_mut().context.budget;
    admission::admit_storage(budget, length)?;
    let mut identity = [0_u8; 16];
    if let Err(error) = getrandom::fill(&mut identity) {
        budget
            .reconcile_retained_reservation(length, 0)
            .expect("the path was not allocated");
        return Err(Failure::RecoveryIdentity(error).into());
    }
    let mut path = String::with_capacity(length);
    path.push_str(".nara-prefab-extraction-");
    use std::fmt::Write as _;
    for byte in digest.as_bytes() {
        write!(&mut path, "{byte:02x}").expect("String formatting is infallible");
    }
    path.push('-');
    for byte in identity {
        write!(&mut path, "{byte:02x}").expect("String formatting is infallible");
    }
    path.push_str(".json");
    Ok(ProjectPath::new(path).expect("fixed ASCII recovery leaf"))
}
