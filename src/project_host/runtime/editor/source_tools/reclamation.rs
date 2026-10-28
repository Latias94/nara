//! Explicit reviewed disposal of finite managed artifacts, with durable intent and real results.

use std::{
    fmt,
    sync::{Arc, Mutex, TryLockError},
};

use nara_fs::{
    CapabilityRights, DirectoryCapability, DirectoryNamespaceLease, FileCapability,
    FileUnlinkReceipt, FsError, RelativePath, TrustMode,
};
use nara_tasks::{TaskHandle, TaskPoolKind, TaskReturnState, TaskSpawnRequest};
use nara_tooling::{
    EditorSourceToolArtifactBasis, EditorSourceToolArtifactError, EditorSourceToolArtifactOutcome,
    EditorSourceToolAttemptId, EditorSourceToolReclamationError as Error,
    EditorSourceToolReclamationMember as Member,
    EditorSourceToolReclamationMemberState as MemberState,
    EditorSourceToolReclamationOutcome as Outcome, EditorSourceToolReclamationPhase as Phase,
    EditorSourceToolReclamationRecord as Record,
    EditorSourceToolReclamationRecordPhase as RecordPhase, EditorSourceToolReclamationView,
    EditorSourceToolSelectionBasis,
};

use super::{EditorProjectSession, artifact_store};

#[cfg(all(test, windows))]
mod tests;

/// Exact held objects selected for disposal; preparation/drop never writes or schedules deletion.
pub struct PreparedEditorSourceToolReclamation {
    root: DirectoryCapability,
    directory: DirectoryCapability,
    directory_path: Arc<str>,
    artifact_basis: EditorSourceToolArtifactBasis,
    selection_basis: EditorSourceToolSelectionBasis,
    record: Record,
    files: Vec<Option<FileCapability>>,
    facts: nara_tooling::EditorSourceToolArtifact,
    creation_custody: bool,
    // Exact native namespace leases keep the reviewed parent chain from being renamed
    // between intent publication and per-file unlink. They do not freeze file contents.
    _namespace_custody: Vec<DirectoryNamespaceLease>,
}

impl fmt::Debug for PreparedEditorSourceToolReclamation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedEditorSourceToolReclamation")
            .field("record", &self.record)
            .field("creation_custody", &self.creation_custody)
            .finish_non_exhaustive()
    }
}

impl PreparedEditorSourceToolReclamation {
    #[must_use]
    pub const fn record(&self) -> &Record {
        &self.record
    }

    /// Continuously held creation facts, not recovered from an equal receipt or persisted ID.
    #[must_use]
    pub const fn creation_custody(&self) -> bool {
        self.creation_custody
    }
}

enum Stage {
    Intent {
        publication: EditorSourceToolAttemptId,
        prepared: PreparedEditorSourceToolReclamation,
    },
    Removing(PendingRemoval),
    Results {
        publication: EditorSourceToolAttemptId,
    },
}

#[derive(Default)]
pub(super) struct ReclamationState {
    view: EditorSourceToolReclamationView,
    stage: Option<Stage>,
    cancelled: bool,
    receipts: Vec<FileUnlinkReceipt>,
    removal_error: Option<Error>,
    facts: Option<nara_tooling::EditorSourceToolArtifact>,
    release_custody: bool,
    #[cfg(all(test, windows))]
    next_gate: Option<Arc<tests::RemovalGate>>,
}

impl ReclamationState {
    pub(super) fn fenced(&self) -> bool {
        self.stage.is_some() || self.view.record_publication_pending
    }

    pub(super) fn pending(&self) -> bool {
        self.stage.is_some()
    }

    pub(super) fn view(&self) -> EditorSourceToolReclamationView {
        let mut view = self.view.clone();
        view.physical_work_pending = self.pending();
        view
    }

    pub(super) fn cancel(&mut self) {
        match &self.stage {
            Some(Stage::Intent { .. }) => self.cancelled = true,
            Some(Stage::Removing(pending)) => {
                self.cancelled = true;
                pending.cancel();
            }
            Some(Stage::Results { .. }) | None => {}
        }
    }
}

impl EditorProjectSession {
    /// Prepares a new explicit whole-object disposal review. Record identity is correlation only.
    /// Captures declared files in the fixed store, without requiring undamaged executable bytes.
    pub fn prepare_source_tool_reclamation(
        &self,
        artifact_basis: &EditorSourceToolArtifactBasis,
        selection_basis: &EditorSourceToolSelectionBasis,
        project: DirectoryCapability,
        installation: &[u8; 32],
    ) -> Result<PreparedEditorSourceToolReclamation, Error> {
        self.require_reclamation_review(artifact_basis, selection_basis, installation)?;
        if project.options().rights() != CapabilityRights::ReadWrite
            || project.options().trust() == TrustMode::TrustedLocal
        {
            return Err(Error::Unsupported);
        }
        if !project
            .same_object(&self.source_project_root())
            .map_err(|_| Error::RootMismatch)?
        {
            return Err(Error::RootMismatch);
        }
        let entry = self
            .source_tools
            .artifacts
            .reclamation_entry(artifact_basis, installation)
            .map_err(artifact_error)?;
        if !artifact_store::validate_path(&entry.directory) {
            return Err(Error::Changed);
        }
        let directory = project
            .open_directory_exact(&relative(&entry.directory)?)
            .map_err(|_| Error::Read)?;
        let mut namespace_custody = Vec::new();
        let mut guarded_directories = Vec::new();
        retain_parent_chain(
            &project,
            &entry.directory,
            &mut guarded_directories,
            &mut namespace_custody,
        )?;
        let created = self.source_tools.artifacts.creation_custody(installation);
        let mut creation_custody = created
            .is_some_and(|created| directory.same_object(created.directory()).unwrap_or(false));
        let mut files = Vec::with_capacity(entry.members.len());
        let mut members = Vec::with_capacity(entry.members.len());
        for member in &entry.members {
            if let Some((parent, _)) = member.path.rsplit_once('/') {
                retain_parent_chain(
                    &project,
                    &format!("{}/{parent}", entry.directory),
                    &mut guarded_directories,
                    &mut namespace_custody,
                )?;
            }
            let (file, state) = match directory.open_file_exact(&relative(&member.path)?) {
                Ok(file) => {
                    creation_custody &= created
                        .and_then(|created| created.member_file(&member.path))
                        .is_some_and(|created| created.same_object(&file).unwrap_or(false));
                    (Some(file), MemberState::Pending)
                }
                Err(error) if missing(&error) => {
                    creation_custody = false;
                    (None, MemberState::Missing)
                }
                Err(_) => return Err(Error::Read),
            };
            files.push(file);
            members.push(Member {
                path: Arc::clone(&member.path),
                state,
            });
        }
        let mut id = [0; 32];
        self.require_reclamation_sources_unused(&files)?;
        getrandom::fill(&mut id).map_err(|_| Error::Limit)?;
        if id == [0; 32] {
            return Err(Error::Limit);
        }
        Ok(PreparedEditorSourceToolReclamation {
            root: project,
            directory,
            directory_path: Arc::clone(&entry.directory),
            artifact_basis: artifact_basis.clone(),
            selection_basis: selection_basis.clone(),
            record: Record {
                id,
                artifact: *installation,
                phase: RecordPhase::Intent,
                members,
            },
            files,
            creation_custody,
            facts: entry,
            _namespace_custody: namespace_custody,
        })
    }

    /// Confirms this exact review and publishes its intent. No unlink precedes accepted intent.
    pub fn confirm_source_tool_reclamation(
        &mut self,
        prepared: PreparedEditorSourceToolReclamation,
    ) -> Result<EditorSourceToolAttemptId, Error> {
        self.advance_source_tools();
        self.require_reclamation_review(
            &prepared.artifact_basis,
            &prepared.selection_basis,
            &prepared.record.artifact,
        )?;
        if !prepared
            .root
            .same_object(&self.source_project_root())
            .map_err(|_| Error::RootMismatch)?
        {
            return Err(Error::RootMismatch);
        }
        validate_original_objects(&prepared)?;
        self.require_reclamation_sources_unused(&prepared.files)?;
        let publication = self
            .publish_reclamation_record(&prepared.artifact_basis, &prepared.record)
            .map_err(artifact_error)?;
        let attempt = EditorSourceToolAttemptId::issue();
        #[cfg(all(test, windows))]
        let next_gate = self.source_tools.reclamation.next_gate.take();
        self.source_tools.reclamation = ReclamationState {
            view: EditorSourceToolReclamationView {
                attempt: Some(attempt.clone()),
                phase: Phase::PublishingIntent,
                record: Some(prepared.record.clone()),
                record_publication_pending: true,
                ..Default::default()
            },
            facts: Some(prepared.facts.clone()),
            release_custody: prepared.creation_custody,
            stage: Some(Stage::Intent {
                publication,
                prepared,
            }),
            cancelled: false,
            receipts: Vec::new(),
            removal_error: None,
            #[cfg(all(test, windows))]
            next_gate,
        };
        Ok(attempt)
    }

    #[must_use]
    pub fn source_tool_reclamation_view(&self) -> EditorSourceToolReclamationView {
        self.source_tools.reclamation.view()
    }

    pub fn cancel_source_tool_reclamation(
        &mut self,
        attempt: &EditorSourceToolAttemptId,
    ) -> Result<(), Error> {
        if self.source_tools.reclamation.view.attempt.as_ref() != Some(attempt) {
            return Err(Error::StaleBasis);
        }
        self.source_tools.reclamation.cancel();
        Ok(())
    }

    /// Republishes retained actual results after explicit artifact reconciliation; never deletes.
    pub fn retry_source_tool_reclamation_record(
        &mut self,
        attempt: &EditorSourceToolAttemptId,
        basis: &EditorSourceToolArtifactBasis,
    ) -> Result<(), Error> {
        if !self.accepts_new_work() {
            return Err(Error::Closing);
        }
        if self.source_tools.reclamation.view.attempt.as_ref() != Some(attempt) {
            return Err(Error::StaleBasis);
        }
        if self.source_tools.reclamation.stage.is_some() {
            return Err(Error::Busy);
        }
        if !self
            .source_tools
            .reclamation
            .view
            .record_publication_pending
        {
            return Err(Error::StaleBasis);
        }
        let record = self
            .source_tools
            .reclamation
            .view
            .record
            .clone()
            .ok_or(Error::StaleBasis)?;
        let expected = self
            .source_tools
            .reclamation
            .facts
            .as_ref()
            .ok_or(Error::StaleBasis)?;
        if !self
            .source_tools
            .artifacts
            .same_reclamation_facts(basis, expected)
            .map_err(artifact_error)?
        {
            return Err(Error::Changed);
        }
        let publication = self
            .publish_reclamation_record(basis, &record)
            .map_err(artifact_error)?;
        let state = &mut self.source_tools.reclamation;
        state.view.phase = Phase::PublishingResults;
        state.view.error = state.removal_error.clone();
        state.view.outcome = None;
        state.stage = Some(Stage::Results { publication });
        Ok(())
    }

    fn require_reclamation_review(
        &self,
        artifact_basis: &EditorSourceToolArtifactBasis,
        selection_basis: &EditorSourceToolSelectionBasis,
        installation: &[u8; 32],
    ) -> Result<(), Error> {
        if !self.accepts_new_work()
            || self.pending_workspace_intent.is_some()
            || self.schema_adoption_fenced()
        {
            return Err(Error::Closing);
        }
        if self.source_tools.reclamation.fenced() {
            return Err(Error::Busy);
        }
        let entry = self
            .source_tools
            .artifacts
            .reclamation_entry(artifact_basis, installation)
            .map_err(artifact_error)?;
        self.source_tools.selection.require_reclamation_absence(
            selection_basis,
            installation,
            &entry.directory,
            &self.source_project_root(),
        )?;
        self.require_reclamation_quiet()
    }

    fn require_reclamation_quiet(&self) -> Result<(), Error> {
        if self.source_tools.active.is_some()
            || self.source_tools.reviewed.is_some()
            || self.source_tools.schema_adoption_pending()
            || self.authoring_runtime.host_custody().has_cleanup()
            || self.persistence.has_pending_work()
            || self.pending_document_open.is_some()
        {
            return Err(Error::Busy);
        }
        if self.source_tools.process_cleanup_failed()
            || self
                .authoring_runtime
                .persistence_tasks()
                .has_cleanup_failure()
        {
            return Err(Error::TaskCleanupFailed);
        }
        self.require_reclamation_source_work_quiet()
    }

    fn require_reclamation_source_work_quiet(&self) -> Result<(), Error> {
        if self.project_source_index_work_pending()
            || self.image_import.has_pending_work()
            || self.image_import.schema_adoption_pending()
            || self.source_asset_import.schema_adoption_pending()
            || self.schema_owner_adoption.reclamation_blocked()
            || self.prefab_conversion.reclamation_blocked()
            || self.authoring_retirements.is_pending()
        {
            return Err(Error::Busy);
        }
        if self.authoring_retirements.failed() || self.authoring_runtime.has_close_failure() {
            return Err(Error::TaskCleanupFailed);
        }
        #[cfg(feature = "presentation-2d")]
        if self.scene_capture.has_pending_work() {
            return Err(Error::Busy);
        }
        if self.trusted_compiled().is_some_and(|lane| {
            !matches!(
                lane.runtime_owner,
                super::super::EditorRuntimeOwnerState::Empty
            ) || lane.runtime_host.has_runtime_custody()
                || lane.pending_control.is_some()
                || lane.pending_apply_changes.is_some()
        }) {
            return Err(Error::Busy);
        }
        #[cfg(any(feature = "presentation-2d", feature = "presentation-ui"))]
        if self
            .trusted_compiled()
            .is_some_and(|lane| lane.pending_step_presentation.is_some())
        {
            return Err(Error::Busy);
        }
        if matches!(
            self.project_content,
            nara_tooling::EditorProjectContentView::RefreshPending { .. }
        ) || self.external_play_request.is_some()
            || self
                .external_play_view()
                .is_some_and(|view| view.state() != nara_tooling::EditorExternalPlayState::Idle)
        {
            return Err(Error::Busy);
        }
        Ok(())
    }

    fn require_reclamation_sources_unused(
        &self,
        files: &[Option<FileCapability>],
    ) -> Result<(), Error> {
        // Validate each historical source set once, rather than rehashing the project for every
        // selected member. These are ephemeral held objects, not another source registry.
        let mut sources = Vec::new();
        match &self.execution {
            super::super::EditorExecutionLane::Authoring(project) => {
                project
                    .collect_reclamation_sources(&mut sources)
                    .map_err(|_| Error::Read)?;
                for file in files.iter().flatten() {
                    if project
                        .source_object_in_use(file)
                        .map_err(|_| Error::Read)?
                    {
                        return Err(Error::SourceOwned);
                    }
                }
            }
            super::super::EditorExecutionLane::TrustedCompiled(lane) => {
                let closure = lane.snapshot.content_closure();
                let root = closure
                    .project_root()
                    .strict_read_only()
                    .map_err(|_| Error::Read)?;
                for member in closure.members() {
                    let current = root
                        .open_file_exact(&relative(member.path().as_str())?)
                        .map_err(|_| Error::Read)?;
                    current
                        .verify_digest(member.digest(), member.byte_len())
                        .map_err(|_| Error::Read)?;
                    sources.push(current);
                }
            }
        }
        let root = self.source_project_root();
        self.image_import
            .collect_reclamation_sources(&root, &mut sources)
            .map_err(|_| Error::Read)?;
        #[cfg(feature = "presentation-2d")]
        self.scene_capture
            .collect_reclamation_sources(&mut sources)
            .map_err(|_| Error::Read)?;
        for file in files.iter().flatten() {
            self.require_unowned_artifact_object(file)
                .map_err(artifact_error)?;
            for source in &sources {
                if source.same_object(file).map_err(|_| Error::Read)? {
                    return Err(Error::SourceOwned);
                }
            }
            #[cfg(feature = "presentation-2d")]
            if self
                .scene_capture
                .source_object_in_use(file)
                .map_err(|_| Error::Read)?
            {
                return Err(Error::SourceOwned);
            }
        }
        Ok(())
    }

    pub(super) fn advance_source_tool_reclamation(&mut self) {
        let mut state = std::mem::take(&mut self.source_tools.reclamation);
        let stage = state.stage.take();
        match stage {
            Some(Stage::Intent {
                publication,
                prepared,
            }) => {
                match publication_result(self, &publication) {
                    None => {
                        state.stage = Some(Stage::Intent {
                            publication,
                            prepared,
                        })
                    }
                    Some(Err(error)) => {
                        // No deletion occurred. Retain the exact intent for observation; only
                        // accepted intent with successful return can advance into removal.
                        state.view.phase = Phase::Terminal;
                        state.view.error = Some(error.clone());
                        state.view.outcome = Some(Outcome::Failed(error));
                        state.view.record_publication_pending = false;
                    }
                    Some(Ok(())) => {
                        let validation = if state.cancelled {
                            Ok(())
                        } else {
                            self.source_tools
                                .selection
                                .require_reclamation_absence(
                                    &prepared.selection_basis,
                                    &prepared.record.artifact,
                                    &prepared.directory_path,
                                    &self.source_project_root(),
                                )
                                .and_then(|()| self.require_reclamation_quiet())
                                .and_then(|()| validate_original_objects(&prepared))
                                .and_then(|()| {
                                    self.require_reclamation_sources_unused(&prepared.files)
                                })
                        };
                        match validation {
                            Err(error) => {
                                state.removal_error = Some(error.clone());
                                state.view.error = Some(error);
                                state
                                    .view
                                    .record
                                    .as_mut()
                                    .expect("intent has a record")
                                    .phase = RecordPhase::Results;
                                begin_result_publication(self, &mut state);
                            }
                            Ok(()) => {
                                state.view.phase = Phase::Removing;
                                let pending = PendingRemoval::start(
                                    prepared,
                                    state.cancelled,
                                    self.authoring_runtime.persistence_tasks(),
                                    #[cfg(all(test, windows))]
                                    state.next_gate.take(),
                                );
                                state.stage = Some(Stage::Removing(pending));
                            }
                        }
                    }
                }
            }
            Some(Stage::Removing(mut pending)) => {
                if let Some(completed) = pending.poll() {
                    state.view.record = Some(completed.record);
                    state.receipts = completed.receipts;
                    state.cancelled |= completed.cancelled;
                    if let Some(error) = completed.error {
                        state.removal_error = Some(error.clone());
                        state.view.error = Some(error);
                    }
                    begin_result_publication(self, &mut state);
                } else {
                    state.stage = Some(Stage::Removing(pending));
                }
            }
            Some(Stage::Results { publication }) => match publication_result(self, &publication) {
                None => state.stage = Some(Stage::Results { publication }),
                Some(Err(error)) => {
                    state.view.phase = Phase::Terminal;
                    state.view.error = Some(error.clone());
                    state.view.outcome = Some(Outcome::Failed(error));
                    state.view.record_publication_pending = true;
                }
                Some(Ok(())) => {
                    let record = state.view.record.as_ref().expect("results retain a record");
                    if state.release_custody
                        && record
                            .members
                            .iter()
                            .all(|member| member.state == MemberState::Removed)
                    {
                        self.source_tools
                            .artifacts
                            .release_creation_custody(&record.artifact);
                    }
                    state.view.phase = Phase::Terminal;
                    state.view.record_publication_pending = false;
                    state.view.outcome = Some(if state.cancelled {
                        Outcome::Cancelled
                    } else if state.view.error.is_some()
                        || record.members.iter().any(|member| {
                            matches!(member.state, MemberState::Pending | MemberState::Failed)
                        })
                    {
                        Outcome::Partial
                    } else {
                        Outcome::Complete
                    });
                }
            },
            None => {}
        }
        self.source_tools.reclamation = state;
    }
}

fn begin_result_publication(session: &mut EditorProjectSession, state: &mut ReclamationState) {
    let record = state
        .view
        .record
        .as_ref()
        .expect("actual removal retains its record");
    let basis = session.source_tool_artifact_view().basis;
    state.view.record_publication_pending = true;
    state.view.phase = Phase::PublishingResults;
    match session.publish_reclamation_record(&basis, record) {
        Ok(publication) => state.stage = Some(Stage::Results { publication }),
        Err(error) => {
            let error = artifact_error(error);
            state.view.phase = Phase::Terminal;
            state.view.error = Some(error.clone());
            state.view.outcome = Some(Outcome::Failed(error));
        }
    }
}

fn publication_result(
    session: &EditorProjectSession,
    attempt: &EditorSourceToolAttemptId,
) -> Option<Result<(), Error>> {
    let view = session.source_tool_artifact_view();
    if view.physical_work_pending {
        return None;
    }
    let operation = view.operation.as_ref()?;
    if &operation.attempt != attempt {
        return Some(Err(Error::StaleBasis));
    }
    Some(match operation.outcome.as_ref()? {
        EditorSourceToolArtifactOutcome::ReclamationRecorded => Ok(()),
        EditorSourceToolArtifactOutcome::PublishedWithCleanupFailure => {
            Err(Error::TaskCleanupFailed)
        }
        EditorSourceToolArtifactOutcome::Failed(error) => Err(artifact_error(error.clone())),
        _ => Err(Error::Changed),
    })
}

fn validate_original_objects(prepared: &PreparedEditorSourceToolReclamation) -> Result<(), Error> {
    let current = prepared
        .root
        .open_directory_exact(&relative(&prepared.directory_path)?)
        .map_err(|_| Error::Changed)?;
    if !current
        .same_object(&prepared.directory)
        .map_err(|_| Error::Changed)?
    {
        return Err(Error::Changed);
    }
    for (member, expected) in prepared.record.members.iter().zip(&prepared.files) {
        match (current.open_file_exact(&relative(&member.path)?), expected) {
            (Ok(current), Some(expected))
                if current.same_object(expected).map_err(|_| Error::Changed)? => {}
            (Err(error), None) if missing(&error) => {}
            _ => return Err(Error::Changed),
        }
    }
    Ok(())
}

struct RemovalWork {
    prepared: Option<PreparedEditorSourceToolReclamation>,
    record: Record,
    receipts: Vec<FileUnlinkReceipt>,
    cancelled: bool,
    completed: bool,
    error: Option<Error>,
}

struct RemovalCompleted {
    record: Record,
    receipts: Vec<FileUnlinkReceipt>,
    cancelled: bool,
    error: Option<Error>,
}

struct PendingRemoval {
    task: Option<TaskHandle<()>>,
    rejected_return: TaskReturnState,
    work: Arc<Mutex<RemovalWork>>,
}

impl PendingRemoval {
    fn start(
        prepared: PreparedEditorSourceToolReclamation,
        cancelled: bool,
        pools: &nara_tasks::TaskPools,
        #[cfg(all(test, windows))] gate: Option<Arc<tests::RemovalGate>>,
    ) -> Self {
        let work = Arc::new(Mutex::new(RemovalWork {
            record: prepared.record.clone(),
            receipts: Vec::with_capacity(prepared.files.len()),
            prepared: Some(prepared),
            cancelled,
            completed: false,
            error: None,
        }));
        let retained = Arc::clone(&work);
        let spawned = pools
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, super::TOOL_DOMAIN),
                move |context| {
                    let mut work = retained.lock().unwrap_or_else(|error| error.into_inner());
                    let prepared = work
                        .prepared
                        .take()
                        .expect("one removal consumes the original review");
                    for (index, file) in prepared.files.iter().enumerate() {
                        if work.cancelled || context.is_cancelled() {
                            work.cancelled = true;
                            break;
                        }
                        let Some(file) = file else { continue };
                        let directory = match prepared.root.open_directory_exact(
                            &relative(&prepared.directory_path).expect("admitted directory"),
                        ) {
                            Ok(directory)
                                if directory.same_object(&prepared.directory).unwrap_or(false) =>
                            {
                                directory
                            }
                            _ => {
                                work.error = Some(Error::Changed);
                                break;
                            }
                        };
                        let path =
                            relative(&work.record.members[index].path).expect("admitted member");
                        match directory.open_file_exact(&path) {
                            Err(error) if missing(&error) => {
                                work.record.members[index].state = MemberState::Missing;
                                continue;
                            }
                            Ok(current) if current.same_object(file).unwrap_or(false) => {}
                            _ => {
                                work.record.members[index].state = MemberState::Failed;
                                work.error = Some(Error::Changed);
                                break;
                            }
                        }
                        let full_path = relative(&format!(
                            "{}/{}",
                            prepared.directory_path, work.record.members[index].path
                        ))
                        .expect("admitted artifact member path");
                        match prepared.root.unlink_file(&full_path, file) {
                            Ok(receipt) => {
                                work.receipts.push(receipt);
                                work.record.members[index].state = MemberState::Removed;
                            }
                            Err(error) => {
                                work.record.members[index].state = MemberState::Failed;
                                work.error = Some(match error {
                                    FsError::Unsupported { .. } | FsError::Unproven { .. } => {
                                        Error::Unsupported
                                    }
                                    _ => Error::Write,
                                });
                            }
                        }
                        #[cfg(all(test, windows))]
                        if let Some(gate) = &gate {
                            gate.after_member(index);
                        }
                        if work.error.is_some() {
                            break;
                        }
                    }
                    work.record.phase = RecordPhase::Results;
                    work.completed = true;
                },
            )
            .into_handle();
        let (task, rejected_return) = match spawned {
            Ok(task) => (Some(task), TaskReturnState::Returned),
            Err(error) => (None, error.return_state),
        };
        Self {
            task,
            rejected_return,
            work,
        }
    }

    fn cancel(&self) {
        if let Some(task) = &self.task {
            task.cancel();
        }
    }

    fn poll(&mut self) -> Option<RemovalCompleted> {
        let returned = self
            .task
            .as_ref()
            .map_or(self.rejected_return, |task| task.return_observer().state());
        if returned == TaskReturnState::Pending {
            return None;
        }
        let mut work = match self.work.try_lock() {
            Ok(work) => work,
            Err(TryLockError::WouldBlock) => return None,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        if returned == TaskReturnState::CleanupFailed {
            work.error = Some(Error::TaskCleanupFailed);
        } else if !work.completed {
            work.error = Some(if self.task.is_none() {
                Error::TaskRejected
            } else {
                Error::RequiresReconcile
            });
        }
        work.record.phase = RecordPhase::Results;
        work.prepared = None;
        Some(RemovalCompleted {
            record: work.record.clone(),
            receipts: std::mem::take(&mut work.receipts),
            cancelled: work.cancelled,
            error: work.error.clone(),
        })
    }
}

fn relative(value: &str) -> Result<RelativePath, Error> {
    RelativePath::new(value).map_err(|_| Error::Changed)
}
fn retain_parent_chain(
    root: &DirectoryCapability,
    path: &str,
    guarded: &mut Vec<nara_fs::FileIdentity>,
    leases: &mut Vec<DirectoryNamespaceLease>,
) -> Result<(), Error> {
    let path = relative(path)?;
    for end in 1..=path.len() {
        let part = RelativePath::from_components(path.components()[..end].iter().cloned())
            .map_err(|_| Error::Changed)?;
        let directory = root.open_directory_exact(&part).map_err(|_| Error::Read)?;
        if guarded.contains(&directory.identity()) {
            continue;
        }
        let lease = directory
            .acquire_namespace_lease()
            .map_err(|error| match error {
                FsError::Unsupported { .. } | FsError::Unproven { .. } => Error::Unsupported,
                _ => Error::Busy,
            })?;
        guarded.push(directory.identity());
        leases.push(lease);
    }
    Ok(())
}
fn missing(error: &FsError) -> bool {
    matches!(error, FsError::Io { source, .. } if source.kind() == std::io::ErrorKind::NotFound)
}

fn artifact_error(error: EditorSourceToolArtifactError) -> Error {
    match error {
        EditorSourceToolArtifactError::StaleBasis => Error::StaleBasis,
        EditorSourceToolArtifactError::Busy => Error::Busy,
        EditorSourceToolArtifactError::Closing => Error::Closing,
        EditorSourceToolArtifactError::RootMismatch => Error::RootMismatch,
        EditorSourceToolArtifactError::Unregistered => Error::Unregistered,
        EditorSourceToolArtifactError::SourceOwned => Error::SourceOwned,
        EditorSourceToolArtifactError::Changed => Error::Changed,
        EditorSourceToolArtifactError::TaskCleanupFailed => Error::TaskCleanupFailed,
        EditorSourceToolArtifactError::TaskRejected => Error::TaskRejected,
        EditorSourceToolArtifactError::RequiresReconcile => Error::RequiresReconcile,
        EditorSourceToolArtifactError::TooLarge => Error::Limit,
        _ => Error::Write,
    }
}
