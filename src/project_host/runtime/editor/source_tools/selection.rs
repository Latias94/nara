//! Project-local desired selection is data; probe and route authority stay in the Session.

use std::sync::{Arc, Mutex, TryLockError};

use nara_fs::{DirectoryCapability, RelativePath};
use nara_tasks::{TaskPools, TaskReturnState};
use nara_tooling::{
    EditorSourceToolDesiredSelection, EditorSourceToolInstalledEntry,
    EditorSourceToolInstalledSelection, EditorSourceToolSelectionAttempt,
    EditorSourceToolSelectionBasis, EditorSourceToolSelectionError as Error,
    EditorSourceToolSelectionOutcome as Outcome, EditorSourceToolSelectionPhase as Phase,
    EditorSourceToolSelectionView,
};

use super::*;
use crate::project_host::file_publication::{
    FilePublicationEvidence, FilePublicationHost, FilePublicationOutcome, FilePublicationRejection,
};
use crate::project_host::runtime::editor::persistence::{HostCustody, RetainedHost};

#[cfg(all(test, any(windows, target_os = "linux")))]
mod admission_tests;
mod graph;
mod installed_graph;
mod store;
#[cfg(test)]
mod tests;

pub(super) use graph::InstalledToolClosure;
pub use graph::{PreparedEditorSourceToolInstallations, PreparedEditorSourceToolWithdrawal};
use installed_graph::Graph;

pub(in crate::project_host::runtime::editor) const SELECTION_PATH: &str = store::SELECTION_PATH;

/// Code-free installed selection prepared from the same project capability used to open Session.
///
/// Preparation performs bounded installation inspection. It grants neither execution nor routing;
/// the supplied definition must already carry explicit managed Host approval.
#[derive(Debug)]
pub struct PreparedEditorSourceToolSelection {
    root: DirectoryCapability,
    desired: EditorSourceToolDesiredSelection,
    definition: EditorSourceToolDefinition,
    graph: Graph,
    basis: Option<EditorSourceToolSelectionBasis>,
}

impl PreparedEditorSourceToolSelection {
    pub fn new(
        project: DirectoryCapability,
        directory: &str,
        definition: EditorSourceToolDefinition,
    ) -> Result<Self, Error> {
        if !artifact_store::validate_path(directory) {
            return Err(Error::InvalidPath);
        }
        let approved = definition
            .command
            .approved
            .as_ref()
            .ok_or(Error::NotApproved)?;
        let (origin, package, installation, receipt, manifest) = approved.installation_facts();
        let path = RelativePath::new(directory).map_err(|_| Error::InvalidPath)?;
        let current = project
            .open_directory_exact(&path)
            .map_err(|_| Error::Installation)?;
        if !current
            .same_object(origin)
            .map_err(|_| Error::RootMismatch)?
        {
            return Err(Error::RootMismatch);
        }
        let desired = EditorSourceToolDesiredSelection {
            installed: EditorSourceToolInstalledSelection {
                package: Arc::from(package),
                directory: Arc::from(directory),
                installation: *installation,
                receipt,
                manifest,
            },
            enabled: true,
        };
        let graph = Graph::default().select(desired.clone())?;
        store::encode(&graph)?;
        graph::verify_entries(&project, &graph.entries)?;
        Ok(Self {
            root: project,
            desired,
            definition,
            graph,
            basis: None,
        })
    }

    #[must_use]
    pub fn selection(&self) -> &EditorSourceToolInstalledSelection {
        &self.desired.installed
    }
}

pub(super) struct EnableSelection {
    graph: Graph,
    closure: Vec<EditorSourceToolInstalledEntry>,
    approved_project: DirectoryCapability,
    fence: EditorSourceToolSelectionBasis,
    predecessor: Option<EditorSourceToolActivationId>,
}

pub(super) struct ReadyTool {
    pub(super) active: EditorSourceToolActive,
    pub(super) definition: EditorSourceToolDefinition,
    pub(super) attempt: EditorSourceToolAttemptId,
    pub(super) selection: Option<EnableSelection>,
}

pub(super) struct SelectionState {
    basis: EditorSourceToolSelectionBasis,
    graph: Graph,
    host: Option<RetainedHost<FilePublicationHost>>,
    phase: Phase,
    operation: Option<EditorSourceToolSelectionAttempt>,
    disable: Option<EditorSourceToolSelectionAttempt>,
    error: Option<Error>,
    pending: Option<PendingIo>,
    ready: Option<ReadyTool>,
    deferred_disable: Option<EditorSourceToolAttemptId>,
    // Retain the actual publication facts even when observation cannot admit a saved selection.
    evidence: Option<FilePublicationEvidence>,
    #[cfg(test)]
    next_io_gate: Option<tests::IoGate>,
}

impl Default for SelectionState {
    fn default() -> Self {
        Self {
            basis: EditorSourceToolSelectionBasis::issue(),
            graph: Graph::default(),
            host: None,
            phase: Phase::Loading,
            operation: None,
            disable: None,
            error: None,
            pending: None,
            ready: None,
            deferred_disable: None,
            evidence: None,
            #[cfg(test)]
            next_io_gate: None,
        }
    }
}

impl SelectionState {
    pub(super) fn require_reclamation_absence(
        &self,
        basis: &EditorSourceToolSelectionBasis,
        installation: &[u8; 32],
        directory: &str,
        root: &DirectoryCapability,
    ) -> Result<(), nara_tooling::EditorSourceToolReclamationError> {
        use nara_tooling::EditorSourceToolReclamationError as Rejection;
        if basis != &self.basis {
            return Err(Rejection::StaleBasis);
        }
        if self.phase != Phase::Idle {
            return Err(Rejection::RequiresReconcile);
        }
        if self.schema_adoption_pending() {
            return Err(Rejection::Busy);
        }
        if self.graph.entries.iter().any(|entry| {
            &entry.installed.installation == installation
                || entry.installed.directory.eq_ignore_ascii_case(directory)
        }) {
            return Err(Rejection::StillInstalled);
        }
        let root = root.strict_read_only().map_err(|_| Rejection::Read)?;
        let path = RelativePath::new(SELECTION_PATH).expect("fixed selection path");
        match root.open_file_exact(&path) {
            Ok(current) => {
                let host = self.host.as_ref().ok_or(Rejection::Changed)?;
                host.try_with(|host| {
                    let Some((_, _, digest)) = host.accepted_source_observation() else { return Err(Rejection::RequiresReconcile); };
                    if !host.same_source_object(&current).map_err(|_| Rejection::Read)? {
                        return Err(Rejection::Changed);
                    }
                    current.verify_digest(digest, store::BYTE_LIMIT).map_err(|_| Rejection::Changed)?;
                    Ok(())
                }).ok_or(Rejection::Busy)?
            }
            Err(nara_fs::FsError::Io { source, .. }) if source.kind() == std::io::ErrorKind::NotFound => {
                match &self.host {
                    None => Ok(()),
                    Some(host) => host.try_with(|host| {
                        if !host.uncertain() && matches!(host.binding(), crate::project_host::file_publication::FilePublicationBinding::Missing) {
                            Ok(())
                        } else { Err(Rejection::Changed) }
                    }).ok_or(Rejection::Busy)?,
                }
            }
            Err(_) => Err(Rejection::Read),
        }?;
        let directory = root
            .open_directory_exact(&RelativePath::new(directory).map_err(|_| Rejection::Read)?)
            .map_err(|_| Rejection::Read)?;
        for entry in &self.graph.entries {
            let installed = root
                .open_directory_exact(
                    &RelativePath::new(entry.installed.directory.as_ref())
                        .map_err(|_| Rejection::Read)?,
                )
                .map_err(|_| Rejection::RequiresReconcile)?;
            if installed
                .same_object(&directory)
                .map_err(|_| Rejection::Read)?
            {
                return Err(Rejection::StillInstalled);
            }
        }
        Ok(())
    }

    /// Retained selection intent can still publish or promote after all process jobs return.
    pub(super) fn schema_adoption_pending(&self) -> bool {
        self.pending.is_some()
            || self.ready.is_some()
            || self.deferred_disable.is_some()
            || self.host.as_ref().is_some_and(RetainedHost::has_cleanup)
    }

    pub(super) fn view(
        &self,
        active_origin: Option<EditorSourceToolInstalledSelection>,
    ) -> EditorSourceToolSelectionView {
        EditorSourceToolSelectionView {
            basis: self.basis.clone(),
            installed: self.graph.entries.clone(),
            desired: self.graph.desired.clone(),
            active_origin,
            phase: self.phase,
            operation: self.operation.clone(),
            disable: self.disable.clone(),
            error: self.error.clone(),
        }
    }

    fn check_basis(&self, basis: &EditorSourceToolSelectionBasis) -> Result<(), Error> {
        if &self.basis != basis {
            return Err(Error::StaleBasis);
        }
        Ok(())
    }

    fn writable(&self) -> Result<(), Error> {
        match self.phase {
            Phase::Idle => Ok(()),
            Phase::RequiresReconcile => Err(Error::RequiresReconcile),
            Phase::Unavailable => Err(self.error.clone().unwrap_or(Error::Read)),
            _ => Err(Error::Busy),
        }
    }

    pub(super) fn graph_mutation_pending(&self) -> bool {
        self.phase == Phase::Reconciling
            || self.pending.as_ref().is_some_and(|pending| {
                matches!(
                    pending.mutation,
                    Some(Mutation::Register | Mutation::Withdraw)
                )
            })
    }

    fn finish(&mut self, attempt: EditorSourceToolAttemptId, outcome: Outcome) {
        let slot = if self
            .operation
            .as_ref()
            .is_some_and(|current| current.attempt == attempt)
        {
            self.operation.as_mut()
        } else {
            self.disable
                .as_mut()
                .filter(|current| current.attempt == attempt)
        }
        .expect("only an admitted selection attempt can complete");
        if slot.outcome.is_some() {
            return;
        }
        slot.outcome = Some(outcome.clone());
        self.basis = EditorSourceToolSelectionBasis::issue();
        self.error = match &outcome {
            Outcome::Failed(error) => Some(error.clone()),
            _ => None,
        };
    }

    pub(super) fn fail_probe(&mut self, attempt: EditorSourceToolAttemptId, error: Error) {
        if self
            .operation
            .as_ref()
            .is_some_and(|current| current.attempt == attempt && current.outcome.is_none())
        {
            self.phase = Phase::Idle;
            self.finish(attempt, Outcome::Failed(error));
        }
    }

    pub(super) fn revoke_promotion(&mut self) {
        self.ready = None;
        if self.phase == Phase::Probing {
            self.phase = Phase::Idle;
        }
        self.basis = EditorSourceToolSelectionBasis::issue();
        // In-flight IO continues to report actual publication; it cannot restore routing.
    }

    pub(super) fn invalidate_route_successor(&mut self) {
        self.basis = EditorSourceToolSelectionBasis::issue();
        self.ready = None;
    }

    pub(super) fn publication_host(&self) -> Option<&RetainedHost<FilePublicationHost>> {
        self.host.as_ref()
    }
}

#[derive(Debug, Clone, Copy)]
enum Mutation {
    Enable,
    Disable,
    Register,
    Withdraw,
}

enum IoRequest {
    Observe {
        route: Option<graph::RouteObservation>,
        graph: Graph,
    },
    Publish {
        graph: Graph,
        mutation: Mutation,
        verify: Vec<EditorSourceToolInstalledEntry>,
        host: Option<RetainedHost<FilePublicationHost>>,
        approved_project: Option<DirectoryCapability>,
    },
}

struct Loaded {
    graph: Graph,
    host: Option<RetainedHost<FilePublicationHost>>,
    revoke: Option<EditorSourceToolActivationId>,
    linked: Option<(
        EditorSourceToolActivationId,
        Vec<EditorSourceToolInstalledEntry>,
    )>,
}

enum IoResult {
    Observed(Loaded),
    ObservationRefused {
        error: Error,
        revoke: Option<EditorSourceToolActivationId>,
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

struct IoWork {
    request: Option<IoRequest>,
    started: bool,
    result: Option<Result<IoResult, Error>>,
}

struct PendingIo {
    work: Arc<Mutex<IoWork>>,
    task: Option<TaskHandle<()>>,
    attempt: EditorSourceToolAttemptId,
    graph: Option<Graph>,
    mutation: Option<Mutation>,
    reconcile: bool,
}

impl PendingIo {
    fn start(
        root: DirectoryCapability,
        custody: HostCustody,
        pools: &TaskPools,
        request: IoRequest,
        attempt: EditorSourceToolAttemptId,
        reconcile: bool,
        #[cfg(test)] gate: Option<tests::IoGate>,
    ) -> Self {
        let (graph, mutation) = match &request {
            IoRequest::Publish {
                graph, mutation, ..
            } => (Some(graph.clone()), Some(*mutation)),
            IoRequest::Observe { .. } => (None, None),
        };
        let work = Arc::new(Mutex::new(IoWork {
            request: Some(request),
            started: false,
            result: None,
        }));
        let retained = Arc::clone(&work);
        let task = pools
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, TOOL_DOMAIN),
                move |context| {
                    let mut work = retained.lock().unwrap_or_else(|error| error.into_inner());
                    if context.is_cancelled() {
                        work.result = Some(Err(Error::TaskRejected));
                        return;
                    }
                    work.started = true;
                    #[cfg(test)]
                    if let Some(gate) = &gate {
                        gate.wait_at(tests::IoPhase::BeforeWork);
                    }
                    let request = work
                        .request
                        .take()
                        .expect("selection work is consumed once");
                    work.result = Some(run_io(&root, &custody, request));
                    #[cfg(test)]
                    if let Some(gate) = &gate {
                        gate.wait_at(tests::IoPhase::AfterWork);
                    }
                },
            )
            .into_handle()
            .ok();
        Self {
            work,
            task,
            attempt,
            graph,
            mutation,
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
    request: IoRequest,
) -> Result<IoResult, Error> {
    match request {
        IoRequest::Observe { route, graph } => {
            let linked = route
                .as_ref()
                .map(|route| route.entries(root, &graph))
                .transpose();
            let loaded = match store::load(root) {
                Ok(loaded) => loaded,
                Err(error) => {
                    return Ok(IoResult::ObservationRefused {
                        error,
                        revoke: route
                            .filter(|_| {
                                linked.as_ref().map_or(true, |linked| {
                                    linked.as_ref().is_some_and(|entries| !entries.is_empty())
                                })
                            })
                            .map(|route| route.activation),
                    });
                }
            };
            let retain_link = match (&route, &linked) {
                (Some(route), Ok(Some(entries))) if !entries.is_empty() => {
                    Some((route.activation.clone(), entries.clone()))
                }
                _ => None,
            };
            let revoke = route
                .filter(|_| match &linked {
                    Err(_) => true,
                    Ok(Some(entries)) => entries
                        .iter()
                        .any(|entry| !loaded.graph.entries.contains(entry)),
                    Ok(None) => false,
                })
                .map(|route| route.activation);
            let host = match loaded.host {
                Some(host) => Some(custody.retain_publication(host).ok_or(Error::Closing)?),
                None => None,
            };
            Ok(IoResult::Observed(Loaded {
                graph: loaded.graph,
                host,
                revoke,
                linked: retain_link,
            }))
        }
        IoRequest::Publish {
            graph,
            mutation: _,
            verify,
            host,
            approved_project,
        } => {
            if !verify.is_empty() {
                let approved = approved_project.as_ref().ok_or(Error::NotApproved)?;
                if !root
                    .same_object(approved)
                    .map_err(|_| Error::RootMismatch)?
                {
                    return Err(Error::RootMismatch);
                }
                graph::verify_entries(approved, &verify)?;
            }
            let bytes = store::encode(&graph)?;
            let host = match host {
                Some(host) => host,
                None => custody
                    .retain_publication(store::create_missing_host(root)?)
                    .ok_or(Error::Closing)?,
            };
            let named_parent =
                RelativePath::new(".nara/source-tools").expect("fixed selection parent");
            let parent = match root.open_directory_exact(&named_parent) {
                Ok(parent) => parent,
                Err(_) => {
                    return Ok(IoResult::Refused {
                        host,
                        error: Error::Changed,
                    });
                }
            };
            let parent_matches = host
                .with_worker(|host| host.parent_identity() == parent.identity())
                .ok_or(Error::Closing)?;
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
                    // The file Host authorizes its retained parent. Separately verify the
                    // project-relative selection still names that actual publication.
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
        &RelativePath::new(".nara/source-tools").expect("fixed selection parent"),
    ) else {
        return false;
    };
    current_parent.identity() == parent
        && current_parent
            .open_file_exact(&RelativePath::new("selection.json").expect("fixed selection leaf"))
            .is_ok_and(|current| {
                current.identity() == identity
                    && current.verify_digest(digest, store::BYTE_LIMIT).is_ok()
            })
}

impl EditorProjectSession {
    /// Prepares through a Host-issued strict capability for this Session's actual project object.
    ///
    /// Performs bounded code-free IO; call from Host preparation, never during painting. Neither
    /// inspection nor approval probes the child. Enablement still consumes an original basis.
    pub fn prepare_installed_source_tool_selection(
        &self,
        project: DirectoryCapability,
        directory: &str,
        receipt: nara_fs::ContentDigest,
        host: &EditorSourceToolManagedHost,
    ) -> Result<PreparedEditorSourceToolSelection, Error> {
        if !self.accepts_new_work() {
            return Err(Error::Closing);
        }
        if self.schema_adoption_fenced() {
            return Err(Error::Busy);
        }
        if !project
            .same_object(&self.source_project_root())
            .map_err(|_| Error::RootMismatch)?
        {
            return Err(Error::RootMismatch);
        }
        graph::prepare_required_selection(self, project, directory, receipt, host)
    }

    pub(in crate::project_host::runtime::editor) fn initialize_source_tool_selection(&mut self) {
        let attempt = EditorSourceToolAttemptId::issue();
        self.source_tools.selection.operation = Some(EditorSourceToolSelectionAttempt {
            attempt: attempt.clone(),
            outcome: None,
        });
        #[cfg(test)]
        let gate = self.source_tools.selection.next_io_gate.take();
        self.source_tools.selection.pending = Some(PendingIo::start(
            self.source_project_root(),
            self.authoring_runtime.host_custody().clone(),
            self.authoring_runtime.persistence_tasks(),
            IoRequest::Observe {
                route: None,
                graph: Graph::default(),
            },
            attempt,
            false,
            #[cfg(test)]
            gate,
        ));
        self.initialize_source_tool_artifacts();
    }

    /// Observes desired disk selection separately from the actual active route; performs no IO.
    #[must_use]
    pub fn source_tool_selection_view(&self) -> EditorSourceToolSelectionView {
        self.source_tools.selection.view(
            self.source_tools
                .active
                .as_ref()
                .and_then(|active| active.origin.clone()),
        )
    }

    pub fn enable_installed_source_tool(
        &mut self,
        basis: &EditorSourceToolSelectionBasis,
        prepared: PreparedEditorSourceToolSelection,
    ) -> Result<EditorSourceToolAttemptId, Error> {
        if self.source_tools.reclamation_fenced() {
            return Err(Error::Busy);
        }
        if !self.accepts_new_work() {
            return Err(Error::Closing);
        }
        if self.schema_adoption_fenced() {
            return Err(Error::Busy);
        }
        self.advance_source_tools();
        self.source_tools.selection.check_basis(basis)?;
        self.source_tools.selection.writable()?;
        if prepared
            .basis
            .as_ref()
            .is_some_and(|prepared| prepared != basis)
        {
            return Err(Error::StaleBasis);
        }
        if !prepared
            .root
            .same_object(&self.source_project_root())
            .map_err(|_| Error::RootMismatch)?
        {
            return Err(Error::RootMismatch);
        }
        self.validate_installed_tool_schema(&prepared.definition)?;
        let graph = if prepared.basis.is_some() {
            prepared.graph
        } else {
            self.source_tools
                .selection
                .graph
                .select(prepared.desired.clone())?
        };
        let closure = graph.closure(&prepared.desired.installed.installation)?;
        let predecessor = self
            .source_tools
            .active
            .as_ref()
            .map(|active| active.active.id().clone());
        let attempt =
            self.start_source_tool_probe(prepared.definition)
                .map_err(|error| match error {
                    EditorSourceToolError::Busy => Error::Busy,
                    EditorSourceToolError::Closing => Error::Closing,
                    _ => Error::TaskRejected,
                })?;
        let fence = EditorSourceToolSelectionBasis::issue();
        self.source_tools
            .probe
            .as_mut()
            .expect("enable admitted one probe")
            .selection = Some(EnableSelection {
            graph,
            closure,
            approved_project: prepared.root,
            fence: fence.clone(),
            predecessor,
        });
        let selection = &mut self.source_tools.selection;
        selection.basis = fence;
        selection.phase = Phase::Probing;
        selection.operation = Some(EditorSourceToolSelectionAttempt {
            attempt: attempt.clone(),
            outcome: None,
        });
        selection.error = None;
        Ok(attempt)
    }

    /// Stops execution immediately; durable completion is observed with this attempt's result.
    pub fn disable_installed_source_tool(
        &mut self,
        basis: &EditorSourceToolSelectionBasis,
    ) -> Result<EditorSourceToolAttemptId, Error> {
        if self.source_tools.reclamation_fenced() {
            return Err(Error::Busy);
        }
        if !self.accepts_new_work() {
            return Err(Error::Closing);
        }
        if self.schema_adoption_fenced() {
            return Err(Error::Busy);
        }
        self.source_tools.selection.check_basis(basis)?;
        if self
            .source_tools
            .selection
            .disable
            .as_ref()
            .is_some_and(|current| current.outcome.is_none())
        {
            return Err(Error::Busy);
        }
        // Stop is allowed even if disk observation/write is unavailable.
        self.source_tools.cancel();
        let attempt = EditorSourceToolAttemptId::issue();
        let selection = &mut self.source_tools.selection;
        selection.disable = Some(EditorSourceToolSelectionAttempt {
            attempt: attempt.clone(),
            outcome: None,
        });
        selection.basis = EditorSourceToolSelectionBasis::issue();
        if selection.pending.is_some() {
            selection.deferred_disable = Some(attempt.clone());
        } else {
            self.start_selection_disable(attempt.clone());
        }
        Ok(attempt)
    }

    /// Admits current selection data without approving, probing or activating executable code.
    pub fn reconcile_source_tool_selection(
        &mut self,
        basis: &EditorSourceToolSelectionBasis,
    ) -> Result<EditorSourceToolAttemptId, Error> {
        if self.source_tools.reclamation_fenced() {
            return Err(Error::Busy);
        }
        if !self.accepts_new_work() {
            return Err(Error::Closing);
        }
        if self.schema_adoption_fenced() {
            return Err(Error::Busy);
        }
        self.source_tools.selection.check_basis(basis)?;
        if self.source_tools.selection.pending.is_some() || self.source_tools.probe.is_some() {
            return Err(Error::Busy);
        }
        let attempt = EditorSourceToolAttemptId::issue();
        #[cfg(test)]
        let gate = self.source_tools.selection.next_io_gate.take();
        let pending = PendingIo::start(
            self.source_project_root(),
            self.authoring_runtime.host_custody().clone(),
            self.authoring_runtime.persistence_tasks(),
            IoRequest::Observe {
                route: self
                    .source_tools
                    .active
                    .as_ref()
                    .map(graph::RouteObservation::of),
                graph: self.source_tools.selection.graph.clone(),
            },
            attempt.clone(),
            true,
            #[cfg(test)]
            gate,
        );
        let selection = &mut self.source_tools.selection;
        selection.basis = EditorSourceToolSelectionBasis::issue();
        selection.phase = Phase::Reconciling;
        selection.operation = Some(EditorSourceToolSelectionAttempt {
            attempt: attempt.clone(),
            outcome: None,
        });
        selection.pending = Some(pending);
        Ok(attempt)
    }

    fn validate_installed_tool_schema(
        &self,
        definition: &EditorSourceToolDefinition,
    ) -> Result<(), Error> {
        let bundle = match &self.execution {
            super::super::EditorExecutionLane::Authoring(project) => project.authoring_bundle(),
            super::super::EditorExecutionLane::TrustedCompiled(lane) => {
                lane.plan.schema_validation().authoring_schema_bundle()
            }
        };
        let admitted = bundle
            .owner(definition.owner.owner_id())
            .ok_or(Error::Schema)?;
        if admitted.current() != definition.owner.current()
            || admitted.predecessor() != definition.owner.predecessor()
        {
            return Err(Error::Schema);
        }
        Ok(())
    }

    pub(super) fn source_tool_ready(&mut self, ready: ReadyTool) {
        if ready.selection.is_none() {
            self.promote_source_tool(ready, None);
            return;
        }
        let selected = ready
            .selection
            .as_ref()
            .expect("durable readiness has a selection");
        let graph = selected.graph.clone();
        let verify = selected.closure.clone();
        let approved_project = selected.approved_project.share();
        let attempt = ready.attempt.clone();
        let selection = &mut self.source_tools.selection;
        if selection.basis != selected.fence || self.closing {
            selection.phase = Phase::Idle;
            selection.finish(attempt, Outcome::Failed(Error::PromotionRefused));
            return;
        }
        selection.phase = Phase::Publishing;
        selection.ready = Some(ready);
        #[cfg(test)]
        let gate = selection.next_io_gate.take();
        self.source_tools.selection.pending = Some(PendingIo::start(
            self.source_project_root(),
            self.authoring_runtime.host_custody().clone(),
            self.authoring_runtime.persistence_tasks(),
            IoRequest::Publish {
                graph,
                mutation: Mutation::Enable,
                verify,
                host: self.source_tools.selection.host.clone(),
                approved_project: Some(approved_project),
            },
            attempt,
            false,
            #[cfg(test)]
            gate,
        ));
    }

    pub(super) fn source_tool_probe_failed(
        &mut self,
        attempt: EditorSourceToolAttemptId,
        error: Error,
    ) {
        self.source_tools.selection.fail_probe(attempt, error);
    }

    fn promote_source_tool(
        &mut self,
        ready: ReadyTool,
        origin: Option<EditorSourceToolInstalledSelection>,
    ) {
        let requirements = ready
            .selection
            .as_ref()
            .map(|selected| selected.closure.clone())
            .unwrap_or_default();
        self.source_tools.selection.invalidate_route_successor();
        self.source_tools.retire_invocation();
        self.source_tools.latest = None;
        self.source_tools.active = Some(Activation {
            active: ready.active,
            definition: ready.definition,
            origin,
            requirements,
        });
        self.source_tools.request_error = None;
    }

    fn start_selection_disable(&mut self, attempt: EditorSourceToolAttemptId) {
        if let Err(error) = self.source_tools.selection.writable() {
            self.source_tools
                .selection
                .finish(attempt, Outcome::Failed(error));
            return;
        }
        if self.source_tools.selection.graph.desired.is_none() {
            self.source_tools
                .selection
                .finish(attempt, Outcome::Disabled);
            return;
        }
        let graph = self.source_tools.selection.graph.disabled();
        self.source_tools.selection.phase = Phase::Publishing;
        #[cfg(test)]
        let gate = self.source_tools.selection.next_io_gate.take();
        self.source_tools.selection.pending = Some(PendingIo::start(
            self.source_project_root(),
            self.authoring_runtime.host_custody().clone(),
            self.authoring_runtime.persistence_tasks(),
            IoRequest::Publish {
                graph,
                mutation: Mutation::Disable,
                verify: Vec::new(),
                host: self.source_tools.selection.host.clone(),
                approved_project: None,
            },
            attempt,
            false,
            #[cfg(test)]
            gate,
        ));
    }

    pub(super) fn advance_source_tool_selection(&mut self) {
        let Some(pending) = self.source_tools.selection.pending.as_mut() else {
            return;
        };
        let Some(result) = pending.poll() else { return };
        let pending = self
            .source_tools
            .selection
            .pending
            .take()
            .expect("polled one selection IO");
        match result {
            Ok(IoResult::Observed(loaded)) => {
                if self
                    .source_tools
                    .active
                    .as_ref()
                    .is_some_and(|active| loaded.revoke.as_ref() == Some(active.active.id()))
                {
                    self.source_tools.cancel();
                }
                if let Some((activation, entries)) = loaded.linked {
                    if let Some(active) = self
                        .source_tools
                        .active
                        .as_mut()
                        .filter(|active| active.active.id() == &activation)
                    {
                        active.requirements = entries;
                    }
                }
                let selection = &mut self.source_tools.selection;
                if let Some(previous) = selection.host.take() {
                    previous.retire();
                }
                selection.host = loaded.host;
                selection.graph = loaded.graph;
                selection.evidence = None;
                selection.phase = Phase::Idle;
                selection.finish(
                    pending.attempt,
                    if pending.reconcile {
                        Outcome::Reconciled
                    } else {
                        Outcome::Observed
                    },
                );
            }
            Ok(IoResult::ObservationRefused { error, revoke }) => {
                if self
                    .source_tools
                    .active
                    .as_ref()
                    .is_some_and(|active| revoke.as_ref() == Some(active.active.id()))
                {
                    self.source_tools.cancel();
                }
                let selection = &mut self.source_tools.selection;
                selection.ready = None;
                selection.phase = Phase::Unavailable;
                selection.finish(pending.attempt, Outcome::Failed(error));
            }
            Ok(IoResult::Published {
                host,
                evidence,
                accepted,
            }) => {
                let ready = self.source_tools.selection.ready.take();
                self.source_tools.selection.host = Some(host);
                self.source_tools.selection.evidence = Some(evidence);
                if accepted {
                    let graph = pending.graph.expect("a publication retains its graph");
                    let desired = graph.desired.clone();
                    self.source_tools.selection.graph = graph;
                    self.source_tools.selection.phase = Phase::Idle;
                    let outcome = if let Some(ready) = ready {
                        let selected = ready
                            .selection
                            .as_ref()
                            .expect("durable readiness retains its fence");
                        let predecessor = self
                            .source_tools
                            .active
                            .as_ref()
                            .map(|active| active.active.id().clone());
                        if !self.closing
                            && self.ensure_source_tool_admission().is_ok()
                            && selected.fence == self.source_tools.selection.basis
                            && selected.predecessor == predecessor
                            && self.source_tools.attempt.as_ref() == Some(&ready.attempt)
                            && self
                                .validate_installed_tool_schema(&ready.definition)
                                .is_ok()
                        {
                            self.promote_source_tool(
                                ready,
                                desired.map(|desired| desired.installed),
                            );
                            Outcome::Enabled
                        } else {
                            Outcome::PublishedWithoutActivation
                        }
                    } else {
                        match pending
                            .mutation
                            .expect("a publication retains its operation")
                        {
                            Mutation::Register => Outcome::Registered,
                            Mutation::Withdraw => Outcome::Withdrawn,
                            Mutation::Disable => Outcome::Disabled,
                            Mutation::Enable => Outcome::PublishedWithoutActivation,
                        }
                    };
                    self.source_tools.selection.finish(pending.attempt, outcome);
                } else {
                    self.source_tools.selection.phase = Phase::RequiresReconcile;
                    self.source_tools
                        .selection
                        .finish(pending.attempt, Outcome::Failed(Error::RequiresReconcile));
                }
            }
            Ok(IoResult::Refused { host, error }) => {
                let selection = &mut self.source_tools.selection;
                selection.host = Some(host);
                selection.ready = None;
                selection.phase = if error == Error::RequiresReconcile {
                    Phase::RequiresReconcile
                } else {
                    Phase::Idle
                };
                selection.finish(pending.attempt, Outcome::Failed(error));
            }
            Err(error) => {
                let selection = &mut self.source_tools.selection;
                selection.ready = None;
                selection.phase = if error == Error::RequiresReconcile {
                    Phase::RequiresReconcile
                } else if pending.graph.is_none() {
                    Phase::Unavailable
                } else {
                    Phase::Idle
                };
                selection.finish(pending.attempt, Outcome::Failed(error));
            }
        }
        if let Some(attempt) = self.source_tools.selection.deferred_disable.take() {
            if self.closing {
                self.source_tools
                    .selection
                    .finish(attempt, Outcome::Failed(Error::Closing));
            } else {
                self.start_selection_disable(attempt);
            }
        }
    }
}
