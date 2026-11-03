//! Storage follows the original context through every successful and refused stage.

use super::*;

impl PrefabExtractionSessionPublication {
    pub(super) fn context(&self) -> &ProjectExtractionContext {
        match self
            .stage
            .as_ref()
            .expect("one original publication context")
        {
            Stage::Confirm(owner) | Stage::StorageInput(owner) | Stage::CancelledReview(owner) => {
                &owner.outputs().review().context
            }
            Stage::Storage(owner) => &owner.outputs().review().context,
            Stage::Files(owner) | Stage::Inventory(owner) => owner.session_context(),
            Stage::Project(owner) => owner.publication().session_context(),
            Stage::BindProject(owner) => owner.publication().session_context(),
            Stage::Workspace(owner) => owner.publication().session_context(),
            Stage::Ready(owner) => owner.project().publication().session_context(),
            Stage::CancelledStorage(owner) => &owner.prepared().outputs().review().context,
            Stage::Installed(owner) => owner.session_context(),
        }
    }

    pub(super) fn context_mut(&mut self) -> &mut ProjectExtractionContext {
        match self
            .stage
            .as_mut()
            .expect("one original publication context")
        {
            Stage::Confirm(owner) | Stage::StorageInput(owner) | Stage::CancelledReview(owner) => {
                &mut owner.review_mut().context
            }
            Stage::Storage(owner) => owner.outputs_mut().session_context_mut(),
            Stage::Files(owner) | Stage::Inventory(owner) => owner.session_context_mut(),
            Stage::Project(owner) => owner.publication_mut().session_context_mut(),
            Stage::BindProject(owner) => owner.publication_mut().session_context_mut(),
            Stage::Workspace(owner) => owner.publication_mut().session_context_mut(),
            Stage::Ready(owner) => owner.project_mut().publication_mut().session_context_mut(),
            Stage::CancelledStorage(owner) => owner.session_context_mut(),
            Stage::Installed(owner) => owner.session_context_mut(),
        }
    }

    pub(crate) fn retained_bytes(&self) -> usize {
        self.context().budget.retained_candidate_bytes
    }

    pub(crate) fn host_work_exhausted(&self) -> bool {
        let budget = &self.context().budget;
        budget.decode_work > budget.limits.decode_work.get()
            || self.failure.as_ref().is_some_and(|error| {
                matches!(
                    error.admission(),
                    Some(AuthoringProjectAdmissionError::DecodeWorkExceeded { .. })
                )
            })
    }

    pub(crate) fn open_handles_exceeded(&self) -> bool {
        self.failure.as_ref().is_some_and(|error| {
            matches!(
                error.admission(),
                Some(AuthoringProjectAdmissionError::PeakOpenHandlesExceeded { .. })
            )
        })
    }

    /// Admission precedes allocating the OwnedJob slot and every callback capture.
    pub(crate) fn admit_task_storage(
        &mut self,
        bytes: usize,
    ) -> Result<(), PrefabExtractionSessionPublicationError> {
        let retained = self
            .task_storage
            .checked_add(bytes)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        self.admit_external_storage(bytes)?;
        self.task_storage = retained;
        Ok(())
    }

    /// Only the actual physical Task return permits this refund, including failed stages.
    pub(crate) fn release_task_storage(&mut self, bytes: usize) {
        self.task_storage = self
            .task_storage
            .checked_sub(bytes)
            .expect("only admitted physically returned Task storage is released");
        self.release_external_storage(bytes);
    }

    pub(crate) fn admit_installation_storage(
        &mut self,
        bytes: usize,
    ) -> Result<(), PrefabExtractionSessionPublicationError> {
        let retained = self
            .installation_storage
            .checked_add(bytes)
            .ok_or(AuthoringProjectSourceAdmissionError::AccountingOverflow)?;
        self.admit_external_storage(bytes)?;
        self.installation_storage = retained;
        Ok(())
    }

    pub(crate) fn release_installation_storage(&mut self, bytes: usize) {
        self.installation_storage = self
            .installation_storage
            .checked_sub(bytes)
            .expect("only admitted physically dropped installation storage is released");
        self.release_external_storage(bytes);
    }

    fn admit_external_storage(
        &mut self,
        bytes: usize,
    ) -> Result<(), PrefabExtractionSessionPublicationError> {
        admit_storage(&mut self.context_mut().budget, bytes)?;
        if let Some(Stage::Storage(owner)) = self.stage.as_mut() {
            owner.adjust_session_storage(bytes, 0);
        }
        Ok(())
    }

    fn release_external_storage(&mut self, bytes: usize) {
        self.context_mut()
            .budget
            .reconcile_retained_reservation(bytes, 0)
            .expect("the original stage retains every still-live allocation");
        if let Some(Stage::Storage(owner)) = self.stage.as_mut() {
            owner.adjust_session_storage(0, bytes);
        }
    }
}

pub(super) fn admit_storage(
    budget: &mut AuthoringProjectAdmissionBudget,
    bytes: usize,
) -> Result<(), PrefabExtractionSessionPublicationError> {
    let before = budget.retained_candidate_bytes;
    if let Err(error) = budget.charge_source_bytes(0, 0, bytes, bytes) {
        let unused = budget
            .retained_candidate_bytes
            .checked_sub(before)
            .expect("failed preallocation cannot release original storage");
        budget
            .reconcile_retained_reservation(unused, 0)
            .expect("the refused allocation never became live");
        return Err(error.into());
    }
    Ok(())
}
