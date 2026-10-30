use std::{
    collections::BTreeSet,
    error::Error,
    fmt::{self, Debug, Display, Formatter},
};

use nara_fs::{
    ContentDigest, DirectoryCapability, DirectoryPublicationBeginError, DirectoryPublicationOrphan,
    DirectoryPublicationOutcome, DirectoryPublicationReceipt, DirectoryPublicationRejection,
    FsError, RelativeComponent, RelativePath,
};
use nara_project::ProjectPath;

use crate::project_host::{ProjectContentDeliveryReceipt, ProjectContentMemberKind};

mod build;
mod stage;

#[cfg(all(test, feature = "project-tooling"))]
mod source_tests;

pub use build::{
    ProjectExportBinaryTarget, ProjectExportBuildError, ProjectExportBuildReceipt,
    ProjectExportExecutableReceipt,
};

#[doc(hidden)]
#[cfg(feature = "project-tooling")]
pub use build::{
    ProjectExportBuildPreparation, ProjectExportCargoArtifact, ProjectExportProductAgreement,
};

use stage::{ProjectExportStageError, stage_export_tree};

const MAX_EXPORT_MEMBERS: usize = 4096;
const MAX_EXPORT_BYTES: u64 = 2 * 1024 * 1024 * 1024;

/// The source role of one member in the declared-content export layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectExportMemberKind {
    HeadlessExecutable,
    DesktopExecutable,
    Content(ProjectContentMemberKind),
}

/// One immutable member fact in a prepared declared-content export.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectExportMemberReceipt {
    kind: ProjectExportMemberKind,
    path: ProjectPath,
    digest: ContentDigest,
}

impl ProjectExportMemberReceipt {
    #[must_use]
    pub const fn kind(&self) -> ProjectExportMemberKind {
        self.kind
    }

    #[must_use]
    pub const fn path(&self) -> &ProjectPath {
        &self.path
    }

    #[must_use]
    pub const fn digest(&self) -> ContentDigest {
        self.digest
    }
}

/// Immutable plan facts retained by both the prepared attempt and final publication receipt.
pub struct ProjectExportPlanReceipt {
    destination: RelativeComponent,
    startup_content_digest: ContentDigest,
    startup_closure_plan_digest: ContentDigest,
    delivery_digest: ContentDigest,
    build_input_digest: ContentDigest,
    member_plan_digest: ContentDigest,
    runtime_directories: Box<[ProjectPath]>,
    members: Box<[ProjectExportMemberReceipt]>,
    sources: Box<[ProjectExportMemberSource]>,
}

impl ProjectExportPlanReceipt {
    #[must_use]
    pub const fn delivery_digest(&self) -> ContentDigest {
        self.delivery_digest
    }

    #[must_use]
    pub const fn destination(&self) -> &RelativeComponent {
        &self.destination
    }

    #[must_use]
    pub const fn startup_content_digest(&self) -> ContentDigest {
        self.startup_content_digest
    }

    #[must_use]
    pub const fn startup_closure_plan_digest(&self) -> ContentDigest {
        self.startup_closure_plan_digest
    }

    #[must_use]
    pub const fn build_input_digest(&self) -> ContentDigest {
        self.build_input_digest
    }

    #[must_use]
    pub const fn member_plan_digest(&self) -> ContentDigest {
        self.member_plan_digest
    }

    #[must_use]
    pub fn members(&self) -> &[ProjectExportMemberReceipt] {
        &self.members
    }

    pub(crate) fn runtime_directories(&self) -> &[ProjectPath] {
        &self.runtime_directories
    }
}

impl Debug for ProjectExportPlanReceipt {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectExportPlanReceipt")
            .field("destination", &self.destination)
            .field("startup_content_digest", &self.startup_content_digest)
            .field("delivery_digest", &self.delivery_digest)
            .field(
                "startup_closure_plan_digest",
                &self.startup_closure_plan_digest,
            )
            .field("build_input_digest", &self.build_input_digest)
            .field("member_plan_digest", &self.member_plan_digest)
            .field("runtime_directory_count", &self.runtime_directories.len())
            .field("member_count", &self.members.len())
            .finish()
    }
}

#[derive(Debug)]
enum ProjectExportMemberSource {
    Executable(ProjectExportBinaryTarget),
    Content(ProjectPath),
}

/// Inputs for one exact declared-content export publication.
pub struct ProjectExportRequest {
    publication_parent: DirectoryCapability,
    destination: RelativeComponent,
    delivery_content: ProjectContentDeliveryReceipt,
    build: ProjectExportBuildReceipt,
}

impl ProjectExportRequest {
    #[must_use]
    pub fn new(
        publication_parent: DirectoryCapability,
        destination: RelativeComponent,
        delivery_content: ProjectContentDeliveryReceipt,
        build: ProjectExportBuildReceipt,
    ) -> Self {
        Self {
            publication_parent,
            destination,
            delivery_content,
            build,
        }
    }
}

impl Debug for ProjectExportRequest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectExportRequest")
            .field(
                "publication_parent_identity",
                &self.publication_parent.identity(),
            )
            .field("destination", &self.destination)
            .field("delivery_content", &self.delivery_content)
            .field("build", &self.build)
            .finish()
    }
}

/// Move-only export candidate whose source and build facts were validated before staging.
pub struct PreparedProjectExport {
    publication_parent: DirectoryCapability,
    plan: ProjectExportPlanReceipt,
    delivery_content: ProjectContentDeliveryReceipt,
    build: ProjectExportBuildReceipt,
}

impl PreparedProjectExport {
    #[must_use]
    pub const fn plan(&self) -> &ProjectExportPlanReceipt {
        &self.plan
    }

    #[must_use = "the project-export terminal outcome must be handled"]
    pub fn publish(self) -> ProjectExportOutcome {
        publish_prepared(self)
    }
}

impl Debug for PreparedProjectExport {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PreparedProjectExport")
            .field(
                "publication_parent_identity",
                &self.publication_parent.identity(),
            )
            .field("plan", &self.plan)
            .field("delivery_content", &self.delivery_content)
            .field("build", &self.build)
            .finish()
    }
}

/// Validates the exact source/build closure before any staging directory exists.
pub fn prepare_project_export(
    request: ProjectExportRequest,
) -> Result<PreparedProjectExport, ProjectExportPlanError> {
    let ProjectExportRequest {
        publication_parent,
        destination,
        delivery_content,
        build,
    } = request;
    if !delivery_content
        .startup_content()
        .has_runtime_schema_agreement()
    {
        return Err(ProjectExportPlanError::RuntimeSchemaAgreementMissing);
    }
    revalidate_sources(&delivery_content, &build)?;
    let plan = plan_export(destination, &delivery_content, &build)?;
    Ok(PreparedProjectExport {
        publication_parent,
        plan,
        delivery_content,
        build,
    })
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectExportPlanError {
    #[error("startup-content closure lacks a checked runtime schema agreement")]
    RuntimeSchemaAgreementMissing,
    #[error(transparent)]
    Build(#[from] ProjectExportBuildError),
    #[error("declared-content member {path} changed after project check")]
    ContentChanged {
        path: ProjectPath,
        #[source]
        source: FsError,
    },
    #[error("declared-content runtime directory {path} changed after project check")]
    ContentDirectoryChanged {
        path: ProjectPath,
        #[source]
        source: FsError,
    },
    #[error("declared-content export contains too many members")]
    MemberLimitExceeded,
    #[error("declared-content export exceeds its total byte limit")]
    ByteLimitExceeded,
    #[error("declared-content export target path collides: {path}")]
    TargetPathCollision { path: ProjectPath },
}

#[must_use = "the project-export terminal outcome must be handled"]
#[derive(Debug)]
pub enum ProjectExportOutcome {
    Published(ProjectExportReceipt),
    Rejected(ProjectExportRejection),
    Orphaned(ProjectExportOrphan),
    Unresolved(ProjectExportUnresolved),
}

pub struct ProjectExportReceipt {
    plan: ProjectExportPlanReceipt,
    publication: DirectoryPublicationReceipt,
}

impl ProjectExportReceipt {
    #[must_use]
    pub const fn plan(&self) -> &ProjectExportPlanReceipt {
        &self.plan
    }

    #[must_use]
    pub const fn publication(&self) -> &DirectoryPublicationReceipt {
        &self.publication
    }
}

impl Debug for ProjectExportReceipt {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectExportReceipt")
            .field("plan", &self.plan)
            .field("publication", &self.publication)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectExportRejectionKind {
    StagingNameEntropy,
    SourceChanged,
    BeforeStaging,
    DirectoryPublication,
    Staging,
}

pub struct ProjectExportRejection {
    plan: ProjectExportPlanReceipt,
    inner: ProjectExportRejectionInner,
}

enum ProjectExportRejectionInner {
    StagingNameEntropy(getrandom::Error),
    SourceChanged(ProjectExportPlanError),
    SourceChangedAfterStaging {
        error: ProjectExportPlanError,
        publication: DirectoryPublicationRejection,
    },
    BeforeStaging(FsError),
    DirectoryPublication(DirectoryPublicationRejection),
    Staging {
        error: ProjectExportStageError,
        publication: DirectoryPublicationRejection,
    },
}

impl ProjectExportRejection {
    #[must_use]
    pub const fn plan(&self) -> &ProjectExportPlanReceipt {
        &self.plan
    }

    #[must_use]
    pub const fn kind(&self) -> ProjectExportRejectionKind {
        match &self.inner {
            ProjectExportRejectionInner::StagingNameEntropy(_) => {
                ProjectExportRejectionKind::StagingNameEntropy
            }
            ProjectExportRejectionInner::SourceChanged(_) => {
                ProjectExportRejectionKind::SourceChanged
            }
            ProjectExportRejectionInner::SourceChangedAfterStaging { .. } => {
                ProjectExportRejectionKind::SourceChanged
            }
            ProjectExportRejectionInner::BeforeStaging(_) => {
                ProjectExportRejectionKind::BeforeStaging
            }
            ProjectExportRejectionInner::DirectoryPublication(_) => {
                ProjectExportRejectionKind::DirectoryPublication
            }
            ProjectExportRejectionInner::Staging { .. } => ProjectExportRejectionKind::Staging,
        }
    }

    #[must_use]
    pub const fn source_change(&self) -> Option<&ProjectExportPlanError> {
        match &self.inner {
            ProjectExportRejectionInner::SourceChanged(error)
            | ProjectExportRejectionInner::SourceChangedAfterStaging { error, .. } => Some(error),
            _ => None,
        }
    }

    #[must_use]
    pub const fn publication_rejection(&self) -> Option<&DirectoryPublicationRejection> {
        match &self.inner {
            ProjectExportRejectionInner::DirectoryPublication(rejection)
            | ProjectExportRejectionInner::SourceChangedAfterStaging {
                publication: rejection,
                ..
            }
            | ProjectExportRejectionInner::Staging {
                publication: rejection,
                ..
            } => Some(rejection),
            _ => None,
        }
    }
}

impl Debug for ProjectExportRejection {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectExportRejection")
            .field("plan", &self.plan)
            .field("kind", &self.kind())
            .field("source", &self.source())
            .finish()
    }
}

impl Display for ProjectExportRejection {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        write!(formatter, "project export was rejected: {:?}", self.kind())
    }
}

impl Error for ProjectExportRejection {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.inner {
            ProjectExportRejectionInner::StagingNameEntropy(error) => Some(error),
            ProjectExportRejectionInner::SourceChanged(error) => Some(error),
            ProjectExportRejectionInner::SourceChangedAfterStaging { error, .. } => Some(error),
            ProjectExportRejectionInner::BeforeStaging(error) => Some(error),
            ProjectExportRejectionInner::DirectoryPublication(rejection) => {
                rejection.cause().filesystem_error().map(|error| error as _)
            }
            ProjectExportRejectionInner::Staging { error, .. } => Some(error),
        }
    }
}

pub struct ProjectExportOrphan {
    plan: ProjectExportPlanReceipt,
    failure: Option<ProjectExportFailure>,
    publication: DirectoryPublicationOrphan,
}

enum ProjectExportFailure {
    SourceChanged(ProjectExportPlanError),
    Staging(ProjectExportStageError),
}

impl ProjectExportOrphan {
    #[must_use]
    pub const fn plan(&self) -> &ProjectExportPlanReceipt {
        &self.plan
    }

    #[must_use]
    pub const fn publication_orphan(&self) -> &DirectoryPublicationOrphan {
        &self.publication
    }
}

impl Debug for ProjectExportOrphan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectExportOrphan")
            .field("plan", &self.plan)
            .field("source", &self.source())
            .field("publication", &self.publication)
            .finish()
    }
}

impl Display for ProjectExportOrphan {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("project export left an unresolved staging directory")
    }
}

impl Error for ProjectExportOrphan {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match &self.failure {
            Some(ProjectExportFailure::SourceChanged(error)) => Some(error),
            Some(ProjectExportFailure::Staging(error)) => Some(error),
            None => Some(self.publication.cleanup_error()),
        }
    }
}

pub struct ProjectExportUnresolved {
    plan: ProjectExportPlanReceipt,
    error: DirectoryPublicationBeginError,
}

impl ProjectExportUnresolved {
    #[must_use]
    pub const fn plan(&self) -> &ProjectExportPlanReceipt {
        &self.plan
    }

    #[must_use]
    pub const fn error(&self) -> &DirectoryPublicationBeginError {
        &self.error
    }
}

impl Debug for ProjectExportUnresolved {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectExportUnresolved")
            .field("plan", &self.plan)
            .field("error", &self.error)
            .finish()
    }
}

impl Display for ProjectExportUnresolved {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("project export publication state is unresolved")
    }
}

impl Error for ProjectExportUnresolved {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        Some(&self.error)
    }
}

fn plan_export(
    destination: RelativeComponent,
    delivery_content: &ProjectContentDeliveryReceipt,
    build: &ProjectExportBuildReceipt,
) -> Result<ProjectExportPlanReceipt, ProjectExportPlanError> {
    let runtime_directories = delivery_content
        .runtime_directories()
        .iter()
        .map(|path| {
            ProjectPath::new(format!("project/{}", path.as_str()))
                .expect("an admitted runtime directory remains valid under the project prefix")
        })
        .collect::<Box<[_]>>();
    let mut members = Vec::with_capacity(delivery_content.members().len().saturating_add(2));
    let mut sources = Vec::with_capacity(members.capacity());
    let executable_suffix = if build.target_triple().contains("windows") {
        ".exe"
    } else {
        ""
    };
    for executable in build.executables() {
        let target = executable.target();
        let kind = match target {
            ProjectExportBinaryTarget::Headless => ProjectExportMemberKind::HeadlessExecutable,
            ProjectExportBinaryTarget::Desktop => ProjectExportMemberKind::DesktopExecutable,
        };
        members.push(ProjectExportMemberReceipt {
            kind,
            path: ProjectPath::new(format!(
                "bin/{}{executable_suffix}",
                target.cargo_target_name()
            ))
            .expect("the closed executable layout is valid"),
            digest: executable.digest(),
        });
        sources.push(ProjectExportMemberSource::Executable(target));
    }
    for member in delivery_content.members() {
        members.push(ProjectExportMemberReceipt {
            kind: ProjectExportMemberKind::Content(member.kind()),
            path: ProjectPath::new(format!("project/{}", member.path().as_str()))
                .expect("an admitted project path remains valid under the project prefix"),
            digest: member.digest(),
        });
        sources.push(ProjectExportMemberSource::Content(member.path().clone()));
    }
    if members.len() > MAX_EXPORT_MEMBERS {
        return Err(ProjectExportPlanError::MemberLimitExceeded);
    }
    let total_bytes = members.iter().try_fold(0_u64, |total, member| {
        total.checked_add(member.digest.length())
    });
    if total_bytes.is_none_or(|total| total > MAX_EXPORT_BYTES) {
        return Err(ProjectExportPlanError::ByteLimitExceeded);
    }
    let mut paths = BTreeSet::new();
    for directory in &runtime_directories {
        paths.insert(directory.as_str());
    }
    for member in &members {
        if !paths.insert(member.path.as_str()) {
            return Err(ProjectExportPlanError::TargetPathCollision {
                path: member.path.clone(),
            });
        }
    }
    let member_plan_digest = digest_member_plan(&runtime_directories, &members);
    Ok(ProjectExportPlanReceipt {
        destination,
        startup_content_digest: delivery_content.startup_content().content_digest(),
        startup_closure_plan_digest: delivery_content.startup_content().closure_plan_digest(),
        delivery_digest: delivery_content.delivery_digest(),
        build_input_digest: build.build_input_digest(),
        member_plan_digest,
        runtime_directories,
        members: members.into_boxed_slice(),
        sources: sources.into_boxed_slice(),
    })
}

fn digest_member_plan(
    runtime_directories: &[ProjectPath],
    members: &[ProjectExportMemberReceipt],
) -> ContentDigest {
    let mut framed = Vec::new();
    framed.extend_from_slice(b"nara.project-declared-content-export.v3\0");
    for directory in runtime_directories {
        append_frame(&mut framed, b"runtime-directory");
        append_frame(&mut framed, directory.as_str().as_bytes());
    }
    for member in members {
        append_frame(&mut framed, member_kind_name(member.kind).as_bytes());
        append_frame(&mut framed, member.path.as_str().as_bytes());
        append_frame(&mut framed, &member.digest.length().to_le_bytes());
        append_frame(&mut framed, member.digest.as_bytes());
    }
    ContentDigest::of_bytes(&framed)
}

fn member_kind_name(kind: ProjectExportMemberKind) -> &'static str {
    match kind {
        ProjectExportMemberKind::HeadlessExecutable => "headless-executable",
        ProjectExportMemberKind::DesktopExecutable => "desktop-executable",
        ProjectExportMemberKind::Content(ProjectContentMemberKind::ProjectManifest) => {
            "project-manifest"
        }
        ProjectExportMemberKind::Content(ProjectContentMemberKind::AuthoringSchemaBundle) => {
            "authoring-schema-bundle"
        }
        ProjectExportMemberKind::Content(ProjectContentMemberKind::Scene) => "scene",
        ProjectExportMemberKind::Content(ProjectContentMemberKind::Prefab) => "prefab",
        ProjectExportMemberKind::Content(ProjectContentMemberKind::InputActionMap) => {
            "input-action-map"
        }
        ProjectExportMemberKind::Content(ProjectContentMemberKind::AssetMetadata) => "asset-meta",
        ProjectExportMemberKind::Content(ProjectContentMemberKind::AssetSource) => "asset",
    }
}

fn append_frame(target: &mut Vec<u8>, bytes: &[u8]) {
    target.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    target.extend_from_slice(bytes);
}

fn revalidate_sources(
    delivery_content: &ProjectContentDeliveryReceipt,
    build: &ProjectExportBuildReceipt,
) -> Result<(), ProjectExportPlanError> {
    build.revalidate(delivery_content.startup_content())?;
    revalidate_delivery_sources(delivery_content)
}

fn revalidate_delivery_sources(
    delivery_content: &ProjectContentDeliveryReceipt,
) -> Result<(), ProjectExportPlanError> {
    let project_root = delivery_content.project_root();
    for directory in delivery_content.runtime_directories() {
        let path = RelativePath::new(directory.as_str())
            .expect("the project-content owner publishes valid runtime directory paths");
        project_root.open_directory(&path).map_err(|source| {
            ProjectExportPlanError::ContentDirectoryChanged {
                path: directory.clone(),
                source,
            }
        })?;
    }
    for member in delivery_content.members() {
        let path = RelativePath::new(member.path().as_str())
            .expect("the project-content owner publishes valid project paths");
        let file = project_root.open_file(&path).map_err(|source| {
            ProjectExportPlanError::ContentChanged {
                path: member.path().clone(),
                source,
            }
        })?;
        file.verify_digest(member.digest(), member.byte_len())
            .map_err(|source| ProjectExportPlanError::ContentChanged {
                path: member.path().clone(),
                source,
            })?;
    }
    Ok(())
}

fn publish_prepared(prepared: PreparedProjectExport) -> ProjectExportOutcome {
    let PreparedProjectExport {
        publication_parent,
        plan,
        delivery_content,
        build,
    } = prepared;
    if let Err(error) = revalidate_sources(&delivery_content, &build) {
        return ProjectExportOutcome::Rejected(ProjectExportRejection {
            plan,
            inner: ProjectExportRejectionInner::SourceChanged(error),
        });
    }
    let staging_name = match next_staging_name() {
        Ok(name) => name,
        Err(error) => {
            return ProjectExportOutcome::Rejected(ProjectExportRejection {
                plan,
                inner: ProjectExportRejectionInner::StagingNameEntropy(error),
            });
        }
    };
    let transaction =
        match publication_parent.begin_directory_publication(plan.destination(), &staging_name) {
            Ok(transaction) => transaction,
            Err(DirectoryPublicationBeginError::BeforeStaging(error)) => {
                return ProjectExportOutcome::Rejected(ProjectExportRejection {
                    plan,
                    inner: ProjectExportRejectionInner::BeforeStaging(error),
                });
            }
            Err(DirectoryPublicationBeginError::RejectedClean(rejection)) => {
                return ProjectExportOutcome::Rejected(ProjectExportRejection {
                    plan,
                    inner: ProjectExportRejectionInner::DirectoryPublication(*rejection),
                });
            }
            Err(DirectoryPublicationBeginError::Orphaned(orphan)) => {
                return ProjectExportOutcome::Orphaned(ProjectExportOrphan {
                    plan,
                    failure: None,
                    publication: *orphan,
                });
            }
            #[allow(unreachable_patterns)]
            Err(error) => {
                return ProjectExportOutcome::Unresolved(ProjectExportUnresolved { plan, error });
            }
        };
    if let Err(error) = stage_export_tree(
        transaction.staging_directory(),
        &plan,
        &delivery_content,
        &build,
    ) {
        return finish_aborted_export(
            plan,
            ProjectExportFailure::Staging(error),
            transaction.abort(),
        );
    }
    if let Err(error) = revalidate_sources(&delivery_content, &build) {
        return finish_aborted_export(
            plan,
            ProjectExportFailure::SourceChanged(error),
            transaction.abort(),
        );
    }
    match transaction.publish() {
        DirectoryPublicationOutcome::Published(publication) => {
            ProjectExportOutcome::Published(ProjectExportReceipt { plan, publication })
        }
        DirectoryPublicationOutcome::RejectedClean(publication) => {
            ProjectExportOutcome::Rejected(ProjectExportRejection {
                plan,
                inner: ProjectExportRejectionInner::DirectoryPublication(publication),
            })
        }
        DirectoryPublicationOutcome::Orphaned(publication) => {
            ProjectExportOutcome::Orphaned(ProjectExportOrphan {
                plan,
                failure: None,
                publication,
            })
        }
    }
}

fn finish_aborted_export(
    plan: ProjectExportPlanReceipt,
    failure: ProjectExportFailure,
    outcome: DirectoryPublicationOutcome,
) -> ProjectExportOutcome {
    match outcome {
        DirectoryPublicationOutcome::RejectedClean(publication) => match failure {
            ProjectExportFailure::Staging(error) => {
                ProjectExportOutcome::Rejected(ProjectExportRejection {
                    plan,
                    inner: ProjectExportRejectionInner::Staging { error, publication },
                })
            }
            ProjectExportFailure::SourceChanged(error) => {
                ProjectExportOutcome::Rejected(ProjectExportRejection {
                    plan,
                    inner: ProjectExportRejectionInner::SourceChangedAfterStaging {
                        error,
                        publication,
                    },
                })
            }
        },
        DirectoryPublicationOutcome::Orphaned(publication) => {
            ProjectExportOutcome::Orphaned(ProjectExportOrphan {
                plan,
                failure: Some(failure),
                publication,
            })
        }
        DirectoryPublicationOutcome::Published(_) => {
            unreachable!("aborting an export publication cannot publish it")
        }
    }
}

fn next_staging_name() -> Result<RelativeComponent, getrandom::Error> {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut entropy = [0_u8; 16];
    getrandom::fill(&mut entropy)?;
    let mut suffix = String::with_capacity(entropy.len() * 2);
    for byte in entropy {
        suffix.push(char::from(HEX[usize::from(byte >> 4)]));
        suffix.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Ok(RelativeComponent::new(format!(".nara-export-{suffix}"))
        .expect("the generated export staging name is a valid component"))
}
