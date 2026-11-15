#![cfg(all(feature = "parallel-executor", not(target_arch = "wasm32")))]

use bevy_tasks::{ComputeTaskPool, TaskPoolBuilder};
use nara_app::ExecutionProfileError;

#[path = "execution_profile/support.rs"]
mod support;

#[test]
fn empty_process_global_pool_refuses_before_startup_and_retires_the_original_failed_owner() {
    assert!(ComputeTaskPool::try_get().is_none());
    let pool = ComputeTaskPool::get_or_init(|| TaskPoolBuilder::new().num_threads(0).build());
    assert_eq!(pool.thread_num(), 0);

    support::assert_parallel_refused_before_startup(ExecutionProfileError::EmptyComputePool);

    let original =
        ComputeTaskPool::try_get().expect("the process-global pool must remain installed");
    assert!(std::ptr::eq(pool, original));
    assert_eq!(original.thread_num(), 0);
}
