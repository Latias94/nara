//! Publication custody survives rejection and cancellation before a finite Task starts.

#[cfg(test)]
use nara_tasks::TaskReturnObserver;
use nara_tasks::{TaskPools, TaskSpawnRequest};

use super::super::authoring_work::{OwnedJob, OwnedReturn};
use super::{AuthoredSourceUpgradePublication, EditorAuthoredSchemaUpgradeError, WorkError};

type PublicationJob = OwnedJob<AuthoredSourceUpgradePublication, AuthoredSourceUpgradePublication>;

pub(super) struct PublicationWork {
    job: Option<PublicationJob>,
    retained_bytes: usize,
}

pub(super) struct PublicationStartFailure {
    pub(super) publication: AuthoredSourceUpgradePublication,
    pub(super) refusal: EditorAuthoredSchemaUpgradeError,
}

impl PublicationWork {
    pub(super) fn start(
        pools: &TaskPools,
        request: TaskSpawnRequest,
        mut publication: AuthoredSourceUpgradePublication,
    ) -> Result<Self, PublicationStartFailure> {
        let advance = AuthoredSourceUpgradePublication::advance;
        let retained_bytes = PublicationJob::storage_bytes(&advance);
        let before = publication.retained_bytes();
        if let Err(error) = publication.admit_installation_storage(retained_bytes) {
            let reserved = publication.retained_bytes() - before;
            publication.release_installation_storage(reserved);
            publication.record_failure(error);
            return Err(PublicationStartFailure {
                publication,
                refusal: EditorAuthoredSchemaUpgradeError::Publication,
            });
        }
        let job = PublicationJob::start(pools, request, publication, advance);
        if job.pending.rejection.is_some() {
            // Rejected Tasks have disposed their callback. The original owner never left Ready.
            let owner = job
                .into_owner()
                .expect("a rejected Task retains its original publication");
            let mut publication = match owner {
                OwnedReturn::Unstarted(publication) | OwnedReturn::Returned(publication) => {
                    publication
                }
            };
            publication.release_installation_storage(retained_bytes);
            return Err(PublicationStartFailure {
                publication,
                refusal: EditorAuthoredSchemaUpgradeError::Task,
            });
        }
        Ok(Self {
            job: Some(job),
            retained_bytes,
        })
    }

    pub(super) fn poll(&mut self) -> Option<Result<AuthoredSourceUpgradePublication, WorkError>> {
        let _returned = self.job.as_mut()?.pending.poll()?;
        // Pending::poll proves physical return. Drop its Job, callback and shared slot before
        // refunding any storage, including when cancellation prevented the callback from running.
        let job = self
            .job
            .take()
            .expect("one physically returned publication job");
        Some(match job.into_owner() {
            Ok(
                OwnedReturn::Unstarted(mut publication) | OwnedReturn::Returned(mut publication),
            ) => {
                publication.release_installation_storage(self.retained_bytes);
                Ok(publication)
            }
            Err(error) => Err(error),
        })
    }

    #[cfg(test)]
    pub(super) fn return_observer(&self) -> TaskReturnObserver {
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

#[cfg(test)]
mod tests {
    use nara_core::ItemLimit;
    use nara_tasks::{TaskPoolConfig, TaskRejectReason, TaskReturnState};

    use super::super::{DOMAIN, EditorAuthoredSchemaUpgradeView};
    use super::*;

    fn pools() -> TaskPools {
        let one = ItemLimit::new(1).unwrap();
        TaskPools::inline_for_tests(TaskPoolConfig::threaded(one, one, one).unwrap()).unwrap()
    }

    fn owner() -> Box<EditorAuthoredSchemaUpgradeView> {
        Box::new(EditorAuthoredSchemaUpgradeView {
            writes_fenced: true,
            ..Default::default()
        })
    }

    #[test]
    fn closed_task_admission_returns_the_original_move_only_owner_and_can_cancel() {
        let pools = pools();
        assert!(pools.shutdown_blocking().unwrap().is_complete());
        let owner = owner();
        let original = std::ptr::from_ref(owner.as_ref());
        let mut job =
            OwnedJob::<_, ()>::start(&pools, TaskSpawnRequest::new(0, DOMAIN), owner, |_| {
                panic!("a rejected publication must not advance")
            });
        let rejection = job.pending.rejection.unwrap();
        assert_eq!(rejection.reason, TaskRejectReason::PoolClosed);
        assert_eq!(rejection.return_state, TaskReturnState::Returned);
        assert!(matches!(
            job.pending.poll(),
            Some(Err(WorkError::Unavailable))
        ));
        let OwnedReturn::Unstarted(owner) = job.into_owner().unwrap() else {
            panic!("a closed pool cannot execute the publication");
        };
        assert_eq!(std::ptr::from_ref(owner.as_ref()), original);
        assert!(owner.can_cancel());
    }

    #[test]
    fn cancellation_before_start_returns_the_original_owner_after_physical_disposal() {
        let pools = pools();
        let owner = owner();
        let original = std::ptr::from_ref(owner.as_ref());
        let mut job =
            OwnedJob::<_, ()>::start(&pools, TaskSpawnRequest::new(0, DOMAIN), owner, |_| {
                panic!("a cancelled unstarted publication must not advance")
            });
        job.pending.cancel();
        let _ = pools.run_pending_for_tests();
        assert!(matches!(
            job.pending.poll(),
            Some(Err(WorkError::CancelledBeforeStart))
        ));
        let OwnedReturn::Unstarted(owner) = job.into_owner().unwrap() else {
            panic!("a cancelled queued publication cannot execute");
        };
        assert_eq!(std::ptr::from_ref(owner.as_ref()), original);
        assert!(owner.can_cancel());
        assert!(pools.shutdown_blocking().unwrap().is_complete());
    }
}
