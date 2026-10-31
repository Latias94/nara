use std::{
    mem,
    path::{Path, PathBuf},
};

use thiserror::Error;

use super::messages::{
    CargoCompilerArtifactFacts, CargoDiagnostics, CargoDiagnosticsBuilder, CargoJsonMessage,
    CargoJsonMessageError,
};
use super::metadata::{CargoMetadataFacts, CargoPackageGraphFact};
use super::selection::CargoBuildSelection;
use super::{
    DEV_PROFILE, DEV_PROFILE_DIRECTORY, MAX_JSON_RECORD_BYTES, MAX_JSON_RECORDS,
    MAX_JSON_STREAM_BYTES, MAX_PATH_BYTES, paths_match,
};

/// A bound package/bin/profile/target Cargo invocation and its metadata observations.
///
/// This records no execution Host and makes no readiness claim. The selected target may be foreign
/// to the process that runs Cargo. Cargo supplies the output filename; only its directory is bound
/// in advance. Absolute paths and package IDs are operational facts, not persistent identity.
/// The command owner must run Cargo with this requested selection; the compiler artifact supplies
/// the actual compiled features separately from the workspace metadata observation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoBuildInvocation {
    pub(super) package_id: String,
    package: CargoPackageGraphFact,
    manifest_path: PathBuf,
    workspace_root: PathBuf,
    target_directory: PathBuf,
    pub(super) source_path: PathBuf,
    pub(super) required_features: Box<[String]>,
    metadata_features: Box<[String]>,
    selection: CargoBuildSelection,
    pub(super) executable_directory: PathBuf,
}

impl CargoBuildInvocation {
    pub(super) fn new(
        metadata: &CargoMetadataFacts,
        selection: CargoBuildSelection,
    ) -> Result<Self, CargoInvocationError> {
        if selection.package() != metadata.package.name() {
            return Err(CargoInvocationError::PackageMismatch);
        }
        if selection != metadata.selection {
            return Err(CargoInvocationError::SelectionMismatch);
        }
        let profile_directory = match selection.profile() {
            DEV_PROFILE | "test" => DEV_PROFILE_DIRECTORY,
            "release" | "bench" => "release",
            profile => profile,
        };
        let executable_directory = metadata
            .target_directory
            .join(selection.target_triple())
            .join(profile_directory);
        if executable_directory.to_string_lossy().len() >= MAX_PATH_BYTES {
            return Err(CargoInvocationError::ExecutablePathLimitExceeded);
        }
        Ok(Self {
            package_id: metadata.package_id.clone(),
            package: metadata.package.clone(),
            manifest_path: metadata.manifest_path.clone(),
            workspace_root: metadata.workspace_root.clone(),
            target_directory: metadata.target_directory.clone(),
            source_path: metadata.source_path.clone(),
            required_features: metadata.required_features.clone(),
            metadata_features: metadata.metadata_features.clone(),
            selection,
            executable_directory,
        })
    }

    pub fn package(&self) -> &CargoPackageGraphFact {
        &self.package
    }

    pub fn package_name(&self) -> &str {
        self.package.name()
    }

    pub fn package_version(&self) -> &str {
        self.package.version()
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }

    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    pub fn target_directory(&self) -> &Path {
        &self.target_directory
    }

    pub fn source_path(&self) -> &Path {
        &self.source_path
    }

    pub fn target_name(&self) -> &str {
        self.selection.target()
    }

    pub fn profile(&self) -> &str {
        self.selection.profile()
    }

    /// Returns requested feature arguments, separate from Cargo's resolved feature closure.
    pub fn features(&self) -> &[String] {
        self.selection.features()
    }

    /// Returns the selected package's features observed by workspace metadata.
    ///
    /// These can differ from the actual features reported by the selected compiler artifact.
    pub fn metadata_features(&self) -> &[String] {
        &self.metadata_features
    }

    pub const fn default_features(&self) -> bool {
        self.selection.default_features()
    }

    pub fn target_triple(&self) -> &str {
        self.selection.target_triple()
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CargoInvocationError {
    #[error("the requested Cargo package does not match the observed manifest owner")]
    PackageMismatch,
    #[error("the Cargo build selection differs from the metadata selection")]
    SelectionMismatch,
    #[error("the selected Cargo executable directory exceeded its bounded size")]
    ExecutablePathLimitExceeded,
}

/// Exact field rejected while binding a selected binary artifact to its invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CargoArtifactMismatchField {
    Manifest,
    TargetKind,
    CrateTypes,
    SourcePath,
    RequiredFeatures,
    Profile,
    ExecutablePath,
}

/// Incremental bounded decoder and exact selector for one Cargo binary build.
///
/// A malformed, contradictory or over-budget stream permanently closes the decoder. Diagnostics
/// remain available after failure; a later record cannot repair a rejected stream.
#[derive(Debug)]
pub struct CargoBuildDecoder {
    invocation: CargoBuildInvocation,
    pending_record: Vec<u8>,
    pub(super) total_bytes: usize,
    pub(super) record_count: usize,
    selected: Option<CargoArtifactFacts>,
    build_finished: Option<bool>,
    diagnostics: CargoDiagnosticsBuilder,
    finalized: bool,
}

impl CargoBuildDecoder {
    pub fn new(invocation: CargoBuildInvocation) -> Self {
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

    pub fn push(&mut self, bytes: &[u8]) -> Result<(), CargoBuildError> {
        if self.finalized {
            return Err(CargoBuildError::AlreadyFinished);
        }
        let result = self.push_inner(bytes);
        if result.is_err() {
            self.finalized = true;
        }
        result
    }

    pub fn diagnostics(&self) -> CargoDiagnostics {
        self.diagnostics.snapshot()
    }

    pub fn finish(&mut self) -> Result<CargoBuildFacts, CargoBuildError> {
        if self.finalized {
            return Err(CargoBuildError::AlreadyFinished);
        }
        self.finalized = true;
        if !self.pending_record.is_empty() {
            self.observe_pending_record()?;
        }
        match self.build_finished {
            None => return Err(CargoBuildError::MissingBuildFinished),
            Some(false) => return Err(CargoBuildError::BuildFailed),
            Some(true) => {}
        }
        let artifact = self
            .selected
            .take()
            .ok_or(CargoBuildError::MissingArtifact)?;
        Ok(CargoBuildFacts {
            artifact,
            diagnostics: self.diagnostics.snapshot(),
        })
    }

    fn push_inner(&mut self, bytes: &[u8]) -> Result<(), CargoBuildError> {
        self.total_bytes = self
            .total_bytes
            .checked_add(bytes.len())
            .filter(|bytes| *bytes <= MAX_JSON_STREAM_BYTES)
            .ok_or(CargoBuildError::InputLimitExceeded)?;
        let mut start = 0;
        for (index, byte) in bytes.iter().enumerate() {
            if *byte != b'\n' {
                continue;
            }
            self.extend_pending(&bytes[start..index])?;
            self.observe_pending_record()?;
            start = index + 1;
        }
        self.extend_pending(&bytes[start..])
    }

    fn extend_pending(&mut self, bytes: &[u8]) -> Result<(), CargoBuildError> {
        if self.pending_record.len().saturating_add(bytes.len()) > MAX_JSON_RECORD_BYTES {
            return Err(CargoBuildError::RecordLimitExceeded);
        }
        self.pending_record.extend_from_slice(bytes);
        Ok(())
    }

    fn observe_pending_record(&mut self) -> Result<(), CargoBuildError> {
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
            .ok_or(CargoBuildError::RecordCountExceeded)?;
        let record = mem::take(&mut self.pending_record);
        self.observe(CargoJsonMessage::parse_slice(&record)?)
    }

    fn observe(&mut self, message: CargoJsonMessage) -> Result<(), CargoBuildError> {
        if matches!(message, CargoJsonMessage::BuildFinished { .. }) {
            if self.build_finished.is_some() {
                return Err(CargoBuildError::DuplicateBuildFinished);
            }
        } else if self.build_finished.is_some() {
            return Err(CargoBuildError::MessageAfterBuildFinished);
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
    ) -> Result<(), CargoBuildError> {
        if artifact.package_id != self.invocation.package_id {
            return Ok(());
        }
        if !paths_match(&artifact.manifest_path, &self.invocation.manifest_path) {
            return Err(CargoBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::Manifest,
            });
        }
        if artifact.target.name != self.invocation.target_name() {
            return Ok(());
        }
        // A package's library may have the same name as its binary. It is not this selection.
        if !artifact.target.kind.iter().any(|kind| kind == "bin") {
            return Ok(());
        }
        if artifact.target.kind.as_ref() != ["bin"] {
            return Err(CargoBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::TargetKind,
            });
        }
        if artifact.target.crate_types.as_ref() != ["bin"] {
            return Err(CargoBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::CrateTypes,
            });
        }
        if !paths_match(&artifact.target.source_path, &self.invocation.source_path) {
            return Err(CargoBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::SourcePath,
            });
        }
        if artifact
            .target
            .required_features
            .as_deref()
            .unwrap_or_default()
            != self.invocation.required_features.as_ref()
        {
            return Err(CargoBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::RequiredFeatures,
            });
        }
        if artifact.profile_test {
            return Err(CargoBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::Profile,
            });
        }
        let executable = artifact
            .executable
            .ok_or(CargoBuildError::NonExecutableArtifact)?;
        if !executable.is_absolute() {
            return Err(CargoBuildError::NonAbsoluteExecutable);
        }
        let text = executable.to_string_lossy();
        if executable.file_name().is_none()
            || text.ends_with(['/', '\\'])
            || !executable
                .parent()
                .is_some_and(|parent| paths_match(parent, &self.invocation.executable_directory))
        {
            return Err(CargoBuildError::ArtifactMismatch {
                field: CargoArtifactMismatchField::ExecutablePath,
            });
        }
        if self.selected.is_some() {
            return Err(CargoBuildError::DuplicateArtifact);
        }
        self.selected = Some(CargoArtifactFacts {
            package: self.invocation.package.clone(),
            target_name: self.invocation.target_name().to_owned(),
            profile: self.invocation.profile().to_owned(),
            features: artifact.features,
            target_triple: self.invocation.target_triple().to_owned(),
            executable,
            fresh: artifact.fresh,
        });
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum CargoBuildError {
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
    #[error("Cargo did not emit the selected binary executable artifact")]
    MissingArtifact,
    #[error("Cargo emitted the selected binary artifact more than once")]
    DuplicateArtifact,
    #[error("Cargo emitted a non-executable artifact for the selected binary")]
    NonExecutableArtifact,
    #[error("Cargo emitted a non-absolute executable path for the selected binary")]
    NonAbsoluteExecutable,
    #[error("Cargo artifact field {field:?} did not match the bound invocation")]
    ArtifactMismatch { field: CargoArtifactMismatchField },
    #[error("the Cargo JSON decoder was already finalized")]
    AlreadyFinished,
}

/// Successful exact artifact selection plus bounded structured diagnostics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoBuildFacts {
    artifact: CargoArtifactFacts,
    diagnostics: CargoDiagnostics,
}

impl CargoBuildFacts {
    pub fn artifact(&self) -> &CargoArtifactFacts {
        &self.artifact
    }

    pub fn diagnostics(&self) -> &CargoDiagnostics {
        &self.diagnostics
    }
}

/// A Cargo-reported target executable, with no execution or publication qualification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoArtifactFacts {
    package: CargoPackageGraphFact,
    target_name: String,
    profile: String,
    features: Box<[String]>,
    target_triple: String,
    executable: PathBuf,
    fresh: bool,
}

impl CargoArtifactFacts {
    pub fn package(&self) -> &CargoPackageGraphFact {
        &self.package
    }

    pub fn target_name(&self) -> &str {
        &self.target_name
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// Returns the actual resolved features reported by Cargo for this artifact.
    pub fn features(&self) -> &[String] {
        &self.features
    }

    pub fn target_triple(&self) -> &str {
        &self.target_triple
    }

    /// Returns the transient Cargo output selected for immutable publication by the owner.
    pub fn executable(&self) -> &Path {
        &self.executable
    }

    pub const fn fresh(&self) -> bool {
        self.fresh
    }
}
