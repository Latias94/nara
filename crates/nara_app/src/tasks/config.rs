use std::time::Duration;

use nara_core::{ItemLimit, TimeLimit};
use thiserror::Error;

pub const MAX_TASK_POOL_THREADS_PER_KIND: usize = 256;
pub const MAX_TASK_POOL_THREADS_TOTAL: usize = 512;
pub const MAX_TASK_POOL_PENDING_PER_KIND: usize = 1_048_576;
pub const MAX_TASK_POOL_PENDING_TOTAL: usize = 2_097_152;
pub const MAX_TASK_SHUTDOWN_PHASE_TIMEOUT: Duration = Duration::from_secs(30);
pub const MAX_TASK_SHUTDOWN_TOTAL_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskPoolKind {
    Io,
    Compute,
    AsyncCompute,
}

impl TaskPoolKind {
    pub const ALL: [Self; 3] = [Self::Io, Self::Compute, Self::AsyncCompute];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Io => "io",
            Self::Compute => "compute",
            Self::AsyncCompute => "async_compute",
        }
    }

    pub(super) const fn index(self) -> usize {
        match self {
            Self::Io => 0,
            Self::Compute => 1,
            Self::AsyncCompute => 2,
        }
    }
}

impl std::fmt::Display for TaskPoolKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskKindConfig {
    workers: ItemLimit,
    pending: ItemLimit,
}

impl TaskKindConfig {
    pub const fn new(workers: ItemLimit, pending: ItemLimit) -> Self {
        Self { workers, pending }
    }

    pub const fn workers(self) -> ItemLimit {
        self.workers
    }

    pub const fn pending(self) -> ItemLimit {
        self.pending
    }
}

impl Default for TaskKindConfig {
    fn default() -> Self {
        Self::new(ItemLimit::ONE, item_limit(256))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskShutdownPolicy {
    drain_timeout: TimeLimit,
    cancel_timeout: TimeLimit,
    join_timeout: TimeLimit,
}

impl TaskShutdownPolicy {
    pub const fn new(
        drain_timeout: TimeLimit,
        cancel_timeout: TimeLimit,
        join_timeout: TimeLimit,
    ) -> Self {
        Self {
            drain_timeout,
            cancel_timeout,
            join_timeout,
        }
    }

    pub const fn drain_timeout(self) -> TimeLimit {
        self.drain_timeout
    }

    pub const fn cancel_timeout(self) -> TimeLimit {
        self.cancel_timeout
    }

    pub const fn join_timeout(self) -> TimeLimit {
        self.join_timeout
    }
}

impl Default for TaskShutdownPolicy {
    fn default() -> Self {
        let grace = TimeLimit::new(Duration::from_millis(250)).unwrap_or(TimeLimit::MIN);
        let join = TimeLimit::new(Duration::from_millis(100)).unwrap_or(TimeLimit::MIN);
        Self::new(grace, grace, join)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskShutdownPhase {
    Drain,
    Cancel,
    Join,
}

impl std::fmt::Display for TaskShutdownPhase {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Drain => "drain",
            Self::Cancel => "cancel",
            Self::Join => "join",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskPoolConfig {
    kinds: [TaskKindConfig; 3],
    shutdown: TaskShutdownPolicy,
}

impl Default for TaskPoolConfig {
    fn default() -> Self {
        let parallelism = bevy_tasks::available_parallelism();
        let io = 2;
        let compute = parallelism.clamp(1, MAX_TASK_POOL_THREADS_PER_KIND);
        let background = (parallelism / 2).clamp(
            1,
            MAX_TASK_POOL_THREADS_PER_KIND.min(MAX_TASK_POOL_THREADS_TOTAL - io - compute),
        );
        Self {
            kinds: [
                TaskKindConfig::new(item_limit(io), item_limit(512)),
                TaskKindConfig::new(item_limit(compute), item_limit(512)),
                TaskKindConfig::new(item_limit(background), item_limit(256)),
            ],
            shutdown: TaskShutdownPolicy::default(),
        }
    }
}

impl TaskPoolConfig {
    pub fn new(
        io: TaskKindConfig,
        compute: TaskKindConfig,
        async_compute: TaskKindConfig,
        shutdown: TaskShutdownPolicy,
    ) -> Result<Self, TaskConfigError> {
        let config = Self {
            kinds: [io, compute, async_compute],
            shutdown,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn threaded(
        io_workers: ItemLimit,
        compute_workers: ItemLimit,
        async_compute_workers: ItemLimit,
    ) -> Result<Self, TaskConfigError> {
        let mut config = Self::default();
        for (kind, workers) in
            config
                .kinds
                .iter_mut()
                .zip([io_workers, compute_workers, async_compute_workers])
        {
            kind.workers = workers;
        }
        config.validate()?;
        Ok(config)
    }

    pub fn with_kind(
        mut self,
        kind: TaskPoolKind,
        kind_config: TaskKindConfig,
    ) -> Result<Self, TaskConfigError> {
        self.kinds[kind.index()] = kind_config;
        self.validate()?;
        Ok(self)
    }

    pub fn with_shutdown_policy(
        mut self,
        shutdown: TaskShutdownPolicy,
    ) -> Result<Self, TaskConfigError> {
        self.shutdown = shutdown;
        self.validate()?;
        Ok(self)
    }

    pub const fn kind(self, kind: TaskPoolKind) -> TaskKindConfig {
        self.kinds[kind.index()]
    }

    pub const fn shutdown_policy(self) -> TaskShutdownPolicy {
        self.shutdown
    }

    fn validate(self) -> Result<(), TaskConfigError> {
        let mut total_workers = 0;
        let mut total_pending = 0;
        for kind in TaskPoolKind::ALL {
            let config = self.kind(kind);
            let workers = config.workers().get();
            if workers > MAX_TASK_POOL_THREADS_PER_KIND {
                return Err(TaskConfigError::TooManyWorkers {
                    kind,
                    requested: workers,
                    maximum: MAX_TASK_POOL_THREADS_PER_KIND,
                });
            }
            let pending = config.pending().get();
            if pending > MAX_TASK_POOL_PENDING_PER_KIND {
                return Err(TaskConfigError::TooManyPending {
                    kind,
                    requested: pending,
                    maximum: MAX_TASK_POOL_PENDING_PER_KIND,
                });
            }
            total_workers += workers;
            total_pending += pending;
        }
        if total_workers > MAX_TASK_POOL_THREADS_TOTAL {
            return Err(TaskConfigError::TooManyTotalWorkers {
                requested: total_workers,
                maximum: MAX_TASK_POOL_THREADS_TOTAL,
            });
        }
        if total_pending > MAX_TASK_POOL_PENDING_TOTAL {
            return Err(TaskConfigError::TooManyTotalPending {
                requested: total_pending,
                maximum: MAX_TASK_POOL_PENDING_TOTAL,
            });
        }
        let phases = [
            (
                TaskShutdownPhase::Drain,
                self.shutdown.drain_timeout().get(),
            ),
            (
                TaskShutdownPhase::Cancel,
                self.shutdown.cancel_timeout().get(),
            ),
            (TaskShutdownPhase::Join, self.shutdown.join_timeout().get()),
        ];
        let mut total = Duration::ZERO;
        for (phase, timeout) in phases {
            if timeout > MAX_TASK_SHUTDOWN_PHASE_TIMEOUT {
                return Err(TaskConfigError::ShutdownPhaseTooLong {
                    phase,
                    requested: timeout,
                    maximum: MAX_TASK_SHUTDOWN_PHASE_TIMEOUT,
                });
            }
            total += timeout;
        }
        if total > MAX_TASK_SHUTDOWN_TOTAL_TIMEOUT {
            return Err(TaskConfigError::ShutdownTotalTooLong {
                requested: total,
                maximum: MAX_TASK_SHUTDOWN_TOTAL_TIMEOUT,
            });
        }
        Ok(())
    }
}

fn item_limit(value: usize) -> ItemLimit {
    ItemLimit::new(value).unwrap_or(ItemLimit::ONE)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TaskConfigError {
    #[error("{kind} task pool requested {requested} workers, maximum is {maximum}")]
    TooManyWorkers {
        kind: TaskPoolKind,
        requested: usize,
        maximum: usize,
    },
    #[error("task pools requested {requested} total workers, maximum is {maximum}")]
    TooManyTotalWorkers { requested: usize, maximum: usize },
    #[error("{kind} task pool requested {requested} pending tasks, maximum is {maximum}")]
    TooManyPending {
        kind: TaskPoolKind,
        requested: usize,
        maximum: usize,
    },
    #[error("task pools requested {requested} total pending tasks, maximum is {maximum}")]
    TooManyTotalPending { requested: usize, maximum: usize },
    #[error("task shutdown {phase} timeout {requested:?} exceeds maximum {maximum:?}")]
    ShutdownPhaseTooLong {
        phase: TaskShutdownPhase,
        requested: Duration,
        maximum: Duration,
    },
    #[error("task shutdown total timeout {requested:?} exceeds maximum {maximum:?}")]
    ShutdownTotalTooLong {
        requested: Duration,
        maximum: Duration,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TaskPoolError {
    #[error("invalid task pool configuration: {0}")]
    InvalidConfig(#[from] TaskConfigError),
    #[error("failed to spawn {kind} worker: {message}")]
    WorkerSpawnFailed { kind: TaskPoolKind, message: String },
}
