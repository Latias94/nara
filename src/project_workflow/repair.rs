use std::{
    fmt,
    io::Write,
    sync::atomic::{AtomicU64, Ordering},
};

use nara_diagnostic::DiagnosticReport;
use nara_fs::{
    ContentDigest, ExpectedTarget, FsError, LockMode, RelativeComponent, RelativePath,
    ReplaceReceipt, StageStatus, TemporaryFileFailure,
};
use nara_project::{
    DEFAULT_MANIFEST_BYTE_LIMIT, ProjectManifest, ProjectManifestMutationError,
    ProjectManifestWriteError, ProjectPath,
};
use thiserror::Error;

use super::{ProjectCheckError, ProjectCheckRequest, ProjectStartupSceneIssue, check_project};
use crate::project_host::{
    ProjectContentMemberKind, StartupContentClosureReceipt,
    check::check_project_source_from_manifest_bytes,
};

const PROJECT_MANIFEST: &str = "nara.toml";
const TEMPORARY_NAME_ATTEMPTS: usize = 16;
static NEXT_TEMPORARY_NAME: AtomicU64 = AtomicU64::new(1);

/// Inputs for one explicit repair of a missing selected startup Scene.
pub struct ProjectStartupSceneRepairRequest {
    issue: ProjectStartupSceneIssue,
    replacement: ProjectPath,
}

impl ProjectStartupSceneRepairRequest {
    #[must_use]
    pub const fn new(issue: ProjectStartupSceneIssue, replacement: ProjectPath) -> Self {
        Self { issue, replacement }
    }
}

impl fmt::Debug for ProjectStartupSceneRepairRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectStartupSceneRepairRequest")
            .field("issue", &self.issue)
            .field("replacement", &self.replacement)
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum ProjectStartupSceneRepairError {
    #[error("the project no longer requires startup Scene repair")]
    NoLongerRequired,
    #[error("the startup Scene repair issue is stale")]
    StaleIssue,
    #[error("the current project no longer has the expected repairable failure")]
    CurrentProject(#[source] ProjectCheckError),
    #[error("the project manifest is locked by another cooperative writer")]
    LockUnavailable,
    #[error("project manifest filesystem access failed")]
    Filesystem(#[source] FsError),
    #[error("the current project manifest cannot be modeled for repair")]
    ManifestInvalid { diagnostics: Box<DiagnosticReport> },
    #[error(transparent)]
    ManifestMutation(#[from] ProjectManifestMutationError),
    #[error(transparent)]
    ManifestWrite(#[from] ProjectManifestWriteError),
    #[error("the replacement startup Scene does not form an admissible startup closure")]
    Replacement(#[source] ProjectCheckError),
    #[error("the prepared startup closure does not contain the selected replacement Scene")]
    PreparedFactsUnavailable,
    #[error("the filesystem cannot provide atomic manifest replacement")]
    UnsupportedPublication,
    #[error("manifest publication failed with retained temporary cleanup ownership")]
    TemporaryFile(#[source] TemporaryFileFailure),
}

impl ProjectStartupSceneRepairError {
    #[must_use]
    pub const fn diagnostics(&self) -> Option<&DiagnosticReport> {
        match self {
            Self::ManifestInvalid { diagnostics } => Some(diagnostics),
            Self::NoLongerRequired
            | Self::StaleIssue
            | Self::CurrentProject(_)
            | Self::LockUnavailable
            | Self::Filesystem(_)
            | Self::ManifestMutation(_)
            | Self::ManifestWrite(_)
            | Self::Replacement(_)
            | Self::PreparedFactsUnavailable
            | Self::UnsupportedPublication
            | Self::TemporaryFile(_) => None,
        }
    }
}

/// Exact manifest-publication facts shared by successful and post-check-failed outcomes.
pub struct ProjectStartupScenePublicationReceipt {
    previous_manifest_digest: ContentDigest,
    manifest_digest: ContentDigest,
    replacement: ProjectPath,
    replacement_scene_digest: ContentDigest,
    prepared_content_digest: ContentDigest,
    publication: ReplaceReceipt,
    parent_directory_sync: StageStatus,
}

impl ProjectStartupScenePublicationReceipt {
    #[must_use]
    pub const fn previous_manifest_digest(&self) -> ContentDigest {
        self.previous_manifest_digest
    }

    #[must_use]
    pub const fn manifest_digest(&self) -> ContentDigest {
        self.manifest_digest
    }

    #[must_use]
    pub const fn replacement(&self) -> &ProjectPath {
        &self.replacement
    }

    #[must_use]
    pub const fn replacement_scene_digest(&self) -> ContentDigest {
        self.replacement_scene_digest
    }

    #[must_use]
    pub const fn prepared_content_digest(&self) -> ContentDigest {
        self.prepared_content_digest
    }

    #[must_use]
    pub const fn publication(&self) -> &ReplaceReceipt {
        &self.publication
    }

    #[must_use]
    pub const fn parent_directory_sync(&self) -> StageStatus {
        self.parent_directory_sync
    }
}

impl fmt::Debug for ProjectStartupScenePublicationReceipt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectStartupScenePublicationReceipt")
            .field("previous_manifest_digest", &self.previous_manifest_digest)
            .field("manifest_digest", &self.manifest_digest)
            .field("replacement", &self.replacement)
            .field("replacement_scene_digest", &self.replacement_scene_digest)
            .field("prepared_content_digest", &self.prepared_content_digest)
            .field("publication", &self.publication)
            .field("parent_directory_sync", &self.parent_directory_sync)
            .finish()
    }
}

/// Successful repair after exact post-publication ordinary checking.
pub struct ProjectStartupSceneRepairReceipt {
    publication: ProjectStartupScenePublicationReceipt,
    startup_content: StartupContentClosureReceipt,
}

impl ProjectStartupSceneRepairReceipt {
    #[must_use]
    pub const fn publication(&self) -> &ProjectStartupScenePublicationReceipt {
        &self.publication
    }

    #[must_use]
    pub const fn startup_content(&self) -> &StartupContentClosureReceipt {
        &self.startup_content
    }
}

impl fmt::Debug for ProjectStartupSceneRepairReceipt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectStartupSceneRepairReceipt")
            .field("publication", &self.publication)
            .field("startup_content", &self.startup_content)
            .finish()
    }
}

#[derive(Debug, Error)]
pub enum ProjectStartupScenePostcheckError {
    #[error("the published manifest failed ordinary project checking")]
    Check(#[source] ProjectCheckError),
    #[error("the post-publication project facts differ from the prepared repair")]
    PreparedFactsChanged,
    #[error("the manifest publication receipt lacks exact published-byte evidence")]
    PublicationEvidenceIncomplete,
}

/// A manifest name switch occurred, but the operation cannot truthfully report repair success.
pub struct PublishedButNotRepaired {
    publication: ProjectStartupScenePublicationReceipt,
    error: ProjectStartupScenePostcheckError,
}

impl PublishedButNotRepaired {
    #[must_use]
    pub const fn publication(&self) -> &ProjectStartupScenePublicationReceipt {
        &self.publication
    }

    #[must_use]
    pub const fn error(&self) -> &ProjectStartupScenePostcheckError {
        &self.error
    }
}

impl fmt::Debug for PublishedButNotRepaired {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PublishedButNotRepaired")
            .field("publication", &self.publication)
            .field("error", &self.error)
            .finish()
    }
}

#[must_use = "the startup Scene repair terminal outcome must be handled"]
pub enum ProjectStartupSceneRepairOutcome {
    Repaired(ProjectStartupSceneRepairReceipt),
    PublishedButNotRepaired(PublishedButNotRepaired),
}

impl fmt::Debug for ProjectStartupSceneRepairOutcome {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Repaired(receipt) => formatter.debug_tuple("Repaired").field(receipt).finish(),
            Self::PublishedButNotRepaired(outcome) => formatter
                .debug_tuple("PublishedButNotRepaired")
                .field(outcome)
                .finish(),
        }
    }
}

/// Rechecks, prepares, publishes and post-checks one explicit startup Scene replacement.
pub fn repair_startup_scene(
    request: ProjectStartupSceneRepairRequest,
) -> Result<ProjectStartupSceneRepairOutcome, ProjectStartupSceneRepairError> {
    let ProjectStartupSceneRepairRequest { issue, replacement } = request;
    revalidate_issue(&issue)?;

    let manifest_path = RelativePath::new(PROJECT_MANIFEST)
        .expect("the engine-owned manifest name is a valid relative path");
    let (manifest_parent, manifest_name) = issue
        .project_root
        .resolve_file_parent(&manifest_path)
        .map_err(ProjectStartupSceneRepairError::Filesystem)?;
    let manifest = manifest_parent
        .open_child_file(&manifest_name)
        .map_err(ProjectStartupSceneRepairError::Filesystem)?;
    if manifest.identity() != issue.manifest_identity {
        return Err(ProjectStartupSceneRepairError::StaleIssue);
    }
    let manifest = match manifest.try_lock(LockMode::Exclusive) {
        Ok(manifest) => manifest,
        Err(failure) => {
            let (error, _file) = failure.into_parts();
            return Err(match error {
                FsError::LockContended => ProjectStartupSceneRepairError::LockUnavailable,
                error => ProjectStartupSceneRepairError::Filesystem(error),
            });
        }
    };
    verify_manifest_digest(&manifest, issue.manifest_digest)?;

    let manifest_bytes = manifest
        .read_to_end_bounded(DEFAULT_MANIFEST_BYTE_LIMIT)
        .map_err(ProjectStartupSceneRepairError::Filesystem)?;
    let load = ProjectManifest::parse_toml_bytes(&manifest_bytes);
    if load.has_errors() || load.manifest.is_none() {
        return Err(ProjectStartupSceneRepairError::ManifestInvalid {
            diagnostics: Box::new(load.diagnostics),
        });
    }
    let mut modeled = load
        .manifest
        .expect("a valid manifest load contains modeled fields");
    modeled.replace_startup_scene(issue.profile.as_deref(), replacement.clone())?;
    let published_bytes = modeled.to_canonical_toml_bytes()?;
    let manifest_digest = ContentDigest::of_bytes(&published_bytes);

    let checked = check_project_source_from_manifest_bytes(
        issue.project_root.share(),
        issue.profile.as_deref(),
        issue.recipe.clone(),
        &published_bytes,
    )
    .map_err(|error| {
        ProjectStartupSceneRepairError::Replacement(ProjectCheckError::from_source(
            error,
            issue.project_root.share(),
            issue.profile.clone(),
            issue.recipe.clone(),
        ))
    })?;
    let (_, prepared_snapshot) = checked.into_parts();
    let prepared = prepared_snapshot
        .startup_content_closure()
        .expect("the startup check selects the manifest's default Scene");
    let replacement_scene_digest =
        startup_scene_digest(&prepared, &issue.scenes_path, &replacement)
            .ok_or(ProjectStartupSceneRepairError::PreparedFactsUnavailable)?;

    if manifest.identity() != issue.manifest_identity {
        return Err(ProjectStartupSceneRepairError::StaleIssue);
    }
    verify_manifest_digest(&manifest, issue.manifest_digest)?;
    manifest_parent
        .preflight_replace(ExpectedTarget::Identity(issue.manifest_identity))
        .map_err(|_| ProjectStartupSceneRepairError::UnsupportedPublication)?;

    let mut temporary = create_temporary(&manifest_parent)
        .map_err(ProjectStartupSceneRepairError::TemporaryFile)?;
    if let Err(source) = temporary
        .write_all(&published_bytes)
        .and_then(|()| temporary.flush())
    {
        return Err(ProjectStartupSceneRepairError::TemporaryFile(
            TemporaryFileFailure::with_cleanup(
                FsError::Io {
                    operation: nara_fs::FsOperation::Write,
                    source,
                },
                temporary.into_cleanup(),
            ),
        ));
    }
    let sync = match temporary.sync() {
        Ok(sync) => sync,
        Err(source) => {
            return Err(ProjectStartupSceneRepairError::TemporaryFile(
                TemporaryFileFailure::with_cleanup(source, temporary.into_cleanup()),
            ));
        }
    };
    if sync.progress().data_synced() != StageStatus::Achieved
        || sync.progress().file_metadata_synced() != StageStatus::Achieved
    {
        return Err(ProjectStartupSceneRepairError::TemporaryFile(
            TemporaryFileFailure::with_cleanup(
                FsError::Unproven {
                    operation: nara_fs::FsOperation::SyncFile,
                    proof: "manifest candidate lacks required synchronization",
                },
                temporary.into_cleanup(),
            ),
        ));
    }
    let publication = match manifest_parent.replace_temp(
        temporary,
        &manifest_name,
        ExpectedTarget::Identity(issue.manifest_identity),
    ) {
        Ok(receipt) => receipt,
        Err(failure) => {
            return Err(ProjectStartupSceneRepairError::TemporaryFile(failure));
        }
    };
    drop(manifest);

    let parent_directory_sync = manifest_parent
        .sync()
        .map_or(StageStatus::Unknown, |receipt| {
            receipt.progress().parent_directory_synced()
        });
    let publication = ProjectStartupScenePublicationReceipt {
        previous_manifest_digest: issue.manifest_digest,
        manifest_digest,
        replacement,
        replacement_scene_digest,
        prepared_content_digest: prepared.content_digest(),
        publication,
        parent_directory_sync,
    };

    let fresh = match check_project(check_request(&issue)) {
        Ok(fresh) => fresh,
        Err(error) => {
            return Ok(ProjectStartupSceneRepairOutcome::PublishedButNotRepaired(
                PublishedButNotRepaired {
                    publication,
                    error: ProjectStartupScenePostcheckError::Check(error),
                },
            ));
        }
    };
    if publication.publication.verified_content_digest() != Some(manifest_digest) {
        return Ok(ProjectStartupSceneRepairOutcome::PublishedButNotRepaired(
            PublishedButNotRepaired {
                publication,
                error: ProjectStartupScenePostcheckError::PublicationEvidenceIncomplete,
            },
        ));
    }
    if fresh.manifest_digest() != manifest_digest
        || fresh.settings_lineage() != prepared.settings_lineage()
        || fresh.schema_fingerprint() != prepared.schema_fingerprint()
        || fresh.startup_content().content_digest() != prepared.content_digest()
        || startup_scene_digest(
            fresh.startup_content(),
            &issue.scenes_path,
            publication.replacement(),
        ) != Some(replacement_scene_digest)
    {
        return Ok(ProjectStartupSceneRepairOutcome::PublishedButNotRepaired(
            PublishedButNotRepaired {
                publication,
                error: ProjectStartupScenePostcheckError::PreparedFactsChanged,
            },
        ));
    }

    Ok(ProjectStartupSceneRepairOutcome::Repaired(
        ProjectStartupSceneRepairReceipt {
            publication,
            startup_content: fresh.startup_content().clone(),
        },
    ))
}

fn revalidate_issue(
    expected: &ProjectStartupSceneIssue,
) -> Result<(), ProjectStartupSceneRepairError> {
    match check_project(check_request(expected)) {
        Ok(_) => Err(ProjectStartupSceneRepairError::NoLongerRequired),
        Err(error) => {
            if error.startup_scene_issue().is_none() {
                return Err(ProjectStartupSceneRepairError::CurrentProject(error));
            }
            let current = error
                .into_startup_scene_issue()
                .expect("the issue was observed before consuming the check error");
            if issue_facts_match(expected, &current) {
                Ok(())
            } else {
                Err(ProjectStartupSceneRepairError::StaleIssue)
            }
        }
    }
}

fn check_request(issue: &ProjectStartupSceneIssue) -> ProjectCheckRequest {
    let request = ProjectCheckRequest::new(issue.project_root.share(), issue.recipe.clone());
    match &issue.profile {
        Some(profile) => request.with_profile(profile.clone()),
        None => request,
    }
}

fn verify_manifest_digest(
    manifest: &nara_fs::LockedFileCapability,
    expected: ContentDigest,
) -> Result<(), ProjectStartupSceneRepairError> {
    match manifest.verify_digest(expected, DEFAULT_MANIFEST_BYTE_LIMIT) {
        Ok(()) => Ok(()),
        Err(FsError::DigestMismatch { .. }) => Err(ProjectStartupSceneRepairError::StaleIssue),
        Err(error) => Err(ProjectStartupSceneRepairError::Filesystem(error)),
    }
}

fn issue_facts_match(
    expected: &ProjectStartupSceneIssue,
    current: &ProjectStartupSceneIssue,
) -> bool {
    expected.project_root.identity() == current.project_root.identity()
        && expected.profile == current.profile
        && expected.manifest_identity == current.manifest_identity
        && expected.manifest_digest == current.manifest_digest
        && expected.settings_lineage == current.settings_lineage
        && expected.scenes_path == current.scenes_path
        && expected.selected_scene == current.selected_scene
}

fn startup_scene_digest(
    closure: &StartupContentClosureReceipt,
    scenes_path: &ProjectPath,
    selected_scene: &ProjectPath,
) -> Option<ContentDigest> {
    let path = ProjectPath::new(format!(
        "{}/{}",
        scenes_path.as_str(),
        selected_scene.as_str()
    ))
    .ok()?;
    closure
        .members()
        .iter()
        .find(|member| member.kind() == ProjectContentMemberKind::Scene && member.path() == &path)
        .map(|member| member.digest())
}

fn create_temporary(
    parent: &nara_fs::DirectoryCapability,
) -> Result<nara_fs::TemporaryFile, TemporaryFileFailure> {
    for _ in 0..TEMPORARY_NAME_ATTEMPTS {
        let sequence = NEXT_TEMPORARY_NAME.fetch_add(1, Ordering::Relaxed);
        let name = RelativeComponent::new(format!(
            ".nara-project-repair-{}-{sequence}.tmp",
            std::process::id()
        ))
        .expect("the engine-owned temporary filename is valid");
        match parent.create_temp(&name) {
            Ok(temporary) => return Ok(temporary),
            Err(error) if matches!(error.error(), FsError::AlreadyExists { .. }) => continue,
            Err(error) => return Err(error),
        }
    }
    Err(FsError::AlreadyExists {
        operation: nara_fs::FsOperation::CreateTemporary,
    }
    .into())
}
