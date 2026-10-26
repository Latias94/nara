use std::sync::Arc;

use nara_diagnostic::DiagnosticReport;
use nara_fs::DirectoryCapability;

use crate::project_host::authoring::{
    AuthoringProjectAdmissionError, AuthoringProjectAdmissionHost, AuthoringProjectReceipt,
    AuthoringProjectSourceAdmissionError, AuthoringSourceKind, PreparedAuthoringProject,
    PreparedAuthoringSchemaBasis, project_authoring_adapters,
};

use super::{EditorProjectOpenError, EditorProjectSession, single_error};

mod preparation;

use preparation::{
    StartupPreparation, StartupPreparationFailure, StartupPreparationHost,
    StartupPreparationProgress,
};

/// Standalone Open Host custody, separate from every Session's Runtime close ledger.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorProjectOpenTasksCloseReport {
    tasks: nara_tasks::TaskShutdownReport,
    rejected_input_cleanup_failed: bool,
}

impl EditorProjectOpenTasksCloseReport {
    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.tasks.is_complete()
    }

    #[must_use]
    pub fn has_cleanup_failure(&self) -> bool {
        self.rejected_input_cleanup_failed
            || self.tasks.has_cleanup_failure()
            || self.tasks.timed_out()
            || nara_tasks::TaskPoolKind::ALL
                .into_iter()
                .any(|kind| self.tasks.for_kind(kind).panicked_workers != 0)
    }

    #[must_use]
    pub const fn tasks(&self) -> &nara_tasks::TaskShutdownReport {
        &self.tasks
    }

    #[must_use]
    pub const fn rejected_input_cleanup_failed(&self) -> bool {
        self.rejected_input_cleanup_failed
    }
}

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
    StartupPreparation,
    SessionPublication,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditorProjectOpenFailureImpact {
    CandidateNotPublished,
}

#[derive(Debug)]
#[must_use = "a rejected Open can retain cleanup independently of publication"]
pub struct EditorProjectOpenFailure {
    phase: EditorProjectOpenPhase,
    source: EditorProjectOpenFailureSource,
    relative_source: Option<Arc<str>>,
    impact: EditorProjectOpenFailureImpact,
    repair_hint: &'static str,
    error: EditorProjectOpenError,
    startup: Option<StartupPreparationFailure>,
}

impl EditorProjectOpenFailure {
    /// Publication refusal and physical retirement are independent observations.
    #[must_use]
    pub fn retirement_state(&self) -> nara_app::RuntimeCandidateRetirementState {
        let runtime = self.error.retirement_state();
        if runtime == nara_app::RuntimeCandidateRetirementState::Retired
            && self
                .startup
                .as_ref()
                .is_some_and(|failure| !failure.is_complete())
        {
            nara_app::RuntimeCandidateRetirementState::Retiring
        } else {
            runtime
        }
    }

    #[must_use]
    pub fn close_evidence(&self) -> Option<&nara_app::RuntimeCloseEvidence> {
        self.error.close_evidence()
    }

    /// Advances cleanup once on the failed App's construction thread.
    pub fn advance_retirement(&mut self) -> nara_app::RuntimeCandidateRetirementState {
        self.error.advance_retirement();
        if let Some(startup) = &mut self.startup {
            startup.advance_retirement();
        }
        self.retirement_state()
    }

    /// Exact pre-admission refusal; unadmitted input cleanup is not Runtime evidence.
    #[must_use]
    pub fn startup_task_rejection(&self) -> Option<nara_tasks::TaskRejection> {
        self.startup.as_ref().and_then(|failure| failure.rejection)
    }

    #[must_use]
    pub fn startup_tasks_close_report(&self) -> Option<EditorProjectOpenTasksCloseReport> {
        self.startup.as_ref().map(StartupPreparationFailure::report)
    }

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
        self.error.diagnostics()
    }
}

/// Coordinates owner-thread admission with bounded background startup preparation.
///
/// Reuse this Host across attempts. `StartupClosure` may remain pending across many calls to
/// [`Self::advance`]; poll from the Host's normal maintenance loop instead of busy-waiting.
/// Manifest/schema admission and session publication still execute on the caller's owner thread.
#[derive(Default)]
pub struct EditorProjectOpenHost {
    admission: AuthoringProjectAdmissionHost,
    preparation: StartupPreparationHost,
    source_asset_importers: super::EditorSourceAssetImporters,
    #[cfg(test)]
    pub(in crate::project_host::runtime::editor) next_authoring_publication_hook:
        Option<Box<dyn FnOnce(&super::authoring_runtime::EditorAuthoringRuntime)>>,
}

impl EditorProjectOpenHost {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Observes failed facility retirement once without closing reusable healthy facilities.
    pub fn advance_retirement(&mut self) -> EditorProjectOpenTasksCloseReport {
        self.preparation.advance_retirement()
    }

    #[must_use]
    pub fn retirement_pending(&self) -> bool {
        self.preparation.retirement_pending()
    }

    /// Irreversibly closes this Host's preparation admission and polls physical return once.
    ///
    /// Session Close does not cover this independent owner. Continue polling during process Exit;
    /// completion and cleanup success are independent. This cannot interrupt a running OS read.
    pub fn poll_shutdown(&mut self) -> EditorProjectOpenTasksCloseReport {
        self.preparation.poll_shutdown()
    }

    #[must_use]
    pub fn tasks_close_report(&self) -> EditorProjectOpenTasksCloseReport {
        self.preparation.report()
    }

    /// Selects only trusted code compiled into the Editor; project files cannot add contributors.
    #[must_use]
    pub fn with_source_asset_importers(
        mut self,
        importers: super::EditorSourceAssetImporters,
    ) -> Self {
        self.source_asset_importers = importers;
        self
    }

    /// Opens the explicitly trusted compiled lane with this Host's selected contributions.
    #[doc(hidden)]
    pub fn open_trusted_compiled(
        &self,
        project_root: DirectoryCapability,
        intent: super::TrustedCompiledEditorProjectIntent,
    ) -> Result<EditorProjectSession, super::EditorProjectOpenError> {
        if self.preparation.is_closing() {
            return Err(startup_worker_failure(None).error);
        }
        let mut session = EditorProjectSession::open_trusted_compiled(project_root, intent)?;
        session.source_asset_import.importers = self.source_asset_importers.clone();
        Ok(session)
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
        if self.preparation.is_closing() {
            return EditorProjectOpenProgress::Rejected(startup_worker_failure(None));
        }
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
                let adapters = match project_authoring_adapters(schema.bundle()) {
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
                match self.preparation.start(prepared) {
                    Ok(preparation) => {
                        pending(EditorProjectOpenAttemptState::StartupPreparing(preparation))
                    }
                    Err(refusal) => {
                        EditorProjectOpenProgress::Rejected(startup_worker_failure(Some(refusal)))
                    }
                }
            }
            EditorProjectOpenAttemptState::StartupPreparing(mut preparation) => {
                match preparation.poll() {
                    StartupPreparationProgress::Pending => {
                        pending(EditorProjectOpenAttemptState::StartupPreparing(preparation))
                    }
                    StartupPreparationProgress::Completed(Ok(candidate)) => pending(
                        EditorProjectOpenAttemptState::SessionPublication(candidate.publish()),
                    ),
                    StartupPreparationProgress::Completed(Err(error)) => {
                        EditorProjectOpenProgress::Rejected(source_failure(error))
                    }
                    StartupPreparationProgress::Unavailable => {
                        EditorProjectOpenProgress::Rejected(startup_worker_failure(None))
                    }
                }
            }
            EditorProjectOpenAttemptState::SessionPublication(receipt) => {
                match EditorProjectSession::open(
                    receipt,
                    #[cfg(test)]
                    self.next_authoring_publication_hook.take(),
                ) {
                    Ok(mut session) => {
                        session.source_asset_import.importers = self.source_asset_importers.clone();
                        EditorProjectOpenProgress::Opened(session)
                    }
                    Err(error) => EditorProjectOpenProgress::Rejected(session_failure(error)),
                }
            }
        }
    }
}

/// A linear project-open attempt. Dropping it revokes pending startup publication.
///
/// Cancellation does not interrupt a filesystem call already running on a task worker.
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
            EditorProjectOpenAttemptState::StartupClosure(_)
            | EditorProjectOpenAttemptState::StartupPreparing(_) => {
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
    StartupPreparing(StartupPreparation),
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
        AuthoringProjectSourceAdmissionError::NativeSource(error) => {
            let mut diagnostics = DiagnosticReport::default();
            diagnostics.push(error.diagnostic());
            (None, diagnostics)
        }
        AuthoringProjectSourceAdmissionError::SourcePublication { path, source, .. } => {
            (Some(path.as_str().to_owned()), source.into_diagnostics())
        }
        AuthoringProjectSourceAdmissionError::SourceStructure { path, source, .. } => {
            (Some(path.as_str().to_owned()), source.into_diagnostics())
        }
        AuthoringProjectSourceAdmissionError::CompilationBudget(diagnostics) => (None, diagnostics),
        AuthoringProjectSourceAdmissionError::SourceChanged { path, .. } => (
            Some(path.as_str().to_owned()),
            single_error(
                "project.editor.authoring-source-changed",
                "A required source changed during authoring capture; retry project admission",
            ),
        ),
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

fn startup_worker_failure(startup: Option<StartupPreparationFailure>) -> EditorProjectOpenFailure {
    let (code, summary) = match startup
        .as_ref()
        .and_then(|failure| failure.rejection)
        .map(|rejection| rejection.reason)
    {
        Some(nara_tasks::TaskRejectReason::WorkerStartupFailed) => (
            "project.editor.startup-worker-startup-failed",
            "The startup preparation worker could not be started",
        ),
        Some(nara_tasks::TaskRejectReason::QueueFull { .. }) => (
            "project.editor.startup-preparation-busy",
            "The startup preparation queue is full",
        ),
        _ => (
            "project.editor.startup-preparation-unavailable",
            "The startup authoring closure could not be prepared",
        ),
    };
    let mut failure = failure(
        EditorProjectOpenPhase::StartupClosure,
        EditorProjectOpenFailureSource::StartupPreparation,
        None,
        "Retry project Open after other startup preparation has finished.",
        single_error(code, summary),
    );
    failure.startup = startup;
    failure
}

fn session_failure(error: EditorProjectOpenError) -> EditorProjectOpenFailure {
    EditorProjectOpenFailure {
        phase: EditorProjectOpenPhase::SessionPublication,
        source: EditorProjectOpenFailureSource::SessionPublication,
        relative_source: None,
        impact: EditorProjectOpenFailureImpact::CandidateNotPublished,
        repair_hint: "Repair the source authority conflict and retry; no candidate session was published.",
        error,
        startup: None,
    }
}

fn source_directory_name(kind: AuthoringSourceKind) -> Option<&'static str> {
    match kind {
        AuthoringSourceKind::ProjectManifest | AuthoringSourceKind::SchemaBundle => None,
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
        error: EditorProjectOpenError::new(diagnostics),
        startup: None,
    }
}
