//! Explicit process recovery of one authenticated, fixed Prefab extraction generation.
//!
//! Inspection and confirmation are read-only. Publication consumes the actual recovered
//! generation, its mixed native inventory and its original allowances. No old Workspace,
//! checkpoint, extraction construction or historical replacement receipt is reconstructed.

use std::{fmt, mem::size_of};

use nara_fs::{ReplaceReceipt, StageStatus};
use nara_project::ProjectAuthoringRecoveryDigest;
use nara_reflect::AuthoringSchemaBundle;
use nara_scene::{AuthoringSchemaBasis, SceneCompilationLimits, SourceDocumentKey};

use super::{
    publication::{
        BoundPrefabExtractionProject, PrefabExtractionProjectOutcome,
        PreparedPrefabExtractionProject, ProjectPrefabExtractionFutureInventory,
        ProjectPrefabExtractionPublication, ProjectPrefabExtractionPublicationPhase,
        PublishedPrefabExtractionProject,
    },
    recovered_generation::{
        RecoveredPrefabExtractionGeneration, RecoveredPrefabExtractionGenerationFailure,
        RecoveredPrefabExtractionGenerationPhase,
    },
    recovered_inventory::RecoveredPrefabExtractionInventory,
    recovery_data::{
        PrefabExtractionRecovery as RecoveryData, PrefabExtractionRecoveryLimits,
        storage::admit_loaded,
    },
};
use crate::project_host::{
    authoring::{
        AuthoringProjectAdmissionBudget, AuthoringProjectAdmissionHost,
        AuthoringProjectAdmissionUsage, AuthoringSourceOperationLimits,
        pending_recovery::{PendingAuthoringRecoveryInspection, PendingAuthoringRecoveryKind},
        recovery_storage::ObservedPendingRecoveryStorage,
    },
    file_publication::FilePublicationEvidence,
};
use nara_scene::__private::{PrefabExtractionUsage, PrefabExtractionWork};

mod error;
mod review;
use error::{Failure, InitialFailure};
pub use error::{
    PrefabExtractionRecoveryErrorKind, PrefabExtractionRecoveryFailure,
    PrefabExtractionRecoveryInspectionFailure,
};
pub use review::{
    PrefabExtractionRecoveryDestination, PrefabExtractionRecoveryMember,
    PrefabExtractionRecoveryMemberDisposition, PrefabExtractionRecoveryMemberState,
    PrefabExtractionRecoveryReview,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabExtractionRecoveryPhase {
    Inspecting,
    Review,
    PublishingSources,
    PreparingReady,
    PublishingReady,
    Complete,
    Closing,
    Closed,
}

/// Cumulative operation work, including refused attempts. These are not ordinary Open limits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrefabExtractionRecoveryUsage {
    pub host_work: usize,
    pub peak_open_handles: usize,
    pub source_work_nodes: usize,
    pub source_work_bytes: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct PrefabExtractionRecoveryView {
    pub operation: ProjectAuthoringRecoveryDigest,
    pub phase: PrefabExtractionRecoveryPhase,
    pub source_count: usize,
    /// Includes Q, Q's sidecar and sparse original outputs. Observed outputs also advance it.
    pub file_count: usize,
    pub completed_files: usize,
    pub refusal: Option<PrefabExtractionRecoveryErrorKind>,
    pub cleanup_pending: bool,
    pub uncertain: bool,
    pub limits: AuthoringSourceOperationLimits,
    pub compilation_limits: SceneCompilationLimits,
    pub usage: PrefabExtractionRecoveryUsage,
}

struct Capture {
    pending: ObservedPendingRecoveryStorage<RecoveryData>,
    budget: AuthoringProjectAdmissionBudget,
    work: PrefabExtractionWork,
}

enum Stage {
    Capture(Capture),
    Generation(RecoveredPrefabExtractionGeneration),
    GenerationRefused(RecoveredPrefabExtractionGenerationFailure),
    Review(RecoveredPrefabExtractionGeneration),
    Confirmed(RecoveredPrefabExtractionGeneration),
    Files(ProjectPrefabExtractionPublication),
    Inventory(ProjectPrefabExtractionFutureInventory),
    Project(PreparedPrefabExtractionProject),
    Ready(BoundPrefabExtractionProject),
    Complete(PublishedPrefabExtractionProject),
}

/// A finite, headless owner of the actual Pending operation.
///
/// First advance read-only inspection to Review, borrow the fixed inputs, then explicitly
/// confirm. Each later advance performs one existing finite stage or file step. A refusal
/// requires explicit retry; neither retry nor close opens a fresh allowance. Close performs
/// only required native cleanup and retains unresolved custody. Dropping is not a close proof.
#[must_use]
pub struct PrefabExtractionRecovery {
    stage: Option<Stage>,
    failure: Option<Failure>,
    limits: AuthoringSourceOperationLimits,
    compilation_limits: SceneCompilationLimits,
    operation: ProjectAuthoringRecoveryDigest,
    source_count: usize,
    file_count: usize,
    admission_host: AuthoringProjectAdmissionHost,
    closing: bool,
    requires_reconcile: bool,
    ready_uncertainty: Option<FilePublicationEvidence>,
}

impl fmt::Debug for PrefabExtractionRecovery {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionRecovery")
            .field("view", &self.view())
            .field("failure", &self.failure())
            .finish_non_exhaustive()
    }
}

impl PrefabExtractionRecovery {
    /// Consumes an already authenticated transport without rereading it or renewing its budget.
    /// A different known kind returns the original inspection for dispatch to its actual owner.
    /// Payload decoding is read-only; a terminal decode refusal has no publisher to retire.
    pub fn inspect(
        inspection: PendingAuthoringRecoveryInspection,
        compilation_limits: SceneCompilationLimits,
    ) -> Result<Self, PrefabExtractionRecoveryInspectionFailure> {
        if inspection.kind() != PendingAuthoringRecoveryKind::PrefabExtraction {
            return Err(PrefabExtractionRecoveryInspectionFailure::new(
                Some(inspection),
                InitialFailure::WrongKind,
            ));
        }
        let work = match PrefabExtractionWork::new(compilation_limits) {
            Ok(work) => work,
            Err(report) => {
                return Err(PrefabExtractionRecoveryInspectionFailure::new(
                    Some(inspection),
                    InitialFailure::Source(report),
                ));
            }
        };
        let (loaded, mut budget, limits) = inspection.into_parts();
        let pending = admit_loaded(
            loaded,
            PrefabExtractionRecoveryLimits::default(),
            &mut budget,
        )
        .map_err(|error| {
            PrefabExtractionRecoveryInspectionFailure::new(None, InitialFailure::Storage(error))
        })?;
        let source_count = pending.recovery().complete_original_sources().len();
        let count_work = source_count
            .checked_mul(size_of::<usize>())
            .ok_or_else(|| {
                PrefabExtractionRecoveryInspectionFailure::new(None, InitialFailure::Accounting)
            })?;
        budget
            .charge_source_bytes(0, 0, count_work, size_of::<Self>())
            .map_err(|error| {
                PrefabExtractionRecoveryInspectionFailure::new(
                    None,
                    InitialFailure::Admission(error),
                )
            })?;
        let file_count = pending
            .recovery()
            .complete_original_sources()
            .iter()
            .filter(|source| source.output().is_some())
            .count()
            .checked_add(2)
            .ok_or_else(|| {
                PrefabExtractionRecoveryInspectionFailure::new(None, InitialFailure::Accounting)
            })?;
        let operation = pending.recovery().digest();
        Ok(Self {
            stage: Some(Stage::Capture(Capture {
                pending,
                budget,
                work,
            })),
            failure: None,
            limits,
            compilation_limits,
            operation,
            source_count,
            file_count,
            admission_host: AuthoringProjectAdmissionHost::default(),
            closing: false,
            requires_reconcile: false,
            ready_uncertainty: None,
        })
    }

    pub fn view(&self) -> PrefabExtractionRecoveryView {
        use PrefabExtractionRecoveryPhase as P;
        let cleanup_pending = self.cleanup_pending();
        let uncertain = self.uncertain();
        let phase = if self.closing && !matches!(self.stage(), Stage::Complete(_)) {
            if cleanup_pending || uncertain {
                P::Closing
            } else {
                P::Closed
            }
        } else {
            match self.stage() {
                Stage::Capture(_) | Stage::Generation(_) | Stage::GenerationRefused(_) => {
                    P::Inspecting
                }
                Stage::Review(_) => P::Review,
                Stage::Confirmed(_) | Stage::Files(_) => P::PublishingSources,
                Stage::Inventory(_) | Stage::Project(_) => P::PreparingReady,
                Stage::Ready(_) => P::PublishingReady,
                Stage::Complete(_) => P::Complete,
            }
        };
        let (host, source) = self.usage();
        PrefabExtractionRecoveryView {
            operation: self.operation,
            phase,
            source_count: self.source_count,
            file_count: self.file_count,
            completed_files: self.publication().map_or(0, |files| files.position()),
            refusal: self.failure().map(|failure| failure.kind()),
            cleanup_pending,
            uncertain,
            limits: self.limits,
            compilation_limits: self.compilation_limits,
            usage: PrefabExtractionRecoveryUsage {
                host_work: host.decode_work(),
                peak_open_handles: host.peak_open_handles(),
                source_work_nodes: source.work_nodes,
                source_work_bytes: source.work_bytes,
            },
        }
    }

    pub fn review(&self) -> Option<PrefabExtractionRecoveryReview<'_>> {
        if self.closing {
            return None;
        }
        match self.stage() {
            Stage::Review(generation) => Some(PrefabExtractionRecoveryReview::new(generation)),
            _ => None,
        }
    }

    /// Borrows the actual owning cause/report; it does not allocate a replacement diagnostic.
    pub fn failure(&self) -> Option<PrefabExtractionRecoveryFailure<'_>> {
        if let Some(failure) = &self.failure {
            return Some(PrefabExtractionRecoveryFailure::owned(failure));
        }
        match self.stage() {
            Stage::GenerationRefused(failure) => {
                Some(PrefabExtractionRecoveryFailure::generation(failure.error()))
            }
            _ => None,
        }
    }

    /// Rechecks this exact mixed inventory. This call performs no source or Ready publication.
    pub fn confirm(
        &mut self,
    ) -> Result<PrefabExtractionRecoveryView, PrefabExtractionRecoveryErrorKind> {
        if self.closing {
            return Err(PrefabExtractionRecoveryErrorKind::Closed);
        }
        if self.uncertain() {
            return Err(PrefabExtractionRecoveryErrorKind::PublicationUncertain);
        }
        if self.failure().is_some() {
            return Err(self.failure().expect("checked refusal").kind());
        }
        let stage = self
            .stage
            .take()
            .expect("recovery retains one concrete stage");
        match stage {
            Stage::Review(mut generation) => match generation.validate() {
                Ok(()) => {
                    self.stage = Some(Stage::Confirmed(generation));
                }
                Err(error) => {
                    self.stage = Some(Stage::Review(generation));
                    return Err(self.refuse(Failure::Generation(error)));
                }
            },
            other => {
                self.stage = Some(other);
                return Err(PrefabExtractionRecoveryErrorKind::Phase);
            }
        }
        Ok(self.view())
    }

    pub fn advance(
        &mut self,
    ) -> Result<PrefabExtractionRecoveryView, PrefabExtractionRecoveryErrorKind> {
        if self.closing {
            return self.close();
        }
        if self.uncertain() {
            return Err(PrefabExtractionRecoveryErrorKind::PublicationUncertain);
        }
        if let Some(failure) = self.failure() {
            return Err(failure.kind());
        }
        self.step()
    }

    /// Explicit retry preserves every current native owner and both original allowances.
    /// Capacity refusals and physical uncertainty cannot be cleared by this method.
    pub fn retry(
        &mut self,
    ) -> Result<PrefabExtractionRecoveryView, PrefabExtractionRecoveryErrorKind> {
        if self.closing {
            return self.close();
        }
        if self.uncertain() {
            return Err(PrefabExtractionRecoveryErrorKind::PublicationUncertain);
        }
        if self.cleanup_pending() {
            self.clean_up()?;
            if self.cleanup_pending() {
                return Err(PrefabExtractionRecoveryErrorKind::CleanupPending);
            }
        }
        if let Some(failure) = self.failure() {
            if failure.kind().is_capacity() {
                return Err(failure.kind());
            }
        }
        self.failure = None;
        self.step()
    }

    /// Stops future publication and explicitly observes only necessary physical cleanup.
    /// Even Closed retains its original candidate; an unresolved close keeps all custody.
    pub fn close(
        &mut self,
    ) -> Result<PrefabExtractionRecoveryView, PrefabExtractionRecoveryErrorKind> {
        self.closing = true;
        if self.cleanup_pending() {
            self.clean_up()?;
        }
        if self.uncertain() {
            return Err(PrefabExtractionRecoveryErrorKind::PublicationUncertain);
        }
        if self.cleanup_pending() {
            return Err(PrefabExtractionRecoveryErrorKind::CleanupPending);
        }
        Ok(self.view())
    }

    /// Only a real accepted Ready and installed private Project can become an outcome.
    /// Every incomplete owner is returned intact, including its failure and native custody.
    pub fn into_outcome(mut self) -> Result<PrefabExtractionRecoveryOutcome, Self> {
        if !matches!(self.stage(), Stage::Complete(_)) {
            return Err(self);
        }
        let Some(Stage::Complete(published)) = self.stage.take() else {
            unreachable!()
        };
        Ok(PrefabExtractionRecoveryOutcome {
            published,
            operation: self.operation,
        })
    }

    fn stage(&self) -> &Stage {
        self.stage
            .as_ref()
            .expect("a recovery owner retains one concrete stage")
    }

    fn publication(&self) -> Option<&ProjectPrefabExtractionPublication> {
        match self.stage() {
            Stage::Files(files) => Some(files),
            Stage::Inventory(future) => Some(future.publication()),
            Stage::Project(project) => Some(project.publication()),
            Stage::Ready(bound) => Some(bound.publication()),
            Stage::Complete(published) => Some(&published.files),
            _ => None,
        }
    }

    fn generation(&self) -> Option<&RecoveredPrefabExtractionGeneration> {
        match self.stage() {
            Stage::Capture(_) => None,
            Stage::Generation(generation)
            | Stage::Review(generation)
            | Stage::Confirmed(generation) => Some(generation),
            Stage::GenerationRefused(failure) => Some(failure.owner()),
            _ => self
                .publication()
                .and_then(|files| files.recovered_generation()),
        }
    }

    fn usage(&self) -> (AuthoringProjectAdmissionUsage, PrefabExtractionUsage) {
        if let Stage::Capture(capture) = self.stage() {
            return (capture.budget.usage(), capture.work.usage());
        }
        let generation = self
            .generation()
            .expect("recovery publication retains its original generation");
        (generation.host_usage(), generation.source_usage())
    }

    fn cleanup_pending(&self) -> bool {
        match self.stage() {
            Stage::Ready(bound) => bound.cleanup_pending() || bound.publication().cleanup_pending(),
            _ => self
                .publication()
                .is_some_and(|files| files.cleanup_pending()),
        }
    }

    fn uncertain(&self) -> bool {
        self.requires_reconcile
            || self.ready_uncertainty.is_some()
            || match self.stage() {
                Stage::Ready(bound) => bound.uncertain() || bound.publication().uncertain(),
                _ => self.publication().is_some_and(|files| files.uncertain()),
            }
    }

    fn refuse(&mut self, failure: Failure) -> PrefabExtractionRecoveryErrorKind {
        let kind = failure.kind();
        if kind == PrefabExtractionRecoveryErrorKind::PublicationUncertain {
            self.requires_reconcile = true;
        }
        self.failure = Some(failure);
        kind
    }

    fn clean_up(&mut self) -> Result<(), PrefabExtractionRecoveryErrorKind> {
        let result = match self.stage.as_mut().expect("retained recovery stage") {
            Stage::Files(files) => files.retry_cleanup().map(|_| ()).map_err(Failure::Files),
            Stage::Inventory(future) => future
                .publication_mut()
                .retry_cleanup()
                .map(|_| ())
                .map_err(Failure::Files),
            Stage::Project(project) => project
                .publication_mut()
                .retry_cleanup()
                .map(|_| ())
                .map_err(Failure::Files),
            Stage::Ready(bound) => {
                if bound.cleanup_pending() {
                    bound.retry_cleanup().map(|_| ()).map_err(Failure::Project)
                } else {
                    bound
                        .publication_mut()
                        .retry_cleanup()
                        .map(|_| ())
                        .map_err(Failure::Files)
                }
            }
            Stage::Complete(published) => published
                .files
                .retry_cleanup()
                .map(|_| ())
                .map_err(Failure::Files),
            _ => Ok(()),
        };
        result.map_err(|failure| self.refuse(failure))
    }

    fn step(&mut self) -> Result<PrefabExtractionRecoveryView, PrefabExtractionRecoveryErrorKind> {
        let stage = self
            .stage
            .take()
            .expect("recovery retains one concrete stage");
        let (next, failure) = match stage {
            Stage::Capture(mut capture) => match RecoveredPrefabExtractionInventory::capture(
                &capture.pending,
                PrefabExtractionRecoveryLimits::default().scene,
                &mut capture.budget,
            ) {
                Ok(inventory) => (
                    Stage::Generation(RecoveredPrefabExtractionGeneration::new(
                        capture.pending,
                        inventory,
                        capture.budget,
                        capture.work,
                    )),
                    None,
                ),
                Err(error) => (Stage::Capture(capture), Some(Failure::Inventory(error))),
            },
            Stage::Generation(generation) => self.advance_generation(generation.advance()),
            Stage::GenerationRefused(failure) => self.advance_generation(failure.retry()),
            Stage::Review(generation) => (Stage::Review(generation), None),
            Stage::Confirmed(generation) => match generation.into_publication() {
                Ok(files) => (Stage::Files(files), None),
                Err(failure) => {
                    let (files, error) = failure.into_parts();
                    (Stage::Files(files), Some(Failure::Files(error)))
                }
            },
            Stage::Files(files)
                if files.phase()
                    == ProjectPrefabExtractionPublicationPhase::CheckpointsComplete =>
            {
                match files.into_future_inventory() {
                    Ok(future) => (Stage::Inventory(future), None),
                    Err(failure) => {
                        let (files, error) = failure.into_parts();
                        (Stage::Files(files), Some(Failure::FutureInventory(error)))
                    }
                }
            }
            Stage::Files(files) => match files.advance() {
                Ok(files) => (Stage::Files(files), None),
                Err(failure) => {
                    let (files, error) = failure.into_parts();
                    (Stage::Files(files), Some(Failure::Files(error)))
                }
            },
            Stage::Inventory(future) => match future.prepare_project(&mut self.admission_host) {
                Ok(project) => (Stage::Project(project), None),
                Err(failure) => {
                    let (future, error) = failure.into_parts();
                    (Stage::Inventory(future), Some(Failure::Project(error)))
                }
            },
            Stage::Project(project) => match project.bind() {
                Ok(bound) => (Stage::Ready(bound), None),
                Err(failure) => {
                    let (project, error) = failure.into_parts();
                    (Stage::Project(project), Some(Failure::Project(error)))
                }
            },
            Stage::Ready(bound) => match bound.publish() {
                Ok(PrefabExtractionProjectOutcome::Published(published)) => {
                    (Stage::Complete(published), None)
                }
                Ok(PrefabExtractionProjectOutcome::Rejected { bound, reason }) => {
                    (Stage::Ready(bound), Some(Failure::Rejected(reason)))
                }
                Ok(PrefabExtractionProjectOutcome::Failed { bound, stage }) => {
                    (Stage::Ready(bound), Some(Failure::Failed(stage)))
                }
                Ok(PrefabExtractionProjectOutcome::Uncertain { bound, evidence }) => {
                    self.ready_uncertainty = Some(evidence);
                    (Stage::Ready(bound), Some(Failure::Uncertain))
                }
                Err(refusal) => {
                    let (bound, error) = refusal.into_parts();
                    (Stage::Ready(bound), Some(Failure::Project(error)))
                }
            },
            complete @ Stage::Complete(_) => (complete, None),
        };
        self.stage = Some(next);
        if let Some(failure) = failure {
            return Err(self.refuse(failure));
        }
        if let Some(failure) = self.failure() {
            return Err(failure.kind());
        }
        Ok(self.view())
    }

    fn advance_generation(
        &self,
        result: Result<
            RecoveredPrefabExtractionGeneration,
            RecoveredPrefabExtractionGenerationFailure,
        >,
    ) -> (Stage, Option<Failure>) {
        match result {
            Ok(generation)
                if generation.phase() == RecoveredPrefabExtractionGenerationPhase::Complete =>
            {
                (Stage::Review(generation), None)
            }
            Ok(generation) => (Stage::Generation(generation), None),
            Err(failure) => (Stage::GenerationRefused(failure), None),
        }
    }

    #[cfg(test)]
    pub(super) fn test_prepared_project(
        &self,
    ) -> Option<&crate::project_host::authoring::project::AuthoringProjectSessionSnapshot> {
        match self.stage() {
            Stage::Project(project) => Some(project.pending_project()),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(super) fn test_fail_ready_write(
        &mut self,
    ) -> Option<crate::project_host::file_publication::CleanupTestControl> {
        match self.stage.as_mut()? {
            Stage::Ready(bound) => Some(bound.test_fail_next_write()),
            _ => None,
        }
    }
}

/// The exact accepted Project, native inventory, Schema basis and remaining publication
/// custody. The continuing owner is private and no resumable publication cursor escapes.
#[must_use]
pub struct PrefabExtractionRecoveryOutcome {
    published: PublishedPrefabExtractionProject,
    operation: ProjectAuthoringRecoveryDigest,
}

impl PrefabExtractionRecoveryOutcome {
    pub fn operation(&self) -> ProjectAuthoringRecoveryDigest {
        self.operation
    }
    pub fn ready_receipt(&self) -> &ReplaceReceipt {
        self.published.evidence.receipt()
    }
    pub fn ready_parent_directory_sync(&self) -> StageStatus {
        self.published.evidence.parent_directory_sync()
    }
    pub fn schema_basis(&self) -> &AuthoringSchemaBasis {
        self.published
            .files
            .recovered_generation()
            .expect("recovery retains its original generation")
            .observation()
            .basis()
    }
    pub fn schema_bundle(&self) -> &AuthoringSchemaBundle {
        self.published.session.authoring_bundle()
    }
    pub fn source_keys(&self) -> impl ExactSizeIterator<Item = &SourceDocumentKey> + '_ {
        self.published
            .inventory
            .participants()
            .map(|source| source.source_key())
    }

    #[cfg(test)]
    pub(super) fn test_project(
        &self,
    ) -> &crate::project_host::authoring::project::AuthoringProjectSessionSnapshot {
        &self.published.session
    }
}

impl fmt::Debug for PrefabExtractionRecoveryOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionRecoveryOutcome")
            .field("operation", &self.operation)
            .field("ready", &self.ready_receipt())
            .field("basis", self.schema_basis())
            .finish_non_exhaustive()
    }
}
