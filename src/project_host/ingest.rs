use std::fmt;

use crate::fs::{ContentDigest, FileCapability, FileIdentity, FsError};
use nara_core::diagnostic::DiagnosticReport;
use nara_project::manifest_source::{
    ProjectManifestSource, ProjectManifestSourceError, ProjectManifestSourceErrorKind,
};

use super::composition::{
    ProjectSettingsCandidate, RuntimeCapabilityErrorKind, validate_runtime_capabilities,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectCandidateErrorKind {
    HostIo,
    HostAuthorityUnsupported,
    HostAuthorityUnproven,
    HostAuthorityRejected,
    ManifestTooLarge,
    Manifest,
    Profile,
    RuntimeConfiguration,
    PresetConflict,
    InvalidCapabilityClosure,
    UnavailableCapabilities,
}

#[derive(Clone, PartialEq)]
pub struct ProjectCandidateError {
    kind: ProjectCandidateErrorKind,
    diagnostics: DiagnosticReport,
}

impl ProjectCandidateError {
    #[must_use]
    pub const fn kind(&self) -> ProjectCandidateErrorKind {
        self.kind
    }

    #[must_use]
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }

    /// Lowers an authority or host failure encountered before a manifest
    /// `FileCapability` exists into the project-manifest diagnostic contract.
    #[must_use]
    pub fn from_manifest_authority(error: FsError) -> Self {
        ProjectManifestSourceError::from_manifest_authority(error).into()
    }

    fn new(kind: ProjectCandidateErrorKind, diagnostics: DiagnosticReport) -> Self {
        Self { kind, diagnostics }
    }
}

impl From<ProjectManifestSourceError> for ProjectCandidateError {
    fn from(error: ProjectManifestSourceError) -> Self {
        let kind = match error.kind() {
            ProjectManifestSourceErrorKind::HostIo => ProjectCandidateErrorKind::HostIo,
            ProjectManifestSourceErrorKind::HostAuthorityUnsupported => {
                ProjectCandidateErrorKind::HostAuthorityUnsupported
            }
            ProjectManifestSourceErrorKind::HostAuthorityUnproven => {
                ProjectCandidateErrorKind::HostAuthorityUnproven
            }
            ProjectManifestSourceErrorKind::HostAuthorityRejected => {
                ProjectCandidateErrorKind::HostAuthorityRejected
            }
            ProjectManifestSourceErrorKind::ManifestTooLarge => {
                ProjectCandidateErrorKind::ManifestTooLarge
            }
            ProjectManifestSourceErrorKind::Manifest => ProjectCandidateErrorKind::Manifest,
            ProjectManifestSourceErrorKind::Profile => ProjectCandidateErrorKind::Profile,
            ProjectManifestSourceErrorKind::RuntimeConfiguration => {
                ProjectCandidateErrorKind::RuntimeConfiguration
            }
        };
        ProjectCandidateError::new(kind, error.into_diagnostics())
    }
}

impl fmt::Debug for ProjectCandidateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ProjectCandidateError")
            .field("kind", &self.kind)
            .field("diagnostics", &self.diagnostics)
            .finish()
    }
}

impl fmt::Display for ProjectCandidateError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self.kind {
            ProjectCandidateErrorKind::HostIo => "project manifest host I/O failed",
            ProjectCandidateErrorKind::HostAuthorityUnsupported => {
                "project manifest authority is unsupported"
            }
            ProjectCandidateErrorKind::HostAuthorityUnproven => {
                "project manifest authority is unproven"
            }
            ProjectCandidateErrorKind::HostAuthorityRejected => {
                "project manifest authority was rejected"
            }
            ProjectCandidateErrorKind::ManifestTooLarge => {
                "project manifest exceeds its byte limit"
            }
            ProjectCandidateErrorKind::Manifest => "project manifest is invalid",
            ProjectCandidateErrorKind::Profile => "project profile is invalid",
            ProjectCandidateErrorKind::RuntimeConfiguration => {
                "project Runtime configuration is invalid"
            }
            ProjectCandidateErrorKind::PresetConflict => {
                "runtime preset conflicts with requested product capabilities"
            }
            ProjectCandidateErrorKind::InvalidCapabilityClosure => {
                "project product capability closure is incomplete"
            }
            ProjectCandidateErrorKind::UnavailableCapabilities => {
                "project requests unavailable product capabilities"
            }
        })
    }
}

impl std::error::Error for ProjectCandidateError {}

/// Reads, parses, lowers, and product-validates one already authorized `nara.toml` handle.
pub fn ingest_project_manifest(
    manifest: &FileCapability,
    profile: Option<&str>,
) -> Result<ProjectSettingsCandidate, ProjectCandidateError> {
    candidate_from_source(ProjectManifestSource::from_file(manifest, profile)?)
}

pub(crate) fn ingest_project_manifest_with_expected_digest(
    manifest: &FileCapability,
    profile: Option<&str>,
    expected_digest: ContentDigest,
) -> Result<ProjectSettingsCandidate, ProjectCandidateError> {
    candidate_from_source(ProjectManifestSource::from_file_with_expected_digest(
        manifest,
        profile,
        expected_digest,
    )?)
}

pub(crate) fn ingest_project_manifest_bytes(
    bytes: &[u8],
    profile: Option<&str>,
    project_root_identity: FileIdentity,
) -> Result<ProjectSettingsCandidate, ProjectCandidateError> {
    candidate_from_source(ProjectManifestSource::from_bytes(
        bytes,
        profile,
        Some(project_root_identity),
    )?)
}

#[cfg(feature = "project-tooling")]
pub(crate) fn read_manifest_profile(
    manifest: &FileCapability,
    profile: Option<&str>,
) -> Result<nara_project::EffectiveProjectSettings, ProjectCandidateError> {
    ProjectManifestSource::read_profile(manifest, profile).map_err(Into::into)
}

fn candidate_from_source(
    source: ProjectManifestSource,
) -> Result<ProjectSettingsCandidate, ProjectCandidateError> {
    let (lineage, manifest_digest, settings, source_runtime_projection) = source.into_parts();
    let admission =
        validate_runtime_capabilities(source_runtime_projection.runtime()).map_err(|error| {
            let kind = match error.kind() {
                RuntimeCapabilityErrorKind::PresetConflict => {
                    ProjectCandidateErrorKind::PresetConflict
                }
                RuntimeCapabilityErrorKind::InvalidCapabilityClosure => {
                    ProjectCandidateErrorKind::InvalidCapabilityClosure
                }
                RuntimeCapabilityErrorKind::UnavailableCapabilities => {
                    ProjectCandidateErrorKind::UnavailableCapabilities
                }
            };
            ProjectCandidateError::new(kind, error.into_diagnostics())
        })?;
    Ok(ProjectSettingsCandidate {
        lineage,
        manifest_digest,
        settings,
        source_runtime_projection,
        capabilities: admission,
    })
}
