//! Final live installation with the original Workspace and every original persistence Host held.

use nara_tooling::{EditorDocumentKind, EditorPersistenceRejection};

use super::*;
use crate::project_host::persistence::{
    ExtractionCheckpointLeafCommit, PersistenceLeafValidationCost, PrefabPersistenceHost,
    ScenePersistenceHost,
};

/// The Session holds its retained Host locks while lending these concrete destinations.
pub(crate) enum ExtractionPersistenceDestination<'a> {
    Scene {
        document: EditorDocumentId,
        project_path: &'a str,
        host: &'a mut ScenePersistenceHost,
    },
    Prefab {
        document: EditorDocumentId,
        project_path: &'a str,
        host: &'a mut PrefabPersistenceHost,
    },
}

impl ExtractionPersistenceDestination<'_> {
    fn document(&self) -> EditorDocumentId {
        match self {
            Self::Scene { document, .. } | Self::Prefab { document, .. } => *document,
        }
    }

    fn kind(&self) -> EditorDocumentKind {
        match self {
            Self::Scene { .. } => EditorDocumentKind::Scene,
            Self::Prefab { .. } => EditorDocumentKind::Prefab,
        }
    }

    fn project_path(&self) -> &str {
        match self {
            Self::Scene { project_path, .. } | Self::Prefab { project_path, .. } => project_path,
        }
    }

    fn accepted(&self) -> Option<(FileIdentity, Option<FileIdentity>, ContentDigest)> {
        match self {
            Self::Scene { host, .. } => host.accepted_source_observation(),
            Self::Prefab { host, .. } => host.accepted_source_observation(),
        }
    }

    fn has_cleanup(&self) -> bool {
        match self {
            Self::Scene { host, .. } => host.has_cleanup(),
            Self::Prefab { host, .. } => host.has_cleanup(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(in crate::project_host::authoring::project::prefab_extraction) enum PrefabExtractionInstallationError
{
    #[error("the complete original persistence destination set changed")]
    Destinations,
    #[error("an original persistence Host refused its actual completed extraction result: {0:?}")]
    Persistence(EditorPersistenceRejection),
    #[error("the final extraction Project refused validation or admission")]
    Project(#[from] PrefabExtractionProjectError),
    #[error("final extraction Ready publication was rejected: {0:?}")]
    Rejected(FilePublicationRejection),
    #[error("final extraction Ready publication failed: {0:?}")]
    Failed(FilePublicationFailureStage),
    #[error("final extraction Ready publication requires reconciliation")]
    Uncertain(FilePublicationEvidence),
}

impl From<SourceInventoryError> for PrefabExtractionInstallationError {
    fn from(error: SourceInventoryError) -> Self {
        Self::Project(PrefabExtractionProjectError::NativeInventory(error))
    }
}

#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct PrefabExtractionInstallationFailure
{
    pub(super) prepared: PreparedPrefabExtractionWorkspaceInstallation,
    pub(super) error: PrefabExtractionInstallationError,
}

impl fmt::Debug for PrefabExtractionInstallationFailure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionInstallationFailure")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl PrefabExtractionInstallationFailure {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_parts(
        self,
    ) -> (
        PreparedPrefabExtractionWorkspaceInstallation,
        PrefabExtractionInstallationError,
    ) {
        (self.prepared, self.error)
    }
}

#[must_use]
pub(in crate::project_host::authoring::project::prefab_extraction) struct InstalledPrefabExtractionProject
{
    pub(in crate::project_host::authoring::project::prefab_extraction) session:
        AuthoringProjectSessionSnapshot,
    // The successful phase retains resources without a cursor that could resume publication
    // after its accepted leaves have moved into the original Hosts.
    custody: FinishedExtractionCustody,
    pub(in crate::project_host::authoring::project::prefab_extraction) inventory: SourceInventory,
    pub(in crate::project_host::authoring::project::prefab_extraction) evidence:
        FilePublicationEvidence,
    // Optional evidence stays optional: observing an existing output is not historical publication.
    pub(in crate::project_host::authoring::project::prefab_extraction) receipts: Vec<(
        EditorDocumentId,
        Option<crate::project_host::persistence::EditorPersistenceReceipt>,
    )>,
}

struct FinishedExtractionCustody {
    origin: Origin,
    _remaining_results: Vec<Entry>,
}

/// Successful Session installation keeps its original resources without a publication cursor.
pub(in crate::project_host::authoring::project::prefab_extraction) struct RetainedPrefabExtractionInstallation
{
    custody: FinishedExtractionCustody,
    _inventory: SourceInventory,
    _evidence: FilePublicationEvidence,
}

impl RetainedPrefabExtractionInstallation {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn session_context(
        &self,
    ) -> &ProjectExtractionContext {
        match &self.custody.origin {
            Origin::LiveInstallation(owner) => owner.session_context(),
            _ => unreachable!("successful Session installation retains its live installation"),
        }
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn session_context_mut(
        &mut self,
    ) -> &mut ProjectExtractionContext {
        match &mut self.custody.origin {
            Origin::LiveInstallation(owner) => owner.session_context_mut(),
            _ => unreachable!("successful Session installation retains its live installation"),
        }
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn budget_mut(
        &mut self,
    ) -> &mut AuthoringProjectAdmissionBudget {
        self.custody
            .origin
            .inputs()
            .expect("finished live installation retains its actual budget")
            .budget
    }
}

impl InstalledPrefabExtractionProject {
    pub(in crate::project_host::authoring::project::prefab_extraction) fn into_session_parts(
        self,
    ) -> (
        AuthoringProjectSessionSnapshot,
        Vec<(
            EditorDocumentId,
            Option<crate::project_host::persistence::EditorPersistenceReceipt>,
        )>,
        RetainedPrefabExtractionInstallation,
    ) {
        let Self {
            session,
            custody,
            inventory,
            evidence,
            receipts,
        } = self;
        (
            session,
            receipts,
            RetainedPrefabExtractionInstallation {
                custody,
                _inventory: inventory,
                _evidence: evidence,
            },
        )
    }

    pub(in crate::project_host::authoring::project::prefab_extraction) fn host_usage(
        &mut self,
    ) -> AuthoringProjectAdmissionUsage {
        self.custody
            .origin
            .inputs()
            .expect("finished extraction retains original operation custody")
            .budget
            .usage()
    }
}

struct LeafCommit<'a> {
    entry: usize,
    document: EditorDocumentId,
    commit: ExtractionCheckpointLeafCommit<'a>,
}

impl BoundPrefabExtractionWorkspaceInstallation<'_> {
    /// The enclosing Session also holds lane/execution destinations and any retained Host locks.
    /// Every fallible validation and leaf binding precedes Ready; successful installation only
    /// consumes guards and moves their original evidence into already allocated result storage.
    pub(in crate::project_host::authoring::project::prefab_extraction) fn publish(
        self,
        destinations: &mut [ExtractionPersistenceDestination<'_>],
    ) -> Result<InstalledPrefabExtractionProject, PrefabExtractionInstallationFailure> {
        let Self {
            mut project,
            workspace,
        } = self;
        let admission = (|| {
            project.validate()?;
            let count = workspace.prepared().input().documents().len();
            let bytes = product(
                count,
                sum(
                    size_of::<LeafCommit<'_>>(),
                    size_of::<(
                        EditorDocumentId,
                        Option<crate::project_host::persistence::EditorPersistenceReceipt>,
                    )>(),
                )?,
            )?;
            let budget = project.project.future.publication.origin.inputs()?.budget;
            let before = budget.retained_candidate_bytes;
            if let Err(error) = budget.charge_source_bytes(0, 0, bytes, bytes) {
                let unused = budget.retained_candidate_bytes - before;
                release(budget, unused)?;
                return Err(PrefabExtractionProjectError::Admission(error));
            }
            Ok(bytes)
        })();
        let reserved = match admission {
            Ok(bytes) => bytes,
            Err(error) => return Err(failure(project, workspace, error.into())),
        };
        let result = publish_admitted(project, workspace, destinations);
        // Refusal drops temporary guard/result vectors; success retains its preallocated receipts.
        match result {
            Ok(mut installed) => {
                let kept = product(
                    installed.receipts.capacity(),
                    size_of::<(
                        EditorDocumentId,
                        Option<crate::project_host::persistence::EditorPersistenceReceipt>,
                    )>(),
                )
                .expect("receipt storage was admitted before Ready");
                let budget = installed
                    .custody
                    .origin
                    .inputs()
                    .expect("completed live installation retains its budget")
                    .budget;
                release(budget, reserved - kept).expect("dropped leaf guards keep no storage");
                Ok(installed)
            }
            Err(mut refusal) => {
                let budget = refusal
                    .prepared
                    .project
                    .project
                    .future
                    .publication
                    .origin
                    .inputs()
                    .expect("refusal retains original installation custody")
                    .budget;
                release(budget, reserved).expect("refused guard collections have dropped");
                Err(refusal)
            }
        }
    }
}

fn publish_admitted(
    mut project: BoundPrefabExtractionProject,
    workspace: EditorWorkspaceExtractionPersistenceCommit<'_>,
    destinations: &mut [ExtractionPersistenceDestination<'_>],
) -> Result<InstalledPrefabExtractionProject, PrefabExtractionInstallationFailure> {
    let count = workspace.prepared().input().documents().len();
    let mut commits = Vec::with_capacity(count);
    let mut receipts = Vec::with_capacity(count);
    if let Err(error) = bind_leaves(&mut project, &workspace, destinations, &mut commits) {
        restore(&mut project, commits);
        return Err(failure(project, workspace, error));
    }
    match project.publish() {
        Ok(PrefabExtractionProjectOutcome::Published(published)) => {
            workspace.commit();
            for leaf in commits {
                receipts.push((leaf.document, leaf.commit.commit()));
            }
            let PublishedPrefabExtractionProject {
                session,
                files,
                inventory,
                evidence,
            } = published;
            let ProjectPrefabExtractionPublication {
                origin,
                entries,
                active,
                uncertain,
                ..
            } = files;
            debug_assert!(active.is_none() && uncertain.is_none());
            Ok(InstalledPrefabExtractionProject {
                session,
                custody: FinishedExtractionCustody {
                    origin,
                    _remaining_results: entries,
                },
                inventory,
                evidence,
                receipts,
            })
        }
        refused => {
            let (mut project, error) = match refused {
                Err(refusal) => (refusal.bound, refusal.error.into()),
                Ok(PrefabExtractionProjectOutcome::Rejected { bound, reason }) => {
                    (bound, PrefabExtractionInstallationError::Rejected(reason))
                }
                Ok(PrefabExtractionProjectOutcome::Failed { bound, stage }) => {
                    (bound, PrefabExtractionInstallationError::Failed(stage))
                }
                Ok(PrefabExtractionProjectOutcome::Uncertain { bound, evidence }) => (
                    bound,
                    PrefabExtractionInstallationError::Uncertain(evidence),
                ),
                Ok(PrefabExtractionProjectOutcome::Published(_)) => unreachable!(),
            };
            restore(&mut project, commits);
            Err(failure(project, workspace, error))
        }
    }
}

fn bind_leaves<'a>(
    project: &mut BoundPrefabExtractionProject,
    workspace: &EditorWorkspaceExtractionPersistenceCommit<'_>,
    destinations: &'a mut [ExtractionPersistenceDestination<'_>],
    commits: &mut Vec<LeafCommit<'a>>,
) -> Result<(), PrefabExtractionInstallationError> {
    let input = workspace.prepared().input();
    if destinations.len() != input.documents().len()
        || !destinations
            .iter()
            .map(ExtractionPersistenceDestination::document)
            .eq(input.documents())
    {
        return Err(PrefabExtractionInstallationError::Destinations);
    }
    for destination in destinations {
        let document = destination.document();
        if input.document_kind(document) != Some(destination.kind()) {
            return Err(PrefabExtractionInstallationError::Destinations);
        }
        let binding = input
            .document_source_binding(document)
            .ok_or(PrefabExtractionInstallationError::Destinations)?;
        let files = &mut project.project.future.publication;
        let inputs = files
            .origin
            .inputs()
            .map_err(PrefabExtractionProjectError::from)?;
        // Capture retains physical paths in sorted order. A lane supplies its actual project
        // path, distinct from its root-relative Workspace binding; neither is inferred from kind.
        let path = destination.project_path();
        let original_open = inputs
            .original_open
            .ok_or(PrefabExtractionInstallationError::Destinations)?;
        let comparisons = (usize::BITS - original_open.len().leading_zeros()) as usize + 1;
        inputs
            .budget
            .charge_source_bytes(
                0,
                0,
                product(
                    comparisons,
                    sum(path.len(), size_of::<PrefabExtractionOpenInput>())?,
                )?,
                0,
            )
            .map_err(PrefabExtractionProjectError::from)?;
        let open = original_open
            .binary_search_by(|open| {
                open.source
                    .as_ref()
                    .map(AuthoringDocumentSourceFact::relative_path)
                    .cmp(&Some(path))
            })
            .ok()
            .and_then(|index| original_open.get(index))
            .ok_or(PrefabExtractionInstallationError::Destinations)?;
        if open.document != document {
            return Err(PrefabExtractionInstallationError::Destinations);
        }
        let original = open
            .source
            .as_ref()
            .ok_or(PrefabExtractionInstallationError::Destinations)?;
        let expected = (
            original.identity(),
            original.parent_identity(),
            original.digest(),
        );
        if destination.has_cleanup() {
            return Err(PrefabExtractionInstallationError::Persistence(
                EditorPersistenceRejection::CleanupPending,
            ));
        }
        if destination.accepted() != Some(expected) {
            return Err(PrefabExtractionInstallationError::Persistence(
                EditorPersistenceRejection::TargetChanged,
            ));
        }
        let path = original.relative_path();
        let originals = inputs.pending.recovery().complete_original_sources();
        let comparisons = (usize::BITS - originals.len().leading_zeros()) as usize + 1;
        inputs
            .budget
            .charge_source_bytes(
                0,
                0,
                product(comparisons, sum(path.len(), size_of::<usize>())?)?,
                0,
            )
            .map_err(PrefabExtractionProjectError::from)?;
        let source = originals
            .binary_search_by(|source| source.path().cmp(path))
            .map_err(|_| PrefabExtractionInstallationError::Destinations)?;
        if originals[source].output().is_none() {
            continue;
        }
        inputs
            .budget
            .charge_source_bytes(
                0,
                0,
                product(
                    (usize::BITS - files.entries.len().leading_zeros()) as usize + 1,
                    size_of::<ExtractionFileMember>(),
                )?,
                0,
            )
            .map_err(PrefabExtractionProjectError::from)?;
        // Q and its sidecar precede sparse original outputs, whose source indices stay sorted.
        let entry = files.entries[2..]
            .binary_search_by_key(&source, |entry| match entry.member {
                ExtractionFileMember::Original(source) => source,
                _ => unreachable!("the fixed original-output table contains only original sources"),
            })
            .map_err(|_| PrefabExtractionInstallationError::Destinations)?
            + 2;
        let digest = files.entries[entry]
            .result
            .as_ref()
            .ok_or(PrefabExtractionInstallationError::Destinations)?
            .observation()
            .2;
        let cost = PersistenceLeafValidationCost::checked(digest, path.len())
            .ok_or(PrefabExtractionInstallationError::Destinations)?;
        let budget = inputs.budget;
        budget
            .admit_handle_peak(cost.transient_handles)
            .map_err(PrefabExtractionProjectError::from)?;
        let before = budget.retained_candidate_bytes;
        if let Err(error) =
            budget.charge_source_bytes(cost.encoded_bytes, 0, cost.work_bytes, cost.scratch_bytes)
        {
            let unused = budget.retained_candidate_bytes - before;
            release(budget, unused).map_err(PrefabExtractionProjectError::from)?;
            return Err(PrefabExtractionProjectError::Admission(error).into());
        }
        let result = files.entries[entry]
            .result
            .take()
            .expect("the validated result remains in its original entry");
        let ExtractionFileResult::Checkpoint(acceptance) = result else {
            files.entries[entry].result = Some(result);
            release(budget, cost.scratch_bytes).map_err(PrefabExtractionProjectError::from)?;
            return Err(PrefabExtractionInstallationError::Destinations);
        };
        let result = match destination {
            ExtractionPersistenceDestination::Scene { host, .. } => {
                acceptance.bind_scene_leaf(host, expected)
            }
            ExtractionPersistenceDestination::Prefab { host, .. } => {
                acceptance.bind_prefab_leaf(host, binding, expected)
            }
        };
        release(budget, cost.scratch_bytes)
            .expect("native leaf validation has physically returned");
        match result {
            Ok(commit) => commits.push(LeafCommit {
                entry,
                document,
                commit,
            }),
            Err(failure) => {
                let (acceptance, reason) = failure.into_parts();
                files.entries[entry].result = Some(ExtractionFileResult::Checkpoint(acceptance));
                return Err(PrefabExtractionInstallationError::Persistence(reason));
            }
        }
    }
    Ok(())
}

fn restore(project: &mut BoundPrefabExtractionProject, commits: Vec<LeafCommit<'_>>) {
    for leaf in commits {
        let slot = &mut project.project.future.publication.entries[leaf.entry].result;
        assert!(
            slot.is_none(),
            "the original file-result slot stays reserved while bound"
        );
        *slot = Some(ExtractionFileResult::Checkpoint(
            leaf.commit.into_acceptance(),
        ));
    }
}

fn failure(
    project: BoundPrefabExtractionProject,
    workspace: EditorWorkspaceExtractionPersistenceCommit<'_>,
    error: PrefabExtractionInstallationError,
) -> PrefabExtractionInstallationFailure {
    PrefabExtractionInstallationFailure {
        prepared: PreparedPrefabExtractionWorkspaceInstallation {
            project,
            persistence: workspace.into_prepared(),
        },
        error,
    }
}
