//! Selection preparation retains initial and revised inputs until actual Task return.

use super::super::authoring_work::{OwnedJob, OwnedReturn, WorkError};
use super::*;

pub(super) enum PreparationInput {
    Initial(PrefabExtractionReviewRequest),
    Revision(PrefabExtractionReviewRevisionRequest),
}

type PreparationResult =
    Result<PreparedPrefabExtractionSessionReview, PrefabExtractionSessionPreparationFailure>;
pub(super) type PreparationJob = OwnedJob<PreparationInput, PreparationResult>;

impl PreparationInput {
    fn admit_task_storage(
        &mut self,
        bytes: usize,
    ) -> Result<(), PrefabExtractionSessionPreparationError> {
        match self {
            Self::Initial(request) => request.admit_task_storage(bytes),
            Self::Revision(request) => request.admit_task_storage(bytes),
        }
    }

    fn release_task_storage(&mut self, bytes: usize) {
        match self {
            Self::Initial(request) => request.release_task_storage(bytes),
            Self::Revision(request) => request.release_task_storage(bytes),
        }
    }

    fn prepare(self) -> PreparationResult {
        match self {
            Self::Initial(request) => request.prepare(),
            Self::Revision(request) => request.prepare(),
        }
    }
}

pub(super) struct PreparationStartFailure {
    pub(super) input: PreparationInput,
    pub(super) error: Option<PrefabExtractionSessionPreparationError>,
}

pub(super) enum PreparationReturn {
    Prepared(PreparationResult),
    Unstarted(PreparationInput),
}

pub(super) struct PreparationWork {
    pub(super) job: Option<PreparationJob>,
    storage: usize,
}

impl PreparationWork {
    pub(super) fn start(
        pools: &nara_tasks::TaskPools,
        operation: EditorPrefabExtractionId,
        mut input: PreparationInput,
        #[cfg(test)] gate: Option<Box<dyn FnOnce() + Send>>,
    ) -> Result<Self, PreparationStartFailure> {
        let advance = move |input: PreparationInput| {
            let result = input.prepare();
            #[cfg(test)]
            if let Some(gate) = gate {
                gate();
            }
            result
        };
        let storage = PreparationJob::storage_bytes(&advance);
        if let Err(error) = input.admit_task_storage(storage) {
            return Err(PreparationStartFailure {
                input,
                error: Some(error),
            });
        }
        let job = PreparationJob::start(
            pools,
            nara_tasks::TaskSpawnRequest::new(operation.0, DOMAIN),
            input,
            advance,
        );
        if job.pending.rejection.is_some() {
            let OwnedReturn::Unstarted(mut input) = job
                .into_owner()
                .expect("synchronous rejection retains its original input")
            else {
                unreachable!("a rejected Task did not execute selection preparation")
            };
            input.release_task_storage(storage);
            return Err(PreparationStartFailure { input, error: None });
        }
        Ok(Self {
            job: Some(job),
            storage,
        })
    }

    pub(super) fn poll(&mut self) -> Option<Result<PreparationReturn, WorkError>> {
        let _returned = self.job.as_mut()?.pending.poll()?;
        let job = self
            .job
            .take()
            .expect("one original preparation physically returned");
        Some(match job.into_owner() {
            Ok(OwnedReturn::Unstarted(mut input)) => {
                input.release_task_storage(self.storage);
                Ok(PreparationReturn::Unstarted(input))
            }
            Ok(OwnedReturn::Returned(mut result)) => {
                match &mut result {
                    Ok(prepared) => prepared.release_task_storage(self.storage),
                    Err(failure) => failure.release_task_storage(self.storage),
                }
                Ok(PreparationReturn::Prepared(result))
            }
            Err(error) => Err(error),
        })
    }

    pub(super) fn cancel(&self) {
        if let Some(job) = &self.job {
            job.pending.cancel();
        }
    }
}
