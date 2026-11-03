//! Explicit process recovery through the existing source, Schema and Ready owners.
//!
//! No old Session, history, nominal token or replacement witness is reconstructed. Inspection
//! admits one complete mixed-generation inventory and verifies the stored working/output pairs.
//! The same owner and allowances survive confirmation, publication refusals and cleanup retries.

use nara_fs::{ReplaceReceipt, StageStatus};
use nara_project::ProjectAuthoringRecoveryDigest;
use nara_reflect::{ComponentFieldId, ComponentPropertyPath, ComponentTypeId};
use nara_scene::{
    __private::{
        AuthoredSchemaUpgradeWork, validate_authored_upgrade_prefab_transition,
        validate_authored_upgrade_scene_transition,
    },
    SceneFileEncoding, ScenePatchReport, SourceDocumentKey,
};

use super::source_upgrade_adoption::{
    BoundSourceUpgradeProject, PreparedSourceUpgradeProject, PublishedSourceUpgradeProject,
    SourceUpgradeProjectError, SourceUpgradeProjectOutcome,
};
use super::*;
use crate::project_host::{
    authoring::{
        AuthoredSchemaUpgradeHistoryPolicy, AuthoringProjectAdmissionHost,
        AuthoringProjectAdmissionLimits, AuthoringSourceOperationLimits,
        PendingAuthoringRecoveryInspection, PendingAuthoringRecoveryInspectionError,
        PendingAuthoringRecoveryInspectionErrorKind, PendingAuthoringRecoveryKind, document,
        publication_admission::with_cleanup_admission,
        recovery_storage::ObservedPendingRecoveryStorage,
        source_upgrade_inventory::{
            RecoveredSourceUpgradeInventory, SourceUpgradeInventoryError,
            VerifiedSourceUpgradeInventory,
        },
        source_upgrade_publication::{
            SourceUpgradePublication, SourceUpgradePublicationError, SourceUpgradePublicationStep,
        },
        source_upgrade_ready::{
            BoundSourceUpgradeReady, PreparedSourceUpgradeReady, PreparedSourceUpgradeSchema,
            SourceUpgradePendingManifest, SourceUpgradeReadyError,
        },
        source_upgrade_recovery::{
            RecoveryMemberState, RecoverySourceEncoding, SourceUpgradeRecovery,
            SourceUpgradeRecoveryLimits,
        },
        source_upgrade_storage::{self, SourceUpgradeStorageError},
    },
    file_publication::{
        FilePublicationEvidence, FilePublicationFailureStage, FilePublicationRejection,
    },
};

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;
#[cfg(test)]
use crate::project_host::authoring::publication_admission::CLEANUP_WORK;

/// A complete recovery has one explicit confirmation before any source replacement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoredSchemaUpgradeRecoveryPhase {
    Review,
    PublishingSources,
    PreparingReady,
    PublishingReady,
    Complete,
}

/// Current-content classification at inspection. It is not historical publication evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoredSchemaUpgradeRecoveryMemberState {
    Original,
    Successor,
    Unchanged,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthoredSchemaUpgradeRecoveryErrorKind {
    Phase,
    Admission,
    Pending,
    Inventory,
    Schema,
    SourceDecoding,
    SourceTransition,
    SourcePublication,
    Ready,
    Project,
    PublicationRejected,
    PublicationFailed,
    PublicationUncertain,
}

/// Cumulative work and observed peaks for this operation, including failed attempts.
/// These are not the continuing admission allowance of a subsequently opened project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthoredSchemaUpgradeRecoveryUsage {
    pub host_work: usize,
    pub peak_open_handles: usize,
    pub source_work_bytes: usize,
    pub source_peak_retained_bytes: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AuthoredSchemaUpgradeRecoveryView {
    pub operation: ProjectAuthoringRecoveryDigest,
    pub phase: AuthoredSchemaUpgradeRecoveryPhase,
    pub source_count: usize,
    /// Completed source observations/replacements, excluding the bundle.
    pub completed_sources: usize,
    pub bundle_complete: bool,
    pub refusal: Option<AuthoredSchemaUpgradeRecoveryErrorKind>,
    pub cleanup_pending: bool,
    pub uncertain: bool,
    pub limits: AuthoringSourceOperationLimits,
    pub usage: AuthoredSchemaUpgradeRecoveryUsage,
}

/// One member of the authenticated package and its actual inspection-time observation.
#[derive(Debug, Clone, Copy)]
pub struct AuthoredSchemaUpgradeRecoveryMember<'a> {
    pub key: &'a SourceDocumentKey,
    pub project_path: &'a str,
    pub encoding: SceneFileEncoding,
    pub original_disk: &'a [u8],
    pub reviewed_working: &'a [u8],
    pub successor: &'a [u8],
    pub observed: AuthoredSchemaUpgradeRecoveryMemberState,
}

/// Borrowed review before confirmation. Inspection never runs a project converter.
pub struct AuthoredSchemaUpgradeRecoveryReview<'a> {
    selected: &'a PreparedSourceUpgradeSchema,
    pending: &'a ObservedPendingRecoveryStorage<SourceUpgradeRecovery>,
    inventory: &'a RecoveredSourceUpgradeInventory,
}

impl AuthoredSchemaUpgradeRecoveryReview<'_> {
    pub fn schema_bundle(&self) -> &AuthoringSchemaBundle {
        self.selected.previous_bundle()
    }

    pub fn successor_schema_bundle(&self) -> &AuthoringSchemaBundle {
        self.selected.bundle()
    }

    pub fn component(&self) -> &ComponentTypeId {
        self.pending.recovery().field_upgrade().0
    }

    pub fn field(&self) -> &ComponentFieldId {
        self.pending.recovery().field_upgrade().1
    }

    pub fn successor_property(&self) -> &ComponentPropertyPath {
        self.pending.recovery().field_upgrade().2
    }

    pub const fn history_policy(&self) -> AuthoredSchemaUpgradeHistoryPolicy {
        AuthoredSchemaUpgradeHistoryPolicy::SaveReviewedWorkingCopiesAndStartNewHistory
    }

    pub fn bundle_state(&self) -> AuthoredSchemaUpgradeRecoveryMemberState {
        member_state(
            self.pending
                .recovery()
                .bundle()
                .classify(Some(self.inventory.bundle_digest())),
        )
    }

    pub fn sources(
        &self,
    ) -> impl ExactSizeIterator<Item = AuthoredSchemaUpgradeRecoveryMember<'_>> + '_ {
        // Both sets were joined by exact canonical path during complete inventory admission.
        self.pending
            .recovery()
            .members()
            .iter()
            .zip(self.inventory.inventory().participants())
            .map(|(member, actual)| AuthoredSchemaUpgradeRecoveryMember {
                key: &member.key,
                project_path: &member.path,
                encoding: match member.encoding {
                    RecoverySourceEncoding::Json => SceneFileEncoding::Json,
                    RecoverySourceEncoding::Ron => SceneFileEncoding::Ron,
                },
                original_disk: member.old_disk.bytes(),
                reviewed_working: member.reviewed_working.bytes(),
                successor: member.successor.bytes(),
                observed: member_state(member.classify(Some(actual.file().digest()))),
            })
    }
}

enum Stage {
    Review,
    Sources,
    PrepareReady,
    Verify(PreparedSourceUpgradeReady),
    Project(PreparedSourceUpgradeReady, VerifiedSourceUpgradeInventory),
    BindReady(PreparedSourceUpgradeReady, PreparedSourceUpgradeProject),
    BindProject(BoundSourceUpgradeReady, PreparedSourceUpgradeProject),
    Ready(BoundSourceUpgradeProject),
    Complete(PublishedSourceUpgradeProject),
}

/// Explicit headless continuation of one actual Pending project.
///
/// `inspect` is read-only. Borrow `review`, then `confirm` the exact captured set and call
/// `advance` until Complete. Each source step handles one member; preparation steps perform
/// finite complete-set checks under the same allowance. No task runtime or Editor is required.
/// Dropping this owner leaves disk Pending unless its own final Ready publication completed.
/// Ordinary Open/Check/Play continues to refuse Pending. An uncertain write cannot be retried
/// through this owner; retain its evidence and establish a fresh complete observation.
pub struct AuthoredSchemaUpgradeRecovery {
    pending: ObservedPendingRecoveryStorage<SourceUpgradeRecovery>,
    inventory: RecoveredSourceUpgradeInventory,
    selected: Option<PreparedSourceUpgradeSchema>,
    publication: SourceUpgradePublication,
    budget: AuthoringProjectAdmissionBudget,
    work: AuthoredSchemaUpgradeWork,
    limits: AuthoringSourceOperationLimits,
    stage: Option<Stage>,
    failure: Option<AuthoredSchemaUpgradeRecoveryError>,
}

impl std::fmt::Debug for AuthoredSchemaUpgradeRecovery {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AuthoredSchemaUpgradeRecovery")
            .field("view", &self.view())
            .field("failure", &self.failure)
            .finish_non_exhaustive()
    }
}

impl AuthoredSchemaUpgradeRecovery {
    /// Opens and verifies the referenced recovery package and every configured participant.
    /// Limits are selected once; retries cannot acquire another allowance.
    pub fn inspect(
        root: DirectoryCapability,
        profile: Option<&str>,
        limits: AuthoringSourceOperationLimits,
    ) -> Result<Self, AuthoredSchemaUpgradeRecoveryError> {
        let inspection = PendingAuthoringRecoveryInspection::inspect(root, limits)
            .map_err(RecoveryFailure::Inspection)?;
        Self::from_inspection(inspection, profile).map_err(|failure| failure.error)
    }

    /// Dispatches the original authenticated read without renewing its allowance. A wrong
    /// known kind returns the inspection for the matching operation, without payload decoding.
    pub fn from_inspection(
        inspection: PendingAuthoringRecoveryInspection,
        profile: Option<&str>,
    ) -> Result<Self, AuthoredSchemaUpgradeRecoveryInspectionFailure> {
        if inspection.kind() != PendingAuthoringRecoveryKind::AuthoredSchemaUpgrade {
            return Err(AuthoredSchemaUpgradeRecoveryInspectionFailure {
                inspection: Some(inspection),
                error: RecoveryFailure::OperationKind.into(),
            });
        }
        Self::admit_inspected(inspection, profile).map_err(|error| {
            AuthoredSchemaUpgradeRecoveryInspectionFailure {
                inspection: None,
                error,
            }
        })
    }

    fn admit_inspected(
        inspection: PendingAuthoringRecoveryInspection,
        profile: Option<&str>,
    ) -> Result<Self, AuthoredSchemaUpgradeRecoveryError> {
        let ordinary = AuthoringProjectAdmissionLimits::default();
        let attempt = AuthoringProjectAdmissionHost::new(ordinary)
            .allocate_attempt()
            .map_err(RecoveryFailure::Admission)?;
        let (loaded, mut budget, limits) = inspection.into_parts();
        budget
            .charge_source_bytes(0, 0, 0, size_of::<Self>())
            .map_err(RecoveryFailure::Admission)?;
        let recovery_limits = SourceUpgradeRecoveryLimits::default();
        let pending = source_upgrade_storage::admit_loaded(loaded, recovery_limits, &mut budget)
            .map_err(RecoveryFailure::Pending)?;
        let inventory =
            RecoveredSourceUpgradeInventory::capture(&pending, recovery_limits.scene, &mut budget)
                .map_err(RecoveryFailure::Inventory)?;
        let selected = PreparedSourceUpgradeSchema::prepare_recovered(
            &pending,
            &inventory,
            profile,
            attempt,
            ordinary,
            &mut budget,
        )
        .map_err(RecoveryFailure::Schema)?;
        let mut work = AuthoredSchemaUpgradeWork::default();
        for (index, member) in pending.recovery().members().iter().enumerate() {
            validate_pair(
                member,
                &selected,
                recovery_limits.scene,
                &mut budget,
                &mut work,
            )
            .map_err(|detail| AuthoredSchemaUpgradeRecoveryError {
                member: Some(index),
                detail,
            })?;
        }
        let publication =
            SourceUpgradePublication::prepare_recovered(&pending, &inventory, &mut budget)
                .map_err(RecoveryFailure::SourcePublication)?;
        Ok(Self {
            pending,
            inventory,
            selected: Some(selected),
            publication,
            budget,
            work,
            limits,
            stage: Some(Stage::Review),
            failure: None,
        })
    }

    pub fn review(&self) -> Option<AuthoredSchemaUpgradeRecoveryReview<'_>> {
        matches!(self.stage, Some(Stage::Review)).then(|| AuthoredSchemaUpgradeRecoveryReview {
            selected: self
                .selected
                .as_ref()
                .expect("review retains its one Schema admission"),
            pending: &self.pending,
            inventory: &self.inventory,
        })
    }

    pub fn view(&self) -> AuthoredSchemaUpgradeRecoveryView {
        use AuthoredSchemaUpgradeRecoveryPhase as P;
        let phase = match self
            .stage
            .as_ref()
            .expect("the owner retains one complete stage")
        {
            Stage::Review => P::Review,
            Stage::Sources => P::PublishingSources,
            Stage::Ready(_) => P::PublishingReady,
            Stage::Complete(_) => P::Complete,
            _ => P::PreparingReady,
        };
        let sources = self.pending.recovery().members().len();
        AuthoredSchemaUpgradeRecoveryView {
            operation: self.pending.recovery().digest(),
            phase,
            source_count: sources,
            completed_sources: self.publication.completed().min(sources),
            bundle_complete: self.publication.is_complete(),
            refusal: self
                .failure
                .as_ref()
                .map(AuthoredSchemaUpgradeRecoveryError::kind),
            cleanup_pending: self.cleanup_pending(),
            uncertain: self.uncertain(),
            limits: self.limits,
            usage: self.usage(),
        }
    }

    pub fn failure(&self) -> Option<&AuthoredSchemaUpgradeRecoveryError> {
        self.failure.as_ref()
    }

    /// Revalidates the same complete mixed set. A change since inspection is a refusal, even
    /// when another writer happened to publish the same successor bytes.
    pub fn confirm(
        &mut self,
    ) -> Result<AuthoredSchemaUpgradeRecoveryView, &AuthoredSchemaUpgradeRecoveryError> {
        if self.failure.is_some() {
            return self.result();
        }
        if !matches!(self.stage, Some(Stage::Review)) {
            return Err(&INVALID_PHASE);
        }
        match self.inventory.validate(&self.pending, &mut self.budget) {
            Ok(()) => self.stage = Some(Stage::Sources),
            Err(error) => self.failure = Some(RecoveryFailure::Inventory(error).into()),
        }
        self.result()
    }

    /// Performs one finite step and retains the exact move-only candidate on refusal.
    pub fn advance(
        &mut self,
    ) -> Result<AuthoredSchemaUpgradeRecoveryView, &AuthoredSchemaUpgradeRecoveryError> {
        if self.failure.is_some() {
            return self.result();
        }
        if matches!(self.stage, Some(Stage::Review)) {
            return Err(&INVALID_PHASE);
        }
        let stage = self.stage.take().expect("one exclusively owned stage");
        let (stage, failure) = self.advance_stage(stage);
        self.stage = Some(stage);
        self.failure = failure.map(Into::into);
        self.result()
    }

    /// Retries only after actual cleanup and without uncertain replacement evidence.
    /// Revalidation and IO use the original cumulative allowance. Conflicting files are never
    /// rebased implicitly; a fresh inspection is a separately reviewed recovery operation.
    pub fn retry(
        &mut self,
    ) -> Result<AuthoredSchemaUpgradeRecoveryView, &AuthoredSchemaUpgradeRecoveryError> {
        if self.failure.is_none() {
            return self.result();
        }
        if self.uncertain()
            || self.failure.as_ref().is_some_and(|failure| {
                matches!(
                    failure.detail,
                    RecoveryFailure::Rejected(FilePublicationRejection::RequiresReconcile)
                        | RecoveryFailure::Uncertain(_)
                )
            })
        {
            return self.result();
        }
        if self.cleanup_pending() {
            let cleaned = with_cleanup_admission(&mut self.budget, || match self.stage.as_mut() {
                Some(Stage::Ready(project)) => project.retry_cleanup(),
                _ => self.publication.retry_cleanup(),
            });
            match cleaned {
                Ok(true) => {}
                Ok(false) => return self.result(),
                Err(error) => {
                    self.failure = Some(RecoveryFailure::Admission(error).into());
                    return self.result();
                }
            }
        }
        self.failure = None;
        if matches!(self.stage, Some(Stage::Review)) {
            self.confirm()
        } else {
            self.advance()
        }
    }

    /// Returns only this process's actual final Ready evidence. The privately prepared saved
    /// project is dropped; ordinary fresh Open admits its own basis and histories afterwards.
    pub fn into_receipt(mut self) -> Result<AuthoredSchemaUpgradeRecoveryReceipt, Self> {
        if !matches!(self.stage, Some(Stage::Complete(_))) {
            return Err(self);
        }
        let view = self.view();
        let Some(Stage::Complete(published)) = self.stage.take() else {
            unreachable!()
        };
        let PublishedSourceUpgradeProject {
            session,
            publication,
        } = published;
        drop(session);
        Ok(AuthoredSchemaUpgradeRecoveryReceipt {
            operation: view.operation,
            source_count: view.source_count,
            usage: view.usage,
            publication,
        })
    }

    fn usage(&self) -> AuthoredSchemaUpgradeRecoveryUsage {
        let source = self.work.usage();
        AuthoredSchemaUpgradeRecoveryUsage {
            host_work: self.budget.decode_work,
            peak_open_handles: self.budget.peak_open_handles,
            source_work_bytes: source.work_bytes,
            source_peak_retained_bytes: source.peak_retained_bytes,
        }
    }

    fn cleanup_pending(&self) -> bool {
        match self.stage.as_ref() {
            Some(Stage::Ready(project)) => project.cleanup_pending(),
            _ => self.publication.cleanup_pending(),
        }
    }

    fn uncertain(&self) -> bool {
        match self.stage.as_ref() {
            Some(Stage::Ready(project)) => project.uncertain(),
            _ => self.publication.uncertain(),
        }
    }

    fn result(
        &self,
    ) -> Result<AuthoredSchemaUpgradeRecoveryView, &AuthoredSchemaUpgradeRecoveryError> {
        match &self.failure {
            Some(error) => Err(error),
            None => Ok(self.view()),
        }
    }

    fn advance_stage(&mut self, stage: Stage) -> (Stage, Option<RecoveryFailure>) {
        use RecoveryFailure as E;
        match stage {
            Stage::Review => (Stage::Review, Some(E::Phase)),
            Stage::Sources => {
                let (next, error) = match self
                    .publication
                    .publish_next(&self.pending, &mut self.budget)
                {
                    SourceUpgradePublicationStep::Complete => (Stage::PrepareReady, None),
                    SourceUpgradePublicationStep::Observed { .. }
                    | SourceUpgradePublicationStep::Published { .. } => (
                        if self.publication.is_complete() {
                            Stage::PrepareReady
                        } else {
                            Stage::Sources
                        },
                        None,
                    ),
                    SourceUpgradePublicationStep::Refused { error, .. } => {
                        (Stage::Sources, Some(E::SourcePublication(error)))
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
                };
                (next, error)
            }
            Stage::PrepareReady => {
                if let Err(error) = self
                    .publication
                    .validate_complete(&self.pending, &mut self.budget)
                {
                    return (Stage::PrepareReady, Some(E::SourcePublication(error)));
                }
                let selected = self
                    .selected
                    .take()
                    .expect("the admitted successor is moved exactly once");
                match PreparedSourceUpgradeReady::prepare(
                    selected,
                    self.pending.recovery(),
                    SourceUpgradePendingManifest {
                        capability: self.pending.capability(),
                        digest: self.pending.digest(),
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
            Stage::Verify(ready) => match VerifiedSourceUpgradeInventory::capture(
                ready.observed_pending_schema(),
                self.pending.recovery(),
                &[],
                self.inventory.inventory().scene_limits(),
                &mut self.budget,
            ) {
                Ok(verified) => (Stage::Project(ready, verified), None),
                Err(error) => (Stage::Verify(ready), Some(E::Inventory(error))),
            },
            Stage::Project(ready, verified) => match PreparedSourceUpgradeProject::prepare(
                &ready,
                verified,
                self.pending.shared_recovery(),
                self.inventory.inventory().scene_limits(),
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
                Ok(project) => (Stage::Ready(project), None),
                Err(failure) => (
                    Stage::BindProject(failure.ready, failure.project),
                    Some(E::Project(failure.error)),
                ),
            },
            Stage::Ready(project) => match project.publish(&mut self.budget) {
                Ok(SourceUpgradeProjectOutcome::Published(published)) => {
                    (Stage::Complete(published), None)
                }
                Ok(SourceUpgradeProjectOutcome::Rejected { bound, reason }) => {
                    (Stage::Ready(bound), Some(E::Rejected(reason)))
                }
                Ok(SourceUpgradeProjectOutcome::Failed { bound, stage }) => {
                    (Stage::Ready(bound), Some(E::Failed(stage)))
                }
                Ok(SourceUpgradeProjectOutcome::Uncertain { bound, publication }) => {
                    (Stage::Ready(bound), Some(E::Uncertain(publication)))
                }
                Err(failure) => (Stage::Ready(failure.bound), Some(E::Project(failure.error))),
            },
            Stage::Complete(published) => (Stage::Complete(published), None),
        }
    }
}

/// No publisher exists at inspection. A mismatched known kind retains the exact transport;
/// a terminal concrete data/semantic failure exposes its original typed cause.
#[derive(Debug, thiserror::Error)]
#[error("{error}")]
pub struct AuthoredSchemaUpgradeRecoveryInspectionFailure {
    inspection: Option<PendingAuthoringRecoveryInspection>,
    #[source]
    error: AuthoredSchemaUpgradeRecoveryError,
}

impl AuthoredSchemaUpgradeRecoveryInspectionFailure {
    pub fn error(&self) -> &AuthoredSchemaUpgradeRecoveryError {
        &self.error
    }
    pub fn inspection(&self) -> Option<&PendingAuthoringRecoveryInspection> {
        self.inspection.as_ref()
    }
    pub fn into_parts(
        self,
    ) -> (
        Option<PendingAuthoringRecoveryInspection>,
        AuthoredSchemaUpgradeRecoveryError,
    ) {
        (self.inspection, self.error)
    }
}

/// A confirmed Ready replacement performed by this recovery owner.
#[derive(Debug)]
pub struct AuthoredSchemaUpgradeRecoveryReceipt {
    operation: ProjectAuthoringRecoveryDigest,
    source_count: usize,
    usage: AuthoredSchemaUpgradeRecoveryUsage,
    publication: FilePublicationEvidence,
}

impl AuthoredSchemaUpgradeRecoveryReceipt {
    pub const fn operation(&self) -> ProjectAuthoringRecoveryDigest {
        self.operation
    }
    pub const fn source_count(&self) -> usize {
        self.source_count
    }
    pub const fn usage(&self) -> AuthoredSchemaUpgradeRecoveryUsage {
        self.usage
    }
    pub const fn ready_publication(&self) -> &ReplaceReceipt {
        self.publication.receipt()
    }
    pub const fn parent_directory_sync(&self) -> StageStatus {
        self.publication.parent_directory_sync()
    }
}

/// A concrete refusal retained with the operation that owns every prepared input and publisher.
#[derive(Debug, thiserror::Error)]
#[error("{detail}")]
pub struct AuthoredSchemaUpgradeRecoveryError {
    member: Option<usize>,
    #[source]
    detail: RecoveryFailure,
}

impl AuthoredSchemaUpgradeRecoveryError {
    pub fn kind(&self) -> AuthoredSchemaUpgradeRecoveryErrorKind {
        use AuthoredSchemaUpgradeRecoveryErrorKind as K;
        match self.detail {
            RecoveryFailure::Phase => K::Phase,
            RecoveryFailure::Admission(_) => K::Admission,
            RecoveryFailure::Pending(_) => K::Pending,
            RecoveryFailure::Inspection(ref error) => match error.kind() {
                PendingAuthoringRecoveryInspectionErrorKind::Admission => K::Admission,
                _ => K::Pending,
            },
            RecoveryFailure::OperationKind => K::Pending,
            RecoveryFailure::Inventory(_) => K::Inventory,
            RecoveryFailure::Schema(_) => K::Schema,
            RecoveryFailure::Decode(_) | RecoveryFailure::Source(_) | RecoveryFailure::Path(_) => {
                K::SourceDecoding
            }
            RecoveryFailure::Transition(_) => K::SourceTransition,
            RecoveryFailure::SourcePublication(_) => K::SourcePublication,
            RecoveryFailure::Ready(_) => K::Ready,
            RecoveryFailure::Project(_) => K::Project,
            RecoveryFailure::Rejected(_) => K::PublicationRejected,
            RecoveryFailure::Failed(_) => K::PublicationFailed,
            RecoveryFailure::Uncertain(_) => K::PublicationUncertain,
        }
    }

    /// Index in the complete reviewed source order, when stored-pair validation refused.
    pub const fn source_index(&self) -> Option<usize> {
        self.member
    }

    pub fn source_report(&self) -> Option<&ScenePatchReport> {
        match &self.detail {
            RecoveryFailure::Transition(report) => Some(report),
            _ => None,
        }
    }

    /// Actual uncertain evidence; matching disk bytes cannot turn it into a confirmed write.
    pub fn uncertain_publication(&self) -> Option<&ReplaceReceipt> {
        match &self.detail {
            RecoveryFailure::Uncertain(evidence) => Some(evidence.receipt()),
            _ => None,
        }
    }
}

#[derive(Debug, thiserror::Error)]
enum RecoveryFailure {
    #[error("the requested recovery action does not match its retained phase")]
    Phase,
    #[error("Pending operation inspection refused: {0}")]
    Inspection(#[source] PendingAuthoringRecoveryInspectionError),
    #[error("the authenticated Pending package is not a Schema upgrade")]
    OperationKind,
    #[error("recovery admission refused: {0}")]
    Admission(#[source] AuthoringProjectAdmissionError),
    #[error("Pending recovery storage refused: {0}")]
    Pending(#[source] SourceUpgradeStorageError),
    #[error("complete recovery inventory refused: {0}")]
    Inventory(#[source] SourceUpgradeInventoryError),
    #[error("recovered Schema admission refused: {0}")]
    Schema(#[source] SourceUpgradeReadyError),
    #[error("stored source decoding refused: {0}")]
    Decode(#[from] SourceDecodeError),
    #[error("stored source identity refused: {0:?}")]
    Source(document::AuthoringDocumentDataError),
    #[error("stored source path is invalid: {0}")]
    Path(#[source] ProjectPathError),
    #[error("stored authored source transition is invalid")]
    Transition(ScenePatchReport),
    #[error("recovered source publication refused: {0}")]
    SourcePublication(#[source] SourceUpgradePublicationError),
    #[error("final Ready preparation refused: {0}")]
    Ready(#[source] SourceUpgradeReadyError),
    #[error("saved successor project preparation refused: {0}")]
    Project(#[source] SourceUpgradeProjectError),
    #[error("the expected file publication was rejected: {0:?}")]
    Rejected(FilePublicationRejection),
    #[error("file publication failed at {0:?}")]
    Failed(FilePublicationFailureStage),
    #[error("file publication has uncertain replacement evidence")]
    Uncertain(FilePublicationEvidence),
}

impl From<RecoveryFailure> for AuthoredSchemaUpgradeRecoveryError {
    fn from(detail: RecoveryFailure) -> Self {
        Self {
            member: None,
            detail,
        }
    }
}

const INVALID_PHASE: AuthoredSchemaUpgradeRecoveryError = AuthoredSchemaUpgradeRecoveryError {
    member: None,
    detail: RecoveryFailure::Phase,
};

fn member_state(state: RecoveryMemberState) -> AuthoredSchemaUpgradeRecoveryMemberState {
    match state {
        RecoveryMemberState::Old => AuthoredSchemaUpgradeRecoveryMemberState::Original,
        RecoveryMemberState::New => AuthoredSchemaUpgradeRecoveryMemberState::Successor,
        RecoveryMemberState::Unchanged => AuthoredSchemaUpgradeRecoveryMemberState::Unchanged,
        RecoveryMemberState::Missing | RecoveryMemberState::Conflict => {
            unreachable!("complete inventory admission rejects missing/conflicting content")
        }
    }
}

fn validate_pair(
    member: &crate::project_host::authoring::source_upgrade_recovery::RecoveryMember,
    selected: &PreparedSourceUpgradeSchema,
    limits: SceneFileLimits,
    budget: &mut AuthoringProjectAdmissionBudget,
    work: &mut AuthoredSchemaUpgradeWork,
) -> Result<(), RecoveryFailure> {
    let before = budget.retained_candidate_bytes;
    let result = (|| {
        let original = decode_source(
            member.key.kind(),
            member.encoding.into(),
            member.reviewed_working.bytes(),
            limits,
            budget,
        )?;
        let successor = decode_source(
            member.key.kind(),
            member.encoding.into(),
            member.successor.bytes(),
            limits,
            budget,
        )?;
        let path_bytes = member
            .path
            .len()
            .checked_mul(2)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)
            .map_err(SourceDecodeError::from)?;
        budget
            .charge_source_bytes(0, 0, path_bytes, path_bytes)
            .map_err(RecoveryFailure::Admission)?;
        let path = ProjectPath::new(member.path.clone()).map_err(RecoveryFailure::Path)?;
        let source = document::document_source(&path).map_err(RecoveryFailure::Source)?;
        let result = match (&original, &successor) {
            (SourceTree::Scene(original), SourceTree::Scene(successor)) => {
                validate_authored_upgrade_scene_transition(
                    original,
                    successor,
                    source,
                    selected.upgrade(),
                    limits,
                    work,
                )
            }
            (SourceTree::Prefab(original), SourceTree::Prefab(successor)) => {
                validate_authored_upgrade_prefab_transition(
                    original,
                    successor,
                    source,
                    selected.upgrade(),
                    limits,
                    work,
                )
            }
            _ => unreachable!("the same authenticated member selects the same document kind"),
        };
        result.map_err(RecoveryFailure::Transition)
    })();
    // All decoded trees, paths and classification temporaries have ended. Keep performed work
    // and peaks, refund only this call's transient Host storage on either result.
    let retained = budget
        .retained_candidate_bytes
        .checked_sub(before)
        .expect("pair validation borrows all earlier retained inputs");
    budget
        .reconcile_retained_reservation(retained, 0)
        .expect("pair temporaries have dropped");
    result
}
