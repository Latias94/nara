//! Exact unpublished refusal custody. Reports remain in the owner that produced them.

use super::super::recovery_data::PrefabExtractionRecoveryError;
use super::*;

/// Terminal capture/corpus errors have no revisable owner. Later refusals keep the original
/// Review, checkpoint-return state or recovery candidate until revision or explicit disposal.
#[derive(Debug, thiserror::Error)]
#[error("{error}")]
#[must_use]
pub(crate) struct PrefabExtractionSessionPreparationFailure {
    pub(super) error: PrefabExtractionSessionPreparationError,
    pub(super) task_storage: usize,
    pub(super) selection_storage: usize,
}

#[derive(Debug)]
pub(super) enum RetainedPreparation {
    Review(ProjectPrefabExtractionReviewFailure),
    Outputs(ProjectPrefabExtractionOutputFailure),
    ReturningOutputs(ProjectPrefabExtractionOutputCancelFailure),
    Recovery(ProjectPrefabExtractionRecoveryFailure),
    Ready(PreparedProjectPrefabExtractionRecovery),
}

impl PrefabExtractionSessionPreparationFailure {
    #[cfg(test)]
    pub(crate) fn test_prepared(&self) -> Option<&PreparedEditorWorkspacePrefabExtraction> {
        let review = match self.error.retained.as_ref()? {
            RetainedPreparation::Review(ProjectPrefabExtractionReviewFailure::Saved {
                persistence,
                ..
            }) => return Some(persistence.sources().prepared()),
            RetainedPreparation::Review(ProjectPrefabExtractionReviewFailure::Revision {
                review,
            }) => review,
            RetainedPreparation::Review(_) => return None,
            RetainedPreparation::Outputs(failure) => failure.outputs().review(),
            RetainedPreparation::ReturningOutputs(failure) => failure.outputs().review(),
            RetainedPreparation::Recovery(failure) => failure.prepared().outputs().review(),
            RetainedPreparation::Ready(recovery) => recovery.outputs().review(),
        };
        Some(review.persistence.sources().prepared())
    }

    pub(super) fn terminal(error: PrefabExtractionSessionPreparationError) -> Self {
        debug_assert!(error.retained.is_none());
        Self {
            error,
            task_storage: 0,
            selection_storage: 0,
        }
    }

    pub(super) fn retained(
        preparation: RetainedPreparation,
        task_storage: usize,
        selection_storage: usize,
    ) -> Self {
        debug_assert!(selection_storage == 0 || preparation.retains_selection());
        let capacity = preparation.admission().and_then(capacity_cause);
        Self {
            error: PrefabExtractionSessionPreparationError {
                phase: preparation.phase(),
                capacity,
                diagnostics: None,
                retained: Some(preparation),
            },
            task_storage,
            selection_storage,
        }
    }

    pub(crate) fn error(&self) -> &PrefabExtractionSessionPreparationError {
        &self.error
    }

    pub(crate) fn diagnostics(&self) -> Option<&DiagnosticReport> {
        self.error.diagnostics()
    }

    pub(crate) fn has_retained_preparation(&self) -> bool {
        self.error.retained.is_some()
    }

    /// This describes retained custody, not a promise that exhausted cumulative work can retry.
    pub(crate) fn can_revise(&self) -> bool {
        self.has_retained_preparation()
    }

    pub(crate) fn source_count(&self) -> usize {
        self.context()
            .map_or(0, |context| context.inventory.participants().len())
    }

    pub(crate) fn open_count(&self) -> usize {
        self.context().map_or(0, |context| context.open.len())
    }

    pub(crate) fn matches(&self, project: &AuthoringProjectSessionSnapshot) -> bool {
        self.context()
            .is_some_and(|context| Arc::ptr_eq(&context.schema, &project.schema))
    }

    pub(crate) fn schema_basis(&self) -> Option<&AuthoringSchemaBasis> {
        self.context().map(|context| &context.basis)
    }

    pub(crate) fn host_usage(&self) -> Option<AuthoringProjectAdmissionUsage> {
        self.context().map(|context| context.budget.usage())
    }

    pub(super) fn context(&self) -> Option<&ProjectExtractionContext> {
        self.error
            .retained
            .as_ref()
            .map(RetainedPreparation::context)
    }

    /// An actual Task return precedes this call. Only terminal failures whose complete context
    /// has already dropped need no refund; every retained candidate uses its original budget.
    pub(crate) fn release_task_storage(&mut self, bytes: usize) {
        let Some(preparation) = self.error.retained.as_mut() else {
            return;
        };
        release_storage(
            &mut preparation.context_mut().budget,
            &mut self.task_storage,
            bytes,
        );
    }

    /// Moving terminal diagnostics cannot silently dispose of an editable preparation.
    pub(crate) fn into_error(self) -> Result<PrefabExtractionSessionPreparationError, Self> {
        if self.has_retained_preparation() {
            Err(self)
        } else {
            Ok(self.error)
        }
    }
}

impl RetainedPreparation {
    pub(super) fn retains_selection(&self) -> bool {
        matches!(
            self,
            Self::Review(ProjectPrefabExtractionReviewFailure::Persistence { .. })
        )
    }

    pub(super) fn context(&self) -> &ProjectExtractionContext {
        match self {
            Self::Review(failure) => match failure {
                ProjectPrefabExtractionReviewFailure::Sources { context, .. }
                | ProjectPrefabExtractionReviewFailure::Persistence { context, .. }
                | ProjectPrefabExtractionReviewFailure::Saved { context, .. } => context,
                ProjectPrefabExtractionReviewFailure::Revision { review } => &review.context,
            },
            Self::Outputs(failure) => &failure.outputs().review().context,
            Self::ReturningOutputs(failure) => &failure.outputs().review().context,
            Self::Recovery(failure) => &failure.prepared().outputs().review().context,
            Self::Ready(recovery) => &recovery.outputs().review().context,
        }
    }

    pub(super) fn context_mut(&mut self) -> &mut ProjectExtractionContext {
        match self {
            Self::Review(failure) => match failure {
                ProjectPrefabExtractionReviewFailure::Sources { context, .. }
                | ProjectPrefabExtractionReviewFailure::Persistence { context, .. }
                | ProjectPrefabExtractionReviewFailure::Saved { context, .. } => context,
                ProjectPrefabExtractionReviewFailure::Revision { review } => &mut review.context,
            },
            Self::Outputs(failure) => failure.outputs_mut().session_context_mut(),
            Self::ReturningOutputs(failure) => failure.outputs_mut().session_context_mut(),
            Self::Recovery(failure) => &mut failure.prepared_mut().review_mut().context,
            Self::Ready(recovery) => &mut recovery.review_mut().context,
        }
    }

    fn phase(&self) -> PreparationPhase {
        match self {
            Self::Review(_) | Self::Ready(_) => PreparationPhase::Review,
            Self::Outputs(_) | Self::ReturningOutputs(_) => PreparationPhase::Outputs,
            Self::Recovery(_) => PreparationPhase::Recovery,
        }
    }

    pub(super) fn diagnostics(&self) -> Option<&DiagnosticReport> {
        match self {
            Self::Review(failure) => match failure {
                ProjectPrefabExtractionReviewFailure::Sources { failure, .. } => {
                    Some(&failure.report().diagnostics)
                }
                ProjectPrefabExtractionReviewFailure::Persistence { failure, .. } => {
                    Some(&failure.report().diagnostics)
                }
                ProjectPrefabExtractionReviewFailure::Saved { error, .. } => match error {
                    EditorWorkspaceExtractionSavedGenerationFailure::Workspace(report) => {
                        Some(&report.diagnostics)
                    }
                    EditorWorkspaceExtractionSavedGenerationFailure::Scene(error) => {
                        Some(&error.report().diagnostics)
                    }
                },
                ProjectPrefabExtractionReviewFailure::Revision { .. } => None,
            },
            Self::Outputs(failure) => output_diagnostics(failure.error()),
            Self::ReturningOutputs(failure) => output_diagnostics(failure.error()),
            Self::Recovery(failure) => match failure.error() {
                ProjectPrefabExtractionRecoveryError::Output(error) => output_diagnostics(error),
                ProjectPrefabExtractionRecoveryError::Data(_) => None,
            },
            Self::Ready(_) => None,
        }
    }

    fn admission(&self) -> Option<&AuthoringProjectAdmissionError> {
        match self {
            Self::Outputs(failure) => output_admission(failure.error()),
            Self::ReturningOutputs(failure) => output_admission(failure.error()),
            Self::Recovery(failure) => match failure.error() {
                ProjectPrefabExtractionRecoveryError::Output(error) => output_admission(error),
                ProjectPrefabExtractionRecoveryError::Data(
                    PrefabExtractionRecoveryError::Admission(error),
                ) => Some(error),
                _ => None,
            },
            Self::Review(_) | Self::Ready(_) => None,
        }
    }
}

fn capacity_cause(error: &AuthoringProjectAdmissionError) -> Option<PreparationCapacityCause> {
    match error {
        AuthoringProjectAdmissionError::DecodeWorkExceeded { .. } => {
            Some(PreparationCapacityCause::WorkLimit)
        }
        AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { .. } => {
            Some(PreparationCapacityCause::OpenHandles)
        }
        _ => None,
    }
}

fn output_admission(
    error: &ProjectPrefabExtractionOutputError,
) -> Option<&AuthoringProjectAdmissionError> {
    let ProjectPrefabExtractionOutputError::Project(error) = error else {
        return None;
    };
    match error {
        PrefabExtractionProjectError::Admission(error)
        | PrefabExtractionProjectError::Inventory(SourceInventoryError::Admission(error))
        | PrefabExtractionProjectError::Decode(SourceDecodeError::Admission(error))
        | PrefabExtractionProjectError::Project(AuthoringProjectSourceAdmissionError::Budget(
            error,
        ))
        | PrefabExtractionProjectError::Inventory(SourceInventoryError::Native(
            AuthoringProjectSourceAdmissionError::Budget(error),
        ))
        | PrefabExtractionProjectError::Decode(SourceDecodeError::Project(
            AuthoringProjectSourceAdmissionError::Budget(error),
        )) => Some(error),
        _ => None,
    }
}

fn output_diagnostics(error: &ProjectPrefabExtractionOutputError) -> Option<&DiagnosticReport> {
    match error {
        ProjectPrefabExtractionOutputError::Workspace(report) => Some(&report.diagnostics),
        ProjectPrefabExtractionOutputError::Open(error) => Some(&error.report().diagnostics),
        ProjectPrefabExtractionOutputError::Source(SceneFileEncodingAdmissionError::Admission(
            report,
        )) => Some(&report.diagnostics),
        ProjectPrefabExtractionOutputError::Project(error) => match error {
            PrefabExtractionProjectError::SourceWork(report) => Some(&report.diagnostics),
            PrefabExtractionProjectError::Project(error)
            | PrefabExtractionProjectError::Inventory(SourceInventoryError::Native(error))
            | PrefabExtractionProjectError::Decode(SourceDecodeError::Project(error)) => {
                source_diagnostics(error)
            }
            PrefabExtractionProjectError::Decode(SourceDecodeError::Structure(error)) => {
                Some(error.diagnostics())
            }
            _ => None,
        },
        _ => None,
    }
}

fn source_diagnostics(error: &AuthoringProjectSourceAdmissionError) -> Option<&DiagnosticReport> {
    match error {
        AuthoringProjectSourceAdmissionError::CompilationBudget(report) => Some(report),
        AuthoringProjectSourceAdmissionError::SourceStructure { source, .. } => {
            Some(source.diagnostics())
        }
        AuthoringProjectSourceAdmissionError::SourcePublication { source, .. } => {
            Some(source.diagnostics())
        }
        _ => None,
    }
}
