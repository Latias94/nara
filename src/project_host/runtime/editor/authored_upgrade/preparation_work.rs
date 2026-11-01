//! Proposal Task custody returns the original baseline on admission refusal or skipped execution.

use nara_tasks::{TaskPools, TaskSpawnRequest};

use super::super::authoring_work::{OwnedJob, OwnedReturn};
use super::{
    AuthoredSchemaUpgradeProposal, PreparedAuthoredSchemaUpgradeReview, SourceUpgradeBaseline,
    SourceUpgradeProposalFailure, WorkError,
};

type PreparationResult = Result<PreparedAuthoredSchemaUpgradeReview, SourceUpgradeProposalFailure>;
type PreparationJob = OwnedJob<(SourceUpgradeBaseline, usize), PreparationResult>;

pub(super) struct PreparationWork {
    job: Option<PreparationJob>,
    task_storage: usize,
    proposal_storage: usize,
}

pub(super) enum PreparationStartFailure {
    Proposal(SourceUpgradeProposalFailure),
    Task(SourceUpgradeBaseline),
}

pub(super) enum PreparationReturn {
    Unstarted(SourceUpgradeBaseline),
    Prepared(PreparationResult),
}

impl PreparationWork {
    pub(super) fn start(
        pools: &TaskPools,
        task: TaskSpawnRequest,
        baseline: SourceUpgradeBaseline,
        proposal: AuthoredSchemaUpgradeProposal,
        #[cfg(test)] gate: Option<Box<dyn FnOnce() + Send>>,
    ) -> Result<Self, PreparationStartFailure> {
        let request = baseline
            .proposal_request(proposal)
            .map_err(PreparationStartFailure::Proposal)?;
        let (mut baseline, proposal_storage, prepare) = request.into_task_parts();
        let advance = move |input| {
            let result = prepare(input);
            #[cfg(test)]
            if let Some(gate) = gate {
                gate();
            }
            result
        };
        let task_storage = PreparationJob::storage_bytes(&advance);
        if let Err(error) = baseline.admit_preparation_task_storage(task_storage) {
            drop(advance);
            baseline.release_preparation_task_storage(proposal_storage);
            return Err(PreparationStartFailure::Proposal(
                SourceUpgradeProposalFailure { baseline, error },
            ));
        }
        let job = PreparationJob::start(pools, task, (baseline, task_storage), advance);
        if job.pending.rejection.is_some() {
            let OwnedReturn::Unstarted((mut baseline, _)) = job
                .into_owner()
                .expect("synchronous rejection retains the unstarted baseline")
            else {
                unreachable!("a rejected preparation cannot have executed");
            };
            baseline.release_preparation_task_storage(task_storage + proposal_storage);
            return Err(PreparationStartFailure::Task(baseline));
        }
        Ok(Self {
            job: Some(job),
            task_storage,
            proposal_storage,
        })
    }

    pub(super) fn poll(&mut self) -> Option<Result<PreparationReturn, WorkError>> {
        let _returned = self.job.as_mut()?.pending.poll()?;
        let job = self
            .job
            .take()
            .expect("one physically returned preparation");
        Some(match job.into_owner() {
            Ok(OwnedReturn::Unstarted((mut baseline, _))) => {
                baseline
                    .release_preparation_task_storage(self.task_storage + self.proposal_storage);
                Ok(PreparationReturn::Unstarted(baseline))
            }
            Ok(OwnedReturn::Returned(mut result)) => {
                match &mut result {
                    Ok(review) => review.release_preparation_task_storage(self.task_storage),
                    Err(failure) => failure
                        .baseline
                        .release_preparation_task_storage(self.task_storage),
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

    #[cfg(test)]
    pub(super) fn return_observer(&self) -> nara_tasks::TaskReturnObserver {
        self.job
            .as_ref()
            .unwrap()
            .pending
            .task
            .as_ref()
            .unwrap()
            .return_observer()
    }
}
