use std::sync::{Arc, Mutex, TryLockError};

use nara_app::tasks::{
    TaskCancellationToken, TaskDomainKey, TaskHandle, TaskPoolKind, TaskPools, TaskReturnState,
    TaskSpawnRequest, TaskTerminal,
};

type SharedResult<C, F> = Arc<Mutex<Option<Result<C, F>>>>;

/// CPU preparation on a threaded I/O pool. Returning from a task and taking the prepared value
/// are separate steps so cancellation cannot release the request while captures are still live.
pub struct ScenePreparation<C, F> {
    task: TaskHandle<()>,
    result: SharedResult<C, F>,
}

impl<C, F> ScenePreparation<C, F> {
    pub fn spawn(
        pools: &TaskPools,
        prepare: impl FnOnce(TaskCancellationToken) -> Option<Result<C, F>> + Send + 'static,
    ) -> Option<Self>
    where
        C: Send + 'static,
        F: Send + 'static,
    {
        if !pools.is_threaded(TaskPoolKind::Io) {
            return None;
        }
        let result: SharedResult<C, F> = Arc::new(Mutex::new(None));
        let worker_result = Arc::clone(&result);
        let task = pools
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, TaskDomainKey::new(0x7363_656e_65)),
                move |cancellation| {
                    let value = prepare(cancellation);
                    *worker_result
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner()) = value;
                },
            )
            .into_handle()
            .ok()?;
        Some(Self { task, result })
    }

    pub fn cancel(&self) {
        self.task.cancel();
    }

    pub fn poll(&mut self, unavailable: impl FnOnce() -> F) -> Option<Result<C, F>> {
        let returned = self.task.return_observer().state();
        if returned == TaskReturnState::Pending {
            return None;
        }
        let mut result = match self.result.try_lock() {
            Ok(result) => result,
            Err(TryLockError::WouldBlock) => return None,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        let terminal = self.task.try_take()?;
        let result = result.take();
        Some(
            if returned != TaskReturnState::CleanupFailed
                && matches!(terminal, TaskTerminal::Completed(()))
            {
                result.unwrap_or_else(|| Err(unavailable()))
            } else {
                Err(unavailable())
            },
        )
    }
}

impl<C, F> Drop for ScenePreparation<C, F> {
    fn drop(&mut self) {
        self.cancel();
    }
}
