use std::sync::Arc;

use nara_diagnostic::DiagnosticReport;
use nara_fs::DirectoryCapability;

use crate::project_host::authoring::{
    AuthoringProjectAdmissionError, AuthoringProjectAdmissionHost, AuthoringProjectReceipt,
    AuthoringProjectSourceAdmissionError, AuthoringSourceKind, PreparedAuthoringProject,
    PreparedAuthoringSchemaBasis, first_party_authoring_adapters,
};
use crate::project_host::composition::built_in_schema_providers;

use super::{EditorProjectOpenError, EditorProjectSession, single_error};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorProjectOpenPhase {
    ManifestAndProfile,
    AuthoringSchemaBundle,
    StartupClosure,
    SessionPublication,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorProjectOpenFailureSource {
    ProjectManifest,
    ProjectProfile,
    AuthoringSchemaBundle,
    AuthoringAdapter,
    ProjectSource,
    SessionPublication,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorProjectOpenFailureImpact {
    CandidateNotPublished,
}

#[derive(Debug)]
pub struct EditorProjectOpenFailure {
    phase: EditorProjectOpenPhase,
    source: EditorProjectOpenFailureSource,
    relative_source: Option<Arc<str>>,
    impact: EditorProjectOpenFailureImpact,
    repair_hint: &'static str,
    diagnostics: DiagnosticReport,
}

impl EditorProjectOpenFailure {
    #[must_use]
    pub const fn phase(&self) -> EditorProjectOpenPhase {
        self.phase
    }

    #[must_use]
    pub const fn source(&self) -> EditorProjectOpenFailureSource {
        self.source
    }

    #[must_use]
    pub fn relative_source(&self) -> Option<&str> {
        self.relative_source.as_deref()
    }

    #[must_use]
    pub const fn impact(&self) -> EditorProjectOpenFailureImpact {
        self.impact
    }

    #[must_use]
    pub const fn repair_hint(&self) -> &'static str {
        self.repair_hint
    }

    #[must_use]
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }
}

#[derive(Default)]
pub struct EditorProjectOpenHost {
    admission: AuthoringProjectAdmissionHost,
}

impl EditorProjectOpenHost {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn begin(
        &mut self,
        project_root: DirectoryCapability,
        profile: Option<String>,
    ) -> EditorProjectOpenAttempt {
        EditorProjectOpenAttempt {
            state: EditorProjectOpenAttemptState::ManifestAndProfile {
                project_root,
                profile,
            },
        }
    }

    #[must_use]
    pub fn advance(&mut self, attempt: EditorProjectOpenAttempt) -> EditorProjectOpenProgress {
        match attempt.state {
            EditorProjectOpenAttemptState::ManifestAndProfile {
                project_root,
                profile,
            } => match self.admission.prepare(project_root, profile.as_deref()) {
                Ok(prepared) => pending(EditorProjectOpenAttemptState::AuthoringSchemaBundle(
                    prepared,
                )),
                Err(error) => EditorProjectOpenProgress::Rejected(manifest_failure(error)),
            },
            EditorProjectOpenAttemptState::AuthoringSchemaBundle(prepared) => {
                let bundle_path = prepared.bundle_path().as_str().to_owned();
                let schema = match prepared.admit_bundle() {
                    Ok(candidate) => candidate.publish(),
                    Err(error) => {
                        return EditorProjectOpenProgress::Rejected(bundle_failure(
                            bundle_path,
                            error,
                        ));
                    }
                };
                let adapters = match first_party_authoring_adapters(built_in_schema_providers()) {
                    Ok(adapters) => adapters,
                    Err(_) => {
                        return EditorProjectOpenProgress::Rejected(failure(
                            EditorProjectOpenPhase::AuthoringSchemaBundle,
                            EditorProjectOpenFailureSource::AuthoringAdapter,
                            Some(bundle_path),
                            "Refresh the project authoring bundle from the selected engine build.",
                            single_error(
                                "project.editor.authoring-adapter-rejected",
                                "The compiled Authoring Adapter set could not be admitted",
                            ),
                        ));
                    }
                };
                pending(EditorProjectOpenAttemptState::StartupClosure(
                    PreparedAuthoringProject::new(schema, adapters),
                ))
            }
            EditorProjectOpenAttemptState::StartupClosure(prepared) => {
                match prepared.admit_startup_closure() {
                    Ok(candidate) => pending(EditorProjectOpenAttemptState::SessionPublication(
                        candidate.publish(),
                    )),
                    Err(error) => EditorProjectOpenProgress::Rejected(source_failure(error)),
                }
            }
            EditorProjectOpenAttemptState::SessionPublication(receipt) => {
                match EditorProjectSession::open(receipt) {
                    Ok(session) => EditorProjectOpenProgress::Opened(session),
                    Err(error) => EditorProjectOpenProgress::Rejected(session_failure(error)),
                }
            }
        }
    }
}

pub struct EditorProjectOpenAttempt {
    state: EditorProjectOpenAttemptState,
}

impl EditorProjectOpenAttempt {
    #[must_use]
    pub const fn phase(&self) -> EditorProjectOpenPhase {
        match &self.state {
            EditorProjectOpenAttemptState::ManifestAndProfile { .. } => {
                EditorProjectOpenPhase::ManifestAndProfile
            }
            EditorProjectOpenAttemptState::AuthoringSchemaBundle(_) => {
                EditorProjectOpenPhase::AuthoringSchemaBundle
            }
            EditorProjectOpenAttemptState::StartupClosure(_) => {
                EditorProjectOpenPhase::StartupClosure
            }
            EditorProjectOpenAttemptState::SessionPublication(_) => {
                EditorProjectOpenPhase::SessionPublication
            }
        }
    }
}

enum EditorProjectOpenAttemptState {
    ManifestAndProfile {
        project_root: DirectoryCapability,
        profile: Option<String>,
    },
    AuthoringSchemaBundle(PreparedAuthoringSchemaBasis),
    StartupClosure(PreparedAuthoringProject),
    SessionPublication(AuthoringProjectReceipt),
}

pub enum EditorProjectOpenProgress {
    Pending(EditorProjectOpenAttempt),
    Opened(EditorProjectSession),
    Rejected(EditorProjectOpenFailure),
}

fn pending(state: EditorProjectOpenAttemptState) -> EditorProjectOpenProgress {
    EditorProjectOpenProgress::Pending(EditorProjectOpenAttempt { state })
}

fn manifest_failure(error: AuthoringProjectAdmissionError) -> EditorProjectOpenFailure {
    let (source, diagnostics, repair_hint) = match error {
        AuthoringProjectAdmissionError::ManifestInvalid { diagnostics } => (
            EditorProjectOpenFailureSource::ProjectManifest,
            diagnostics,
            "Repair nara.toml and retry the project-open attempt.",
        ),
        AuthoringProjectAdmissionError::Profile(_) => (
            EditorProjectOpenFailureSource::ProjectProfile,
            single_error(
                "project.editor.profile-rejected",
                "The selected project profile could not be resolved",
            ),
            "Select a profile declared by nara.toml or clear the profile field.",
        ),
        _ => (
            EditorProjectOpenFailureSource::ProjectManifest,
            single_error(
                "project.editor.manifest-admission-rejected",
                "The project manifest could not be admitted",
            ),
            "Repair nara.toml and its host-visible path, then retry.",
        ),
    };
    failure(
        EditorProjectOpenPhase::ManifestAndProfile,
        source,
        Some("nara.toml".to_owned()),
        repair_hint,
        diagnostics,
    )
}

fn bundle_failure(
    bundle_path: String,
    error: AuthoringProjectAdmissionError,
) -> EditorProjectOpenFailure {
    let diagnostics = match error {
        AuthoringProjectAdmissionError::BundleInvalid(_) => single_error(
            "project.editor.authoring-bundle-invalid",
            "The declared authoring schema bundle is invalid",
        ),
        _ => single_error(
            "project.editor.authoring-bundle-admission-rejected",
            "The declared authoring schema bundle could not be admitted",
        ),
    };
    failure(
        EditorProjectOpenPhase::AuthoringSchemaBundle,
        EditorProjectOpenFailureSource::AuthoringSchemaBundle,
        Some(bundle_path),
        "Restore or regenerate the exact authoring schema bundle, then retry.",
        diagnostics,
    )
}

fn source_failure(error: AuthoringProjectSourceAdmissionError) -> EditorProjectOpenFailure {
    let (relative_source, diagnostics) = match error {
        AuthoringProjectSourceAdmissionError::SourcePublication { path, source, .. } => {
            (Some(path.as_str().to_owned()), source.into_diagnostics())
        }
        AuthoringProjectSourceAdmissionError::SourcePath { path, .. }
        | AuthoringProjectSourceAdmissionError::SourceHost { path, .. }
        | AuthoringProjectSourceAdmissionError::SourceIdentityKindConflict { path }
        | AuthoringProjectSourceAdmissionError::SourceTooLarge { path, .. }
        | AuthoringProjectSourceAdmissionError::UnsupportedSourceEncoding { path, .. }
        | AuthoringProjectSourceAdmissionError::SourceFormat { path, .. }
        | AuthoringProjectSourceAdmissionError::DocumentSourcePath { path, .. } => (
            Some(path.as_str().to_owned()),
            single_error(
                "project.editor.authoring-source-rejected",
                "A required project source could not be admitted for authoring",
            ),
        ),
        AuthoringProjectSourceAdmissionError::AssetMetadataFormat { path, .. }
        | AuthoringProjectSourceAdmissionError::AssetIdentity { path, .. } => (
            Some(path.as_str().to_owned()),
            single_error(
                "project.editor.authoring-asset-rejected",
                "A required project asset could not be admitted for authoring",
            ),
        ),
        AuthoringProjectSourceAdmissionError::AssetMetadataPathMismatch { requested, .. } => (
            Some(requested.as_str().to_owned()),
            single_error(
                "project.editor.authoring-asset-metadata-mismatch",
                "Asset metadata does not describe its requested project source",
            ),
        ),
        AuthoringProjectSourceAdmissionError::AssetMetadataChanged { path } => (
            Some(path.as_str().to_owned()),
            single_error(
                "project.editor.authoring-asset-metadata-source-changed",
                "Asset metadata changed during stable reference lookup; retry project admission",
            ),
        ),
        AuthoringProjectSourceAdmissionError::MissingStartupScene => (
            Some("nara.toml".to_owned()),
            single_error(
                "project.editor.startup-scene-missing",
                "The selected project profile does not declare a startup Scene",
            ),
        ),
        AuthoringProjectSourceAdmissionError::SourceDirectoryPath { kind, .. }
        | AuthoringProjectSourceAdmissionError::SourceDirectoryHost { kind, .. } => (
            source_directory_name(kind).map(str::to_owned),
            single_error(
                "project.editor.authoring-source-directory-rejected",
                "A required project source directory could not be admitted",
            ),
        ),
        _ => (
            None,
            single_error(
                "project.editor.startup-closure-rejected",
                "The startup authoring closure could not be admitted",
            ),
        ),
    };
    failure(
        EditorProjectOpenPhase::StartupClosure,
        EditorProjectOpenFailureSource::ProjectSource,
        relative_source,
        "Repair the reported source without changing unrelated project files, then retry.",
        diagnostics,
    )
}

fn session_failure(error: EditorProjectOpenError) -> EditorProjectOpenFailure {
    failure(
        EditorProjectOpenPhase::SessionPublication,
        EditorProjectOpenFailureSource::SessionPublication,
        None,
        "Repair the source authority conflict and retry; no candidate session was published.",
        error.into_diagnostics(),
    )
}

fn source_directory_name(kind: AuthoringSourceKind) -> Option<&'static str> {
    match kind {
        AuthoringSourceKind::Scene => Some("scenes"),
        AuthoringSourceKind::Prefab => Some("prefabs"),
        AuthoringSourceKind::AssetMetadata | AuthoringSourceKind::AssetSource => Some("assets"),
    }
}

fn failure(
    phase: EditorProjectOpenPhase,
    source: EditorProjectOpenFailureSource,
    relative_source: Option<String>,
    repair_hint: &'static str,
    diagnostics: DiagnosticReport,
) -> EditorProjectOpenFailure {
    EditorProjectOpenFailure {
        phase,
        source,
        relative_source: relative_source.map(Arc::from),
        impact: EditorProjectOpenFailureImpact::CandidateNotPublished,
        repair_hint,
        diagnostics,
    }
}
