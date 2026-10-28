//! Explicit finite template copies use the existing Tasks and publication owners.

use std::{
    fmt,
    sync::{Arc, Mutex, TryLockError},
};

use nara_fs::{
    CapabilityRights, ContentDigest, DirectoryCapability, FsError, RelativeComponent, RelativePath,
    TrustMode,
};
use nara_tasks::{TaskHandle, TaskPoolKind, TaskReturnState, TaskSpawnRequest};
use nara_tooling::{
    EditorSourceToolAttemptId, EditorSourceToolInstalledEntry, EditorSourceToolSelectionBasis,
    EditorSourceToolTemplateImportError as Error, EditorSourceToolTemplateImportOutcome as Outcome,
    EditorSourceToolTemplateImportView,
};

use super::{
    EditorProjectSession, EditorSourceToolInstallationInspection,
    EditorSourceToolReleaseMemberKind, artifact_store,
};
use crate::project_host::{
    file_publication::{
        FilePublicationEvidence, FilePublicationHost, FilePublicationOutcome,
        FilePublicationRejection,
    },
    runtime::editor::persistence::RetainedHost,
};

const BYTE_LIMIT: u64 = 16 * 1024 * 1024;

mod prefab;
pub use prefab::{
    EditorSourceToolPrefabTemplateFile, EditorSourceToolPrefabTemplateImportRequest,
    EditorSourceToolPrefabTemplatePublication, PreparedEditorSourceToolPrefabTemplateImport,
};

#[cfg(all(test, windows))]
mod tests;

/// Captured bytes and original observations; preparation/drop never creates a project file.
pub struct PreparedEditorSourceToolTemplateImport {
    root: DirectoryCapability,
    basis: EditorSourceToolSelectionBasis,
    entry: EditorSourceToolInstalledEntry,
    source_directory: DirectoryCapability,
    destination: Arc<str>,
    parent: DirectoryCapability,
    target: RelativeComponent,
    bytes: Arc<[u8]>,
    digest: ContentDigest,
}

impl fmt::Debug for PreparedEditorSourceToolTemplateImport {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedEditorSourceToolTemplateImport")
            .field("installation", &self.entry.installed)
            .field("destination", &self.destination)
            .field("digest", &self.digest)
            .finish_non_exhaustive()
    }
}

impl PreparedEditorSourceToolTemplateImport {
    #[must_use]
    pub fn destination(&self) -> &str {
        &self.destination
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }
}

#[derive(Default)]
pub(super) struct TemplateImportState {
    view: EditorSourceToolTemplateImportView,
    pending: Option<Pending>,
    host: Option<RetainedHost<FilePublicationHost>>,
    evidence: Option<FilePublicationEvidence>,
    native_publication: Option<EditorSourceToolPrefabTemplatePublication>,
    native_pending: bool,
    #[cfg(all(test, windows))]
    next_gate: Option<Arc<tests::IoGate>>,
}

impl TemplateImportState {
    pub(super) fn native_pending(&self) -> bool {
        self.native_pending && self.pending.is_some()
    }

    pub(super) fn pending(&self) -> bool {
        self.pending.is_some() || self.host.as_ref().is_some_and(|host| !host.is_retired())
    }

    pub(super) fn view(&self) -> EditorSourceToolTemplateImportView {
        let mut view = self.view.clone();
        view.physical_work_pending = self.pending();
        view
    }

    pub(super) fn publication_host(&self) -> Option<&RetainedHost<FilePublicationHost>> {
        self.host.as_ref()
    }

    pub(super) fn cancel(&self) {
        if let Some(task) = self
            .pending
            .as_ref()
            .and_then(|pending| pending.task.as_ref())
        {
            task.cancel();
        }
    }
}

impl EditorProjectSession {
    /// Captures one declared UTF-8 .txt member without writes or document publication.
    /// Performs finite code-free IO; prepare outside frontend painting.
    pub fn prepare_source_tool_template_import(
        &self,
        basis: &EditorSourceToolSelectionBasis,
        project: DirectoryCapability,
        installation: &[u8; 32],
        member: &str,
        destination: &str,
    ) -> Result<PreparedEditorSourceToolTemplateImport, Error> {
        self.require_template_import(basis)?;
        if project.options().rights() != CapabilityRights::ReadWrite
            || project.options().trust() == TrustMode::TrustedLocal
        {
            return Err(Error::Capability);
        }
        if !project
            .same_object(&self.source_project_root())
            .map_err(|_| Error::RootMismatch)?
        {
            return Err(Error::RootMismatch);
        }
        validate_text_path(member)?;
        validate_destination(destination)?;
        let entry = self
            .source_tool_selection_view()
            .installed
            .into_iter()
            .find(|entry| &entry.installed.installation == installation)
            .ok_or(Error::Installation)?;
        if !artifact_store::validate_path(&entry.installed.directory) {
            return Err(Error::Installation);
        }
        let read_root = project.strict_read_only().map_err(|_| Error::Capability)?;
        let directory = read_root
            .open_directory_exact(&relative(&entry.installed.directory)?)
            .map_err(|_| Error::Installation)?;
        let inspection =
            EditorSourceToolInstallationInspection::read(&directory, entry.installed.receipt)
                .map_err(|_| Error::Installation)?;
        if inspection.receipt().installation_id() != installation
            || inspection.release().manifest_digest() != entry.installed.manifest
            || inspection.release().package() != entry.installed.package.as_ref()
        {
            return Err(Error::Installation);
        }
        let member = inspection
            .release()
            .members()
            .iter()
            .find(|candidate| {
                candidate.path() == member
                    && candidate.kind() == EditorSourceToolReleaseMemberKind::Content
            })
            .ok_or(Error::UnsupportedTemplate)?;
        if member.bytes().len() as u64 > BYTE_LIMIT || std::str::from_utf8(member.bytes()).is_err()
        {
            return Err(Error::UnsupportedTemplate);
        }
        let (parent, target) = exact_destination_parent(&project, destination)?;
        ensure_absent(&parent, &target)?;
        Ok(PreparedEditorSourceToolTemplateImport {
            root: project,
            basis: basis.clone(),
            entry,
            source_directory: directory,
            destination: destination.into(),
            parent,
            target,
            bytes: member.bytes().into(),
            digest: member.digest(),
        })
    }

    /// Confirms the original selection and captured copy. Existing targets are never replaced.
    pub fn commit_source_tool_template_import(
        &mut self,
        prepared: PreparedEditorSourceToolTemplateImport,
    ) -> Result<EditorSourceToolAttemptId, Error> {
        self.advance_source_tools();
        self.require_template_import(&prepared.basis)?;
        if !prepared
            .root
            .same_object(&self.source_project_root())
            .map_err(|_| Error::RootMismatch)?
        {
            return Err(Error::RootMismatch);
        }
        if !self
            .source_tool_selection_view()
            .installed
            .iter()
            .any(|entry| entry == &prepared.entry)
        {
            return Err(Error::StaleBasis);
        }
        let host = FilePublicationHost::bind_missing(
            &prepared.parent,
            prepared.target.to_str().ok_or(Error::InvalidPath)?,
            BYTE_LIMIT,
        )
        .map_err(|_| Error::Write)?;
        let host = self
            .authoring_runtime
            .host_custody()
            .retain_publication(host)
            .ok_or(Error::Closing)?;
        self.start_template_import(
            ImportRequest::Text {
                prepared,
                publisher: host.clone(),
            },
            Some(host),
        )
    }

    /// Confirms the fixed native copy without allocating new source identities or recapturing bytes.
    pub fn commit_source_tool_prefab_template_import(
        &mut self,
        prepared: PreparedEditorSourceToolPrefabTemplateImport,
    ) -> Result<EditorSourceToolAttemptId, Error> {
        self.advance_source_tools();
        prefab::validate_session(self, &prepared)?;
        if !self
            .source_tool_selection_view()
            .installed
            .contains(&prepared.installation)
        {
            return Err(Error::StaleBasis);
        }
        self.start_template_import(ImportRequest::Prefab(prepared), None)
    }

    fn start_template_import(
        &mut self,
        prepared: ImportRequest,
        host: Option<RetainedHost<FilePublicationHost>>,
    ) -> Result<EditorSourceToolAttemptId, Error> {
        let native_pending = matches!(&prepared, ImportRequest::Prefab(_));
        let work = Arc::new(Mutex::new(Work {
            request: Some(prepared),
            started: false,
            result: None,
        }));
        let retained = Arc::clone(&work);
        #[cfg(all(test, windows))]
        let gate = self.source_tools.template_import.next_gate.take();
        let spawned = self
            .authoring_runtime
            .persistence_tasks()
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, super::TOOL_DOMAIN),
                move |context| {
                    let mut work = retained.lock().unwrap_or_else(|error| error.into_inner());
                    if context.is_cancelled() {
                        return;
                    }
                    let request = work
                        .request
                        .take()
                        .expect("one import consumes one candidate");
                    work.started = true;
                    work.result = Some(match request {
                        ImportRequest::Text {
                            prepared,
                            publisher,
                        } => publish(&prepared, &publisher).map(|(outcome, evidence)| {
                            CompletedImport {
                                outcome,
                                evidence,
                                native: None,
                            }
                        }),
                        ImportRequest::Prefab(prepared) => {
                            prefab::publish(&prepared, || context.is_cancelled()).map(
                                |(outcome, native)| CompletedImport {
                                    outcome,
                                    evidence: None,
                                    native: Some(native),
                                },
                            )
                        }
                    });
                    #[cfg(all(test, windows))]
                    if let Some(gate) = gate {
                        gate.wait_after_io();
                    }
                },
            )
            .into_handle();
        let (task, rejected_return) = match spawned {
            Ok(task) => (Some(task), TaskReturnState::Returned),
            Err(rejected) => (None, rejected.return_state),
        };
        let attempt = EditorSourceToolAttemptId::issue();
        let state = &mut self.source_tools.template_import;
        state.evidence = None;
        state.native_publication = None;
        state.native_pending = native_pending;
        state.view = EditorSourceToolTemplateImportView {
            attempt: Some(attempt.clone()),
            ..Default::default()
        };
        state.host = host;
        state.pending = Some(Pending {
            task,
            rejected_return,
            work,
        });
        Ok(attempt)
    }

    #[must_use]
    pub fn source_tool_template_import_view(&self) -> EditorSourceToolTemplateImportView {
        self.source_tools.template_import.view()
    }

    /// Borrows the actual native directory result after its task has physically returned.
    #[must_use]
    pub fn source_tool_prefab_template_publication(
        &self,
    ) -> Option<&EditorSourceToolPrefabTemplatePublication> {
        self.source_tools
            .template_import
            .native_publication
            .as_ref()
    }

    /// Transfers the last native publication's actual custody for explicit recovery or inspection.
    pub fn take_source_tool_prefab_template_publication(
        &mut self,
        attempt: &EditorSourceToolAttemptId,
    ) -> Result<EditorSourceToolPrefabTemplatePublication, Error> {
        let state = &mut self.source_tools.template_import;
        if state.view.attempt.as_ref() != Some(attempt) {
            return Err(Error::StaleBasis);
        }
        if state.pending() {
            return Err(Error::Busy);
        }
        state
            .native_publication
            .take()
            .ok_or(Error::RequiresReconcile)
    }

    /// Revokes future work for this exact attempt; an entered publication retains its real result.
    pub fn cancel_source_tool_template_import(
        &mut self,
        attempt: &EditorSourceToolAttemptId,
    ) -> Result<(), Error> {
        if self.source_tools.template_import.view.attempt.as_ref() != Some(attempt) {
            return Err(Error::StaleBasis);
        }
        self.source_tools.template_import.cancel();
        Ok(())
    }

    fn require_template_import(&self, basis: &EditorSourceToolSelectionBasis) -> Result<(), Error> {
        if !self.accepts_new_work()
            || self.pending_workspace_intent.is_some()
            || self.schema_adoption_fenced()
        {
            return Err(Error::Closing);
        }
        if &self.source_tool_selection_view().basis != basis {
            return Err(Error::StaleBasis);
        }
        if self.source_tools.template_import.pending()
            || self.source_tools.reclamation_fenced()
            || self.source_tools.selection.schema_adoption_pending()
            || self.source_tools.probe.is_some()
        {
            return Err(Error::Busy);
        }
        if self
            .source_tools
            .template_import
            .native_publication
            .as_ref()
            .is_some_and(|publication| publication.error.is_some())
        {
            return Err(Error::RequiresReconcile);
        }
        Ok(())
    }

    pub(super) fn advance_source_tool_template_import(&mut self) {
        let state = &mut self.source_tools.template_import;
        let mut native_returned = false;
        if let Some(pending) = state.pending.as_mut() {
            if let Some((result, returned)) = pending.poll() {
                state.pending = None;
                state.view.cleanup_failed = returned == TaskReturnState::CleanupFailed;
                match result {
                    Ok(completed) => {
                        native_returned = completed.native.is_some();
                        state.view.outcome = Some(completed.outcome);
                        state.evidence = completed.evidence;
                        state.native_publication = completed.native;
                    }
                    Err(error) => state.view.outcome = Some(Outcome::Failed(error)),
                }
            }
        }
        if state.pending.is_none() && state.host.as_ref().is_some_and(RetainedHost::retire) {
            state.host = None;
        }
        if native_returned && !self.closing {
            self.request_project_source_index_refresh();
        }
    }
}

type ImportResult = Result<(Outcome, Option<FilePublicationEvidence>), Error>;

enum ImportRequest {
    Text {
        prepared: PreparedEditorSourceToolTemplateImport,
        publisher: RetainedHost<FilePublicationHost>,
    },
    Prefab(PreparedEditorSourceToolPrefabTemplateImport),
}

struct CompletedImport {
    outcome: Outcome,
    evidence: Option<FilePublicationEvidence>,
    native: Option<EditorSourceToolPrefabTemplatePublication>,
}

struct Work {
    request: Option<ImportRequest>,
    started: bool,
    result: Option<Result<CompletedImport, Error>>,
}

struct Pending {
    task: Option<TaskHandle<()>>,
    rejected_return: TaskReturnState,
    work: Arc<Mutex<Work>>,
}

impl Pending {
    fn poll(&mut self) -> Option<(Result<CompletedImport, Error>, TaskReturnState)> {
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
        let result = work.result.take().unwrap_or_else(|| {
            if work.started || returned == TaskReturnState::CleanupFailed {
                Err(Error::RequiresReconcile)
            } else if self.task.is_none() {
                Err(Error::TaskRejected)
            } else {
                Ok(CompletedImport {
                    outcome: Outcome::Cancelled,
                    evidence: None,
                    native: None,
                })
            }
        });
        Some((result, returned))
    }
}

fn publish(
    request: &PreparedEditorSourceToolTemplateImport,
    publisher: &RetainedHost<FilePublicationHost>,
) -> ImportResult {
    let directory = request
        .root
        .open_directory_exact(&relative(&request.entry.installed.directory)?)
        .map_err(|_| Error::Changed)?;
    if !directory
        .same_object(&request.source_directory)
        .map_err(|_| Error::Changed)?
    {
        return Err(Error::Changed);
    }
    validate_parent(request)?;
    ensure_absent(&request.parent, &request.target)?;
    publisher
        .with_worker(|host| match host.publish(&request.bytes) {
            FilePublicationOutcome::Published(accepted) => {
                let (evidence, source) = accepted.into_parts();
                let admitted = validate_parent(request).is_ok()
                    && request
                        .parent
                        .open_child_file(&request.target)
                        .is_ok_and(|current| {
                            current.identity() == source.identity()
                                && current.verify_digest(request.digest, BYTE_LIMIT).is_ok()
                        });
                let outcome = if admitted {
                    Outcome::Imported {
                        destination: Arc::clone(&request.destination),
                        digest: request.digest,
                        parent_directory_sync: evidence.parent_directory_sync(),
                    }
                } else {
                    Outcome::PublishedWithoutAdmission {
                        destination: Arc::clone(&request.destination),
                        digest: request.digest,
                        parent_directory_sync: evidence.parent_directory_sync(),
                    }
                };
                host.commit_published(source);
                Ok((outcome, Some(evidence)))
            }
            FilePublicationOutcome::Uncertain(evidence) => Ok((
                Outcome::PublicationUncertain {
                    destination: Arc::clone(&request.destination),
                    digest: request.digest,
                    parent_directory_sync: evidence.parent_directory_sync(),
                },
                Some(evidence),
            )),
            FilePublicationOutcome::Rejected(FilePublicationRejection::TargetChanged) => {
                Err(Error::TargetExists)
            }
            FilePublicationOutcome::Rejected(FilePublicationRejection::RequiresReconcile) => {
                Err(Error::RequiresReconcile)
            }
            FilePublicationOutcome::Rejected(_) | FilePublicationOutcome::Failed(_) => {
                Err(Error::Write)
            }
        })
        .ok_or(Error::Closing)?
}

fn validate_parent(request: &PreparedEditorSourceToolTemplateImport) -> Result<(), Error> {
    let (parent, target) = exact_destination_parent(&request.root, &request.destination)
        .map_err(|_| Error::Changed)?;
    if target != request.target
        || !parent
            .same_object(&request.parent)
            .map_err(|_| Error::Changed)?
    {
        return Err(Error::Changed);
    }
    Ok(())
}

fn ensure_absent(parent: &DirectoryCapability, target: &RelativeComponent) -> Result<(), Error> {
    match parent.open_child_file(target) {
        Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Ok(_) | Err(FsError::NotRegularFile) => Err(Error::TargetExists),
        Err(_) => Err(Error::Read),
    }
}

fn relative(path: &str) -> Result<RelativePath, Error> {
    RelativePath::new(path).map_err(|_| Error::InvalidPath)
}

fn exact_destination_parent(
    root: &DirectoryCapability,
    destination: &str,
) -> Result<(DirectoryCapability, RelativeComponent), Error> {
    let path = relative(destination)?;
    let (target, components) = path.components().split_last().ok_or(Error::InvalidPath)?;
    let parent = if components.is_empty() {
        root.share()
    } else {
        let path = RelativePath::from_components(components.iter().cloned())
            .map_err(|_| Error::InvalidPath)?;
        root.open_directory_exact(&path)
            .map_err(|_| Error::InvalidPath)?
    };
    Ok((parent, target.clone()))
}

fn validate_text_path(path: &str) -> Result<(), Error> {
    if path.len() > 512
        || path.split('/').count() > 8
        || !path.is_ascii()
        || path.contains('\\')
        || path.split('/').any(str::is_empty)
    {
        return Err(Error::InvalidPath);
    }
    relative(path)?;
    if !path.ends_with(".txt") {
        return Err(Error::UnsupportedTemplate);
    }
    Ok(())
}

fn validate_destination(path: &str) -> Result<(), Error> {
    validate_text_path(path)?;
    if path
        .get(..6)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(".nara/"))
    {
        return Err(Error::InvalidPath);
    }
    Ok(())
}
