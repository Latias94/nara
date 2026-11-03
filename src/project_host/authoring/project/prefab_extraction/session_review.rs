//! Session preparation over the actual capture, Review, fixed outputs and recovery owners.
//!
//! This bridge retains the original schema authority and cumulative allowances. It neither
//! publishes files nor changes live Workspace sources, history or saved checkpoints.

use nara_diagnostic::DiagnosticReport;
use nara_project::ProjectAuthoringRecoveryDigest;
use nara_scene::SceneFileEncodingAdmissionError;
use nara_tooling::{
    EditorPrefabExtractionDisposition, EditorWorkspace, EditorWorkspaceCommandReport,
};

use super::{
    outputs::recovery::{
        PreparedProjectPrefabExtractionRecovery, ProjectPrefabExtractionRecoveryError,
        ProjectPrefabExtractionRecoveryFailure,
    },
    recovery_data::PrefabExtractionRecoveryLimits,
    *,
};

mod failure;
mod revision;

pub(crate) use failure::PrefabExtractionSessionPreparationFailure;
use failure::RetainedPreparation;
pub(crate) use revision::PrefabExtractionReviewRevisionRequest;

/// Explicit save selection and the exact previously admitted capture request.
#[must_use]
pub(crate) struct PrefabExtractionReviewRequest {
    request: PrefabExtractionCaptureRequest,
    save_current: Vec<EditorDocumentId>,
    selection_storage: usize,
    task_storage: usize,
}

/// Complete unpublished preparation. The recovery owner retains the fixed output encodings,
/// exact Workspace checkpoints, original physical guards, schema basis and both allowances.
#[derive(Debug)]
#[must_use]
pub(crate) struct PreparedPrefabExtractionSessionReview {
    recovery: PreparedProjectPrefabExtractionRecovery,
    task_storage: usize,
}

/// The bounded preparation cause and its original diagnostics. Recoverable refusals retain the
/// exact private preparation here, so borrowing a report never clones it into another allowance.
#[derive(Debug, thiserror::Error)]
#[error("Prefab extraction preparation failed at {phase}")]
pub(crate) struct PrefabExtractionSessionPreparationError {
    phase: PreparationPhase,
    capacity: Option<PreparationCapacityCause>,
    diagnostics: Option<DiagnosticReport>,
    retained: Option<RetainedPreparation>,
}

#[derive(Debug, Clone, Copy)]
enum PreparationCapacityCause {
    WorkLimit,
    SourceLimit,
    OpenHandles,
}

#[derive(Debug, Clone, Copy)]
enum PreparationPhase {
    Admission,
    Capture,
    Corpus,
    Review,
    Outputs,
    Recovery,
    ProjectValidation,
    WorkspaceValidation,
}

impl fmt::Display for PreparationPhase {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Admission => "request or task admission",
            Self::Capture => "physical capture",
            Self::Corpus => "complete corpus preparation",
            Self::Review => "explicit save Review",
            Self::Outputs => "fixed output preparation",
            Self::Recovery => "recovery data preparation",
            Self::ProjectValidation => "live Project validation",
            Self::WorkspaceValidation => "live Workspace validation",
        })
    }
}

impl fmt::Debug for PrefabExtractionReviewRequest {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PrefabExtractionReviewRequest")
            .field("save_current", &self.save_current.len())
            .field("task_storage", &self.task_storage)
            .field("work", &self.request.work.usage())
            .finish_non_exhaustive()
    }
}

impl PrefabExtractionReviewRequest {
    /// The caller already owns `save_current`. Admit its complete allocation and the bridge
    /// metadata before retaining them in a worker request. Selection semantics stay with the
    /// existing persistence owner, which checks them under the original extraction Work.
    pub(crate) fn new(
        mut request: PrefabExtractionCaptureRequest,
        save_current: Vec<EditorDocumentId>,
    ) -> Result<Self, PrefabExtractionSessionPreparationError> {
        let selection_storage =
            checked_product(save_current.capacity(), size_of::<EditorDocumentId>())
                .map_err(PrefabExtractionSessionPreparationError::admission)?;
        // One fixed envelope follows the operation through ready/refused/revision/publication
        // states. Each selection's Vec has its own shorter, independently settled lifetime.
        let bytes = checked_sum([
            size_of::<Self>()
                .max(size_of::<PrefabExtractionSessionPreparationFailure>())
                .max(size_of::<PrefabExtractionReviewRevisionRequest>())
                .max(size_of::<PreparedPrefabExtractionSessionReview>())
                .max(size_of::<PrefabExtractionSessionPublication>()),
            selection_storage,
        ])
        .map_err(PrefabExtractionSessionPreparationError::admission)?;
        admit_storage(&mut request.inputs.budget, bytes)?;
        Ok(Self {
            request,
            save_current,
            selection_storage,
            task_storage: 0,
        })
    }

    /// Nominal authority comparison only; this performs no filesystem observation.
    pub(crate) fn matches(&self, project: &AuthoringProjectSessionSnapshot) -> bool {
        Arc::ptr_eq(&self.request.schema, &project.schema)
    }

    pub(crate) fn schema_basis(&self) -> &AuthoringSchemaBasis {
        &self.request.basis
    }

    /// Call before allocating the OwnedJob slot and callback captures. Failed admission
    /// releases only unallocated storage; the original Host work remains spent.
    pub(crate) fn admit_task_storage(
        &mut self,
        bytes: usize,
    ) -> Result<(), PrefabExtractionSessionPreparationError> {
        let retained = self.task_storage.checked_add(bytes).ok_or_else(|| {
            PrefabExtractionSessionPreparationError::at(PreparationPhase::Admission, None)
        })?;
        admit_storage(&mut self.request.inputs.budget, bytes)?;
        self.task_storage = retained;
        Ok(())
    }

    /// Call only after the corresponding Task slot and captures have physically returned or
    /// dropped. A terminal observation or cancellation request alone cannot release custody.
    pub(crate) fn release_task_storage(&mut self, bytes: usize) {
        release_storage(
            &mut self.request.inputs.budget,
            &mut self.task_storage,
            bytes,
        );
    }

    /// Worker-only preparation through the actual owners. No retry recaptures inputs, creates
    /// another allowance, invokes project code, or grants Pending/publication authority.
    pub(crate) fn prepare(
        self,
    ) -> Result<PreparedPrefabExtractionSessionReview, PrefabExtractionSessionPreparationFailure>
    {
        let Self {
            request,
            save_current,
            selection_storage,
            task_storage,
        } = self;
        let baseline = request.capture().map_err(|failure| {
            let PrefabExtractionCaptureFailure { request, error } = failure;
            drop(request);
            PrefabExtractionSessionPreparationFailure::terminal(project_error(
                PreparationPhase::Capture,
                error,
            ))
        })?;
        let prepared = baseline.prepare().map_err(|failure| {
            PrefabExtractionSessionPreparationFailure::terminal(
                PrefabExtractionSessionPreparationError::preparation(failure),
            )
        })?;
        let review = prepared
            .prepare_review(save_current)
            .map_err(RetainedPreparation::Review);
        finish_selection(review, 0, selection_storage, task_storage)
    }
}

impl PreparedPrefabExtractionSessionReview {
    /// Revision must first return this owner's actual recovery data and encoded checkpoints.
    /// A refused admission keeps that same owner available through the returned failure.
    pub(crate) fn into_revision(
        self,
        save_current: Vec<EditorDocumentId>,
    ) -> Result<PrefabExtractionReviewRevisionRequest, PrefabExtractionSessionPreparationFailure>
    {
        PrefabExtractionSessionPreparationFailure::retained(
            RetainedPreparation::Ready(self.recovery),
            self.task_storage,
            0,
        )
        .into_revision(save_current)
    }

    #[cfg(test)]
    pub(crate) fn test_prepared(&self) -> &PreparedEditorWorkspacePrefabExtraction {
        self.recovery
            .outputs()
            .review()
            .persistence
            .sources()
            .prepared()
    }

    /// Confirmation moves this exact Review only after its preparation Task physically returns.
    /// No inventory, encoded output, checkpoint or allowance is reconstructed here.
    pub(crate) fn into_publication(self) -> PrefabExtractionSessionPublication {
        assert_eq!(
            self.task_storage, 0,
            "the preparation Task must physically return before publication takes custody"
        );
        PrefabExtractionSessionPublication::new(self.recovery)
    }

    pub(crate) fn outputs(
        &self,
    ) -> impl ExactSizeIterator<Item = ProjectPrefabExtractionOutput<'_>> + '_ {
        self.recovery.outputs().outputs()
    }

    pub(crate) fn documents(
        &self,
    ) -> impl ExactSizeIterator<Item = (EditorDocumentId, EditorPrefabExtractionDisposition)> + '_
    {
        self.recovery.outputs().review().documents()
    }

    pub(crate) fn digest(&self) -> ProjectAuthoringRecoveryDigest {
        self.recovery
            .recovery()
            .expect("a complete Session Review retains its admitted recovery data")
            .digest()
    }

    /// Complete original physical membership, including unchanged closed guards.
    pub(crate) fn source_count(&self) -> usize {
        self.recovery
            .outputs()
            .review()
            .context
            .inventory
            .participants()
            .len()
    }

    pub(crate) fn open_count(&self) -> usize {
        self.documents().len()
    }

    pub(crate) fn host_usage(&self) -> AuthoringProjectAdmissionUsage {
        self.recovery.outputs().review().context.budget.usage()
    }

    /// The original borrowed basis; preparing fixed outputs admits no successor schema.
    pub(crate) fn schema_basis(&self) -> &AuthoringSchemaBasis {
        &self.recovery.outputs().review().context.basis
    }

    /// Cheap nominal check for Session routing, independent of the later physical guard.
    pub(crate) fn matches(&self, project: &AuthoringProjectSessionSnapshot) -> bool {
        Arc::ptr_eq(
            &self.recovery.outputs().review().context.schema,
            &project.schema,
        )
    }

    /// Rechecks complete physical observations and every original open source/history input.
    /// The shared owners spend their original Host budget and extraction Work; nothing is
    /// recaptured, installed or changed in the borrowed live Workspace.
    pub(crate) fn validate_live(
        &mut self,
        project: &AuthoringProjectSessionSnapshot,
        workspace: &EditorWorkspace,
    ) -> Result<(), PrefabExtractionSessionPreparationError> {
        let review = self.recovery.review_mut();
        review
            .revalidate_project(project)
            .map_err(|error| project_error(PreparationPhase::ProjectValidation, error))?;
        review.persistence.validate(workspace).map_err(|report| {
            PrefabExtractionSessionPreparationError::workspace(
                PreparationPhase::WorkspaceValidation,
                report,
            )
        })
    }

    /// Admission remains in the same context even after the worker has produced its Review.
    pub(crate) fn admit_task_storage(
        &mut self,
        bytes: usize,
    ) -> Result<(), PrefabExtractionSessionPreparationError> {
        let retained = self.task_storage.checked_add(bytes).ok_or_else(|| {
            PrefabExtractionSessionPreparationError::at(PreparationPhase::Admission, None)
        })?;
        admit_storage(&mut self.recovery.review_mut().context.budget, bytes)?;
        self.task_storage = retained;
        Ok(())
    }

    /// Physical Task return must precede this refund. Original cumulative work is never reset.
    pub(crate) fn release_task_storage(&mut self, bytes: usize) {
        release_storage(
            &mut self.recovery.review_mut().context.budget,
            &mut self.task_storage,
            bytes,
        );
    }
}

impl PrefabExtractionSessionPreparationError {
    fn at(phase: PreparationPhase, diagnostics: Option<DiagnosticReport>) -> Self {
        // The owning Source report carries this stable code through Workspace wrapping.
        // Keep its bounded diagnostics intact; summary text is not a capacity contract.
        let capacity = diagnostics.as_ref().and_then(|report| {
            report
                .iter()
                .any(|diagnostic| diagnostic.code().as_str() == "scene.prefab-extraction-capacity")
                .then_some(PreparationCapacityCause::SourceLimit)
        });
        Self {
            phase,
            capacity,
            diagnostics,
            retained: None,
        }
    }

    pub(crate) fn capture(diagnostics: DiagnosticReport) -> Self {
        Self::at(PreparationPhase::Capture, Some(diagnostics))
    }

    fn admission(error: AuthoringProjectSourceAdmissionError) -> Self {
        source_error(PreparationPhase::Admission, error)
    }

    fn preparation(failure: ProjectPrefabExtractionPreparationFailure) -> Self {
        match failure {
            ProjectPrefabExtractionPreparationFailure::Core { failure, .. } => {
                let (_, _, report) = failure.into_parts();
                Self::at(PreparationPhase::Corpus, Some(report.diagnostics))
            }
            ProjectPrefabExtractionPreparationFailure::Workspace { failure, .. } => {
                let (_, _, _, report) = failure.into_parts();
                Self::workspace(PreparationPhase::Corpus, report)
            }
        }
    }

    fn workspace(phase: PreparationPhase, report: EditorWorkspaceCommandReport) -> Self {
        Self::at(phase, Some(report.diagnostics))
    }

    pub(crate) fn diagnostics(&self) -> Option<&DiagnosticReport> {
        self.diagnostics.as_ref().or_else(|| {
            self.retained
                .as_ref()
                .and_then(RetainedPreparation::diagnostics)
        })
    }

    pub(crate) fn host_work_limit_exceeded(&self) -> bool {
        matches!(self.capacity, Some(PreparationCapacityCause::WorkLimit))
    }

    pub(crate) fn source_limit_exceeded(&self) -> bool {
        matches!(self.capacity, Some(PreparationCapacityCause::SourceLimit))
            || self.diagnostics().is_some_and(|report| {
                report.iter().any(|diagnostic| {
                    diagnostic.code().as_str() == "scene.prefab-extraction-capacity"
                })
            })
    }

    pub(crate) fn open_handles_limit_exceeded(&self) -> bool {
        matches!(self.capacity, Some(PreparationCapacityCause::OpenHandles))
    }

    /// Moves the original bounded report without cloning diagnostics or retaining candidates.
    /// Non-report refusals leave the caller to present a fixed preparation diagnostic.
    pub(crate) fn into_diagnostics(self) -> Option<DiagnosticReport> {
        assert!(
            self.retained.is_none(),
            "retained preparation diagnostics must be borrowed from their original owner"
        );
        self.diagnostics
    }
}

fn finish_review(
    review: PreparedProjectPrefabExtractionReview,
    task_storage: usize,
) -> Result<PreparedPrefabExtractionSessionReview, PrefabExtractionSessionPreparationFailure> {
    let outputs = review.prepare_outputs().map_err(|failure| {
        PrefabExtractionSessionPreparationFailure::retained(
            RetainedPreparation::Outputs(failure),
            task_storage,
            0,
        )
    })?;
    let recovery = outputs
        .prepare_recovery(PrefabExtractionRecoveryLimits::default())
        .map_err(|failure| {
            PrefabExtractionSessionPreparationFailure::retained(
                RetainedPreparation::Recovery(failure),
                task_storage,
                0,
            )
        })?;
    Ok(PreparedPrefabExtractionSessionReview {
        recovery,
        task_storage,
    })
}

/// Settle only allocations that the actual persistence/return call has dropped. A refused
/// Persistence selection still owns its original Vec and carries that reservation forward.
fn finish_selection(
    mut review: Result<PreparedProjectPrefabExtractionReview, RetainedPreparation>,
    previous_selection_storage: usize,
    selection_storage: usize,
    task_storage: usize,
) -> Result<PreparedPrefabExtractionSessionReview, PrefabExtractionSessionPreparationFailure> {
    let retained_selection = match &review {
        Err(preparation) if preparation.retains_selection() => selection_storage,
        _ => 0,
    };
    let released = previous_selection_storage
        .checked_add(selection_storage - retained_selection)
        .expect("both selection allocations were admitted under the same Host reservation");
    let context = match &mut review {
        Ok(review) => &mut review.context,
        Err(preparation) => preparation.context_mut(),
    };
    release_selection_storage(&mut context.budget, released);
    match review {
        Ok(review) => finish_review(review, task_storage),
        Err(preparation) => Err(PrefabExtractionSessionPreparationFailure::retained(
            preparation,
            task_storage,
            retained_selection,
        )),
    }
}

fn release_selection_storage(budget: &mut AuthoringProjectAdmissionBudget, bytes: usize) {
    budget
        .reconcile_retained_reservation(bytes, 0)
        .expect("only a dropped selection's original admitted storage is released");
}

fn admit_storage(
    budget: &mut AuthoringProjectAdmissionBudget,
    bytes: usize,
) -> Result<(), PrefabExtractionSessionPreparationError> {
    let retained = budget.retained_candidate_bytes;
    if let Err(error) = budget.charge_source_bytes(0, 0, bytes, bytes) {
        // Admission preceded allocation, so only this new live reservation can be released.
        // Reads, comparisons and cumulative Host work remain charged in this same budget.
        budget.retained_candidate_bytes = retained;
        return Err(admission_error(PreparationPhase::Admission, error));
    }
    Ok(())
}

fn release_storage(
    budget: &mut AuthoringProjectAdmissionBudget,
    task_storage: &mut usize,
    bytes: usize,
) {
    let remaining = task_storage
        .checked_sub(bytes)
        .expect("only admitted, physically returned Task storage is released");
    budget
        .reconcile_retained_reservation(bytes, 0)
        .expect("Task storage stays charged until its physical return");
    *task_storage = remaining;
}

fn admission_error(
    phase: PreparationPhase,
    error: AuthoringProjectAdmissionError,
) -> PrefabExtractionSessionPreparationError {
    let capacity = match error {
        AuthoringProjectAdmissionError::DecodeWorkExceeded { .. } => {
            Some(PreparationCapacityCause::WorkLimit)
        }
        AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { .. } => {
            Some(PreparationCapacityCause::OpenHandles)
        }
        _ => None,
    };
    PrefabExtractionSessionPreparationError {
        phase,
        capacity,
        diagnostics: None,
        retained: None,
    }
}

fn source_error(
    phase: PreparationPhase,
    error: AuthoringProjectSourceAdmissionError,
) -> PrefabExtractionSessionPreparationError {
    let diagnostics = match error {
        AuthoringProjectSourceAdmissionError::Budget(error) => {
            return admission_error(phase, error);
        }
        AuthoringProjectSourceAdmissionError::CompilationBudget(diagnostics) => Some(diagnostics),
        AuthoringProjectSourceAdmissionError::SourceStructure { source, .. } => {
            Some(source.into_diagnostics())
        }
        AuthoringProjectSourceAdmissionError::SourcePublication { source, .. } => {
            Some(source.into_diagnostics())
        }
        _ => None,
    };
    PrefabExtractionSessionPreparationError::at(phase, diagnostics)
}

fn project_error(
    phase: PreparationPhase,
    error: PrefabExtractionProjectError,
) -> PrefabExtractionSessionPreparationError {
    let diagnostics = match error {
        PrefabExtractionProjectError::Admission(error)
        | PrefabExtractionProjectError::Inventory(SourceInventoryError::Admission(error))
        | PrefabExtractionProjectError::Decode(SourceDecodeError::Admission(error)) => {
            return admission_error(phase, error);
        }
        PrefabExtractionProjectError::SourceWork(report) => Some(report.diagnostics),
        PrefabExtractionProjectError::Project(error)
        | PrefabExtractionProjectError::Inventory(SourceInventoryError::Native(error))
        | PrefabExtractionProjectError::Decode(SourceDecodeError::Project(error)) => {
            return source_error(phase, error);
        }
        PrefabExtractionProjectError::Decode(SourceDecodeError::Structure(error)) => {
            Some(error.into_diagnostics())
        }
        _ => None,
    };
    PrefabExtractionSessionPreparationError::at(phase, diagnostics)
}
