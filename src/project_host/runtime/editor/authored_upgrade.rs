//! Session-owned capture, review, physical retirement and coordinated authored Schema publication.

use std::{
    mem::size_of,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_diagnostic::{Diagnostic, DiagnosticField, DiagnosticReport};
use nara_scene::__private::AuthoredSchemaUpgradeWork;
use nara_tasks::{TaskDomainKey, TaskSpawnRequest};

use crate::project_host::runtime_diagnostics::{
    diagnostic_code, field_key, safe_summary, single_diagnostic,
};

use super::{
    authoring_sources::AuthoringSourceSelection,
    authoring_work::{Pending, WorkError},
    *,
};
pub use crate::project_host::authoring::{
    AuthoredSchemaUpgradeProposal, AuthoredSchemaUpgradeReview,
    AuthoredSchemaUpgradeReviewedSource, AuthoredSchemaUpgradeSourceProposal,
};
use crate::project_host::authoring::{
    AuthoredSourceUpgradePublication, AuthoredSourceUpgradePublicationPhase,
    AuthoringProjectSourceAdmissionError, AuthoringSourceOperationLimits,
    PreparedAuthoredSchemaUpgradeReview, SourceUpgradeBaseline, SourceUpgradeOpenInput,
    SourceUpgradePreparationError, SourceUpgradeProposalError, SourceUpgradeProposalFailure,
};

mod publication;
mod publication_work;
use publication_work::PublicationWork;
mod preparation_work;
use preparation_work::{PreparationReturn, PreparationStartFailure, PreparationWork};

const DOMAIN: TaskDomainKey = TaskDomainKey::new(0x6175_7468_7570_6772);
static NEXT_OPERATION: AtomicU64 = AtomicU64::new(1);

/// Identifies one Session's finite reviewed operation; it grants no file publication authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorAuthoredSchemaUpgradeId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAuthoredSchemaUpgradePhase {
    Idle,
    Draining,
    Capturing,
    AwaitingProposal,
    Preparing,
    Review,
    Retiring,
    Confirming,
    PublishingRecovery,
    PublishingPending,
    PublishingSources,
    PreparingReady,
    Adopting,
    RequiresReconcile,
    Terminal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EditorAuthoredSchemaUpgradeError {
    #[error("the Session is closing")]
    Closing,
    #[error("this operation requires a source Authoring project")]
    NotAuthoring,
    #[error("conflicting source or publication work is pending")]
    Busy,
    #[error("the operation or phase does not match")]
    Operation,
    #[error("an accepted Save could not establish a confirmed baseline")]
    Persistence,
    #[error("the complete source set or its nominal authority changed")]
    Stale,
    #[error("complete source capture was refused")]
    Capture,
    #[error("the proposed complete authored transition was refused")]
    Preparation,
    #[error("the complete operation exhausted its selected cumulative Host work allowance")]
    WorkLimit,
    #[error("the complete operation exceeds its selected native-handle ceiling")]
    OpenHandleLimit,
    #[error("an execution owner did not complete physical cleanup")]
    ExecutionCleanup,
    #[error("publication requires reconciliation and keeps the incumbent writes fenced")]
    Publication,
    #[error("finite work was not admitted or did not return its result")]
    Task,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorAuthoredSchemaUpgradeOutcome {
    Cancelled,
    Adopted,
    Failed(EditorAuthoredSchemaUpgradeError),
}

/// Observes actual finite work separately from the Session's write admission fence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorAuthoredSchemaUpgradeView {
    pub operation: Option<EditorAuthoredSchemaUpgradeId>,
    pub phase: EditorAuthoredSchemaUpgradePhase,
    pub outcome: Option<EditorAuthoredSchemaUpgradeOutcome>,
    /// A refused proposal keeps the captured baseline available for a corrected proposal.
    pub refusal: Option<EditorAuthoredSchemaUpgradeError>,
    pub documents: usize,
    pub sources: usize,
    pub work_pending: bool,
    pub writes_fenced: bool,
    /// Selected at acceptance and shared by capture, every proposal attempt and publication.
    pub limits: AuthoringSourceOperationLimits,
    cancellation_allowed: bool,
    retry_allowed: bool,
}

impl EditorAuthoredSchemaUpgradeView {
    /// The operation owner decides whether cancellation can still preserve all original sources.
    /// Frontends must not infer this authority from idle workers or duplicate phase matching.
    pub const fn can_cancel(&self) -> bool {
        self.writes_fenced && self.cancellation_allowed
    }

    /// Only the retained publication owner can authorize retrying its exact returned objects.
    pub const fn can_retry(&self) -> bool {
        self.writes_fenced && !self.work_pending && self.retry_allowed
    }
}

/// Borrowed command inputs from one retained operation, never a fresh filesystem query.
pub struct EditorAuthoredSchemaUpgradeInputs<'a> {
    baseline: &'a SourceUpgradeBaseline,
}

impl EditorAuthoredSchemaUpgradeInputs<'_> {
    pub fn schema_bundle(&self) -> &nara_reflect::AuthoringSchemaBundle {
        self.baseline.bundle()
    }

    pub fn source_count(&self) -> usize {
        self.baseline.source_count()
    }

    pub fn sources(
        &self,
    ) -> impl ExactSizeIterator<Item = EditorAuthoredSchemaUpgradeSource<'_>> + '_ {
        self.baseline.sources().map(|source| {
            let working = match (source.scene, source.prefab) {
                (Some(scene), None) => EditorAuthoredUpgradeWorkingSource::Scene(scene),
                (None, Some(prefab)) => EditorAuthoredUpgradeWorkingSource::Prefab(prefab),
                (None, None) => EditorAuthoredUpgradeWorkingSource::Saved(source.disk),
                (Some(_), Some(_)) => {
                    unreachable!("one nominal source has exactly one document kind")
                }
            };
            EditorAuthoredSchemaUpgradeSource {
                key: source.key,
                project_path: source.path,
                encoding: source.encoding,
                original_disk: source.disk,
                document: source.document,
                working,
            }
        })
    }
}

/// An open document supplies its exact reviewed working copy. A closed source supplies its
/// captured disk bytes in the enclosing source's encoding; the project command owns decoding.
pub enum EditorAuthoredUpgradeWorkingSource<'a> {
    Scene(&'a nara_scene::SceneDocument),
    Prefab(&'a nara_scene::PrefabDocument),
    Saved(&'a [u8]),
}

pub struct EditorAuthoredSchemaUpgradeSource<'a> {
    pub key: &'a nara_scene::SourceDocumentKey,
    pub project_path: &'a str,
    pub encoding: nara_scene::SceneFileEncoding,
    pub original_disk: &'a [u8],
    pub document: Option<nara_tooling::EditorDocumentId>,
    pub working: EditorAuthoredUpgradeWorkingSource<'a>,
}

impl Default for EditorAuthoredSchemaUpgradeView {
    fn default() -> Self {
        Self {
            operation: None,
            phase: EditorAuthoredSchemaUpgradePhase::Idle,
            outcome: None,
            refusal: None,
            documents: 0,
            sources: 0,
            work_pending: false,
            writes_fenced: false,
            limits: AuthoringSourceOperationLimits::default(),
            cancellation_allowed: true,
            retry_allowed: false,
        }
    }
}

#[derive(Default)]
pub(super) struct EditorAuthoredSchemaUpgradeHost {
    view: EditorAuthoredSchemaUpgradeView,
    pending: Option<Work>,
    baseline: Option<SourceUpgradeBaseline>,
    review: Option<PreparedAuthoredSchemaUpgradeReview>,
    publication: Option<AuthoredSourceUpgradePublication>,
    proposal_failure: Option<SourceUpgradeProposalError>,
    sources: Vec<AuthoringSourceSelection>,
    diagnostics: Option<DiagnosticReport>,
    cancelled: bool,
    draining_save: Option<EditorPersistenceAttemptId>,
    retirement_started: bool,
    #[cfg(test)]
    next_capture_gate: Option<Box<dyn FnOnce() + Send>>,
    #[cfg(test)]
    next_preparation_gate: Option<Box<dyn FnOnce() + Send>>,
}

enum Work {
    Capture(Pending<Result<SourceUpgradeBaseline, SourceUpgradePreparationError>>),
    Prepare(PreparationWork),
    Publish(PublicationWork),
}

enum Returned {
    Captured(Result<SourceUpgradeBaseline, SourceUpgradePreparationError>),
    Prepared(Result<PreparedAuthoredSchemaUpgradeReview, SourceUpgradeProposalFailure>),
    Unprepared(SourceUpgradeBaseline),
    Published(AuthoredSourceUpgradePublication),
}

impl Work {
    fn poll(&mut self) -> Option<Result<Returned, WorkError>> {
        match self {
            Self::Capture(pending) => pending.poll().map(|result| result.map(Returned::Captured)),
            Self::Prepare(pending) => pending.poll().map(|result| {
                result.map(|returned| match returned {
                    PreparationReturn::Unstarted(baseline) => Returned::Unprepared(baseline),
                    PreparationReturn::Prepared(result) => Returned::Prepared(result),
                })
            }),
            Self::Publish(pending) => pending.poll().map(|result| result.map(Returned::Published)),
        }
    }

    fn cancel(&self) {
        match self {
            Self::Capture(pending) => pending.cancel(),
            Self::Prepare(pending) => pending.cancel(),
            // Publication owns durable effects and must always return its actual result.
            Self::Publish(_) => {}
        }
    }

    #[cfg(test)]
    fn return_observer(&self) -> nara_tasks::TaskReturnObserver {
        match self {
            Self::Capture(pending) => pending.task.as_ref().unwrap().return_observer(),
            Self::Prepare(pending) => pending.return_observer(),
            Self::Publish(pending) => pending.return_observer(),
        }
    }
}

impl EditorAuthoredSchemaUpgradeHost {
    pub(super) fn writes_fenced(&self) -> bool {
        self.view.writes_fenced
    }
    pub(super) fn work_pending(&self) -> bool {
        self.view.work_pending
    }
    pub(super) fn waits_for_save(&self, attempt: EditorPersistenceAttemptId) -> bool {
        self.draining_save == Some(attempt)
    }

    pub(super) fn cancel(&mut self) {
        if !self.view.can_cancel() {
            return;
        }
        self.cancelled = true;
        self.view.retry_allowed = false;
        if let Some(publication) = &mut self.publication {
            publication.request_cancellation();
        }
        if let Some(pending) = &self.pending {
            pending.cancel();
        } else if !self.retirement_started {
            self.finish(EditorAuthoredSchemaUpgradeOutcome::Cancelled);
        }
    }

    fn finish(&mut self, outcome: EditorAuthoredSchemaUpgradeOutcome) {
        debug_assert!(
            self.pending.is_none(),
            "physical work must return before releasing its inputs"
        );
        self.baseline = None;
        self.review = None;
        self.publication = None;
        self.sources = Vec::new();
        self.draining_save = None;
        self.view.phase = EditorAuthoredSchemaUpgradePhase::Terminal;
        self.view.outcome = Some(outcome);
        self.view.work_pending = false;
        self.view.writes_fenced = false;
        self.view.retry_allowed = false;
        self.retirement_started = false;
    }
}

impl EditorProjectSession {
    pub fn authored_schema_upgrade_view(&self) -> &EditorAuthoredSchemaUpgradeView {
        &self.authored_schema_upgrade.view
    }

    pub fn authored_schema_upgrade_diagnostics(&self) -> Option<&DiagnosticReport> {
        self.authored_schema_upgrade
            .proposal_failure
            .as_ref()
            .and_then(SourceUpgradeProposalError::diagnostics)
            .or(self.authored_schema_upgrade.diagnostics.as_ref())
    }

    pub fn authored_schema_upgrade_inputs(
        &self,
        operation: EditorAuthoredSchemaUpgradeId,
    ) -> Result<EditorAuthoredSchemaUpgradeInputs<'_>, EditorAuthoredSchemaUpgradeError> {
        if self.authored_schema_upgrade.view.operation != Some(operation)
            || self.authored_schema_upgrade.view.phase
                != EditorAuthoredSchemaUpgradePhase::AwaitingProposal
        {
            return Err(EditorAuthoredSchemaUpgradeError::Operation);
        }
        Ok(EditorAuthoredSchemaUpgradeInputs {
            baseline: self
                .authored_schema_upgrade
                .baseline
                .as_ref()
                .ok_or(EditorAuthoredSchemaUpgradeError::Operation)?,
        })
    }

    /// Fences new source writes immediately. An already accepted Save returns and installs its
    /// actual outcome before the original capture begins. This does not execute project code.
    pub fn begin_authored_schema_upgrade(
        &mut self,
    ) -> Result<EditorAuthoredSchemaUpgradeId, EditorAuthoredSchemaUpgradeError> {
        self.begin_authored_schema_upgrade_with_limits(AuthoringSourceOperationLimits::default())
    }

    /// Chooses finite Host work and native-handle ceilings before complete capture starts.
    /// Refusal never renews them. Retained memory and source preparation keep their own limits.
    pub fn begin_authored_schema_upgrade_with_limits(
        &mut self,
        limits: AuthoringSourceOperationLimits,
    ) -> Result<EditorAuthoredSchemaUpgradeId, EditorAuthoredSchemaUpgradeError> {
        use EditorAuthoredSchemaUpgradeError as E;
        if !self.accepts_new_work() {
            return Err(E::Closing);
        }
        if self.authoring_project().is_none() {
            return Err(E::NotAuthoring);
        }
        if self.authoring_operation_fenced()
            || self.schema_owner_adoption.reclamation_blocked()
            || self.pending_document_open.is_some()
            || self.pending_workspace_intent.is_some()
            || self.authoring_retirements.is_pending()
            || self.image_import.schema_adoption_import_pending()
            || self.source_asset_import.schema_adoption_pending()
            || self.prefab_conversion.schema_adoption_pending()
            || !self.source_tools.reclamation_settled()
            || self.source_tools.schema_adoption_pending()
            || matches!(
                self.persistence,
                persistence::PersistenceState::Pending {
                    work: PendingPersistence::Reopen(_) | PendingPersistence::Reopening(_),
                    ..
                }
            )
        {
            return Err(E::Busy);
        }
        let operation = EditorAuthoredSchemaUpgradeId(
            NEXT_OPERATION
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                    next.checked_add(1)
                })
                .map_err(|_| E::Operation)?,
        );
        *self.authored_schema_upgrade = EditorAuthoredSchemaUpgradeHost {
            draining_save: match &self.persistence {
                persistence::PersistenceState::Pending { attempt, .. } => Some(*attempt),
                _ => None,
            },
            view: EditorAuthoredSchemaUpgradeView {
                operation: Some(operation),
                phase: EditorAuthoredSchemaUpgradePhase::Draining,
                work_pending: true,
                writes_fenced: true,
                limits,
                ..Default::default()
            },
            ..Default::default()
        };
        Ok(operation)
    }

    pub fn cancel_authored_schema_upgrade(
        &mut self,
        operation: EditorAuthoredSchemaUpgradeId,
    ) -> Result<(), EditorAuthoredSchemaUpgradeError> {
        if self.authored_schema_upgrade.view.operation != Some(operation)
            || !self.authored_schema_upgrade.view.can_cancel()
        {
            return Err(EditorAuthoredSchemaUpgradeError::Operation);
        }
        self.authored_schema_upgrade.cancel();
        Ok(())
    }

    /// Retries only the retained live publication. It neither recaptures sources nor renews
    /// limits, and uncertain or missing publisher returns require explicit reconciliation.
    /// An already requested Close can wait for this operation while its Tasks remain admitted.
    pub fn retry_authored_schema_upgrade(
        &mut self,
        operation: EditorAuthoredSchemaUpgradeId,
    ) -> Result<(), EditorAuthoredSchemaUpgradeError> {
        use EditorAuthoredSchemaUpgradeError as E;
        if !self.authoring_runtime.host_custody().is_accepting() {
            return Err(E::Closing);
        }
        let host = &mut self.authored_schema_upgrade;
        if host.view.operation != Some(operation)
            || host.view.phase != EditorAuthoredSchemaUpgradePhase::RequiresReconcile
            || !host.view.writes_fenced
        {
            return Err(E::Operation);
        }
        if host.pending.is_some() || host.view.work_pending {
            return Err(E::Busy);
        }
        let publication = host.publication.as_mut().ok_or(E::Task)?;
        if !publication.request_retry() {
            return Err(if publication.host_work_exhausted() {
                E::WorkLimit
            } else {
                E::Publication
            });
        }
        host.view.retry_allowed = false;
        host.view.work_pending = true;
        host.view.refusal = None;
        host.diagnostics = None;
        Ok(())
    }

    /// Admits pure data against the original complete capture, then prepares it on existing Tasks.
    /// Refusal preserves that same capture for correction. No source file is published here.
    pub fn prepare_authored_schema_upgrade(
        &mut self,
        operation: EditorAuthoredSchemaUpgradeId,
        proposal: AuthoredSchemaUpgradeProposal,
    ) -> Result<(), EditorAuthoredSchemaUpgradeError> {
        use EditorAuthoredSchemaUpgradeError as E;
        if !self.accepts_new_work() {
            return Err(E::Closing);
        }
        if self.authored_schema_upgrade.view.operation != Some(operation)
            || self.authored_schema_upgrade.view.phase
                != EditorAuthoredSchemaUpgradePhase::AwaitingProposal
        {
            return Err(E::Operation);
        }
        let baseline = self
            .authored_schema_upgrade
            .baseline
            .take()
            .expect("AwaitingProposal owns its original captured inputs");
        #[cfg(test)]
        let gate = self.authored_schema_upgrade.next_preparation_gate.take();
        let pending = match PreparationWork::start(
            self.authoring_runtime.persistence_tasks(),
            TaskSpawnRequest::new(operation.0, DOMAIN),
            baseline,
            proposal,
            #[cfg(test)]
            gate,
        ) {
            Ok(pending) => pending,
            Err(PreparationStartFailure::Proposal(failure)) => {
                return Err(self.refuse_authored_schema_upgrade_proposal(failure));
            }
            Err(PreparationStartFailure::Task(baseline)) => {
                self.restore_unstarted_authored_schema_upgrade_proposal(baseline);
                return Err(E::Task);
            }
        };
        self.authored_schema_upgrade.pending = Some(Work::Prepare(pending));
        self.authored_schema_upgrade.proposal_failure = None;
        self.authored_schema_upgrade.diagnostics = None;
        self.authored_schema_upgrade.view.refusal = None;
        self.authored_schema_upgrade.view.phase = EditorAuthoredSchemaUpgradePhase::Preparing;
        self.authored_schema_upgrade.view.work_pending = true;
        Ok(())
    }

    pub fn authored_schema_upgrade_review(
        &self,
        operation: EditorAuthoredSchemaUpgradeId,
    ) -> Result<AuthoredSchemaUpgradeReview<'_>, EditorAuthoredSchemaUpgradeError> {
        if self.authored_schema_upgrade.view.operation != Some(operation)
            || self.authored_schema_upgrade.view.phase != EditorAuthoredSchemaUpgradePhase::Review
        {
            return Err(EditorAuthoredSchemaUpgradeError::Operation);
        }
        Ok(self
            .authored_schema_upgrade
            .review
            .as_ref()
            .expect("Review owns its complete prepared transition")
            .review())
    }

    /// Confirms the actual retained Review. Physical execution retirement precedes Pending.
    pub fn confirm_authored_schema_upgrade(
        &mut self,
        operation: EditorAuthoredSchemaUpgradeId,
    ) -> Result<(), EditorAuthoredSchemaUpgradeError> {
        use EditorAuthoredSchemaUpgradeError as E;
        if !self.accepts_new_work() {
            return Err(E::Closing);
        }
        if self.authored_schema_upgrade.view.operation != Some(operation)
            || self.authored_schema_upgrade.view.phase != EditorAuthoredSchemaUpgradePhase::Review
        {
            return Err(E::Operation);
        }
        if self.workspace.len() != self.authored_schema_upgrade.sources.len()
            || !self
                .authored_schema_upgrade
                .sources
                .iter()
                .all(|source| self.authoring_source_selection_is_current(source))
        {
            return Err(E::Stale);
        }
        let mut review = self
            .authored_schema_upgrade
            .review
            .take()
            .expect("Review owns one preparation");
        let project = self.authoring_project().ok_or(E::NotAuthoring)?;
        if let Err(error) = review.validate_live(project, &self.workspace) {
            self.authored_schema_upgrade.review = Some(review);
            self.authored_schema_upgrade.proposal_failure = Some(error);
            self.authored_schema_upgrade.view.refusal = Some(E::Stale);
            return Err(E::Stale);
        }
        let digest = review.recovery_digest();
        if self
            .begin_supervised_play_retirement(
                EditorSourceOperationId::AuthoredSchemaUpgrade(operation),
                digest,
            )
            .is_err()
        {
            self.authored_schema_upgrade.review = Some(review);
            return Err(E::Busy);
        }
        self.source_tools.cancel();
        self.authored_schema_upgrade.publication = Some(review.into_publication_parts());
        self.authored_schema_upgrade.retirement_started = true;
        self.authored_schema_upgrade.view.phase = EditorAuthoredSchemaUpgradePhase::Retiring;
        self.authored_schema_upgrade.view.work_pending = true;
        Ok(())
    }

    fn refuse_authored_schema_upgrade_proposal(
        &mut self,
        failure: SourceUpgradeProposalFailure,
    ) -> EditorAuthoredSchemaUpgradeError {
        let reason = if failure.error.host_work_limit_exceeded() {
            EditorAuthoredSchemaUpgradeError::WorkLimit
        } else {
            EditorAuthoredSchemaUpgradeError::Preparation
        };
        self.authored_schema_upgrade.baseline = Some(failure.baseline);
        self.authored_schema_upgrade.proposal_failure = Some(failure.error);
        self.authored_schema_upgrade.diagnostics = Some(
            if reason == EditorAuthoredSchemaUpgradeError::WorkLimit {
                single_error(
                    "project.editor.authored-upgrade-work-limit",
                    "The complete upgrade exhausted its selected work allowance; cancel before starting another operation",
                )
            } else {
                single_error(
                    "project.editor.authored-upgrade-proposal-refused",
                    "The proposal did not prove the complete authored transition; its original inputs remain available",
                )
            },
        );
        self.authored_schema_upgrade.view.refusal = Some(reason);
        self.authored_schema_upgrade.view.phase =
            EditorAuthoredSchemaUpgradePhase::AwaitingProposal;
        self.authored_schema_upgrade.view.work_pending = false;
        reason
    }

    fn restore_unstarted_authored_schema_upgrade_proposal(
        &mut self,
        baseline: SourceUpgradeBaseline,
    ) {
        self.authored_schema_upgrade.baseline = Some(baseline);
        self.authored_schema_upgrade.proposal_failure = None;
        self.authored_schema_upgrade.diagnostics = Some(single_error(
            "project.editor.authored-upgrade-preparation-task-refused",
            "The preparation Task did not start; its original captured inputs remain available for retry",
        ));
        self.authored_schema_upgrade.view.refusal = Some(EditorAuthoredSchemaUpgradeError::Task);
        self.authored_schema_upgrade.view.phase =
            EditorAuthoredSchemaUpgradePhase::AwaitingProposal;
        self.authored_schema_upgrade.view.work_pending = false;
    }

    pub(super) fn advance_authored_schema_upgrade(&mut self) {
        use EditorAuthoredSchemaUpgradePhase as P;
        if self.authored_schema_upgrade.retirement_started {
            self.advance_authored_schema_upgrade_publication();
            return;
        }
        if self.authored_schema_upgrade.view.phase == P::Draining && !self.closing {
            if self.persistence.has_pending_work() {
                return;
            }
            if let Some(expected) = self.authored_schema_upgrade.draining_save {
                if !matches!(&self.persistence,
                    persistence::PersistenceState::Terminal { attempt, result: EditorPersistenceResult::Saved { .. } }
                        if *attempt == expected)
                {
                    self.fail_authored_schema_upgrade(
                        EditorAuthoredSchemaUpgradeError::Persistence,
                        None,
                    );
                    return;
                }
                self.authored_schema_upgrade.draining_save = None;
            }
            if self.image_import.schema_adoption_pending()
                || self.source_asset_import.schema_adoption_pending()
                || self
                    .authoring_project()
                    .is_some_and(|project| project.source_index_scan_pending())
            {
                return;
            }
            #[cfg(feature = "presentation-2d")]
            if self.scene_capture.schema_adoption_pending() {
                return;
            }
            match self.capture_authored_schema_upgrade_baseline() {
                Ok(request) => {
                    let operation = self.authored_schema_upgrade.view.operation.unwrap();
                    #[cfg(test)]
                    let gate = self.authored_schema_upgrade.next_capture_gate.take();
                    self.authored_schema_upgrade.pending = Some(Work::Capture(Pending::start(
                        self.authoring_runtime.persistence_tasks(),
                        TaskSpawnRequest::new(operation.0, DOMAIN),
                        move || {
                            let captured = request.capture();
                            #[cfg(test)]
                            if let Some(gate) = gate {
                                gate();
                            }
                            captured
                        },
                    )));
                    self.authored_schema_upgrade.view.phase = P::Capturing;
                }
                Err(error) => self.fail_authored_schema_upgrade(
                    EditorAuthoredSchemaUpgradeError::Capture,
                    Some(error),
                ),
            }
        }
        let Some(mut pending) = self.authored_schema_upgrade.pending.take() else {
            return;
        };
        let Some(result) = pending.poll() else {
            self.authored_schema_upgrade.pending = Some(pending);
            return;
        };
        if self.closing || self.authored_schema_upgrade.cancelled {
            drop(result);
            self.authored_schema_upgrade
                .finish(EditorAuthoredSchemaUpgradeOutcome::Cancelled);
            return;
        }
        match result {
            Ok(Returned::Captured(Ok(baseline))) => {
                if !self
                    .authoring_project()
                    .is_some_and(|project| baseline.matches(project))
                    || self.workspace.len() != self.authored_schema_upgrade.sources.len()
                    || !self
                        .authored_schema_upgrade
                        .sources
                        .iter()
                        .all(|source| self.authoring_source_selection_is_current(source))
                {
                    drop(baseline);
                    self.fail_authored_schema_upgrade(
                        EditorAuthoredSchemaUpgradeError::Stale,
                        None,
                    );
                    return;
                }
                self.authored_schema_upgrade.view.documents = baseline.open_count();
                self.authored_schema_upgrade.view.sources = baseline.source_count();
                self.authored_schema_upgrade.baseline = Some(baseline);
                self.authored_schema_upgrade.view.phase = P::AwaitingProposal;
                self.authored_schema_upgrade.view.work_pending = false;
            }
            Ok(Returned::Captured(Err(error))) => self.fail_authored_schema_upgrade(
                EditorAuthoredSchemaUpgradeError::Capture,
                Some(error),
            ),
            Ok(Returned::Prepared(Ok(review))) => {
                if !self
                    .authoring_project()
                    .is_some_and(|project| review.matches(project))
                    || self.workspace.len() != self.authored_schema_upgrade.sources.len()
                    || !self
                        .authored_schema_upgrade
                        .sources
                        .iter()
                        .all(|source| self.authoring_source_selection_is_current(source))
                {
                    drop(review);
                    self.fail_authored_schema_upgrade(
                        EditorAuthoredSchemaUpgradeError::Stale,
                        None,
                    );
                    return;
                }
                self.authored_schema_upgrade.review = Some(review);
                self.authored_schema_upgrade.view.phase = P::Review;
                self.authored_schema_upgrade.view.work_pending = false;
            }
            Ok(Returned::Prepared(Err(failure))) => {
                self.refuse_authored_schema_upgrade_proposal(failure);
            }
            Ok(Returned::Unprepared(baseline)) => {
                self.restore_unstarted_authored_schema_upgrade_proposal(baseline);
            }
            Ok(Returned::Published(_)) => {
                unreachable!("publication returns through its persistent owner")
            }
            Err(
                WorkError::CancelledBeforeStart | WorkError::Unavailable | WorkError::Unobserved,
            ) => {
                self.fail_authored_schema_upgrade(EditorAuthoredSchemaUpgradeError::Task, None);
            }
        }
    }

    fn fail_authored_schema_upgrade(
        &mut self,
        reason: EditorAuthoredSchemaUpgradeError,
        error: Option<SourceUpgradePreparationError>,
    ) {
        let metadata_diagnostics = error.as_ref().and_then(|error| {
            let path = error.invalid_metadata_path()?;
            let mut diagnostic = Diagnostic::error(
                diagnostic_code("project.editor.authored-upgrade-metadata-invalid"),
                safe_summary("The complete upgrade inventory contains invalid asset metadata"),
            );
            if let Ok(field) = DiagnosticField::project_relative(field_key("metadata"), path) {
                diagnostic = diagnostic
                    .try_with_field(field)
                    .expect("the inventory diagnostic owns one unique bounded field");
            }
            Some(single_diagnostic(diagnostic))
        });
        let reason = if error
            .as_ref()
            .is_some_and(SourceUpgradePreparationError::host_work_limit_exceeded)
        {
            EditorAuthoredSchemaUpgradeError::WorkLimit
        } else {
            reason
        };
        self.authored_schema_upgrade.diagnostics = Some(match error {
            Some(SourceUpgradePreparationError::Source(report)) => report.diagnostics,
            Some(SourceUpgradePreparationError::Workspace(report)) => report.diagnostics,
            _ if reason == EditorAuthoredSchemaUpgradeError::WorkLimit => single_error(
                "project.editor.authored-upgrade-work-limit",
                "The complete upgrade exhausted its selected cumulative work allowance",
            ),
            _ => metadata_diagnostics.unwrap_or_else(|| {
                single_error(
                    "project.editor.authored-upgrade-capture-failed",
                    "The reviewed authored upgrade could not capture its complete original source set",
                )
            }),
        });
        self.authored_schema_upgrade
            .finish(EditorAuthoredSchemaUpgradeOutcome::Failed(reason));
    }

    fn capture_authored_schema_upgrade_baseline(
        &mut self,
    ) -> Result<
        crate::project_host::authoring::SourceUpgradeCaptureRequest,
        SourceUpgradePreparationError,
    > {
        let project = self
            .authoring_project()
            .ok_or(SourceUpgradePreparationError::OpenSet)?;
        let basis = project.schema_owner_input().schema_basis().clone();
        let mut work = AuthoredSchemaUpgradeWork::default();
        let input = self
            .workspace
            .capture_authored_schema_upgrade_input(&basis, &mut work)
            .map_err(SourceUpgradePreparationError::Workspace)?;
        let mut host_bytes = size_of::<EditorAuthoredSchemaUpgradeHost>();
        let mut worker_bytes =
            size_of::<crate::project_host::authoring::SourceUpgradeCaptureRequest>();
        for document in input.documents() {
            host_bytes = host_bytes
                .checked_add(
                    self.authoring_source_selection_retained_bytes(document)
                        .ok_or(SourceUpgradePreparationError::OpenSet)?,
                )
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
            let source = self
                .scene_lanes
                .get(&document)
                .and_then(|lane| lane.authoring_source.as_ref())
                .or_else(|| {
                    self.prefab_lanes
                        .get(&document)
                        .and_then(|lane| lane.authoring_source.as_ref())
                })
                .ok_or(SourceUpgradePreparationError::OpenSet)?;
            worker_bytes = worker_bytes
                .checked_add(size_of::<SourceUpgradeOpenInput>())
                .and_then(|bytes| bytes.checked_add(source.checked_retained_bytes()?))
                .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        }
        let mut inputs = project.admit_source_operation_inputs(
            host_bytes,
            worker_bytes,
            self.authored_schema_upgrade.view.limits.host_work(),
            self.authored_schema_upgrade.view.limits.open_handles(),
        )?;
        let mut sources = Vec::with_capacity(input.documents().len());
        let mut open = Vec::with_capacity(input.documents().len());
        for document in input.documents() {
            let selected = self
                .authoring_source_selection(document)
                .ok_or(SourceUpgradePreparationError::OpenSet)?;
            let observation = self.capture_authoring_source_observation(&selected, &mut inputs)?;
            open.push(SourceUpgradeOpenInput {
                document,
                source: selected.receipt.clone(),
                observation: Some(observation),
            });
            sources.push(selected);
        }
        let request = self
            .authoring_project()
            .unwrap()
            .capture_authored_upgrade_request(input, open, inputs, work)?;
        self.authored_schema_upgrade.sources = sources;
        Ok(request)
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;
