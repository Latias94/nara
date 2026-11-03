//! Save-selection changes over the exact returned unpublished preparation.

use super::*;

/// A finite worker request. It carries the original corpus, checkpoints, schema and allowances;
/// only the caller's newly admitted SaveCurrent selection differs from its predecessor.
#[derive(Debug)]
#[must_use]
pub(crate) struct PrefabExtractionReviewRevisionRequest {
    preparation: RetainedPreparation,
    save_current: Vec<EditorDocumentId>,
    previous_selection_storage: usize,
    selection_storage: usize,
    task_storage: usize,
}

impl PrefabExtractionSessionPreparationFailure {
    pub(crate) fn into_revision(
        mut self,
        save_current: Vec<EditorDocumentId>,
    ) -> Result<PrefabExtractionReviewRevisionRequest, Self> {
        if !self.has_retained_preparation() {
            return Err(self);
        }
        // A logical completion or cancel request cannot revise an owner whose Task storage
        // still represents an actual callback/slot. The physical-return path refunds it first.
        if self.task_storage != 0 {
            self.record_refusal(PrefabExtractionSessionPreparationError::at(
                PreparationPhase::Admission,
                None,
            ));
            return Err(self);
        }
        let bytes = checked_product(save_current.capacity(), size_of::<EditorDocumentId>())
            .map_err(PrefabExtractionSessionPreparationError::admission);
        let admission = bytes.and_then(|selection_storage| {
            let work = checked_sum([
                size_of::<PrefabExtractionReviewRevisionRequest>(),
                selection_storage,
            ])
            .map_err(PrefabExtractionSessionPreparationError::admission)?;
            let preparation = self
                .error
                .retained
                .as_mut()
                .expect("retained preparation was checked before admission");
            let budget = &mut preparation.context_mut().budget;
            let before = budget.retained_candidate_bytes;
            if let Err(error) = budget.charge_source_bytes(0, 0, work, selection_storage) {
                budget.retained_candidate_bytes = before;
                return Err(admission_error(PreparationPhase::Admission, error));
            }
            Ok(selection_storage)
        });
        let selection_storage = match admission {
            Ok(bytes) => bytes,
            Err(error) => {
                // The new selection was never retained. Admission refunds only its unallocated
                // live storage; original work and the previous editable owner remain unchanged.
                self.record_refusal(error);
                return Err(self);
            }
        };
        let preparation = self
            .error
            .retained
            .take()
            .expect("successful admission retains the same preparation");
        Ok(PrefabExtractionReviewRevisionRequest {
            preparation,
            save_current,
            previous_selection_storage: self.selection_storage,
            selection_storage,
            task_storage: self.task_storage,
        })
    }

    fn record_refusal(&mut self, mut error: PrefabExtractionSessionPreparationError) {
        assert!(error.retained.is_none());
        error.retained = self.error.retained.take();
        self.error = error;
    }
}

impl PrefabExtractionReviewRevisionRequest {
    pub(crate) fn matches(&self, project: &AuthoringProjectSessionSnapshot) -> bool {
        Arc::ptr_eq(&self.preparation.context().schema, &project.schema)
    }

    pub(crate) fn schema_basis(&self) -> &AuthoringSchemaBasis {
        &self.preparation.context().basis
    }

    pub(crate) fn source_count(&self) -> usize {
        self.preparation.context().inventory.participants().len()
    }

    pub(crate) fn open_count(&self) -> usize {
        self.preparation.context().open.len()
    }

    /// Failed admission borrows this request, retaining its exact selection and original owner.
    /// The caller can retry that request or move it into an editable returned failure.
    pub(crate) fn admit_task_storage(
        &mut self,
        bytes: usize,
    ) -> Result<(), PrefabExtractionSessionPreparationError> {
        let retained = self.task_storage.checked_add(bytes).ok_or_else(|| {
            PrefabExtractionSessionPreparationError::at(PreparationPhase::Admission, None)
        })?;
        admit_storage(&mut self.preparation.context_mut().budget, bytes)?;
        self.task_storage = retained;
        Ok(())
    }

    pub(crate) fn release_task_storage(&mut self, bytes: usize) {
        release_storage(
            &mut self.preparation.context_mut().budget,
            &mut self.task_storage,
            bytes,
        );
    }

    /// Pure custody transfer after a refused dispatch or an actual unstarted Task return. A
    /// caller may then select a different SaveCurrent set without recapturing any source.
    pub(crate) fn into_failure(
        self,
        error: Option<PrefabExtractionSessionPreparationError>,
    ) -> PrefabExtractionSessionPreparationFailure {
        let Self {
            mut preparation,
            save_current,
            previous_selection_storage,
            selection_storage,
            task_storage,
        } = self;
        drop(save_current);
        release_selection_storage(&mut preparation.context_mut().budget, selection_storage);
        let mut failure = PrefabExtractionSessionPreparationFailure::retained(
            preparation,
            task_storage,
            previous_selection_storage,
        );
        failure.record_refusal(error.unwrap_or_else(|| {
            PrefabExtractionSessionPreparationError::at(PreparationPhase::Admission, None)
        }));
        failure
    }

    pub(crate) fn prepare(
        self,
    ) -> Result<PreparedPrefabExtractionSessionReview, PrefabExtractionSessionPreparationFailure>
    {
        let Self {
            preparation,
            save_current,
            previous_selection_storage,
            selection_storage,
            task_storage,
        } = self;
        finish_selection(
            revise(preparation, save_current),
            previous_selection_storage,
            selection_storage,
            task_storage,
        )
    }
}

fn revise(
    preparation: RetainedPreparation,
    save_current: Vec<EditorDocumentId>,
) -> Result<PreparedProjectPrefabExtractionReview, RetainedPreparation> {
    // These are the existing owners' actual return protocols. A refused checkpoint return
    // remains a checkpoint-return owner; it is never replaced with a reconstructed Review.
    let review = match preparation {
        RetainedPreparation::Review(failure) => {
            return failure
                .retry(save_current)
                .map_err(RetainedPreparation::Review);
        }
        RetainedPreparation::Outputs(failure) => failure
            .cancel()
            .map_err(RetainedPreparation::ReturningOutputs)?,
        RetainedPreparation::ReturningOutputs(failure) => failure
            .retry()
            .map_err(RetainedPreparation::ReturningOutputs)?,
        RetainedPreparation::Recovery(failure) => failure
            .cancel()
            .map_err(RetainedPreparation::Recovery)?
            .cancel()
            .map_err(RetainedPreparation::ReturningOutputs)?,
        RetainedPreparation::Ready(recovery) => recovery
            .cancel()
            .map_err(RetainedPreparation::Recovery)?
            .cancel()
            .map_err(RetainedPreparation::ReturningOutputs)?,
    };
    review
        .revise(save_current)
        .map_err(RetainedPreparation::Review)
}
