//! Borrowed owning diagnostics and bounded refusal classification for process recovery.

use std::{error::Error, fmt};

use nara_diagnostic::DiagnosticReport;
use nara_scene::ScenePatchReport;

use super::super::{
    PrefabExtractionProjectError as OriginalError,
    pending::PendingConfigurationError,
    publication::{
        PrefabExtractionProjectError as ProjectError, ProjectPrefabExtractionFutureInventoryError,
        ProjectPrefabExtractionPublicationError,
    },
    recovered_generation::RecoveredPrefabExtractionGenerationError,
    recovered_inventory::RecoveredPrefabExtractionInventoryError,
    recovery_data::{PrefabExtractionRecoveryError, storage::PrefabExtractionStorageLoadError},
};
use crate::project_host::{
    authoring::{
        AuthoringProjectAdmissionError, AuthoringProjectSourceAdmissionError,
        pending_recovery::PendingAuthoringRecoveryInspection,
        recovery_storage::RecoveryStorageError, schema_models::SchemaModelsError,
        source_decode::SourceDecodeError, source_inventory::SourceInventoryError,
    },
    file_publication::{FilePublicationFailureStage, FilePublicationRejection},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrefabExtractionRecoveryErrorKind {
    WrongKind,
    Phase,
    Closed,
    HostWorkLimit,
    OpenHandleLimit,
    SourceLimit,
    Admission,
    Pending,
    Inventory,
    Schema,
    SourceDecoding,
    SourceTransition,
    SourcePublication,
    Ready,
    PublicationRejected,
    PublicationFailed,
    PublicationUncertain,
    CleanupPending,
}

impl PrefabExtractionRecoveryErrorKind {
    pub(super) fn is_capacity(self) -> bool {
        matches!(
            self,
            Self::HostWorkLimit | Self::OpenHandleLimit | Self::SourceLimit | Self::Admission
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum InitialFailure {
    #[error("the authenticated Pending package belongs to a different concrete operation")]
    WrongKind,
    #[error("the selected source allowance refused initial recovery ownership")]
    Source(ScenePatchReport),
    #[error("the authenticated Prefab extraction package could not be admitted: {0}")]
    Storage(#[source] PrefabExtractionStorageLoadError),
    #[error("initial recovery ownership exceeded its original Host allowance: {0}")]
    Admission(#[source] AuthoringProjectAdmissionError),
    #[error("initial recovery storage accounting overflowed")]
    Accounting,
}

/// A wrong known kind always returns its original authenticated inspection. Initial decoding
/// has no publisher; other initial refusals are terminal and expose their actual cause/report.
#[derive(Debug, thiserror::Error)]
#[error("Prefab extraction recovery inspection failed: {detail}")]
#[must_use]
pub struct PrefabExtractionRecoveryInspectionFailure {
    inspection: Option<PendingAuthoringRecoveryInspection>,
    #[source]
    detail: InitialFailure,
}

impl PrefabExtractionRecoveryInspectionFailure {
    pub(super) fn new(
        inspection: Option<PendingAuthoringRecoveryInspection>,
        detail: InitialFailure,
    ) -> Self {
        Self { inspection, detail }
    }

    pub fn kind(&self) -> PrefabExtractionRecoveryErrorKind {
        use PrefabExtractionRecoveryErrorKind as K;
        match &self.detail {
            InitialFailure::WrongKind => K::WrongKind,
            InitialFailure::Source(_) => K::SourceLimit,
            InitialFailure::Storage(error) => storage_kind(error),
            InitialFailure::Admission(error) => admission_kind(error),
            InitialFailure::Accounting => K::Admission,
        }
    }

    pub fn diagnostics(&self) -> Option<&DiagnosticReport> {
        match &self.detail {
            InitialFailure::Source(report) => Some(&report.diagnostics),
            _ => None,
        }
    }

    pub fn inspection(&self) -> Option<&PendingAuthoringRecoveryInspection> {
        self.inspection.as_ref()
    }

    /// Wrong-kind dispatch consumes this same transport, never another filesystem read.
    pub fn into_inspection(mut self) -> Result<PendingAuthoringRecoveryInspection, Self> {
        match self.inspection.take() {
            Some(inspection) => Ok(inspection),
            None => Err(self),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub(super) enum Failure {
    #[error("the actual mixed extraction inventory was refused: {0}")]
    Inventory(#[source] RecoveredPrefabExtractionInventoryError),
    #[error("the retained semantic generation was refused: {0}")]
    Generation(#[source] RecoveredPrefabExtractionGenerationError),
    #[error("the original extraction file publisher was refused: {0}")]
    Files(#[source] ProjectPrefabExtractionPublicationError),
    #[error("the complete future extraction inventory was refused: {0}")]
    FutureInventory(#[source] ProjectPrefabExtractionFutureInventoryError),
    #[error("the unchanged-basis future Project was refused: {0}")]
    Project(#[source] ProjectError),
    #[error("the actual Ready publisher rejected publication: {0:?}")]
    Rejected(FilePublicationRejection),
    #[error("the actual Ready publisher failed: {0:?}")]
    Failed(FilePublicationFailureStage),
    #[error("the actual Ready publisher retains unresolved physical evidence")]
    Uncertain,
}

impl Failure {
    pub(super) fn kind(&self) -> PrefabExtractionRecoveryErrorKind {
        match self {
            Self::Inventory(error) => inventory_kind(error),
            Self::Generation(error) => generation_kind(error),
            Self::Files(error) => files_kind(error),
            Self::FutureInventory(error) => future_kind(error),
            Self::Project(error) => project_kind(error),
            Self::Rejected(FilePublicationRejection::RequiresReconcile) | Self::Uncertain => {
                PrefabExtractionRecoveryErrorKind::PublicationUncertain
            }
            Self::Rejected(_) => PrefabExtractionRecoveryErrorKind::PublicationRejected,
            Self::Failed(_) => PrefabExtractionRecoveryErrorKind::PublicationFailed,
        }
    }

    fn diagnostics(&self) -> Option<&DiagnosticReport> {
        match self {
            Self::Generation(error) => generation_report(error),
            Self::Files(error) => files_report(error),
            Self::FutureInventory(error) => future_report(error),
            Self::Project(error) => project_report(error),
            _ => None,
        }
    }
}

#[derive(Clone, Copy)]
enum BorrowedFailure<'a> {
    Owned(&'a Failure),
    Generation(&'a RecoveredPrefabExtractionGenerationError),
}

/// A view into the original cause. Reports remain owned and charged by the retained stage.
#[derive(Clone, Copy)]
pub struct PrefabExtractionRecoveryFailure<'a> {
    detail: BorrowedFailure<'a>,
}

impl<'a> PrefabExtractionRecoveryFailure<'a> {
    pub(super) fn owned(failure: &'a Failure) -> Self {
        Self {
            detail: BorrowedFailure::Owned(failure),
        }
    }
    pub(super) fn generation(failure: &'a RecoveredPrefabExtractionGenerationError) -> Self {
        Self {
            detail: BorrowedFailure::Generation(failure),
        }
    }
    pub fn kind(self) -> PrefabExtractionRecoveryErrorKind {
        match self.detail {
            BorrowedFailure::Owned(error) => error.kind(),
            BorrowedFailure::Generation(error) => generation_kind(error),
        }
    }
    pub fn diagnostics(self) -> Option<&'a DiagnosticReport> {
        match self.detail {
            BorrowedFailure::Owned(error) => error.diagnostics(),
            BorrowedFailure::Generation(error) => generation_report(error),
        }
    }
}

impl fmt::Display for PrefabExtractionRecoveryFailure<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.detail {
            BorrowedFailure::Owned(error) => fmt::Display::fmt(error, formatter),
            BorrowedFailure::Generation(error) => fmt::Display::fmt(error, formatter),
        }
    }
}
impl fmt::Debug for PrefabExtractionRecoveryFailure<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.detail {
            BorrowedFailure::Owned(error) => fmt::Debug::fmt(error, formatter),
            BorrowedFailure::Generation(error) => fmt::Debug::fmt(error, formatter),
        }
    }
}
impl Error for PrefabExtractionRecoveryFailure<'_> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self.detail {
            BorrowedFailure::Owned(error) => Some(error),
            BorrowedFailure::Generation(error) => Some(error),
        }
    }
}

fn admission_kind(error: &AuthoringProjectAdmissionError) -> PrefabExtractionRecoveryErrorKind {
    use PrefabExtractionRecoveryErrorKind as K;
    match error {
        AuthoringProjectAdmissionError::DecodeWorkExceeded { .. } => K::HostWorkLimit,
        AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { .. } => K::OpenHandleLimit,
        _ => K::Admission,
    }
}
fn source_kind(error: &AuthoringProjectSourceAdmissionError) -> PrefabExtractionRecoveryErrorKind {
    match error {
        AuthoringProjectSourceAdmissionError::Budget(error) => admission_kind(error),
        AuthoringProjectSourceAdmissionError::CompilationBudget(_) => {
            PrefabExtractionRecoveryErrorKind::SourceLimit
        }
        _ => PrefabExtractionRecoveryErrorKind::SourceDecoding,
    }
}
fn native_kind(error: &SourceInventoryError) -> PrefabExtractionRecoveryErrorKind {
    match error {
        SourceInventoryError::Admission(error) => admission_kind(error),
        SourceInventoryError::Native(error) => source_kind(error),
        _ => PrefabExtractionRecoveryErrorKind::Inventory,
    }
}
fn models_kind(error: &SchemaModelsError) -> PrefabExtractionRecoveryErrorKind {
    match error {
        SchemaModelsError::Admission(error) => admission_kind(error),
        _ => PrefabExtractionRecoveryErrorKind::Schema,
    }
}
fn pending_kind(error: &RecoveryStorageError) -> PrefabExtractionRecoveryErrorKind {
    match error {
        RecoveryStorageError::Admission(error) => admission_kind(error),
        _ => PrefabExtractionRecoveryErrorKind::Pending,
    }
}
fn data_kind(error: &PrefabExtractionRecoveryError) -> PrefabExtractionRecoveryErrorKind {
    match error {
        PrefabExtractionRecoveryError::Admission(error) => admission_kind(error),
        _ => PrefabExtractionRecoveryErrorKind::SourceDecoding,
    }
}
fn storage_kind(error: &PrefabExtractionStorageLoadError) -> PrefabExtractionRecoveryErrorKind {
    match error {
        PrefabExtractionStorageLoadError::Storage(error) => pending_kind(error),
        PrefabExtractionStorageLoadError::Data(error) => data_kind(error),
    }
}
fn inventory_kind(
    error: &RecoveredPrefabExtractionInventoryError,
) -> PrefabExtractionRecoveryErrorKind {
    match error {
        RecoveredPrefabExtractionInventoryError::Inventory(error) => native_kind(error),
        RecoveredPrefabExtractionInventoryError::Pending(error) => pending_kind(error),
        RecoveredPrefabExtractionInventoryError::Schema(error) => models_kind(error),
        _ => PrefabExtractionRecoveryErrorKind::Inventory,
    }
}
fn original_kind(error: &OriginalError) -> PrefabExtractionRecoveryErrorKind {
    match error {
        OriginalError::SourceWork(report) if source_capacity(&report.diagnostics) => {
            PrefabExtractionRecoveryErrorKind::SourceLimit
        }
        OriginalError::Admission(error) => admission_kind(error),
        OriginalError::Project(error) => source_kind(error),
        OriginalError::Inventory(error) => native_kind(error),
        OriginalError::Decode(error) => decode_kind(error),
        _ => PrefabExtractionRecoveryErrorKind::SourceTransition,
    }
}
fn decode_kind(error: &SourceDecodeError) -> PrefabExtractionRecoveryErrorKind {
    match error {
        SourceDecodeError::Admission(error) => admission_kind(error),
        SourceDecodeError::Project(error) => source_kind(error),
        _ => PrefabExtractionRecoveryErrorKind::SourceDecoding,
    }
}
fn generation_kind(
    error: &RecoveredPrefabExtractionGenerationError,
) -> PrefabExtractionRecoveryErrorKind {
    use RecoveredPrefabExtractionGenerationError as G;
    if generation_report(error).is_some_and(source_capacity) {
        return PrefabExtractionRecoveryErrorKind::SourceLimit;
    }
    match error {
        G::Inventory(error) => inventory_kind(error),
        G::Source(error) => original_kind(error),
        G::Decode(error) => decode_kind(error),
        G::Data(error) => data_kind(error),
        _ => PrefabExtractionRecoveryErrorKind::SourceTransition,
    }
}
fn files_kind(
    error: &ProjectPrefabExtractionPublicationError,
) -> PrefabExtractionRecoveryErrorKind {
    use PrefabExtractionRecoveryErrorKind as K;
    use ProjectPrefabExtractionPublicationError as F;
    if files_report(error).is_some_and(source_capacity) {
        return K::SourceLimit;
    }
    match error {
        F::Admission(error) => admission_kind(error),
        F::Pending(error) => pending_kind(error),
        F::Inventory(error) => native_kind(error),
        F::Generation(error) => generation_kind(error),
        F::Uncertain | F::Rejected(FilePublicationRejection::RequiresReconcile) => {
            K::PublicationUncertain
        }
        F::Rejected(_) => K::PublicationRejected,
        F::Failed(_) => K::PublicationFailed,
        F::Phase => K::Phase,
        _ => K::SourcePublication,
    }
}
fn future_kind(
    error: &ProjectPrefabExtractionFutureInventoryError,
) -> PrefabExtractionRecoveryErrorKind {
    match error {
        ProjectPrefabExtractionFutureInventoryError::Publication(error) => files_kind(error),
        ProjectPrefabExtractionFutureInventoryError::Inventory(error)
        | ProjectPrefabExtractionFutureInventoryError::Configuration(
            PendingConfigurationError::Inventory(error),
        ) => native_kind(error),
        _ => PrefabExtractionRecoveryErrorKind::Inventory,
    }
}
fn project_kind(error: &ProjectError) -> PrefabExtractionRecoveryErrorKind {
    match error {
        ProjectError::Admission(error) => admission_kind(error),
        ProjectError::Models(error) => models_kind(error),
        ProjectError::Source(error) => source_kind(error),
        ProjectError::NativeInventory(error) => native_kind(error),
        ProjectError::Publication(error) => files_kind(error),
        ProjectError::Inventory(error) => future_kind(error),
        _ => PrefabExtractionRecoveryErrorKind::Ready,
    }
}
fn source_capacity(report: &DiagnosticReport) -> bool {
    report.iter().any(|diagnostic| {
        matches!(
            diagnostic.code().as_str(),
            "scene.prefab-extraction-capacity" | "scene.prefab-expansion-budget-exceeded"
        )
    })
}
fn original_report(error: &OriginalError) -> Option<&DiagnosticReport> {
    match error {
        OriginalError::SourceWork(report) => Some(&report.diagnostics),
        OriginalError::Project(AuthoringProjectSourceAdmissionError::CompilationBudget(report)) => {
            Some(report)
        }
        _ => None,
    }
}
fn generation_report(
    error: &RecoveredPrefabExtractionGenerationError,
) -> Option<&DiagnosticReport> {
    use RecoveredPrefabExtractionGenerationError as G;
    match error {
        G::Source(error) => original_report(error),
        G::SourceWork(report) | G::Correspondence { report, .. } => Some(&report.diagnostics),
        G::SavedGeneration(error) => Some(&error.report().diagnostics),
        G::Decode(SourceDecodeError::Project(
            AuthoringProjectSourceAdmissionError::CompilationBudget(report),
        )) => Some(report),
        _ => None,
    }
}
fn files_report(error: &ProjectPrefabExtractionPublicationError) -> Option<&DiagnosticReport> {
    match error {
        ProjectPrefabExtractionPublicationError::Generation(error) => generation_report(error),
        #[cfg(feature = "tooling")]
        ProjectPrefabExtractionPublicationError::Workspace(report) => Some(&report.diagnostics),
        #[cfg(feature = "tooling")]
        ProjectPrefabExtractionPublicationError::Checkpoint(
            crate::project_host::persistence::ExtractionPersistenceError::Admission(report),
        ) => Some(&report.diagnostics),
        _ => None,
    }
}
fn future_report(error: &ProjectPrefabExtractionFutureInventoryError) -> Option<&DiagnosticReport> {
    match error {
        ProjectPrefabExtractionFutureInventoryError::Publication(error) => files_report(error),
        _ => None,
    }
}
fn project_report(error: &ProjectError) -> Option<&DiagnosticReport> {
    match error {
        ProjectError::Publication(error) => files_report(error),
        ProjectError::Inventory(error) => future_report(error),
        ProjectError::Source(AuthoringProjectSourceAdmissionError::CompilationBudget(report)) => {
            Some(report)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use nara_core::ItemLimit;
    use nara_reflect::ComponentRegistryBuilder;
    use nara_scene::{
        InMemoryPrefabSourceResolver, SceneCompilationLimits, SceneCompilationOptions,
        SceneDocument, SceneEntityId, SceneEntityRecord,
    };

    use super::*;

    #[test]
    fn ordinary_compiler_budget_remains_capacity_through_the_saved_project_error() {
        let registry = ComponentRegistryBuilder::new().freeze().unwrap();
        let document = SceneDocument::from_ordered_roots(
            ["first", "second"]
                .map(|name| SceneEntityRecord::new(SceneEntityId::new(name).unwrap())),
        );
        let compiled = document.compile_with_options(
            &registry,
            &InMemoryPrefabSourceResolver::new(),
            SceneCompilationOptions::default().with_limits(
                SceneCompilationLimits::default().with_materialized_entities(ItemLimit::ONE),
            ),
        );
        assert!(compiled.candidate.is_none());
        let failure = Failure::Project(ProjectError::Source(
            AuthoringProjectSourceAdmissionError::CompilationBudget(compiled.diagnostics),
        ));
        let borrowed = PrefabExtractionRecoveryFailure::owned(&failure);
        assert_eq!(
            borrowed.kind(),
            PrefabExtractionRecoveryErrorKind::SourceLimit
        );
        assert!(borrowed.kind().is_capacity());
        assert!(borrowed.diagnostics().unwrap().iter().any(|diagnostic| {
            diagnostic.code().as_str() == "scene.prefab-expansion-budget-exceeded"
        }));
        assert!(std::ptr::eq(
            borrowed.diagnostics().unwrap(),
            failure.diagnostics().unwrap()
        ));
    }
}
