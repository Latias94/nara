//! Bevy task pools and typed futures.
//!
//! Keep each [`Task`] with the state that needs its result. Dropping that handle cancels the
//! task under Bevy's cancellation semantics. A detached task may outlive the App.

pub use bevy_tasks::{
    AsyncComputeTaskPool, ComputeTaskPool, IoTaskPool, Task, TaskPool, TaskPoolBuilder, block_on,
    poll_once,
};

/// Returns the process-wide pool for work that may span multiple frames.
/// Spawn with `async_compute_pool().spawn(future)` and keep the returned task with its owner.
pub fn async_compute_pool() -> &'static AsyncComputeTaskPool {
    AsyncComputeTaskPool::get_or_init(|| {
        TaskPoolBuilder::new()
            .num_threads(bevy_tasks::available_parallelism().min(4))
            .thread_name("Nara Async Compute".to_owned())
            .build()
    })
}

/// Returns the process-wide pool for I/O futures.
pub fn io_pool() -> &'static IoTaskPool {
    IoTaskPool::get_or_init(|| {
        TaskPoolBuilder::new()
            .num_threads(bevy_tasks::available_parallelism().min(2))
            .thread_name("Nara I/O".to_owned())
            .build()
    })
}
