#[cfg(feature = "project-tooling")]
use std::array;
use std::{fmt, sync::Arc};

use nara_fs::{
    ContentDigest, DirectoryCapability, FileCapability, FileIdentity, FsError, RelativePath,
};
#[cfg(feature = "project-tooling")]
use nara_reflect::SchemaCompositionFingerprint;

use crate::project_host::StartupContentClosureReceipt;
#[cfg(feature = "project-tooling")]
use crate::project_tooling::cargo_build::CapturedCargoExecutable;

#[cfg(feature = "project-tooling")]
const MAX_BUILD_INPUT_COUNT: usize = 1024;
#[cfg(feature = "project-tooling")]
const MAX_BUILD_INPUT_BYTES: u64 = 64 * 1024 * 1024;
#[cfg(feature = "project-tooling")]
const MAX_EXECUTABLE_BYTES: u64 = 512 * 1024 * 1024;
#[cfg(feature = "project-tooling")]
const BUILD_INPUT_DOMAIN: &[u8] = b"nara.project-export-build-input.v1\0";

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
    build_provenance_digest: ContentDigest,
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

    /// Identifies the captured build observation separately from executable bytes and startup.
    #[must_use]
    pub const fn build_provenance_digest(self) -> ContentDigest {
        self.build_provenance_digest
    }
}

/// Exact build facts consumed by one startup-closure export attempt.
///
/// The receipt owns the exact captured executable bytes privately. It is deliberately move-only: an
/// export consumes one exact Cargo selection rather than accepting caller-authored executable
/// paths or rediscovering target-directory contents. Captured build observations do not establish
/// Runtime Ready; separately executed products must still agree with the startup inputs.
pub struct ProjectExportBuildReceipt {
    project_root_identity: FileIdentity,
    build_input_revision: BuildInputRevision,
    package_identity_digest: ContentDigest,
    target_triple: String,
    profile: String,
    features: Box<[String]>,
    executables: [PreparedExecutable; 2],
    product_agreements: [ProjectExportProductAgreement; 2],
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

    /// The normalized requested features shared by both builds, separate from Cargo's resolution.
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
        let authoring_schema_bundle_digest = startup_content
            .runtime_schema_bundle_digest()
            .ok_or(ProjectExportBuildError::RuntimeSchemaAgreementMissing)?;
        let schema_fingerprint = startup_content.schema_fingerprint().to_string();
        for executable in &self.executables {
            let agreement = &self.product_agreements[executable.receipt.target.index()];
            if !agreement.matches_inputs(
                &executable.receipt,
                startup_content.manifest_digest(),
                startup_content.content_digest(),
                startup_content.closure_plan_digest(),
                authoring_schema_bundle_digest,
                &schema_fingerprint,
            ) {
                return Err(ProjectExportBuildError::ProductAgreementMismatch {
                    target: executable.receipt.target,
                });
            }
        }
        Ok(())
    }

    pub(super) fn executable_bytes(&self, target: ProjectExportBinaryTarget) -> &[u8] {
        &self.executables[target.index()].bytes
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
            .field("product_agreements", &self.product_agreements)
            .finish()
    }
}

struct PreparedExecutable {
    receipt: ProjectExportExecutableReceipt,
    bytes: Arc<[u8]>,
}

/// Declared build-input checkpoint captured before the Cargo Adapter starts a locked build.
///
/// The inventory is explicit and bounded, with per-file identity and digest revalidation. It is
/// not a complete source snapshot or an inventory of Cargo's build-script and environment inputs.
#[doc(hidden)]
#[cfg(feature = "project-tooling")]
pub struct ProjectExportBuildPreparation {
    project_root: DirectoryCapability,
    revision: BuildInputRevision,
    manifest_digest: ContentDigest,
    startup_content_digest: ContentDigest,
    startup_closure_plan_digest: ContentDigest,
    authoring_schema_bundle_digest: ContentDigest,
    schema_fingerprint: SchemaCompositionFingerprint,
}

#[cfg(feature = "project-tooling")]
impl ProjectExportBuildPreparation {
    pub fn capture(
        startup_content: &StartupContentClosureReceipt,
        inputs: impl IntoIterator<Item = RelativePath>,
    ) -> Result<Self, ProjectExportBuildError> {
        let project_root = startup_content.project_root();
        let revision = BuildInputRevision::capture(&project_root, inputs)?;
        let authoring_schema_bundle_digest = startup_content
            .runtime_schema_bundle_digest()
            .ok_or(ProjectExportBuildError::RuntimeSchemaAgreementMissing)?;
        Ok(Self {
            project_root,
            revision,
            manifest_digest: startup_content.manifest_digest(),
            startup_content_digest: startup_content.content_digest(),
            startup_closure_plan_digest: startup_content.closure_plan_digest(),
            authoring_schema_bundle_digest,
            schema_fingerprint: startup_content.schema_fingerprint(),
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
        agreements: [ProjectExportProductAgreement; 2],
    ) -> Result<ProjectExportBuildReceipt, ProjectExportBuildError> {
        self.revision.revalidate(&self.project_root)?;
        let [first, second] = artifacts;
        let selection = ArtifactSelection::matching(&first, &second)?;
        let mut executables: [Option<PreparedExecutable>; 2] = array::from_fn(|_| None);
        for artifact in [first, second] {
            let target = artifact.target;
            // Startup agreement cannot prove which Rust input revision produced these bytes.
            if !self
                .revision
                .matches_declared_inputs(&artifact.declared_inputs)
            {
                return Err(ProjectExportBuildError::ArtifactBuildInputsMismatch { target });
            }
            let index = target.index();
            if executables[index].is_some() {
                return Err(ProjectExportBuildError::DuplicateExecutable { target });
            }
            executables[index] = Some(PreparedExecutable {
                receipt: ProjectExportExecutableReceipt {
                    target,
                    digest: artifact.digest,
                    build_provenance_digest: artifact.build_provenance_digest,
                },
                bytes: artifact.bytes,
            });
        }
        let [headless, desktop] = executables;
        let headless = headless.ok_or(ProjectExportBuildError::MissingExecutable {
            target: ProjectExportBinaryTarget::Headless,
        })?;
        let desktop = desktop.ok_or(ProjectExportBuildError::MissingExecutable {
            target: ProjectExportBinaryTarget::Desktop,
        })?;
        let agreements = bind_product_agreements(
            agreements,
            [&headless.receipt, &desktop.receipt],
            self.manifest_digest,
            self.startup_content_digest,
            self.startup_closure_plan_digest,
            self.authoring_schema_bundle_digest,
            &self.schema_fingerprint.to_string(),
        )?;
        Ok(ProjectExportBuildReceipt {
            project_root_identity: self.project_root.identity(),
            build_input_revision: self.revision,
            package_identity_digest: ContentDigest::of_bytes(selection.package.as_bytes()),
            target_triple: selection.target_triple,
            profile: selection.profile,
            features: selection.features,
            executables: [headless, desktop],
            product_agreements: agreements,
        })
    }
}

#[cfg(feature = "project-tooling")]
impl fmt::Debug for ProjectExportBuildPreparation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectExportBuildPreparation")
            .field("project_root_identity", &self.project_root.identity())
            .field("build_input_digest", &self.revision.digest)
            .field("manifest_digest", &self.manifest_digest)
            .field("startup_content_digest", &self.startup_content_digest)
            .field(
                "startup_closure_plan_digest",
                &self.startup_closure_plan_digest,
            )
            .field(
                "authoring_schema_bundle_digest",
                &self.authoring_schema_bundle_digest,
            )
            .field("schema_fingerprint", &self.schema_fingerprint)
            .finish()
    }
}

/// Exact pre-publication observation made by one selected product executable.
///
/// The private Cargo Adapter may construct this value only after the executable has run its own
/// provider-capable startup path and reported the exact schema and source facts it admitted.
#[doc(hidden)]
pub struct ProjectExportProductAgreement {
    target: ProjectExportBinaryTarget,
    executable_digest: ContentDigest,
    manifest_digest: ContentDigest,
    startup_content_digest: ContentDigest,
    startup_closure_plan_digest: ContentDigest,
    authoring_schema_bundle_digest: ContentDigest,
    schema_fingerprint: Box<str>,
    observation_digest: ContentDigest,
}

impl ProjectExportProductAgreement {
    fn matches_inputs(
        &self,
        executable: &ProjectExportExecutableReceipt,
        manifest_digest: ContentDigest,
        startup_content_digest: ContentDigest,
        startup_closure_plan_digest: ContentDigest,
        authoring_schema_bundle_digest: ContentDigest,
        schema_fingerprint: &str,
    ) -> bool {
        self.target == executable.target
            && self.executable_digest == executable.digest
            && self.manifest_digest == manifest_digest
            && self.startup_content_digest == startup_content_digest
            && self.startup_closure_plan_digest == startup_closure_plan_digest
            && self.authoring_schema_bundle_digest == authoring_schema_bundle_digest
            && self.schema_fingerprint.as_ref() == schema_fingerprint
    }

    #[cfg(feature = "project-tooling")]
    pub fn new(
        target: ProjectExportBinaryTarget,
        executable_digest: ContentDigest,
        manifest_digest: ContentDigest,
        startup_content_digest: ContentDigest,
        startup_closure_plan_digest: ContentDigest,
        authoring_schema_bundle_digest: ContentDigest,
        schema_fingerprint: impl Into<String>,
        observation_digest: ContentDigest,
    ) -> Result<Self, ProjectExportBuildError> {
        let schema_fingerprint = schema_fingerprint.into();
        if schema_fingerprint.len() != 64
            || !schema_fingerprint
                .bytes()
                .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
        {
            return Err(ProjectExportBuildError::InvalidProductSchemaFingerprint { target });
        }
        Ok(Self {
            target,
            executable_digest,
            manifest_digest,
            startup_content_digest,
            startup_closure_plan_digest,
            authoring_schema_bundle_digest,
            schema_fingerprint: schema_fingerprint.into_boxed_str(),
            observation_digest,
        })
    }
}

impl fmt::Debug for ProjectExportProductAgreement {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectExportProductAgreement")
            .field("target", &self.target)
            .field("executable_digest", &self.executable_digest)
            .field("manifest_digest", &self.manifest_digest)
            .field("startup_content_digest", &self.startup_content_digest)
            .field(
                "startup_closure_plan_digest",
                &self.startup_closure_plan_digest,
            )
            .field(
                "authoring_schema_bundle_digest",
                &self.authoring_schema_bundle_digest,
            )
            .field("schema_fingerprint", &self.schema_fingerprint)
            .field("observation_digest", &self.observation_digest)
            .finish()
    }
}

#[cfg(feature = "project-tooling")]
fn bind_product_agreements(
    agreements: [ProjectExportProductAgreement; 2],
    executables: [&ProjectExportExecutableReceipt; 2],
    manifest_digest: ContentDigest,
    startup_content_digest: ContentDigest,
    startup_closure_plan_digest: ContentDigest,
    authoring_schema_bundle_digest: ContentDigest,
    schema_fingerprint: &str,
) -> Result<[ProjectExportProductAgreement; 2], ProjectExportBuildError> {
    let mut ordered: [Option<ProjectExportProductAgreement>; 2] = array::from_fn(|_| None);
    for agreement in agreements {
        let target = agreement.target;
        let executable = executables[target.index()];
        if !agreement.matches_inputs(
            executable,
            manifest_digest,
            startup_content_digest,
            startup_closure_plan_digest,
            authoring_schema_bundle_digest,
            schema_fingerprint,
        ) {
            return Err(ProjectExportBuildError::ProductAgreementMismatch { target });
        }
        if ordered[target.index()].replace(agreement).is_some() {
            return Err(ProjectExportBuildError::DuplicateProductAgreement { target });
        }
    }
    let [headless, desktop] = ordered;
    Ok([
        headless.ok_or(ProjectExportBuildError::MissingProductAgreement {
            target: ProjectExportBinaryTarget::Headless,
        })?,
        desktop.ok_or(ProjectExportBuildError::MissingProductAgreement {
            target: ProjectExportBinaryTarget::Desktop,
        })?,
    ])
}

/// One exact captured Cargo executable selected by the private Adapter.
#[doc(hidden)]
#[cfg(feature = "project-tooling")]
pub struct ProjectExportCargoArtifact {
    target: ProjectExportBinaryTarget,
    package: String,
    target_triple: String,
    profile: String,
    features: Box<[String]>,
    default_features: bool,
    bytes: Arc<[u8]>,
    digest: ContentDigest,
    build_provenance_digest: ContentDigest,
    declared_inputs: Box<[CapturedBuildInput]>,
}

#[cfg(feature = "project-tooling")]
impl ProjectExportCargoArtifact {
    /// Retains captured bytes and bounded declared-input facts without reopening Cargo output.
    pub fn from_captured(
        target: ProjectExportBinaryTarget,
        captured: CapturedCargoExecutable,
    ) -> Result<Self, ProjectExportBuildError> {
        let provenance = captured.provenance();
        let selection = provenance.selection();
        if selection.target() != target.cargo_target_name() {
            return Err(ProjectExportBuildError::ArtifactTargetMismatch { target });
        }
        let digest = captured.digest();
        if digest.length() > MAX_EXECUTABLE_BYTES {
            return Err(ProjectExportBuildError::ExecutableUnavailable {
                target,
                source: FsError::ByteLimitExceeded {
                    limit: MAX_EXECUTABLE_BYTES,
                },
            });
        }
        let declared_inputs = captured_build_inputs(
            provenance
                .declared_inputs()
                .iter()
                .map(|input| (input.path(), input.digest())),
        )?;
        Ok(Self {
            target,
            package: selection.package().to_owned(),
            target_triple: selection.target_triple().to_owned(),
            profile: selection.profile().to_owned(),
            features: selection.features().to_vec().into_boxed_slice(),
            default_features: selection.default_features(),
            bytes: captured.shared_bytes(),
            digest,
            build_provenance_digest: provenance.digest(),
            declared_inputs,
        })
    }

    // Synthetic owner fixtures have no Cargo observation. Keep file-based construction out of
    // the production API and mark its provenance explicitly rather than fabricating build facts.
    #[cfg(test)]
    pub(super) fn from_file(
        target: ProjectExportBinaryTarget,
        package: impl Into<String>,
        target_triple: impl Into<String>,
        profile: impl Into<String>,
        features: impl IntoIterator<Item = String>,
        file: FileCapability,
        preparation: &ProjectExportBuildPreparation,
    ) -> Result<Self, ProjectExportBuildError> {
        let package = bounded_ascii(package.into(), "Cargo package selection", 4096)?;
        let target_triple = bounded_ascii(target_triple.into(), "Cargo target triple", 256)?;
        let profile = bounded_ascii(profile.into(), "Cargo profile", 64)?;
        let features = normalize_features(features)?;
        let bytes = file
            .read_to_end_bounded(MAX_EXECUTABLE_BYTES)
            .map_err(|source| ProjectExportBuildError::ExecutableUnavailable { target, source })?;
        let digest = ContentDigest::of_bytes(&bytes);
        let declared_inputs = captured_build_inputs(
            preparation
                .revision
                .members
                .iter()
                .map(|member| (member.path.as_str(), member.digest)),
        )?;
        Ok(Self {
            target,
            package,
            target_triple,
            profile,
            features,
            default_features: false,
            bytes: bytes.into(),
            digest,
            build_provenance_digest: ContentDigest::of_bytes(
                b"synthetic export build provenance; no Cargo build ran",
            ),
            declared_inputs,
        })
    }
}

#[cfg(feature = "project-tooling")]
struct CapturedBuildInput {
    path: String,
    digest: ContentDigest,
}

#[cfg(feature = "project-tooling")]
fn captured_build_inputs<'a>(
    inputs: impl IntoIterator<Item = (&'a str, ContentDigest)>,
) -> Result<Box<[CapturedBuildInput]>, ProjectExportBuildError> {
    let mut captured = Vec::new();
    let mut total_bytes = 0_u64;
    for (index, (path, digest)) in inputs.into_iter().enumerate() {
        if index >= MAX_BUILD_INPUT_COUNT {
            return Err(ProjectExportBuildError::BuildInputCountExceeded);
        }
        RelativePath::new(path).map_err(|_| ProjectExportBuildError::InvalidBuildInputPath)?;
        total_bytes = total_bytes
            .checked_add(digest.length())
            .filter(|total| *total <= MAX_BUILD_INPUT_BYTES)
            .ok_or(ProjectExportBuildError::BuildInputByteLimitExceeded)?;
        captured.push(CapturedBuildInput {
            path: path.to_owned(),
            digest,
        });
    }
    captured.sort_unstable_by(|left, right| left.path.cmp(&right.path));
    if let Some(pair) = captured
        .windows(2)
        .find(|pair| pair[0].path == pair[1].path)
    {
        return Err(ProjectExportBuildError::DuplicateBuildInput {
            path: pair[0].path.clone(),
        });
    }
    Ok(captured.into_boxed_slice())
}

#[cfg(feature = "project-tooling")]
struct ArtifactSelection {
    package: String,
    target_triple: String,
    profile: String,
    features: Box<[String]>,
}

#[cfg(feature = "project-tooling")]
impl ArtifactSelection {
    fn matching(
        first: &ProjectExportCargoArtifact,
        second: &ProjectExportCargoArtifact,
    ) -> Result<Self, ProjectExportBuildError> {
        if first.package != second.package
            || first.target_triple != second.target_triple
            || first.profile != second.profile
            || first.features != second.features
            || first.default_features != second.default_features
        {
            return Err(ProjectExportBuildError::ArtifactInvocationMismatch);
        }
        Ok(Self {
            package: first.package.clone(),
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
    #[cfg(feature = "project-tooling")]
    fn matches_declared_inputs(&self, captured: &[CapturedBuildInput]) -> bool {
        self.members.len() == captured.len()
            && self
                .members
                .iter()
                .zip(captured)
                .all(|(member, input)| member.path == input.path && member.digest == input.digest)
    }

    #[cfg(feature = "project-tooling")]
    fn capture(
        project_root: &DirectoryCapability,
        inputs: impl IntoIterator<Item = RelativePath>,
    ) -> Result<Self, ProjectExportBuildError> {
        let mut paths = Vec::new();
        for relative in inputs {
            if paths.len() >= MAX_BUILD_INPUT_COUNT {
                return Err(ProjectExportBuildError::BuildInputCountExceeded);
            }
            let mut path = String::new();
            for component in relative.components() {
                if !path.is_empty() {
                    path.push('/');
                }
                path.push_str(
                    component
                        .to_str()
                        .ok_or(ProjectExportBuildError::InvalidBuildInputPath)?,
                );
            }
            paths.push((path, relative));
        }
        paths.sort_unstable_by(|left, right| left.0.cmp(&right.0));
        if let Some(pair) = paths.windows(2).find(|pair| pair[0].0 == pair[1].0) {
            return Err(ProjectExportBuildError::DuplicateBuildInput {
                path: pair[0].0.clone(),
            });
        }
        for required in ["Cargo.toml", "Cargo.lock"] {
            if !paths.iter().any(|(path, _)| path == required) {
                return Err(ProjectExportBuildError::MissingBuildInput { path: required });
            }
        }
        let mut members = Vec::with_capacity(paths.len());
        let mut total_bytes = 0;
        let mut framed = Vec::new();
        framed.extend_from_slice(BUILD_INPUT_DOMAIN);
        for (path, relative) in paths {
            let file = project_root.open_file_exact(&relative).map_err(|source| {
                ProjectExportBuildError::BuildInputUnavailable {
                    path: path.clone(),
                    source,
                }
            })?;
            let digest = file
                .digest(MAX_BUILD_INPUT_BYTES - total_bytes)
                .map_err(|source| match source {
                    FsError::ByteLimitExceeded { .. } => {
                        ProjectExportBuildError::BuildInputByteLimitExceeded
                    }
                    source => ProjectExportBuildError::BuildInputUnavailable {
                        path: path.clone(),
                        source,
                    },
                })?;
            total_bytes += digest.length();
            append_frame(&mut framed, path.as_bytes());
            append_frame(&mut framed, &digest.length().to_le_bytes());
            append_frame(&mut framed, digest.as_bytes());
            members.push(BuildInputMember { path, file, digest });
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
        for member in &self.members {
            let relative = RelativePath::new(&member.path)
                .expect("captured build input paths retain their validated exact spelling");
            let current = project_root.open_file_exact(&relative).map_err(|source| {
                ProjectExportBuildError::BuildInputUnavailable {
                    path: member.path.clone(),
                    source,
                }
            })?;
            let same_file = current.same_object(&member.file).map_err(|source| {
                ProjectExportBuildError::BuildInputUnavailable {
                    path: member.path.clone(),
                    source,
                }
            })?;
            if !same_file {
                return Err(ProjectExportBuildError::BuildInputsChanged);
            }
            current
                .verify_digest(member.digest, member.digest.length())
                .map_err(|source| match source {
                    FsError::DigestMismatch { .. } | FsError::ByteLimitExceeded { .. } => {
                        ProjectExportBuildError::BuildInputsChanged
                    }
                    source => ProjectExportBuildError::BuildInputUnavailable {
                        path: member.path.clone(),
                        source,
                    },
                })?;
        }
        Ok(())
    }
}

struct BuildInputMember {
    path: String,
    file: FileCapability,
    digest: ContentDigest,
}

#[cfg(all(test, feature = "project-tooling"))]
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

#[cfg(all(test, feature = "project-tooling"))]
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

#[cfg(feature = "project-tooling")]
fn append_frame(target: &mut Vec<u8>, bytes: &[u8]) {
    target.extend_from_slice(&(bytes.len() as u64).to_le_bytes());
    target.extend_from_slice(bytes);
}

#[derive(Debug, thiserror::Error)]
pub enum ProjectExportBuildError {
    #[error("project export startup content lacks a checked runtime schema agreement")]
    RuntimeSchemaAgreementMissing,
    #[error("project export build input {path} is unavailable")]
    BuildInputUnavailable {
        path: String,
        #[source]
        source: FsError,
    },
    #[error("project export build inputs changed after preparation")]
    BuildInputsChanged,
    #[error("project export build input paths must have exact UTF-8 spellings")]
    InvalidBuildInputPath,
    #[error("project export build inputs exceed the 1024-member limit")]
    BuildInputCountExceeded,
    #[error("project export build inputs exceed the 64 MiB total byte limit")]
    BuildInputByteLimitExceeded,
    #[error("project export build input {path} was declared more than once")]
    DuplicateBuildInput { path: String },
    #[error("project export build inputs must declare {path}")]
    MissingBuildInput { path: &'static str },
    #[error(
        "the Cargo artifacts do not share one package, target triple, profile and feature selection"
    )]
    ArtifactInvocationMismatch,
    #[error("the captured Cargo binary does not match the {target:?} export role")]
    ArtifactTargetMismatch { target: ProjectExportBinaryTarget },
    #[error("the {target:?} captured build inputs do not match the export preparation")]
    ArtifactBuildInputsMismatch { target: ProjectExportBinaryTarget },
    #[error("the Cargo artifact field {field} is invalid")]
    InvalidArtifactField { field: &'static str },
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
    #[error("the Cargo build receipt belongs to another project authority")]
    ProjectAuthorityMismatch,
    #[error("the {target:?} product reported an invalid schema fingerprint")]
    InvalidProductSchemaFingerprint { target: ProjectExportBinaryTarget },
    #[error("the {target:?} product was observed more than once")]
    DuplicateProductAgreement { target: ProjectExportBinaryTarget },
    #[error("the {target:?} product was not observed before export publication")]
    MissingProductAgreement { target: ProjectExportBinaryTarget },
    #[error("the {target:?} product observation does not match the export inputs")]
    ProductAgreementMismatch { target: ProjectExportBinaryTarget },
}
