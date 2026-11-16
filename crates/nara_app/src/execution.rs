//! Composition-selected execution policy over Bevy's original schedule executors.

use std::{collections::HashMap, fmt};

use nara_ecs::bevy_ecs::schedule::{InternedScheduleLabel, Schedules, SingleThreadedExecutor};
use thiserror::Error;

use crate::PluginPlanFingerprint;

/// Selects the executor of every engine-driven Startup, Core, intent and frame-cleanup schedule.
///
/// Custom schedules retain their owner's executor. Parallel execution uses the existing
/// Host-initialized process-global Bevy ComputeTaskPool; it does not initialize or size a pool.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ExecutionProfile {
    #[default]
    Serial,
    Parallel,
}

/// A selected execution policy could not be admitted before Startup.
#[derive(Debug, Error, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionProfileError {
    #[error("parallel automatic execution requires the parallel-executor build feature")]
    ParallelNotBuilt,
    #[error("parallel automatic execution is unavailable on this target")]
    UnsupportedTarget,
    #[error("parallel automatic execution requires a Host-initialized Bevy ComputeTaskPool")]
    ComputePoolUnavailable,
    #[error("parallel automatic execution requires a ComputeTaskPool with worker threads")]
    EmptyComputePool,
}

impl ExecutionProfile {
    pub(crate) fn install(
        self,
        schedules: &mut Schedules,
        final_deferred: &HashMap<InternedScheduleLabel, bool>,
    ) -> Result<(), ExecutionProfileError> {
        self.validate()?;
        #[cfg(all(feature = "parallel-executor", not(target_arch = "wasm32")))]
        if self == Self::Parallel {
            use nara_ecs::bevy_ecs::schedule::MultiThreadedExecutor;

            for (_, schedule) in schedules.iter_mut() {
                if crate::is_built_in_schedule(schedule.label()) {
                    schedule.set_executor(MultiThreadedExecutor::new());
                    if let Some(apply) = final_deferred.get(&schedule.label()) {
                        schedule.set_apply_final_deferred(*apply);
                    }
                }
            }
            return Ok(());
        }
        for (_, schedule) in schedules.iter_mut() {
            if crate::is_built_in_schedule(schedule.label()) {
                schedule.set_executor(SingleThreadedExecutor::new());
                if let Some(apply) = final_deferred.get(&schedule.label()) {
                    schedule.set_apply_final_deferred(*apply);
                }
            }
        }
        Ok(())
    }

    fn validate(self) -> Result<(), ExecutionProfileError> {
        if self == Self::Serial {
            return Ok(());
        }
        #[cfg(not(feature = "parallel-executor"))]
        return Err(ExecutionProfileError::ParallelNotBuilt);
        #[cfg(all(feature = "parallel-executor", target_arch = "wasm32"))]
        return Err(ExecutionProfileError::UnsupportedTarget);
        #[cfg(all(feature = "parallel-executor", not(target_arch = "wasm32")))]
        {
            let pool = bevy_tasks::ComputeTaskPool::try_get()
                .ok_or(ExecutionProfileError::ComputePoolUnavailable)?;
            if pool.thread_num() == 0 {
                return Err(ExecutionProfileError::EmptyComputePool);
            }
            Ok(())
        }
    }
}

/// Identity of the admitted plugin plan and selected automatic execution policy.
///
/// Arbitrary Rust system graphs, raw schedule policies, resource values, worker counts and binary
/// provenance are separate facts. A plugin-plan fingerprint alone does not include this selection.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AppConfigurationFingerprint([u8; 32]);

impl AppConfigurationFingerprint {
    pub(crate) fn new(plan: PluginPlanFingerprint, profile: ExecutionProfile) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"nara.app.configuration/v1\0");
        hasher.update(&plan.as_bytes());
        hasher.update(match profile {
            ExecutionProfile::Serial => b"serial",
            ExecutionProfile::Parallel => b"bevy-parallel",
        });
        Self(*hasher.finalize().as_bytes())
    }

    #[must_use]
    pub const fn as_bytes(self) -> [u8; 32] {
        self.0
    }
}

impl fmt::Debug for AppConfigurationFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&blake3::Hash::from_bytes(self.0), formatter)
    }
}
