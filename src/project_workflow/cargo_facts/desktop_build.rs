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
use super::{
    DESKTOP_FEATURE, DESKTOP_TARGET, DEV_PROFILE, DEV_PROFILE_DIRECTORY, MAX_HOST_TARGET_BYTES,
    MAX_JSON_RECORD_BYTES, MAX_JSON_RECORDS, MAX_JSON_STREAM_BYTES, MAX_PATH_BYTES, paths_match,
};

impl CargoMetadataFacts {
    pub fn desktop_invocation(
        &self,
        host_target: impl Into<String>,
    ) -> Result<CargoDesktopBuildInvocation, CargoDesktopInvocationError> {
        CargoDesktopBuildInvocation::new(self, host_target.into())
    }
}

/// Exact transient invocation facts for the closed desktop/dev/host Cargo build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CargoDesktopBuildInvocation {
    pub(super) package_id: String,
    pub(super) root_package: CargoPackageGraphFact,
    pub(super) manifest_path: PathBuf,
    pub(super) target_directory: PathBuf,
    pub(super) desktop_source_path: PathBuf,
    pub(super) host_target: String,
    pub(super) expected_executable: PathBuf,
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
            desktop_source_path: metadata.selected_source_path.clone(),
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
    pub(super) invocation: CargoDesktopBuildInvocation,
    pub(super) pending_record: Vec<u8>,
    pub(super) total_bytes: usize,
    pub(super) record_count: usize,
    pub(super) selected: Option<CargoDesktopArtifactFacts>,
    pub(super) build_finished: Option<bool>,
    pub(super) diagnostics: CargoDiagnosticsBuilder,
    pub(super) finalized: bool,
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
    pub(super) artifact: CargoDesktopArtifactFacts,
    pub(super) diagnostics: CargoDiagnostics,
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
    pub(super) root_package: CargoPackageGraphFact,
    pub(super) host_target: String,
    pub(super) executable: PathBuf,
    pub(super) fresh: bool,
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
