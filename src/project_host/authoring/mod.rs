//! Root-private admission for non-executable project authoring schema data.

mod bundle;

use std::{
    error::Error,
    fmt::{self, Display, Formatter},
    num::NonZeroU64,
    sync::Arc,
};

use nara_core::ByteLimit;
use nara_diagnostic::DiagnosticReport;
use nara_fs::{
    ContentDigest, DirectoryCapability, FileCapability, FileIdentity, FsError, PathValidationError,
    RelativePath,
};
use nara_project::{
    DEFAULT_MANIFEST_BYTE_LIMIT, EffectiveProjectSettings, ProjectManifest, ProjectPath,
    ProjectProfileError,
};
use nara_reflect::{
    AuthoringSchemaBundle, AuthoringSchemaBundleError, SchemaCompositionFingerprint,
};

use self::bundle::{AdmittedAuthoringSchemaBundle, admit_bundle};

const PROJECT_MANIFEST_PATH: &str = "nara.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AuthoringSchemaAdmissionLimits {
    aggregate_encoded_bytes: ByteLimit,
}

impl AuthoringSchemaAdmissionLimits {
    #[must_use]
    pub(crate) const fn new(aggregate_encoded_bytes: ByteLimit) -> Self {
        Self {
            aggregate_encoded_bytes,
        }
    }

    #[must_use]
    pub(crate) const fn aggregate_encoded_bytes(self) -> ByteLimit {
        self.aggregate_encoded_bytes
    }
}

impl Default for AuthoringSchemaAdmissionLimits {
    fn default() -> Self {
        let manifest_bytes = usize::try_from(DEFAULT_MANIFEST_BYTE_LIMIT)
            .expect("the manifest byte limit fits usize on supported targets");
        let bundle_bytes = nara_reflect::AuthoringSchemaBundleLimits::default()
            .encoded_bytes()
            .get();
        let aggregate_encoded_bytes = manifest_bytes
            .checked_add(bundle_bytes)
            .expect("the default authoring admission byte limits fit usize");
        Self::new(
            ByteLimit::new(aggregate_encoded_bytes)
                .expect("the default authoring admission byte limit is non-zero"),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AuthoringSchemaAdmissionUsage {
    manifest_encoded_bytes: usize,
    bundle_encoded_bytes: usize,
}

impl AuthoringSchemaAdmissionUsage {
    #[must_use]
    pub(crate) const fn manifest_encoded_bytes(self) -> usize {
        self.manifest_encoded_bytes
    }

    #[must_use]
    pub(crate) const fn bundle_encoded_bytes(self) -> usize {
        self.bundle_encoded_bytes
    }

    #[must_use]
    pub(crate) const fn aggregate_encoded_bytes(self) -> usize {
        self.manifest_encoded_bytes + self.bundle_encoded_bytes
    }
}

struct AuthoringSchemaAdmissionBudget {
    limits: AuthoringSchemaAdmissionLimits,
    manifest_encoded_bytes: usize,
    bundle_encoded_bytes: usize,
}

impl AuthoringSchemaAdmissionBudget {
    fn new(limits: AuthoringSchemaAdmissionLimits) -> Self {
        Self {
            limits,
            manifest_encoded_bytes: 0,
            bundle_encoded_bytes: 0,
        }
    }

    fn read_limit(
        &self,
        source_limit: usize,
    ) -> Result<(usize, bool), AuthoringSchemaAdmissionError> {
        let remaining = self
            .limits
            .aggregate_encoded_bytes()
            .get()
            .saturating_sub(self.usage().aggregate_encoded_bytes());
        if remaining == 0 {
            return Err(
                AuthoringSchemaAdmissionError::AggregateEncodedBytesExceeded {
                    maximum: self.limits.aggregate_encoded_bytes().get(),
                },
            );
        }
        Ok((remaining.min(source_limit), remaining < source_limit))
    }

    const fn maximum(&self) -> usize {
        self.limits.aggregate_encoded_bytes().get()
    }

    fn charge_manifest(&mut self, bytes: usize) -> Result<(), AuthoringSchemaAdmissionError> {
        self.manifest_encoded_bytes = bytes;
        self.ensure_within_limit()
    }

    fn charge_bundle(&mut self, bytes: usize) -> Result<(), AuthoringSchemaAdmissionError> {
        self.bundle_encoded_bytes = bytes;
        self.ensure_within_limit()
    }

    fn ensure_within_limit(&self) -> Result<(), AuthoringSchemaAdmissionError> {
        if self.usage().aggregate_encoded_bytes() > self.limits.aggregate_encoded_bytes().get() {
            return Err(
                AuthoringSchemaAdmissionError::AggregateEncodedBytesExceeded {
                    maximum: self.limits.aggregate_encoded_bytes().get(),
                },
            );
        }
        Ok(())
    }

    const fn usage(&self) -> AuthoringSchemaAdmissionUsage {
        AuthoringSchemaAdmissionUsage {
            manifest_encoded_bytes: self.manifest_encoded_bytes,
            bundle_encoded_bytes: self.bundle_encoded_bytes,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct AuthoringSchemaAttemptGeneration(NonZeroU64);

impl AuthoringSchemaAttemptGeneration {
    #[must_use]
    pub(crate) const fn get(self) -> u64 {
        self.0.get()
    }
}

pub(crate) struct AuthoringProjectAdmissionHost {
    limits: AuthoringSchemaAdmissionLimits,
    next_attempt: Option<NonZeroU64>,
}

impl Default for AuthoringProjectAdmissionHost {
    fn default() -> Self {
        Self::new(AuthoringSchemaAdmissionLimits::default())
    }
}

impl AuthoringProjectAdmissionHost {
    #[must_use]
    pub(crate) const fn new(limits: AuthoringSchemaAdmissionLimits) -> Self {
        Self {
            limits,
            next_attempt: Some(NonZeroU64::MIN),
        }
    }

    pub(crate) fn prepare(
        &mut self,
        project_root: DirectoryCapability,
        profile: Option<&str>,
    ) -> Result<PreparedAuthoringSchemaBasis, AuthoringSchemaAdmissionError> {
        let attempt = self.allocate_attempt()?;
        let root_identity = project_root.identity();
        let manifest_path = RelativePath::new(PROJECT_MANIFEST_PATH)
            .map_err(AuthoringSchemaAdmissionError::ManifestPath)?;
        let manifest_capability = project_root
            .open_file(&manifest_path)
            .map_err(AuthoringSchemaAdmissionError::ManifestHost)?;
        if manifest_capability.parent_identity() != Some(root_identity) {
            return Err(AuthoringSchemaAdmissionError::ManifestRootMismatch);
        }

        let mut budget = AuthoringSchemaAdmissionBudget::new(self.limits);
        let manifest_limit = usize::try_from(DEFAULT_MANIFEST_BYTE_LIMIT)
            .expect("the manifest byte limit fits usize on supported targets");
        let aggregate_maximum = budget.maximum();
        let (read_limit, aggregate_limited) = budget.read_limit(manifest_limit)?;
        let manifest_bytes = manifest_capability
            .read_to_end_bounded(u64::try_from(read_limit).unwrap_or(u64::MAX))
            .map_err(|error| {
                map_manifest_read_error(error, aggregate_limited, read_limit, aggregate_maximum)
            })?;
        budget.charge_manifest(manifest_bytes.len())?;
        let manifest_digest = ContentDigest::of_bytes(&manifest_bytes);

        let load = ProjectManifest::parse_toml_bytes(&manifest_bytes);
        if load.has_errors() || load.manifest.is_none() {
            return Err(AuthoringSchemaAdmissionError::ManifestInvalid {
                diagnostics: load.diagnostics,
            });
        }
        let manifest = load
            .manifest
            .expect("a successful manifest load contains a manifest");
        let settings = manifest
            .resolve_profile(profile)
            .map_err(AuthoringSchemaAdmissionError::Profile)?;
        let bundle_path = settings
            .authoring_schema_bundle
            .clone()
            .ok_or(AuthoringSchemaAdmissionError::MissingSchemaBundle)?;

        Ok(PreparedAuthoringSchemaBasis {
            attempt,
            project_root,
            manifest: AdmittedAuthoringManifest {
                capability: manifest_capability,
                bytes: Arc::from(manifest_bytes),
                digest: manifest_digest,
            },
            settings,
            bundle_path,
            budget,
        })
    }

    fn allocate_attempt(
        &mut self,
    ) -> Result<AuthoringSchemaAttemptGeneration, AuthoringSchemaAdmissionError> {
        let attempt = self
            .next_attempt
            .ok_or(AuthoringSchemaAdmissionError::AttemptGenerationExhausted)?;
        self.next_attempt = attempt.get().checked_add(1).and_then(NonZeroU64::new);
        Ok(AuthoringSchemaAttemptGeneration(attempt))
    }
}

struct AdmittedAuthoringManifest {
    capability: FileCapability,
    bytes: Arc<[u8]>,
    digest: ContentDigest,
}

pub(crate) struct PreparedAuthoringSchemaBasis {
    attempt: AuthoringSchemaAttemptGeneration,
    project_root: DirectoryCapability,
    manifest: AdmittedAuthoringManifest,
    settings: EffectiveProjectSettings,
    bundle_path: ProjectPath,
    budget: AuthoringSchemaAdmissionBudget,
}

impl PreparedAuthoringSchemaBasis {
    pub(crate) fn admit_bundle(
        mut self,
    ) -> Result<AuthoringSchemaCandidate, AuthoringSchemaAdmissionError> {
        let bundle = admit_bundle(&self.project_root, self.bundle_path, &mut self.budget)?;
        Ok(AuthoringSchemaCandidate {
            attempt: self.attempt,
            project_root: self.project_root,
            manifest: self.manifest,
            settings: self.settings,
            bundle,
            usage: self.budget.usage(),
        })
    }
}

pub(crate) struct AuthoringSchemaCandidate {
    attempt: AuthoringSchemaAttemptGeneration,
    project_root: DirectoryCapability,
    manifest: AdmittedAuthoringManifest,
    settings: EffectiveProjectSettings,
    bundle: AdmittedAuthoringSchemaBundle,
    usage: AuthoringSchemaAdmissionUsage,
}

impl AuthoringSchemaCandidate {
    #[must_use]
    pub(crate) fn publish(self) -> AuthoringSchemaReceipt {
        AuthoringSchemaReceipt {
            attempt: self.attempt,
            project_root: self.project_root,
            manifest: self.manifest,
            settings: self.settings,
            bundle: self.bundle,
            usage: self.usage,
        }
    }
}

pub(crate) struct AuthoringSchemaReceipt {
    attempt: AuthoringSchemaAttemptGeneration,
    project_root: DirectoryCapability,
    manifest: AdmittedAuthoringManifest,
    settings: EffectiveProjectSettings,
    bundle: AdmittedAuthoringSchemaBundle,
    usage: AuthoringSchemaAdmissionUsage,
}

impl AuthoringSchemaReceipt {
    #[must_use]
    pub(crate) const fn attempt_generation(&self) -> AuthoringSchemaAttemptGeneration {
        self.attempt
    }

    #[must_use]
    pub(crate) fn project_root(&self) -> DirectoryCapability {
        self.project_root.share()
    }

    #[must_use]
    pub(crate) fn root_identity(&self) -> FileIdentity {
        self.project_root.identity()
    }

    #[must_use]
    pub(crate) const fn manifest_identity(&self) -> FileIdentity {
        self.manifest.capability.identity()
    }

    #[must_use]
    pub(crate) const fn manifest_parent_identity(&self) -> Option<FileIdentity> {
        self.manifest.capability.parent_identity()
    }

    #[must_use]
    pub(crate) fn manifest_bytes(&self) -> &[u8] {
        &self.manifest.bytes
    }

    #[must_use]
    pub(crate) const fn manifest_digest(&self) -> ContentDigest {
        self.manifest.digest
    }

    #[must_use]
    pub(crate) const fn settings(&self) -> &EffectiveProjectSettings {
        &self.settings
    }

    #[must_use]
    pub(crate) const fn bundle_path(&self) -> &ProjectPath {
        self.bundle.path()
    }

    #[must_use]
    pub(crate) const fn bundle_identity(&self) -> FileIdentity {
        self.bundle.capability().identity()
    }

    #[must_use]
    pub(crate) const fn bundle_parent_identity(&self) -> Option<FileIdentity> {
        self.bundle.capability().parent_identity()
    }

    #[must_use]
    pub(crate) fn bundle_bytes(&self) -> &[u8] {
        self.bundle.bytes()
    }

    #[must_use]
    pub(crate) const fn bundle_digest(&self) -> ContentDigest {
        self.bundle.digest()
    }

    #[must_use]
    pub(crate) fn bundle(&self) -> &AuthoringSchemaBundle {
        self.bundle.bundle()
    }

    #[must_use]
    pub(crate) fn composition_fingerprint(&self) -> SchemaCompositionFingerprint {
        self.bundle.fingerprint()
    }

    #[must_use]
    pub(crate) const fn usage(&self) -> AuthoringSchemaAdmissionUsage {
        self.usage
    }
}

#[derive(Debug)]
pub(crate) enum AuthoringSchemaAdmissionError {
    AttemptGenerationExhausted,
    ManifestPath(PathValidationError),
    ManifestHost(FsError),
    ManifestRootMismatch,
    ManifestTooLarge { maximum: usize },
    ManifestInvalid { diagnostics: DiagnosticReport },
    Profile(ProjectProfileError),
    MissingSchemaBundle,
    BundlePath(PathValidationError),
    BundleHost(FsError),
    BundleTooLarge { maximum: usize },
    BundleInvalid(AuthoringSchemaBundleError),
    AggregateEncodedBytesExceeded { maximum: usize },
}

impl Display for AuthoringSchemaAdmissionError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::AttemptGenerationExhausted => {
                formatter.write_str("authoring schema admission generation is exhausted")
            }
            Self::ManifestPath(_) => {
                formatter.write_str("project manifest path is invalid for host admission")
            }
            Self::ManifestHost(_) => formatter.write_str("project manifest host admission failed"),
            Self::ManifestRootMismatch => {
                formatter.write_str("project manifest handle is not bound to the project root")
            }
            Self::ManifestTooLarge { maximum } => write!(
                formatter,
                "project manifest exceeds its {maximum}-byte admission limit"
            ),
            Self::ManifestInvalid { .. } => formatter.write_str("project manifest is invalid"),
            Self::Profile(_) => formatter.write_str("project profile could not be resolved"),
            Self::MissingSchemaBundle => {
                formatter.write_str("project manifest does not declare an authoring schema bundle")
            }
            Self::BundlePath(_) => {
                formatter.write_str("authoring schema bundle path is invalid for host admission")
            }
            Self::BundleHost(_) => {
                formatter.write_str("authoring schema bundle host admission failed")
            }
            Self::BundleTooLarge { maximum } => write!(
                formatter,
                "authoring schema bundle exceeds its {maximum}-byte admission limit"
            ),
            Self::BundleInvalid(_) => formatter.write_str("authoring schema bundle is invalid"),
            Self::AggregateEncodedBytesExceeded { maximum } => write!(
                formatter,
                "authoring schema admission exceeds its aggregate {maximum}-byte limit"
            ),
        }
    }
}

impl Error for AuthoringSchemaAdmissionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::ManifestPath(error) | Self::BundlePath(error) => Some(error),
            Self::ManifestHost(error) | Self::BundleHost(error) => Some(error),
            Self::Profile(error) => Some(error),
            Self::BundleInvalid(error) => Some(error),
            Self::AttemptGenerationExhausted
            | Self::ManifestRootMismatch
            | Self::ManifestTooLarge { .. }
            | Self::ManifestInvalid { .. }
            | Self::MissingSchemaBundle
            | Self::BundleTooLarge { .. }
            | Self::AggregateEncodedBytesExceeded { .. } => None,
        }
    }
}

fn map_manifest_read_error(
    error: FsError,
    aggregate_limited: bool,
    read_limit: usize,
    aggregate_maximum: usize,
) -> AuthoringSchemaAdmissionError {
    match error {
        FsError::ByteLimitExceeded { .. } if aggregate_limited => {
            AuthoringSchemaAdmissionError::AggregateEncodedBytesExceeded {
                maximum: aggregate_maximum,
            }
        }
        FsError::ByteLimitExceeded { .. } => AuthoringSchemaAdmissionError::ManifestTooLarge {
            maximum: read_limit,
        },
        error => AuthoringSchemaAdmissionError::ManifestHost(error),
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs::{self, File},
        io,
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    #[cfg(windows)]
    use std::fs::OpenOptions;

    use nara_fs::{CapabilityRights, HostCapabilityOptions, TrustMode};
    use nara_reflect::{
        AuthoringSchemaBundleOwner, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaOwnerId, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponentSchemaOwnerId,
    };

    use super::*;

    static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
    const BUNDLE_PATH: &str = "schemas/authoring.bundle.json";
    const BUILTIN_OWNER: ComponentSchemaOwnerId =
        ComponentSchemaOwnerId::new("nara.ui.components.native");

    #[test]
    fn exact_manifest_and_bundle_handles_publish_one_schema_receipt() {
        let project = TestAuthoringProject::new(
            "exact-receipt",
            schema_bundle("nara.test.authoring", "nara.test.Transform"),
        );
        let project_root = project.capability();
        let expected_root_identity = project_root.identity();
        let expected_manifest = project_root
            .open_file(&RelativePath::new(PROJECT_MANIFEST_PATH).unwrap())
            .unwrap();
        let expected_bundle = project_root
            .open_file(&RelativePath::new(BUNDLE_PATH).unwrap())
            .unwrap();
        let manifest_bytes = fs::read(project.root.join(PROJECT_MANIFEST_PATH)).unwrap();
        let bundle_bytes = fs::read(project.root.join(BUNDLE_PATH)).unwrap();

        let mut host = AuthoringProjectAdmissionHost::default();
        let receipt = host
            .prepare(project_root, None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();

        assert_eq!(receipt.attempt_generation().get(), 1);
        assert_eq!(receipt.root_identity(), expected_root_identity);
        assert_eq!(receipt.project_root().identity(), expected_root_identity);
        assert_eq!(receipt.manifest_identity(), expected_manifest.identity());
        assert_eq!(
            receipt.manifest_parent_identity(),
            expected_manifest.parent_identity()
        );
        assert_eq!(receipt.bundle_identity(), expected_bundle.identity());
        assert_eq!(
            receipt.bundle_parent_identity(),
            expected_bundle.parent_identity()
        );
        assert_eq!(receipt.manifest_bytes(), manifest_bytes);
        assert_eq!(receipt.bundle_bytes(), bundle_bytes);
        assert_eq!(
            receipt.manifest_digest(),
            ContentDigest::of_bytes(&manifest_bytes)
        );
        assert_eq!(
            receipt.bundle_digest(),
            ContentDigest::of_bytes(&bundle_bytes)
        );
        assert_eq!(receipt.bundle_path().as_str(), BUNDLE_PATH);
        assert_eq!(
            receipt.composition_fingerprint(),
            receipt.bundle().fingerprint()
        );
        assert_eq!(
            receipt.usage().aggregate_encoded_bytes(),
            manifest_bytes.len() + bundle_bytes.len()
        );
        assert_eq!(
            receipt.usage().manifest_encoded_bytes(),
            manifest_bytes.len()
        );
        assert_eq!(receipt.usage().bundle_encoded_bytes(), bundle_bytes.len());
        assert_eq!(receipt.settings().project.name, "exact-receipt");

        let second = host
            .prepare(project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();
        assert_eq!(second.attempt_generation().get(), 2);
    }

    #[test]
    fn aggregate_manifest_and_bundle_budget_rejects_before_publication() {
        let project = TestAuthoringProject::new(
            "aggregate-budget",
            schema_bundle("nara.test.authoring", "nara.test.Transform"),
        );
        let manifest_bytes = fs::read(project.root.join(PROJECT_MANIFEST_PATH)).unwrap();
        let bundle_bytes = fs::read(project.root.join(BUNDLE_PATH)).unwrap();
        let aggregate_limit = manifest_bytes.len() + bundle_bytes.len() - 1;
        let limits = AuthoringSchemaAdmissionLimits::new(
            ByteLimit::new(aggregate_limit).expect("the test aggregate limit is non-zero"),
        );
        let mut host = AuthoringProjectAdmissionHost::new(limits);

        let prepared = host.prepare(project.capability(), None).unwrap();
        let error = match prepared.admit_bundle() {
            Ok(_) => panic!("aggregate budget rejection must not publish a candidate"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            AuthoringSchemaAdmissionError::AggregateEncodedBytesExceeded { maximum }
                if maximum == aggregate_limit
        ));
    }

    #[test]
    fn spoofed_builtin_owner_does_not_match_the_trusted_adapter_basis() {
        let spoofed = schema_bundle(BUILTIN_OWNER.as_str(), "nara.test.SpoofedUiNode");
        let project = TestAuthoringProject::new("spoofed-owner", spoofed);
        let trusted_catalog = component_catalog("nara.ui.UiNode");
        let mut host = AuthoringProjectAdmissionHost::default();
        let receipt = host
            .prepare(project.capability(), None)
            .unwrap()
            .admit_bundle()
            .unwrap()
            .publish();

        let admitted_owner = &receipt.bundle().owners()[0];
        assert_eq!(
            admitted_owner.owner_id().matches_executable(BUILTIN_OWNER),
            Ok(true)
        );
        assert_eq!(admitted_owner.current().generation(), 1);
        assert_ne!(
            admitted_owner.current().fingerprint(),
            trusted_catalog.fingerprint()
        );
    }

    fn schema_bundle(owner: &str, component: &str) -> AuthoringSchemaBundle {
        AuthoringSchemaBundle::new([AuthoringSchemaBundleOwner::new(
            PersistentComponentSchemaOwnerId::try_new(owner).unwrap(),
            component_catalog(component),
            None,
        )])
        .unwrap()
    }

    fn component_catalog(component: &str) -> ComponentSchemaCatalog {
        ComponentSchemaCatalog {
            components: vec![ComponentSchema::new(
                ComponentTypeId::new(component),
                component.rsplit('.').next().unwrap(),
                ComponentSchemaVersion::ONE,
            )],
            ..ComponentSchemaCatalog::default()
        }
    }

    struct TestAuthoringProject {
        root: PathBuf,
    }

    impl TestAuthoringProject {
        fn new(name: &str, bundle: AuthoringSchemaBundle) -> Self {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "nara_authoring_admission_{}_{}",
                std::process::id(),
                sequence
            ));
            fs::create_dir_all(root.join("schemas")).unwrap();
            fs::write(
                root.join(PROJECT_MANIFEST_PATH),
                format!(
                    r#"schema_version = 1

[project]
name = "{name}"

[authoring]
schema_bundle = "{BUNDLE_PATH}"
"#
                ),
            )
            .unwrap();
            fs::write(root.join(BUNDLE_PATH), bundle.to_json_string().unwrap()).unwrap();
            Self { root }
        }

        fn capability(&self) -> DirectoryCapability {
            DirectoryCapability::from_host_handle(
                host_directory(&self.root).unwrap(),
                HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
            )
            .unwrap()
        }
    }

    impl Drop for TestAuthoringProject {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.root).unwrap();
        }
    }

    #[cfg(windows)]
    fn host_directory(path: &Path) -> io::Result<File> {
        use std::os::windows::fs::OpenOptionsExt;

        const FILE_SHARE_READ_WRITE_DELETE: u32 = 0x1 | 0x2 | 0x4;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;

        OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ_WRITE_DELETE)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
    }

    #[cfg(not(windows))]
    fn host_directory(path: &Path) -> io::Result<File> {
        File::open(path)
    }
}
