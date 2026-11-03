//! Preserve concrete failures without exporting private storage/publication transport types.

use nara_diagnostic::DiagnosticReport;
use nara_tooling::{EditorPersistenceRejection, EditorWorkspaceCommandReport};

use super::super::{
    outputs::{ProjectPrefabExtractionOutputError, storage::ProjectPrefabExtractionStorageError},
    pending::PendingConfigurationError,
    publication::{
        PrefabExtractionInstallationError, PrefabExtractionProjectError as FutureProjectError,
        ProjectPrefabExtractionFutureInventoryError, ProjectPrefabExtractionPublicationError,
    },
    recovery_data::PrefabExtractionRecoveryError,
};
use super::*;
use crate::project_host::{
    authoring::{
        document::AuthoringDocumentAdmissionError, recovery_storage::RecoveryStorageError,
        schema_models::SchemaModelsError, source_inventory::SourceInventoryError,
    },
    file_publication::FilePublicationRejection,
};

#[derive(Debug, thiserror::Error)]
#[error("Prefab extraction publication failed: {cause}")]
pub(crate) struct PrefabExtractionSessionPublicationError {
    cause: Failure,
}

#[derive(Debug, thiserror::Error)]
pub(super) enum Failure {
    #[error("the complete original extraction inputs changed")]
    Original(PrefabExtractionProjectError),
    #[error("recovery storage refused its original preparation: {0:?}")]
    Storage(ProjectPrefabExtractionStorageError),
    #[error("extraction files or checkpoints refused publication")]
    Files(ProjectPrefabExtractionPublicationError),
    #[error("the complete future inventory refused admission")]
    Inventory(ProjectPrefabExtractionFutureInventoryError),
    #[error("the unchanged-basis future Project refused preparation")]
    Project(FutureProjectError),
    #[error("the original all-bound Workspace installation was refused")]
    Installation(PrefabExtractionInstallationError),
    #[error("the live Workspace refused its exact prepared checkpoint")]
    Workspace(EditorWorkspaceCommandReport),
    #[error("an original persistence destination was refused: {0:?}")]
    Persistence(EditorPersistenceRejection),
    #[error("the original document source fact could not retain its actual published observation")]
    Document(AuthoringDocumentAdmissionError),
    #[error("the original operation allowance was exceeded")]
    Admission(AuthoringProjectAdmissionError),
    #[error("operation storage accounting was refused")]
    Accounting(AuthoringProjectSourceAdmissionError),
    #[error("the recovery package publication identity could not be generated")]
    RecoveryIdentity(getrandom::Error),
    #[error("native cleanup remains physically pending")]
    CleanupPending,
    #[error("the finite publication Task was not started")]
    TaskSuspended,
    #[error("the exact publication owner is not in the required phase")]
    Phase,
}

impl From<Failure> for PrefabExtractionSessionPublicationError {
    fn from(cause: Failure) -> Self {
        Self { cause }
    }
}

impl From<SourceInventoryError> for Failure {
    fn from(error: SourceInventoryError) -> Self {
        Self::Original(error.into())
    }
}

impl From<AuthoringProjectAdmissionError> for PrefabExtractionSessionPublicationError {
    fn from(error: AuthoringProjectAdmissionError) -> Self {
        Failure::Admission(error).into()
    }
}

impl From<AuthoringProjectSourceAdmissionError> for PrefabExtractionSessionPublicationError {
    fn from(error: AuthoringProjectSourceAdmissionError) -> Self {
        Failure::Accounting(error).into()
    }
}

impl From<EditorWorkspaceCommandReport> for PrefabExtractionSessionPublicationError {
    fn from(report: EditorWorkspaceCommandReport) -> Self {
        Failure::Workspace(report).into()
    }
}

impl From<EditorPersistenceRejection> for PrefabExtractionSessionPublicationError {
    fn from(reason: EditorPersistenceRejection) -> Self {
        Failure::Persistence(reason).into()
    }
}

impl PrefabExtractionSessionPublicationError {
    /// Borrow the owning report rather than cloning it into a second Session allowance.
    pub(crate) fn diagnostics(&self) -> Option<&DiagnosticReport> {
        match &self.cause {
            Failure::Original(error) => original_diagnostics(error),
            Failure::Storage(ProjectPrefabExtractionStorageError::Output(error)) => match error {
                ProjectPrefabExtractionOutputError::Project(error) => original_diagnostics(error),
                ProjectPrefabExtractionOutputError::Workspace(report) => Some(&report.diagnostics),
                _ => None,
            },
            Failure::Files(error) => files_diagnostics(error),
            Failure::Inventory(ProjectPrefabExtractionFutureInventoryError::Publication(error)) => {
                files_diagnostics(error)
            }
            Failure::Project(error) => project_diagnostics(error),
            Failure::Installation(PrefabExtractionInstallationError::Project(error)) => {
                project_diagnostics(error)
            }
            Failure::Workspace(report) => Some(&report.diagnostics),
            _ => None,
        }
    }

    pub(crate) fn source_limit_exceeded(&self) -> bool {
        self.diagnostics().is_some_and(|report| {
            report
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == "scene.prefab-extraction-capacity")
        })
    }

    /// Native failures without an owning report still keep their bounded operational cause.
    pub(crate) fn diagnostic(&self) -> (&'static str, &'static str) {
        if self.uncertain() {
            return (
                "project.editor.prefab-extraction-publication-uncertain",
                "The actual extraction publisher has uncertain replacement evidence; retain the original operation for recovery",
            );
        }
        match &self.cause {
            Failure::Original(_) => (
                "project.editor.prefab-extraction-original-changed",
                "The complete original extraction inputs could not be revalidated",
            ),
            Failure::CleanupPending
            | Failure::Storage(ProjectPrefabExtractionStorageError::CleanupPending) => (
                "project.editor.prefab-extraction-cleanup-pending",
                "The extraction's original native publisher still owns incomplete cleanup",
            ),
            Failure::Storage(_) | Failure::RecoveryIdentity(_) => (
                "project.editor.prefab-extraction-recovery-publication-refused",
                "The actual recovery package or Pending manifest publisher refused its retained operation",
            ),
            Failure::Files(_) => (
                "project.editor.prefab-extraction-source-publication-refused",
                "An actual extraction file or saved checkpoint refused publication",
            ),
            Failure::Inventory(_) => (
                "project.editor.prefab-extraction-future-inventory-refused",
                "The complete future extraction inventory could not be admitted from its actual published files",
            ),
            Failure::Project(_) => (
                "project.editor.prefab-extraction-future-project-refused",
                "The saved future Project could not be prepared under the unchanged Schema basis",
            ),
            Failure::Installation(_) => (
                "project.editor.prefab-extraction-ready-publication-refused",
                "The final all-bound extraction installation or actual Ready publisher was refused",
            ),
            Failure::Workspace(_) => (
                "project.editor.prefab-extraction-workspace-refused",
                "The live Workspace refused its original complete extraction checkpoints",
            ),
            Failure::Persistence(_) | Failure::Document(_) => (
                "project.editor.prefab-extraction-persistence-refused",
                "An original persistence destination or its actual native source observation was refused",
            ),
            Failure::Admission(_) | Failure::Accounting(_) => (
                "project.editor.prefab-extraction-allowance-refused",
                "The original extraction allowance refused further publication storage or work",
            ),
            Failure::TaskSuspended => (
                "project.editor.prefab-extraction-task-unavailable",
                "The finite publication Task did not start; its original move-only owner remains available",
            ),
            Failure::Phase => (
                "project.editor.prefab-extraction-publication-phase",
                "The exact extraction owner is not in the required publication phase",
            ),
        }
    }

    pub(super) fn admission(&self) -> Option<&AuthoringProjectAdmissionError> {
        match &self.cause {
            Failure::Original(error) => original_admission(error),
            Failure::Storage(error) => storage_admission(error),
            Failure::Files(error) => files_admission(error),
            Failure::Inventory(error) => future_inventory_admission(error),
            Failure::Project(error) => project_admission(error),
            Failure::Installation(PrefabExtractionInstallationError::Project(error)) => {
                project_admission(error)
            }
            Failure::Admission(error) => Some(error),
            Failure::Accounting(error) => source_admission(error),
            _ => None,
        }
    }

    pub(super) fn terminal(&self) -> bool {
        self.admission().is_some()
            || self.source_limit_exceeded()
            || self.uncertain()
            || matches!(
                self.cause,
                Failure::Accounting(AuthoringProjectSourceAdmissionError::AccountingOverflow)
                    | Failure::Original(PrefabExtractionProjectError::SourceWork(_))
                    | Failure::Phase
            )
    }

    pub(super) fn uncertain(&self) -> bool {
        use FilePublicationRejection::RequiresReconcile;
        match &self.cause {
            Failure::Storage(
                ProjectPrefabExtractionStorageError::Uncertain
                | ProjectPrefabExtractionStorageError::Rejected(RequiresReconcile)
                | ProjectPrefabExtractionStorageError::Storage(
                    RecoveryStorageError::MissingPublisher(RequiresReconcile),
                ),
            ) => true,
            Failure::Files(error) => files_uncertain(error),
            Failure::Inventory(ProjectPrefabExtractionFutureInventoryError::Publication(error)) => {
                files_uncertain(error)
            }
            Failure::Project(error) => project_uncertain(error),
            Failure::Installation(error) => match error {
                PrefabExtractionInstallationError::Uncertain(_)
                | PrefabExtractionInstallationError::Rejected(RequiresReconcile)
                | PrefabExtractionInstallationError::Persistence(
                    EditorPersistenceRejection::RequiresReconcile,
                ) => true,
                PrefabExtractionInstallationError::Project(error) => project_uncertain(error),
                _ => false,
            },
            Failure::Persistence(EditorPersistenceRejection::RequiresReconcile) => true,
            _ => false,
        }
    }
}

fn original_diagnostics(error: &PrefabExtractionProjectError) -> Option<&DiagnosticReport> {
    match error {
        PrefabExtractionProjectError::SourceWork(report) => Some(&report.diagnostics),
        PrefabExtractionProjectError::Project(
            AuthoringProjectSourceAdmissionError::CompilationBudget(report),
        ) => Some(report),
        _ => None,
    }
}

fn files_diagnostics(error: &ProjectPrefabExtractionPublicationError) -> Option<&DiagnosticReport> {
    match error {
        ProjectPrefabExtractionPublicationError::Workspace(report) => Some(&report.diagnostics),
        ProjectPrefabExtractionPublicationError::Checkpoint(
            crate::project_host::persistence::ExtractionPersistenceError::Admission(report),
        ) => Some(&report.diagnostics),
        _ => None,
    }
}

fn project_diagnostics(error: &FutureProjectError) -> Option<&DiagnosticReport> {
    match error {
        FutureProjectError::Publication(error)
        | FutureProjectError::Inventory(
            ProjectPrefabExtractionFutureInventoryError::Publication(error),
        ) => files_diagnostics(error),
        FutureProjectError::Source(AuthoringProjectSourceAdmissionError::CompilationBudget(
            report,
        )) => Some(report),
        _ => None,
    }
}

fn source_admission(
    error: &AuthoringProjectSourceAdmissionError,
) -> Option<&AuthoringProjectAdmissionError> {
    match error {
        AuthoringProjectSourceAdmissionError::Budget(error) => Some(error),
        _ => None,
    }
}

fn inventory_admission(error: &SourceInventoryError) -> Option<&AuthoringProjectAdmissionError> {
    match error {
        SourceInventoryError::Admission(error) => Some(error),
        SourceInventoryError::Native(error) => source_admission(error),
        _ => None,
    }
}

fn original_admission(
    error: &PrefabExtractionProjectError,
) -> Option<&AuthoringProjectAdmissionError> {
    match error {
        PrefabExtractionProjectError::Admission(error) => Some(error),
        PrefabExtractionProjectError::Project(error) => source_admission(error),
        PrefabExtractionProjectError::Inventory(error) => inventory_admission(error),
        PrefabExtractionProjectError::Decode(SourceDecodeError::Admission(error)) => Some(error),
        PrefabExtractionProjectError::Decode(SourceDecodeError::Project(error)) => {
            source_admission(error)
        }
        _ => None,
    }
}

fn storage_admission(
    error: &ProjectPrefabExtractionStorageError,
) -> Option<&AuthoringProjectAdmissionError> {
    match error {
        ProjectPrefabExtractionStorageError::Storage(RecoveryStorageError::Admission(error))
        | ProjectPrefabExtractionStorageError::Data(PrefabExtractionRecoveryError::Admission(
            error,
        )) => Some(error),
        ProjectPrefabExtractionStorageError::Output(
            ProjectPrefabExtractionOutputError::Project(error),
        ) => original_admission(error),
        _ => None,
    }
}

fn files_admission(
    error: &ProjectPrefabExtractionPublicationError,
) -> Option<&AuthoringProjectAdmissionError> {
    match error {
        ProjectPrefabExtractionPublicationError::Admission(error)
        | ProjectPrefabExtractionPublicationError::Pending(RecoveryStorageError::Admission(
            error,
        )) => Some(error),
        ProjectPrefabExtractionPublicationError::Inventory(error) => inventory_admission(error),
        ProjectPrefabExtractionPublicationError::Installation(error) => storage_admission(error),
        _ => None,
    }
}

fn future_inventory_admission(
    error: &ProjectPrefabExtractionFutureInventoryError,
) -> Option<&AuthoringProjectAdmissionError> {
    match error {
        ProjectPrefabExtractionFutureInventoryError::Publication(error) => files_admission(error),
        ProjectPrefabExtractionFutureInventoryError::Inventory(error)
        | ProjectPrefabExtractionFutureInventoryError::Configuration(
            PendingConfigurationError::Inventory(error),
        ) => inventory_admission(error),
        _ => None,
    }
}

fn project_admission(error: &FutureProjectError) -> Option<&AuthoringProjectAdmissionError> {
    match error {
        FutureProjectError::Admission(error)
        | FutureProjectError::Models(SchemaModelsError::Admission(error)) => Some(error),
        FutureProjectError::Source(error) => source_admission(error),
        FutureProjectError::Inventory(error) => future_inventory_admission(error),
        FutureProjectError::Publication(error) => files_admission(error),
        FutureProjectError::NativeInventory(error) => inventory_admission(error),
        _ => None,
    }
}

fn files_uncertain(error: &ProjectPrefabExtractionPublicationError) -> bool {
    matches!(
        error,
        ProjectPrefabExtractionPublicationError::Uncertain
            | ProjectPrefabExtractionPublicationError::Rejected(
                FilePublicationRejection::RequiresReconcile
            )
    )
}

fn project_uncertain(error: &FutureProjectError) -> bool {
    match error {
        FutureProjectError::Publication(error)
        | FutureProjectError::Inventory(
            ProjectPrefabExtractionFutureInventoryError::Publication(error),
        ) => files_uncertain(error),
        _ => false,
    }
}
