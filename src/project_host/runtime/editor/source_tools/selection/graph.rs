//! Exact installed relationships share the existing selection publisher and Session executor.

use nara_fs::ContentDigest;
use nara_tooling::{EditorSourceToolInstallationRequest, EditorSourceToolWithdrawalMode};

use super::super::artifact_store;
use super::*;
use crate::project_host::runtime::editor::source_tools::release::{
    EditorSourceToolReleaseError, ReleaseCaptureBudget,
};

const CAPTURE_BYTES: usize = 128 * 1024 * 1024;

/// Execution retains captured top-level images and rechecks actual required installations.
#[derive(Debug)]
pub(in crate::project_host::runtime::editor::source_tools) struct InstalledToolClosure {
    root: DirectoryCapability,
    entries: Vec<EditorSourceToolInstalledEntry>,
    selected: [u8; 32],
}

impl InstalledToolClosure {
    pub(in crate::project_host::runtime::editor::source_tools) fn verify(
        &self,
    ) -> Result<(), Error> {
        let mut budget = ReleaseCaptureBudget::new(CAPTURE_BYTES);
        for entry in &self.entries {
            if entry.installed.installation == self.selected {
                continue;
            }
            let inspection = inspect(
                &self.root,
                &entry.installed.directory,
                entry.installed.receipt,
                &mut budget,
            )?;
            if installed_facts(Arc::clone(&entry.installed.directory), &inspection)
                != entry.installed
            {
                return Err(Error::Installation);
            }
            verify_requirements(inspection.release().requires(), entry, &self.entries)?;
        }
        Ok(())
    }
}

/// A code-free additive successor pinned to the original Session selection observation.
#[derive(Debug)]
pub struct PreparedEditorSourceToolInstallations {
    root: DirectoryCapability,
    basis: EditorSourceToolSelectionBasis,
    graph: Graph,
}

impl PreparedEditorSourceToolInstallations {
    #[must_use]
    pub fn installations(&self) -> &[EditorSourceToolInstalledEntry] {
        &self.graph.entries
    }
}

/// A read-only review of the exact reverse required closure, without file deletion authority.
#[derive(Debug)]
pub struct PreparedEditorSourceToolWithdrawal {
    basis: EditorSourceToolSelectionBasis,
    predecessor: Option<EditorSourceToolActivationId>,
    graph: Graph,
    affected: Vec<EditorSourceToolInstalledEntry>,
    withdraw_route: bool,
}

/// Executable provenance is independent of a route's optional durable selection origin.
pub(super) struct RouteObservation {
    pub(super) activation: EditorSourceToolActivationId,
    approved: Option<Arc<managed::ApprovedManagedLaunch>>,
    linked: Vec<EditorSourceToolInstalledEntry>,
}

impl RouteObservation {
    pub(super) fn of(active: &Activation) -> Self {
        Self {
            activation: active.active.id().clone(),
            approved: active.definition.command.approved.clone(),
            linked: active.requirements.clone(),
        }
    }

    fn nominal_entries(&self, graph: &Graph) -> Result<Vec<EditorSourceToolInstalledEntry>, Error> {
        if !self.linked.is_empty() {
            return Ok(self.linked.clone());
        }
        let Some(approved) = &self.approved else {
            return Ok(Vec::new());
        };
        let (_, package, installation, receipt, manifest) = approved.installation_facts();
        let Some(entry) = graph.entries.iter().find(|entry| {
            entry.installed.installation == *installation
                && entry.installed.package.as_ref() == package
                && entry.installed.receipt == receipt
                && entry.installed.manifest == manifest
        }) else {
            return Ok(Vec::new());
        };
        graph.closure(&entry.installed.installation)
    }

    pub(super) fn entries(
        &self,
        root: &DirectoryCapability,
        graph: &Graph,
    ) -> Result<Vec<EditorSourceToolInstalledEntry>, Error> {
        let entries = self.nominal_entries(graph)?;
        if !self.linked.is_empty() || entries.is_empty() {
            return Ok(entries);
        }
        let approved = self
            .approved
            .as_ref()
            .expect("an unlinked nominal origin requires approval");
        let (origin, _, installation, _, _) = approved.installation_facts();
        let entry = entries
            .iter()
            .find(|entry| entry.installed.installation == *installation)
            .expect("nominal closure includes its selected member");
        if !artifact_store::validate_path(&entry.installed.directory) {
            return Err(Error::InvalidPath);
        }
        let root = root.strict_read_only().map_err(|_| Error::Installation)?;
        let directory = root
            .open_directory_exact(
                &RelativePath::new(entry.installed.directory.as_ref())
                    .map_err(|_| Error::InvalidPath)?,
            )
            .map_err(|_| Error::Installation)?;
        if !directory
            .same_object(origin)
            .map_err(|_| Error::Installation)?
        {
            return Ok(Vec::new());
        }
        Ok(entries)
    }
}

impl PreparedEditorSourceToolWithdrawal {
    #[must_use]
    pub fn installations(&self) -> &[EditorSourceToolInstalledEntry] {
        &self.affected
    }
}

fn capture_error(error: EditorSourceToolInstallationError) -> Error {
    match error {
        EditorSourceToolInstallationError::Inspection(
            EditorSourceToolReleaseError::CaptureBudget,
        ) => Error::CaptureBudget,
        _ => Error::Installation,
    }
}

fn inspect(
    root: &DirectoryCapability,
    directory: &str,
    receipt: ContentDigest,
    budget: &mut ReleaseCaptureBudget,
) -> Result<EditorSourceToolInstallationInspection, Error> {
    if !artifact_store::validate_path(directory) {
        return Err(Error::InvalidPath);
    }
    let root = root.strict_read_only().map_err(|_| Error::Installation)?;
    let path = RelativePath::new(directory).map_err(|_| Error::InvalidPath)?;
    let directory = root
        .open_directory_exact(&path)
        .map_err(|_| Error::Installation)?;
    EditorSourceToolInstallationInspection::read_with_budget(&directory, receipt, budget)
        .map_err(capture_error)
}

fn installed_facts(
    directory: Arc<str>,
    inspection: &EditorSourceToolInstallationInspection,
) -> EditorSourceToolInstalledSelection {
    EditorSourceToolInstalledSelection {
        package: Arc::from(inspection.release().package()),
        directory,
        installation: *inspection.receipt().installation_id(),
        receipt: inspection.receipt_digest(),
        manifest: inspection.release().manifest_digest(),
    }
}

fn verify_requirements(
    requires: &[EditorSourceToolReleaseRequirement],
    entry: &EditorSourceToolInstalledEntry,
    entries: &[EditorSourceToolInstalledEntry],
) -> Result<(), Error> {
    if requires.len() != entry.requirements.len() {
        return Err(Error::Graph);
    }
    for requirement in requires {
        let binding = entry
            .requirements
            .iter()
            .find(|binding| binding.package.as_ref() == requirement.package())
            .ok_or(Error::Graph)?;
        let target = entries
            .iter()
            .find(|target| target.installed.installation == binding.installation)
            .ok_or(Error::Graph)?;
        if target.installed.package.as_ref() != requirement.package()
            || target.installed.manifest != requirement.manifest_digest()
        {
            return Err(Error::Graph);
        }
    }
    Ok(())
}

pub(super) fn verify_entries(
    root: &DirectoryCapability,
    entries: &[EditorSourceToolInstalledEntry],
) -> Result<(), Error> {
    let mut budget = ReleaseCaptureBudget::new(CAPTURE_BYTES);
    for entry in entries {
        let inspection = inspect(
            root,
            &entry.installed.directory,
            entry.installed.receipt,
            &mut budget,
        )?;
        if installed_facts(Arc::clone(&entry.installed.directory), &inspection) != entry.installed {
            return Err(Error::Installation);
        }
        verify_requirements(inspection.release().requires(), entry, entries)?;
    }
    Ok(())
}

impl EditorProjectSession {
    /// Captures exact local installations without probing code or changing project metadata.
    ///
    /// Performs bounded IO; use Host preparation rather than painting. Requests explicitly bind
    /// each requirement to an installation ID, including members added in this same operation.
    pub fn prepare_source_tool_installations(
        &self,
        basis: &EditorSourceToolSelectionBasis,
        project: DirectoryCapability,
        requests: &[EditorSourceToolInstallationRequest],
    ) -> Result<PreparedEditorSourceToolInstallations, Error> {
        self.require_graph_mutation(basis)?;
        if requests.is_empty() || requests.len() > 32 {
            return Err(Error::Graph);
        }
        if !project
            .same_object(&self.source_project_root())
            .map_err(|_| Error::RootMismatch)?
        {
            return Err(Error::RootMismatch);
        }
        let mut budget = ReleaseCaptureBudget::new(CAPTURE_BYTES);
        let mut additions = Vec::with_capacity(requests.len());
        // Retain only bounded graph metadata between sequential captures. Actual immutable
        // executable images are acquired later by the selected managed approval operation.
        let mut declarations = Vec::with_capacity(requests.len());
        for request in requests {
            if !artifact_store::validate_path(&request.directory) {
                return Err(Error::InvalidPath);
            }
            if request.requirements.len() > 16
                || request
                    .requirements
                    .iter()
                    .any(|binding| binding.package.len() > 128)
            {
                return Err(Error::Graph);
            }
            let inspection = inspect(&project, &request.directory, request.receipt, &mut budget)?;
            declarations.push(inspection.release().requires().to_vec());
            additions.push(EditorSourceToolInstalledEntry {
                installed: installed_facts(Arc::clone(&request.directory), &inspection),
                requirements: request.requirements.clone(),
            });
        }
        let graph = self.source_tools.selection.graph.register(&additions)?;
        for (entry, requires) in additions.iter().zip(declarations) {
            verify_requirements(&requires, entry, &graph.entries)?;
        }
        store::encode(&graph)?;
        Ok(PreparedEditorSourceToolInstallations {
            root: project,
            basis: basis.clone(),
            graph,
        })
    }

    /// Publishes the prepared additive set using the original file observation and Session basis.
    pub fn register_source_tool_installations(
        &mut self,
        prepared: PreparedEditorSourceToolInstallations,
    ) -> Result<EditorSourceToolAttemptId, Error> {
        self.advance_source_tools();
        self.require_graph_mutation(&prepared.basis)?;
        if !prepared
            .root
            .same_object(&self.source_project_root())
            .map_err(|_| Error::RootMismatch)?
        {
            return Err(Error::RootMismatch);
        }
        self.start_graph_publication(prepared.graph, Mutation::Register, Some(prepared.root))
    }

    /// Reviews all required dependents, including disabled tools, without changing routes or files.
    pub fn prepare_source_tool_withdrawal(
        &self,
        basis: &EditorSourceToolSelectionBasis,
        installation: &[u8; 32],
        mode: EditorSourceToolWithdrawalMode,
    ) -> Result<PreparedEditorSourceToolWithdrawal, Error> {
        self.require_graph_mutation(basis)?;
        let (graph, affected) = self
            .source_tools
            .selection
            .graph
            .withdraw(installation, mode)?;
        let withdraw_route = match self.source_tools.active.as_ref() {
            Some(active) => {
                let route = RouteObservation::of(active);
                let nominal = route.nominal_entries(&self.source_tools.selection.graph)?;
                if nominal.iter().any(|entry| affected.contains(entry)) {
                    // An unreadable implicated installation cannot keep executing through graph
                    // withdrawal. A proven different object remains an external installation.
                    route
                        .entries(
                            &self.source_project_root(),
                            &self.source_tools.selection.graph,
                        )
                        .map_or(true, |entries| {
                            entries.iter().any(|entry| affected.contains(entry))
                        })
                } else {
                    false
                }
            }
            None => false,
        };
        Ok(PreparedEditorSourceToolWithdrawal {
            basis: basis.clone(),
            predecessor: self
                .source_tools
                .active
                .as_ref()
                .map(|active| active.active.id().clone()),
            graph,
            affected,
            withdraw_route,
        })
    }

    /// Withdraws the exact reviewed set. This operation neither removes files nor undoes Schema.
    ///
    /// Affected execution stops immediately. Durable outcome and physical child return remain
    /// separate observations; failed publication never restarts stopped execution.
    pub fn withdraw_source_tool_installations(
        &mut self,
        prepared: PreparedEditorSourceToolWithdrawal,
    ) -> Result<EditorSourceToolAttemptId, Error> {
        self.advance_source_tools();
        self.require_graph_mutation(&prepared.basis)?;
        let predecessor = self
            .source_tools
            .active
            .as_ref()
            .map(|active| active.active.id().clone());
        if predecessor != prepared.predecessor {
            return Err(Error::StaleBasis);
        }
        if prepared.withdraw_route {
            self.source_tools.cancel();
        }
        self.start_graph_publication(prepared.graph, Mutation::Withdraw, None)
    }

    fn require_graph_mutation(&self, basis: &EditorSourceToolSelectionBasis) -> Result<(), Error> {
        if !self.accepts_new_work() {
            return Err(Error::Closing);
        }
        if self.schema_adoption_fenced() || self.source_tools.probe.is_some() {
            return Err(Error::Busy);
        }
        self.source_tools.selection.check_basis(basis)?;
        self.source_tools.selection.writable()
    }

    fn start_graph_publication(
        &mut self,
        graph: Graph,
        mutation: Mutation,
        approved_project: Option<DirectoryCapability>,
    ) -> Result<EditorSourceToolAttemptId, Error> {
        store::encode(&graph)?;
        let verify = if matches!(mutation, Mutation::Register) {
            graph.entries.clone()
        } else {
            Vec::new()
        };
        let attempt = EditorSourceToolAttemptId::issue();
        #[cfg(test)]
        let gate = self.source_tools.selection.next_io_gate.take();
        let pending = PendingIo::start(
            self.source_project_root(),
            self.authoring_runtime.host_custody().clone(),
            self.authoring_runtime.persistence_tasks(),
            IoRequest::Publish {
                graph,
                mutation,
                verify,
                host: self.source_tools.selection.host.clone(),
                approved_project,
            },
            attempt.clone(),
            false,
            #[cfg(test)]
            gate,
        );
        let selection = &mut self.source_tools.selection;
        selection.basis = EditorSourceToolSelectionBasis::issue();
        selection.phase = Phase::Publishing;
        selection.operation = Some(EditorSourceToolSelectionAttempt {
            attempt: attempt.clone(),
            outcome: None,
        });
        selection.error = None;
        selection.pending = Some(pending);
        Ok(attempt)
    }
}

pub(super) fn prepare_required_selection(
    session: &EditorProjectSession,
    project: DirectoryCapability,
    directory: &str,
    receipt: ContentDigest,
    host: &EditorSourceToolManagedHost,
) -> Result<PreparedEditorSourceToolSelection, Error> {
    session.source_tools.selection.writable()?;
    let mut budget = ReleaseCaptureBudget::new(CAPTURE_BYTES);
    let inspection = inspect(&project, directory, receipt, &mut budget)?;
    let installed = installed_facts(Arc::from(directory), &inspection);
    let desired = EditorSourceToolDesiredSelection {
        installed: installed.clone(),
        enabled: true,
    };
    let registered = session
        .source_tools
        .selection
        .graph
        .entries
        .iter()
        .find(|entry| entry.installed == installed);
    if !inspection.release().requires().is_empty() && registered.is_none() {
        return Err(Error::Graph);
    }
    let graph = session
        .source_tools
        .selection
        .graph
        .select(desired.clone())?;
    let closure = graph.closure(&installed.installation)?;
    verify_requirements(
        inspection.release().requires(),
        closure
            .iter()
            .find(|entry| entry.installed == installed)
            .ok_or(Error::Graph)?,
        &closure,
    )?;
    for entry in &closure {
        if entry.installed == installed {
            continue;
        }
        let observed = inspect(
            &project,
            &entry.installed.directory,
            entry.installed.receipt,
            &mut budget,
        )?;
        if installed_facts(Arc::clone(&entry.installed.directory), &observed) != entry.installed {
            return Err(Error::Installation);
        }
        verify_requirements(observed.release().requires(), entry, &closure)?;
    }
    let mut definition = host
        .approve_graph_installation(inspection)
        .map_err(|_| Error::NotApproved)?;
    if closure.len() > 1 {
        definition.command.installed = Some(Arc::new(InstalledToolClosure {
            root: project.share(),
            entries: closure,
            selected: installed.installation,
        }));
    }
    Ok(PreparedEditorSourceToolSelection {
        root: project,
        desired,
        definition,
        graph,
        basis: Some(session.source_tools.selection.basis.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project_host::runtime::editor::source_tools::release::tests::Release;

    #[test]
    fn capture_rejects_arbitrary_or_nested_installation_paths_before_reading_artifacts() {
        let release = Release::new();
        let root = release.root(nara_fs::TrustMode::Untrusted);
        let receipt = ContentDigest::of_bytes(b"not an installation receipt");
        for directory in [
            "tools/install-1",
            ".nara/source-tools/install-1",
            ".nara/source-tools/artifacts",
            ".nara/source-tools/artifacts/",
            ".nara/source-tools/artifacts/install-1/child",
            ".NARA/source-tools/artifacts/install-1",
            ".nara/source-tools/artifacts/工具",
        ] {
            let mut budget = ReleaseCaptureBudget::new(0);
            assert!(
                matches!(
                    inspect(&root, directory, receipt, &mut budget),
                    Err(Error::InvalidPath)
                ),
                "{directory}"
            );
        }
        let mut budget = ReleaseCaptureBudget::new(0);
        assert!(matches!(
            inspect(
                &root,
                ".nara/source-tools/artifacts/install-1",
                receipt,
                &mut budget,
            ),
            Err(Error::Installation)
        ));
    }
}
