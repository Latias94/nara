use std::{future::Future, panic::catch_unwind, sync::OnceLock};

use bevy_tasks::{TaskPool, TaskPoolBuilder};

use super::{TaskPoolError, TaskPoolKind, drop_catching};

pub(super) struct PoolBackend(TaskPool);

impl PoolBackend {
    pub(super) fn workers(&self) -> usize {
        #[cfg(not(target_arch = "wasm32"))]
        return self.0.thread_num();
        #[cfg(target_arch = "wasm32")]
        return 0;
    }

    pub(super) fn spawn(&self, work: impl Future<Output = ()> + Send + 'static) {
        self.0.spawn(work).detach();
    }
}

pub(super) fn build_pool(kind: TaskPoolKind, workers: usize) -> Result<PoolBackend, TaskPoolError> {
    worker_start(kind, || {
        PoolBackend(
            TaskPoolBuilder::new()
                .num_threads(workers)
                .thread_name(format!("nara-task-{}", kind.as_str()))
                .build(),
        )
    })
}

pub(super) fn retirement_pool() -> Result<&'static PoolBackend, TaskPoolError> {
    static RETIREMENT: OnceLock<PoolBackend> = OnceLock::new();
    worker_start(TaskPoolKind::Io, || {
        RETIREMENT.get_or_init(|| {
            PoolBackend(
                TaskPoolBuilder::new()
                    .num_threads(2)
                    .thread_name("nara-task-retirement".to_owned())
                    .build(),
            )
        })
    })
}

fn worker_start<T>(
    kind: TaskPoolKind,
    build: impl FnOnce() -> T + std::panic::UnwindSafe,
) -> Result<T, TaskPoolError> {
    catch_unwind(build).map_err(|payload| {
        let message = payload
            .downcast_ref::<String>()
            .cloned()
            .or_else(|| {
                payload
                    .downcast_ref::<&str>()
                    .map(|message| (*message).to_owned())
            })
            .unwrap_or_else(|| "Bevy task pool startup panicked".to_owned());
        drop_catching(payload);
        TaskPoolError::WorkerSpawnFailed { kind, message }
    })
}
