//! Finite artifact inventory publication, independent of ordinary document persistence.

use std::{collections::BTreeMap, fmt, sync::Arc};

use nara_fs::{DirectoryCapability, FileCapability, FsError, RelativePath};
use nara_tooling::{
    EditorSourceToolArtifact, EditorSourceToolArtifactAttempt, EditorSourceToolArtifactBasis,
    EditorSourceToolArtifactError as Error, EditorSourceToolArtifactMember,
    EditorSourceToolArtifactOutcome as Outcome, EditorSourceToolArtifactPhase as Phase,
    EditorSourceToolArtifactPublication, EditorSourceToolArtifactView, EditorSourceToolAttemptId,
};

use super::{
    EditorProjectSession, EditorSourceToolIncompletePublication, EditorSourceToolInstallation,
    EditorSourceToolReleaseMemberKind,
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
    registration: Option<ArtifactCustody>,
    outcome: Outcome,
}

impl fmt::Debug for PreparedEditorSourceToolArtifacts {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedEditorSourceToolArtifacts")
            .field("artifacts", &self.inventory.artifacts.len())
            .field("outcome", &self.outcome)
            .finish_non_exhaustive()
    }
}

impl PreparedEditorSourceToolArtifacts {
    #[must_use]
    pub fn artifacts(&self) -> Vec<EditorSourceToolArtifact> {
        self.inventory.artifacts.clone()
    }

    /// Recovers unaccepted incomplete custody for a fresh explicit registration review.
    pub fn into_incomplete_publication(
        mut self,
    ) -> Result<EditorSourceToolIncompletePublication, Self> {
        match self.registration.take() {
            Some(ArtifactCustody::Incomplete(publication)) => match Arc::try_unwrap(publication) {
                Ok(publication) => Ok(publication),
                Err(publication) => {
                    self.registration = Some(ArtifactCustody::Incomplete(publication));
                    Err(self)
                }
            },
            registration => {
                self.registration = registration;
                Err(self)
            }
        }
    }
}

/// Confirmation refusal retains the original candidate and its live publication custody.
#[derive(Debug, thiserror::Error)]
#[error("{error}")]
pub struct EditorSourceToolArtifactCommitError {
    error: Error,
    prepared: Box<PreparedEditorSourceToolArtifacts>,
}

impl EditorSourceToolArtifactCommitError {
    #[must_use]
    pub fn with_prepared(error: Error, prepared: PreparedEditorSourceToolArtifacts) -> Self {
        Self {
            error,
            prepared: Box::new(prepared),
        }
    }
    #[must_use]
    pub const fn error(&self) -> &Error {
        &self.error
    }
    #[must_use]
    pub fn into_parts(self) -> (Error, PreparedEditorSourceToolArtifacts) {
        (self.error, *self.prepared)
    }
}

/// Registration refusal preserves the exact incomplete publication and unresolved cleanup.
#[derive(Debug, thiserror::Error)]
#[error("{error}")]
pub struct EditorSourceToolIncompleteArtifactRegistrationError {
    error: Error,
    publication: Box<EditorSourceToolIncompletePublication>,
}

impl EditorSourceToolIncompleteArtifactRegistrationError {
    #[must_use]
    pub fn with_publication(
        error: Error,
        publication: EditorSourceToolIncompletePublication,
    ) -> Self {
        Self {
            error,
            publication: Box::new(publication),
        }
    }
    #[must_use]
    pub const fn error(&self) -> &Error {
        &self.error
    }
    #[must_use]
    pub fn into_parts(self) -> (Error, EditorSourceToolIncompletePublication) {
        (self.error, *self.publication)
    }
}

#[derive(Clone)]
pub(super) enum ArtifactCustody {
    Ready(Arc<EditorSourceToolInstallation>),
    Incomplete(Arc<EditorSourceToolIncompletePublication>),
}

impl ArtifactCustody {
    fn id(&self) -> &[u8; 32] {
        match self {
            Self::Ready(value) => value.receipt().installation_id(),
            Self::Incomplete(value) => value.id(),
        }
    }
    fn project_root(&self) -> &DirectoryCapability {
        match self {
            Self::Ready(value) => value.project_root(),
            Self::Incomplete(value) => value.project_root(),
        }
    }
    fn project_path(&self) -> &str {
        match self {
            Self::Ready(value) => value.project_path(),
            Self::Incomplete(value) => value.project_path(),
        }
    }
    pub(super) fn directory(&self) -> &DirectoryCapability {
        match self {
            Self::Ready(value) => value.directory(),
            Self::Incomplete(value) => value.directory(),
        }
    }
    pub(super) fn member_file(&self, path: &str) -> Option<&FileCapability> {
        match self {
            Self::Ready(value) => value
                .members()
                .iter()
                .find(|member| member.path() == path)
                .map(|member| member.file()),
            Self::Incomplete(value) => value
                .members()
                .iter()
                .find(|member| member.path() == path)
                .and_then(|member| member.file()),
        }
    }
    fn facts(&self) -> EditorSourceToolArtifact {
        match self {
            Self::Ready(value) => installation_facts(value),
            Self::Incomplete(value) => incomplete_facts(value),
        }
    }
    fn contains_source_object(&self, candidate: &FileCapability) -> Result<bool, FsError> {
        match self {
            Self::Ready(value) => {
                for member in value.members() {
                    if candidate.same_object(member.file())? {
                        return Ok(true);
                    }
                }
            }
            Self::Incomplete(value) => {
                for member in value.members() {
                    if candidate.same_object(member.file().ok_or(FsError::IdentityUnavailable)?)? {
                        return Ok(true);
                    }
                }
            }
        }
        Ok(false)
    }
}

pub(super) struct ArtifactState {
    basis: EditorSourceToolArtifactBasis,
    inventory: record::Inventory,
    // Confirmed registration retains actual objects, including after failed publication or a
    // weaker disk observation. It supplies alias custody only, never deletion authorization.
    created: BTreeMap<[u8; 32], ArtifactCustody>,
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
            artifacts: self.inventory.artifacts.clone(),
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
    ) -> Result<EditorSourceToolArtifact, Error> {
        self.require_basis(basis)?;
        self.inventory
            .artifacts
            .iter()
            .find(|entry| &entry.id == installation)
            .cloned()
            .ok_or(Error::Unregistered)
    }

    pub(super) fn creation_custody(&self, installation: &[u8; 32]) -> Option<&ArtifactCustody> {
        self.created.get(installation)
    }

    pub(super) fn release_creation_custody(&mut self, installation: &[u8; 32]) {
        self.created.remove(installation);
    }

    pub(super) fn same_reclamation_facts(
        &self,
        basis: &EditorSourceToolArtifactBasis,
        expected: &EditorSourceToolArtifact,
    ) -> Result<bool, Error> {
        let current = self.reclamation_entry(basis, &expected.id)?;
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
        for artifact in self.created.values() {
            if artifact.contains_source_object(candidate)? {
                return Ok(true);
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
        registration: Option<ArtifactCustody>,
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
        if !state.created.contains_key(&id) && state.created.len() == record::MAX_ARTIFACTS {
            return Err(Error::TooLarge);
        }
        if state.created.get(&id).is_some_and(|previous| {
            !matches!(previous,
                ArtifactCustody::Ready(previous) if Arc::ptr_eq(previous, &installation))
        }) {
            return Err(Error::Changed);
        }
        let root = self.source_project_root();
        let registration = ArtifactCustody::Ready(installation);
        validate_registration(&root, &registration)?;
        self.require_unowned_artifact_members(&registration)?;
        let mut entry = registration.facts();
        observe_members(registration.directory(), &mut entry, Some(&registration));
        state.prepare(
            root,
            state.inventory.register(entry)?,
            Some(registration),
            Outcome::Registered,
        )
    }

    /// Prepares registration again from this Session's continuously retained actual custody.
    /// After explicit reconciliation, serialized IDs alone cannot supply a missing owner.
    pub fn prepare_retained_source_tool_artifact_registration(
        &self,
        basis: &EditorSourceToolArtifactBasis,
        artifact: &[u8; 32],
    ) -> Result<PreparedEditorSourceToolArtifacts, Error> {
        self.require_artifact_preparation(basis)?;
        let state = &self.source_tools.artifacts;
        let retained = state.created.get(artifact).ok_or(Error::Unregistered)?;
        let root = self.source_project_root();
        validate_registration(&root, retained)?;
        self.require_unowned_artifact_members(retained)?;
        let mut entry = retained.facts();
        observe_members(retained.directory(), &mut entry, Some(retained));
        state.prepare(
            root,
            state.inventory.register(entry)?,
            Some(retained.clone()),
            Outcome::Registered,
        )
    }

    /// Retains actual unready publication facts. A refusal returns its exact live custody.
    /// Reader handover closes duplicate handles only; preparation never discards a file.
    pub fn prepare_source_tool_incomplete_artifact_registration(
        &self,
        basis: &EditorSourceToolArtifactBasis,
        mut publication: EditorSourceToolIncompletePublication,
    ) -> Result<
        PreparedEditorSourceToolArtifacts,
        EditorSourceToolIncompleteArtifactRegistrationError,
    > {
        let prepared = (|| {
            self.require_artifact_preparation(basis)?;
            let state = &self.source_tools.artifacts;
            if state.created.contains_key(publication.id()) {
                return Err(Error::Changed);
            }
            if state.created.len() == record::MAX_ARTIFACTS {
                return Err(Error::TooLarge);
            }
            let root = self.source_project_root();
            record::validate_directory(publication.project_path())?;
            if !root
                .same_object(publication.project_root())
                .map_err(|_| Error::RootMismatch)?
            {
                return Err(Error::RootMismatch);
            }
            publication
                .hand_over_readers()
                .map_err(|_| Error::Changed)?;
            let mut entry = incomplete_facts(&publication);
            for member in &mut entry.members {
                let original = publication
                    .members()
                    .iter()
                    .find(|original| original.path() == member.path.as_ref())
                    .and_then(|original| original.file())
                    .ok_or(Error::Read)?;
                self.require_unowned_artifact_object(original)?;
                let current = publication
                    .directory()
                    .open_file_exact(
                        &RelativePath::new(member.path.as_ref()).map_err(|_| Error::InvalidPath)?,
                    )
                    .map_err(|_| Error::Changed)?;
                if !current.same_object(original).map_err(|_| Error::Read)? {
                    return Err(Error::Changed);
                }
                self.require_unowned_artifact_object(&current)?;
                member.observation = observe_file(publication.directory(), member, Some(original));
            }
            let inventory = state.inventory.register(entry)?;
            let bytes = record::encode(&inventory)?.into();
            Ok((root, inventory, bytes))
        })();
        match prepared {
            Ok((root, inventory, bytes)) => Ok(PreparedEditorSourceToolArtifacts {
                root,
                basis: basis.clone(),
                inventory,
                bytes,
                registration: Some(ArtifactCustody::Incomplete(Arc::new(publication))),
                outcome: Outcome::Registered,
            }),
            Err(error) => Err(
                EditorSourceToolIncompleteArtifactRegistrationError::with_publication(
                    error,
                    publication,
                ),
            ),
        }
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
            .artifacts
            .iter_mut()
            .find(|entry| &entry.id == installation)
            .ok_or(Error::Unregistered)?;
        let path = RelativePath::new(entry.directory.as_ref()).map_err(|_| Error::InvalidPath)?;
        let created = state.created.get(installation);
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
    ) -> Result<EditorSourceToolAttemptId, EditorSourceToolArtifactCommitError> {
        self.advance_source_tools();
        let admission = (|| {
            self.require_artifact_preparation(&prepared.basis)?;
            if !prepared
                .root
                .same_object(&self.source_project_root())
                .map_err(|_| Error::RootMismatch)?
            {
                return Err(Error::RootMismatch);
            }
            if let Some(artifact) = &prepared.registration {
                validate_registration(&prepared.root, artifact)?;
                self.require_unowned_artifact_members(artifact)?;
            }
            Ok(())
        })();
        if let Err(error) = admission {
            return Err(EditorSourceToolArtifactCommitError::with_prepared(
                error, prepared,
            ));
        }
        if let Some(installation) = &prepared.registration {
            self.source_tools
                .artifacts
                .created
                .insert(*installation.id(), installation.clone());
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
    fn require_unowned_artifact_members(&self, artifact: &ArtifactCustody) -> Result<(), Error> {
        for member in artifact.facts().members {
            self.require_unowned_artifact_object(
                artifact.member_file(&member.path).ok_or(Error::Read)?,
            )?;
            let path = RelativePath::new(member.path.as_ref()).map_err(|_| Error::InvalidPath)?;
            match artifact.directory().open_file_exact(&path) {
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

fn installation_facts(installation: &EditorSourceToolInstallation) -> EditorSourceToolArtifact {
    EditorSourceToolArtifact {
        id: *installation.receipt().installation_id(),
        package: installation.package().into(),
        directory: installation.project_path().into(),
        manifest: installation.receipt().manifest_digest(),
        publication: EditorSourceToolArtifactPublication::Ready {
            receipt: installation.receipt_digest(),
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

fn incomplete_facts(
    publication: &EditorSourceToolIncompletePublication,
) -> EditorSourceToolArtifact {
    EditorSourceToolArtifact {
        id: *publication.id(),
        package: publication.package().into(),
        directory: publication.project_path().into(),
        manifest: publication.manifest_digest(),
        publication: EditorSourceToolArtifactPublication::Incomplete,
        members: publication
            .members()
            .iter()
            .map(|member| EditorSourceToolArtifactMember {
                path: member.path().into(),
                role: member.role(),
                baseline: member.baseline(),
                observation: record::Observation::Failed,
            })
            .collect(),
    }
}

fn validate_registration(
    root: &DirectoryCapability,
    installation: &ArtifactCustody,
) -> Result<(), Error> {
    record::validate_directory(installation.project_path())?;
    if !installation
        .project_root()
        .same_object(root)
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

fn set_observations(entry: &mut record::Artifact, observation: record::Observation) {
    for member in &mut entry.members {
        member.observation = observation;
    }
}

fn observe_members(
    directory: &DirectoryCapability,
    entry: &mut record::Artifact,
    created: Option<&ArtifactCustody>,
) {
    for member in &mut entry.members {
        member.observation = observe_member(directory, member, created);
    }
}

fn observe_member(
    directory: &DirectoryCapability,
    member: &record::Member,
    created: Option<&ArtifactCustody>,
) -> record::Observation {
    let original = match created {
        Some(created) => match created.member_file(&member.path) {
            Some(file) => Some(file),
            None => return record::Observation::Failed,
        },
        None => None,
    };
    observe_file(directory, member, original)
}

fn observe_file(
    directory: &DirectoryCapability,
    member: &record::Member,
    original: Option<&FileCapability>,
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
    if let Some(original) = original {
        match current.same_object(original) {
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
