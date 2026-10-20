use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

use nara::{
    fs::ContentDigest,
    project::DESKTOP_SUPERVISED_PLAY_V1_CONTRACT,
    project_host::{
        SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION, SupervisedDesktopSourceAuthority,
        admit_supervised_desktop_source_receipt,
    },
    tooling::{
        EditorExternalPlayObservation, EditorExternalPlayRequest, EditorExternalPlayState,
        EditorPlayFailure, EditorPlayOperation, EditorPlayOperationResult,
    },
};

use super::{
    build::{
        DesktopBuildAttempt, DesktopBuildInputRevision, DesktopBuildRequest, DesktopBuildStatus,
        DesktopBuildTools, VerifiedDesktopBuildCandidate,
    },
    failure::HostFailureFacts,
    generation::{
        ExecutableGenerationId, ExecutableGenerationStore, GenerationBuildObservation,
        GenerationManifestInput, GenerationPackageFacts, GenerationSourceFacts,
        GenerationTargetFacts, ImmutableExecutableGeneration,
    },
    handshake::SupervisedDesktopHandshakeFailureKind,
    output_projection::EditorExternalPlayOutputProjection,
    preparation::{GenerationPreparationOwner, GenerationPreparationRequest},
    process::{SupervisedProcessError, SupervisedProcessSpawnFailure},
    supervisor::{
        SupervisedDesktopAttemptId, SupervisedDesktopLaunchRequest,
        SupervisedDesktopSessionSupervisor, SupervisedDesktopSupervisorAdvanceError,
        SupervisedDesktopSupervisorFailure, SupervisedDesktopSupervisorStartError,
        SupervisedDesktopSupervisorStatus, SupervisedDesktopSupervisorTerminal,
    },
    wake::SupervisedPlayWake,
};

const READY_TIMEOUT: Duration = Duration::from_secs(10);

struct PendingRequest {
    request: EditorExternalPlayRequest,
    operation: EditorPlayOperation,
    project_root: PathBuf,
    source: Option<SupervisedDesktopSourceAuthority>,
    source_receipt_digest: Option<ContentDigest>,
    phase: PendingPhase,
    completion: Option<PendingCompletion>,
    ready_reported: bool,
}

#[derive(Clone, Copy)]
struct PendingCompletion {
    state: EditorExternalPlayState,
    result: Option<EditorPlayOperationResult>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingPhase {
    Building,
    StoppingBuild,
    PreparingGeneration,
    StoppingPreparation,
    Starting,
    Running,
    StoppingSupervisor,
    StoppingGeneration,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CleanupRequestProgress {
    InProgress,
    Complete,
    Incomplete,
    NoOwner,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum SupervisorTerminalKind {
    Stopped,
    Exited,
    Failed { failure: EditorPlayFailure },
}

/// Owns all process, build, generation, and cleanup capabilities for one published Editor.
///
/// The Host never receives this value. It exchanges only `EditorExternalPlayRequest` and
/// `EditorExternalPlayObservation` values with this owner through the product shell.
pub(crate) struct SupervisedDesktopPlayOwner {
    store: Option<ExecutableGenerationStore>,
    tools: Option<DesktopBuildTools>,
    build: Option<DesktopBuildAttempt>,
    preparation: Option<GenerationPreparationOwner>,
    supervisor: Option<SupervisedDesktopSessionSupervisor>,
    supervisor_advance_failure_reported: bool,
    pending: Option<PendingRequest>,
    completed: Option<EditorExternalPlayObservation>,
    last_observation: Option<EditorExternalPlayObservation>,
    output: EditorExternalPlayOutputProjection,
    wake: SupervisedPlayWake,
    last_attempt: u64,
    active_generation_label: Option<u64>,
    active_source_receipt_digest: Option<ContentDigest>,
    close_requested: bool,
    close_cleanup_incomplete: bool,
    close_complete: bool,
}

impl SupervisedDesktopPlayOwner {
    #[cfg(test)]
    pub(crate) fn new() -> Self {
        Self::with_wake(SupervisedPlayWake::default())
    }

    pub(crate) fn with_wake(wake: SupervisedPlayWake) -> Self {
        Self {
            store: None,
            tools: None,
            build: None,
            preparation: None,
            supervisor: None,
            supervisor_advance_failure_reported: false,
            pending: None,
            completed: None,
            last_observation: None,
            output: EditorExternalPlayOutputProjection::new(),
            wake,
            last_attempt: 1,
            active_generation_label: None,
            active_source_receipt_digest: None,
            close_requested: false,
            close_cleanup_incomplete: false,
            close_complete: false,
        }
    }

    pub(crate) fn set_wake(&mut self, wake: SupervisedPlayWake) {
        self.wake = wake;
    }

    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        if self.close_retry_required() || self.close_complete {
            return None;
        }
        self.build
            .as_ref()
            .and_then(DesktopBuildAttempt::next_deadline)
            .into_iter()
            .chain(
                self.supervisor
                    .as_ref()
                    .and_then(SupervisedDesktopSessionSupervisor::next_deadline),
            )
            .chain(
                self.preparation
                    .as_ref()
                    .map(GenerationPreparationOwner::next_deadline),
            )
            .min()
    }

    pub(crate) fn submit(
        &mut self,
        request: EditorExternalPlayRequest,
        project_root: PathBuf,
        now: Instant,
    ) {
        self.last_attempt = request.attempt();
        self.completed = None;
        if self.close_requested {
            if request.operation() == EditorPlayOperation::RetryClose {
                self.retry_close(now);
                return;
            }
            self.complete_failure(
                &request,
                None,
                EditorPlayFailure::Close,
                EditorExternalPlayState::CleanupIncomplete,
            );
            return;
        }
        if matches!(
            request.operation(),
            EditorPlayOperation::Play
                | EditorPlayOperation::Restart
                | EditorPlayOperation::RunLastGood
        ) {
            self.output.reset_attempt();
        }
        if self.pending.is_some() {
            if matches!(
                request.operation(),
                EditorPlayOperation::Stop
                    | EditorPlayOperation::Cancel
                    | EditorPlayOperation::Restart
                    | EditorPlayOperation::RetryRetirement
                    | EditorPlayOperation::RetryClose
            ) {
                self.replace_pending_request(request, now);
                return;
            }
            self.complete_failure(
                &request,
                request.source_receipt().map(ContentDigest::of_bytes),
                EditorPlayFailure::Start,
                EditorExternalPlayState::Faulted,
            );
            return;
        }

        let source = match request.operation() {
            EditorPlayOperation::Play | EditorPlayOperation::RunLastGood => {
                let Some(encoded) = request.source_receipt() else {
                    self.output
                        .observe_host_failure("source", "missing-receipt", None);
                    self.complete_failure(
                        &request,
                        None,
                        EditorPlayFailure::Start,
                        EditorExternalPlayState::Faulted,
                    );
                    return;
                };
                match admit_supervised_desktop_source_receipt(encoded) {
                    Ok(source) => Some(source),
                    Err(error) => {
                        self.output.observe_host_failure(
                            "source",
                            "receipt-admission",
                            Some(&error),
                        );
                        self.complete_failure(
                            &request,
                            Some(ContentDigest::of_bytes(encoded)),
                            EditorPlayFailure::Start,
                            EditorExternalPlayState::Faulted,
                        );
                        return;
                    }
                }
            }
            EditorPlayOperation::Stop
            | EditorPlayOperation::Cancel
            | EditorPlayOperation::Restart
            | EditorPlayOperation::RetryRetirement
            | EditorPlayOperation::RetryClose
            | EditorPlayOperation::Pause
            | EditorPlayOperation::Resume
            | EditorPlayOperation::StepFixedTick => None,
        };
        let source_receipt_digest = request.source_receipt().map(ContentDigest::of_bytes);
        let operation = request.operation();
        let mut pending = PendingRequest {
            request,
            operation,
            project_root,
            source,
            source_receipt_digest,
            phase: PendingPhase::Starting,
            completion: None,
            ready_reported: false,
        };

        match pending.request.operation() {
            EditorPlayOperation::Stop
            | EditorPlayOperation::Cancel
            | EditorPlayOperation::RetryRetirement
            | EditorPlayOperation::RetryClose => {
                self.submit_cleanup_request(pending, now);
                return;
            }
            EditorPlayOperation::Play
            | EditorPlayOperation::Restart
            | EditorPlayOperation::RunLastGood => {
                if self.supervisor.is_some() {
                    self.submit_cleanup_request(pending, now);
                    return;
                } else if let Err(failure) = self.begin_start(&mut pending, now) {
                    if failure == EditorPlayFailure::Retirement {
                        self.complete_cleanup_incomplete(pending);
                        return;
                    }
                    self.complete_failure(
                        &pending.request,
                        pending.source_receipt_digest,
                        failure,
                        EditorExternalPlayState::Faulted,
                    );
                    return;
                }
            }
            EditorPlayOperation::Pause
            | EditorPlayOperation::Resume
            | EditorPlayOperation::StepFixedTick => {
                self.complete_failure(
                    &pending.request,
                    pending.source_receipt_digest,
                    EditorPlayFailure::Runtime,
                    EditorExternalPlayState::Faulted,
                );
                return;
            }
        }
        self.pending = Some(pending);
    }

    pub(crate) fn advance(&mut self, now: Instant) -> Option<EditorExternalPlayObservation> {
        let observation = self.advance_inner(now)?;
        let unchanged = self.last_observation.as_ref().is_some_and(|previous| {
            previous.attempt() == observation.attempt()
                && previous.state() == observation.state()
                && previous.generation() == observation.generation()
                && previous.last_good_available() == observation.last_good_available()
                && previous.source_receipt_digest() == observation.source_receipt_digest()
                && previous.output() == observation.output()
                && observation
                    .result()
                    .is_none_or(|result| previous.result() == Some(result))
        });
        if unchanged {
            return None;
        }
        self.last_observation = Some(observation.clone());
        Some(observation)
    }

    fn advance_inner(&mut self, now: Instant) -> Option<EditorExternalPlayObservation> {
        if let Some(observation) = self.completed.take() {
            return Some(observation);
        }

        if self.preparation.is_some() {
            return self.advance_preparation(now);
        }

        if self.build.is_some() {
            return self.advance_build(now);
        }
        if self.supervisor.is_some() {
            return self.advance_supervisor(now);
        }
        self.try_finish_close();
        self.completed.take()
    }

    pub(crate) fn request_close(&mut self, now: Instant) {
        if self.close_requested {
            self.try_finish_close();
            return;
        }
        self.close_requested = true;
        self.close_cleanup_incomplete = false;
        self.completed = None;
        self.output.clear();
        if let Some(preparation) = self.preparation.as_ref() {
            preparation.request_cancel();
        }
        if let Some(build) = self.build.as_mut() {
            if build.request_cancel(now).is_err() {
                self.close_cleanup_incomplete = true;
                self.publish_close_incomplete();
            }
        }
        if let Some(supervisor) = self.supervisor.as_mut() {
            let attempt = supervisor.attempt();
            if supervisor.request_close(attempt, now).is_err() {
                self.close_cleanup_incomplete = true;
                self.publish_close_incomplete();
            }
        }
        if self.build.is_none() && self.supervisor.is_none() && self.preparation.is_none() {
            self.pending = None;
        }
        self.try_finish_close();
    }

    #[must_use]
    pub(crate) const fn close_complete(&self) -> bool {
        self.close_complete
    }

    #[must_use]
    pub(crate) const fn close_retry_required(&self) -> bool {
        self.close_requested && self.close_cleanup_incomplete && !self.close_complete
    }

    fn retry_close(&mut self, now: Instant) {
        self.close_cleanup_incomplete = false;
        if let Some(preparation) = self.preparation.as_ref() {
            preparation.request_cancel();
        }
        if let Some(build) = self.build.as_mut()
            && build.request_cancel(now).is_err()
        {
            self.close_cleanup_incomplete = true;
        }
        if let Some(supervisor) = self.supervisor.as_mut() {
            let attempt = supervisor.attempt();
            if supervisor.retry_cleanup(attempt, now).is_err() {
                self.close_cleanup_incomplete = true;
            }
        }
        self.try_finish_close();
        if self.close_cleanup_incomplete {
            self.publish_close_incomplete();
        }
    }

    fn replace_pending_request(&mut self, request: EditorExternalPlayRequest, now: Instant) {
        let previous = self
            .pending
            .take()
            .expect("pending replacement requires an existing request");
        let requested_operation = request.operation();
        let retry = matches!(
            requested_operation,
            EditorPlayOperation::RetryRetirement | EditorPlayOperation::RetryClose
        );
        let operation = if retry {
            previous.operation
        } else {
            requested_operation
        };
        let source = if retry || requested_operation == EditorPlayOperation::Restart {
            previous.source
        } else {
            None
        };
        let pending = PendingRequest {
            request,
            operation,
            project_root: previous.project_root,
            source,
            source_receipt_digest: previous
                .source_receipt_digest
                .or(self.active_source_receipt_digest),
            phase: previous.phase,
            completion: if retry { previous.completion } else { None },
            ready_reported: false,
        };
        self.submit_cleanup_request(pending, now);
    }

    fn submit_cleanup_request(&mut self, mut pending: PendingRequest, now: Instant) {
        match self.begin_stop(&mut pending, now) {
            CleanupRequestProgress::InProgress => self.pending = Some(pending),
            CleanupRequestProgress::Complete => {
                self.completed = Some(self.resume_after_retirement(pending, now));
            }
            CleanupRequestProgress::Incomplete => self.complete_cleanup_incomplete(pending),
            CleanupRequestProgress::NoOwner
                if pending.completion.is_some()
                    || matches!(
                        pending.operation,
                        EditorPlayOperation::Stop | EditorPlayOperation::Cancel
                    ) =>
            {
                self.completed = Some(self.resume_after_retirement(pending, now));
            }
            CleanupRequestProgress::NoOwner => {
                self.complete_failure(
                    &pending.request,
                    pending.source_receipt_digest,
                    EditorPlayFailure::Retirement,
                    EditorExternalPlayState::Faulted,
                );
            }
        }
    }

    fn begin_stop(&mut self, pending: &mut PendingRequest, now: Instant) -> CleanupRequestProgress {
        if let Some(preparation) = self.preparation.as_ref() {
            preparation.request_cancel();
            pending.phase = PendingPhase::StoppingPreparation;
            return CleanupRequestProgress::InProgress;
        }
        if let Some(supervisor) = self.supervisor.as_mut() {
            let attempt = supervisor.attempt();
            let result = if matches!(
                supervisor.status(),
                SupervisedDesktopSupervisorStatus::CleanupIncomplete
            ) {
                supervisor.retry_cleanup(attempt, now)
            } else {
                supervisor.request_stop(attempt, now)
            };
            pending.phase = PendingPhase::StoppingSupervisor;
            return if result.is_ok() {
                CleanupRequestProgress::InProgress
            } else {
                CleanupRequestProgress::Incomplete
            };
        }
        if let Some(build) = self.build.as_mut() {
            pending.phase = PendingPhase::StoppingBuild;
            return if build.request_cancel(now).is_ok() {
                CleanupRequestProgress::InProgress
            } else {
                CleanupRequestProgress::Incomplete
            };
        }
        if self.active_generation_id().is_some()
            || self.active_generation_label.is_some()
            || self.active_source_receipt_digest.is_some()
            || self
                .store
                .as_ref()
                .is_some_and(ExecutableGenerationStore::cleanup_incomplete)
        {
            pending.phase = PendingPhase::StoppingGeneration;
            return if self.finish_active_launch().is_ok() {
                self.clear_active_generation_tracking();
                CleanupRequestProgress::Complete
            } else {
                CleanupRequestProgress::Incomplete
            };
        }
        CleanupRequestProgress::NoOwner
    }

    fn begin_start(
        &mut self,
        pending: &mut PendingRequest,
        now: Instant,
    ) -> Result<(), EditorPlayFailure> {
        if matches!(
            pending.operation,
            EditorPlayOperation::Restart | EditorPlayOperation::RunLastGood
        ) {
            self.activate_last_good(pending, now)
        } else {
            self.start_build(pending, now)
        }
    }

    fn start_build(
        &mut self,
        pending: &mut PendingRequest,
        now: Instant,
    ) -> Result<(), EditorPlayFailure> {
        let source = pending.source.as_ref().ok_or(EditorPlayFailure::Start)?;
        let expected_revision =
            DesktopBuildInputRevision::from_source(source).ok_or(EditorPlayFailure::Preparation)?;
        let tools = match self.tools.clone() {
            Some(tools) => tools,
            None => {
                let tools =
                    DesktopBuildTools::discover().map_err(|_| EditorPlayFailure::Preparation)?;
                self.tools = Some(tools.clone());
                tools
            }
        };
        let request =
            DesktopBuildRequest::new(pending.project_root.clone(), expected_revision, tools)
                .map_err(|_| EditorPlayFailure::Preparation)?;
        pending.project_root = request.project_root().to_path_buf();
        self.build = Some(
            DesktopBuildAttempt::start(request, now, self.wake.clone())
                .map_err(|_| EditorPlayFailure::Preparation)?,
        );
        pending.phase = PendingPhase::Building;
        Ok(())
    }

    fn activate_last_good(
        &mut self,
        pending: &mut PendingRequest,
        _now: Instant,
    ) -> Result<(), EditorPlayFailure> {
        let source = pending.source.as_ref().ok_or(EditorPlayFailure::Start)?;
        self.start_preparation(
            GenerationPreparationRequest::LastGood {
                source_receipt: source.canonical_source_receipt().to_vec(),
            },
            pending,
        )
    }

    fn complete_cleanup_incomplete(&mut self, pending: PendingRequest) {
        let observation = self.observation(
            pending.request.attempt(),
            pending.source_receipt_digest,
            EditorExternalPlayState::CleanupIncomplete,
            None,
        );
        self.pending = Some(pending);
        self.completed = Some(observation);
    }

    fn resume_after_retirement(
        &mut self,
        mut pending: PendingRequest,
        now: Instant,
    ) -> EditorExternalPlayObservation {
        if let Some(completion) = pending.completion.take() {
            return self.observation(
                pending.request.attempt(),
                pending.source_receipt_digest,
                completion.state,
                completion.result,
            );
        }

        match pending.operation {
            EditorPlayOperation::Play
            | EditorPlayOperation::Restart
            | EditorPlayOperation::RunLastGood => match self.begin_start(&mut pending, now) {
                Ok(()) => {
                    let attempt = pending.request.attempt();
                    let source_digest = pending.source_receipt_digest;
                    self.pending = Some(pending);
                    self.observation(
                        attempt,
                        source_digest,
                        if self.build.is_some() {
                            EditorExternalPlayState::Building
                        } else {
                            EditorExternalPlayState::Starting
                        },
                        None,
                    )
                }
                Err(EditorPlayFailure::Retirement) => {
                    let attempt = pending.request.attempt();
                    let source_digest = pending.source_receipt_digest;
                    self.pending = Some(pending);
                    self.observation(
                        attempt,
                        source_digest,
                        EditorExternalPlayState::CleanupIncomplete,
                        None,
                    )
                }
                Err(failure) => self.complete_terminal_request(
                    &pending,
                    EditorExternalPlayState::Faulted,
                    EditorPlayOperationResult::Failed {
                        operation: pending.operation,
                        failure,
                    },
                ),
            },
            EditorPlayOperation::Cancel => self.complete_terminal_request(
                &pending,
                EditorExternalPlayState::Idle,
                EditorPlayOperationResult::Cancelled {
                    operation: pending.operation,
                },
            ),
            EditorPlayOperation::Stop
            | EditorPlayOperation::RetryRetirement
            | EditorPlayOperation::RetryClose => self.complete_terminal_request(
                &pending,
                EditorExternalPlayState::Idle,
                EditorPlayOperationResult::Applied {
                    operation: pending.operation,
                    generation: None,
                },
            ),
            EditorPlayOperation::Pause
            | EditorPlayOperation::Resume
            | EditorPlayOperation::StepFixedTick => self.complete_terminal_request(
                &pending,
                EditorExternalPlayState::Faulted,
                EditorPlayOperationResult::Failed {
                    operation: pending.operation,
                    failure: EditorPlayFailure::Runtime,
                },
            ),
        }
    }

    fn advance_build(&mut self, now: Instant) -> Option<EditorExternalPlayObservation> {
        let project_root = self
            .pending
            .as_ref()
            .map(|pending| pending.project_root.clone());
        let (result, status, diagnostics) = {
            let build = self.build.as_mut()?;
            let result = build.advance(now);
            let status = build.status();
            let diagnostics = build.diagnostics_snapshot();
            (result, status, diagnostics)
        };
        if !self.close_requested
            && let Some(project_root) = project_root.as_deref()
        {
            self.output
                .observe_cargo_diagnostics(diagnostics.as_ref(), project_root);
        }
        if self.close_requested {
            if result.is_err() {
                self.close_cleanup_incomplete = true;
                return Some(self.close_incomplete_observation());
            }
            if matches!(
                status,
                DesktopBuildStatus::Succeeded
                    | DesktopBuildStatus::Failed(_)
                    | DesktopBuildStatus::Cancelled
                    | DesktopBuildStatus::Superseded(_)
            ) {
                self.build = None;
                self.pending = None;
                self.try_finish_close();
            }
            return None;
        }
        let mut pending = self.pending.take()?;
        if result.is_err() {
            pending.completion.get_or_insert(PendingCompletion {
                state: EditorExternalPlayState::Faulted,
                result: Some(EditorPlayOperationResult::Failed {
                    operation: pending.operation,
                    failure: EditorPlayFailure::Preparation,
                }),
            });
            pending.phase = PendingPhase::StoppingBuild;
            let cleanup_requested = self
                .build
                .as_mut()
                .is_some_and(|build| build.request_cancel(now).is_ok());
            let observation = self.observation(
                pending.request.attempt(),
                pending.source_receipt_digest,
                if cleanup_requested {
                    EditorExternalPlayState::Stopping
                } else {
                    EditorExternalPlayState::CleanupIncomplete
                },
                None,
            );
            self.pending = Some(pending);
            return Some(observation);
        }

        match status {
            DesktopBuildStatus::Running
            | DesktopBuildStatus::Cancelling
            | DesktopBuildStatus::CleaningUp => {
                let observation = self.observation(
                    pending.request.attempt(),
                    pending.source_receipt_digest,
                    if status == DesktopBuildStatus::Running {
                        EditorExternalPlayState::Building
                    } else {
                        EditorExternalPlayState::Stopping
                    },
                    None,
                );
                self.pending = Some(pending);
                Some(observation)
            }
            DesktopBuildStatus::Cancelled => {
                self.build = None;
                let completion = pending.completion.take().unwrap_or(PendingCompletion {
                    state: EditorExternalPlayState::Idle,
                    result: Some(if pending.operation == EditorPlayOperation::Cancel {
                        EditorPlayOperationResult::Cancelled {
                            operation: pending.operation,
                        }
                    } else {
                        EditorPlayOperationResult::Applied {
                            operation: pending.operation,
                            generation: None,
                        }
                    }),
                });
                Some(self.observation(
                    pending.request.attempt(),
                    pending.source_receipt_digest,
                    completion.state,
                    completion.result,
                ))
            }
            DesktopBuildStatus::Superseded(_) => {
                self.build = None;
                let completion = pending.completion.take().unwrap_or(PendingCompletion {
                    state: EditorExternalPlayState::Faulted,
                    result: Some(EditorPlayOperationResult::Failed {
                        operation: pending.operation,
                        failure: EditorPlayFailure::Superseded,
                    }),
                });
                Some(self.observation(
                    pending.request.attempt(),
                    pending.source_receipt_digest,
                    completion.state,
                    completion.result,
                ))
            }
            DesktopBuildStatus::Failed(_) => {
                self.build = None;
                let completion = pending.completion.take().unwrap_or_else(|| {
                    if matches!(
                        pending.operation,
                        EditorPlayOperation::Stop | EditorPlayOperation::Cancel
                    ) {
                        PendingCompletion {
                            state: EditorExternalPlayState::Idle,
                            result: Some(if pending.operation == EditorPlayOperation::Cancel {
                                EditorPlayOperationResult::Cancelled {
                                    operation: pending.operation,
                                }
                            } else {
                                EditorPlayOperationResult::Applied {
                                    operation: pending.operation,
                                    generation: None,
                                }
                            }),
                        }
                    } else {
                        PendingCompletion {
                            state: EditorExternalPlayState::Faulted,
                            result: Some(EditorPlayOperationResult::Failed {
                                operation: pending.operation,
                                failure: EditorPlayFailure::Preparation,
                            }),
                        }
                    }
                });
                Some(self.observation(
                    pending.request.attempt(),
                    pending.source_receipt_digest,
                    completion.state,
                    completion.result,
                ))
            }
            DesktopBuildStatus::Succeeded => {
                let candidate = self
                    .build
                    .as_mut()
                    .and_then(DesktopBuildAttempt::take_candidate);
                self.build = None;
                let Some(candidate) = candidate else {
                    return Some(self.complete_terminal_request(
                        &pending,
                        EditorExternalPlayState::Faulted,
                        EditorPlayOperationResult::Failed {
                            operation: pending.operation,
                            failure: EditorPlayFailure::Preparation,
                        },
                    ));
                };
                let launch = match pending.source.as_ref() {
                    Some(source) => {
                        let manifest = generation_manifest(&candidate, source);
                        self.start_preparation(
                            GenerationPreparationRequest::Build {
                                candidate,
                                manifest,
                            },
                            &mut pending,
                        )
                    }
                    None => Err(EditorPlayFailure::Start),
                };
                match launch {
                    Ok(()) => {
                        let observation = self.observation(
                            pending.request.attempt(),
                            pending.source_receipt_digest,
                            EditorExternalPlayState::Starting,
                            None,
                        );
                        self.pending = Some(pending);
                        Some(observation)
                    }
                    Err(EditorPlayFailure::Retirement) => {
                        let observation = self.observation(
                            pending.request.attempt(),
                            pending.source_receipt_digest,
                            EditorExternalPlayState::CleanupIncomplete,
                            None,
                        );
                        self.pending = Some(pending);
                        Some(observation)
                    }
                    Err(failure) => Some(self.complete_terminal_request(
                        &pending,
                        EditorExternalPlayState::Faulted,
                        EditorPlayOperationResult::Failed {
                            operation: pending.operation,
                            failure,
                        },
                    )),
                }
            }
        }
    }

    fn advance_supervisor(&mut self, now: Instant) -> Option<EditorExternalPlayObservation> {
        let project_root = self
            .pending
            .as_ref()
            .map(|pending| pending.project_root.clone());
        let project_product_logs = self.pending.as_ref().is_some_and(|pending| {
            !matches!(
                pending.operation,
                EditorPlayOperation::Play
                    | EditorPlayOperation::Restart
                    | EditorPlayOperation::RunLastGood
            ) || matches!(
                pending.phase,
                PendingPhase::Starting | PendingPhase::Running
            )
        });
        let close_requested = self.close_requested;
        let (advance_result, status, ready, terminal) = {
            let supervisor = self.supervisor.as_mut()?;
            let advance_result = supervisor.advance(now);
            if !close_requested {
                if let Err(error) = &advance_result {
                    observe_supervisor_advance_failure(
                        &mut self.output,
                        &mut self.supervisor_advance_failure_reported,
                        error,
                    );
                }
                if let Some(SupervisedDesktopSupervisorTerminal::Failed(failure)) =
                    supervisor.terminal()
                {
                    observe_supervisor_failure(&mut self.output, failure);
                }
            }
            if !close_requested
                && project_product_logs
                && let Some(project_root) = project_root.as_deref()
            {
                if let Some(process) = supervisor.process() {
                    self.output.observe_product_logs(
                        process.logs(),
                        process.output_loss_observed(),
                        project_root,
                    );
                }
            }
            let status = supervisor.status();
            let ready = supervisor.ready().is_some();
            let terminal = supervisor.terminal().map(supervisor_terminal_kind);
            (advance_result, status, ready, terminal)
        };
        if self.close_requested {
            if advance_result.is_err()
                || status == SupervisedDesktopSupervisorStatus::CleanupIncomplete
            {
                self.close_cleanup_incomplete = true;
                return Some(self.close_incomplete_observation());
            }
            if terminal.is_some() {
                self.supervisor = None;
                self.try_finish_close();
            }
            return None;
        }
        if advance_result.is_err() {
            let was_ready = ready
                || self
                    .pending
                    .as_ref()
                    .is_some_and(|pending| pending.ready_reported);
            if let Some(pending) = self.pending.as_mut() {
                pending.completion.get_or_insert(PendingCompletion {
                    state: EditorExternalPlayState::Faulted,
                    result: Some(EditorPlayOperationResult::Failed {
                        operation: pending.operation,
                        failure: if was_ready {
                            EditorPlayFailure::Runtime
                        } else {
                            EditorPlayFailure::Start
                        },
                    }),
                });
                pending.phase = PendingPhase::StoppingSupervisor;
            }
            let cleanup_requested = self.supervisor.as_mut().is_some_and(|supervisor| {
                supervisor.request_stop(supervisor.attempt(), now).is_ok()
            });
            return Some(
                self.observation(
                    self.pending
                        .as_ref()
                        .map_or(self.last_attempt, |pending| pending.request.attempt()),
                    self.pending
                        .as_ref()
                        .and_then(|pending| pending.source_receipt_digest)
                        .or(self.active_source_receipt_digest),
                    if cleanup_requested {
                        EditorExternalPlayState::Stopping
                    } else {
                        EditorExternalPlayState::CleanupIncomplete
                    },
                    None,
                ),
            );
        }

        if status == SupervisedDesktopSupervisorStatus::CleanupIncomplete {
            return Some(
                self.observation(
                    self.pending
                        .as_ref()
                        .map_or(self.last_attempt, |pending| pending.request.attempt()),
                    self.pending
                        .as_ref()
                        .and_then(|pending| pending.source_receipt_digest)
                        .or(self.active_source_receipt_digest),
                    EditorExternalPlayState::CleanupIncomplete,
                    None,
                ),
            );
        }

        if ready && terminal.is_some() && self.pending.as_ref().is_some_and(pending_accepts_ready) {
            let mut pending = self
                .pending
                .take()
                .expect("terminal READY admission has a pending request");
            pending.ready_reported = true;
            if self.admit_active_ready().is_err() {
                pending.completion = Some(PendingCompletion {
                    state: EditorExternalPlayState::Faulted,
                    result: Some(EditorPlayOperationResult::Failed {
                        operation: pending.operation,
                        failure: EditorPlayFailure::Start,
                    }),
                });
            }
            self.pending = Some(pending);
        }

        if ready && terminal.is_none() {
            let should_report = self.pending.as_ref().is_some_and(pending_accepts_ready);
            if should_report {
                let mut pending = self
                    .pending
                    .take()
                    .expect("ready report has pending request");
                pending.ready_reported = true;
                let operation = pending.operation;
                let attempt = pending.request.attempt();
                let source_digest = pending.source_receipt_digest;
                let generation = self.active_generation_label;
                if self.admit_active_ready().is_err() {
                    pending.completion = Some(PendingCompletion {
                        state: EditorExternalPlayState::Faulted,
                        result: Some(EditorPlayOperationResult::Failed {
                            operation: pending.operation,
                            failure: EditorPlayFailure::Start,
                        }),
                    });
                }
                if pending.completion.is_some() {
                    pending.phase = PendingPhase::StoppingSupervisor;
                    let cleanup_requested = self.supervisor.as_mut().is_some_and(|supervisor| {
                        supervisor.request_stop(supervisor.attempt(), now).is_ok()
                    });
                    self.pending = Some(pending);
                    return Some(self.observation(
                        attempt,
                        source_digest,
                        if cleanup_requested {
                            EditorExternalPlayState::Stopping
                        } else {
                            EditorExternalPlayState::CleanupIncomplete
                        },
                        None,
                    ));
                }
                let observation = self.observation(
                    attempt,
                    source_digest,
                    EditorExternalPlayState::Running,
                    Some(EditorPlayOperationResult::Applied {
                        operation,
                        generation,
                    }),
                );
                pending.phase = PendingPhase::Running;
                self.pending = Some(pending);
                return Some(observation);
            }
            if status == SupervisedDesktopSupervisorStatus::Running {
                return Some(self.observation(
                    self.last_attempt,
                    self.active_source_receipt_digest,
                    EditorExternalPlayState::Running,
                    None,
                ));
            }
        }

        let Some(terminal) = terminal else {
            return Some(
                self.observation(
                    self.pending
                        .as_ref()
                        .map_or(self.last_attempt, |pending| pending.request.attempt()),
                    self.pending
                        .as_ref()
                        .and_then(|pending| pending.source_receipt_digest)
                        .or(self.active_source_receipt_digest),
                    if status == SupervisedDesktopSupervisorStatus::Stopping {
                        EditorExternalPlayState::Stopping
                    } else {
                        EditorExternalPlayState::Starting
                    },
                    None,
                ),
            );
        };

        let active_source_receipt_digest = self.active_source_receipt_digest;
        let mut pending = self.pending.take();
        if let Some(pending) = pending.as_mut() {
            pending.source_receipt_digest = pending
                .source_receipt_digest
                .or(active_source_receipt_digest);
            if pending.completion.is_none() {
                pending.completion = match terminal {
                    SupervisorTerminalKind::Stopped
                        if !matches!(
                            pending.operation,
                            EditorPlayOperation::Play
                                | EditorPlayOperation::Restart
                                | EditorPlayOperation::RunLastGood
                        ) =>
                    {
                        Some(PendingCompletion {
                            state: EditorExternalPlayState::Idle,
                            result: Some(if pending.operation == EditorPlayOperation::Cancel {
                                EditorPlayOperationResult::Cancelled {
                                    operation: pending.operation,
                                }
                            } else {
                                EditorPlayOperationResult::Applied {
                                    operation: pending.operation,
                                    generation: None,
                                }
                            }),
                        })
                    }
                    SupervisorTerminalKind::Exited => Some(PendingCompletion {
                        state: EditorExternalPlayState::Idle,
                        result: None,
                    }),
                    SupervisorTerminalKind::Failed { failure } => Some(PendingCompletion {
                        state: EditorExternalPlayState::Faulted,
                        result: Some(EditorPlayOperationResult::Failed {
                            operation: pending.operation,
                            failure,
                        }),
                    }),
                    SupervisorTerminalKind::Stopped => None,
                };
            }
            pending.phase = PendingPhase::StoppingGeneration;
        }
        self.supervisor = None;
        if self.finish_active_launch().is_err() {
            self.pending = pending;
            return Some(self.observation(
                self.last_attempt,
                active_source_receipt_digest,
                EditorExternalPlayState::CleanupIncomplete,
                None,
            ));
        }
        self.clear_active_generation_tracking();

        if let Some(pending) = pending {
            return Some(self.resume_after_retirement(pending, now));
        }

        let completion = match terminal {
            SupervisorTerminalKind::Stopped | SupervisorTerminalKind::Exited => PendingCompletion {
                state: EditorExternalPlayState::Idle,
                result: None,
            },
            SupervisorTerminalKind::Failed { failure } => PendingCompletion {
                state: EditorExternalPlayState::Faulted,
                result: Some(EditorPlayOperationResult::Failed {
                    operation: EditorPlayOperation::Play,
                    failure,
                }),
            },
        };
        Some(self.observation(
            self.last_attempt,
            active_source_receipt_digest,
            completion.state,
            completion.result,
        ))
    }

    fn start_preparation(
        &mut self,
        request: GenerationPreparationRequest,
        pending: &mut PendingRequest,
    ) -> Result<(), EditorPlayFailure> {
        match GenerationPreparationOwner::start(self.store.take(), request, self.wake.clone()) {
            Ok(preparation) => {
                self.preparation = Some(preparation);
                pending.phase = PendingPhase::PreparingGeneration;
                Ok(())
            }
            Err(store) => {
                self.store = store;
                Err(EditorPlayFailure::Preparation)
            }
        }
    }

    fn advance_preparation(&mut self, now: Instant) -> Option<EditorExternalPlayObservation> {
        let outcome = self.preparation.as_mut()?.take_finished()?;
        self.preparation = None;
        self.store = outcome.store;
        let mut pending = self.pending.take()?;
        if let Some(active) = self
            .store
            .as_ref()
            .and_then(ExecutableGenerationStore::active)
        {
            self.active_generation_label = Some(active.sequence());
            self.active_source_receipt_digest = pending.source_receipt_digest;
        }
        if self.close_requested {
            self.pending = Some(pending);
            self.try_finish_close();
            return self.completed.take();
        }
        if pending.phase == PendingPhase::StoppingPreparation {
            return match self.finish_active_launch() {
                Ok(()) => {
                    self.clear_active_generation_tracking();
                    Some(self.resume_after_retirement(pending, now))
                }
                Err(_) => {
                    pending.phase = PendingPhase::StoppingGeneration;
                    self.complete_cleanup_incomplete(pending);
                    self.completed.take()
                }
            };
        }
        let result = outcome
            .result
            .map_err(|error| {
                if let Some(facts) = error.diagnostic {
                    self.output.observe_host_failure_facts(facts);
                }
                error.failure
            })
            .and_then(|generation| self.launch_generation(&mut pending, generation, now));
        match result {
            Ok(()) => {
                let observation = self.observation(
                    pending.request.attempt(),
                    pending.source_receipt_digest,
                    EditorExternalPlayState::Starting,
                    None,
                );
                self.pending = Some(pending);
                Some(observation)
            }
            Err(EditorPlayFailure::Retirement) => {
                self.complete_cleanup_incomplete(pending);
                self.completed.take()
            }
            Err(failure) => Some(self.complete_terminal_request(
                &pending,
                EditorExternalPlayState::Faulted,
                EditorPlayOperationResult::Failed {
                    operation: pending.operation,
                    failure,
                },
            )),
        }
    }

    fn launch_generation(
        &mut self,
        pending: &mut PendingRequest,
        generation: ImmutableExecutableGeneration,
        now: Instant,
    ) -> Result<(), EditorPlayFailure> {
        let id = generation.id();
        self.active_generation_label = Some(generation.sequence());
        self.active_source_receipt_digest = pending.source_receipt_digest;
        let Some(attempt) = SupervisedDesktopAttemptId::new(pending.request.attempt()) else {
            return Err(self.abort_generation_launch(pending, id));
        };
        let Some(source) = pending.source.as_ref() else {
            return Err(self.abort_generation_launch(pending, id));
        };
        let Some(store) = self.store.as_mut() else {
            self.output
                .observe_host_failure("work-directory", "store-unavailable", None);
            return Err(self.abort_generation_launch(pending, id));
        };
        let working_directory = match store.begin_work_directory() {
            Ok(working_directory) => working_directory,
            Err(error) => {
                self.output
                    .observe_host_failure_facts(HostFailureFacts::generation(
                        "work-directory",
                        &error,
                    ));
                return Err(self.abort_generation_launch(pending, id));
            }
        };
        let launch = match SupervisedDesktopLaunchRequest::new(
            attempt,
            pending.project_root.clone(),
            working_directory,
            generation,
            source.canonical_source_receipt().to_vec(),
            READY_TIMEOUT,
        ) {
            Ok(launch) => launch,
            Err(error) => {
                self.output.observe_host_failure(
                    "launch-context",
                    supervisor_start_failure_reason(&error),
                    Some(&error),
                );
                return Err(self.abort_generation_launch(pending, id));
            }
        };
        let supervisor =
            match SupervisedDesktopSessionSupervisor::start(launch, now, self.wake.clone()) {
                Ok(supervisor) => supervisor,
                Err(error) => {
                    self.output.observe_host_failure(
                        "supervisor-start",
                        supervisor_start_failure_reason(&error),
                        Some(&error),
                    );
                    return Err(self.abort_generation_launch(pending, id));
                }
            };
        self.supervisor = Some(supervisor);
        self.supervisor_advance_failure_reported = false;
        pending.phase = PendingPhase::Starting;
        Ok(())
    }

    fn abort_generation_launch(
        &mut self,
        pending: &mut PendingRequest,
        id: ExecutableGenerationId,
    ) -> EditorPlayFailure {
        pending.completion = Some(PendingCompletion {
            state: EditorExternalPlayState::Faulted,
            result: Some(EditorPlayOperationResult::Failed {
                operation: pending.operation,
                failure: EditorPlayFailure::Start,
            }),
        });
        if self.finish_launch(id).is_err() {
            pending.phase = PendingPhase::StoppingGeneration;
            EditorPlayFailure::Retirement
        } else {
            self.clear_active_generation_tracking();
            EditorPlayFailure::Start
        }
    }

    fn active_generation_id(&self) -> Option<ExecutableGenerationId> {
        self.store
            .as_ref()
            .and_then(|store| store.active().map(|generation| generation.id()))
    }

    fn admit_active_ready(&mut self) -> Result<(), EditorPlayFailure> {
        let active_id = self
            .active_generation_id()
            .ok_or(EditorPlayFailure::Start)?;
        self.store
            .as_mut()
            .ok_or(EditorPlayFailure::Start)?
            .admit_ready(active_id)
            .map_err(|error| {
                self.output
                    .observe_host_failure_facts(HostFailureFacts::generation(
                        "ready-admission",
                        &error,
                    ));
                EditorPlayFailure::Start
            })
    }

    fn finish_launch(&mut self, id: ExecutableGenerationId) -> Result<(), EditorPlayFailure> {
        let store = self.store.as_mut().ok_or(EditorPlayFailure::Retirement)?;
        store
            .finish_work_directory()
            .map_err(|_| EditorPlayFailure::Retirement)?;
        store
            .finish_active(id)
            .map_err(|_| EditorPlayFailure::Retirement)
    }

    fn finish_active_launch(&mut self) -> Result<(), EditorPlayFailure> {
        if let Some(store) = self.store.as_mut()
            && store.active().is_none()
            && store.cleanup_incomplete()
        {
            store
                .try_close()
                .map_err(|_| EditorPlayFailure::Retirement)?;
            self.store = None;
            return Ok(());
        }
        match (
            self.active_generation_id(),
            self.active_generation_label,
            self.active_source_receipt_digest,
        ) {
            (Some(id), Some(_), Some(_)) => self.finish_launch(id)?,
            (None, None, None) => {}
            _ => return Err(EditorPlayFailure::Retirement),
        }
        Ok(())
    }

    fn clear_active_generation_tracking(&mut self) {
        self.active_source_receipt_digest = None;
        self.active_generation_label = None;
    }

    fn try_finish_close(&mut self) {
        if !self.close_requested
            || self.close_cleanup_incomplete
            || self.build.is_some()
            || self.preparation.is_some()
            || self.supervisor.is_some()
        {
            return;
        }
        if self.finish_active_launch().is_err() {
            self.close_cleanup_incomplete = true;
            self.publish_close_incomplete();
            return;
        }
        self.clear_active_generation_tracking();
        self.pending = None;
        let Some(store) = self.store.as_mut() else {
            self.output.clear();
            self.close_complete = true;
            return;
        };
        if store.try_close().is_ok() {
            self.store = None;
            self.close_cleanup_incomplete = false;
            self.output.clear();
            self.close_complete = true;
        } else {
            self.close_cleanup_incomplete = true;
            self.publish_close_incomplete();
        }
    }

    fn publish_close_incomplete(&mut self) {
        self.completed = Some(self.close_incomplete_observation());
    }

    fn close_incomplete_observation(&self) -> EditorExternalPlayObservation {
        self.observation(
            self.last_attempt,
            self.pending
                .as_ref()
                .and_then(|pending| pending.source_receipt_digest)
                .or(self.active_source_receipt_digest),
            EditorExternalPlayState::CleanupIncomplete,
            Some(EditorPlayOperationResult::Failed {
                operation: EditorPlayOperation::RetryClose,
                failure: EditorPlayFailure::Close,
            }),
        )
    }

    fn complete_failure(
        &mut self,
        request: &EditorExternalPlayRequest,
        source_receipt_digest: Option<ContentDigest>,
        failure: EditorPlayFailure,
        state: EditorExternalPlayState,
    ) {
        self.completed = Some(self.observation(
            request.attempt(),
            source_receipt_digest,
            state,
            Some(EditorPlayOperationResult::Failed {
                operation: request.operation(),
                failure,
            }),
        ));
    }

    fn complete_terminal_request(
        &self,
        pending: &PendingRequest,
        state: EditorExternalPlayState,
        result: EditorPlayOperationResult,
    ) -> EditorExternalPlayObservation {
        self.observation(
            pending.request.attempt(),
            pending.source_receipt_digest,
            state,
            Some(result),
        )
    }

    fn observation(
        &self,
        attempt: u64,
        source_receipt_digest: Option<ContentDigest>,
        state: EditorExternalPlayState,
        result: Option<EditorPlayOperationResult>,
    ) -> EditorExternalPlayObservation {
        EditorExternalPlayObservation::new(
            attempt,
            state,
            self.active_generation_label,
            self.last_good_available(source_receipt_digest),
            source_receipt_digest.or(self.active_source_receipt_digest),
            self.output.snapshot(),
            result,
        )
        .expect("owner observations always carry a non-zero request attempt")
    }

    fn last_good_available(&self, source_receipt_digest: Option<ContentDigest>) -> bool {
        let Some(source_receipt_digest) =
            source_receipt_digest.or(self.active_source_receipt_digest)
        else {
            return false;
        };
        self.preparation
            .as_ref()
            .is_some_and(|preparation| preparation.last_good_matches(source_receipt_digest))
            || self.store.as_ref().is_some_and(|store| {
                store.last_good().is_some_and(|generation| {
                    ContentDigest::of_bytes(generation.source_receipt()) == source_receipt_digest
                })
            })
    }
}

fn generation_manifest(
    candidate: &VerifiedDesktopBuildCandidate,
    source: &SupervisedDesktopSourceAuthority,
) -> GenerationManifestInput {
    let invocation = candidate.invocation();
    let package = invocation.root_package();
    let package_identity = format!("{}@{}", package.name(), package.version());
    let target = GenerationTargetFacts::new(
        GenerationPackageFacts::new(
            ContentDigest::of_bytes(package_identity.as_bytes()),
            package.name(),
            package.version(),
            "Cargo.toml",
        ),
        invocation.target_name(),
        candidate.toolchain().host_target(),
        invocation.profile(),
        invocation
            .features()
            .into_iter()
            .map(str::to_owned)
            .collect(),
    );
    let source_facts = GenerationSourceFacts::new(
        DESKTOP_SUPERVISED_PLAY_V1_CONTRACT,
        SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION.to_string(),
        source.project_stable_id(),
        source.profile_name(),
        source.canonical_source_receipt().to_vec(),
    );
    let build = GenerationBuildObservation::new(
        candidate.source_revision().manifest(),
        Some(candidate.source_revision().lockfile()),
        candidate.metadata_digest(),
        candidate.toolchain().cargo_version_digest(),
        candidate.toolchain().rustc_verbose_version_digest(),
        candidate.invocation_digest(),
    );
    GenerationManifestInput::new(source_facts, target, build)
}

fn pending_accepts_ready(pending: &PendingRequest) -> bool {
    pending.phase == PendingPhase::Starting
        && !pending.ready_reported
        && matches!(
            pending.operation,
            EditorPlayOperation::Play
                | EditorPlayOperation::Restart
                | EditorPlayOperation::RunLastGood
        )
}

fn supervisor_terminal_kind(
    terminal: &SupervisedDesktopSupervisorTerminal,
) -> SupervisorTerminalKind {
    match terminal {
        SupervisedDesktopSupervisorTerminal::Stopped => SupervisorTerminalKind::Stopped,
        SupervisedDesktopSupervisorTerminal::Exited => SupervisorTerminalKind::Exited,
        SupervisedDesktopSupervisorTerminal::Failed(failure) => SupervisorTerminalKind::Failed {
            failure: match failure {
                SupervisedDesktopSupervisorFailure::HandshakeStart(_)
                | SupervisedDesktopSupervisorFailure::Handshake(_)
                | SupervisedDesktopSupervisorFailure::ExitedBeforeReady
                | SupervisedDesktopSupervisorFailure::InvalidState
                | SupervisedDesktopSupervisorFailure::ReadyDeadlineOverflow
                | SupervisedDesktopSupervisorFailure::Spawn(_)
                | SupervisedDesktopSupervisorFailure::SpawnTimeout => EditorPlayFailure::Start,
                SupervisedDesktopSupervisorFailure::NonZeroExit(_) => EditorPlayFailure::Runtime,
            },
        },
    }
}

fn observe_supervisor_failure(
    output: &mut EditorExternalPlayOutputProjection,
    failure: &SupervisedDesktopSupervisorFailure,
) {
    if let SupervisedDesktopSupervisorFailure::Spawn(SupervisedProcessSpawnFailure::Process(
        SupervisedProcessError::Generation(error),
    )) = failure
    {
        output.observe_host_failure_facts(HostFailureFacts::generation("spawn-generation", error));
        return;
    }
    let (stage, reason) = match failure {
        SupervisedDesktopSupervisorFailure::Spawn(error) => ("spawn", spawn_failure_reason(error)),
        SupervisedDesktopSupervisorFailure::SpawnTimeout => ("spawn", "deadline"),
        SupervisedDesktopSupervisorFailure::HandshakeStart(_) => ("handshake-start", "dispatch"),
        SupervisedDesktopSupervisorFailure::Handshake(kind) => (
            "handshake",
            match kind {
                SupervisedDesktopHandshakeFailureKind::Writer => "writer-worker",
                SupervisedDesktopHandshakeFailureKind::Write => "bootstrap-write",
                SupervisedDesktopHandshakeFailureKind::Read => "ready-read",
                SupervisedDesktopHandshakeFailureKind::Wire => "ready-invalid",
                SupervisedDesktopHandshakeFailureKind::ReadyTooLarge => "ready-too-large",
                SupervisedDesktopHandshakeFailureKind::AllocationFailed => "allocation",
                SupervisedDesktopHandshakeFailureKind::DuplicateOrTrailingReady => "ready-trailing",
                SupervisedDesktopHandshakeFailureKind::EofBeforeReady => "eof-before-ready",
                SupervisedDesktopHandshakeFailureKind::LaunchFactsMismatch => "launch-facts",
                SupervisedDesktopHandshakeFailureKind::RuntimeFactsMismatch => "runtime-facts",
                SupervisedDesktopHandshakeFailureKind::ReadyTimeout => "ready-deadline",
                SupervisedDesktopHandshakeFailureKind::Cancelled => "cancelled",
            },
        ),
        SupervisedDesktopSupervisorFailure::ExitedBeforeReady => ("handshake", "child-exited"),
        SupervisedDesktopSupervisorFailure::NonZeroExit(_) => ("runtime", "non-zero-exit"),
        SupervisedDesktopSupervisorFailure::InvalidState => ("supervisor", "invalid-state"),
        SupervisedDesktopSupervisorFailure::ReadyDeadlineOverflow => {
            ("handshake", "deadline-overflow")
        }
    };
    output.observe_host_failure(stage, reason, Some(failure));
}

fn observe_supervisor_advance_failure(
    output: &mut EditorExternalPlayOutputProjection,
    reported: &mut bool,
    error: &SupervisedDesktopSupervisorAdvanceError,
) {
    if !*reported {
        output.observe_host_failure("supervisor-advance", "process-observation", Some(error));
        *reported = true;
    }
}

fn spawn_failure_reason(failure: &SupervisedProcessSpawnFailure) -> &'static str {
    match failure {
        SupervisedProcessSpawnFailure::Dispatch(_) => "worker-dispatch",
        SupervisedProcessSpawnFailure::WorkerDisconnected => "worker-disconnected",
        SupervisedProcessSpawnFailure::WorkerPanicked => "worker-panicked",
        SupervisedProcessSpawnFailure::Process(SupervisedProcessError::Generation(_)) => {
            "generation-revalidation"
        }
        SupervisedProcessSpawnFailure::Process(SupervisedProcessError::Spawn(_)) => "os-spawn",
        SupervisedProcessSpawnFailure::Process(_) => "process-setup",
        SupervisedProcessSpawnFailure::Cleanup(_) => "late-process-cleanup",
    }
}

fn supervisor_start_failure_reason(error: &SupervisedDesktopSupervisorStartError) -> &'static str {
    match error {
        SupervisedDesktopSupervisorStartError::SourceAdmission(_) => "source-admission",
        SupervisedDesktopSupervisorStartError::SourceReceiptMismatch => "source-receipt-mismatch",
        SupervisedDesktopSupervisorStartError::InvalidReadyTimeout => "ready-timeout-invalid",
        SupervisedDesktopSupervisorStartError::InvalidWorkingDirectory => {
            "working-directory-invalid"
        }
        SupervisedDesktopSupervisorStartError::SpawnDeadlineOverflow => "spawn-deadline-overflow",
        SupervisedDesktopSupervisorStartError::Wire(_) => "launch-invalid",
        SupervisedDesktopSupervisorStartError::NonceIo(_) => "session-nonce",
        SupervisedDesktopSupervisorStartError::Spawn(error) => spawn_failure_reason(error),
        #[cfg(not(any(windows, all(unix, target_os = "linux", target_arch = "x86_64"))))]
        SupervisedDesktopSupervisorStartError::UnsupportedPlatform => "unsupported-platform",
    }
}

impl DesktopBuildInputRevision {
    fn from_source(source: &SupervisedDesktopSourceAuthority) -> Option<Self> {
        Some(Self::new(
            source.cargo_manifest_digest()?,
            source.cargo_lock_digest()?,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, thread};

    use super::*;

    fn request(
        attempt: u64,
        operation: EditorPlayOperation,
        source_receipt: Option<Vec<u8>>,
    ) -> EditorExternalPlayRequest {
        EditorExternalPlayRequest::new(attempt, operation, source_receipt)
            .expect("test attempts are non-zero")
    }

    #[test]
    fn malformed_source_receipt_fails_before_build_or_process_authority() {
        let mut owner = SupervisedDesktopPlayOwner::new();
        owner.submit(
            request(7, EditorPlayOperation::Play, Some(vec![0x01, 0x02, 0x03])),
            PathBuf::from("C:\\nara\\missing-project"),
            Instant::now(),
        );

        let observation = owner
            .advance(Instant::now())
            .expect("rejected source receipt is reported once");
        assert_eq!(observation.attempt(), 7);
        assert_eq!(observation.state(), EditorExternalPlayState::Faulted);
        assert_eq!(
            observation.source_receipt_digest(),
            Some(ContentDigest::of_bytes(&[0x01, 0x02, 0x03]))
        );
        assert!(matches!(
            observation.result(),
            Some(EditorPlayOperationResult::Failed {
                operation: EditorPlayOperation::Play,
                failure: EditorPlayFailure::Start,
            })
        ));
        assert_eq!(observation.output().messages().len(), 1);
        let message = &observation.output().messages()[0];
        assert_eq!(message.code(), Some("external-play.source"));
        assert_eq!(
            message.text(),
            "External Play failed: stage=source; reason=receipt-admission"
        );
        assert!(owner.build.is_none());
        assert!(owner.supervisor.is_none());
        assert!(owner.store.is_none());
    }

    #[test]
    fn spawn_failure_reports_the_typed_boundary_without_error_text() {
        let mut owner = SupervisedDesktopPlayOwner::new();
        let failure =
            SupervisedDesktopSupervisorFailure::Spawn(SupervisedProcessSpawnFailure::Process(
                SupervisedProcessError::Spawn(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "private-token /private/project/product C:\\private\\product.exe",
                )),
            ));
        observe_supervisor_failure(&mut owner.output, &failure);

        let output = owner.output.snapshot();
        assert_eq!(output.messages().len(), 1);
        let message = &output.messages()[0];
        assert_eq!(message.code(), Some("external-play.spawn"));
        assert_eq!(
            message.text(),
            "External Play failed: stage=spawn; reason=os-spawn; io_kind=PermissionDenied"
        );
        assert!(owner.build.is_none());
        assert!(owner.supervisor.is_none());
        assert!(owner.store.is_none());
    }

    #[test]
    fn repeated_supervisor_advance_failure_preserves_the_first_output_sequence() {
        let mut owner = SupervisedDesktopPlayOwner::new();
        let failure = SupervisedDesktopSupervisorAdvanceError::Process(
            SupervisedProcessError::Containment(std::io::Error::from_raw_os_error(13)),
        );
        observe_supervisor_advance_failure(
            &mut owner.output,
            &mut owner.supervisor_advance_failure_reported,
            &failure,
        );
        let first = owner.output.snapshot();
        assert_eq!(first.messages().len(), 1);
        assert!(first.messages()[0].text().ends_with("os_error=13"));
        for _ in 0..16 {
            observe_supervisor_advance_failure(
                &mut owner.output,
                &mut owner.supervisor_advance_failure_reported,
                &SupervisedDesktopSupervisorAdvanceError::InvalidState,
            );
            assert_eq!(owner.output.snapshot(), first);
        }
    }

    #[test]
    fn preparation_failure_returns_store_and_projects_exact_reason() {
        let base = unique_test_root("preparation-failure-facts");
        fs::create_dir_all(&base).unwrap();
        let store = ExecutableGenerationStore::create(&base).unwrap();
        let (preparation, started, release) = GenerationPreparationOwner::held_for_test(store);
        started.recv_timeout(Duration::from_secs(10)).unwrap();
        let mut owner = SupervisedDesktopPlayOwner::new();
        owner.preparation = Some(preparation);
        owner.pending = Some(PendingRequest {
            request: request(31, EditorPlayOperation::Play, None),
            operation: EditorPlayOperation::Play,
            project_root: base.clone(),
            source: None,
            source_receipt_digest: None,
            phase: PendingPhase::PreparingGeneration,
            completion: None,
            ready_reported: false,
        });
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let terminal = loop {
            if let Some(observation) = owner.advance(Instant::now()) {
                break observation;
            }
            assert!(
                Instant::now() < deadline,
                "preparation failure was not returned"
            );
            thread::sleep(Duration::from_millis(1));
        };
        assert_eq!(terminal.state(), EditorExternalPlayState::Faulted);
        assert_eq!(
            terminal.result(),
            Some(EditorPlayOperationResult::Failed {
                operation: EditorPlayOperation::Play,
                failure: EditorPlayFailure::Start,
            })
        );
        assert_eq!(terminal.output().messages().len(), 1);
        assert_eq!(
            terminal.output().messages()[0].code(),
            Some("external-play.generation-last-good")
        );
        assert_eq!(
            terminal.output().messages()[0].text(),
            "External Play failed: stage=generation-last-good; reason=source-receipt-mismatch"
        );
        assert!(
            owner.store.is_some(),
            "the finished worker returns physical store ownership"
        );
        assert!(owner.supervisor.is_none());
        owner.request_close(Instant::now());
        assert!(owner.close_complete());
        drop(owner);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn unsupported_runtime_control_fails_without_starting_a_process() {
        let mut owner = SupervisedDesktopPlayOwner::new();
        owner.submit(
            request(11, EditorPlayOperation::Pause, None),
            PathBuf::from("C:\\nara\\missing-project"),
            Instant::now(),
        );

        let observation = owner
            .advance(Instant::now())
            .expect("unsupported control is reported once");
        assert_eq!(observation.state(), EditorExternalPlayState::Faulted);
        assert!(matches!(
            observation.result(),
            Some(EditorPlayOperationResult::Failed {
                operation: EditorPlayOperation::Pause,
                failure: EditorPlayFailure::Runtime,
            })
        ));
    }

    #[test]
    fn close_without_live_capabilities_completes_immediately() {
        let mut owner = SupervisedDesktopPlayOwner::new();
        owner.request_close(Instant::now());

        assert!(owner.close_complete());
        assert!(owner.advance(Instant::now()).is_none());
    }

    #[test]
    fn cancel_and_close_retain_publication_until_the_worker_is_joined() {
        let base = unique_test_root("cancel-publication");
        fs::create_dir_all(&base).unwrap();
        let store = ExecutableGenerationStore::create(&base).unwrap();
        let (preparation, started, release) = GenerationPreparationOwner::held_for_test(store);
        started.recv_timeout(Duration::from_secs(10)).unwrap();
        let mut owner = SupervisedDesktopPlayOwner::new();
        owner.preparation = Some(preparation);
        owner.pending = Some(PendingRequest {
            request: request(29, EditorPlayOperation::Play, None),
            operation: EditorPlayOperation::Play,
            project_root: base.clone(),
            source: None,
            source_receipt_digest: Some(ContentDigest::of_bytes(b"held-publication")),
            phase: PendingPhase::PreparingGeneration,
            completion: None,
            ready_reported: false,
        });
        owner.submit(
            request(30, EditorPlayOperation::Cancel, None),
            base.clone(),
            Instant::now(),
        );
        assert_eq!(
            owner.pending.as_ref().unwrap().phase,
            PendingPhase::StoppingPreparation
        );
        assert!(owner.advance(Instant::now()).is_none());
        owner.request_close(Instant::now());
        assert!(!owner.close_complete());
        assert!(owner.preparation.is_some());
        assert!(
            owner.store.is_none(),
            "the worker retains the physical store"
        );

        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while !owner.close_complete() {
            let _ = owner.advance(Instant::now());
            assert!(
                Instant::now() < deadline,
                "publication cleanup did not retire"
            );
            thread::yield_now();
        }
        assert!(owner.preparation.is_none());
        assert!(owner.store.is_none());
        drop(owner);
        fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn ready_is_never_admitted_after_stop_owns_the_pending_session() {
        let mut pending = PendingRequest {
            request: request(15, EditorPlayOperation::Play, None),
            operation: EditorPlayOperation::Play,
            project_root: std::env::temp_dir(),
            source: None,
            source_receipt_digest: None,
            phase: PendingPhase::Starting,
            completion: None,
            ready_reported: false,
        };
        assert!(pending_accepts_ready(&pending));

        pending.request = request(16, EditorPlayOperation::Stop, None);
        pending.operation = EditorPlayOperation::Stop;
        pending.phase = PendingPhase::StoppingSupervisor;
        assert!(!pending_accepts_ready(&pending));
    }

    #[test]
    fn cancel_replaces_an_in_flight_build_request_and_waits_for_cleanup() {
        let mut owner = SupervisedDesktopPlayOwner::new();
        owner.build = Some(DesktopBuildAttempt::waiting_test_attempt());
        owner.pending = Some(PendingRequest {
            request: request(17, EditorPlayOperation::Play, None),
            operation: EditorPlayOperation::Play,
            project_root: std::env::temp_dir(),
            source: None,
            source_receipt_digest: Some(ContentDigest::of_bytes(b"source")),
            phase: PendingPhase::Building,
            completion: None,
            ready_reported: false,
        });

        let start = Instant::now();
        owner.submit(
            request(18, EditorPlayOperation::Cancel, None),
            std::env::temp_dir(),
            start,
        );

        let pending = owner.pending.as_ref().expect("cleanup remains owned");
        assert_eq!(pending.request.attempt(), 18);
        assert_eq!(pending.operation, EditorPlayOperation::Cancel);
        assert_eq!(pending.phase, PendingPhase::StoppingBuild);
        assert!(owner.build.is_some());

        let deadline = start + Duration::from_secs(10);
        let terminal = loop {
            if let Some(observation) = owner.advance(Instant::now())
                && observation.result().is_some()
            {
                break observation;
            }
            assert!(Instant::now() < deadline, "build cancellation timed out");
            thread::sleep(Duration::from_millis(5));
        };

        assert_eq!(terminal.state(), EditorExternalPlayState::Idle);
        assert!(matches!(
            terminal.result(),
            Some(EditorPlayOperationResult::Cancelled {
                operation: EditorPlayOperation::Cancel,
            })
        ));
        assert!(owner.build.is_none());
        assert!(owner.pending.is_none());
    }

    #[test]
    fn close_retains_an_in_flight_build_until_cleanup_is_terminal() {
        let mut owner = SupervisedDesktopPlayOwner::new();
        owner.build = Some(DesktopBuildAttempt::waiting_test_attempt());
        owner.pending = Some(PendingRequest {
            request: request(19, EditorPlayOperation::Play, None),
            operation: EditorPlayOperation::Play,
            project_root: std::env::temp_dir(),
            source: None,
            source_receipt_digest: Some(ContentDigest::of_bytes(b"source")),
            phase: PendingPhase::Building,
            completion: None,
            ready_reported: false,
        });

        let start = Instant::now();
        owner.request_close(start);
        assert!(!owner.close_complete());
        assert!(owner.build.is_some());

        let deadline = start + Duration::from_secs(10);
        while !owner.close_complete() {
            assert!(owner.advance(Instant::now()).is_none());
            assert!(Instant::now() < deadline, "build close cleanup timed out");
            thread::sleep(Duration::from_millis(5));
        }

        assert!(owner.build.is_none());
        assert!(owner.pending.is_none());
    }

    #[test]
    fn generation_retirement_failure_remains_retryable_and_retains_identity() {
        let base = unique_test_root("retirement-retry");
        fs::create_dir_all(&base).unwrap();
        let artifact = base.join(
            std::env::current_exe()
                .unwrap()
                .file_name()
                .expect("the test executable has a file name"),
        );
        fs::copy(std::env::current_exe().unwrap(), &artifact).unwrap();
        let source_receipt = b"retirement-source";
        let mut store = ExecutableGenerationStore::create(&base).unwrap();
        let generation = store
            .publish_candidate(&artifact, generation_manifest(source_receipt))
            .unwrap();
        store.activate_candidate(generation.id()).unwrap();
        store.set_cleanup_incomplete_for_test(true);

        let source_digest = ContentDigest::of_bytes(source_receipt);
        let mut owner = SupervisedDesktopPlayOwner::new();
        owner.store = Some(store);
        owner.active_generation_label = Some(41);
        owner.active_source_receipt_digest = Some(source_digest);
        assert_eq!(
            owner.admit_active_ready(),
            Err(EditorPlayFailure::Start),
            "READY admission must fail closed while the generation store is not operational"
        );
        assert!(owner.store.as_ref().unwrap().last_good().is_none());
        owner.submit(
            request(21, EditorPlayOperation::RetryRetirement, None),
            base.clone(),
            Instant::now(),
        );

        let incomplete = owner
            .advance(Instant::now())
            .expect("retirement failure is observable");
        assert_eq!(
            incomplete.state(),
            EditorExternalPlayState::CleanupIncomplete
        );
        assert_eq!(incomplete.generation(), Some(41));
        assert_eq!(incomplete.source_receipt_digest(), Some(source_digest));
        assert!(owner.active_generation_id().is_some());

        owner
            .store
            .as_mut()
            .unwrap()
            .set_cleanup_incomplete_for_test(false);
        owner.submit(
            request(22, EditorPlayOperation::RetryRetirement, None),
            base.clone(),
            Instant::now(),
        );
        let retired = owner
            .advance(Instant::now())
            .expect("the explicit retry completes retirement");
        assert_eq!(retired.state(), EditorExternalPlayState::Idle);
        assert!(matches!(
            retired.result(),
            Some(EditorPlayOperationResult::Applied {
                operation: EditorPlayOperation::RetryRetirement,
                generation: None,
            })
        ));
        assert!(owner.active_generation_id().is_none());
        assert_eq!(owner.active_generation_label, None);

        owner.request_close(Instant::now());
        assert!(owner.close_complete());
        drop(owner);
        let _ = fs::remove_dir_all(base);
    }

    #[test]
    fn closing_failure_accepts_retry_close_and_retires_the_same_generation() {
        let base = unique_test_root("close-retry");
        fs::create_dir_all(&base).unwrap();
        let artifact = std::env::current_exe().unwrap();
        let mut store = ExecutableGenerationStore::create(&base).unwrap();
        let source = b"close-retry-source";
        let generation = store
            .publish_candidate(&artifact, generation_manifest(source))
            .unwrap();
        store.activate_candidate(generation.id()).unwrap();
        store.set_cleanup_incomplete_for_test(true);
        let mut owner = SupervisedDesktopPlayOwner::new();
        owner.store = Some(store);
        owner.active_generation_label = Some(generation.sequence());
        owner.active_source_receipt_digest = Some(ContentDigest::of_bytes(source));
        owner.request_close(Instant::now());
        assert!(owner.close_retry_required());
        assert!(!owner.close_complete());
        let incomplete = owner.advance(Instant::now()).unwrap();
        assert_eq!(incomplete.generation(), Some(generation.sequence()));
        assert_eq!(
            incomplete.result(),
            Some(EditorPlayOperationResult::Failed {
                operation: EditorPlayOperation::RetryClose,
                failure: EditorPlayFailure::Close,
            })
        );
        owner
            .store
            .as_mut()
            .unwrap()
            .set_cleanup_incomplete_for_test(false);
        owner.submit(
            request(31, EditorPlayOperation::RetryClose, None),
            base.clone(),
            Instant::now(),
        );
        assert!(owner.close_complete());
        assert!(!owner.close_retry_required());
        assert!(owner.store.is_none());
        drop(owner);
        drop(generation);
        fs::remove_dir_all(base).unwrap();
    }

    fn generation_manifest(source_receipt: &[u8]) -> GenerationManifestInput {
        let digest = |value: &[u8]| ContentDigest::of_bytes(value);
        GenerationManifestInput::new(
            GenerationSourceFacts::new(
                DESKTOP_SUPERVISED_PLAY_V1_CONTRACT,
                SUPERVISED_DESKTOP_LAUNCH_CONTEXT_VERSION.to_string(),
                "project-identity",
                "desktop",
                source_receipt.to_vec(),
            ),
            GenerationTargetFacts::new(
                GenerationPackageFacts::new(
                    digest(b"package-id"),
                    "package",
                    "0.1.0",
                    "Cargo.toml",
                ),
                "desktop",
                "test-host",
                "dev",
                Vec::new(),
            ),
            GenerationBuildObservation::new(
                digest(b"manifest"),
                Some(digest(b"lock")),
                digest(b"metadata"),
                digest(b"cargo version"),
                digest(b"rustc -vV"),
                digest(b"build invocation"),
            ),
        )
    }

    fn unique_test_root(name: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};

        static NEXT: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "nara-owner-{name}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }
}
