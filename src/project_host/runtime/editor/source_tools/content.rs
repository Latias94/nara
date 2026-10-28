//! Durable protection facts retain their own authority, independently of installed selection.

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::{Arc, Mutex, TryLockError},
};

use nara_fs::{DirectoryCapability, FsError, RelativePath};
use nara_tasks::{TaskHandle, TaskPoolKind, TaskPools, TaskReturnState, TaskSpawnRequest};
use nara_tooling::{
    EditorSourceToolAttemptId, EditorSourceToolContentAttempt, EditorSourceToolContentBasis,
    EditorSourceToolContentError as Error, EditorSourceToolContentInstallation,
    EditorSourceToolContentMember, EditorSourceToolContentOutcome as Outcome,
    EditorSourceToolContentPhase as Phase, EditorSourceToolContentView,
    EditorSourceToolInstalledSelection,
};

use super::{
    EditorProjectSession, EditorSourceToolInstallation, EditorSourceToolReleaseMemberKind,
};
use crate::project_host::{
    file_publication::{
        FilePublicationEvidence, FilePublicationHost, FilePublicationOutcome,
        FilePublicationReadSource, FilePublicationRejection,
    },
    runtime::editor::persistence::{HostCustody, RetainedHost},
};

mod record;
mod store;
#[cfg(all(test, windows))]
mod tests;

pub(in crate::project_host::runtime::editor) const CONTENT_PATH: &str = store::PATH;

/// One original-basis protection successor. Preparation and dropping it perform no writes.
///
/// This never authorizes file deletion, graph mutation, source edits or executable activation.
pub struct PreparedEditorSourceToolContent {
    root: DirectoryCapability,
    basis: EditorSourceToolContentBasis,
    ledger: record::Ledger,
    bytes: Vec<u8>,
    creation_custody: BTreeSet<[u8; 32]>,
    created: Option<Arc<EditorSourceToolInstallation>>,
    outcome: Outcome,
}

impl fmt::Debug for PreparedEditorSourceToolContent {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedEditorSourceToolContent")
            .field("installations", &self.ledger.installations.len())
            .field("creation_custody", &self.creation_custody.len())
            .field("outcome", &self.outcome)
            .finish_non_exhaustive()
    }
}

impl PreparedEditorSourceToolContent {
    /// Projects the exact candidate that confirmation will publish.
    #[must_use]
    pub fn installations(&self) -> Vec<EditorSourceToolContentInstallation> {
        project_ledger(&self.ledger, |id| self.creation_custody.contains(id))
    }
}

pub(super) struct ContentState {
    basis: EditorSourceToolContentBasis,
    ledger: record::Ledger,
    created: BTreeMap<[u8; 32], Arc<EditorSourceToolInstallation>>,
    host: Option<RetainedHost<FilePublicationHost>>,
    pending: Option<Pending>,
    phase: Phase,
    operation: Option<EditorSourceToolContentAttempt>,
    error: Option<Error>,
    evidence: Option<FilePublicationEvidence>,
    protection_publication_pending: bool,
    #[cfg(all(test, windows))]
    next_io_gate: Option<Arc<tests::IoGate>>,
}

impl Default for ContentState {
    fn default() -> Self {
        Self {
            basis: EditorSourceToolContentBasis::issue(),
            ledger: record::Ledger::default(),
            created: BTreeMap::new(),
            host: None,
            pending: None,
            phase: Phase::Loading,
            operation: None,
            error: None,
            evidence: None,
            protection_publication_pending: false,
            #[cfg(all(test, windows))]
            next_io_gate: None,
        }
    }
}

impl ContentState {
    pub(super) fn view(&self) -> EditorSourceToolContentView {
        EditorSourceToolContentView {
            basis: self.basis.clone(),
            installations: project_ledger(&self.ledger, |id| self.created.contains_key(id)),
            phase: self.phase,
            operation: self.operation.clone(),
            error: self.error.clone(),
            protection_publication_pending: self.protection_publication_pending,
            physical_work_pending: self.pending(),
        }
    }

    pub(super) fn pending(&self) -> bool {
        self.pending.is_some()
            || self
                .host
                .as_ref()
                .is_some_and(|host| host.try_with(|host| host.has_cleanup()).unwrap_or(true))
    }

    pub(super) fn read_source(&self) -> Result<Option<FilePublicationReadSource>, ()> {
        self.host
            .as_ref()
            .map(|host| host.try_with(|host| host.read_source()).ok_or(()))
            .transpose()
    }

    fn require_basis(&self, basis: &EditorSourceToolContentBasis) -> Result<(), Error> {
        if basis != &self.basis {
            return Err(Error::StaleBasis);
        }
        if self.pending() {
            return Err(Error::Busy);
        }
        match self.phase {
            Phase::Idle => Ok(()),
            // Only a successful read of weaker protection admits repair; a failed publish needs
            // another explicit read before any candidate may use its source binding.
            Phase::RequiresReconcile
                if self.protection_publication_pending && self.error.is_none() =>
            {
                Ok(())
            }
            Phase::RequiresReconcile => Err(Error::RequiresReconcile),
            Phase::Unavailable => Err(self.error.clone().unwrap_or(Error::Read)),
            _ => Err(Error::Busy),
        }
    }

    fn prepare_candidate(
        &self,
        root: DirectoryCapability,
        ledger: record::Ledger,
        created: Option<Arc<EditorSourceToolInstallation>>,
        outcome: Outcome,
    ) -> Result<PreparedEditorSourceToolContent, Error> {
        let bytes = record::encode(&ledger)?;
        let mut creation_custody: BTreeSet<_> = self.created.keys().copied().collect();
        if let Some(created) = &created {
            creation_custody.insert(*created.receipt().installation_id());
        }
        Ok(PreparedEditorSourceToolContent {
            root,
            basis: self.basis.clone(),
            ledger,
            bytes,
            creation_custody,
            created,
            outcome,
        })
    }

    fn finish(&mut self, attempt: EditorSourceToolAttemptId, outcome: Outcome) {
        self.basis = EditorSourceToolContentBasis::issue();
        self.error = match &outcome {
            Outcome::Failed(error) => Some(error.clone()),
            _ => None,
        };
        self.operation = Some(EditorSourceToolContentAttempt {
            attempt,
            outcome: Some(outcome),
        });
    }
}

fn project_ledger(
    ledger: &record::Ledger,
    has_creation_custody: impl Fn(&[u8; 32]) -> bool,
) -> Vec<EditorSourceToolContentInstallation> {
    ledger
        .installations
        .iter()
        .map(|installation| EditorSourceToolContentInstallation {
            installed: installation.installed.clone(),
            members: installation
                .members
                .iter()
                .map(|member| EditorSourceToolContentMember {
                    path: Arc::clone(&member.path),
                    role: member.role,
                    baseline: member.baseline,
                    modified: member.modified,
                    adopted: member.adopted,
                    observation: member.observation,
                })
                .collect(),
            creation_custody: has_creation_custody(&installation.installed.installation),
        })
        .collect()
}

impl EditorProjectSession {
    pub(super) fn initialize_source_tool_content(&mut self) {
        self.start_content_work(Request::Observe, None, None, false);
    }

    /// Observes the last admitted ledger without I/O or recovery of creation/deletion authority.
    #[must_use]
    pub fn source_tool_content_view(&self) -> EditorSourceToolContentView {
        self.source_tools.content.view()
    }

    /// Transfers a share of actual created-file custody into an original-basis ledger candidate.
    ///
    /// Performs bounded code-free I/O; call from Host preparation, never painting. The caller may
    /// retain its Arc across refusal. Modified members do not require the complete release to be
    /// valid again. A later commit publishes protection facts only.
    pub fn prepare_source_tool_content_management(
        &self,
        basis: &EditorSourceToolContentBasis,
        directory: &str,
        installation: Arc<EditorSourceToolInstallation>,
    ) -> Result<PreparedEditorSourceToolContent, Error> {
        self.require_content_preparation(basis)?;
        let root = self.source_project_root();
        let installed = EditorSourceToolInstalledSelection {
            package: Arc::from(installation.package()),
            directory: Arc::from(directory),
            installation: *installation.receipt().installation_id(),
            receipt: installation.receipt_digest(),
            manifest: installation.receipt().manifest_digest(),
        };
        let entry = record::Installation {
            installed,
            members: installation
                .members()
                .iter()
                .map(|member| record::Member {
                    path: Arc::from(member.path()),
                    role: member_role(member.kind()),
                    baseline: member.digest(),
                    modified: false,
                    adopted: false,
                    observation: record::Observation::Present,
                })
                .collect(),
        };
        let id = entry.installed.installation;
        let mut ledger = self
            .source_tools
            .content
            .ledger
            .register_installation(entry)?;
        let path = RelativePath::new(directory).map_err(|_| Error::InvalidPath)?;
        let current = root.open_directory_exact(&path).map_err(|_| Error::Read)?;
        if !current
            .same_object(installation.directory())
            .map_err(|_| Error::RootMismatch)?
        {
            return Err(Error::RootMismatch);
        }
        let entry = ledger
            .installations
            .iter_mut()
            .find(|entry| entry.installed.installation == id)
            .expect("registered installation exists");
        observe_members(&current, entry, Some(&installation))?;
        self.source_tools.content.prepare_candidate(
            root,
            ledger,
            Some(installation),
            Outcome::Managed,
        )
    }

    /// Prepares explicit project ownership for selected declared Content members.
    ///
    /// Adoption never clears observed modification, changes source/history or follows an imported
    /// copy's origin. Unknown members and executable/metadata members cannot be adopted as samples.
    pub fn prepare_source_tool_content_adoption(
        &self,
        basis: &EditorSourceToolContentBasis,
        installation: &[u8; 32],
        paths: &[&str],
    ) -> Result<PreparedEditorSourceToolContent, Error> {
        self.require_content_preparation(basis)?;
        if paths.is_empty() || paths.len() > 64 {
            return Err(Error::InvalidPath);
        }
        let mut ledger = self.source_tools.content.ledger.clone();
        for path in paths {
            ledger.set_adopted(installation, path)?;
        }
        self.source_tools.content.prepare_candidate(
            self.source_project_root(),
            ledger,
            None,
            Outcome::Adopted,
        )
    }

    /// Observes a finite declared member set and retains any newly discovered alteration.
    ///
    /// Hashes are observations of the default data stream, not immutable whole-file proof or
    /// deletion permission. Missing files never become invented successful-removal receipts.
    pub fn prepare_source_tool_content_observation(
        &self,
        basis: &EditorSourceToolContentBasis,
        installation: &[u8; 32],
    ) -> Result<PreparedEditorSourceToolContent, Error> {
        self.require_content_preparation(basis)?;
        let root = self.source_project_root();
        let mut ledger = self.source_tools.content.ledger.clone();
        let entry = ledger
            .installations
            .iter_mut()
            .find(|entry| &entry.installed.installation == installation)
            .ok_or(Error::Unmanaged)?;
        let directory = root
            .open_directory_exact(
                &RelativePath::new(entry.installed.directory.as_ref())
                    .map_err(|_| Error::InvalidPath)?,
            )
            .map_err(|_| Error::Read)?;
        let created = self.source_tools.content.created.get(installation);
        if let Some(created) = created {
            if !directory
                .same_object(created.directory())
                .map_err(|_| Error::Read)?
            {
                return Err(Error::Changed);
            }
        }
        observe_members(&directory, entry, created.map(Arc::as_ref))?;
        self.source_tools
            .content
            .prepare_candidate(root, ledger, None, Outcome::Observed)
    }

    /// Confirms the exact prepared protection successor through existing owned file publication.
    pub fn commit_source_tool_content(
        &mut self,
        prepared: PreparedEditorSourceToolContent,
    ) -> Result<EditorSourceToolAttemptId, Error> {
        self.advance_source_tools();
        self.require_content_preparation(&prepared.basis)?;
        if !prepared
            .root
            .same_object(&self.source_project_root())
            .map_err(|_| Error::RootMismatch)?
        {
            return Err(Error::RootMismatch);
        }
        if let Some(created) = &prepared.created {
            let entry = prepared
                .ledger
                .installations
                .iter()
                .find(|entry| entry.installed.installation == *created.receipt().installation_id())
                .ok_or(Error::Unmanaged)?;
            let directory = prepared
                .root
                .open_directory_exact(
                    &RelativePath::new(entry.installed.directory.as_ref())
                        .map_err(|_| Error::InvalidPath)?,
                )
                .map_err(|_| Error::Changed)?;
            if !directory
                .same_object(created.directory())
                .map_err(|_| Error::Changed)?
            {
                return Err(Error::Changed);
            }
        }
        Ok(self.start_content_work(
            Request::Publish {
                bytes: prepared.bytes,
                host: self.source_tools.content.host.clone(),
            },
            Some((prepared.ledger, prepared.outcome)),
            prepared.created,
            false,
        ))
    }

    /// Reads current protection/recovery data without writing, executing or deleting anything.
    ///
    /// Weaker disk protection yields NeedsProtectionPublication and RequiresReconcile. Prepare
    /// and explicitly commit a candidate against the returned basis to persist the retained facts.
    pub fn reconcile_source_tool_content(
        &mut self,
        basis: &EditorSourceToolContentBasis,
    ) -> Result<EditorSourceToolAttemptId, Error> {
        if !self.accepts_new_work() || self.schema_adoption_fenced() {
            return Err(Error::Closing);
        }
        let state = &self.source_tools.content;
        if basis != &state.basis {
            return Err(Error::StaleBasis);
        }
        if state.pending() {
            return Err(Error::Busy);
        }
        Ok(self.start_content_work(Request::Observe, None, None, true))
    }

    fn require_content_preparation(
        &self,
        basis: &EditorSourceToolContentBasis,
    ) -> Result<(), Error> {
        if !self.accepts_new_work() || self.schema_adoption_fenced() {
            return Err(Error::Closing);
        }
        self.source_tools.content.require_basis(basis)
    }

    fn start_content_work(
        &mut self,
        request: Request,
        candidate: Option<(record::Ledger, Outcome)>,
        created: Option<Arc<EditorSourceToolInstallation>>,
        reconcile: bool,
    ) -> EditorSourceToolAttemptId {
        let attempt = EditorSourceToolAttemptId::issue();
        #[cfg(all(test, windows))]
        let gate = self.source_tools.content.next_io_gate.take();
        let pending = Pending::start(
            self.source_project_root(),
            self.authoring_runtime.host_custody().clone(),
            self.authoring_runtime.persistence_tasks(),
            request,
            attempt.clone(),
            candidate,
            created,
            reconcile,
            #[cfg(all(test, windows))]
            gate,
        );
        let state = &mut self.source_tools.content;
        state.phase = if pending.candidate.is_some() {
            Phase::Publishing
        } else {
            Phase::Reconciling
        };
        state.basis = EditorSourceToolContentBasis::issue();
        state.error = None;
        state.operation = Some(EditorSourceToolContentAttempt {
            attempt: attempt.clone(),
            outcome: None,
        });
        state.pending = Some(pending);
        attempt
    }

    pub(super) fn advance_source_tool_content(&mut self) {
        if let Some(host) = &self.source_tools.content.host {
            host.try_with(|host| host.retry_cleanup());
        }
        let Some(pending) = self.source_tools.content.pending.as_mut() else {
            return;
        };
        let Some(result) = pending.poll() else { return };
        let pending = self
            .source_tools
            .content
            .pending
            .take()
            .expect("polled one content task");
        let state = &mut self.source_tools.content;
        match result {
            Ok(IoResult::Observed { mut ledger, host }) => {
                let needs_publication = match preserve_protection(&state.ledger, &mut ledger) {
                    Ok(needs_publication) => needs_publication,
                    Err(error) => {
                        if let Some(host) = host {
                            host.retire();
                        }
                        state.phase = Phase::Unavailable;
                        state.finish(pending.attempt, Outcome::Failed(error));
                        return;
                    }
                };
                if let Some(previous) = state.host.take() {
                    previous.retire();
                }
                state.host = host;
                state.ledger = ledger;
                // A fresh disk observation never reconstructs creation proof. Keep existing live
                // owners only when every immutable installation/member fact is still identical.
                state.created.retain(|id, created| {
                    state.ledger.installations.iter().any(|entry| {
                        &entry.installed.installation == id && matches_created(entry, created)
                    })
                });
                state.evidence = None;
                // Keep the Host's actual disk baseline while retaining stronger in-memory facts.
                // Only explicit publication or a later matching read can settle this difference.
                state.protection_publication_pending = needs_publication;
                state.phase = if needs_publication {
                    Phase::RequiresReconcile
                } else {
                    Phase::Idle
                };
                let outcome = if needs_publication {
                    Outcome::NeedsProtectionPublication
                } else if pending.reconcile {
                    Outcome::Reconciled
                } else {
                    Outcome::Observed
                };
                state.finish(pending.attempt, outcome);
            }
            Ok(IoResult::Published {
                host,
                evidence,
                accepted,
            }) => {
                state.host = Some(host);
                state.evidence = Some(evidence);
                if accepted {
                    let (ledger, outcome) =
                        pending.candidate.expect("publication keeps its candidate");
                    state.ledger = ledger;
                    if let Some(created) = pending.created {
                        state
                            .created
                            .insert(*created.receipt().installation_id(), created);
                    }
                    state.protection_publication_pending = false;
                    state.phase = Phase::Idle;
                    state.finish(pending.attempt, outcome);
                } else {
                    state.phase = Phase::RequiresReconcile;
                    state.finish(pending.attempt, Outcome::Failed(Error::RequiresReconcile));
                }
            }
            Ok(IoResult::Refused { host, error }) => {
                state.host = Some(host);
                state.phase =
                    if state.protection_publication_pending || error == Error::RequiresReconcile {
                        Phase::RequiresReconcile
                    } else {
                        Phase::Idle
                    };
                state.finish(pending.attempt, Outcome::Failed(error));
            }
            Err(error) => {
                state.phase = if error == Error::RequiresReconcile {
                    Phase::RequiresReconcile
                } else if pending.candidate.is_none() {
                    Phase::Unavailable
                } else if state.protection_publication_pending {
                    Phase::RequiresReconcile
                } else {
                    Phase::Idle
                };
                state.finish(pending.attempt, Outcome::Failed(error));
            }
        }
    }
}

fn preserve_protection(
    previous: &record::Ledger,
    current: &mut record::Ledger,
) -> Result<bool, Error> {
    let mut needs_publication = false;
    for original in &previous.installations {
        let successor = current
            .installations
            .iter_mut()
            .find(|entry| entry.installed.installation == original.installed.installation)
            .ok_or(Error::Changed)?;
        if successor.installed != original.installed
            || successor.members.len() != original.members.len()
        {
            return Err(Error::Changed);
        }
        for prior in &original.members {
            let member = successor
                .members
                .iter_mut()
                .find(|member| {
                    member.path == prior.path
                        && member.role == prior.role
                        && member.baseline == prior.baseline
                })
                .ok_or(Error::Changed)?;
            needs_publication |=
                (prior.modified && !member.modified) || (prior.adopted && !member.adopted);
            member.modified |= prior.modified;
            member.adopted |= prior.adopted;
        }
    }
    Ok(needs_publication)
}

fn matches_created(entry: &record::Installation, created: &EditorSourceToolInstallation) -> bool {
    entry.installed.package.as_ref() == created.package()
        && entry.installed.installation == *created.receipt().installation_id()
        && entry.installed.receipt == created.receipt_digest()
        && entry.installed.manifest == created.receipt().manifest_digest()
        && entry.members.len() == created.members().len()
        && entry.members.iter().all(|member| {
            created.members().iter().any(|original| {
                member.path.as_ref() == original.path()
                    && member.baseline == original.digest()
                    && member.role == member_role(original.kind())
            })
        })
}

fn member_role(kind: Option<EditorSourceToolReleaseMemberKind>) -> record::Role {
    match kind {
        Some(EditorSourceToolReleaseMemberKind::Tool) => record::Role::Tool,
        Some(EditorSourceToolReleaseMemberKind::Content) => record::Role::Content,
        None => record::Role::Metadata,
    }
}

fn observe_members(
    directory: &DirectoryCapability,
    entry: &mut record::Installation,
    created: Option<&EditorSourceToolInstallation>,
) -> Result<(), Error> {
    for member in &mut entry.members {
        let current = match directory.open_file_exact(
            &RelativePath::new(member.path.as_ref()).map_err(|_| Error::InvalidPath)?,
        ) {
            Ok(current) => current,
            Err(FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                member.modified = true;
                member.observation = record::Observation::Missing;
                continue;
            }
            Err(_) => {
                member.modified = true;
                member.observation = record::Observation::Failed;
                continue;
            }
        };
        let same_created = created
            .map(|created| {
                created
                    .members()
                    .iter()
                    .find(|original| original.path() == member.path.as_ref())
                    .is_some_and(|original| current.same_object(original.file()).unwrap_or(false))
            })
            .unwrap_or(true);
        if !same_created || current.digest(member.baseline.length()).ok() != Some(member.baseline) {
            member.modified = true;
        }
        // Pending is interrupted intent until explicitly observed. This read supplies a current
        // presence fact, not an assertion that the old attempt removed anything.
        member.observation = record::Observation::Present;
    }
    Ok(())
}

enum Request {
    Observe,
    Publish {
        bytes: Vec<u8>,
        host: Option<RetainedHost<FilePublicationHost>>,
    },
}

enum IoResult {
    Observed {
        ledger: record::Ledger,
        host: Option<RetainedHost<FilePublicationHost>>,
    },
    Published {
        host: RetainedHost<FilePublicationHost>,
        evidence: FilePublicationEvidence,
        accepted: bool,
    },
    Refused {
        host: RetainedHost<FilePublicationHost>,
        error: Error,
    },
}

struct Work {
    request: Option<Request>,
    started: bool,
    result: Option<Result<IoResult, Error>>,
}

struct Pending {
    work: Arc<Mutex<Work>>,
    task: Option<TaskHandle<()>>,
    attempt: EditorSourceToolAttemptId,
    candidate: Option<(record::Ledger, Outcome)>,
    created: Option<Arc<EditorSourceToolInstallation>>,
    reconcile: bool,
}

impl Pending {
    fn start(
        root: DirectoryCapability,
        custody: HostCustody,
        pools: &TaskPools,
        request: Request,
        attempt: EditorSourceToolAttemptId,
        candidate: Option<(record::Ledger, Outcome)>,
        created: Option<Arc<EditorSourceToolInstallation>>,
        reconcile: bool,
        #[cfg(all(test, windows))] gate: Option<Arc<tests::IoGate>>,
    ) -> Self {
        let work = Arc::new(Mutex::new(Work {
            request: Some(request),
            started: false,
            result: None,
        }));
        let retained = Arc::clone(&work);
        let task = pools
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, super::TOOL_DOMAIN),
                move |context| {
                    let mut work = retained.lock().unwrap_or_else(|error| error.into_inner());
                    if context.is_cancelled() {
                        work.result = Some(Err(Error::TaskRejected));
                        return;
                    }
                    work.started = true;
                    let request = work
                        .request
                        .take()
                        .expect("one content task consumes one request");
                    work.result = Some(run_io(&root, &custody, request));
                    #[cfg(all(test, windows))]
                    if let Some(gate) = gate {
                        gate.wait_after_io();
                    }
                },
            )
            .into_handle()
            .ok();
        Self {
            work,
            task,
            attempt,
            candidate,
            created,
            reconcile,
        }
    }

    fn poll(&mut self) -> Option<Result<IoResult, Error>> {
        if self
            .task
            .as_ref()
            .is_some_and(|task| task.return_observer().state() == TaskReturnState::Pending)
        {
            return None;
        }
        let mut work = match self.work.try_lock() {
            Ok(work) => work,
            Err(TryLockError::WouldBlock) => return None,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        Some(work.result.take().unwrap_or(Err(if work.started {
            Error::RequiresReconcile
        } else {
            Error::TaskRejected
        })))
    }
}

fn run_io(
    root: &DirectoryCapability,
    custody: &HostCustody,
    request: Request,
) -> Result<IoResult, Error> {
    match request {
        Request::Observe => {
            let loaded = store::load(root)?;
            let host = loaded
                .host
                .map(|host| custody.retain_publication(host).ok_or(Error::Closing))
                .transpose()?;
            Ok(IoResult::Observed {
                ledger: loaded.ledger,
                host,
            })
        }
        Request::Publish { bytes, host } => {
            let host = match host {
                Some(host) => host,
                None => custody
                    .retain_publication(store::create_missing_host(root)?)
                    .ok_or(Error::Closing)?,
            };
            let current_parent = root.open_directory_exact(
                &RelativePath::new(".nara/source-tools").expect("fixed content parent"),
            );
            let parent_matches = current_parent.is_ok_and(|parent| {
                host.with_worker(|host| host.parent_identity() == parent.identity())
                    .unwrap_or(false)
            });
            if !parent_matches {
                return Ok(IoResult::Refused {
                    host,
                    error: Error::Changed,
                });
            }
            let outcome = host
                .with_worker(|host| match host.publish(&bytes) {
                    FilePublicationOutcome::Published(accepted) => {
                        let (evidence, source) = accepted.into_parts();
                        host.commit_published(source);
                        Ok((evidence, true))
                    }
                    FilePublicationOutcome::Uncertain(evidence) => Ok((evidence, false)),
                    FilePublicationOutcome::Rejected(reason) => Err(match reason {
                        FilePublicationRejection::TargetChanged
                        | FilePublicationRejection::TargetDeleted => Error::Changed,
                        FilePublicationRejection::RequiresReconcile => Error::RequiresReconcile,
                        FilePublicationRejection::CleanupPending => Error::Busy,
                        _ => Error::Write,
                    }),
                    FilePublicationOutcome::Failed(_) => Err(Error::Write),
                })
                .ok_or(Error::Closing)?;
            Ok(match outcome {
                Ok((evidence, accepted)) => {
                    let admitted = host
                        .with_worker(|host| admits_publication(root, host))
                        .unwrap_or(false);
                    if !admitted {
                        host.with_worker(|host| host.mark_uncertain());
                    }
                    IoResult::Published {
                        host,
                        evidence,
                        accepted: accepted && admitted,
                    }
                }
                Err(error) => IoResult::Refused { host, error },
            })
        }
    }
}

fn admits_publication(root: &DirectoryCapability, host: &FilePublicationHost) -> bool {
    let Some((identity, Some(parent), digest)) = host.accepted_source_observation() else {
        return false;
    };
    let Ok(current_parent) = root.open_directory_exact(
        &RelativePath::new(".nara/source-tools").expect("fixed content parent"),
    ) else {
        return false;
    };
    current_parent.identity() == parent
        && current_parent
            .open_file_exact(&RelativePath::new("content.json").expect("fixed content leaf"))
            .is_ok_and(|current| {
                current.identity() == identity
                    && current.verify_digest(digest, store::BYTE_LIMIT).is_ok()
            })
}
