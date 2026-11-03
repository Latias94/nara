//! Finite publication work keeps its move-only owner through actual return or spawn rejection.

use nara_tasks::{TaskPools, TaskSpawnRequest};

use super::super::authoring_work::{OwnedJob, OwnedReturn, WorkError};
use super::{EditorPrefabExtractionError, PrefabExtractionSessionPublication};

type PublicationJob =
    OwnedJob<PrefabExtractionSessionPublication, PrefabExtractionSessionPublication>;

pub(super) struct PublicationWork {
    job: Option<PublicationJob>,
    storage: usize,
}

pub(super) struct PublicationStartFailure {
    pub(super) publication: PrefabExtractionSessionPublication,
    pub(super) refusal: EditorPrefabExtractionError,
}

impl PublicationWork {
    pub(super) fn start(
        pools: &TaskPools,
        request: TaskSpawnRequest,
        mut publication: PrefabExtractionSessionPublication,
    ) -> Result<Self, PublicationStartFailure> {
        let advance = PrefabExtractionSessionPublication::advance;
        let storage = PublicationJob::storage_bytes(&advance);
        if let Err(error) = publication.admit_task_storage(storage) {
            publication.record_failure(error);
            return Err(PublicationStartFailure {
                publication,
                refusal: EditorPrefabExtractionError::Publication,
            });
        }
        let job = PublicationJob::start(pools, request, publication, advance);
        if job.pending.rejection.is_some() {
            // Synchronous rejection disposes the callback and leaves the original owner here.
            let mut publication = match job
                .into_owner()
                .expect("rejected publication retains its original owner")
            {
                OwnedReturn::Unstarted(owner) | OwnedReturn::Returned(owner) => owner,
            };
            publication.release_task_storage(storage);
            return Err(PublicationStartFailure {
                publication,
                refusal: EditorPrefabExtractionError::Task,
            });
        }
        Ok(Self {
            job: Some(job),
            storage,
        })
    }

    pub(super) fn poll(&mut self) -> Option<Result<PrefabExtractionSessionPublication, WorkError>> {
        let _returned = self.job.as_mut()?.pending.poll()?;
        let job = self
            .job
            .take()
            .expect("one physically returned publication job");
        Some(match job.into_owner() {
            Ok(
                OwnedReturn::Unstarted(mut publication) | OwnedReturn::Returned(mut publication),
            ) => {
                // Job and its callback were disposed by into_owner before the reservation ends.
                publication.release_task_storage(self.storage);
                Ok(publication)
            }
            Err(error) => Err(error),
        })
    }
}
