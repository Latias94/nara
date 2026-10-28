//! Finite artifact inventory publication, independent of ordinary document persistence.

use std::{collections::BTreeMap, fmt, sync::Arc};

use nara_fs::{DirectoryCapability, FileCapability, FsError, RelativePath};
use nara_tooling::{
    EditorSourceToolArtifactAttempt, EditorSourceToolArtifactBasis,
    EditorSourceToolArtifactError as Error, EditorSourceToolArtifactInstallation,
    EditorSourceToolArtifactMember, EditorSourceToolArtifactOutcome as Outcome,
    EditorSourceToolArtifactPhase as Phase, EditorSourceToolArtifactView,
    EditorSourceToolAttemptId, EditorSourceToolInstalledSelection,
};

use super::{
    EditorProjectSession, EditorSourceToolInstallation, EditorSourceToolReleaseMemberKind,
};
use crate::project_host::{
    file_publication::FilePublicationHost,
    runtime::editor::{
        persistence::RetainedHost,
        source_admission::{SourceAdmissionError, exclude_publication_alias},
    },
};

mod io;
pub(super) mod record;
mod store;
#[cfg(all(test, windows))]
mod tests;

pub(in crate::project_host::runtime::editor) const ARTIFACTS_PATH: &str = store::PATH;

/// One exact inventory successor. Preparation and dropping the candidate perform no writes.
///
/// This grants no source editing, graph mutation, executable activation or deletion permission.
pub struct PreparedEditorSourceToolArtifacts {
    root: DirectoryCapability,
    basis: EditorSourceToolArtifactBasis,
    inventory: record::Inventory,
    bytes: Arc<[u8]>,
    registration: Option<Arc<EditorSourceToolInstallation>>,
    outcome: Outcome,
}

impl fmt::Debug for PreparedEditorSourceToolArtifacts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedEditorSourceToolArtifacts")
            .field("installations", &self.inventory.installations.len())
            .field("outcome", &self.outcome)
            .finish_non_exhaustive()
    }
}

impl PreparedEditorSourceToolArtifacts {
    #[must_use]
    pub fn installations(&self) -> Vec<EditorSourceToolArtifactInstallation> {
        self.inventory.installations.clone()
    }
}

pub(super) struct ArtifactState {
    basis: EditorSourceToolArtifactBasis,
    inventory: record::Inventory,
    // Confirmed registration retains actual objects, including after failed publication or a
    // weaker disk observation. It supplies alias custody only, never deletion authorization.
    created: BTreeMap<[u8; 32], Arc<EditorSourceToolInstallation>>,
    host: Option<RetainedHost<FilePublicationHost>>,
    pending: Option<io::Pending>,
    phase: Phase,
    operation: Option<EditorSourceToolArtifactAttempt>,
    error: Option<Error>,
    // A later read does not erase an actual publication result or its candidate handles.
    received: Option<io::ReceivedPublication>,
    #[cfg(all(test, windows))]
    next_io_gate: Option<Arc<tests::IoGate>>,
}

impl Default for ArtifactState {
    fn default() -> Self {
        Self {
            basis: EditorSourceToolArtifactBasis::issue(),
            inventory: record::Inventory::default(),
            created: BTreeMap::new(),
            host: None,
            pending: None,
            phase: Phase::Loading,
            operation: None,
            error: None,
            received: None,
            #[cfg(all(test, windows))]
            next_io_gate: None,
        }
    }
}

impl ArtifactState {
    #[cfg(all(test, windows))]
    pub(super) fn test_decode_reclamations(
        bytes: &[u8],
    ) -> Vec<nara_tooling::EditorSourceToolReclamationRecord> {
        record::decode(bytes)
            .expect("valid artifact inventory")
            .reclamations
    }

    #[cfg(all(test, windows))]
    pub(super) fn test_fail_next_publication(
        &self,
    ) -> crate::project_host::file_publication::CleanupTestControl {
        self.host
            .as_ref()
            .expect("inventory publication host is admitted")
            .try_with(|host| host.test_fail_after_write())
            .expect("test publisher is available")
    }

    #[cfg(all(test, windows))]
    pub(super) fn test_allow_publication_cleanup(&self) {
        if let Some(host) = &self.host {
            host.try_with(|host| host.test_allow_cleanup());
        }
    }

    pub(super) fn view(&self) -> EditorSourceToolArtifactView {
        EditorSourceToolArtifactView {
            basis: self.basis.clone(),
            installations: self.inventory.installations.clone(),
            reclamations: self.inventory.reclamations.clone(),
            phase: self.phase,
            operation: self.operation.clone(),
            error: self.error.clone(),
            physical_work_pending: self.pending(),
        }
    }

    pub(super) fn pending(&self) -> bool {
        self.pending.is_some() || self.host.as_ref().is_some_and(RetainedHost::has_cleanup)
    }

    pub(super) fn reclamation_entry(
        &self,
        basis: &EditorSourceToolArtifactBasis,
        installation: &[u8; 32],
    ) -> Result<EditorSourceToolArtifactInstallation, Error> {
        self.require_basis(basis)?;
        self.inventory
            .installations
            .iter()
            .find(|entry| &entry.installed.installation == installation)
            .cloned()
            .ok_or(Error::Unregistered)
    }

    pub(super) fn creation_custody(
        &self,
        installation: &[u8; 32],
    ) -> Option<&Arc<EditorSourceToolInstallation>> {
        self.created.get(installation)
    }

    pub(super) fn release_creation_custody(&mut self, installation: &[u8; 32]) {
        self.created.remove(installation);
    }

    pub(super) fn same_reclamation_facts(
        &self,
        basis: &EditorSourceToolArtifactBasis,
        expected: &EditorSourceToolArtifactInstallation,
    ) -> Result<bool, Error> {
        let current = self.reclamation_entry(basis, &expected.installed.installation)?;
        Ok(record::same_facts(&current, expected))
    }

    pub(super) fn publication_host(&self) -> Option<&RetainedHost<FilePublicationHost>> {
        self.host.as_ref()
    }

    /// Uses continuously retained member objects even after rename outside the store.
    /// Serialized inventory identities never participate in this comparison.
    pub(super) fn contains_source_object(
        &self,
        candidate: &FileCapability,
    ) -> Result<bool, FsError> {
        for installation in self.created.values() {
            for member in installation.members() {
                if candidate.same_object(member.file())? {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn require_basis(&self, basis: &EditorSourceToolArtifactBasis) -> Result<(), Error> {
        if basis != &self.basis {
            return Err(Error::StaleBasis);
        }
        if self.pending() {
            return Err(Error::Busy);
        }
        match self.phase {
            Phase::Idle => Ok(()),
            Phase::RequiresReconcile => Err(Error::RequiresReconcile),
            Phase::Unavailable => Err(self.error.clone().unwrap_or(Error::Read)),
            _ => Err(Error::Busy),
        }
    }

    fn prepare(
        &self,
        root: DirectoryCapability,
        inventory: record::Inventory,
        registration: Option<Arc<EditorSourceToolInstallation>>,
        outcome: Outcome,
    ) -> Result<PreparedEditorSourceToolArtifacts, Error> {
        let bytes = record::encode(&inventory)?.into();
        Ok(PreparedEditorSourceToolArtifacts {
            root,
            basis: self.basis.clone(),
            inventory,
            bytes,
            registration,
            outcome,
        })
    }

    fn finish(&mut self, attempt: EditorSourceToolAttemptId, outcome: Outcome) {
        self.basis = EditorSourceToolArtifactBasis::issue();
        self.error = match &outcome {
            Outcome::Failed(error) => Some(error.clone()),
            Outcome::PublishedWithCleanupFailure => Some(Error::TaskCleanupFailed),
            _ => None,
        };
        self.operation = Some(EditorSourceToolArtifactAttempt {
            attempt,
            outcome: Some(outcome),
        });
    }
}

impl EditorProjectSession {
    pub(super) fn initialize_source_tool_artifacts(&mut self) {
        self.start_artifact_work(io::Request::Observe, None, false);
    }

    /// Observes inventory and actual work return without I/O or automatic reconciliation.
    #[must_use]
    pub fn source_tool_artifact_view(&self) -> EditorSourceToolArtifactView {
        self.source_tools.artifacts.view()
    }

    /// Prepares exact installation facts using the installation's retained project association.
    /// Performs bounded code-free reads; call during preparation, never painting.
    pub fn prepare_source_tool_artifact_registration(
        &self,
        basis: &EditorSourceToolArtifactBasis,
        installation: Arc<EditorSourceToolInstallation>,
    ) -> Result<PreparedEditorSourceToolArtifacts, Error> {
        self.require_artifact_preparation(basis)?;
        let state = &self.source_tools.artifacts;
        let id = *installation.receipt().installation_id();
        if !state.created.contains_key(&id) && state.created.len() == record::MAX_INSTALLATIONS {
            return Err(Error::TooLarge);
        }
        if state
            .created
            .get(&id)
            .is_some_and(|previous| !Arc::ptr_eq(previous, &installation))
        {
            return Err(Error::Changed);
        }
        let root = self.source_project_root();
        validate_registration(&root, &installation)?;
        self.require_unowned_artifact_members(&installation)?;
        let mut entry = installation_facts(&installation);
        observe_members(installation.directory(), &mut entry, Some(&installation));
        state.prepare(
            root,
            state.inventory.register(entry)?,
            Some(installation),
            Outcome::Registered,
        )
    }

    /// Refreshes only the finite declared members. Digests observe the default data stream.
    /// Missing members are observations, never invented successful-removal receipts.
    pub fn prepare_source_tool_artifact_observation(
        &self,
        basis: &EditorSourceToolArtifactBasis,
        installation: &[u8; 32],
    ) -> Result<PreparedEditorSourceToolArtifacts, Error> {
        self.require_artifact_preparation(basis)?;
        let state = &self.source_tools.artifacts;
        let root = self.source_project_root();
        let mut inventory = state.inventory.clone();
        let entry = inventory
            .installations
            .iter_mut()
            .find(|entry| &entry.installed.installation == installation)
            .ok_or(Error::Unregistered)?;
        let path = RelativePath::new(entry.installed.directory.as_ref())
            .map_err(|_| Error::InvalidPath)?;
        let created = state.created.get(installation).map(Arc::as_ref);
        match root.open_directory_exact(&path) {
            Ok(directory) => {
                let same_directory = created
                    .map(|created| directory.same_object(created.directory()))
                    .transpose();
                match same_directory {
                    Ok(Some(false)) => set_observations(entry, record::Observation::Changed),
                    Err(_) => set_observations(entry, record::Observation::Failed),
                    _ => observe_members(&directory, entry, created),
                }
            }
            Err(error) if store::is_missing(&error) => {
                set_observations(entry, record::Observation::Missing)
            }
            Err(_) => set_observations(entry, record::Observation::Failed),
        }
        state.prepare(root, inventory, None, Outcome::Observed)
    }

    /// Confirms the original candidate. Acknowledgment waits for actual task return even if its
    /// logical result was cancelled; an uncertain publication requires explicit reconciliation.
    pub fn commit_source_tool_artifacts(
        &mut self,
        prepared: PreparedEditorSourceToolArtifacts,
    ) -> Result<EditorSourceToolAttemptId, Error> {
        self.advance_source_tools();
        self.require_artifact_preparation(&prepared.basis)?;
        if !prepared
            .root
            .same_object(&self.source_project_root())
            .map_err(|_| Error::RootMismatch)?
        {
            return Err(Error::RootMismatch);
        }
        if let Some(installation) = &prepared.registration {
            validate_registration(&prepared.root, installation)?;
            self.require_unowned_artifact_members(installation)?;
            self.source_tools.artifacts.created.insert(
                *installation.receipt().installation_id(),
                Arc::clone(installation),
            );
        }
        let request = io::Request::Publish {
            bytes: Arc::clone(&prepared.bytes),
            host: self.source_tools.artifacts.host.clone(),
            registration: prepared.registration.clone(),
        };
        Ok(self.start_artifact_work(request, Some(prepared), false))
    }

    /// Reads current disk facts without writes, sticky merging or recovery of live authority.
    pub fn reconcile_source_tool_artifacts(
        &mut self,
        basis: &EditorSourceToolArtifactBasis,
    ) -> Result<EditorSourceToolAttemptId, Error> {
        if self.source_tools.reclamation.pending() {
            return Err(Error::Busy);
        }
        if !self.accepts_new_work() || self.schema_adoption_fenced() {
            return Err(Error::Closing);
        }
        let state = &self.source_tools.artifacts;
        if basis != &state.basis {
            return Err(Error::StaleBasis);
        }
        if state.pending() {
            return Err(Error::Busy);
        }
        Ok(self.start_artifact_work(io::Request::Observe, None, true))
    }

    fn require_artifact_preparation(
        &self,
        basis: &EditorSourceToolArtifactBasis,
    ) -> Result<(), Error> {
        if !self.accepts_new_work() || self.schema_adoption_fenced() {
            return Err(Error::Closing);
        }
        if self.source_tools.reclamation.fenced() {
            return Err(Error::Busy);
        }
        self.source_tools.artifacts.require_basis(basis)
    }

    // Registration must also work when an ordinary source was admitted before registration.
    // Check its retained original and current namespace object without invoking store admission.
    fn require_unowned_artifact_members(
        &self,
        installation: &EditorSourceToolInstallation,
    ) -> Result<(), Error> {
        for member in installation.members() {
            self.require_unowned_artifact_object(member.file())?;
            let path = RelativePath::new(member.path()).map_err(|_| Error::InvalidPath)?;
            match installation.directory().open_file_exact(&path) {
                Ok(current) => self.require_unowned_artifact_object(&current)?,
                Err(error) if store::is_missing(&error) => {}
                Err(_) => return Err(Error::Read),
            }
        }
        Ok(())
    }

    pub(super) fn require_unowned_artifact_object(
        &self,
        current: &FileCapability,
    ) -> Result<(), Error> {
        let check = |host: &mut FilePublicationHost| {
            exclude_publication_alias(
                current,
                host.same_source_object(current),
                host.read_source(),
            )
        };
        let map = |error| match error {
            SourceAdmissionError::AlreadyOpen | SourceAdmissionError::PrivateArtifact => {
                Error::SourceOwned
            }
            SourceAdmissionError::HostUnavailable => Error::Busy,
            SourceAdmissionError::SourceRead => Error::Read,
        };
        for lane in self.scene_lanes.values() {
            lane.host
                .try_with(|host| {
                    exclude_publication_alias(
                        current,
                        host.same_source_object(current),
                        host.read_source(),
                    )
                })
                .ok_or(Error::Busy)?
                .map_err(map)?;
        }
        for lane in self.prefab_lanes.values() {
            lane.host
                .try_with(|host| {
                    exclude_publication_alias(
                        current,
                        host.same_source_object(current),
                        host.read_source(),
                    )
                })
                .ok_or(Error::Busy)?
                .map_err(map)?;
        }
        for lane in self.extension_documents.lanes() {
            lane.host.try_with(check).ok_or(Error::Busy)?.map_err(map)?;
        }
        Ok(())
    }

    /// Only the admitted reclamation owner publishes its exact intent/result through this lane.
    pub(super) fn publish_reclamation_record(
        &mut self,
        basis: &EditorSourceToolArtifactBasis,
        record: &nara_tooling::EditorSourceToolReclamationRecord,
    ) -> Result<EditorSourceToolAttemptId, Error> {
        // Already owned removal must settle its result before Close shuts its publisher down.
        // This private drain permission cannot admit a new intent during Close.
        if !self.authoring_runtime.host_custody().is_accepting()
            || self.schema_adoption_fenced()
            || (self.closing
                && record.phase != nara_tooling::EditorSourceToolReclamationRecordPhase::Results)
        {
            return Err(Error::Closing);
        }
        let state = &self.source_tools.artifacts;
        state.require_basis(basis)?;
        let prepared = state.prepare(
            self.source_project_root(),
            state.inventory.with_reclamation(record)?,
            None,
            Outcome::ReclamationRecorded,
        )?;
        let request = io::Request::Publish {
            bytes: Arc::clone(&prepared.bytes),
            host: self.source_tools.artifacts.host.clone(),
            registration: None,
        };
        Ok(self.start_artifact_work(request, Some(prepared), false))
    }

    fn start_artifact_work(
        &mut self,
        request: io::Request,
        candidate: Option<PreparedEditorSourceToolArtifacts>,
        reconcile: bool,
    ) -> EditorSourceToolAttemptId {
        let attempt = EditorSourceToolAttemptId::issue();
        let pending = io::Pending::start(
            self.source_project_root(),
            self.authoring_runtime.host_custody().clone(),
            self.authoring_runtime.persistence_tasks(),
            request,
            candidate,
            attempt.clone(),
            reconcile,
            #[cfg(all(test, windows))]
            self.source_tools.artifacts.next_io_gate.take(),
        );
        let state = &mut self.source_tools.artifacts;
        state.phase = if pending.candidate.is_some() {
            Phase::Publishing
        } else {
            Phase::Reconciling
        };
        state.basis = EditorSourceToolArtifactBasis::issue();
        state.error = None;
        state.operation = Some(EditorSourceToolArtifactAttempt {
            attempt: attempt.clone(),
            outcome: None,
        });
        state.pending = Some(pending);
        attempt
    }

    pub(in crate::project_host::runtime::editor) fn advance_source_tool_artifacts(&mut self) {
        if let Some(host) = &self.source_tools.artifacts.host {
            host.try_with(|host| host.retry_cleanup());
        }
        let Some(pending) = self.source_tools.artifacts.pending.as_mut() else {
            return;
        };
        let Some(completed) = pending.poll() else {
            return;
        };
        let pending = self
            .source_tools
            .artifacts
            .pending
            .take()
            .expect("polled artifact task");
        let state = &mut self.source_tools.artifacts;
        let mut outcome = match &completed.result {
            Ok(io::IoResult::Observed { inventory, host }) => {
                if let Some(previous) = state.host.take() {
                    previous.retire();
                }
                state.host = host.clone();
                state.inventory = inventory.clone();
                state.phase = Phase::Idle;
                if pending.reconcile {
                    Outcome::Reconciled
                } else {
                    Outcome::Observed
                }
            }
            Ok(io::IoResult::Published { host, accepted, .. }) => {
                state.host = Some(host.clone());
                if *accepted {
                    let candidate = pending
                        .candidate
                        .as_ref()
                        .expect("publication retains candidate");
                    state.inventory = candidate.inventory.clone();
                    state.phase = Phase::Idle;
                    candidate.outcome.clone()
                } else {
                    state.phase = Phase::RequiresReconcile;
                    Outcome::Failed(Error::RequiresReconcile)
                }
            }
            Ok(io::IoResult::Refused { host, error, .. }) => {
                state.host = Some(host.clone());
                state.phase = failed_phase(error, false);
                Outcome::Failed(error.clone())
            }
            Err(error) => {
                state.phase = failed_phase(error, pending.candidate.is_none());
                Outcome::Failed(error.clone())
            }
        };
        if completed.return_state == nara_tasks::TaskReturnState::CleanupFailed {
            state.phase = Phase::RequiresReconcile;
            outcome = if matches!(
                &completed.result,
                Ok(io::IoResult::Published { accepted: true, .. })
            ) {
                Outcome::PublishedWithCleanupFailure
            } else {
                Outcome::Failed(Error::TaskCleanupFailed)
            };
        }
        state.finish(pending.attempt, outcome);
        if let Some(candidate) = pending.candidate {
            state.received = Some(io::ReceivedPublication {
                _candidate: candidate,
                _completed: completed,
            });
        }
    }
}

fn failed_phase(error: &Error, observing: bool) -> Phase {
    if matches!(
        error,
        Error::RequiresReconcile | Error::Changed | Error::TaskCleanupFailed
    ) {
        Phase::RequiresReconcile
    } else if observing {
        Phase::Unavailable
    } else {
        Phase::Idle
    }
}

fn installation_facts(
    installation: &EditorSourceToolInstallation,
) -> EditorSourceToolArtifactInstallation {
    EditorSourceToolArtifactInstallation {
        installed: EditorSourceToolInstalledSelection {
            package: installation.package().into(),
            directory: installation.project_path().into(),
            installation: *installation.receipt().installation_id(),
            receipt: installation.receipt_digest(),
            manifest: installation.receipt().manifest_digest(),
        },
        members: installation
            .members()
            .iter()
            .map(|member| EditorSourceToolArtifactMember {
                path: member.path().into(),
                role: member_role(member.kind()),
                baseline: member.digest(),
                observation: record::Observation::Present,
            })
            .collect(),
    }
}

fn validate_registration(
    root: &DirectoryCapability,
    installation: &EditorSourceToolInstallation,
) -> Result<(), Error> {
    record::validate_directory(installation.project_path())?;
    if !installation
        .same_project(root)
        .map_err(|_| Error::RootMismatch)?
    {
        return Err(Error::RootMismatch);
    }
    let path = RelativePath::new(installation.project_path()).map_err(|_| Error::InvalidPath)?;
    let current = root
        .open_directory_exact(&path)
        .map_err(|_| Error::Changed)?;
    if !current
        .same_object(installation.directory())
        .map_err(|_| Error::Changed)?
    {
        return Err(Error::Changed);
    }
    Ok(())
}

fn member_role(kind: Option<EditorSourceToolReleaseMemberKind>) -> record::Role {
    match kind {
        Some(EditorSourceToolReleaseMemberKind::Tool) => record::Role::Tool,
        Some(EditorSourceToolReleaseMemberKind::Content) => record::Role::Content,
        None => record::Role::Metadata,
    }
}

fn set_observations(entry: &mut record::Installation, observation: record::Observation) {
    for member in &mut entry.members {
        member.observation = observation;
    }
}

fn observe_members(
    directory: &DirectoryCapability,
    entry: &mut record::Installation,
    created: Option<&EditorSourceToolInstallation>,
) {
    for member in &mut entry.members {
        member.observation = observe_member(directory, member, created);
    }
}

fn observe_member(
    directory: &DirectoryCapability,
    member: &record::Member,
    created: Option<&EditorSourceToolInstallation>,
) -> record::Observation {
    use record::Observation;
    let Ok(path) = RelativePath::new(member.path.as_ref()) else {
        return Observation::Failed;
    };
    let current = match directory.open_file_exact(&path) {
        Ok(current) => current,
        Err(error) if store::is_missing(&error) => return Observation::Missing,
        Err(_) => return Observation::Failed,
    };
    if let Some(created) = created {
        let Some(original) = created
            .members()
            .iter()
            .find(|original| original.path() == member.path.as_ref())
        else {
            return Observation::Failed;
        };
        match current.same_object(original.file()) {
            Ok(true) => {}
            Ok(false) => return Observation::Changed,
            Err(_) => return Observation::Failed,
        }
    }
    match current.digest(member.baseline.length()) {
        Ok(digest) if digest == member.baseline => Observation::Present,
        Ok(_) | Err(FsError::ByteLimitExceeded { .. }) => Observation::Changed,
        Err(_) => Observation::Failed,
    }
}
