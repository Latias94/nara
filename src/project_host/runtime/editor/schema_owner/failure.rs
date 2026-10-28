//! A failed operation retains the owning source report rather than inferring it from its outcome.

use std::sync::Arc;

use nara_diagnostic::DiagnosticReport;
use nara_tasks::TaskRejection;
use nara_tooling::{EditorDocumentId, EditorWorkspaceCommandReport};

use super::{
    EditorSchemaOwnerAdoptionError as Reason, EditorSchemaOwnerAdoptionId,
    EditorSchemaOwnerAdoptionPhase as Phase,
};
use crate::project_host::authoring::{
    AuthoringProjectSourceAdmissionError, AuthoringSchemaOwnerError,
};
use crate::project_host::file_publication::{
    FilePublicationFailureStage, FilePublicationRejection,
};

/// Exact failure observation for one admitted metadata operation.
///
/// Cancellation, file publication and physical cleanup remain independent. A cancelled operation
/// can retain a failure returned by its worker. Reports preserve their existing bounded retention
/// and truncation facts; native filesystem paths and arbitrary error Display strings are not rendered.
#[derive(Debug, thiserror::Error)]
#[error("Schema owner adoption failed during {phase:?}: {reason}")]
pub struct EditorSchemaOwnerAdoptionFailure {
    operation: Option<EditorSchemaOwnerAdoptionId>,
    reason: Reason,
    phase: Phase,
    document: Option<EditorDocumentId>,
    relative_source: Option<Arc<str>>,
    diagnostics: DiagnosticReport,
    source_report: Option<Box<EditorWorkspaceCommandReport>>,
    task_rejection: Option<TaskRejection>,
}

impl EditorSchemaOwnerAdoptionFailure {
    pub const fn operation(&self) -> Option<EditorSchemaOwnerAdoptionId> {
        self.operation
    }
    pub const fn reason(&self) -> Reason {
        self.reason
    }
    pub const fn phase(&self) -> Phase {
        self.phase
    }
    pub const fn document(&self) -> Option<EditorDocumentId> {
        self.document
    }
    pub fn relative_source(&self) -> Option<&str> {
        self.relative_source.as_deref()
    }
    pub const fn diagnostics(&self) -> &DiagnosticReport {
        &self.diagnostics
    }
    pub fn source_report(&self) -> Option<&EditorWorkspaceCommandReport> {
        self.source_report.as_deref()
    }
    pub const fn task_rejection(&self) -> Option<TaskRejection> {
        self.task_rejection
    }

    pub(super) fn new(reason: Reason, phase: Phase) -> Self {
        let (code, summary) = match reason {
            Reason::Installation => (
                "project.editor.schema-owner-installation-rejected",
                "The selected installation changed or could not be inspected",
            ),
            Reason::Schema => (
                "project.editor.schema-owner-composition-rejected",
                "The installed Schema owner could not be composed with this project",
            ),
            Reason::Stale => (
                "project.editor.schema-owner-source-stale",
                "An original source or project observation changed",
            ),
            Reason::Preparation => (
                "project.editor.schema-owner-preparation-rejected",
                "The complete source and history set could not be prepared",
            ),
            Reason::Task => (
                "project.editor.schema-owner-task-unavailable",
                "The metadata operation's finite worker was unavailable",
            ),
            Reason::Publication => (
                "project.editor.schema-owner-publication-rejected",
                "The original file observation refused metadata publication",
            ),
            Reason::RequiresReconcile => (
                "project.editor.schema-owner-publication-unobserved",
                "The metadata worker did not return its publication observation; reconcile actual disk state",
            ),
            _ => (
                "project.editor.schema-owner-operation-refused",
                "The metadata operation could not continue in its current Session",
            ),
        };
        Self {
            operation: None,
            reason,
            phase,
            document: None,
            relative_source: None,
            diagnostics: super::super::single_error(code, summary),
            source_report: None,
            task_rejection: None,
        }
    }

    pub(super) fn workspace(report: EditorWorkspaceCommandReport, phase: Phase) -> Self {
        let mut failure = Self::new(Reason::Preparation, phase);
        failure.document = report.document;
        failure.diagnostics = report.diagnostics.clone();
        failure.source_report = Some(Box::new(report));
        failure
    }

    pub(super) fn source(
        error: AuthoringProjectSourceAdmissionError,
        reason: Reason,
        phase: Phase,
        document: Option<EditorDocumentId>,
    ) -> Self {
        let (path, diagnostics) = super::super::authoring_sources::authoring_source_failure(error);
        let mut failure = Self::new(reason, phase).at_source(path.as_deref());
        failure.document = document;
        failure.diagnostics = diagnostics;
        failure
    }

    pub(super) fn metadata(error: AuthoringSchemaOwnerError, phase: Phase) -> Self {
        use AuthoringSchemaOwnerError as E;
        let mut failure = Self::new(
            if phase == Phase::Publishing {
                Reason::Stale
            } else {
                Reason::Schema
            },
            phase,
        );
        let manifest_changed = matches!(&error, E::ManifestChanged);
        failure.diagnostics = match error {
            E::Basis(source) => source.into_diagnostics(),
            E::ManifestChanged => super::super::single_error(
                "project.editor.schema-owner-manifest-changed",
                "The originally admitted project manifest changed",
            ),
            E::BundleChanged => super::super::single_error(
                "project.editor.schema-owner-bundle-changed",
                "The originally selected authoring bundle changed",
            ),
            E::UnsupportedLineage => super::super::single_error(
                "project.editor.schema-owner-lineage-unsupported",
                "Adding metadata requires a generation-one catalog without a predecessor",
            ),
            E::ExistingOwnerChanged => super::super::single_error(
                "project.editor.schema-owner-existing-content-changed",
                "An existing owner's complete declarations differ from the selected installation",
            ),
            E::Budget(_) => super::super::single_error(
                "project.editor.schema-owner-budget-exceeded",
                "Metadata preparation exhausted its finite project allowance",
            ),
            E::Composition(_) => failure.diagnostics,
            E::Encode(_) => super::super::single_error(
                "project.editor.schema-owner-encoding-rejected",
                "The composed authoring bundle could not be encoded",
            ),
            E::Adapters(_) => super::super::single_error(
                "project.editor.schema-owner-adapter-rejected",
                "The composed metadata could not admit its authoring adapters",
            ),
            E::Publisher(_) => super::super::single_error(
                "project.editor.schema-owner-publisher-rejected",
                "The file owner could not bind the originally admitted bundle leaf",
            ),
        };
        if manifest_changed {
            failure = failure.at_source(Some("nara.toml"));
        }
        failure
    }

    pub(super) fn publication_rejected(reason: FilePublicationRejection) -> Self {
        use FilePublicationRejection as E;
        let (code, summary) = match reason {
            E::TargetChanged => (
                "project.editor.schema-owner-target-changed",
                "The selected metadata file changed before publication",
            ),
            E::TargetDeleted => (
                "project.editor.schema-owner-target-deleted",
                "The selected metadata file was deleted before publication",
            ),
            E::LockUnavailable => (
                "project.editor.schema-owner-lock-unavailable",
                "The metadata file's publication lock is unavailable",
            ),
            E::UnsupportedGuarantee => (
                "project.editor.schema-owner-publication-unsupported",
                "The filesystem cannot establish the required publication guarantees",
            ),
            E::RequiresReconcile => (
                "project.editor.schema-owner-publisher-reconcile-required",
                "The metadata file owner requires explicit reconciliation",
            ),
            E::CleanupPending => (
                "project.editor.schema-owner-cleanup-pending",
                "Earlier temporary metadata cleanup is still pending",
            ),
        };
        let mut failure = Self::new(Reason::Publication, Phase::Publishing);
        failure.diagnostics = super::super::single_error(code, summary);
        failure
    }

    pub(super) fn publication_failed(stage: FilePublicationFailureStage) -> Self {
        use FilePublicationFailureStage as E;
        let (code, summary) = match stage {
            E::PayloadTooLarge => (
                "project.editor.schema-owner-payload-too-large",
                "The prepared metadata exceeds the file owner's payload limit",
            ),
            E::OpenTarget => (
                "project.editor.schema-owner-target-unavailable",
                "The original metadata target could not be opened",
            ),
            E::CreateTemporary => (
                "project.editor.schema-owner-temporary-create-failed",
                "The temporary metadata file could not be created",
            ),
            E::WriteTemporary => (
                "project.editor.schema-owner-temporary-write-failed",
                "The temporary metadata file could not be written",
            ),
            E::SyncTemporary => (
                "project.editor.schema-owner-temporary-sync-failed",
                "The temporary metadata file could not be synchronized",
            ),
            E::ValidateTemporary => (
                "project.editor.schema-owner-temporary-validation-failed",
                "The written temporary metadata file did not validate",
            ),
            E::ReplaceTarget => (
                "project.editor.schema-owner-target-replace-failed",
                "The metadata target could not be replaced",
            ),
        };
        let mut failure = Self::new(Reason::Publication, Phase::Publishing);
        failure.diagnostics = super::super::single_error(code, summary);
        failure
    }

    pub(super) fn at_source(mut self, path: Option<&str>) -> Self {
        // The terminal envelope has the same public-text ceiling as the diagnostic subsystem.
        self.relative_source = path
            .filter(|path| path.len() <= nara_diagnostic::MAX_DIAGNOSTIC_DRAFT_TEXT_BYTES)
            .filter(|path| nara_fs::RelativePath::new(path).is_ok())
            .map(Arc::from);
        self
    }

    pub(super) fn bind_operation(&mut self, operation: Option<EditorSchemaOwnerAdoptionId>) {
        self.operation = operation;
    }
    pub(super) fn rejected(rejection: TaskRejection, phase: Phase) -> Self {
        let mut failure = Self::new(Reason::Task, phase);
        failure.task_rejection = Some(rejection);
        failure
    }
}
