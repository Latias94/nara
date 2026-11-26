use nara_ecs::bevy_ecs::schedule::Schedules;
use thiserror::Error;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionProfile {
    #[default]
    Serial,
    Parallel,
}

#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionProfileError {
    #[error("parallel execution requires the parallel-executor feature")]
    ParallelNotBuilt,
    #[error("parallel execution is unavailable on this target")]
    UnsupportedTarget,
    #[error("the Bevy compute pool has no worker threads")]
    EmptyComputePool,
}

impl ExecutionProfile {
    pub(crate) fn install(self, _schedules: &mut Schedules) -> Result<(), ExecutionProfileError> {
        if self == Self::Serial {
            return Ok(());
        }
        #[cfg(not(feature = "parallel-executor"))]
        return Err(ExecutionProfileError::ParallelNotBuilt);
        #[cfg(all(feature = "parallel-executor", target_arch = "wasm32"))]
        return Err(ExecutionProfileError::UnsupportedTarget);
        #[cfg(all(feature = "parallel-executor", not(target_arch = "wasm32")))]
        {
            use bevy_tasks::{ComputeTaskPool, TaskPoolBuilder};
            use nara_ecs::bevy_ecs::schedule::MultiThreadedExecutor;

            let pool = ComputeTaskPool::get_or_init(|| TaskPoolBuilder::new().build());
            if pool.thread_num() == 0 {
                return Err(ExecutionProfileError::EmptyComputePool);
            }
            for (_, schedule) in _schedules
                .iter_mut()
                .filter(|(_, schedule)| crate::is_built_in_schedule(schedule.label()))
            {
                schedule.set_executor(MultiThreadedExecutor::new());
            }
            Ok(())
        }
    }
}
