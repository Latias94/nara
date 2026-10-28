//! Installed metadata recovery remains one finite Project Session operation.

use std::sync::{
    Arc, Mutex, TryLockError,
    atomic::{AtomicU64, Ordering},
};

use nara_fs::{ContentDigest, FileIdentity, RelativePath, ReplaceReceipt};
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskPoolKind, TaskPools, TaskReturnState, TaskSpawnRequest,
};
use nara_tooling::{
    EditorDocumentId, EditorWorkspaceSchemaReclassificationRequest,
    PreparedEditorWorkspaceSchemaReclassification,
};

use super::authoring_sources::AuthoringSourceSelection;
use super::persistence::{HostCustody, RetainedHost};
use super::{EditorExecutionLane, EditorProjectSession, EditorSourceToolInstallationInspection};
use crate::project_host::{
    authoring::{
        AdmittedAuthoringSchemaOwner, AuthoringDocumentCapture, AuthoringPrefabSessionSnapshot,
        AuthoringProjectSessionSnapshot, AuthoringSchemaOwnerAttachmentFailure,
        AuthoringSchemaOwnerPreparation, AuthoringSchemaOwnerProjectInput,
        AuthoringSchemaOwnerReconciliation, PreparedAuthoringSchemaOwner,
        PreparedSchemaOwnerProject, PreparedSchemaOwnerSceneCapture,
        SchemaOwnerObservedProjectAttachmentFailure, SchemaOwnerProjectAttachmentFailure,
    },
    file_publication::{FilePublicationEvidence, FilePublicationHost, FilePublicationOutcome},
};

const DOMAIN: TaskDomainKey = TaskDomainKey::new(0x7363_6865_6d61_6f77);
static NEXT_OPERATION: AtomicU64 = AtomicU64::new(1);

mod failure;
use EditorSchemaOwnerAdoptionFailure as Failure;
pub use failure::EditorSchemaOwnerAdoptionFailure;

/// Process-local operation identity; no filesystem or executable authority follows from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EditorSchemaOwnerAdoptionId(u64);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSchemaOwnerAdoptionPhase {
    Idle,
    Quiescing,
    Inspecting,
    Preparing,
    Review,
    Publishing,
    Terminal,
    RequiresReconcile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum EditorSchemaOwnerAdoptionError {
    #[error("the Session is closing")]
    Closing,
    #[error("conflicting source or publication work is pending")]
    Busy,
    #[error("this operation requires an authoring project")]
    NotAuthoring,
    #[error("the installed release observation is unavailable or changed")]
    Installation,
    #[error("the metadata owner cannot be composed with this project")]
    Schema,
    #[error("the original source set or project observation changed")]
    Stale,
    #[error("source/history or actual dependency preparation failed")]
    Preparation,
    #[error("the finite task was not admitted or did not return a result")]
    Task,
    #[error("the original file observation refused publication")]
    Publication,
    #[error("publication facts require explicit reconciliation")]
    RequiresReconcile,
    #[error("the operation or its current phase does not match")]
    Operation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorSchemaOwnerAdoptionOutcome {
    AlreadyAdmitted,
    Adopted,
    Cancelled,
    Failed(EditorSchemaOwnerAdoptionError),
    PublishedWithoutAdoption,
    Uncertain,
}

/// Immutable finite observations; installation and execution approval are separate operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorSchemaOwnerAdoptionView {
    pub operation: Option<EditorSchemaOwnerAdoptionId>,
    pub phase: EditorSchemaOwnerAdoptionPhase,
    pub outcome: Option<EditorSchemaOwnerAdoptionOutcome>,
    pub owner: Option<Arc<str>>,
    pub documents: usize,
    pub degraded_roots: usize,
    pub reconciliation: bool,
}

impl Default for EditorSchemaOwnerAdoptionView {
    fn default() -> Self {
        Self {
            operation: None,
            phase: EditorSchemaOwnerAdoptionPhase::Idle,
            outcome: None,
            owner: None,
            documents: 0,
            degraded_roots: 0,
            reconciliation: false,
        }
    }
}

struct Installation {
    directory: Arc<str>,
    receipt: ContentDigest,
    identity: FileIdentity,
    manifest: ContentDigest,
    package: Arc<str>,
    owner: Arc<str>,
}

struct Start {
    input: AuthoringSchemaOwnerProjectInput,
    directory: Arc<str>,
    receipt: ContentDigest,
    reconcile: bool,
}
struct Metadata {
    input: AuthoringSchemaOwnerProjectInput,
    installation: Installation,
    owner: PreparedAuthoringSchemaOwner,
    publisher: Option<RetainedHost<FilePublicationHost>>,
}
enum Inspected {
    AlreadyAdmitted,
    Prepared(Metadata),
}
struct Candidate {
    metadata: Metadata,
    workspace: PreparedEditorWorkspaceSchemaReclassification,
    project: PreparedSchemaOwnerProject,
    current: Vec<(EditorDocumentId, PreparedSchemaOwnerSceneCapture)>,
    sources: Vec<AuthoringSourceSelection>,
}
enum Publication {
    Written(FilePublicationOutcome),
    Observed,
}
struct Published {
    candidate: Candidate,
    outcome: Publication,
}
struct PublishedProject {
    project: AuthoringProjectSessionSnapshot,
    evidence: FilePublicationEvidence,
}
enum AttachmentFailure {
    Metadata(AuthoringSchemaOwnerAttachmentFailure),
    Project(SchemaOwnerProjectAttachmentFailure),
    Current(CurrentAttachmentFailure),
    ObservedProject(SchemaOwnerObservedProjectAttachmentFailure),
}
struct CurrentAttachmentFailure {
    owner: AdmittedAuthoringSchemaOwner,
    project: PreparedSchemaOwnerProject,
    failed: (EditorDocumentId, PreparedSchemaOwnerSceneCapture),
    attached: Vec<(EditorDocumentId, AuthoringDocumentCapture)>,
    remaining: std::vec::IntoIter<(EditorDocumentId, PreparedSchemaOwnerSceneCapture)>,
}
impl AttachmentFailure {
    fn evidence(&self) -> Option<&FilePublicationEvidence> {
        match self {
            Self::Metadata(failure) => Some(failure.publication.evidence()),
            Self::Project(failure) => Some(&failure.successor.publication),
            Self::Current(failure) => Some(&failure.owner.publication),
            Self::ObservedProject(_) => None,
        }
    }
    fn diagnostics(&self) -> Option<&nara_diagnostic::DiagnosticReport> {
        match self {
            Self::Project(failure) => Some(failure.prepared.diagnostics()),
            Self::ObservedProject(failure) => Some(failure.prepared.diagnostics()),
            Self::Current(failure) => Some(failure.project.diagnostics()),
            _ => None,
        }
    }
    fn current_diagnostics(
        &self,
        document: EditorDocumentId,
    ) -> Option<&nara_diagnostic::DiagnosticReport> {
        let Self::Current(failure) = self else {
            return None;
        };
        if failure.failed.0 == document {
            return Some(failure.failed.1.diagnostics());
        }
        failure
            .attached
            .iter()
            .find(|(id, _)| *id == document)
            .map(|(_, capture)| capture.diagnostics())
            .or_else(|| {
                failure
                    .remaining
                    .as_slice()
                    .iter()
                    .find(|(id, _)| *id == document)
                    .map(|(_, capture)| capture.diagnostics())
            })
    }
}

enum Work {
    Inspect(Pending<Result<Inspected, Failure>>),
    Prepare(Pending<Result<Candidate, Failure>>),
    Publish(Pending<Result<Published, Failure>>),
}

#[derive(Default)]
pub(super) struct EditorSchemaOwnerAdoptionHost {
    view: EditorSchemaOwnerAdoptionView,
    start: Option<Start>,
    work: Option<Work>,
    candidate: Option<Candidate>,
    published: Option<PublishedProject>,
    evidence: Option<FilePublicationEvidence>,
    publication_host: Option<RetainedHost<FilePublicationHost>>,
    captures: Vec<(EditorDocumentId, AuthoringDocumentCapture)>,
    installation: Option<(Arc<str>, ContentDigest)>,
    attachment_failure: Option<AttachmentFailure>,
    failure: Option<Failure>,
    cancelled: bool,
    #[cfg(test)]
    next_publication_gate: Option<Box<dyn FnOnce() + Send>>,
}

struct Job<T> {
    run: Option<Box<dyn FnOnce() -> T + Send>>,
    started: bool,
    result: Option<T>,
}
struct Pending<T> {
    job: Arc<Mutex<Job<T>>>,
    task: Option<TaskHandle<()>>,
    rejection: Option<nara_tasks::TaskRejection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorkError {
    CancelledBeforeStart,
    Unavailable,
    Unobserved,
}

impl<T: Send + 'static> Pending<T> {
    fn start(pools: &TaskPools, run: impl FnOnce() -> T + Send + 'static) -> Self {
        let job = Arc::new(Mutex::new(Job {
            run: Some(Box::new(run) as Box<dyn FnOnce() -> T + Send>),
            started: false,
            result: None,
        }));
        let worker = Arc::clone(&job);
        let admitted = pools
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, DOMAIN),
                move |context| {
                    let mut job = worker.lock().unwrap_or_else(|error| error.into_inner());
                    if context.is_cancelled() {
                        return;
                    }
                    job.started = true;
                    let run = job.run.take().expect("finite operation executes once");
                    job.result = Some(run());
                },
            )
            .into_handle();
        let (task, rejection) = match admitted {
            Ok(task) => (Some(task), None),
            Err(rejection) => (None, Some(rejection)),
        };
        Self {
            job,
            task,
            rejection,
        }
    }
    fn poll(&mut self) -> Option<Result<T, WorkError>> {
        if self
            .task
            .as_ref()
            .is_some_and(|task| task.return_observer().state() == TaskReturnState::Pending)
        {
            return None;
        }
        let mut job = match self.job.try_lock() {
            Ok(job) => job,
            Err(TryLockError::WouldBlock) => return None,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        Some(match job.result.take() {
            Some(result) => Ok(result),
            None if job.started => Err(WorkError::Unobserved),
            None if self
                .task
                .as_ref()
                .is_some_and(|task| task.cancellation_token().is_cancelled()) =>
            {
                Err(WorkError::CancelledBeforeStart)
            }
            None => Err(WorkError::Unavailable),
        })
    }
    fn cancel(&self) {
        if let Some(task) = &self.task {
            task.cancel();
        }
    }
}

impl EditorSchemaOwnerAdoptionHost {
    pub(super) fn fenced(&self) -> bool {
        matches!(
            self.view.phase,
            EditorSchemaOwnerAdoptionPhase::Quiescing
                | EditorSchemaOwnerAdoptionPhase::Inspecting
                | EditorSchemaOwnerAdoptionPhase::Preparing
                | EditorSchemaOwnerAdoptionPhase::Review
                | EditorSchemaOwnerAdoptionPhase::Publishing
        )
    }
    pub(super) fn pending(&self) -> bool {
        self.work.is_some() || (self.publication_host.is_some() && self.candidate.is_none())
    }
    pub(super) fn work_pending(&self) -> bool {
        self.view.phase == EditorSchemaOwnerAdoptionPhase::Quiescing || self.pending()
    }
    pub(super) fn cancel(&mut self) {
        if matches!(
            self.view.phase,
            EditorSchemaOwnerAdoptionPhase::Idle
                | EditorSchemaOwnerAdoptionPhase::Terminal
                | EditorSchemaOwnerAdoptionPhase::RequiresReconcile
        ) {
            return;
        }
        self.cancelled = true;
        self.start = None;
        if let Some(work) = &self.work {
            match work {
                Work::Inspect(task) => task.cancel(),
                Work::Prepare(task) => task.cancel(),
                Work::Publish(task) => task.cancel(),
            }
        }
        if self.work.is_none() {
            self.finish(EditorSchemaOwnerAdoptionOutcome::Cancelled);
        }
    }
    fn finish(&mut self, outcome: EditorSchemaOwnerAdoptionOutcome) {
        self.candidate = None;
        self.retry_cleanup();
        self.view.outcome = Some(outcome);
        self.view.phase = if matches!(
            outcome,
            EditorSchemaOwnerAdoptionOutcome::PublishedWithoutAdoption
                | EditorSchemaOwnerAdoptionOutcome::Uncertain
        ) || (self.view.reconciliation
            && !matches!(
                outcome,
                EditorSchemaOwnerAdoptionOutcome::Adopted
                    | EditorSchemaOwnerAdoptionOutcome::AlreadyAdmitted
            )) {
            EditorSchemaOwnerAdoptionPhase::RequiresReconcile
        } else {
            EditorSchemaOwnerAdoptionPhase::Terminal
        };
    }
    fn retry_cleanup(&mut self) {
        if self.work.is_none()
            && self.candidate.is_none()
            && self
                .publication_host
                .as_ref()
                .is_some_and(RetainedHost::retire)
        {
            self.publication_host = None;
        }
    }
}

impl EditorProjectSession {
    pub fn schema_owner_adoption_view(&self) -> &EditorSchemaOwnerAdoptionView {
        &self.schema_owner_adoption.view
    }

    /// Retains exact failure facts independently of cancellation and actual publication evidence.
    pub fn schema_owner_adoption_failure(&self) -> Option<&EditorSchemaOwnerAdoptionFailure> {
        self.schema_owner_adoption.failure.as_ref()
    }

    /// Begins code-free installed metadata inspection. It neither enables the tool nor approves execution.
    pub fn prepare_installed_schema_owner(
        &mut self,
        directory: &str,
        receipt: ContentDigest,
    ) -> Result<EditorSchemaOwnerAdoptionId, EditorSchemaOwnerAdoptionError> {
        if !self.accepts_new_work() {
            return Err(EditorSchemaOwnerAdoptionError::Closing);
        }
        if self.schema_owner_adoption.view.phase
            == EditorSchemaOwnerAdoptionPhase::RequiresReconcile
        {
            return Err(EditorSchemaOwnerAdoptionError::RequiresReconcile);
        }
        if self.schema_owner_adoption.fenced()
            || self.schema_owner_adoption.pending()
            || self.schema_adoption_conflicting_work()
        {
            return Err(EditorSchemaOwnerAdoptionError::Busy);
        }
        if directory.len() > 512 || RelativePath::new(directory).is_err() {
            return Err(EditorSchemaOwnerAdoptionError::Installation);
        }
        let input = self
            .authoring_project()
            .ok_or(EditorSchemaOwnerAdoptionError::NotAuthoring)?
            .schema_owner_input();
        let id = EditorSchemaOwnerAdoptionId(
            NEXT_OPERATION
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| EditorSchemaOwnerAdoptionError::Operation)?,
        );
        self.schema_owner_adoption = EditorSchemaOwnerAdoptionHost {
            view: EditorSchemaOwnerAdoptionView {
                operation: Some(id),
                phase: EditorSchemaOwnerAdoptionPhase::Quiescing,
                ..Default::default()
            },
            start: Some(Start {
                input,
                directory: Arc::from(directory),
                receipt,
                reconcile: false,
            }),
            installation: Some((Arc::from(directory), receipt)),
            ..Default::default()
        };
        Ok(id)
    }

    pub fn cancel_schema_owner_adoption(
        &mut self,
        operation: EditorSchemaOwnerAdoptionId,
    ) -> Result<(), EditorSchemaOwnerAdoptionError> {
        self.require_schema_adoption_operation(operation)?;
        self.schema_owner_adoption.cancel();
        Ok(())
    }

    /// Rechecks actual disk metadata and the complete current dirty source set under a new review.
    /// This grants no execution approval and never reuses the stale original prepared candidate.
    pub fn reconcile_schema_owner_adoption(
        &mut self,
        operation: EditorSchemaOwnerAdoptionId,
    ) -> Result<EditorSchemaOwnerAdoptionId, EditorSchemaOwnerAdoptionError> {
        self.require_schema_adoption_operation(operation)?;
        if !self.accepts_new_work() {
            return Err(EditorSchemaOwnerAdoptionError::Closing);
        }
        if self.schema_owner_adoption.view.phase
            != EditorSchemaOwnerAdoptionPhase::RequiresReconcile
        {
            return Err(EditorSchemaOwnerAdoptionError::Operation);
        }
        if self.schema_owner_adoption.pending() || self.schema_adoption_conflicting_work() {
            return Err(EditorSchemaOwnerAdoptionError::Busy);
        }
        let (directory, receipt) = self
            .schema_owner_adoption
            .installation
            .clone()
            .ok_or(EditorSchemaOwnerAdoptionError::Installation)?;
        let input = self
            .authoring_project()
            .ok_or(EditorSchemaOwnerAdoptionError::NotAuthoring)?
            .schema_owner_input();
        let id = EditorSchemaOwnerAdoptionId(
            NEXT_OPERATION
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                    value.checked_add(1)
                })
                .map_err(|_| EditorSchemaOwnerAdoptionError::Operation)?,
        );
        self.schema_owner_adoption.view = EditorSchemaOwnerAdoptionView {
            operation: Some(id),
            phase: EditorSchemaOwnerAdoptionPhase::Quiescing,
            reconciliation: true,
            ..Default::default()
        };
        self.schema_owner_adoption.start = Some(Start {
            input,
            directory,
            receipt,
            reconcile: true,
        });
        self.schema_owner_adoption.cancelled = false;
        self.schema_owner_adoption.failure = None;
        Ok(id)
    }

    pub fn acknowledge_schema_owner_adoption(
        &mut self,
        operation: EditorSchemaOwnerAdoptionId,
    ) -> Result<(), EditorSchemaOwnerAdoptionError> {
        self.require_schema_adoption_operation(operation)?;
        if self.schema_owner_adoption.view.phase != EditorSchemaOwnerAdoptionPhase::Terminal
            || self.schema_owner_adoption.pending()
        {
            return Err(EditorSchemaOwnerAdoptionError::Operation);
        }
        self.schema_owner_adoption = Default::default();
        Ok(())
    }

    /// Exact current-root diagnostics retained during review; no ready-to-run claim follows.
    pub fn schema_owner_adoption_diagnostics(
        &self,
        document: Option<EditorDocumentId>,
    ) -> Option<&nara_diagnostic::DiagnosticReport> {
        let Some(candidate) = self.schema_owner_adoption.candidate.as_ref() else {
            return match document {
                None => self
                    .schema_owner_adoption
                    .published
                    .as_ref()
                    .map(|published| published.project.startup_compilation_diagnostics())
                    .or_else(|| {
                        self.schema_owner_adoption
                            .attachment_failure
                            .as_ref()
                            .and_then(AttachmentFailure::diagnostics)
                    }),
                Some(document) => self
                    .schema_owner_adoption
                    .captures
                    .iter()
                    .find(|(id, _)| *id == document)
                    .map(|(_, capture)| capture.diagnostics())
                    .or_else(|| {
                        self.schema_owner_adoption
                            .attachment_failure
                            .as_ref()
                            .and_then(|failure| failure.current_diagnostics(document))
                    }),
            };
        };
        match document {
            None => Some(candidate.project.diagnostics()),
            Some(document) => candidate
                .current
                .iter()
                .find(|(id, _)| *id == document)
                .map(|(_, capture)| capture.diagnostics()),
        }
    }

    pub fn schema_owner_adoption_composition_failure(
        &self,
        document: Option<EditorDocumentId>,
    ) -> Option<&nara_scene::PrefabExpansionFailure> {
        let candidate = self.schema_owner_adoption.candidate.as_ref()?;
        match document {
            None => candidate.project.composition_failure(),
            Some(document) => candidate
                .current
                .iter()
                .find(|(id, _)| *id == document)
                .and_then(|(_, capture)| capture.composition_failure()),
        }
    }

    pub fn adopt_schema_owner(
        &mut self,
        operation: EditorSchemaOwnerAdoptionId,
    ) -> Result<(), EditorSchemaOwnerAdoptionError> {
        self.require_schema_adoption_operation(operation)?;
        if self.schema_owner_adoption.view.phase != EditorSchemaOwnerAdoptionPhase::Review {
            return Err(EditorSchemaOwnerAdoptionError::Operation);
        }
        let candidate = self
            .schema_owner_adoption
            .candidate
            .as_ref()
            .ok_or(EditorSchemaOwnerAdoptionError::Operation)?;
        if self.closing || !self.schema_adoption_matches(candidate) {
            return Err(EditorSchemaOwnerAdoptionError::Stale);
        }
        let candidate = self
            .schema_owner_adoption
            .candidate
            .take()
            .expect("review owns its candidate");
        #[cfg(test)]
        let gate = self.schema_owner_adoption.next_publication_gate.take();
        let task = Pending::start(self.authoring_runtime.persistence_tasks(), move || {
            publish(
                candidate,
                #[cfg(test)]
                gate,
            )
        });
        self.schema_owner_adoption.work = Some(Work::Publish(task));
        self.schema_owner_adoption.view.phase = EditorSchemaOwnerAdoptionPhase::Publishing;
        Ok(())
    }

    fn require_schema_adoption_operation(
        &self,
        operation: EditorSchemaOwnerAdoptionId,
    ) -> Result<(), EditorSchemaOwnerAdoptionError> {
        if self.schema_owner_adoption.view.operation == Some(operation) {
            Ok(())
        } else {
            Err(EditorSchemaOwnerAdoptionError::Operation)
        }
    }

    pub(super) fn schema_adoption_fenced(&self) -> bool {
        self.schema_owner_adoption.fenced()
    }

    fn schema_adoption_conflicting_work(&self) -> bool {
        self.persistence.has_pending_work()
            || self.pending_document_open.is_some()
            || self.pending_workspace_intent.is_some()
            || self.authoring_retirements.is_pending()
            || self.image_import.schema_adoption_import_pending()
            || self.source_asset_import.schema_adoption_pending()
            || self.prefab_conversion.schema_adoption_pending()
            || self.source_tools.schema_adoption_pending()
    }

    fn schema_adoption_matches(&self, candidate: &Candidate) -> bool {
        self.authoring_project()
            .is_some_and(|project| candidate.metadata.input.matches(project))
            && candidate.workspace.validate(&self.workspace).is_ok()
            && candidate.sources.iter().all(|source| {
                self.authoring_source_selection(source.document).as_ref() == Some(source)
            })
    }

    pub(super) fn advance_schema_owner_adoption(&mut self) {
        use EditorSchemaOwnerAdoptionPhase as Phase;
        self.schema_owner_adoption.retry_cleanup();
        if self.schema_owner_adoption.view.phase == Phase::Quiescing && !self.closing {
            if self.image_import.schema_adoption_pending() {
                return;
            }
            #[cfg(feature = "presentation-2d")]
            if self.scene_capture.schema_adoption_pending() {
                return;
            }
            if self
                .authoring_project()
                .is_some_and(|project| project.source_index_scan_pending())
            {
                return;
            }
            let Some(start) = self.schema_owner_adoption.start.take() else {
                return;
            };
            let custody = self.authoring_runtime.host_custody().clone();
            self.schema_owner_adoption.work = Some(Work::Inspect(Pending::start(
                self.authoring_runtime.persistence_tasks(),
                move || inspect(start, custody),
            )));
            self.schema_owner_adoption.view.phase = Phase::Inspecting;
        }
        let Some(mut work) = self.schema_owner_adoption.work.take() else {
            return;
        };
        match &mut work {
            Work::Inspect(task) => match task.poll() {
                None => self.schema_owner_adoption.work = Some(work),
                Some(Ok(Ok(Inspected::Prepared(metadata))))
                    if !self.closing && !self.schema_owner_adoption.cancelled =>
                {
                    self.start_schema_source_preparation(metadata)
                }
                Some(Ok(Ok(Inspected::Prepared(metadata)))) => {
                    self.schema_owner_adoption.publication_host = metadata.publisher;
                    self.schema_owner_adoption
                        .finish(EditorSchemaOwnerAdoptionOutcome::Cancelled);
                }
                Some(Ok(Ok(Inspected::AlreadyAdmitted))) => self.schema_owner_adoption.finish(
                    if self.closing || self.schema_owner_adoption.cancelled {
                        EditorSchemaOwnerAdoptionOutcome::Cancelled
                    } else {
                        EditorSchemaOwnerAdoptionOutcome::AlreadyAdmitted
                    },
                ),
                Some(result) => {
                    self.finish_schema_work_return(result, task.rejection, Phase::Inspecting)
                }
            },
            Work::Prepare(task) => {
                match task.poll() {
                    None => self.schema_owner_adoption.work = Some(work),
                    Some(Ok(Ok(candidate)))
                        if !self.closing
                            && !self.schema_owner_adoption.cancelled
                            && self.schema_adoption_matches(&candidate) =>
                    {
                        self.schema_owner_adoption.view.documents = candidate.sources.len();
                        self.schema_owner_adoption.view.degraded_roots = usize::from(matches!(candidate.project.readiness(), crate::project_host::authoring::AuthoringProjectReadiness::Degraded))
                        + candidate.current.iter().filter(|(_, capture)| matches!(capture.readiness(), crate::project_host::authoring::AuthoringProjectReadiness::Degraded)).count();
                        self.schema_owner_adoption.view.owner =
                            Some(Arc::clone(&candidate.metadata.installation.owner));
                        self.schema_owner_adoption.candidate = Some(candidate);
                        self.schema_owner_adoption.view.phase = Phase::Review;
                    }
                    Some(Ok(Ok(_candidate))) => {
                        if self.closing || self.schema_owner_adoption.cancelled {
                            self.schema_owner_adoption
                                .finish(EditorSchemaOwnerAdoptionOutcome::Cancelled);
                        } else {
                            self.finish_schema_work_error(EditorSchemaOwnerAdoptionError::Stale);
                        }
                    }
                    Some(result) => {
                        self.finish_schema_work_return(result, task.rejection, Phase::Preparing)
                    }
                }
            }
            Work::Publish(task) => match task.poll() {
                None => self.schema_owner_adoption.work = Some(work),
                Some(Ok(Ok(published))) => self.accept_schema_publication(published),
                Some(Err(WorkError::Unobserved)) => {
                    self.record_schema_failure(Failure::new(
                        EditorSchemaOwnerAdoptionError::RequiresReconcile,
                        Phase::Publishing,
                    ));
                    self.schema_owner_adoption
                        .finish(EditorSchemaOwnerAdoptionOutcome::Uncertain);
                }
                Some(result) => {
                    self.finish_schema_work_return(result, task.rejection, Phase::Publishing)
                }
            },
        }
    }

    fn finish_schema_work_error(&mut self, error: EditorSchemaOwnerAdoptionError) {
        self.finish_schema_failure(Failure::new(error, self.schema_owner_adoption.view.phase));
    }

    fn finish_schema_work_return<T>(
        &mut self,
        result: Result<Result<T, Failure>, WorkError>,
        rejection: Option<nara_tasks::TaskRejection>,
        phase: EditorSchemaOwnerAdoptionPhase,
    ) {
        match work_error(result, rejection, phase) {
            Some(failure) => self.finish_schema_failure(failure),
            None => self
                .schema_owner_adoption
                .finish(EditorSchemaOwnerAdoptionOutcome::Cancelled),
        }
    }

    fn record_schema_failure(&mut self, mut failure: Failure) {
        failure.bind_operation(self.schema_owner_adoption.view.operation);
        self.schema_owner_adoption.failure = Some(failure);
    }

    fn finish_schema_failure(&mut self, failure: Failure) {
        let reason = failure.reason();
        self.record_schema_failure(failure);
        self.schema_owner_adoption.finish(
            if self.closing || self.schema_owner_adoption.cancelled {
                EditorSchemaOwnerAdoptionOutcome::Cancelled
            } else {
                EditorSchemaOwnerAdoptionOutcome::Failed(reason)
            },
        );
    }

    fn start_schema_source_preparation(&mut self, mut metadata: Metadata) {
        self.schema_owner_adoption.publication_host = metadata.publisher.clone();
        let result = (|| {
            let count = self.workspace.len();
            let buffers = count
                .checked_mul(
                    std::mem::size_of::<EditorDocumentId>()
                        + std::mem::size_of::<AuthoringPrefabSessionSnapshot>()
                        + std::mem::size_of::<(EditorDocumentId, PreparedSchemaOwnerSceneCapture)>(
                        )
                        + std::mem::size_of::<(EditorDocumentId, AuthoringDocumentCapture)>(),
                )
                .ok_or_else(|| {
                    Failure::new(
                        EditorSchemaOwnerAdoptionError::Preparation,
                        EditorSchemaOwnerAdoptionPhase::Preparing,
                    )
                })?;
            metadata
                .owner
                .retain_operation_inputs(buffers)
                .map_err(|_| {
                    Failure::new(
                        EditorSchemaOwnerAdoptionError::Preparation,
                        EditorSchemaOwnerAdoptionPhase::Preparing,
                    )
                })?;
            let documents = self.workspace.__document_ids();
            let selection_bytes = documents
                .iter()
                .try_fold(0_usize, |total, &id| {
                    total.checked_add(self.authoring_source_selection_retained_bytes(id)?)
                })
                .ok_or_else(|| {
                    Failure::new(
                        EditorSchemaOwnerAdoptionError::Stale,
                        EditorSchemaOwnerAdoptionPhase::Preparing,
                    )
                })?;
            metadata
                .owner
                .retain_operation_inputs(selection_bytes)
                .map_err(|_| {
                    Failure::new(
                        EditorSchemaOwnerAdoptionError::Preparation,
                        EditorSchemaOwnerAdoptionPhase::Preparing,
                    )
                })?;
            let sources = documents
                .into_iter()
                .map(|id| {
                    self.authoring_source_selection(id)
                        .filter(|source| source.receipt.is_some())
                        .ok_or_else(|| {
                            Failure::new(
                                EditorSchemaOwnerAdoptionError::Stale,
                                EditorSchemaOwnerAdoptionPhase::Preparing,
                            )
                        })
                })
                .collect::<Result<Vec<_>, _>>()?;
            let workspace = self
                .workspace
                .capture_schema_reclassification(
                    metadata.input.schema_basis(),
                    metadata.owner.schema_basis(),
                    Default::default(),
                )
                .map_err(|report| {
                    Failure::workspace(report, EditorSchemaOwnerAdoptionPhase::Preparing)
                })?;
            Ok((sources, workspace))
        })();
        match result {
            Ok((sources, workspace)) => {
                self.schema_owner_adoption.work = Some(Work::Prepare(Pending::start(
                    self.authoring_runtime.persistence_tasks(),
                    move || prepare(metadata, sources, workspace),
                )));
                self.schema_owner_adoption.view.phase = EditorSchemaOwnerAdoptionPhase::Preparing;
            }
            Err(error) => {
                self.finish_schema_failure(error);
            }
        }
    }

    fn accept_schema_publication(&mut self, published: Published) {
        let Published {
            mut candidate,
            outcome,
        } = published;
        let live = !self.closing
            && !self.schema_owner_adoption.cancelled
            && self.schema_adoption_matches(&candidate);
        match outcome {
            Publication::Written(FilePublicationOutcome::Published(accepted)) => {
                let admitted = match candidate.metadata.owner.attach(accepted) {
                    Ok(admitted) => admitted,
                    Err(failure) => {
                        self.schema_owner_adoption.attachment_failure =
                            Some(AttachmentFailure::Metadata(failure));
                        self.schema_owner_adoption
                            .finish(EditorSchemaOwnerAdoptionOutcome::PublishedWithoutAdoption);
                        return;
                    }
                };
                let mut captures = Vec::with_capacity(candidate.current.len());
                let mut current = std::mem::take(&mut candidate.current).into_iter();
                while let Some((document, capture)) = current.next() {
                    match capture.attach(&admitted) {
                        Ok(capture) => captures.push((document, capture)),
                        Err(failed) => {
                            self.schema_owner_adoption.attachment_failure =
                                Some(AttachmentFailure::Current(CurrentAttachmentFailure {
                                    owner: admitted,
                                    project: candidate.project,
                                    failed: (document, failed),
                                    attached: captures,
                                    remaining: current,
                                }));
                            self.schema_owner_adoption
                                .finish(EditorSchemaOwnerAdoptionOutcome::PublishedWithoutAdoption);
                            return;
                        }
                    }
                }
                let (receipt, evidence) = match candidate.project.attach(admitted) {
                    Ok(result) => result,
                    Err(failure) => {
                        self.schema_owner_adoption.attachment_failure =
                            Some(AttachmentFailure::Project(failure));
                        self.schema_owner_adoption
                            .finish(EditorSchemaOwnerAdoptionOutcome::PublishedWithoutAdoption);
                        return;
                    }
                };
                let project = match receipt.into_session_snapshot() {
                    Ok(project) => project,
                    Err(_) => {
                        self.schema_owner_adoption.evidence = Some(evidence);
                        self.schema_owner_adoption
                            .finish(EditorSchemaOwnerAdoptionOutcome::PublishedWithoutAdoption);
                        return;
                    }
                };
                self.schema_owner_adoption.captures = captures;
                if live {
                    let report = match self
                        .workspace
                        .bind_schema_reclassification(candidate.workspace)
                    {
                        Ok(guard) => {
                            guard.commit();
                            self.execution = EditorExecutionLane::Authoring(project);
                            self.adopt_schema_project_index();
                            self.invalidate_schema_adoption_preview();
                            self.schema_owner_adoption.published = None;
                            self.schema_owner_adoption.evidence = Some(evidence);
                            self.schema_owner_adoption
                                .finish(EditorSchemaOwnerAdoptionOutcome::Adopted);
                            self.publish_editor_frame(true);
                            return;
                        }
                        Err(report) => report,
                    };
                    self.record_schema_failure(Failure::workspace(
                        report,
                        EditorSchemaOwnerAdoptionPhase::Publishing,
                    ));
                }
                self.schema_owner_adoption.published = Some(PublishedProject { project, evidence });
                self.schema_owner_adoption
                    .finish(EditorSchemaOwnerAdoptionOutcome::PublishedWithoutAdoption);
            }
            Publication::Observed => self.accept_observed_schema_owner(candidate, live),
            Publication::Written(FilePublicationOutcome::Uncertain(evidence)) => {
                self.schema_owner_adoption.evidence = Some(evidence);
                self.schema_owner_adoption
                    .finish(EditorSchemaOwnerAdoptionOutcome::Uncertain);
            }
            Publication::Written(FilePublicationOutcome::Rejected(reason)) => {
                self.finish_schema_failure(Failure::publication_rejected(reason))
            }
            Publication::Written(FilePublicationOutcome::Failed(stage)) => {
                self.finish_schema_failure(Failure::publication_failed(stage))
            }
        }
    }

    fn accept_observed_schema_owner(&mut self, mut candidate: Candidate, live: bool) {
        let observed = match candidate.metadata.owner.attach_observed() {
            Ok(observed) => observed,
            Err(_) => {
                self.finish_schema_work_error(EditorSchemaOwnerAdoptionError::Preparation);
                return;
            }
        };
        let mut captures = Vec::with_capacity(candidate.current.len());
        for (document, capture) in candidate.current.drain(..) {
            match capture.attach_observed(&observed) {
                Ok(capture) => captures.push((document, capture)),
                Err(_) => {
                    self.finish_schema_work_error(EditorSchemaOwnerAdoptionError::Preparation);
                    return;
                }
            }
        }
        let receipt = match candidate.project.attach_observed(observed) {
            Ok(receipt) => receipt,
            Err(failure) => {
                self.schema_owner_adoption.attachment_failure =
                    Some(AttachmentFailure::ObservedProject(failure));
                self.finish_schema_work_error(EditorSchemaOwnerAdoptionError::Preparation);
                return;
            }
        };
        let project = match receipt.into_session_snapshot() {
            Ok(project) => project,
            Err(_) => {
                self.finish_schema_work_error(EditorSchemaOwnerAdoptionError::Preparation);
                return;
            }
        };
        if !live {
            if self.closing || self.schema_owner_adoption.cancelled {
                self.schema_owner_adoption
                    .finish(EditorSchemaOwnerAdoptionOutcome::Cancelled);
            } else {
                self.finish_schema_work_error(EditorSchemaOwnerAdoptionError::Stale);
            }
            return;
        }
        let failure = match self
            .workspace
            .bind_schema_reclassification(candidate.workspace)
        {
            Ok(guard) => {
                guard.commit();
                None
            }
            Err(report) => Some(report),
        };
        if let Some(report) = failure {
            self.finish_schema_failure(Failure::workspace(
                report,
                EditorSchemaOwnerAdoptionPhase::Publishing,
            ));
            return;
        }
        self.execution = EditorExecutionLane::Authoring(project);
        self.adopt_schema_project_index();
        self.schema_owner_adoption.captures = captures;
        if let Some(published) = self.schema_owner_adoption.published.take() {
            self.schema_owner_adoption.evidence = Some(published.evidence);
        }
        self.invalidate_schema_adoption_preview();
        self.schema_owner_adoption
            .finish(EditorSchemaOwnerAdoptionOutcome::Adopted);
        self.publish_editor_frame(true);
    }

    fn invalidate_schema_adoption_preview(&mut self) {
        #[cfg(feature = "presentation-2d")]
        {
            self.authoring_runtime.invalidate_preview_content();
            self.scene_capture.invalidate();
        }
        #[cfg(feature = "runtime-ui")]
        {
            self.ui_authoring_frame = None;
            self.ui_authoring_projection_cache = None;
        }
        #[cfg(feature = "presentation-ui")]
        self.ui_preview.retire();
    }

    fn adopt_schema_project_index(&mut self) {
        if let EditorExecutionLane::Authoring(project) = &mut self.execution {
            self.image_import.adopt_schema_project_index(project);
        }
    }

    pub fn schema_owner_adoption_publication(&self) -> Option<&ReplaceReceipt> {
        self.schema_owner_adoption
            .published
            .as_ref()
            .map(|value| value.evidence.receipt())
            .or_else(|| {
                self.schema_owner_adoption
                    .attachment_failure
                    .as_ref()
                    .and_then(AttachmentFailure::evidence)
                    .map(FilePublicationEvidence::receipt)
            })
            .or_else(|| {
                self.schema_owner_adoption
                    .evidence
                    .as_ref()
                    .map(FilePublicationEvidence::receipt)
            })
    }
}

fn inspect(start: Start, custody: HostCustody) -> Result<Inspected, Failure> {
    let phase = EditorSchemaOwnerAdoptionPhase::Inspecting;
    let directory = start
        .input
        .project_root()
        .strict_read_only()
        .map_err(|_| {
            Failure::new(EditorSchemaOwnerAdoptionError::Installation, phase)
                .at_source(Some(&start.directory))
        })?
        .open_directory_exact(&RelativePath::new(start.directory.as_ref()).map_err(|_| {
            Failure::new(EditorSchemaOwnerAdoptionError::Installation, phase)
                .at_source(Some(&start.directory))
        })?)
        .map_err(|_| {
            Failure::new(EditorSchemaOwnerAdoptionError::Installation, phase)
                .at_source(Some(&start.directory))
        })?;
    let inspection = EditorSourceToolInstallationInspection::read(&directory, start.receipt)
        .map_err(|_| {
            Failure::new(EditorSchemaOwnerAdoptionError::Installation, phase)
                .at_source(Some(&start.directory))
        })?;
    let installation = Installation {
        directory: start.directory,
        receipt: start.receipt,
        identity: directory.identity(),
        manifest: inspection.receipt().manifest_digest(),
        package: Arc::from(inspection.release().package()),
        owner: Arc::from(inspection.release().schema_owner().owner_id().as_str()),
    };
    let preparation = if start.reconcile {
        match start
            .input
            .prepare_observed_owner(inspection.release().schema_owner())
            .map_err(|error| Failure::metadata(error, phase))?
        {
            AuthoringSchemaOwnerReconciliation::Observed(owner) => {
                return Ok(Inspected::Prepared(Metadata {
                    input: start.input,
                    installation,
                    owner,
                    publisher: None,
                }));
            }
            AuthoringSchemaOwnerReconciliation::Publication(preparation) => preparation,
        }
    } else {
        start
            .input
            .prepare_owner(inspection.release().schema_owner())
            .map_err(|error| Failure::metadata(error, phase))?
    };
    match preparation {
        AuthoringSchemaOwnerPreparation::AlreadyAdmitted => Ok(Inspected::AlreadyAdmitted),
        AuthoringSchemaOwnerPreparation::Prepared { owner, publication } => {
            let publisher = custody
                .retain_publication(publication)
                .ok_or_else(|| Failure::new(EditorSchemaOwnerAdoptionError::Closing, phase))?;
            Ok(Inspected::Prepared(Metadata {
                input: start.input,
                installation,
                owner,
                publisher: Some(publisher),
            }))
        }
    }
}

fn prepare(
    mut metadata: Metadata,
    sources: Vec<AuthoringSourceSelection>,
    request: EditorWorkspaceSchemaReclassificationRequest,
) -> Result<Candidate, Failure> {
    let phase = EditorSchemaOwnerAdoptionPhase::Preparing;
    let workspace = request
        .prepare()
        .map_err(|report| Failure::workspace(report, phase))?;
    let mut prefabs = Vec::with_capacity(sources.len());
    for source in &sources {
        if let Some(view) = workspace.prefab_snapshot(source.document) {
            prefabs.push(
                AuthoringPrefabSessionSnapshot::capture_prepared(
                    source.receipt.as_ref().ok_or_else(|| {
                        Failure::new(EditorSchemaOwnerAdoptionError::Stale, phase)
                    })?,
                    view,
                )
                .map_err(|error| {
                    Failure::source(
                        error,
                        EditorSchemaOwnerAdoptionError::Preparation,
                        phase,
                        Some(source.document),
                    )
                })?,
            );
        }
    }
    let project = metadata
        .owner
        .prepare_project_input(&metadata.input, &prefabs)
        .map_err(|error| {
            Failure::source(
                error,
                EditorSchemaOwnerAdoptionError::Preparation,
                phase,
                None,
            )
        })?;
    let mut current = Vec::with_capacity(sources.len());
    for source in &sources {
        if let Some(view) = workspace.scene_snapshot(source.document) {
            let capture = metadata
                .owner
                .capture_scene(
                    source.receipt.as_ref().ok_or_else(|| {
                        Failure::new(EditorSchemaOwnerAdoptionError::Stale, phase)
                    })?,
                    view,
                    &prefabs,
                )
                .map_err(|error| {
                    Failure::source(
                        error,
                        EditorSchemaOwnerAdoptionError::Preparation,
                        phase,
                        Some(source.document),
                    )
                })?;
            current.push((source.document, capture));
        }
    }
    Ok(Candidate {
        metadata,
        workspace,
        project,
        current,
        sources,
    })
}

fn publish(
    mut candidate: Candidate,
    #[cfg(test)] gate: Option<Box<dyn FnOnce() + Send>>,
) -> Result<Published, Failure> {
    let phase = EditorSchemaOwnerAdoptionPhase::Publishing;
    let installation = &candidate.metadata.installation;
    let directory = candidate
        .metadata
        .input
        .project_root()
        .strict_read_only()
        .map_err(|_| {
            Failure::new(EditorSchemaOwnerAdoptionError::Installation, phase)
                .at_source(Some(&installation.directory))
        })?
        .open_directory_exact(&RelativePath::new(installation.directory.as_ref()).map_err(
            |_| {
                Failure::new(EditorSchemaOwnerAdoptionError::Installation, phase)
                    .at_source(Some(&installation.directory))
            },
        )?)
        .map_err(|_| {
            Failure::new(EditorSchemaOwnerAdoptionError::Installation, phase)
                .at_source(Some(&installation.directory))
        })?;
    if directory.identity() != installation.identity {
        return Err(
            Failure::new(EditorSchemaOwnerAdoptionError::Installation, phase)
                .at_source(Some(&installation.directory)),
        );
    }
    let inspection = EditorSourceToolInstallationInspection::read(&directory, installation.receipt)
        .map_err(|_| {
            Failure::new(EditorSchemaOwnerAdoptionError::Installation, phase)
                .at_source(Some(&installation.directory))
        })?;
    if inspection.receipt().manifest_digest() != installation.manifest
        || inspection.release().package() != installation.package.as_ref()
    {
        return Err(
            Failure::new(EditorSchemaOwnerAdoptionError::Installation, phase)
                .at_source(Some(&installation.directory)),
        );
    }
    candidate
        .metadata
        .owner
        .validate()
        .map_err(|error| Failure::metadata(error, phase))?;
    candidate
        .project
        .validate(&mut candidate.metadata.owner)
        .map_err(|error| {
            Failure::source(error, EditorSchemaOwnerAdoptionError::Stale, phase, None)
        })?;
    for (document, capture) in &candidate.current {
        capture
            .validate(&mut candidate.metadata.owner)
            .map_err(|error| {
                Failure::source(
                    error,
                    EditorSchemaOwnerAdoptionError::Stale,
                    phase,
                    Some(*document),
                )
            })?;
    }
    let outcome = match &candidate.metadata.publisher {
        Some(publisher) => {
            let bytes = candidate.metadata.owner.bytes();
            Publication::Written(
                publisher
                    .with_worker(|host| host.publish(&bytes))
                    .ok_or_else(|| Failure::new(EditorSchemaOwnerAdoptionError::Closing, phase))?,
            )
        }
        None => Publication::Observed,
    };
    #[cfg(test)]
    if let Some(gate) = gate {
        gate();
    }
    Ok(Published { candidate, outcome })
}

fn work_error<T>(
    result: Result<Result<T, Failure>, WorkError>,
    rejection: Option<nara_tasks::TaskRejection>,
    phase: EditorSchemaOwnerAdoptionPhase,
) -> Option<Failure> {
    match result {
        Ok(Err(error)) => Some(error),
        Err(WorkError::CancelledBeforeStart) => None,
        Err(WorkError::Unavailable) => Some(match rejection {
            Some(rejection) => Failure::rejected(rejection, phase),
            None => Failure::new(EditorSchemaOwnerAdoptionError::Task, phase),
        }),
        Err(WorkError::Unobserved) => Some(Failure::new(
            EditorSchemaOwnerAdoptionError::RequiresReconcile,
            phase,
        )),
        Ok(Ok(_)) => Some(Failure::new(
            EditorSchemaOwnerAdoptionError::Operation,
            phase,
        )),
    }
}

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;
#[cfg(test)]
mod work_tests;
