use std::{
    error::Error,
    fmt::{self, Debug, Display, Formatter},
};

use nara_fs::{
    ContentDigest, DirectoryCapability, DirectoryPublicationBeginError, DirectoryPublicationOrphan,
    DirectoryPublicationOutcome, DirectoryPublicationReceipt, DirectoryPublicationRejection,
    FileIdentity, FsError, RelativeComponent,
};
use nara_reflect::SchemaCompositionFingerprint;

use super::{
    creation::PreparedProjectCreation,
    dependency::{DependencyRevalidationError, PreparedDependencyRevalidation},
    starter::{ProjectCreationPlanReceipt, ProjectMember},
};
use crate::project_host::{
    ProjectCandidateError, ProjectContentError, ProjectContentRevision, ProjectSettingsLineage,
    RuntimePlanError,
};

mod admission;
mod stage;

pub use stage::ProjectTreeStageError;

use admission::admit_published;
use stage::{next_staging_name, stage_project_tree};

#[must_use = "the project-creation terminal outcome must be handled"]
#[derive(Debug)]
pub enum ProjectCreationOutcome {
    Created(ProjectCreationReceipt),
    Rejected(ProjectCreationRejection),
    Orphaned(ProjectCreationOrphan),
    Unresolved(ProjectCreationUnresolved),
    PublishedButUnadmitted(PublishedButUnadmitted),
}

pub struct ProjectCreationReceipt {
    plan: ProjectCreationPlanReceipt,
    publication: DirectoryPublicationReceipt,
    project_root: DirectoryCapability,
    settings_lineage: ProjectSettingsLineage,
    schema_fingerprint: SchemaCompositionFingerprint,
    content_revision: ProjectContentRevision,
    content_digest: ContentDigest,
}

impl ProjectCreationReceipt {
    #[must_use]
    pub const fn plan(&self) -> &ProjectCreationPlanReceipt {
        &self.plan
    }

    #[must_use]
    pub const fn publication(&self) -> &DirectoryPublicationReceipt {
        &self.publication
    }

    #[must_use]
    pub fn project_root(&self) -> DirectoryCapability {
        self.project_root.share()
    }

    #[must_use]
    pub fn project_root_identity(&self) -> FileIdentity {
        self.project_root.identity()
    }

    #[must_use]
    pub const fn settings_lineage(&self) -> ProjectSettingsLineage {
        self.settings_lineage
    }

    #[must_use]
    pub const fn schema_fingerprint(&self) -> SchemaCompositionFingerprint {
        self.schema_fingerprint
    }

    #[must_use]
    pub const fn content_revision(&self) -> ProjectContentRevision {
        self.content_revision
    }

    #[must_use]
    pub const fn content_digest(&self) -> ContentDigest {
        self.content_digest
    }
}

impl Debug for ProjectCreationReceipt {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectCreationReceipt")
            .field("plan", &self.plan)
            .field("publication", &self.publication)
            .field("project_root_identity", &self.project_root.identity())
            .field("settings_lineage", &self.settings_lineage)
            .field("schema_fingerprint", &self.schema_fingerprint)
            .field("content_revision", &self.content_revision)
            .field("content_digest", &self.content_digest)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectCreationRejectionKind {
    StagingNameEntropy,
    DependencyRelation,
    BeforeStaging,
    DirectoryPublication,
    Staging,
}

pub struct ProjectCreationRejection {
    plan: ProjectCreationPlanReceipt,
    inner: ProjectCreationRejectionInner,
}

enum ProjectCreationRejectionInner {
    StagingNameEntropy(getrandom::Error),
    DependencyRelation(DependencyRevalidationError),
    BeforeStaging(FsError),
    DirectoryPublication(DirectoryPublicationRejection),
    Staging {
        error: ProjectTreeStageError,
        publication: DirectoryPublicationRejection,
    },
}

impl ProjectCreationRejection {
    #[must_use]
    pub const fn plan(&self) -> &ProjectCreationPlanReceipt {
        &self.plan
    }

    #[must_use]
    pub const fn kind(&self) -> ProjectCreationRejectionKind {
        match &self.inner {
            ProjectCreationRejectionInner::StagingNameEntropy(_) => {
                ProjectCreationRejectionKind::StagingNameEntropy
            }
            ProjectCreationRejectionInner::DependencyRelation(_) => {
                ProjectCreationRejectionKind::DependencyRelation
            }
            ProjectCreationRejectionInner::BeforeStaging(_) => {
                ProjectCreationRejectionKind::BeforeStaging
            }
            ProjectCreationRejectionInner::DirectoryPublication(_) => {
                ProjectCreationRejectionKind::DirectoryPublication
            }
            ProjectCreationRejectionInner::Staging { .. } => ProjectCreationRejectionKind::Staging,
        }
    }

    #[must_use]
    pub const fn before_staging_error(&self) -> Option<&FsError> {
        match &self.inner {
            ProjectCreationRejectionInner::BeforeStaging(error) => Some(error),
            ProjectCreationRejectionInner::StagingNameEntropy(_)
            | ProjectCreationRejectionInner::DependencyRelation(_)
            | ProjectCreationRejectionInner::DirectoryPublication(_)
            | ProjectCreationRejectionInner::Staging { .. } => None,
        }
    }

    #[must_use]
    pub const fn publication_rejection(&self) -> Option<&DirectoryPublicationRejection> {
        match &self.inner {
            ProjectCreationRejectionInner::DirectoryPublication(rejection) => Some(rejection),
            ProjectCreationRejectionInner::Staging { publication, .. } => Some(publication),
            ProjectCreationRejectionInner::StagingNameEntropy(_)
            | ProjectCreationRejectionInner::DependencyRelation(_)
            | ProjectCreationRejectionInner::BeforeStaging(_) => None,
        }
    }

    #[must_use]
    pub const fn staging_error(&self) -> Option<&ProjectTreeStageError> {
        match &self.inner {
            ProjectCreationRejectionInner::Staging { error, .. } => Some(error),
            ProjectCreationRejectionInner::StagingNameEntropy(_)
            | ProjectCreationRejectionInner::DependencyRelation(_)
            | ProjectCreationRejectionInner::BeforeStaging(_)
            | ProjectCreationRejectionInner::DirectoryPublication(_) => None,
        }
    }
}

impl Debug for ProjectCreationRejection {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectCreationRejection")
            .field("plan", &self.plan)
            .field("kind", &self.kind())
            .field("staging_error", &self.staging_error())
            .field("publication", &self.publication_rejection())
            .finish()
    }
}

impl Display for ProjectCreationRejection {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "project creation was rejected: {:?}",
            self.kind()
        )
    }
}

impl Error for ProjectCreationRejection {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.inner {
            ProjectCreationRejectionInner::StagingNameEntropy(error) => Some(error),
            ProjectCreationRejectionInner::DependencyRelation(error) => Some(error),
            ProjectCreationRejectionInner::BeforeStaging(error) => Some(error),
            ProjectCreationRejectionInner::DirectoryPublication(rejection) => {
                rejection.cause().filesystem_error().map(|error| error as _)
            }
            ProjectCreationRejectionInner::Staging { error, .. } => Some(error),
        }
    }
}

pub struct ProjectCreationOrphan {
    plan: ProjectCreationPlanReceipt,
    staging_error: Option<ProjectTreeStageError>,
    publication: DirectoryPublicationOrphan,
}

impl ProjectCreationOrphan {
    #[must_use]
    pub const fn plan(&self) -> &ProjectCreationPlanReceipt {
        &self.plan
    }

    #[must_use]
    pub const fn staging_error(&self) -> Option<&ProjectTreeStageError> {
        self.staging_error.as_ref()
    }

    #[must_use]
    pub const fn publication_orphan(&self) -> &DirectoryPublicationOrphan {
        &self.publication
    }
}

impl Debug for ProjectCreationOrphan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectCreationOrphan")
            .field("plan", &self.plan)
            .field("staging_error", &self.staging_error)
            .field("publication", &self.publication)
            .finish()
    }
}

impl Display for ProjectCreationOrphan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("project creation left a recoverable staging orphan")
    }
}

impl Error for ProjectCreationOrphan {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        self.staging_error
            .as_ref()
            .map(|error| error as &(dyn Error + 'static))
            .or_else(|| Some(self.publication.cleanup_error()))
    }
}

pub struct ProjectCreationUnresolved {
    plan: ProjectCreationPlanReceipt,
    error: DirectoryPublicationBeginError,
}

impl ProjectCreationUnresolved {
    #[must_use]
    pub const fn plan(&self) -> &ProjectCreationPlanReceipt {
        &self.plan
    }

    #[must_use]
    pub const fn error(&self) -> &DirectoryPublicationBeginError {
        &self.error
    }
}

impl Debug for ProjectCreationUnresolved {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectCreationUnresolved")
            .field("plan", &self.plan)
            .field("error", &self.error)
            .finish()
    }
}

impl Display for ProjectCreationUnresolved {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("project creation publication state is unresolved")
    }
}

impl Error for ProjectCreationUnresolved {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProjectCreationAdmissionErrorKind {
    DependencyRelation,
    ReopenPublishedDirectory,
    PublishedIdentityMismatch,
    PublishedDirectory,
    PublishedMember,
    Manifest,
    ProductRecipe,
    Content,
}

pub struct ProjectCreationAdmissionError {
    inner: ProjectCreationAdmissionErrorInner,
}

enum ProjectCreationAdmissionErrorInner {
    DependencyRelation(DependencyRevalidationError),
    ReopenPublishedDirectory(FsError),
    PublishedIdentityMismatch {
        expected: FileIdentity,
        observed: FileIdentity,
    },
    PublishedDirectory {
        directory: super::starter::ProjectDirectory,
        source: FsError,
    },
    PublishedMember {
        member: ProjectMember,
        source: FsError,
    },
    Manifest(Box<ProjectCandidateError>),
    ProductRecipe(Box<RuntimePlanError>),
    Content(Box<ProjectContentError>),
}

impl ProjectCreationAdmissionError {
    #[must_use]
    pub const fn kind(&self) -> ProjectCreationAdmissionErrorKind {
        match &self.inner {
            ProjectCreationAdmissionErrorInner::DependencyRelation(_) => {
                ProjectCreationAdmissionErrorKind::DependencyRelation
            }
            ProjectCreationAdmissionErrorInner::ReopenPublishedDirectory(_) => {
                ProjectCreationAdmissionErrorKind::ReopenPublishedDirectory
            }
            ProjectCreationAdmissionErrorInner::PublishedIdentityMismatch { .. } => {
                ProjectCreationAdmissionErrorKind::PublishedIdentityMismatch
            }
            ProjectCreationAdmissionErrorInner::PublishedDirectory { .. } => {
                ProjectCreationAdmissionErrorKind::PublishedDirectory
            }
            ProjectCreationAdmissionErrorInner::PublishedMember { .. } => {
                ProjectCreationAdmissionErrorKind::PublishedMember
            }
            ProjectCreationAdmissionErrorInner::Manifest(_) => {
                ProjectCreationAdmissionErrorKind::Manifest
            }
            ProjectCreationAdmissionErrorInner::ProductRecipe(_) => {
                ProjectCreationAdmissionErrorKind::ProductRecipe
            }
            ProjectCreationAdmissionErrorInner::Content(_) => {
                ProjectCreationAdmissionErrorKind::Content
            }
        }
    }

    #[must_use]
    pub const fn member(&self) -> Option<ProjectMember> {
        match &self.inner {
            ProjectCreationAdmissionErrorInner::PublishedMember { member, .. } => Some(*member),
            ProjectCreationAdmissionErrorInner::DependencyRelation(_)
            | ProjectCreationAdmissionErrorInner::ReopenPublishedDirectory(_)
            | ProjectCreationAdmissionErrorInner::PublishedIdentityMismatch { .. }
            | ProjectCreationAdmissionErrorInner::PublishedDirectory { .. }
            | ProjectCreationAdmissionErrorInner::Manifest(_)
            | ProjectCreationAdmissionErrorInner::ProductRecipe(_)
            | ProjectCreationAdmissionErrorInner::Content(_) => None,
        }
    }

    #[must_use]
    pub const fn directory(&self) -> Option<super::starter::ProjectDirectory> {
        match &self.inner {
            ProjectCreationAdmissionErrorInner::PublishedDirectory { directory, .. } => {
                Some(*directory)
            }
            ProjectCreationAdmissionErrorInner::DependencyRelation(_)
            | ProjectCreationAdmissionErrorInner::ReopenPublishedDirectory(_)
            | ProjectCreationAdmissionErrorInner::PublishedIdentityMismatch { .. }
            | ProjectCreationAdmissionErrorInner::PublishedMember { .. }
            | ProjectCreationAdmissionErrorInner::Manifest(_)
            | ProjectCreationAdmissionErrorInner::ProductRecipe(_)
            | ProjectCreationAdmissionErrorInner::Content(_) => None,
        }
    }

    #[must_use]
    pub const fn identity_mismatch(&self) -> Option<(FileIdentity, FileIdentity)> {
        match &self.inner {
            ProjectCreationAdmissionErrorInner::PublishedIdentityMismatch {
                expected,
                observed,
            } => Some((*expected, *observed)),
            ProjectCreationAdmissionErrorInner::DependencyRelation(_)
            | ProjectCreationAdmissionErrorInner::ReopenPublishedDirectory(_)
            | ProjectCreationAdmissionErrorInner::PublishedDirectory { .. }
            | ProjectCreationAdmissionErrorInner::PublishedMember { .. }
            | ProjectCreationAdmissionErrorInner::Manifest(_)
            | ProjectCreationAdmissionErrorInner::ProductRecipe(_)
            | ProjectCreationAdmissionErrorInner::Content(_) => None,
        }
    }
}

impl Debug for ProjectCreationAdmissionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectCreationAdmissionError")
            .field("kind", &self.kind())
            .field("directory", &self.directory())
            .field("member", &self.member())
            .field("identity_mismatch", &self.identity_mismatch())
            .finish()
    }
}

impl Display for ProjectCreationAdmissionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "published project was not admitted: {:?}",
            self.kind()
        )
    }
}

impl Error for ProjectCreationAdmissionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.inner {
            ProjectCreationAdmissionErrorInner::DependencyRelation(error) => Some(error),
            ProjectCreationAdmissionErrorInner::ReopenPublishedDirectory(error)
            | ProjectCreationAdmissionErrorInner::PublishedDirectory { source: error, .. }
            | ProjectCreationAdmissionErrorInner::PublishedMember { source: error, .. } => {
                Some(error)
            }
            ProjectCreationAdmissionErrorInner::PublishedIdentityMismatch { .. } => None,
            ProjectCreationAdmissionErrorInner::Manifest(error) => Some(error.as_ref()),
            ProjectCreationAdmissionErrorInner::ProductRecipe(error) => Some(error.as_ref()),
            ProjectCreationAdmissionErrorInner::Content(error) => Some(error.as_ref()),
        }
    }
}

pub struct PublishedButUnadmitted {
    plan: ProjectCreationPlanReceipt,
    publication: DirectoryPublicationReceipt,
    published_root: Option<DirectoryCapability>,
    error: ProjectCreationAdmissionError,
}

impl PublishedButUnadmitted {
    #[must_use]
    pub const fn plan(&self) -> &ProjectCreationPlanReceipt {
        &self.plan
    }

    #[must_use]
    pub const fn publication(&self) -> &DirectoryPublicationReceipt {
        &self.publication
    }

    #[must_use]
    pub fn published_root(&self) -> Option<DirectoryCapability> {
        self.published_root.as_ref().map(DirectoryCapability::share)
    }

    #[must_use]
    pub const fn error(&self) -> &ProjectCreationAdmissionError {
        &self.error
    }
}

impl Debug for PublishedButUnadmitted {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PublishedButUnadmitted")
            .field("plan", &self.plan)
            .field("publication", &self.publication)
            .field(
                "published_root_identity",
                &self
                    .published_root
                    .as_ref()
                    .map(DirectoryCapability::identity),
            )
            .field("error", &self.error)
            .finish()
    }
}

pub(crate) fn publish_prepared(prepared: PreparedProjectCreation) -> ProjectCreationOutcome {
    publish_prepared_with(prepared, admit_published)
}

fn publish_prepared_with(
    prepared: PreparedProjectCreation,
    admit: impl FnOnce(
        &DirectoryCapability,
        DirectoryPublicationReceipt,
        ProjectCreationPlanReceipt,
        PreparedDependencyRevalidation,
    ) -> ProjectCreationOutcome,
) -> ProjectCreationOutcome {
    publish_prepared_with_staging_name(prepared, next_staging_name, admit)
}

fn publish_prepared_with_staging_name(
    prepared: PreparedProjectCreation,
    staging_name: impl FnOnce() -> Result<RelativeComponent, getrandom::Error>,
    admit: impl FnOnce(
        &DirectoryCapability,
        DirectoryPublicationReceipt,
        ProjectCreationPlanReceipt,
        PreparedDependencyRevalidation,
    ) -> ProjectCreationOutcome,
) -> ProjectCreationOutcome {
    let PreparedProjectCreation {
        publication_parent,
        destination,
        plan,
        dependency_revalidation,
    } = prepared;
    let plan_receipt = plan.receipt();
    if let Err(error) = dependency_revalidation.revalidate(&publication_parent) {
        return ProjectCreationOutcome::Rejected(ProjectCreationRejection {
            plan: plan_receipt,
            inner: ProjectCreationRejectionInner::DependencyRelation(error),
        });
    }
    let staging_name = match staging_name() {
        Ok(staging_name) => staging_name,
        Err(error) => {
            return ProjectCreationOutcome::Rejected(ProjectCreationRejection {
                plan: plan_receipt,
                inner: ProjectCreationRejectionInner::StagingNameEntropy(error),
            });
        }
    };
    let transaction =
        match publication_parent.begin_directory_publication(&destination, &staging_name) {
            Ok(transaction) => transaction,
            Err(DirectoryPublicationBeginError::BeforeStaging(error)) => {
                return ProjectCreationOutcome::Rejected(ProjectCreationRejection {
                    plan: plan_receipt,
                    inner: ProjectCreationRejectionInner::BeforeStaging(error),
                });
            }
            Err(DirectoryPublicationBeginError::RejectedClean(rejection)) => {
                return ProjectCreationOutcome::Rejected(ProjectCreationRejection {
                    plan: plan_receipt,
                    inner: ProjectCreationRejectionInner::DirectoryPublication(*rejection),
                });
            }
            Err(DirectoryPublicationBeginError::Orphaned(orphan)) => {
                return ProjectCreationOutcome::Orphaned(ProjectCreationOrphan {
                    plan: plan_receipt,
                    staging_error: None,
                    publication: *orphan,
                });
            }
            #[allow(unreachable_patterns)]
            Err(error) => {
                return ProjectCreationOutcome::Unresolved(ProjectCreationUnresolved {
                    plan: plan_receipt,
                    error,
                });
            }
        };

    if let Err(error) = stage_project_tree(transaction.staging_directory(), &plan) {
        return match transaction.abort() {
            DirectoryPublicationOutcome::RejectedClean(publication) => {
                ProjectCreationOutcome::Rejected(ProjectCreationRejection {
                    plan: plan_receipt,
                    inner: ProjectCreationRejectionInner::Staging { error, publication },
                })
            }
            DirectoryPublicationOutcome::Orphaned(publication) => {
                ProjectCreationOutcome::Orphaned(ProjectCreationOrphan {
                    plan: plan_receipt,
                    staging_error: Some(error),
                    publication,
                })
            }
            DirectoryPublicationOutcome::Published(_) => {
                unreachable!("aborting a directory publication cannot publish it")
            }
        };
    }

    match transaction.publish() {
        DirectoryPublicationOutcome::Published(publication) => admit(
            &publication_parent,
            publication,
            plan_receipt,
            dependency_revalidation,
        ),
        DirectoryPublicationOutcome::RejectedClean(publication) => {
            ProjectCreationOutcome::Rejected(ProjectCreationRejection {
                plan: plan_receipt,
                inner: ProjectCreationRejectionInner::DirectoryPublication(publication),
            })
        }
        DirectoryPublicationOutcome::Orphaned(publication) => {
            ProjectCreationOutcome::Orphaned(ProjectCreationOrphan {
                plan: plan_receipt,
                staging_error: None,
                publication,
            })
        }
    }
}

#[cfg(all(
    test,
    any(
        all(target_os = "windows", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
mod tests {
    use std::{
        fs::{self, File, OpenOptions},
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use nara_fs::{
        CapabilityRights, FsError, FsOperation, HostCapabilityOptions, RelativePath, TrustMode,
    };

    use super::*;
    use crate::project_workflow::{
        LocalDependencyParent, NaraDependencySource, ProjectCreationRequest, ProjectSlug,
        prepare_desktop_2d_project,
    };

    const GIT_REVISION: &str = "0123456789abcdef0123456789abcdef01234567";
    static NEXT_TEST_ROOT: AtomicU64 = AtomicU64::new(1);

    fn publication_is_unavailable_and_clean(
        outcome: &ProjectCreationOutcome,
        parent: &Path,
        destination: &str,
    ) -> bool {
        let ProjectCreationOutcome::Rejected(rejection) = outcome else {
            return false;
        };
        if !matches!(
            rejection.before_staging_error(),
            Some(FsError::Unsupported { .. }) | Some(FsError::Unproven { .. })
        ) {
            return false;
        }
        assert!(!parent.join(destination).exists());
        assert!(fs::read_dir(parent).unwrap().all(|entry| {
            !entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .starts_with(".nara-stage-")
        }));
        true
    }

    #[test]
    fn entropy_failure_rejects_before_allocating_a_staging_name() {
        let root = TestRoot::new();
        let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
            root.capability(),
            ProjectSlug::new("entropy-failure").unwrap(),
            NaraDependencySource::immutable_git(
                "https://github.com/nara-engine/nara.git",
                GIT_REVISION,
            )
            .unwrap(),
        ))
        .unwrap();

        let outcome = publish_prepared_with_staging_name(
            prepared,
            || Err(getrandom::Error::UNSUPPORTED),
            admit_published,
        );
        let ProjectCreationOutcome::Rejected(rejection) = outcome else {
            panic!("expected a pre-staging rejection: {outcome:?}");
        };

        assert_eq!(
            rejection.kind(),
            ProjectCreationRejectionKind::StagingNameEntropy
        );
        assert!(!root.path.join("entropy-failure").exists());
        assert!(fs::read_dir(&root.path).unwrap().next().is_none());
    }

    #[test]
    fn staging_names_use_independent_128_bit_random_suffixes() {
        let first = next_staging_name().unwrap();
        let second = next_staging_name().unwrap();

        assert_ne!(first, second);
    }

    #[test]
    fn admission_failure_retains_the_published_directory_and_publication_receipt() {
        let root = TestRoot::new();
        let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
            root.capability(),
            ProjectSlug::new("unadmitted-game").unwrap(),
            NaraDependencySource::immutable_git(
                "https://github.com/nara-engine/nara.git",
                GIT_REVISION,
            )
            .unwrap(),
        ))
        .unwrap();

        let outcome = publish_prepared_with(prepared, |parent, publication, plan, _dependency| {
            let project_root = parent
                .open_child_directory(publication.destination())
                .unwrap();
            assert_eq!(project_root.identity(), publication.published_identity());
            ProjectCreationOutcome::PublishedButUnadmitted(PublishedButUnadmitted {
                plan,
                publication,
                published_root: Some(project_root),
                error: ProjectCreationAdmissionError {
                    inner: ProjectCreationAdmissionErrorInner::PublishedMember {
                        member: ProjectMember::ProjectManifest,
                        source: FsError::Unsupported {
                            operation: FsOperation::OpenFile,
                            capability: "forced admission failure",
                        },
                    },
                },
            })
        });
        if publication_is_unavailable_and_clean(&outcome, &root.path, "unadmitted-game") {
            return;
        }
        let ProjectCreationOutcome::PublishedButUnadmitted(outcome) = outcome else {
            panic!("expected a published-but-unadmitted outcome: {outcome:?}");
        };

        assert!(root.path.join("unadmitted-game").is_dir());
        assert_eq!(
            outcome
                .published_root()
                .expect("the published root was reopened")
                .identity(),
            outcome.publication().published_identity()
        );
        assert_eq!(
            outcome.error().kind(),
            ProjectCreationAdmissionErrorKind::PublishedMember
        );
    }

    #[test]
    fn post_publication_member_change_prevents_creation_success() {
        let root = TestRoot::new();
        let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
            root.capability(),
            ProjectSlug::new("changed-after-publication").unwrap(),
            NaraDependencySource::immutable_git(
                "https://github.com/nara-engine/nara.git",
                GIT_REVISION,
            )
            .unwrap(),
        ))
        .unwrap();
        let published_readme = root.path.join("changed-after-publication/README.md");

        let outcome = publish_prepared_with(prepared, |parent, publication, plan, dependency| {
            fs::write(&published_readme, b"changed after publication").unwrap();
            admit_published(parent, publication, plan, dependency)
        });
        if publication_is_unavailable_and_clean(&outcome, &root.path, "changed-after-publication") {
            return;
        }
        let ProjectCreationOutcome::PublishedButUnadmitted(outcome) = outcome else {
            panic!("expected a published-but-unadmitted outcome: {outcome:?}");
        };

        assert!(root.path.join("changed-after-publication").is_dir());
        assert_eq!(
            outcome.error().kind(),
            ProjectCreationAdmissionErrorKind::PublishedMember
        );
        assert_eq!(outcome.error().member(), Some(ProjectMember::AuthorGuide));
    }

    #[test]
    fn manifest_parsing_rejects_bytes_that_do_not_match_the_plan_digest() {
        let root = TestRoot::new();
        let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
            root.capability(),
            ProjectSlug::new("changed-manifest-after-publication").unwrap(),
            NaraDependencySource::immutable_git(
                "https://github.com/nara-engine/nara.git",
                GIT_REVISION,
            )
            .unwrap(),
        ))
        .unwrap();
        let manifest = root
            .path
            .join("changed-manifest-after-publication/nara.toml");

        let outcome = publish_prepared_with(prepared, |parent, publication, plan, dependency| {
            let mut bytes = fs::read_to_string(&manifest).unwrap();
            bytes = bytes.replace(
                "name = \"changed-manifest-after-publication\"",
                "name = \"mutated-but-valid\"",
            );
            fs::write(&manifest, bytes).unwrap();
            admit_published(parent, publication, plan, dependency)
        });
        if publication_is_unavailable_and_clean(
            &outcome,
            &root.path,
            "changed-manifest-after-publication",
        ) {
            return;
        }
        let ProjectCreationOutcome::PublishedButUnadmitted(outcome) = outcome else {
            panic!("expected a published-but-unadmitted outcome: {outcome:?}");
        };

        assert_eq!(
            outcome.error().kind(),
            ProjectCreationAdmissionErrorKind::Manifest
        );
        assert_eq!(
            outcome
                .error()
                .source()
                .and_then(|error| error.downcast_ref::<ProjectCandidateError>())
                .map(ProjectCandidateError::kind),
            Some(crate::project_host::ProjectCandidateErrorKind::HostAuthorityRejected)
        );
    }

    #[test]
    fn startup_scene_parsing_rejects_bytes_that_do_not_match_the_plan_digest() {
        let root = TestRoot::new();
        let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
            root.capability(),
            ProjectSlug::new("changed-scene-after-publication").unwrap(),
            NaraDependencySource::immutable_git(
                "https://github.com/nara-engine/nara.git",
                GIT_REVISION,
            )
            .unwrap(),
        ))
        .unwrap();
        let scene = root
            .path
            .join("changed-scene-after-publication/scenes/startup.scene.json");

        let outcome = publish_prepared_with(prepared, |parent, publication, plan, dependency| {
            let bytes = fs::read_to_string(&scene).unwrap();
            fs::write(&scene, format!("{bytes}\n")).unwrap();
            admit_published(parent, publication, plan, dependency)
        });
        if publication_is_unavailable_and_clean(
            &outcome,
            &root.path,
            "changed-scene-after-publication",
        ) {
            return;
        }
        let ProjectCreationOutcome::PublishedButUnadmitted(outcome) = outcome else {
            panic!("expected a published-but-unadmitted outcome: {outcome:?}");
        };

        assert_eq!(
            outcome.error().kind(),
            ProjectCreationAdmissionErrorKind::Content
        );
        assert_eq!(
            outcome
                .error()
                .source()
                .and_then(|error| error.downcast_ref::<ProjectContentError>())
                .map(ProjectContentError::kind),
            Some(crate::project_content::ProjectContentErrorKind::HostAuthorityRejected)
        );
    }

    #[test]
    fn local_dependency_relation_is_revalidated_after_publication() {
        let checkout = TestRoot::new();
        fs::create_dir(checkout.path.join("projects")).unwrap();
        let checkout_capability = checkout.capability();
        let publication_parent = checkout_capability
            .open_directory(&RelativePath::new("projects").unwrap())
            .unwrap();
        let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
            publication_parent,
            ProjectSlug::new("moved-after-publication").unwrap(),
            NaraDependencySource::local(
                checkout_capability,
                LocalDependencyParent::Descendant(RelativePath::new("projects").unwrap()),
            ),
        ))
        .unwrap();
        let outside = TestRoot::new();
        let moved_parent = outside.path.join("moved-projects");

        let outcome = publish_prepared_with(prepared, |parent, publication, plan, dependency| {
            fs::rename(checkout.path.join("projects"), &moved_parent).unwrap();
            admit_published(parent, publication, plan, dependency)
        });
        if publication_is_unavailable_and_clean(
            &outcome,
            &checkout.path.join("projects"),
            "moved-after-publication",
        ) {
            return;
        }
        let ProjectCreationOutcome::PublishedButUnadmitted(outcome) = outcome else {
            panic!("expected dependency drift after publication: {outcome:?}");
        };

        assert_eq!(
            outcome.error().kind(),
            ProjectCreationAdmissionErrorKind::DependencyRelation
        );
        assert!(moved_parent.join("moved-after-publication").is_dir());
    }

    #[test]
    fn post_publication_directory_removal_prevents_creation_success() {
        let root = TestRoot::new();
        let prepared = prepare_desktop_2d_project(ProjectCreationRequest::new(
            root.capability(),
            ProjectSlug::new("missing-directory-after-publication").unwrap(),
            NaraDependencySource::immutable_git(
                "https://github.com/nara-engine/nara.git",
                GIT_REVISION,
            )
            .unwrap(),
        ))
        .unwrap();
        let published_assets = root.path.join("missing-directory-after-publication/assets");

        let outcome = publish_prepared_with(prepared, |parent, publication, plan, dependency| {
            fs::remove_dir(&published_assets).unwrap();
            admit_published(parent, publication, plan, dependency)
        });
        if publication_is_unavailable_and_clean(
            &outcome,
            &root.path,
            "missing-directory-after-publication",
        ) {
            return;
        }
        let ProjectCreationOutcome::PublishedButUnadmitted(outcome) = outcome else {
            panic!("expected a published-but-unadmitted outcome: {outcome:?}");
        };

        assert_eq!(
            outcome.error().kind(),
            ProjectCreationAdmissionErrorKind::PublishedDirectory
        );
        assert_eq!(
            outcome.error().directory(),
            Some(super::super::starter::ProjectDirectory::Assets)
        );
    }

    struct TestRoot {
        path: PathBuf,
    }

    impl TestRoot {
        fn new() -> Self {
            let sequence = NEXT_TEST_ROOT.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "nara-project-publication-unit-{}-{sequence}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn capability(&self) -> DirectoryCapability {
            DirectoryCapability::from_host_handle(
                open_directory(&self.path),
                HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
            )
            .unwrap()
        }
    }

    impl Drop for TestRoot {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[cfg(windows)]
    fn open_directory(path: &Path) -> File {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0x1 | 0x2 | 0x4)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .unwrap()
    }

    #[cfg(unix)]
    fn open_directory(path: &Path) -> File {
        File::open(path).unwrap()
    }
}
