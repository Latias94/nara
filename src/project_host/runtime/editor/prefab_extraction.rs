//! Session-owned reviewed Prefab extraction under complete source write admission.

use std::sync::atomic::{AtomicU64, Ordering};

use nara_asset::AssetMetaIdentity;
use nara_diagnostic::DiagnosticReport;
use nara_scene::{SceneCompilationLimits, SceneEntityId};
use nara_tooling::EditorDocumentId;

use super::authoring_sources::AuthoringSourceSelection;
use super::*;
use crate::project_host::authoring::AuthoringSourceOperationLimits;
use crate::project_host::authoring::{
    PrefabExtractionCaptureRequest, PrefabExtractionOpenInput, PrefabExtractionReviewRequest,
    PrefabExtractionReviewRevisionRequest, PrefabExtractionSessionPreparationError,
    PrefabExtractionSessionPreparationFailure, PrefabExtractionSessionPublication,
    PrefabExtractionTarget, PreparedPrefabExtractionSessionReview,
};
use nara_scene::__private::PrefabExtractionWork;

mod publication;
mod publication_work;
use publication_work::PublicationWork;
mod preparation_work;
use preparation_work::{
    PreparationInput, PreparationReturn, PreparationStartFailure, PreparationWork,
};
mod selection;
pub use selection::{EditorPrefabExtractionSelection, EditorPrefabExtractionSelectionDocument};

const DOMAIN: nara_tasks::TaskDomainKey = nara_tasks::TaskDomainKey::new(0x7072_6566_6162_6578);
static NEXT_OPERATION: AtomicU64 = AtomicU64::new(1);

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

/// Identifies one finite Session operation; it does not authorize source publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorPrefabExtractionId(u64);

/// The destination metadata fixes the new Prefab identity before complete capture begins.
#[derive(Debug)]
pub struct EditorPrefabExtractionRequest {
    pub document: EditorDocumentId,
    pub root: SceneEntityId,
    pub target: AssetMetaIdentity,
    pub save_current: Vec<EditorDocumentId>,
    pub compilation_limits: SceneCompilationLimits,
}

impl EditorPrefabExtractionRequest {
    pub fn new(document: EditorDocumentId, root: SceneEntityId, target: AssetMetaIdentity) -> Self {
        Self {
            document,
            root,
            target,
            save_current: Vec::new(),
            compilation_limits: SceneCompilationLimits::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPrefabExtractionPhase {
    Idle,
    Draining,
    Preparing,
    AwaitingSelection,
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
pub enum EditorPrefabExtractionError {
    #[error("the Session is closing")]
    Closing,
    #[error("Prefab extraction requires a source Authoring project")]
    NotAuthoring,
    #[error("conflicting source or publication work is pending")]
    Busy,
    #[error("the extraction operation or phase does not match")]
    Operation,
    #[error("an accepted Save did not establish a confirmed baseline")]
    Persistence,
    #[error("the original complete source or Workspace authority changed")]
    Stale,
    #[error("the complete extraction preparation was refused")]
    Preparation,
    #[error("the complete extraction exhausted its selected cumulative Host work allowance")]
    WorkLimit,
    #[error("the complete extraction exceeds its selected cumulative Source allowance")]
    SourceLimit,
    #[error("the complete extraction exceeds its selected native-handle ceiling")]
    OpenHandleLimit,
    #[error("an execution owner did not complete physical cleanup")]
    ExecutionCleanup,
    #[error("the extraction publication requires reconciliation")]
    Publication,
    #[error("finite work was not admitted or did not return its actual result")]
    Task,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorPrefabExtractionOutcome {
    Cancelled,
    Adopted,
    Failed(EditorPrefabExtractionError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorPrefabExtractionView {
    pub operation: Option<EditorPrefabExtractionId>,
    pub phase: EditorPrefabExtractionPhase,
    pub outcome: Option<EditorPrefabExtractionOutcome>,
    pub refusal: Option<EditorPrefabExtractionError>,
    pub documents: usize,
    pub sources: usize,
    pub work_pending: bool,
    pub writes_fenced: bool,
    pub limits: AuthoringSourceOperationLimits,
    cancellation_allowed: bool,
    retry_allowed: bool,
}

impl EditorPrefabExtractionView {
    pub const fn can_cancel(&self) -> bool {
        self.writes_fenced && self.cancellation_allowed
    }

    /// Retries the retained unstarted preparation or returned publication without recapturing
    /// inputs or renewing the operation's allowance.
    pub const fn can_retry(&self) -> bool {
        self.writes_fenced && !self.work_pending && self.retry_allowed
    }

    /// The original captured preparation accepts a revised SaveCurrent proposal. Its original
    /// cumulative allowance may still refuse that proposal; revision never renews the limit.
    pub const fn can_revise(&self) -> bool {
        self.writes_fenced
            && !self.work_pending
            && matches!(
                self.phase,
                EditorPrefabExtractionPhase::Review
                    | EditorPrefabExtractionPhase::AwaitingSelection
            )
    }
}

impl Default for EditorPrefabExtractionView {
    fn default() -> Self {
        Self {
            operation: None,
            phase: EditorPrefabExtractionPhase::Idle,
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

/// Read-only observation of this operation's already prepared complete outputs.
pub struct EditorPrefabExtractionReview<'a> {
    prepared: &'a PreparedPrefabExtractionSessionReview,
}

impl EditorPrefabExtractionReview<'_> {
    pub fn source_count(&self) -> usize {
        self.prepared.source_count()
    }
    pub fn documents(
        &self,
    ) -> impl ExactSizeIterator<
        Item = (
            EditorDocumentId,
            nara_tooling::EditorPrefabExtractionDisposition,
        ),
    > + '_ {
        self.prepared.documents()
    }
    pub fn outputs(
        &self,
    ) -> impl ExactSizeIterator<Item = EditorPrefabExtractionReviewedOutput<'_>> + '_ {
        self.prepared
            .outputs()
            .map(|output| EditorPrefabExtractionReviewedOutput { output })
    }
}

pub struct EditorPrefabExtractionReviewedOutput<'a> {
    output: crate::project_host::authoring::ProjectPrefabExtractionOutput<'a>,
}

impl EditorPrefabExtractionReviewedOutput<'_> {
    pub fn project_path(&self) -> &str {
        self.output.path()
    }
    pub fn source_key(&self) -> Option<&nara_scene::SourceDocumentKey> {
        self.output.source_key()
    }
    /// Metadata is JSON; authored source outputs retain their actual JSON/RON encoding.
    pub fn source_encoding(&self) -> Option<nara_scene::SceneFileEncoding> {
        match self.output.encoding() {
            crate::project_host::authoring::ProjectPrefabExtractionOutputEncoding::Source(
                encoding,
            ) => Some(encoding),
            crate::project_host::authoring::ProjectPrefabExtractionOutputEncoding::MetadataJson => {
                None
            }
        }
    }
    pub fn bytes(&self) -> &[u8] {
        self.output
            .bytes()
            .expect("Review retains its complete actual output bytes")
    }
}

type PreparationRefusal = (EditorPrefabExtractionError, Option<DiagnosticReport>);

fn preparation_reason(
    error: &PrefabExtractionSessionPreparationError,
) -> EditorPrefabExtractionError {
    if error.host_work_limit_exceeded() {
        EditorPrefabExtractionError::WorkLimit
    } else if error.source_limit_exceeded() {
        EditorPrefabExtractionError::SourceLimit
    } else if error.open_handles_limit_exceeded() {
        EditorPrefabExtractionError::OpenHandleLimit
    } else {
        EditorPrefabExtractionError::Preparation
    }
}

fn preparation_refusal(error: PrefabExtractionSessionPreparationError) -> PreparationRefusal {
    let reason = preparation_reason(&error);
    (reason, error.into_diagnostics())
}

fn capture_refusal(
    error: crate::project_host::authoring::AuthoringProjectSourceAdmissionError,
) -> PreparationRefusal {
    use crate::project_host::authoring::{
        AuthoringProjectAdmissionError as A, AuthoringProjectSourceAdmissionError as E,
    };
    match error {
        E::Budget(A::DecodeWorkExceeded { .. }) => (EditorPrefabExtractionError::WorkLimit, None),
        E::Budget(A::PeakOpenHandlesExceeded { .. }) => {
            (EditorPrefabExtractionError::OpenHandleLimit, None)
        }
        E::CompilationBudget(diagnostics) => {
            (EditorPrefabExtractionError::Preparation, Some(diagnostics))
        }
        _ => (EditorPrefabExtractionError::Preparation, None),
    }
}

#[derive(Default)]
pub(super) struct EditorPrefabExtractionHost {
    view: EditorPrefabExtractionView,
    request: Option<EditorPrefabExtractionRequest>,
    pending: Option<PreparationWork>,
    review: Option<PreparedPrefabExtractionSessionReview>,
    preparation_failure: Option<PrefabExtractionSessionPreparationFailure>,
    unstarted: Option<PreparationInput>,
    publication_work: Option<PublicationWork>,
    publication: Option<PrefabExtractionSessionPublication>,
    sources: Vec<AuthoringSourceSelection>,
    draining_save: Option<EditorPersistenceAttemptId>,
    cancelled: bool,
    retirement_started: bool,
    diagnostics: Option<DiagnosticReport>,
    #[cfg(test)]
    next_preparation_gate: Option<Box<dyn FnOnce() + Send>>,
}

impl EditorPrefabExtractionHost {
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
        if !self.view.can_cancel() || self.cancelled {
            return;
        }
        self.cancelled = true;
        if let Some(pending) = &self.pending {
            pending.cancel();
        } else if !self.retirement_started {
            self.finish(EditorPrefabExtractionOutcome::Cancelled);
        } else {
            // A publisher may already have run. Its actual returned owner decides whether
            // native cleanup can preserve the incumbent before the write fence is released.
            self.view.work_pending = true;
            if let Some(publication) = &mut self.publication {
                publication.request_cancellation();
            }
        }
    }
    fn finish(&mut self, outcome: EditorPrefabExtractionOutcome) {
        debug_assert!(
            self.pending.is_none() && self.publication_work.is_none(),
            "physical worker return precedes input release"
        );
        self.review = None;
        self.preparation_failure = None;
        self.unstarted = None;
        self.publication = None;
        self.request = None;
        self.sources = Vec::new();
        self.draining_save = None;
        self.retirement_started = false;
        self.view.phase = EditorPrefabExtractionPhase::Terminal;
        self.view.outcome = Some(outcome);
        self.view.writes_fenced = false;
        self.view.work_pending = false;
        self.view.cancellation_allowed = false;
        self.view.retry_allowed = false;
    }
}

impl EditorProjectSession {
    pub fn prefab_extraction_view(&self) -> &EditorPrefabExtractionView {
        &self.prefab_extraction.view
    }
    pub fn prefab_extraction_diagnostics(&self) -> Option<&DiagnosticReport> {
        self.prefab_extraction
            .publication
            .as_ref()
            .and_then(|publication| publication.failure())
            .and_then(|failure| failure.diagnostics())
            .or_else(|| {
                self.prefab_extraction
                    .preparation_failure
                    .as_ref()
                    .and_then(|failure| failure.error().diagnostics())
            })
            .or(self.prefab_extraction.diagnostics.as_ref())
    }

    /// Fences new source writes immediately; already accepted Save work establishes the exact
    /// original disk baseline before this operation captures any working input.
    pub fn begin_prefab_extraction(
        &mut self,
        request: EditorPrefabExtractionRequest,
    ) -> Result<EditorPrefabExtractionId, EditorPrefabExtractionError> {
        self.begin_prefab_extraction_with_limits(request, AuthoringSourceOperationLimits::default())
    }

    pub fn begin_prefab_extraction_with_limits(
        &mut self,
        request: EditorPrefabExtractionRequest,
        limits: AuthoringSourceOperationLimits,
    ) -> Result<EditorPrefabExtractionId, EditorPrefabExtractionError> {
        use EditorPrefabExtractionError as E;
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
        let operation = EditorPrefabExtractionId(
            NEXT_OPERATION
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                    next.checked_add(1)
                })
                .map_err(|_| E::Operation)?,
        );
        self.prefab_extraction = EditorPrefabExtractionHost {
            view: EditorPrefabExtractionView {
                operation: Some(operation),
                phase: EditorPrefabExtractionPhase::Draining,
                work_pending: true,
                writes_fenced: true,
                limits,
                ..Default::default()
            },
            draining_save: match self.persistence {
                persistence::PersistenceState::Pending { attempt, .. } => Some(attempt),
                _ => None,
            },
            request: Some(request),
            ..Default::default()
        };
        Ok(operation)
    }

    pub fn cancel_prefab_extraction(
        &mut self,
        operation: EditorPrefabExtractionId,
    ) -> Result<(), EditorPrefabExtractionError> {
        if self.prefab_extraction.view.operation != Some(operation)
            || !self.prefab_extraction.view.can_cancel()
        {
            return Err(EditorPrefabExtractionError::Operation);
        }
        self.prefab_extraction.cancel();
        Ok(())
    }

    pub fn prefab_extraction_review(
        &self,
        operation: EditorPrefabExtractionId,
    ) -> Result<EditorPrefabExtractionReview<'_>, EditorPrefabExtractionError> {
        if self.prefab_extraction.view.operation != Some(operation)
            || self.prefab_extraction.view.phase != EditorPrefabExtractionPhase::Review
        {
            return Err(EditorPrefabExtractionError::Operation);
        }
        Ok(EditorPrefabExtractionReview {
            prepared: self
                .prefab_extraction
                .review
                .as_ref()
                .expect("Review owns its exact complete preparation"),
        })
    }

    /// Confirms the retained Review. Physical execution retirement precedes every possible
    /// Pending replacement; the final Workspace and native destinations are bound together.
    pub fn confirm_prefab_extraction(
        &mut self,
        operation: EditorPrefabExtractionId,
    ) -> Result<(), EditorPrefabExtractionError> {
        use EditorPrefabExtractionError as E;
        if !self.accepts_new_work() {
            return Err(E::Closing);
        }
        if self.prefab_extraction.view.operation != Some(operation)
            || self.prefab_extraction.view.phase != EditorPrefabExtractionPhase::Review
        {
            return Err(E::Operation);
        }
        if self.workspace.len() != self.prefab_extraction.sources.len()
            || !self
                .prefab_extraction
                .sources
                .iter()
                .all(|source| self.authoring_source_selection_is_current(source))
        {
            return Err(E::Stale);
        }
        if self.authoring_project().is_none() {
            return Err(E::NotAuthoring);
        }
        let mut review = self
            .prefab_extraction
            .review
            .take()
            .expect("Review owns its exact complete preparation");
        if let Err(error) = review.validate_live(self.authoring_project().unwrap(), &self.workspace)
        {
            let (reason, diagnostics) = preparation_refusal(error);
            let reason = match reason {
                E::WorkLimit | E::SourceLimit | E::OpenHandleLimit => reason,
                _ => E::Stale,
            };
            self.prefab_extraction.review = Some(review);
            self.prefab_extraction.view.refusal = Some(reason);
            self.prefab_extraction.diagnostics = diagnostics;
            return Err(reason);
        }
        if self
            .begin_supervised_play_retirement(
                EditorSourceOperationId::PrefabExtraction(operation),
                review.digest(),
            )
            .is_err()
        {
            self.prefab_extraction.review = Some(review);
            return Err(E::Busy);
        }
        self.source_tools.cancel();
        self.prefab_extraction.publication = Some(review.into_publication());
        self.prefab_extraction.retirement_started = true;
        self.prefab_extraction.view.phase = EditorPrefabExtractionPhase::Retiring;
        self.prefab_extraction.view.work_pending = true;
        self.prefab_extraction.view.refusal = None;
        self.prefab_extraction.diagnostics = None;
        Ok(())
    }

    /// Continues this operation's unstarted preparation or retained physical publication.
    /// A missing publisher return or uncertain replacement requires explicit recovery.
    pub fn retry_prefab_extraction(
        &mut self,
        operation: EditorPrefabExtractionId,
    ) -> Result<(), EditorPrefabExtractionError> {
        use EditorPrefabExtractionError as E;
        if !self.authoring_runtime.host_custody().is_accepting() {
            return Err(E::Closing);
        }
        if self.prefab_extraction.view.operation == Some(operation)
            && self.prefab_extraction.view.phase == EditorPrefabExtractionPhase::AwaitingSelection
        {
            if self.prefab_extraction.unstarted.is_none() {
                return Err(E::Operation);
            }
            if !self.prefab_extraction.view.can_retry() {
                return Err(self.prefab_extraction.view.refusal.unwrap_or(E::Operation));
            }
            let input = self
                .prefab_extraction
                .unstarted
                .take()
                .ok_or(E::Operation)?;
            return self.start_prefab_extraction_preparation(input);
        }
        let host = &mut self.prefab_extraction;
        if host.view.operation != Some(operation)
            || host.view.phase != EditorPrefabExtractionPhase::RequiresReconcile
            || !host.view.writes_fenced
        {
            return Err(E::Operation);
        }
        if host.publication_work.is_some() || host.view.work_pending {
            return Err(E::Busy);
        }
        let publication = host.publication.as_mut().ok_or(E::Task)?;
        if !publication.request_retry() {
            return Err(if publication.host_work_exhausted() {
                E::WorkLimit
            } else if publication
                .failure()
                .is_some_and(|failure| failure.source_limit_exceeded())
            {
                E::SourceLimit
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

    pub(super) fn advance_prefab_extraction(&mut self) {
        use EditorPrefabExtractionPhase as P;
        if self.prefab_extraction.retirement_started {
            self.advance_prefab_extraction_publication();
            return;
        }
        if self.prefab_extraction.view.phase == P::Draining && !self.closing {
            if self.persistence.has_pending_work() {
                return;
            }
            if let Some(expected) = self.prefab_extraction.draining_save {
                if !matches!(&self.persistence, persistence::PersistenceState::Terminal { attempt, result: EditorPersistenceResult::Saved { .. } } if *attempt == expected)
                {
                    self.fail_prefab_extraction(EditorPrefabExtractionError::Persistence, None);
                    return;
                }
                self.prefab_extraction.draining_save = None;
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
            match self.capture_prefab_extraction_review_request() {
                Ok(request) => {
                    let _ = self
                        .start_prefab_extraction_preparation(PreparationInput::Initial(request));
                }
                Err((reason, diagnostics)) => self.fail_prefab_extraction(reason, diagnostics),
            }
        }
        let Some(mut pending) = self.prefab_extraction.pending.take() else {
            return;
        };
        let Some(result) = pending.poll() else {
            self.prefab_extraction.pending = Some(pending);
            return;
        };
        drop(pending);
        if self.closing || self.prefab_extraction.cancelled {
            drop(result);
            self.prefab_extraction
                .finish(EditorPrefabExtractionOutcome::Cancelled);
            return;
        }
        match result {
            Ok(PreparationReturn::Prepared(Ok(review))) => {
                if !self
                    .authoring_project()
                    .is_some_and(|project| review.matches(project))
                    || self.workspace.len() != self.prefab_extraction.sources.len()
                    || !self
                        .prefab_extraction
                        .sources
                        .iter()
                        .all(|source| self.authoring_source_selection_is_current(source))
                {
                    drop(review);
                    self.fail_prefab_extraction(EditorPrefabExtractionError::Stale, None);
                    return;
                }
                self.prefab_extraction.view.sources = review.source_count();
                self.prefab_extraction.view.documents = review.documents().len();
                self.prefab_extraction.review = Some(review);
                self.prefab_extraction.preparation_failure = None;
                self.prefab_extraction.view.phase = P::Review;
                self.prefab_extraction.view.work_pending = false;
                self.prefab_extraction.view.refusal = None;
                self.prefab_extraction.view.retry_allowed = false;
                self.prefab_extraction.diagnostics = None;
            }
            Ok(PreparationReturn::Prepared(Err(failure))) => {
                self.observe_prefab_extraction_preparation_failure(failure);
            }
            Ok(PreparationReturn::Unstarted(input)) => {
                self.observe_prefab_extraction_unstarted(PreparationStartFailure {
                    input,
                    error: None,
                })
            }
            Err(_) => self.fail_prefab_extraction(EditorPrefabExtractionError::Task, None),
        }
    }

    fn fail_prefab_extraction(
        &mut self,
        reason: EditorPrefabExtractionError,
        diagnostics: Option<DiagnosticReport>,
    ) {
        let (code, summary) = match reason {
            EditorPrefabExtractionError::WorkLimit => (
                "project.editor.prefab-extraction-work-limit",
                "The complete extraction exhausted its selected cumulative Host work allowance",
            ),
            EditorPrefabExtractionError::SourceLimit => (
                "project.editor.prefab-extraction-source-limit",
                "The complete extraction exceeds its selected cumulative Source allowance",
            ),
            EditorPrefabExtractionError::OpenHandleLimit => (
                "project.editor.prefab-extraction-handle-limit",
                "The complete extraction exceeds its selected native-handle ceiling",
            ),
            _ => (
                "project.editor.prefab-extraction-refused",
                "The complete Prefab extraction did not establish its required original inputs",
            ),
        };
        self.prefab_extraction.diagnostics = diagnostics.or_else(|| {
            Some(crate::project_host::runtime_diagnostics::single_error(
                code, summary,
            ))
        });
        self.prefab_extraction
            .finish(EditorPrefabExtractionOutcome::Failed(reason));
    }

    fn capture_prefab_extraction_review_request(
        &mut self,
    ) -> Result<PrefabExtractionReviewRequest, PreparationRefusal> {
        let request = self
            .prefab_extraction
            .request
            .take()
            .expect("Draining retains its original request");
        let refused = || (EditorPrefabExtractionError::Preparation, None);
        let project = self.authoring_project().ok_or_else(refused)?;
        let basis = project.schema_owner_input().schema_basis().clone();
        let mut work = PrefabExtractionWork::new(request.compilation_limits).map_err(|report| {
            preparation_refusal(PrefabExtractionSessionPreparationError::capture(
                report.diagnostics,
            ))
        })?;
        let input = self
            .workspace
            .capture_prefab_extraction_input(request.document, &request.root, &basis, &mut work)
            .map_err(|report| {
                preparation_refusal(PrefabExtractionSessionPreparationError::capture(
                    report.diagnostics,
                ))
            })?;
        let mut host_bytes =
            size_of::<EditorPrefabExtractionHost>() + size_of::<EditorPrefabExtractionRequest>();
        let mut worker_bytes =
            PrefabExtractionCaptureRequest::metadata_retained_bytes(project, &request.target)
                .map_err(capture_refusal)?;
        for document in input.documents() {
            host_bytes = host_bytes
                .checked_add(
                    self.authoring_source_selection_retained_bytes(document)
                        .ok_or_else(refused)?,
                )
                .ok_or_else(refused)?;
            let source = self
                .scene_lanes
                .get(&document)
                .and_then(|lane| lane.authoring_source.as_ref())
                .or_else(|| {
                    self.prefab_lanes
                        .get(&document)
                        .and_then(|lane| lane.authoring_source.as_ref())
                })
                .ok_or_else(refused)?;
            worker_bytes = worker_bytes
                .checked_add(
                    PrefabExtractionOpenInput::retained_bytes(Some(source))
                        .map_err(capture_refusal)?,
                )
                .ok_or_else(refused)?;
        }
        let mut inputs = project
            .admit_source_operation_inputs(
                host_bytes,
                worker_bytes,
                self.prefab_extraction.view.limits.host_work(),
                self.prefab_extraction.view.limits.open_handles(),
            )
            .map_err(capture_refusal)?;
        let mut sources = Vec::with_capacity(input.documents().len());
        let mut open = Vec::with_capacity(input.documents().len());
        for document in input.documents() {
            let source = self
                .authoring_source_selection(document)
                .ok_or_else(refused)?;
            let observation = self
                .capture_authoring_source_observation(&source, &mut inputs)
                .map_err(capture_refusal)?;
            open.push(PrefabExtractionOpenInput {
                document,
                source: source.receipt.clone(),
                observation: Some(observation),
            });
            sources.push(source);
        }
        let capture = self
            .authoring_project()
            .unwrap()
            .capture_prefab_extraction_request(
                input,
                open,
                inputs,
                work,
                PrefabExtractionTarget {
                    metadata: request.target,
                },
            );
        self.prefab_extraction.sources = sources;
        PrefabExtractionReviewRequest::new(capture, request.save_current)
            .map_err(preparation_refusal)
    }
}
