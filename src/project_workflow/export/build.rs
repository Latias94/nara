use std::fmt;

#[cfg(feature = "project-workflow-cli")]
use std::array;

use nara_fs::{
    ContentDigest, DirectoryCapability, FileCapability, FileIdentity, FsError, RelativePath,
};

use crate::project_host::StartupContentClosureReceipt;

const MAX_BUILD_INPUT_BYTES: u64 = 4 * 1024 * 1024;
const MAX_EXECUTABLE_BYTES: u64 = 512 * 1024 * 1024;
const BUILD_INPUT_DOMAIN: &[u8] = b"nara.project-export-build-input.v1\0";
#[cfg(feature = "project-workflow-cli")]
const EXPECTED_PROFILE: &str = "release";
#[cfg(feature = "project-workflow-cli")]
const EXPECTED_FEATURE: &str = "desktop";

const BUILD_INPUT_PATHS: [&str; 5] = [
    "Cargo.toml",
    "Cargo.lock",
    "src/lib.rs",
    "src/bin/headless.rs",
    "src/bin/desktop.rs",
];

/// The two executable roles admitted by the closed desktop-2D export contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectExportBinaryTarget {
    Headless,
    Desktop,
}

impl ProjectExportBinaryTarget {
    #[must_use]
    pub const fn cargo_target_name(self) -> &'static str {
        match self {
            Self::Headless => "headless",
            Self::Desktop => "desktop",
        }
    }

    const fn index(self) -> usize {
        match self {
            Self::Headless => 0,
            Self::Desktop => 1,
        }
    }
}

/// Public, path-free facts for one exact executable selected by the private Cargo Adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProjectExportExecutableReceipt {
    target: ProjectExportBinaryTarget,
    digest: ContentDigest,
}

impl ProjectExportExecutableReceipt {
    #[must_use]
    pub const fn target(self) -> ProjectExportBinaryTarget {
        self.target
    }

    #[must_use]
    pub const fn digest(self) -> ContentDigest {
        self.digest
    }
}

/// Exact build facts consumed by one startup-closure export attempt.
///
/// The receipt owns the opened executable handles privately. It is deliberately move-only: an
/// export consumes one exact Cargo selection rather than accepting caller-authored executable
/// paths or rediscovering target-directory contents.
pub struct ProjectExportBuildReceipt {
    project_root_identity: FileIdentity,
    build_input_revision: BuildInputRevision,
    package_identity_digest: ContentDigest,
    target_triple: String,
    profile: String,
    features: Box<[String]>,
    executables: [PreparedExecutable; 2],
}

impl ProjectExportBuildReceipt {
    #[must_use]
    pub const fn project_root_identity(&self) -> FileIdentity {
        self.project_root_identity
    }

    #[must_use]
    pub const fn build_input_digest(&self) -> ContentDigest {
        self.build_input_revision.digest
    }

    #[must_use]
    pub const fn package_identity_digest(&self) -> ContentDigest {
        self.package_identity_digest
    }

    #[must_use]
    pub fn target_triple(&self) -> &str {
        &self.target_triple
    }

    #[must_use]
    pub fn profile(&self) -> &str {
        &self.profile
    }

    #[must_use]
    pub fn features(&self) -> &[String] {
        &self.features
    }

    #[must_use]
    pub const fn executables(&self) -> [ProjectExportExecutableReceipt; 2] {
        [self.executables[0].receipt, self.executables[1].receipt]
    }

    pub(super) fn revalidate(
        &self,
        startup_content: &StartupContentClosureReceipt,
    ) -> Result<(), ProjectExportBuildError> {
        if startup_content.project_root_identity() != self.project_root_identity {
            return Err(ProjectExportBuildError::ProjectAuthorityMismatch);
        }
        self.build_input_revision
            .revalidate(&startup_content.project_root())?;
        for executable in &self.executables {
            executable
                .file
                .verify_digest(executable.receipt.digest, MAX_EXECUTABLE_BYTES)
                .map_err(|source| ProjectExportBuildError::ExecutableChanged {
                    target: executable.receipt.target,
                    source,
                })?;
        }
        Ok(())
    }

    pub(super) fn executable(&self, target: ProjectExportBinaryTarget) -> &FileCapability {
        &self.executables[target.index()].file
    }
}

impl fmt::Debug for ProjectExportBuildReceipt {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let executables = self.executables();
        formatter
            .debug_struct("ProjectExportBuildReceipt")
            .field("project_root_identity", &self.project_root_identity)
            .field("build_input_digest", &self.build_input_revision.digest)
            .field("package_identity_digest", &self.package_identity_digest)
            .field("target_triple", &self.target_triple)
            .field("profile", &self.profile)
            .field("features", &self.features)
            .field("executables", &executables)
            .finish()
    }
}

struct PreparedExecutable {
    receipt: ProjectExportExecutableReceipt,
    file: FileCapability,
}

/// Build-input checkpoint captured before the private Cargo Adapter starts `cargo build --locked`.
#[doc(hidden)]
#[cfg(feature = "project-workflow-cli")]
pub struct ProjectExportBuildPreparation {
    project_root: DirectoryCapability,
    revision: BuildInputRevision,
}

#[cfg(feature = "project-workflow-cli")]
impl ProjectExportBuildPreparation {
    pub fn capture(
        startup_content: &StartupContentClosureReceipt,
    ) -> Result<Self, ProjectExportBuildError> {
        let project_root = startup_content.project_root();
        let revision = BuildInputRevision::capture(&project_root)?;
        Ok(Self {
            project_root,
            revision,
        })
    }

    #[must_use]
    pub fn project_root_identity(&self) -> FileIdentity {
        self.project_root.identity()
    }

    #[must_use]
    pub const fn build_input_digest(&self) -> ContentDigest {
        self.revision.digest
    }

    pub fn verify_build_inputs(
        &self,
        project_root: &DirectoryCapability,
    ) -> Result<(), ProjectExportBuildError> {
        self.revision.revalidate(project_root)
    }

    pub fn bind(
        self,
        artifacts: [ProjectExportCargoArtifact; 2],
    ) -> Result<ProjectExportBuildReceipt, ProjectExportBuildError> {
        self.revision.revalidate(&self.project_root)?;
        let [first, second] = artifacts;
        let invocation = ArtifactInvocation::matching(&first, &second)?;
        let mut executables: [Option<PreparedExecutable>; 2] = array::from_fn(|_| None);
        for artifact in [first, second] {
            let target = artifact.target;
            let index = target.index();
            if executables[index].is_some() {
                return Err(ProjectExportBuildError::DuplicateExecutable { target });
            }
            let digest = artifact
                .file
                .digest(MAX_EXECUTABLE_BYTES)
                .map_err(|source| ProjectExportBuildError::ExecutableUnavailable {
                    target,
                    source,
                })?;
            executables[index] = Some(PreparedExecutable {
                receipt: ProjectExportExecutableReceipt { target, digest },
                file: artifact.file,
            });
        }
        let [headless, desktop] = executables;
        let headless = headless.ok_or(ProjectExportBuildError::MissingExecutable {
            target: ProjectExportBinaryTarget::Headless,
        })?;
        let desktop = desktop.ok_or(ProjectExportBuildError::MissingExecutable {
            target: ProjectExportBinaryTarget::Desktop,
        })?;
        Ok(ProjectExportBuildReceipt {
            project_root_identity: self.project_root.identity(),
            build_input_revision: self.revision,
            package_identity_digest: ContentDigest::of_bytes(invocation.package_id.as_bytes()),
            target_triple: invocation.target_triple,
            profile: invocation.profile,
            features: invocation.features,
            executables: [headless, desktop],
        })
    }
}

#[cfg(feature = "project-workflow-cli")]
impl fmt::Debug for ProjectExportBuildPreparation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectExportBuildPreparation")
            .field("project_root_identity", &self.project_root.identity())
            .field("build_input_digest", &self.revision.digest)
            .finish()
    }
}

/// One structurally matched Cargo compiler-artifact selected by the private Adapter.
#[doc(hidden)]
#[cfg(feature = "project-workflow-cli")]
pub struct ProjectExportCargoArtifact {
    target: ProjectExportBinaryTarget,
    package_id: String,
    target_triple: String,
    profile: String,
    features: Box<[String]>,
    file: FileCapability,
}

#[cfg(feature = "project-workflow-cli")]
impl ProjectExportCargoArtifact {
    pub fn new(
        target: ProjectExportBinaryTarget,
        package_id: impl Into<String>,
        target_triple: impl Into<String>,
        profile: impl Into<String>,
        features: impl IntoIterator<Item = String>,
        file: FileCapability,
    ) -> Result<Self, ProjectExportBuildError> {
        let package_id = bounded_ascii(package_id.into(), "Cargo package identity", 4096)?;
        let target_triple = bounded_ascii(target_triple.into(), "Cargo target triple", 256)?;
        let profile = bounded_ascii(profile.into(), "Cargo profile", 64)?;
        if profile != EXPECTED_PROFILE {
            return Err(ProjectExportBuildError::UnexpectedProfile);
        }
        let features = normalize_features(features)?;
        if features.as_ref() != [EXPECTED_FEATURE] {
            return Err(ProjectExportBuildError::UnexpectedFeatures);
        }
        Ok(Self {
            target,
            package_id,
            target_triple,
            profile,
            features,
            file,
        })
    }
}

#[cfg(feature = "project-workflow-cli")]
struct ArtifactInvocation {
    package_id: String,
    target_triple: String,
    profile: String,
    features: Box<[String]>,
}

#[cfg(feature = "project-workflow-cli")]
impl ArtifactInvocation {
    fn matching(
        first: &ProjectExportCargoArtifact,
        second: &ProjectExportCargoArtifact,
    ) -> Result<Self, ProjectExportBuildError> {
        if first.package_id != second.package_id
            || first.target_triple != second.target_triple
            || first.profile != second.profile
            || first.features != second.features
        {
            return Err(ProjectExportBuildError::ArtifactInvocationMismatch);
        }
        Ok(Self {
            package_id: first.package_id.clone(),
            target_triple: first.target_triple.clone(),
            profile: first.profile.clone(),
            features: first.features.clone(),
        })
    }
}

struct BuildInputRevision {
    members: Box<[BuildInputMember]>,
    digest: ContentDigest,
}

impl BuildInputRevision {
    fn capture(project_root: &DirectoryCapability) -> Result<Self, ProjectExportBuildError> {
        let mut members = Vec::with_capacity(BUILD_INPUT_PATHS.len());
        let mut framed = Vec::new();
        framed.extend_from_slice(BUILD_INPUT_DOMAIN);
        for path in BUILD_INPUT_PATHS {
            let relative = RelativePath::new(path)
                .expect("the closed project build input inventory uses valid paths");
            let file = project_root.open_file(&relative).map_err(|source| {
                ProjectExportBuildError::BuildInputUnavailable { path, source }
            })?;
            let digest = file.digest(MAX_BUILD_INPUT_BYTES).map_err(|source| {
                ProjectExportBuildError::BuildInputUnavailable { path, source }
            })?;
            append_frame(&mut framed, path.as_bytes());
            append_frame(&mut framed, &digest.length().to_le_bytes());
            append_frame(&mut framed, digest.as_bytes());
            members.push(BuildInputMember { path, digest });
        }
        Ok(Self {
            members: members.into_boxed_slice(),
            digest: ContentDigest::of_bytes(&framed),
        })
    }

    fn revalidate(
        &self,
        project_root: &DirectoryCapability,
    ) -> Result<(), ProjectExportBuildError> {
        let current = Self::capture(project_root)?;
        if current.digest == self.digest
            && current
                .members
                .iter()
                .zip(self.members.iter())
                .all(|(current, expected)| current == expected)
        {
            Ok(())
        } else {
            Err(ProjectExportBuildError::BuildInputsChanged)
        }
    }
}

#[derive(PartialEq, Eq)]
struct BuildInputMember {
    path: &'static str,
    digest: ContentDigest,
}

#[cfg(feature = "project-workflow-cli")]
fn normalize_features(
    features: impl IntoIterator<Item = String>,
) -> Result<Box<[String]>, ProjectExportBuildError> {
    let mut features = features
        .into_iter()
        .map(|feature| bounded_ascii(feature, "Cargo feature", 256))
        .collect::<Result<Vec<_>, _>>()?;
    features.sort_unstable();
    features.dedup();
    Ok(features.into_boxed_slice())
}

#[cfg(feature = "project-workflow-cli")]
fn bounded_ascii(
    value: String,
    field: &'static str,
    limit: usize,
) -> Result<String, ProjectExportBuildError> {
    if value.is_empty() || value.len() > limit || !value.is_ascii() {
        Err(ProjectExportBuildError::InvalidArtifactField { field })
    } else {
        Ok(value)
    }
}

fn append_frame(target: &mut Vec<u8>, bytes: &[u8]) {
    target.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    target.extend_from_slice(bytes);
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectExportBuildError {
    #[error("project export build input {path} is unavailable")]
    BuildInputUnavailable {
        path: &'static str,
        #[source]
        source: FsError,
    },
    #[error("project export build inputs changed after preparation")]
    BuildInputsChanged,
    #[error("the Cargo artifact set does not belong to one exact invocation")]
    ArtifactInvocationMismatch,
    #[error("the Cargo artifact field {field} is invalid")]
    InvalidArtifactField { field: &'static str },
    #[error("the Cargo build profile is not the closed release profile")]
    UnexpectedProfile,
    #[error("the Cargo feature set is not the closed desktop export set")]
    UnexpectedFeatures,
    #[error("Cargo produced the {target:?} executable more than once")]
    DuplicateExecutable { target: ProjectExportBinaryTarget },
    #[error("Cargo did not produce the {target:?} executable")]
    MissingExecutable { target: ProjectExportBinaryTarget },
    #[error("the {target:?} executable is unavailable")]
    ExecutableUnavailable {
        target: ProjectExportBinaryTarget,
        #[source]
        source: FsError,
    },
    #[error("the {target:?} executable changed after Cargo selection")]
    ExecutableChanged {
        target: ProjectExportBinaryTarget,
        #[source]
        source: FsError,
    },
    #[error("the Cargo build receipt belongs to another project authority")]
    ProjectAuthorityMismatch,
}
