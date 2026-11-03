//! Finite publication steps for the one reviewed, retained operation.
//!
//! Workers move this owner rather than rebuilding its inventory, preparations or allowance.
//! Ready itself belongs to the final synchronous Workspace/persistence owner turn.

use super::super::source_upgrade_adoption::{
    BoundSourceUpgradeProject, PreparedSourceUpgradeProject, SourceUpgradeProjectError,
};
use super::*;
use crate::project_host::authoring::{
    publication_admission::with_cleanup_admission,
    recovery_storage::{
        BoundRecoveryPending, PendingRecoveryStorage, PreparedRecoveryStorage,
        RecoveryPendingPublication, RecoveryStorageError, RecoveryStorageHost,
        RecoveryStoragePublication, StoredRecovery,
    },
    source_upgrade_inventory::VerifiedSourceUpgradeInventory,
    source_upgrade_publication::{
        SourceUpgradePublication, SourceUpgradePublicationError, SourceUpgradePublicationStep,
    },
    source_upgrade_ready::{
        PreparedSourceUpgradeReady, SourceUpgradePendingManifest, SourceUpgradeReadyError,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthoredSourceUpgradePublicationPhase {
    Confirming,
    Recovery,
    Pending,
    Sources,
    PreparingReady,
    Ready,
}

/// Actual failure witnesses stay in the operation together with their publishers/custody.
#[derive(Debug, thiserror::Error)]
pub(crate) enum AuthoredSourceUpgradePublicationFailure {
    #[error("the original complete source set changed")]
    Original(#[from] SourceUpgradePreparationError),
    #[error("recovery publication was refused")]
    Storage(#[from] RecoveryStorageError),
    #[error("the recovery publication identity could not be generated")]
    RecoveryIdentity(#[source] getrandom::Error),
    #[error("source publication was refused")]
    Source(#[from] SourceUpgradePublicationError),
    #[error("the successor complete inventory was refused")]
    Inventory(SourceUpgradeInventoryError),
    #[error("Ready preparation was refused")]
    Ready(#[from] SourceUpgradeReadyError),
    #[error("saved project preparation was refused")]
    Project(#[from] SourceUpgradeProjectError),
    #[error("the shared operation allowance was exhausted")]
    Admission(#[from] AuthoringProjectAdmissionError),
    #[error("operation storage accounting was refused")]
    Accounting(#[from] AuthoringProjectSourceAdmissionError),
    #[error("the file publisher rejected its expected input: {0:?}")]
    Rejected(crate::project_host::file_publication::FilePublicationRejection),
    #[error("file publication failed at {0:?}")]
    Failed(crate::project_host::file_publication::FilePublicationFailureStage),
    #[error("file publication has uncertain replacement evidence")]
    Uncertain(crate::project_host::file_publication::FilePublicationEvidence),
    #[error("a current persistence destination refused binding: {0:?}")]
    Persistence(nara_tooling::EditorPersistenceRejection),
    #[error("the current Workspace refused the prepared checkpoint binding")]
    Workspace(nara_tooling::EditorWorkspaceCommandReport),
}

impl From<SourceUpgradeInventoryError> for AuthoredSourceUpgradePublicationFailure {
    fn from(error: SourceUpgradeInventoryError) -> Self {
        Self::Inventory(error)
    }
}

enum Stage {
    Confirm,
    CaptureStorage,
    PrepareStorage(RecoveryStorageHost, nara_project::ProjectPath),
    Recovery(PreparedRecoveryStorage<SourceUpgradeRecovery>),
    Stored(StoredRecovery<SourceUpgradeRecovery>),
    Pending(BoundRecoveryPending<SourceUpgradeRecovery>),
    Sources,
    PrepareReady,
    Verify(PreparedSourceUpgradeReady),
    Project(PreparedSourceUpgradeReady, VerifiedSourceUpgradeInventory),
    BindReady(PreparedSourceUpgradeReady, PreparedSourceUpgradeProject),
    BindProject(
        crate::project_host::authoring::source_upgrade_ready::BoundSourceUpgradeReady,
        PreparedSourceUpgradeProject,
    ),
    Ready,
}

pub(crate) struct AuthoredSourceUpgradePublication {
    sources: Option<PreparedAuthoredSourceUpgrade>,
    selected: Option<PreparedSourceUpgradeSchema>,
    budget: AuthoringProjectAdmissionBudget,
    pub(crate) work: AuthoredSchemaUpgradeWork,
    open: Vec<SourceUpgradeOpenInput>,
    stage: Option<Stage>,
    publication: Option<SourceUpgradePublication>,
    pending: Option<PendingRecoveryStorage<SourceUpgradeRecovery>>,
    observations: Vec<AuthoredSourceUpgradeObservedFile>,
    project: Option<BoundSourceUpgradeProject>,
    failure: Option<AuthoredSourceUpgradePublicationFailure>,
    retry_requested: bool,
    cancelling: bool,
    // Set BEFORE asking the Pending publisher to replace the manifest. A lost/uncertain
    // return must never allow cancellation to resume the incumbent's persistence lanes.
    pending_attempted: bool,
    recovery_path: Option<nara_project::ProjectPath>,
    #[cfg(test)]
    refuse_pending_admission: bool,
}

pub(crate) struct AuthoredSourceUpgradeObservedFile {
    pub(crate) document: EditorDocumentId,
    pub(crate) file: Option<nara_fs::FileCapability>,
    pub(crate) source: AuthoringDocumentSourceFact,
}

impl PreparedAuthoredSchemaUpgradeReview {
    pub(crate) fn recovery_digest(&self) -> nara_project::ProjectAuthoringRecoveryDigest {
        self.sources.recovery().digest()
    }

    /// Memory-only confirmation. The finite publication worker rechecks the physical inventory.
    pub(crate) fn validate_live(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        workspace: &EditorWorkspace,
    ) -> Result<(), SourceUpgradeProposalError> {
        if !self.matches(project) {
            // Reuse the existing full validator's specific nominal-authority refusal.
            return self.validate(project, workspace);
        }
        self.sources
            .workspace()
            .validate(workspace, &mut self.work)
            .map_err(SourceUpgradePreparationError::Workspace)
            .map_err(SourceUpgradeProposalError::from)
    }

    pub(crate) fn into_publication_parts(self) -> AuthoredSourceUpgradePublication {
        let Self {
            selected,
            sources,
            budget,
            work,
            open,
        } = self;
        AuthoredSourceUpgradePublication {
            sources: Some(sources),
            selected: Some(selected),
            budget,
            work,
            open,
            stage: Some(Stage::Confirm),
            publication: None,
            pending: None,
            observations: Vec::new(),
            project: None,
            failure: None,
            retry_requested: false,
            cancelling: false,
            pending_attempted: false,
            recovery_path: None,
            #[cfg(test)]
            refuse_pending_admission: false,
        }
    }
}

impl AuthoredSourceUpgradePublication {
    #[cfg(test)]
    pub(crate) fn refuse_next_pending_admission(&mut self) {
        self.refuse_pending_admission = true;
    }

    #[cfg(test)]
    pub(crate) fn test_fail_next_source_write(
        &mut self,
    ) -> crate::project_host::file_publication::CleanupTestControl {
        assert!(matches!(self.stage, Some(Stage::Sources)));
        self.publication.as_mut().unwrap().test_fail_next_write()
    }

    #[cfg(test)]
    pub(crate) fn test_usage(&self) -> AuthoringProjectAdmissionUsage {
        self.budget.usage()
    }

    #[cfg(test)]
    pub(crate) fn test_cleanup_pending(&self) -> bool {
        self.cleanup_pending()
    }

    /// Test teardown only: an exhausted operation cannot authorize another native call.
    #[cfg(test)]
    pub(crate) fn test_discard_source_cleanup(&mut self) -> bool {
        self.publication.as_mut().unwrap().retry_cleanup()
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        self.budget.retained_candidate_bytes
    }

    pub(crate) fn phase(&self) -> AuthoredSourceUpgradePublicationPhase {
        use AuthoredSourceUpgradePublicationPhase as P;
        match self.stage.as_ref().expect("one retained publication phase") {
            Stage::Confirm => P::Confirming,
            Stage::CaptureStorage
            | Stage::PrepareStorage(..)
            | Stage::Recovery(_)
            | Stage::Stored(_) => P::Recovery,
            Stage::Pending(_) => P::Pending,
            Stage::Sources => P::Sources,
            Stage::Ready => P::Ready,
            _ => P::PreparingReady,
        }
    }

    pub(crate) fn cancellation_allowed(&self) -> bool {
        !self.pending_attempted
    }

    pub(crate) fn pending_attempt_is_next(&self) -> bool {
        matches!(self.stage, Some(Stage::Pending(_)))
    }

    pub(crate) fn failure(&self) -> Option<&AuthoredSourceUpgradePublicationFailure> {
        self.failure.as_ref()
    }

    pub(crate) fn can_retry(&self) -> bool {
        use crate::project_host::file_publication::FilePublicationRejection;
        use AuthoredSourceUpgradePublicationFailure as E;
        !self.retry_requested
            && !self.host_work_exhausted()
            && !matches!(
                self.failure,
                Some(E::Uncertain(_))
                    | Some(E::Rejected(FilePublicationRejection::RequiresReconcile))
                    | Some(E::Storage(RecoveryStorageError::MissingPublisher(
                        FilePublicationRejection::RequiresReconcile,
                    )))
                    | Some(E::Persistence(
                        nara_tooling::EditorPersistenceRejection::RequiresReconcile,
                    ))
            )
            && !self.publisher_uncertain()
    }

    /// Requests one finite attempt with the same preparations, native custody and allowance.
    pub(crate) fn request_retry(&mut self) -> bool {
        if !self.can_retry() {
            return false;
        }
        self.retry_requested = true;
        true
    }

    pub(crate) fn retry_requested(&self) -> bool {
        self.retry_requested
    }

    /// A refused Task admission returns its owner without consuming a requested attempt.
    /// Require another explicit request instead of repeatedly resubmitting on every frame.
    pub(crate) fn defer_retry(&mut self) {
        self.retry_requested = false;
    }

    fn publisher_uncertain(&self) -> bool {
        let stage_uncertain = match self.stage.as_ref() {
            Some(Stage::Recovery(prepared)) => prepared.uncertain(),
            Some(Stage::Pending(bound)) => bound.uncertain(),
            Some(Stage::BindProject(ready, _)) => ready.uncertain(),
            _ => false,
        };
        stage_uncertain
            || self
                .publication
                .as_ref()
                .is_some_and(SourceUpgradePublication::uncertain)
            || self
                .project
                .as_ref()
                .is_some_and(BoundSourceUpgradeProject::uncertain)
    }

    pub(crate) fn host_work_exhausted(&self) -> bool {
        self.budget.decode_work > self.budget.limits.decode_work.get()
    }

    pub(crate) fn open_handles_exceeded(&self) -> bool {
        use AuthoredSourceUpgradePublicationFailure as E;
        let admission = match self.failure.as_ref() {
            Some(E::Admission(error))
            | Some(E::Source(SourceUpgradePublicationError::Admission(error)))
            | Some(E::Storage(RecoveryStorageError::Admission(error)))
            | Some(E::Ready(SourceUpgradeReadyError::Admission(error)))
            | Some(E::Project(SourceUpgradeProjectError::Admission(error)))
            | Some(E::Inventory(SourceUpgradeInventoryError::Inventory(
                SourceInventoryError::Admission(error),
            ))) => Some(error),
            _ => None,
        };
        matches!(
            admission,
            Some(AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { .. })
        )
    }

    pub(crate) fn request_cancellation(&mut self) {
        if self.cancellation_allowed() && !self.cancelling {
            self.cancelling = true;
            self.retry_requested = self.cleanup_pending() && !self.host_work_exhausted();
        }
    }

    /// Cancellation never performs native cleanup on the Session owner thread.
    pub(crate) fn settle_cancellation(&self) -> bool {
        self.cancellation_allowed() && !self.cleanup_pending() && !self.publisher_uncertain()
    }

    fn cleanup_pending(&self) -> bool {
        match self.stage.as_ref() {
            Some(Stage::Recovery(prepared)) => prepared.cleanup_pending(),
            Some(Stage::Pending(bound)) => bound.cleanup_pending(),
            Some(Stage::Sources) => self
                .publication
                .as_ref()
                .is_some_and(SourceUpgradePublication::cleanup_pending),
            Some(Stage::Ready) => self
                .project
                .as_ref()
                .is_some_and(BoundSourceUpgradeProject::cleanup_pending),
            _ => false,
        }
    }

    fn retry_cleanup(&mut self) -> Result<bool, AuthoringProjectAdmissionError> {
        if !self.cleanup_pending() {
            return Ok(true);
        }
        match self.stage.as_mut().expect("one retained publication phase") {
            Stage::Recovery(prepared) => {
                with_cleanup_admission(&mut self.budget, || prepared.retry_cleanup())
            }
            Stage::Pending(bound) => {
                with_cleanup_admission(&mut self.budget, || bound.retry_cleanup())
            }
            Stage::Sources => {
                let publication = self
                    .publication
                    .as_mut()
                    .expect("retained source publisher");
                with_cleanup_admission(&mut self.budget, || publication.retry_cleanup())
            }
            Stage::Ready => {
                let project = self.project.as_mut().expect("retained Ready publisher");
                with_cleanup_admission(&mut self.budget, || project.retry_cleanup())
            }
            _ => unreachable!("cleanup belongs to the retained publisher"),
        }
    }

    pub(crate) fn sources(&self) -> &PreparedAuthoredSourceUpgrade {
        self.sources
            .as_ref()
            .expect("the operation retains its complete source preparation")
    }

    /// Called only inside the final synchronous owner turn. Refusal returns these same parts.
    pub(crate) fn take_installation(
        &mut self,
    ) -> (
        PreparedEditorWorkspaceAuthoredSchemaUpgrade,
        AuthoredSourceUpgradeInstallRetention,
        BoundSourceUpgradeProject,
    ) {
        assert_eq!(self.phase(), AuthoredSourceUpgradePublicationPhase::Ready);
        let (workspace, retained) = self.sources.take().unwrap().into_install_parts();
        (workspace, retained, self.project.take().unwrap())
    }

    pub(crate) fn restore_installation(
        &mut self,
        workspace: PreparedEditorWorkspaceAuthoredSchemaUpgrade,
        retained: AuthoredSourceUpgradeInstallRetention,
        project: BoundSourceUpgradeProject,
    ) {
        self.sources = Some(PreparedAuthoredSourceUpgrade::from_install_parts(
            workspace, retained,
        ));
        self.project = Some(project);
    }

    pub(crate) fn record_failure(&mut self, failure: AuthoredSourceUpgradePublicationFailure) {
        self.failure = Some(failure);
    }

    pub(crate) fn admit_installation_storage(
        &mut self,
        bytes: usize,
    ) -> Result<(), AuthoredSourceUpgradePublicationFailure> {
        let before = self.budget.retained_candidate_bytes;
        let result = self.budget.charge_source_bytes(0, 0, bytes, bytes);
        if result.is_err() {
            let unused = self.budget.retained_candidate_bytes - before;
            self.release_installation_storage(unused);
        }
        result.map_err(AuthoredSourceUpgradePublicationFailure::from)
    }

    pub(crate) fn release_installation_storage(&mut self, bytes: usize) {
        self.budget
            .reconcile_retained_reservation(bytes, 0)
            .expect("all final binding collections have dropped");
    }

    pub(crate) fn admit_persistence_validation(
        &mut self,
        cost: crate::project_host::persistence::PersistenceLeafValidationCost,
    ) -> Result<usize, AuthoredSourceUpgradePublicationFailure> {
        self.budget.admit_handle_peak(cost.transient_handles)?;
        let before = self.budget.retained_candidate_bytes;
        if let Err(error) = self.budget.charge_source_bytes(
            cost.encoded_bytes,
            0,
            cost.work_bytes,
            cost.scratch_bytes,
        ) {
            let unused = self.budget.retained_candidate_bytes - before;
            self.release_installation_storage(unused);
            return Err(error.into());
        }
        Ok(cost.scratch_bytes)
    }

    pub(crate) fn publish_project(
        &mut self,
        project: BoundSourceUpgradeProject,
    ) -> Result<
        super::super::source_upgrade_adoption::SourceUpgradeProjectOutcome,
        super::super::source_upgrade_adoption::SourceUpgradeProjectPublicationRefusal,
    > {
        project.publish(&mut self.budget)
    }

    pub(crate) fn take_observations(&mut self) -> Vec<AuthoredSourceUpgradeObservedFile> {
        std::mem::take(&mut self.observations)
    }

    pub(crate) fn restore_observations(
        &mut self,
        observations: Vec<AuthoredSourceUpgradeObservedFile>,
    ) {
        self.observations = observations;
    }

    /// Advances one bounded stage. Refusal returns this exact owner and actual failure witness.
    pub(crate) fn advance(mut self) -> Self {
        if std::mem::take(&mut self.retry_requested) {
            match self.retry_cleanup() {
                Ok(true) => self.failure = None,
                Ok(false) => return self,
                Err(error) => {
                    self.failure = Some(AuthoredSourceUpgradePublicationFailure::Admission(error));
                    return self;
                }
            }
        }
        if self.cancelling {
            return self;
        }
        if self.failure.is_some() || self.phase() == AuthoredSourceUpgradePublicationPhase::Ready {
            return self;
        }
        let stage = self.stage.take().expect("one retained publication phase");
        let (stage, failure) = self.advance_stage(stage);
        self.stage = Some(stage);
        self.failure = failure;
        self
    }

    fn advance_stage(
        &mut self,
        stage: Stage,
    ) -> (Stage, Option<AuthoredSourceUpgradePublicationFailure>) {
        use AuthoredSourceUpgradePublicationFailure as E;
        match stage {
            Stage::Confirm => {
                let before = self.budget.retained_candidate_bytes;
                let result = (|| {
                    let bytes = checked_product(self.open.len(), size_of::<OpenSource<'_>>())?;
                    self.budget.charge_source_bytes(0, 0, bytes, bytes)?;
                    let open: Vec<_> = self
                        .open
                        .iter()
                        .map(|open| match &open.source {
                            Some(source) => OpenSource::Bound(source),
                            None => OpenSource::Unbound,
                        })
                        .collect();
                    let sources = self.sources.as_ref().unwrap();
                    sources
                        .inventory()
                        .validate(sources.schema(), &open, &mut self.budget)?;
                    Ok::<_, SourceUpgradePreparationError>(())
                })();
                let released = self
                    .budget
                    .retained_candidate_bytes
                    .checked_sub(before)
                    .expect("confirmation retains original owners");
                self.budget
                    .reconcile_retained_reservation(released, 0)
                    .expect("temporary confirmation inputs have dropped");
                match result {
                    Ok(()) => (Stage::CaptureStorage, None),
                    Err(error) => (Stage::Confirm, Some(E::Original(error))),
                }
            }
            Stage::CaptureStorage => {
                if self.publication.is_none() {
                    match SourceUpgradePublication::prepare(
                        self.sources.as_ref().unwrap(),
                        &mut self.budget,
                    ) {
                        Ok(publication) => self.publication = Some(publication),
                        Err(error) => return (Stage::CaptureStorage, Some(E::Source(error))),
                    }
                }
                // A fresh publication identity separates identical proposals, including across
                // processes. Retain the exact path on retry; cancelled packages stay durable.
                // A root-level leaf needs no hidden-directory creation authority.
                if self.recovery_path.is_none() {
                    let path_bytes = ".nara-authored-upgrade-".len() + 64 + 1 + 32 + ".json".len();
                    let retained_before = self.budget.retained_candidate_bytes;
                    if let Err(error) = self
                        .budget
                        .charge_source_bytes(0, 0, path_bytes, path_bytes)
                    {
                        let unused = self.budget.retained_candidate_bytes - retained_before;
                        self.release_installation_storage(unused);
                        return (Stage::CaptureStorage, Some(E::Admission(error)));
                    }
                    let mut identity = [0_u8; 16];
                    if let Err(error) = getrandom::fill(&mut identity) {
                        self.release_installation_storage(path_bytes);
                        return (Stage::CaptureStorage, Some(E::RecoveryIdentity(error)));
                    }
                    let mut path = String::with_capacity(path_bytes);
                    path.push_str(".nara-authored-upgrade-");
                    use std::fmt::Write as _;
                    for byte in self.sources().recovery().digest().as_bytes() {
                        write!(&mut path, "{byte:02x}").expect("String formatting is infallible");
                    }
                    path.push('-');
                    for byte in identity {
                        write!(&mut path, "{byte:02x}").expect("String formatting is infallible");
                    }
                    path.push_str(".json");
                    self.recovery_path = Some(
                        nara_project::ProjectPath::new(path).expect("fixed ASCII recovery leaf"),
                    );
                }
                match RecoveryStorageHost::capture(
                    self.sources().schema().project_root(),
                    &mut self.budget,
                ) {
                    Ok(host) => (
                        Stage::PrepareStorage(host, self.recovery_path.take().unwrap()),
                        None,
                    ),
                    Err(error) => (Stage::CaptureStorage, Some(E::Storage(error))),
                }
            }
            Stage::PrepareStorage(host, path) => {
                match host.prepare(self.sources().shared_recovery(), path, &mut self.budget) {
                    Ok(prepared) => (Stage::Recovery(prepared), None),
                    Err(failure) => (
                        Stage::PrepareStorage(failure.host, failure.path),
                        Some(E::Storage(failure.error)),
                    ),
                }
            }
            Stage::Recovery(prepared) => match prepared.publish(&mut self.budget) {
                RecoveryStoragePublication::Published(stored) => (Stage::Stored(stored), None),
                RecoveryStoragePublication::Refused { prepared, error } => {
                    (Stage::Recovery(prepared), Some(E::Storage(error)))
                }
                RecoveryStoragePublication::Rejected { prepared, reason } => {
                    (Stage::Recovery(prepared), Some(E::Rejected(reason)))
                }
                RecoveryStoragePublication::Failed { prepared, stage } => {
                    (Stage::Recovery(prepared), Some(E::Failed(stage)))
                }
                RecoveryStoragePublication::Uncertain {
                    prepared,
                    publication,
                } => (Stage::Recovery(prepared), Some(E::Uncertain(publication))),
            },
            Stage::Stored(stored) => match stored.bind_pending(&mut self.budget) {
                Ok(bound) => (Stage::Pending(bound), None),
                Err(failure) => (
                    Stage::Stored(failure.stored),
                    Some(E::Storage(failure.error)),
                ),
            },
            Stage::Pending(bound) => {
                #[cfg(test)]
                if self.refuse_pending_admission {
                    self.budget.limits.decode_work =
                        nara_core::ItemLimit::new(self.budget.decode_work.max(1)).unwrap();
                    self.refuse_pending_admission = false;
                }
                let previously_attempted = self.pending_attempted;
                self.pending_attempted = true;
                match bound.publish(&mut self.budget) {
                    RecoveryPendingPublication::Published(pending) => {
                        self.pending = Some(pending);
                        (Stage::Sources, None)
                    }
                    RecoveryPendingPublication::Refused { pending, error } => {
                        // This attempt precedes the file publisher. It cannot undo the fence
                        // from an earlier attempted replacement of this same Pending owner.
                        self.pending_attempted = previously_attempted;
                        (Stage::Pending(pending), Some(E::Storage(error)))
                    }
                    RecoveryPendingPublication::Rejected { pending, reason } => {
                        (Stage::Pending(pending), Some(E::Rejected(reason)))
                    }
                    RecoveryPendingPublication::Failed { pending, stage } => {
                        (Stage::Pending(pending), Some(E::Failed(stage)))
                    }
                    RecoveryPendingPublication::Uncertain {
                        pending,
                        publication,
                    } => (Stage::Pending(pending), Some(E::Uncertain(publication))),
                }
            }
            Stage::Sources => {
                let publication = self
                    .publication
                    .as_mut()
                    .expect("pre-Pending publication admission");
                let pending = self
                    .pending
                    .as_ref()
                    .expect("confirmed Pending source authority");
                match publication.publish_next(pending.observed(), &mut self.budget) {
                    SourceUpgradePublicationStep::Complete => (Stage::PrepareReady, None),
                    SourceUpgradePublicationStep::Published { .. }
                    | SourceUpgradePublicationStep::Observed { .. } => (Stage::Sources, None),
                    SourceUpgradePublicationStep::Refused { error, .. } => {
                        (Stage::Sources, Some(E::Source(error)))
                    }
                    SourceUpgradePublicationStep::Rejected { reason, .. } => {
                        (Stage::Sources, Some(E::Rejected(reason)))
                    }
                    SourceUpgradePublicationStep::Failed { stage, .. } => {
                        (Stage::Sources, Some(E::Failed(stage)))
                    }
                    SourceUpgradePublicationStep::Uncertain { evidence, .. } => {
                        (Stage::Sources, Some(E::Uncertain(evidence)))
                    }
                }
            }
            Stage::PrepareReady => {
                let pending = self
                    .pending
                    .as_ref()
                    .expect("confirmed Pending source authority");
                if let Err(error) = self
                    .publication
                    .as_ref()
                    .unwrap()
                    .validate_complete(pending.observed(), &mut self.budget)
                {
                    return (Stage::PrepareReady, Some(E::Source(error)));
                }
                let selected = self
                    .selected
                    .take()
                    .expect("one successor Schema admission");
                match PreparedSourceUpgradeReady::prepare(
                    selected,
                    self.sources.as_ref().unwrap().recovery(),
                    SourceUpgradePendingManifest {
                        capability: pending.capability(),
                        digest: pending.digest(),
                    },
                    &mut self.budget,
                ) {
                    Ok(ready) => (Stage::Verify(ready), None),
                    Err(failure) => {
                        self.selected = Some(failure.selected);
                        (Stage::PrepareReady, Some(E::Ready(failure.error)))
                    }
                }
            }
            Stage::Verify(ready) => {
                let result = self.capture_successor(&ready);
                match result {
                    Ok(verified) => (Stage::Project(ready, verified), None),
                    Err(error) => (Stage::Verify(ready), Some(error)),
                }
            }
            Stage::Project(ready, verified) => match PreparedSourceUpgradeProject::prepare(
                &ready,
                verified,
                self.sources().shared_recovery(),
                self.sources().inventory().scene_limits(),
                &mut self.budget,
            ) {
                Ok(project) => (Stage::BindReady(ready, project), None),
                Err(failure) => (
                    Stage::Project(ready, failure.verified),
                    Some(E::Project(failure.error)),
                ),
            },
            Stage::BindReady(ready, project) => match ready.bind(&mut self.budget) {
                Ok(ready) => (Stage::BindProject(ready, project), None),
                Err(failure) => (
                    Stage::BindReady(failure.prepared, project),
                    Some(E::Ready(failure.error)),
                ),
            },
            Stage::BindProject(ready, project) => match project.bind(ready, &mut self.budget) {
                Ok(project) => {
                    self.project = Some(project);
                    (Stage::Ready, None)
                }
                Err(failure) => (
                    Stage::BindProject(failure.ready, failure.project),
                    Some(E::Project(failure.error)),
                ),
            },
            Stage::Ready => (Stage::Ready, None),
        }
    }

    fn capture_successor(
        &mut self,
        ready: &PreparedSourceUpgradeReady,
    ) -> Result<VerifiedSourceUpgradeInventory, AuthoredSourceUpgradePublicationFailure> {
        // Open only the actual replacement leaves needed by the existing persistence lanes.
        // Charge their complete observation/key/path data before retaining them.
        let count = self.sources().open.len();
        if self.observations.capacity() == 0 && count != 0 {
            let slots = checked_product(count, size_of::<AuthoredSourceUpgradeObservedFile>())?;
            self.budget.charge_source_bytes(0, 0, slots, slots)?;
            self.observations = Vec::with_capacity(count);
        }
        let sources = self.sources.as_ref().unwrap();
        for source in &sources.open[self.observations.len()..] {
            let index = sources
                .recovery()
                .members()
                .binary_search_by(|member| member.path.as_str().cmp(source.source.relative_path()))
                .expect("the admitted complete set contains every open source");
            let observation = self
                .publication
                .as_ref()
                .unwrap()
                .observe_source(index, &mut self.budget)?;
            self.observations.push(AuthoredSourceUpgradeObservedFile {
                document: source.document,
                file: Some(observation.file),
                source: observation.source,
            });
        }
        let comparisons = (usize::BITS - count.leading_zeros()) as usize + 1;
        self.budget
            .charge_source_bytes(0, 0, checked_product(count, comparisons)?, 0)?;
        self.observations
            .sort_unstable_by_key(|observation| observation.document);
        let before = self.budget.retained_candidate_bytes;
        let bytes = checked_product(count, size_of::<OpenSource<'_>>())?;
        self.budget.charge_source_bytes(0, 0, bytes, bytes)?;
        let open: Vec<_> = self
            .observations
            .iter()
            .map(|observation| OpenSource::Bound(&observation.source))
            .collect();
        let result = VerifiedSourceUpgradeInventory::capture(
            ready.observed_pending_schema(),
            sources.recovery(),
            &open,
            sources.inventory().scene_limits(),
            &mut self.budget,
        );
        drop(open);
        // Successful inventory output keeps its own reservation; only the borrowed input Vec ends.
        self.budget
            .reconcile_retained_reservation(bytes, 0)
            .expect("borrowed successor inputs have dropped");
        debug_assert!(self.budget.retained_candidate_bytes >= before);
        result.map_err(AuthoredSourceUpgradePublicationFailure::from)
    }
}
