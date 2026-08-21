//! Bounded, capability-free Cargo metadata and JSON-message facts.
//!
//! This Module deliberately owns no process, filesystem, or publication capability. Absolute paths
//! are retained only as transient invocation facts so the private owner can bind Cargo output before
//! copying the selected executable into its immutable generation.

use std::{
    collections::BTreeSet,
    mem,
    path::{Path, PathBuf},
};

use serde_json::{Map, Value};
use thiserror::Error;

const DESKTOP_TARGET: &str = "desktop";
const DESKTOP_FEATURE: &str = "desktop";
const DEV_PROFILE: &str = "dev";
const DEV_PROFILE_DIRECTORY: &str = "debug";

const MAX_METADATA_BYTES: usize = 16 * 1024 * 1024;
const MAX_PACKAGE_COUNT: usize = 8_192;
const MAX_TARGET_COUNT: usize = 512;
const MAX_FEATURE_COUNT: usize = 256;
const MAX_PACKAGE_ID_BYTES: usize = 4 * 1024;
const MAX_PACKAGE_NAME_BYTES: usize = 256;
const MAX_VERSION_BYTES: usize = 256;
const MAX_PATH_BYTES: usize = 128 * 1024;
const MAX_FEATURE_BYTES: usize = 256;
const MAX_HOST_TARGET_BYTES: usize = 256;

const MAX_JSON_STREAM_BYTES: usize = 64 * 1024 * 1024;
const MAX_JSON_RECORD_BYTES: usize = 4 * 1024 * 1024;
const MAX_JSON_RECORDS: usize = 131_072;
const MAX_DIAGNOSTIC_COUNT: usize = 256;
const MAX_DIAGNOSTIC_BYTES: usize = 256 * 1024;
const MAX_DIAGNOSTIC_MESSAGE_BYTES: usize = 16 * 1024;
const MAX_DIAGNOSTIC_CODE_BYTES: usize = 256;

/// Path-safe classification retained for one package in the bounded Cargo graph observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum CargoPackageSourceKind {
    Path,
    Registry,
    Git,
    Other,
}

/// One bounded, path-safe package observation from Cargo metadata.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CargoPackageGraphFact {
    name: String,
    version: String,
    source_kind: CargoPackageSourceKind,
}

impl CargoPackageGraphFact {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn version(&self) -> &str {
        &self.version
    }

    pub const fn source_kind(&self) -> CargoPackageSourceKind {
        self.source_kind
    }
}

/// Bounded Cargo metadata facts admitted for the closed supervised desktop build convention.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoMetadataFacts {
    root_package_id: String,
    root_package: CargoPackageGraphFact,
    package_graph: Box<[CargoPackageGraphFact]>,
    manifest_path: PathBuf,
    target_directory: PathBuf,
    desktop_source_path: PathBuf,
}

impl CargoMetadataFacts {
    pub fn parse(
        metadata: &[u8],
        expected_manifest_path: &Path,
    ) -> Result<Self, CargoMetadataFactsError> {
        if metadata.len() > MAX_METADATA_BYTES {
            return Err(CargoMetadataFactsError::InputLimitExceeded);
        }
        validate_absolute_metadata_path(expected_manifest_path, "expected_manifest_path")?;

        let value = serde_json::from_slice::<Value>(metadata)
            .map_err(CargoMetadataFactsError::MalformedJson)?;
        let object = value
            .as_object()
            .ok_or(CargoMetadataFactsError::InvalidField { field: "metadata" })?;
        if u64_field(object.get("version"), "version")? != 1 {
            return Err(CargoMetadataFactsError::UnsupportedFormatVersion);
        }

        let workspace_root = metadata_path_field(object.get("workspace_root"), "workspace_root")?;
        let expected_workspace_root =
            expected_manifest_path
                .parent()
                .ok_or(CargoMetadataFactsError::InvalidField {
                    field: "expected_manifest_path",
                })?;
        if !paths_match(&workspace_root, expected_workspace_root) {
            return Err(CargoMetadataFactsError::WorkspaceRootMismatch);
        }

        let target_directory =
            metadata_path_field(object.get("target_directory"), "target_directory")?;
        let resolved_root_package_id = optional_resolved_root_package_id(object.get("resolve"))?;

        let workspace_members = metadata_string_array(
            object.get("workspace_members"),
            "workspace_members",
            MAX_PACKAGE_COUNT,
            MAX_PACKAGE_ID_BYTES,
        )?;
        let packages = object
            .get("packages")
            .and_then(Value::as_array)
            .ok_or(CargoMetadataFactsError::InvalidField { field: "packages" })?;
        if packages.len() > MAX_PACKAGE_COUNT {
            return Err(CargoMetadataFactsError::PackageLimitExceeded);
        }

        let mut package_ids = BTreeSet::new();
        let mut package_graph = Vec::with_capacity(packages.len());
        let mut root_package_id = None;
        let mut root_package = None;
        let mut desktop_source_path = None;

        for package in packages {
            let package = package
                .as_object()
                .ok_or(CargoMetadataFactsError::InvalidField {
                    field: "packages[]",
                })?;
            let package_id =
                metadata_string_field(package.get("id"), "packages[].id", MAX_PACKAGE_ID_BYTES)?;
            if !package_ids.insert(package_id.to_owned()) {
                return Err(CargoMetadataFactsError::DuplicatePackageId);
            }
            let fact = CargoPackageGraphFact {
                name: metadata_string_field(
                    package.get("name"),
                    "packages[].name",
                    MAX_PACKAGE_NAME_BYTES,
                )?
                .to_owned(),
                version: metadata_string_field(
                    package.get("version"),
                    "packages[].version",
                    MAX_VERSION_BYTES,
                )?
                .to_owned(),
                source_kind: package_source_kind(package.get("source"))?,
            };
            let manifest_path =
                metadata_path_field(package.get("manifest_path"), "packages[].manifest_path")?;
            if paths_match(&manifest_path, expected_manifest_path) {
                if root_package.replace(fact.clone()).is_some() {
                    return Err(CargoMetadataFactsError::DuplicateRootPackage);
                }
                root_package_id = Some(package_id.to_owned());
                require_desktop_feature(package)?;
                desktop_source_path = Some(select_desktop_target(package)?);
            }
            package_graph.push(fact);
        }

        let root_package_id = root_package_id.ok_or(CargoMetadataFactsError::MissingRootPackage)?;
        if resolved_root_package_id
            .as_deref()
            .is_some_and(|resolved| resolved != root_package_id.as_str())
        {
            return Err(CargoMetadataFactsError::RootResolutionMismatch);
        }
        if !workspace_members
            .iter()
            .any(|member| member == &root_package_id)
        {
            return Err(CargoMetadataFactsError::RootPackageNotWorkspaceMember);
        }
        let root_package = root_package.ok_or(CargoMetadataFactsError::MissingRootPackage)?;
        let desktop_source_path =
            desktop_source_path.ok_or(CargoMetadataFactsError::MissingDesktopTarget)?;
        package_graph.sort_unstable();

        Ok(Self {
            root_package_id,
            root_package,
            package_graph: package_graph.into_boxed_slice(),
            manifest_path: expected_manifest_path.to_path_buf(),
            target_directory,
            desktop_source_path,
        })
    }

    pub fn root_package(&self) -> &CargoPackageGraphFact {
        &self.root_package
    }

    pub fn package_name(&self) -> &str {
        self.root_package.name()
    }

    pub fn package_version(&self) -> &str {
        self.root_package.version()
    }

    pub fn package_graph(&self) -> &[CargoPackageGraphFact] {
        &self.package_graph
    }

    pub fn desktop_invocation(
        &self,
        host_target: impl Into<String>,
    ) -> Result<CargoDesktopBuildInvocation, CargoDesktopInvocationError> {
        CargoDesktopBuildInvocation::new(self, host_target.into())
    }
}

#[derive(Debug, Error)]
pub enum CargoMetadataFactsError {
    #[error("Cargo metadata exceeded its bounded input size")]
    InputLimitExceeded,
    #[error("Cargo metadata emitted malformed JSON")]
    MalformedJson(#[source] serde_json::Error),
    #[error("Cargo metadata field `{field}` was missing or invalid")]
    InvalidField { field: &'static str },
    #[error("Cargo metadata field `{field}` exceeded its bounded size")]
    FieldLimitExceeded { field: &'static str },
    #[error("Cargo metadata version is not supported")]
    UnsupportedFormatVersion,
    #[error("Cargo metadata contained too many packages")]
    PackageLimitExceeded,
    #[error("Cargo metadata repeated a package identity")]
    DuplicatePackageId,
    #[error("Cargo metadata did not identify one root package")]
    MissingRootPackage,
    #[error("Cargo metadata repeated the package that owns the selected manifest")]
    DuplicateRootPackage,
    #[error("the resolved Cargo root package is not a workspace member")]
    RootPackageNotWorkspaceMember,
    #[error("the Cargo workspace root does not match the selected project manifest")]
    WorkspaceRootMismatch,
    #[error("Cargo resolve.root does not match the package that owns the selected manifest")]
    RootResolutionMismatch,
    #[error("the Cargo root package does not declare the `desktop` feature")]
    MissingDesktopFeature,
    #[error("the Cargo root package did not define the exact `desktop` binary target")]
    MissingDesktopTarget,
    #[error("the Cargo root package defined the `desktop` target more than once")]
    DuplicateDesktopTarget,
    #[error("the Cargo `desktop` target field `{field}` did not match the closed convention")]
    DesktopTargetMismatch { field: &'static str },
    #[error("Cargo metadata path field `{field}` was not absolute")]
    NonAbsolutePath { field: &'static str },
}

/// Exact transient invocation facts for the closed desktop/dev/host Cargo build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoDesktopBuildInvocation {
    package_id: String,
    root_package: CargoPackageGraphFact,
    manifest_path: PathBuf,
    target_directory: PathBuf,
    desktop_source_path: PathBuf,
    host_target: String,
    expected_executable: PathBuf,
}

impl CargoDesktopBuildInvocation {
    fn new(
        metadata: &CargoMetadataFacts,
        host_target: String,
    ) -> Result<Self, CargoDesktopInvocationError> {
        validate_host_target(&host_target)?;
        let executable_name = if host_target.contains("-windows-") {
            format!("{DESKTOP_TARGET}.exe")
        } else {
            DESKTOP_TARGET.to_owned()
        };
        let expected_executable = metadata
            .target_directory
            .join(&host_target)
            .join(DEV_PROFILE_DIRECTORY)
            .join(executable_name);
        if expected_executable.to_string_lossy().len() > MAX_PATH_BYTES {
            return Err(CargoDesktopInvocationError::ExecutablePathLimitExceeded);
        }
        Ok(Self {
            package_id: metadata.root_package_id.clone(),
            root_package: metadata.root_package.clone(),
            manifest_path: metadata.manifest_path.clone(),
            target_directory: metadata.target_directory.clone(),
            desktop_source_path: metadata.desktop_source_path.clone(),
            host_target,
            expected_executable,
        })
    }

    pub fn root_package(&self) -> &CargoPackageGraphFact {
        &self.root_package
    }

    pub fn package_name(&self) -> &str {
        self.root_package.name()
    }

    pub fn package_version(&self) -> &str {
        self.root_package.version()
    }

    /// Returns the operational manifest path used for this invocation.
    ///
    /// The caller must not persist this absolute path as project identity.
    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    /// Returns Cargo's operational target directory for this invocation.
    ///
    /// The caller must not persist this absolute path as project identity.
    pub fn target_directory(&self) -> &Path {
        &self.target_directory
    }

    pub fn host_target(&self) -> &str {
        &self.host_target
    }

    pub const fn target_name(&self) -> &'static str {
        DESKTOP_TARGET
    }

    pub const fn profile(&self) -> &'static str {
        DEV_PROFILE
    }

    pub const fn features(&self) -> [&'static str; 1] {
        [DESKTOP_FEATURE]
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CargoDesktopInvocationError {
    #[error("the Cargo host target was not one bounded ASCII target triple")]
    InvalidHostTarget,
    #[error("the expected Cargo executable path exceeded its bounded size")]
    ExecutablePathLimitExceeded,
}

/// One normalized compiler-artifact message with no process or filesystem capability.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoCompilerArtifactFacts {
    package_id: String,
    manifest_path: PathBuf,
    target: CargoTargetFacts,
    features: Box<[String]>,
    profile_test: bool,
    executable: Option<PathBuf>,
    fresh: bool,
}

impl CargoCompilerArtifactFacts {
    pub fn package_id(&self) -> &str {
        &self.package_id
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    pub fn target_name(&self) -> &str {
        &self.target.name
    }

    pub fn target_kind(&self) -> &[String] {
        &self.target.kind
    }

    pub fn target_crate_types(&self) -> &[String] {
        &self.target.crate_types
    }

    pub fn target_source_path(&self) -> &Path {
        &self.target.source_path
    }

    pub fn target_required_features(&self) -> Option<&[String]> {
        self.target.required_features.as_deref()
    }

    pub fn features(&self) -> &[String] {
        &self.features
    }

    pub const fn profile_test(&self) -> bool {
        self.profile_test
    }

    pub fn executable(&self) -> Option<&Path> {
        self.executable.as_deref()
    }

    pub const fn fresh(&self) -> bool {
        self.fresh
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CargoTargetFacts {
    name: String,
    kind: Box<[String]>,
    crate_types: Box<[String]>,
    source_path: PathBuf,
    required_features: Option<Box<[String]>>,
}

/// Bounded compiler diagnostic retained separately from raw Cargo output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoCompilerDiagnostic {
    level: CargoDiagnosticLevel,
    code: Option<String>,
    message: String,
    truncated: bool,
}

impl CargoCompilerDiagnostic {
    pub const fn level(&self) -> CargoDiagnosticLevel {
        self.level
    }

    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub const fn truncated(&self) -> bool {
        self.truncated
    }

    fn retained_bytes(&self) -> usize {
        self.message
            .len()
            .saturating_add(self.code.as_ref().map_or(0, String::len))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CargoDiagnosticLevel {
    Error,
    Warning,
    Note,
    Help,
    FailureNote,
    Other,
}

/// One normalized Cargo JSON record used by owner-specific selectors.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CargoJsonMessage {
    CompilerArtifact(CargoCompilerArtifactFacts),
    CompilerDiagnostic(CargoCompilerDiagnostic),
    BuildScriptExecuted,
    BuildFinished { success: bool },
}

impl CargoJsonMessage {
    pub fn parse(value: Value) -> Result<Self, CargoJsonMessageError> {
        let object = value
            .as_object()
            .ok_or(CargoJsonMessageError::InvalidField { field: "message" })?;
        match message_string_field(object.get("reason"), "reason", 64)? {
            "compiler-artifact" => parse_compiler_artifact(object).map(Self::CompilerArtifact),
            "compiler-message" => parse_compiler_diagnostic(object).map(Self::CompilerDiagnostic),
            "build-script-executed" => Ok(Self::BuildScriptExecuted),
            "build-finished" => Ok(Self::BuildFinished {
                success: message_bool_field(object.get("success"), "success")?,
            }),
            _ => Err(CargoJsonMessageError::UnknownReason),
        }
    }

    fn parse_slice(record: &[u8]) -> Result<Self, CargoJsonMessageError> {
        let value = serde_json::from_slice(record).map_err(CargoJsonMessageError::MalformedJson)?;
        Self::parse(value)
    }
}

#[derive(Debug, Error)]
pub enum CargoJsonMessageError {
    #[error("Cargo emitted malformed JSON")]
    MalformedJson(#[source] serde_json::Error),
    #[error("Cargo JSON field `{field}` was missing or invalid")]
    InvalidField { field: &'static str },
    #[error("Cargo JSON field `{field}` exceeded its bounded size")]
    FieldLimitExceeded { field: &'static str },
    #[error("Cargo emitted an unsupported JSON message reason")]
    UnknownReason,
}

/// Bounded structured diagnostics observed while decoding Cargo JSON messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoDiagnostics {
    entries: Box<[CargoCompilerDiagnostic]>,
    dropped_count: u64,
    truncated_count: u64,
}

impl CargoDiagnostics {
    pub fn entries(&self) -> &[CargoCompilerDiagnostic] {
        &self.entries
    }

    pub const fn dropped_count(&self) -> u64 {
        self.dropped_count
    }

    pub const fn truncated_count(&self) -> u64 {
        self.truncated_count
    }
}

#[derive(Debug, Default)]
struct CargoDiagnosticsBuilder {
    entries: Vec<CargoCompilerDiagnostic>,
    retained_bytes: usize,
    dropped_count: u64,
    truncated_count: u64,
}

impl CargoDiagnosticsBuilder {
    fn push(&mut self, diagnostic: CargoCompilerDiagnostic) {
        if diagnostic.truncated {
            self.truncated_count = self.truncated_count.saturating_add(1);
        }
        let retained_bytes = diagnostic.retained_bytes();
        let Some(next_bytes) = self.retained_bytes.checked_add(retained_bytes) else {
            self.dropped_count = self.dropped_count.saturating_add(1);
            return;
        };
        if self.entries.len() >= MAX_DIAGNOSTIC_COUNT || next_bytes > MAX_DIAGNOSTIC_BYTES {
            self.dropped_count = self.dropped_count.saturating_add(1);
            return;
        }
        self.retained_bytes = next_bytes;
        self.entries.push(diagnostic);
    }

    fn snapshot(&self) -> CargoDiagnostics {
        CargoDiagnostics {
            entries: self.entries.clone().into_boxed_slice(),
            dropped_count: self.dropped_count,
            truncated_count: self.truncated_count,
        }
    }
}

/// Exact field rejected while binding a root-package desktop artifact to its invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CargoArtifactMismatchField {
    Manifest,
    TargetKind,
    CrateTypes,
    SourcePath,
    RequiredFeatures,
    Features,
    Profile,
    ExecutablePath,
}

/// Incremental bounded decoder and exact selector for one supervised desktop Cargo build.
#[derive(Debug)]
pub struct CargoDesktopBuildDecoder {
    invocation: CargoDesktopBuildInvocation,
    pending_record: Vec<u8>,
    total_bytes: usize,
    record_count: usize,
    selected: Option<CargoDesktopArtifactFacts>,
    build_finished: Option<bool>,
    diagnostics: CargoDiagnosticsBuilder,
    finalized: bool,
}

impl CargoDesktopBuildDecoder {
    pub fn new(invocation: CargoDesktopBuildInvocation) -> Self {
        Self {
            invocation,
            pending_record: Vec::new(),
            total_bytes: 0,
            record_count: 0,
            selected: None,
            build_finished: None,
            diagnostics: CargoDiagnosticsBuilder::default(),
            finalized: false,
        }
    }

    pub fn push(&mut self, bytes: &[u8]) -> Result<(), CargoDesktopBuildError> {
        if self.finalized {
            return Err(CargoDesktopBuildError::AlreadyFinished);
        }
        self.total_bytes = self
            .total_bytes
            .checked_add(bytes.len())
            .filter(|bytes| *bytes <= MAX_JSON_STREAM_BYTES)
            .ok_or(CargoDesktopBuildError::InputLimitExceeded)?;

        let mut start = 0;
        for (index, byte) in bytes.iter().enumerate() {
            if *byte != b'\n' {
                continue;
            }
            self.extend_pending(&bytes[start..index])?;
            self.observe_pending_record()?;
            start = index.saturating_add(1);
        }
        self.extend_pending(&bytes[start..])
    }

    pub fn diagnostics(&self) -> CargoDiagnostics {
        self.diagnostics.snapshot()
    }

    pub fn finish(&mut self) -> Result<CargoDesktopBuildFacts, CargoDesktopBuildError> {
        if self.finalized {
            return Err(CargoDesktopBuildError::AlreadyFinished);
        }
        if !self.pending_record.is_empty() {
            self.observe_pending_record()?;
        }
        self.finalized = true;

        match self.build_finished {
            None => return Err(CargoDesktopBuildError::MissingBuildFinished),
            Some(false) => return Err(CargoDesktopBuildError::BuildFailed),
            Some(true) => {}
        }
        let artifact = self
            .selected
            .clone()
            .ok_or(CargoDesktopBuildError::MissingArtifact)?;
        Ok(CargoDesktopBuildFacts {
            artifact,
            diagnostics: self.diagnostics.snapshot(),
        })
    }

    fn extend_pending(&mut self, bytes: &[u8]) -> Result<(), CargoDesktopBuildError> {
        if self.pending_record.len().saturating_add(bytes.len()) > MAX_JSON_RECORD_BYTES {
            return Err(CargoDesktopBuildError::RecordLimitExceeded);
        }
        self.pending_record.extend_from_slice(bytes);
        Ok(())
    }

    fn observe_pending_record(&mut self) -> Result<(), CargoDesktopBuildError> {
        while self.pending_record.last() == Some(&b'\r') {
            self.pending_record.pop();
        }
        if self.pending_record.iter().all(u8::is_ascii_whitespace) {
            self.pending_record.clear();
            return Ok(());
        }
        self.record_count = self
            .record_count
            .checked_add(1)
            .filter(|count| *count <= MAX_JSON_RECORDS)
            .ok_or(CargoDesktopBuildError::RecordCountExceeded)?;
        let record = mem::take(&mut self.pending_record);
        let message = CargoJsonMessage::parse_slice(&record)?;
        self.observe(message)
    }

    fn observe(&mut self, message: CargoJsonMessage) -> Result<(), CargoDesktopBuildError> {
        if matches!(message, CargoJsonMessage::BuildFinished { .. }) {
            if self.build_finished.is_some() {
                return Err(CargoDesktopBuildError::DuplicateBuildFinished);
            }
        } else if self.build_finished.is_some() {
            return Err(CargoDesktopBuildError::MessageAfterBuildFinished);
        }

        match message {
            CargoJsonMessage::CompilerArtifact(artifact) => self.observe_artifact(artifact),
            CargoJsonMessage::CompilerDiagnostic(diagnostic) => {
                self.diagnostics.push(diagnostic);
                Ok(())
            }
            CargoJsonMessage::BuildScriptExecuted => Ok(()),
            CargoJsonMessage::BuildFinished { success } => {
                self.build_finished = Some(success);
                Ok(())
            }
        }
    }

    fn observe_artifact(
        &mut self,
        artifact: CargoCompilerArtifactFacts,
    ) -> Result<(), CargoDesktopBuildError> {
        if artifact.package_id != self.invocation.package_id {
            return Ok(());
        }
        if !paths_match(&artifact.manifest_path, &self.invocation.manifest_path) {
            return Err(CargoDesktopBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::Manifest,
            });
        }
        if artifact.target.name != DESKTOP_TARGET {
            return Ok(());
        }
        if artifact.target.kind.as_ref() != ["bin"] {
            return Err(CargoDesktopBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::TargetKind,
            });
        }
        if artifact.target.crate_types.as_ref() != ["bin"] {
            return Err(CargoDesktopBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::CrateTypes,
            });
        }
        if !paths_match(
            &artifact.target.source_path,
            &self.invocation.desktop_source_path,
        ) {
            return Err(CargoDesktopBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::SourcePath,
            });
        }
        if artifact
            .target
            .required_features
            .as_deref()
            .is_some_and(|features| features != [DESKTOP_FEATURE])
        {
            return Err(CargoDesktopBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::RequiredFeatures,
            });
        }
        if artifact.features.as_ref() != [DESKTOP_FEATURE] {
            return Err(CargoDesktopBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::Features,
            });
        }
        if artifact.profile_test {
            return Err(CargoDesktopBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::Profile,
            });
        }
        let executable = artifact
            .executable
            .ok_or(CargoDesktopBuildError::NonExecutableArtifact)?;
        if !executable.is_absolute() {
            return Err(CargoDesktopBuildError::NonAbsoluteExecutable);
        }
        if !paths_match(&executable, &self.invocation.expected_executable) {
            return Err(CargoDesktopBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::ExecutablePath,
            });
        }
        let selected = CargoDesktopArtifactFacts {
            root_package: self.invocation.root_package.clone(),
            host_target: self.invocation.host_target.clone(),
            executable,
            fresh: artifact.fresh,
        };
        if self.selected.replace(selected).is_some() {
            return Err(CargoDesktopBuildError::DuplicateArtifact);
        }
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CargoDesktopBuildError {
    #[error("the Cargo JSON stream exceeded its bounded size")]
    InputLimitExceeded,
    #[error("one Cargo JSON record exceeded its bounded size")]
    RecordLimitExceeded,
    #[error("Cargo emitted too many JSON records")]
    RecordCountExceeded,
    #[error(transparent)]
    Message(#[from] CargoJsonMessageError),
    #[error("Cargo emitted a message after build-finished")]
    MessageAfterBuildFinished,
    #[error("Cargo emitted build-finished more than once")]
    DuplicateBuildFinished,
    #[error("Cargo did not emit build-finished")]
    MissingBuildFinished,
    #[error("Cargo reported an unsuccessful build")]
    BuildFailed,
    #[error("Cargo did not emit the exact desktop executable artifact")]
    MissingArtifact,
    #[error("Cargo emitted the exact desktop artifact more than once")]
    DuplicateArtifact,
    #[error("Cargo emitted a non-executable desktop artifact")]
    NonExecutableArtifact,
    #[error("Cargo emitted a non-absolute desktop executable path")]
    NonAbsoluteExecutable,
    #[error("Cargo artifact field {field:?} did not match the bound invocation")]
    ArtifactMismatch { field: CargoArtifactMismatchField },
    #[error("the Cargo JSON decoder was already finalized")]
    AlreadyFinished,
}

/// Successful exact desktop artifact selection plus bounded structured diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoDesktopBuildFacts {
    artifact: CargoDesktopArtifactFacts,
    diagnostics: CargoDiagnostics,
}

impl CargoDesktopBuildFacts {
    pub fn artifact(&self) -> &CargoDesktopArtifactFacts {
        &self.artifact
    }

    pub fn diagnostics(&self) -> &CargoDiagnostics {
        &self.diagnostics
    }
}

/// One exact executable artifact selected for transient immutable-generation publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoDesktopArtifactFacts {
    root_package: CargoPackageGraphFact,
    host_target: String,
    executable: PathBuf,
    fresh: bool,
}

impl CargoDesktopArtifactFacts {
    pub fn root_package(&self) -> &CargoPackageGraphFact {
        &self.root_package
    }

    pub fn host_target(&self) -> &str {
        &self.host_target
    }

    pub const fn target_name(&self) -> &'static str {
        DESKTOP_TARGET
    }

    pub const fn profile(&self) -> &'static str {
        DEV_PROFILE
    }

    pub const fn features(&self) -> [&'static str; 1] {
        [DESKTOP_FEATURE]
    }

    /// Returns the transient Cargo output selected for immutable publication.
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub const fn fresh(&self) -> bool {
        self.fresh
    }
}

fn require_desktop_feature(package: &Map<String, Value>) -> Result<(), CargoMetadataFactsError> {
    let features = package.get("features").and_then(Value::as_object).ok_or(
        CargoMetadataFactsError::InvalidField {
            field: "packages[].features",
        },
    )?;
    if features.len() > MAX_FEATURE_COUNT {
        return Err(CargoMetadataFactsError::FieldLimitExceeded {
            field: "packages[].features",
        });
    }
    if !features.contains_key(DESKTOP_FEATURE) {
        return Err(CargoMetadataFactsError::MissingDesktopFeature);
    }
    Ok(())
}

fn select_desktop_target(package: &Map<String, Value>) -> Result<PathBuf, CargoMetadataFactsError> {
    let targets = package.get("targets").and_then(Value::as_array).ok_or(
        CargoMetadataFactsError::InvalidField {
            field: "packages[].targets",
        },
    )?;
    if targets.len() > MAX_TARGET_COUNT {
        return Err(CargoMetadataFactsError::FieldLimitExceeded {
            field: "packages[].targets",
        });
    }

    let mut selected = None;
    for target in targets {
        let target = target
            .as_object()
            .ok_or(CargoMetadataFactsError::InvalidField {
                field: "packages[].targets[]",
            })?;
        let name = metadata_string_field(
            target.get("name"),
            "packages[].targets[].name",
            MAX_PACKAGE_NAME_BYTES,
        )?;
        if name != DESKTOP_TARGET {
            continue;
        }
        if selected.is_some() {
            return Err(CargoMetadataFactsError::DuplicateDesktopTarget);
        }
        if metadata_string_array(
            target.get("kind"),
            "packages[].targets[].kind",
            MAX_FEATURE_COUNT,
            MAX_FEATURE_BYTES,
        )?
        .as_slice()
            != ["bin"]
        {
            return Err(CargoMetadataFactsError::DesktopTargetMismatch { field: "kind" });
        }
        if metadata_string_array(
            target.get("crate_types"),
            "packages[].targets[].crate_types",
            MAX_FEATURE_COUNT,
            MAX_FEATURE_BYTES,
        )?
        .as_slice()
            != ["bin"]
        {
            return Err(CargoMetadataFactsError::DesktopTargetMismatch {
                field: "crate_types",
            });
        }
        let required_features = normalized_metadata_features(
            target.get("required-features"),
            "packages[].targets[].required-features",
        )?;
        if required_features.as_ref() != [DESKTOP_FEATURE] {
            return Err(CargoMetadataFactsError::DesktopTargetMismatch {
                field: "required-features",
            });
        }
        selected = Some(metadata_path_field(
            target.get("src_path"),
            "packages[].targets[].src_path",
        )?);
    }
    selected.ok_or(CargoMetadataFactsError::MissingDesktopTarget)
}

fn optional_resolved_root_package_id(
    value: Option<&Value>,
) -> Result<Option<String>, CargoMetadataFactsError> {
    let Some(value) = value else {
        return Ok(None);
    };
    let Value::Object(resolve) = value else {
        if value.is_null() {
            return Ok(None);
        }
        return Err(CargoMetadataFactsError::InvalidField { field: "resolve" });
    };
    match resolve.get("root") {
        Some(Value::Null) | None => Ok(None),
        Some(root) => metadata_string_field(Some(root), "resolve.root", MAX_PACKAGE_ID_BYTES)
            .map(|root| Some(root.to_owned())),
    }
}

fn package_source_kind(
    value: Option<&Value>,
) -> Result<CargoPackageSourceKind, CargoMetadataFactsError> {
    match value {
        Some(Value::Null) | None => Ok(CargoPackageSourceKind::Path),
        Some(Value::String(source)) => {
            if source.len() > MAX_PATH_BYTES {
                return Err(CargoMetadataFactsError::FieldLimitExceeded {
                    field: "packages[].source",
                });
            }
            Ok(if source.starts_with("registry+") {
                CargoPackageSourceKind::Registry
            } else if source.starts_with("git+") {
                CargoPackageSourceKind::Git
            } else if source.starts_with("path+") {
                CargoPackageSourceKind::Path
            } else {
                CargoPackageSourceKind::Other
            })
        }
        Some(_) => Err(CargoMetadataFactsError::InvalidField {
            field: "packages[].source",
        }),
    }
}

fn parse_compiler_artifact(
    object: &Map<String, Value>,
) -> Result<CargoCompilerArtifactFacts, CargoJsonMessageError> {
    let target = object
        .get("target")
        .and_then(Value::as_object)
        .ok_or(CargoJsonMessageError::InvalidField { field: "target" })?;
    let executable = match object.get("executable") {
        Some(Value::Null) => None,
        Some(Value::String(path)) => {
            validate_message_text(path, "executable", MAX_PATH_BYTES)?;
            Some(PathBuf::from(path))
        }
        _ => {
            return Err(CargoJsonMessageError::InvalidField {
                field: "executable",
            });
        }
    };
    Ok(CargoCompilerArtifactFacts {
        package_id: message_string_field(
            object.get("package_id"),
            "package_id",
            MAX_PACKAGE_ID_BYTES,
        )?
        .to_owned(),
        manifest_path: message_path_field(object.get("manifest_path"), "manifest_path")?,
        target: CargoTargetFacts {
            name: message_string_field(target.get("name"), "target.name", MAX_PACKAGE_NAME_BYTES)?
                .to_owned(),
            kind: normalized_message_array(target.get("kind"), "target.kind")?,
            crate_types: normalized_message_array(target.get("crate_types"), "target.crate_types")?,
            source_path: message_path_field(target.get("src_path"), "target.src_path")?,
            required_features: target
                .get("required-features")
                .map(|value| normalized_message_array(Some(value), "target.required-features"))
                .transpose()?,
        },
        features: normalized_message_array(object.get("features"), "features")?,
        profile_test: object
            .get("profile")
            .and_then(Value::as_object)
            .ok_or(CargoJsonMessageError::InvalidField { field: "profile" })
            .and_then(|profile| message_bool_field(profile.get("test"), "profile.test"))?,
        executable,
        fresh: message_bool_field(object.get("fresh"), "fresh")?,
    })
}

fn parse_compiler_diagnostic(
    object: &Map<String, Value>,
) -> Result<CargoCompilerDiagnostic, CargoJsonMessageError> {
    let message = object
        .get("message")
        .and_then(Value::as_object)
        .ok_or(CargoJsonMessageError::InvalidField { field: "message" })?;
    let level = match message_string_field(message.get("level"), "message.level", 64)? {
        "error" => CargoDiagnosticLevel::Error,
        "warning" => CargoDiagnosticLevel::Warning,
        "note" => CargoDiagnosticLevel::Note,
        "help" => CargoDiagnosticLevel::Help,
        "failure-note" => CargoDiagnosticLevel::FailureNote,
        _ => CargoDiagnosticLevel::Other,
    };
    let (message_text, message_truncated) = truncate_utf8(
        message_string_field_unbounded(message.get("message"), "message.message")?,
        MAX_DIAGNOSTIC_MESSAGE_BYTES,
    );
    let (code, code_truncated) = match message.get("code") {
        Some(Value::Null) | None => (None, false),
        Some(Value::Object(code)) => {
            let (code, truncated) = truncate_utf8(
                message_string_field_unbounded(code.get("code"), "message.code.code")?,
                MAX_DIAGNOSTIC_CODE_BYTES,
            );
            (Some(code), truncated)
        }
        Some(_) => {
            return Err(CargoJsonMessageError::InvalidField {
                field: "message.code",
            });
        }
    };
    Ok(CargoCompilerDiagnostic {
        level,
        code,
        message: message_text,
        truncated: message_truncated || code_truncated,
    })
}

fn validate_host_target(host_target: &str) -> Result<(), CargoDesktopInvocationError> {
    let valid = !host_target.is_empty()
        && host_target.len() <= MAX_HOST_TARGET_BYTES
        && host_target.is_ascii()
        && host_target
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'));
    if valid {
        Ok(())
    } else {
        Err(CargoDesktopInvocationError::InvalidHostTarget)
    }
}

fn validate_absolute_metadata_path(
    path: &Path,
    field: &'static str,
) -> Result<(), CargoMetadataFactsError> {
    if path.to_string_lossy().len() > MAX_PATH_BYTES {
        return Err(CargoMetadataFactsError::FieldLimitExceeded { field });
    }
    if !path.is_absolute() {
        return Err(CargoMetadataFactsError::NonAbsolutePath { field });
    }
    Ok(())
}

fn metadata_path_field(
    value: Option<&Value>,
    field: &'static str,
) -> Result<PathBuf, CargoMetadataFactsError> {
    let path = PathBuf::from(metadata_string_field(value, field, MAX_PATH_BYTES)?);
    validate_absolute_metadata_path(&path, field)?;
    Ok(path)
}

fn message_path_field(
    value: Option<&Value>,
    field: &'static str,
) -> Result<PathBuf, CargoJsonMessageError> {
    Ok(PathBuf::from(message_string_field(
        value,
        field,
        MAX_PATH_BYTES,
    )?))
}

fn metadata_string_field<'a>(
    value: Option<&'a Value>,
    field: &'static str,
    max_bytes: usize,
) -> Result<&'a str, CargoMetadataFactsError> {
    let value = value
        .and_then(Value::as_str)
        .ok_or(CargoMetadataFactsError::InvalidField { field })?;
    if value.is_empty() {
        return Err(CargoMetadataFactsError::InvalidField { field });
    }
    if value.len() > max_bytes {
        return Err(CargoMetadataFactsError::FieldLimitExceeded { field });
    }
    Ok(value)
}

fn metadata_string_array(
    value: Option<&Value>,
    field: &'static str,
    max_items: usize,
    max_item_bytes: usize,
) -> Result<Vec<String>, CargoMetadataFactsError> {
    let values = value
        .and_then(Value::as_array)
        .ok_or(CargoMetadataFactsError::InvalidField { field })?;
    if values.len() > max_items {
        return Err(CargoMetadataFactsError::FieldLimitExceeded { field });
    }
    values
        .iter()
        .map(|value| metadata_string_field(Some(value), field, max_item_bytes).map(str::to_owned))
        .collect()
}

fn normalized_metadata_features(
    value: Option<&Value>,
    field: &'static str,
) -> Result<Box<[String]>, CargoMetadataFactsError> {
    let mut features = metadata_string_array(value, field, MAX_FEATURE_COUNT, MAX_FEATURE_BYTES)?;
    features.sort_unstable();
    if features.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(CargoMetadataFactsError::InvalidField { field });
    }
    Ok(features.into_boxed_slice())
}

fn message_string_field<'a>(
    value: Option<&'a Value>,
    field: &'static str,
    max_bytes: usize,
) -> Result<&'a str, CargoJsonMessageError> {
    let value = message_string_field_unbounded(value, field)?;
    validate_message_text(value, field, max_bytes)?;
    Ok(value)
}

fn message_string_field_unbounded<'a>(
    value: Option<&'a Value>,
    field: &'static str,
) -> Result<&'a str, CargoJsonMessageError> {
    value
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or(CargoJsonMessageError::InvalidField { field })
}

fn validate_message_text(
    value: &str,
    field: &'static str,
    max_bytes: usize,
) -> Result<(), CargoJsonMessageError> {
    if value.len() > max_bytes {
        Err(CargoJsonMessageError::FieldLimitExceeded { field })
    } else {
        Ok(())
    }
}

fn message_bool_field(
    value: Option<&Value>,
    field: &'static str,
) -> Result<bool, CargoJsonMessageError> {
    value
        .and_then(Value::as_bool)
        .ok_or(CargoJsonMessageError::InvalidField { field })
}

fn normalized_message_array(
    value: Option<&Value>,
    field: &'static str,
) -> Result<Box<[String]>, CargoJsonMessageError> {
    let values = value
        .and_then(Value::as_array)
        .ok_or(CargoJsonMessageError::InvalidField { field })?;
    if values.len() > MAX_FEATURE_COUNT {
        return Err(CargoJsonMessageError::FieldLimitExceeded { field });
    }
    let mut values = values
        .iter()
        .map(|value| message_string_field(Some(value), field, MAX_FEATURE_BYTES).map(str::to_owned))
        .collect::<Result<Vec<_>, _>>()?;
    values.sort_unstable();
    if values.windows(2).any(|pair| pair[0] == pair[1]) {
        return Err(CargoJsonMessageError::InvalidField { field });
    }
    Ok(values.into_boxed_slice())
}

fn u64_field(value: Option<&Value>, field: &'static str) -> Result<u64, CargoMetadataFactsError> {
    value
        .and_then(Value::as_u64)
        .ok_or(CargoMetadataFactsError::InvalidField { field })
}

fn truncate_utf8(value: &str, max_bytes: usize) -> (String, bool) {
    if value.len() <= max_bytes {
        return (value.to_owned(), false);
    }
    let mut end = max_bytes;
    while !value.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    (value[..end].to_owned(), true)
}

fn paths_match(left: &Path, right: &Path) -> bool {
    #[cfg(windows)]
    {
        fn normalized(path: &Path) -> String {
            let path = path.to_string_lossy();
            path.strip_prefix(r"\\?\")
                .unwrap_or(&path)
                .replace('/', "\\")
                .to_ascii_lowercase()
        }

        normalized(left) == normalized(right)
    }

    #[cfg(not(windows))]
    {
        left == right
    }
}

#[cfg(test)]
#[path = "cargo_facts/tests.rs"]
mod tests;
