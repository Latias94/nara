use std::{
    collections::{BTreeMap, VecDeque},
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use nara_app::{App, CoreStage, Plugin, PluginCleanupContext, PluginError, TaskUpdateSet};
use nara_core::{ItemLimit, TimeLimit};
use nara_ecs::{Resource, schedule::IntoScheduleConfigs};

#[cfg(test)]
use std::{cell::RefCell, sync::atomic::AtomicUsize};

pub const MAX_TASK_POOL_THREADS_PER_KIND: usize = 256;
pub const MAX_TASK_POOL_THREADS_TOTAL: usize = 512;
pub const MAX_TASK_POOL_PENDING_PER_KIND: usize = 1_048_576;
pub const MAX_TASK_POOL_PENDING_TOTAL: usize = 2_097_152;
pub const MAX_TASK_SHUTDOWN_PHASE_TIMEOUT: Duration = Duration::from_secs(30);
pub const MAX_TASK_SHUTDOWN_TOTAL_TIMEOUT: Duration = Duration::from_secs(60);

pub(crate) fn default_worker_allocation(available_parallelism: usize) -> [usize; 3] {
    let io = 2.min(MAX_TASK_POOL_THREADS_PER_KIND);
    let remaining = MAX_TASK_POOL_THREADS_TOTAL.saturating_sub(io);
    let compute = available_parallelism
        .clamp(1, MAX_TASK_POOL_THREADS_PER_KIND)
        .min(remaining.saturating_sub(1));
    let async_compute = (available_parallelism / 2)
        .clamp(1, MAX_TASK_POOL_THREADS_PER_KIND)
        .min(remaining.saturating_sub(compute));
    [io, compute, async_compute]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskPoolKind {
    Io,
    Compute,
    AsyncCompute,
}

impl TaskPoolKind {
    pub const ALL: [Self; 3] = [Self::Io, Self::Compute, Self::AsyncCompute];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Io => "io",
            Self::Compute => "compute",
            Self::AsyncCompute => "async_compute",
        }
    }
}

impl Display for TaskPoolKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskKindConfig {
    workers: ItemLimit,
    pending: ItemLimit,
}

impl TaskKindConfig {
    #[must_use]
    pub const fn new(workers: ItemLimit, pending: ItemLimit) -> Self {
        Self { workers, pending }
    }

    #[must_use]
    pub const fn workers(self) -> ItemLimit {
        self.workers
    }

    #[must_use]
    pub const fn pending(self) -> ItemLimit {
        self.pending
    }
}

impl Default for TaskKindConfig {
    fn default() -> Self {
        Self::new(ItemLimit::ONE, item_limit_or_one(256))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskShutdownPolicy {
    drain_timeout: TimeLimit,
    cancel_timeout: TimeLimit,
    join_timeout: TimeLimit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskShutdownPhase {
    Drain,
    Cancel,
    Join,
}

impl Display for TaskShutdownPhase {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Drain => "drain",
            Self::Cancel => "cancel",
            Self::Join => "join",
        })
    }
}

impl TaskShutdownPolicy {
    #[must_use]
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

    #[must_use]
    pub const fn drain_timeout(self) -> TimeLimit {
        self.drain_timeout
    }

    #[must_use]
    pub const fn cancel_timeout(self) -> TimeLimit {
        self.cancel_timeout
    }

    #[must_use]
    pub const fn join_timeout(self) -> TimeLimit {
        self.join_timeout
    }
}

impl Default for TaskShutdownPolicy {
    fn default() -> Self {
        Self::new(
            time_limit_or_min(Duration::from_millis(250)),
            time_limit_or_min(Duration::from_millis(250)),
            time_limit_or_min(Duration::from_millis(100)),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskPoolConfig {
    io: TaskKindConfig,
    compute: TaskKindConfig,
    async_compute: TaskKindConfig,
    shutdown: TaskShutdownPolicy,
}

impl Default for TaskPoolConfig {
    fn default() -> Self {
        let parallelism = thread::available_parallelism().map_or(1, usize::from);
        let [io_workers, compute_workers, async_workers] = default_worker_allocation(parallelism);
        Self {
            io: TaskKindConfig::new(item_limit_or_one(io_workers), item_limit_or_one(512)),
            compute: TaskKindConfig::new(
                item_limit_or_one(compute_workers),
                item_limit_or_one(512),
            ),
            async_compute: TaskKindConfig::new(
                item_limit_or_one(async_workers),
                item_limit_or_one(256),
            ),
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
            io,
            compute,
            async_compute,
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
        let defaults = Self::default();
        Self::new(
            TaskKindConfig::new(io_workers, defaults.io.pending()),
            TaskKindConfig::new(compute_workers, defaults.compute.pending()),
            TaskKindConfig::new(async_compute_workers, defaults.async_compute.pending()),
            defaults.shutdown,
        )
    }

    pub fn with_kind(
        mut self,
        kind: TaskPoolKind,
        kind_config: TaskKindConfig,
    ) -> Result<Self, TaskConfigError> {
        match kind {
            TaskPoolKind::Io => self.io = kind_config,
            TaskPoolKind::Compute => self.compute = kind_config,
            TaskPoolKind::AsyncCompute => self.async_compute = kind_config,
        }
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

    #[must_use]
    pub const fn kind(self, kind: TaskPoolKind) -> TaskKindConfig {
        match kind {
            TaskPoolKind::Io => self.io,
            TaskPoolKind::Compute => self.compute,
            TaskPoolKind::AsyncCompute => self.async_compute,
        }
    }

    #[must_use]
    pub const fn shutdown_policy(self) -> TaskShutdownPolicy {
        self.shutdown
    }

    fn validate(self) -> Result<(), TaskConfigError> {
        let mut total_workers = 0_usize;
        let mut total_pending = 0_usize;
        for kind in TaskPoolKind::ALL {
            let kind_config = self.kind(kind);
            let workers = kind_config.workers().get();
            if workers > MAX_TASK_POOL_THREADS_PER_KIND {
                return Err(TaskConfigError::TooManyWorkers {
                    kind,
                    requested: workers,
                    maximum: MAX_TASK_POOL_THREADS_PER_KIND,
                });
            }
            total_workers = total_workers.saturating_add(workers);
            let pending = kind_config.pending().get();
            if pending > MAX_TASK_POOL_PENDING_PER_KIND {
                return Err(TaskConfigError::TooManyPending {
                    kind,
                    requested: pending,
                    maximum: MAX_TASK_POOL_PENDING_PER_KIND,
                });
            }
            total_pending = total_pending.saturating_add(pending);
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

        let shutdown = self.shutdown_policy();
        for (phase, timeout) in [
            (TaskShutdownPhase::Drain, shutdown.drain_timeout().get()),
            (TaskShutdownPhase::Cancel, shutdown.cancel_timeout().get()),
            (TaskShutdownPhase::Join, shutdown.join_timeout().get()),
        ] {
            if timeout > MAX_TASK_SHUTDOWN_PHASE_TIMEOUT {
                return Err(TaskConfigError::ShutdownPhaseTooLong {
                    phase,
                    requested: timeout,
                    maximum: MAX_TASK_SHUTDOWN_PHASE_TIMEOUT,
                });
            }
        }
        let total_shutdown = shutdown
            .drain_timeout()
            .get()
            .saturating_add(shutdown.cancel_timeout().get())
            .saturating_add(shutdown.join_timeout().get());
        if total_shutdown > MAX_TASK_SHUTDOWN_TOTAL_TIMEOUT {
            return Err(TaskConfigError::ShutdownTotalTooLong {
                requested: total_shutdown,
                maximum: MAX_TASK_SHUTDOWN_TOTAL_TIMEOUT,
            });
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) const fn unchecked_for_tests(
        io: TaskKindConfig,
        compute: TaskKindConfig,
        async_compute: TaskKindConfig,
        shutdown: TaskShutdownPolicy,
    ) -> Self {
        Self {
            io,
            compute,
            async_compute,
            shutdown,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskConfigError {
    TooManyWorkers {
        kind: TaskPoolKind,
        requested: usize,
        maximum: usize,
    },
    TooManyTotalWorkers {
        requested: usize,
        maximum: usize,
    },
    TooManyPending {
        kind: TaskPoolKind,
        requested: usize,
        maximum: usize,
    },
    TooManyTotalPending {
        requested: usize,
        maximum: usize,
    },
    ShutdownPhaseTooLong {
        phase: TaskShutdownPhase,
        requested: Duration,
        maximum: Duration,
    },
    ShutdownTotalTooLong {
        requested: Duration,
        maximum: Duration,
    },
}

impl Display for TaskConfigError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooManyWorkers {
                kind,
                requested,
                maximum,
            } => write!(
                formatter,
                "{kind} task pool requested {requested} workers, maximum is {maximum}"
            ),
            Self::TooManyTotalWorkers { requested, maximum } => write!(
                formatter,
                "task pools requested {requested} total workers, maximum is {maximum}"
            ),
            Self::TooManyPending {
                kind,
                requested,
                maximum,
            } => write!(
                formatter,
                "{kind} task pool requested {requested} pending tasks, maximum is {maximum}"
            ),
            Self::TooManyTotalPending { requested, maximum } => write!(
                formatter,
                "task pools requested {requested} total pending tasks, maximum is {maximum}"
            ),
            Self::ShutdownPhaseTooLong {
                phase,
                requested,
                maximum,
            } => write!(
                formatter,
                "task shutdown {phase} timeout {requested:?} exceeds maximum {maximum:?}"
            ),
            Self::ShutdownTotalTooLong { requested, maximum } => write!(
                formatter,
                "task shutdown total timeout {requested:?} exceeds maximum {maximum:?}"
            ),
        }
    }
}

impl Error for TaskConfigError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskPoolError {
    InvalidConfig(TaskConfigError),
    WorkerSpawnFailed { kind: TaskPoolKind, message: String },
}

impl Display for TaskPoolError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(error) => {
                write!(formatter, "invalid task pool configuration: {error}")
            }
            Self::WorkerSpawnFailed { kind, message } => {
                write!(formatter, "failed to spawn {kind} worker: {message}")
            }
        }
    }
}

impl Error for TaskPoolError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(u64);

impl TaskId {
    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskDomainKey(u64);

impl TaskDomainKey {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskCoalesceKey(u64);

impl TaskCoalesceKey {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskOrderKey {
    admission_tick: u64,
    domain_key: TaskDomainKey,
    task_id: TaskId,
}

impl TaskOrderKey {
    #[must_use]
    pub const fn new(admission_tick: u64, domain_key: TaskDomainKey, task_id: TaskId) -> Self {
        Self {
            admission_tick,
            domain_key,
            task_id,
        }
    }

    #[must_use]
    pub const fn admission_tick(self) -> u64 {
        self.admission_tick
    }

    #[must_use]
    pub const fn domain_key(self) -> TaskDomainKey {
        self.domain_key
    }

    #[must_use]
    pub const fn task_id(self) -> TaskId {
        self.task_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskDescriptor {
    id: TaskId,
    kind: TaskPoolKind,
    order_key: TaskOrderKey,
}

impl TaskDescriptor {
    #[must_use]
    pub const fn id(self) -> TaskId {
        self.id
    }

    #[must_use]
    pub const fn kind(self) -> TaskPoolKind {
        self.kind
    }

    #[must_use]
    pub const fn order_key(self) -> TaskOrderKey {
        self.order_key
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TaskOverloadPolicy {
    #[default]
    Reject,
    CoalescePending(TaskCoalesceKey),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskSpawnRequest {
    admission_tick: u64,
    domain_key: TaskDomainKey,
    overload: TaskOverloadPolicy,
}

impl TaskSpawnRequest {
    #[must_use]
    pub const fn new(admission_tick: u64, domain_key: TaskDomainKey) -> Self {
        Self {
            admission_tick,
            domain_key,
            overload: TaskOverloadPolicy::Reject,
        }
    }

    #[must_use]
    pub const fn with_overload(mut self, overload: TaskOverloadPolicy) -> Self {
        self.overload = overload;
        self
    }

    #[must_use]
    pub const fn admission_tick(self) -> u64 {
        self.admission_tick
    }

    #[must_use]
    pub const fn domain_key(self) -> TaskDomainKey {
        self.domain_key
    }

    #[must_use]
    pub const fn overload(self) -> TaskOverloadPolicy {
        self.overload
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskRejectReason {
    QueueFull { capacity: usize },
    PoolClosed,
    TaskIdExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskRejection {
    pub task: Option<TaskDescriptor>,
    pub kind: TaskPoolKind,
    pub admission_tick: u64,
    pub domain_key: TaskDomainKey,
    pub reason: TaskRejectReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskCancellationReason {
    Requested,
    Coalesced { replacement: TaskId },
    PoolShutdown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskCancellation {
    pub reason: TaskCancellationReason,
    pub before_start: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskFailure {
    /// The worker caught a task panic and discarded its payload.
    ///
    /// Rust invokes the process panic hook before unwinding reaches this boundary. Hook output and
    /// redaction therefore remain host responsibilities; this value never retains the payload.
    Panicked,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskTerminalState {
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskTerminal<T> {
    Completed(T),
    Cancelled(TaskCancellation),
    Failed(TaskFailure),
}

impl<T> TaskTerminal<T> {
    #[must_use]
    pub const fn state(&self) -> TaskTerminalState {
        match self {
            Self::Completed(_) => TaskTerminalState::Completed,
            Self::Cancelled(_) => TaskTerminalState::Cancelled,
            Self::Failed(_) => TaskTerminalState::Failed,
        }
    }

    pub fn into_completed(self) -> Option<T> {
        match self {
            Self::Completed(value) => Some(value),
            Self::Cancelled(_) | Self::Failed(_) => None,
        }
    }
}

#[derive(Clone, Default)]
pub struct TaskCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl TaskCancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn mark_cancelled(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}

impl Debug for TaskCancellationToken {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskCancellationToken")
            .field("cancelled", &self.is_cancelled())
            .finish()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct TaskCounters {
    admitted: u64,
    rejected: u64,
    coalesced: u64,
    started: u64,
    completed: u64,
    failed: u64,
    cancelled: u64,
    taken: u64,
    shutdowns: u64,
    shutdown_timeouts: u64,
    detached_workers: u64,
}

type SharedTaskCounters = Arc<Mutex<TaskCounters>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskPhase {
    Pending,
    Running,
    Terminal,
}

struct TaskStateInner<T> {
    phase: TaskPhase,
    terminal: Option<TaskTerminal<T>>,
}

struct TaskState<T> {
    inner: Mutex<TaskStateInner<T>>,
    finished: AtomicBool,
    token: TaskCancellationToken,
    counters: SharedTaskCounters,
}

impl<T> TaskState<T> {
    fn new(counters: SharedTaskCounters) -> Self {
        Self {
            inner: Mutex::new(TaskStateInner {
                phase: TaskPhase::Pending,
                terminal: None,
            }),
            finished: AtomicBool::new(false),
            token: TaskCancellationToken::new(),
            counters,
        }
    }

    fn mark_running(&self) -> bool {
        let mut inner = lock_unpoisoned(&self.inner);
        if inner.phase != TaskPhase::Pending {
            return false;
        }
        inner.phase = TaskPhase::Running;
        drop(inner);
        update_counters(&self.counters, |counters| {
            counters.started = counters.started.saturating_add(1);
        });
        true
    }

    fn complete(&self, value: T) -> bool {
        let mut inner = lock_unpoisoned(&self.inner);
        if inner.phase == TaskPhase::Terminal {
            drop(inner);
            drop(value);
            return false;
        }
        inner.phase = TaskPhase::Terminal;
        inner.terminal = Some(TaskTerminal::Completed(value));
        self.finished.store(true, Ordering::Release);
        drop(inner);
        update_counters(&self.counters, |counters| {
            counters.completed = counters.completed.saturating_add(1);
        });
        true
    }

    fn fail(&self, failure: TaskFailure) -> bool {
        let mut inner = lock_unpoisoned(&self.inner);
        if inner.phase == TaskPhase::Terminal {
            return false;
        }
        inner.phase = TaskPhase::Terminal;
        inner.terminal = Some(TaskTerminal::Failed(failure));
        self.finished.store(true, Ordering::Release);
        drop(inner);
        update_counters(&self.counters, |counters| {
            counters.failed = counters.failed.saturating_add(1);
        });
        true
    }

    fn cancel(&self, reason: TaskCancellationReason) -> bool {
        let mut inner = lock_unpoisoned(&self.inner);
        if inner.phase == TaskPhase::Terminal {
            return false;
        }
        let before_start = inner.phase == TaskPhase::Pending;
        inner.phase = TaskPhase::Terminal;
        inner.terminal = Some(TaskTerminal::Cancelled(TaskCancellation {
            reason,
            before_start,
        }));
        self.token.mark_cancelled();
        self.finished.store(true, Ordering::Release);
        drop(inner);
        update_counters(&self.counters, |counters| {
            counters.cancelled = counters.cancelled.saturating_add(1);
        });
        true
    }

    fn is_terminal(&self) -> bool {
        self.finished.load(Ordering::Acquire)
    }

    fn try_take(&self) -> Option<TaskTerminal<T>> {
        if !self.is_terminal() {
            return None;
        }
        let terminal = lock_unpoisoned(&self.inner).terminal.take();
        if terminal.is_some() {
            update_counters(&self.counters, |counters| {
                counters.taken = counters.taken.saturating_add(1);
            });
        }
        terminal
    }
}

impl<T> Drop for TaskState<T> {
    fn drop(&mut self) {
        let inner = self
            .inner
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(terminal) = inner.terminal.take() {
            let _ = catch_unwind(AssertUnwindSafe(|| drop(terminal)));
        }
    }
}

trait ErasedTaskState: Send + Sync {
    fn mark_running(&self) -> bool;
    fn cancel(&self, reason: TaskCancellationReason) -> bool;
    fn fail(&self, failure: TaskFailure) -> bool;
    fn is_terminal(&self) -> bool;
}

impl<T: Send + 'static> ErasedTaskState for TaskState<T> {
    fn mark_running(&self) -> bool {
        TaskState::mark_running(self)
    }

    fn cancel(&self, reason: TaskCancellationReason) -> bool {
        TaskState::cancel(self, reason)
    }

    fn fail(&self, failure: TaskFailure) -> bool {
        TaskState::fail(self, failure)
    }

    fn is_terminal(&self) -> bool {
        TaskState::is_terminal(self)
    }
}

#[derive(Clone)]
struct TaskControl {
    state: Arc<dyn ErasedTaskState>,
}

pub struct TaskHandle<T> {
    descriptor: TaskDescriptor,
    state: Arc<TaskState<T>>,
}

impl<T> TaskHandle<T> {
    #[must_use]
    pub const fn id(&self) -> TaskId {
        self.descriptor.id()
    }

    #[must_use]
    pub const fn kind(&self) -> TaskPoolKind {
        self.descriptor.kind()
    }

    #[must_use]
    pub const fn descriptor(&self) -> TaskDescriptor {
        self.descriptor
    }

    #[must_use]
    pub const fn admission_tick(&self) -> u64 {
        self.descriptor.order_key().admission_tick()
    }

    #[must_use]
    pub const fn domain_key(&self) -> TaskDomainKey {
        self.descriptor.order_key().domain_key()
    }

    #[must_use]
    pub const fn order_key(&self) -> TaskOrderKey {
        self.descriptor.order_key()
    }

    #[must_use]
    pub fn cancellation_token(&self) -> TaskCancellationToken {
        self.state.token.clone()
    }

    pub fn cancel(&self) -> bool {
        self.state.cancel(TaskCancellationReason::Requested)
    }

    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.state.is_terminal()
    }

    pub fn try_take(&mut self) -> Option<TaskTerminal<T>> {
        self.state.try_take()
    }
}

impl<T> Debug for TaskHandle<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskHandle")
            .field("descriptor", &self.descriptor)
            .field("finished", &self.is_finished())
            .field("cancelled", &self.state.token.is_cancelled())
            .finish()
    }
}

pub enum TaskSpawnOutcome<T> {
    Accepted(TaskHandle<T>),
    Coalesced {
        handle: TaskHandle<T>,
        replaced: TaskId,
    },
    Rejected(TaskRejection),
}

impl<T> TaskSpawnOutcome<T> {
    #[must_use]
    pub fn handle(&self) -> Option<&TaskHandle<T>> {
        match self {
            Self::Accepted(handle) | Self::Coalesced { handle, .. } => Some(handle),
            Self::Rejected(_) => None,
        }
    }

    pub fn into_handle(self) -> Result<TaskHandle<T>, TaskRejection> {
        match self {
            Self::Accepted(handle) | Self::Coalesced { handle, .. } => Ok(handle),
            Self::Rejected(rejection) => Err(rejection),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderedTaskTerminal<T> {
    pub order_key: TaskOrderKey,
    pub terminal: TaskTerminal<T>,
}

pub struct OrderedTaskResults<T> {
    pending: BTreeMap<TaskOrderKey, TaskHandle<T>>,
}

impl<T> Default for OrderedTaskResults<T> {
    fn default() -> Self {
        Self {
            pending: BTreeMap::new(),
        }
    }
}

impl<T> OrderedTaskResults<T> {
    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn push(&mut self, handle: TaskHandle<T>) -> Result<(), TaskHandle<T>> {
        let key = handle.order_key();
        if self.pending.contains_key(&key) {
            return Err(handle);
        }
        self.pending.insert(key, handle);
        Ok(())
    }

    pub fn drain_ready_prefix(&mut self) -> Vec<OrderedTaskTerminal<T>> {
        let mut ready = Vec::new();
        while let Some((&key, handle)) = self.pending.first_key_value() {
            if !handle.is_finished() {
                break;
            }
            let Some(mut handle) = self.pending.remove(&key) else {
                break;
            };
            if let Some(terminal) = handle.try_take() {
                ready.push(OrderedTaskTerminal {
                    order_key: key,
                    terminal,
                });
            }
        }
        ready
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TaskPoolStats {
    pub admitted: u64,
    pub rejected: u64,
    pub coalesced: u64,
    pub queued: usize,
    pub running: usize,
    pub started: u64,
    pub completed: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub taken: u64,
    pub oldest_queued_age: Option<Duration>,
    pub oldest_running_age: Option<Duration>,
    pub shutdowns: u64,
    pub shutdown_timeouts: u64,
    pub detached_workers: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskStats {
    per_kind: BTreeMap<TaskPoolKind, TaskPoolStats>,
}

impl Default for TaskStats {
    fn default() -> Self {
        let mut per_kind = BTreeMap::new();
        for kind in TaskPoolKind::ALL {
            per_kind.insert(kind, TaskPoolStats::default());
        }
        Self { per_kind }
    }
}

impl TaskStats {
    #[must_use]
    pub fn for_kind(&self, kind: TaskPoolKind) -> TaskPoolStats {
        self.per_kind.get(&kind).copied().unwrap_or_default()
    }

    fn insert(&mut self, kind: TaskPoolKind, stats: TaskPoolStats) {
        self.per_kind.insert(kind, stats);
    }
}

struct TaskPoolInstanceToken;

#[derive(Resource)]
pub struct TaskPools {
    config: TaskPoolConfig,
    next_id: AtomicU64,
    instance_token: Arc<TaskPoolInstanceToken>,
    io: TaskPoolExecutor,
    compute: TaskPoolExecutor,
    async_compute: TaskPoolExecutor,
    shutdown_report: Option<TaskShutdownReport>,
}

impl TaskPools {
    pub fn try_new(config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        config.validate().map_err(TaskPoolError::InvalidConfig)?;
        let io = TaskPoolExecutor::threaded(TaskPoolKind::Io, config)?;
        let compute = TaskPoolExecutor::threaded(TaskPoolKind::Compute, config)?;
        let async_compute = TaskPoolExecutor::threaded(TaskPoolKind::AsyncCompute, config)?;
        Ok(Self {
            config,
            next_id: AtomicU64::new(1),
            instance_token: Arc::new(TaskPoolInstanceToken),
            io,
            compute,
            async_compute,
            shutdown_report: None,
        })
    }

    pub fn inline_for_tests(config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        config.validate().map_err(TaskPoolError::InvalidConfig)?;
        Ok(Self {
            config,
            next_id: AtomicU64::new(1),
            instance_token: Arc::new(TaskPoolInstanceToken),
            io: TaskPoolExecutor::inline(TaskPoolKind::Io, config),
            compute: TaskPoolExecutor::inline(TaskPoolKind::Compute, config),
            async_compute: TaskPoolExecutor::inline(TaskPoolKind::AsyncCompute, config),
            shutdown_report: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn try_new_with_worker_spawn_failure_for_tests(
        config: TaskPoolConfig,
        kind: TaskPoolKind,
        fail_index: usize,
    ) -> (Result<Self, TaskPoolError>, WorkerSpawnFailureProbe) {
        let probe = WorkerSpawnFailureProbe::default();
        let previous = WORKER_SPAWN_FAILURE.with(|injection| {
            injection.replace(Some(WorkerSpawnFailureInjection {
                kind,
                fail_index,
                started: probe.started.clone(),
                exited: probe.exited.clone(),
            }))
        });
        assert!(
            previous.is_none(),
            "worker spawn failure injection is nested"
        );
        let _reset = WorkerSpawnFailureReset;
        (Self::try_new(config), probe)
    }

    #[must_use]
    pub const fn config(&self) -> &TaskPoolConfig {
        &self.config
    }

    #[must_use]
    pub fn stats(&self) -> TaskStats {
        let mut stats = TaskStats::default();
        for kind in TaskPoolKind::ALL {
            stats.insert(kind, self.executor(kind).stats());
        }
        stats
    }

    pub fn spawn<T, F>(
        &self,
        kind: TaskPoolKind,
        request: TaskSpawnRequest,
        function: F,
    ) -> TaskSpawnOutcome<T>
    where
        T: Send + 'static,
        F: FnOnce(TaskCancellationToken) -> T + Send + 'static,
    {
        let Some(id) = self.allocate_task_id() else {
            self.executor(kind).record_rejected();
            let _ = catch_unwind(AssertUnwindSafe(|| drop(function)));
            return TaskSpawnOutcome::Rejected(TaskRejection {
                task: None,
                kind,
                admission_tick: request.admission_tick(),
                domain_key: request.domain_key(),
                reason: TaskRejectReason::TaskIdExhausted,
            });
        };
        let order_key = TaskOrderKey::new(request.admission_tick(), request.domain_key(), id);
        let descriptor = TaskDescriptor {
            id,
            kind,
            order_key,
        };
        let counters = self.executor(kind).shared.counters.clone();
        let state = Arc::new(TaskState::new(counters));
        let handle = TaskHandle {
            descriptor,
            state: state.clone(),
        };
        let control = TaskControl {
            state: state.clone(),
        };
        let pending = PendingJob {
            descriptor,
            coalesce_key: match request.overload() {
                TaskOverloadPolicy::Reject => None,
                TaskOverloadPolicy::CoalescePending(key) => Some(key),
            },
            admitted_at: Instant::now(),
            control,
            job: Box::new(TaskJob {
                function: Some(function),
                state,
            }),
        };

        match self.executor(kind).admit(pending) {
            AdmissionDecision::Accepted => TaskSpawnOutcome::Accepted(handle),
            AdmissionDecision::Coalesced { replaced } => {
                TaskSpawnOutcome::Coalesced { handle, replaced }
            }
            AdmissionDecision::Rejected(reason) => TaskSpawnOutcome::Rejected(TaskRejection {
                task: Some(descriptor),
                kind,
                admission_tick: request.admission_tick(),
                domain_key: request.domain_key(),
                reason,
            }),
        }
    }

    #[must_use]
    pub fn run_pending_for_tests(&self) -> TaskInlineRunReport {
        let mut report = TaskInlineRunReport::default();
        loop {
            let mut progressed = false;
            for kind in TaskPoolKind::ALL {
                match self.executor(kind).run_one_inline() {
                    InlineStep::Executed => {
                        report.executed = report.executed.saturating_add(1);
                        progressed = true;
                    }
                    InlineStep::DiscardedCancelled => {
                        report.cancelled_before_start =
                            report.cancelled_before_start.saturating_add(1);
                        progressed = true;
                    }
                    InlineStep::Empty => {}
                }
            }
            if !progressed {
                return report;
            }
        }
    }

    pub fn shutdown(&mut self) -> TaskShutdownReport {
        if let Some(report) = &self.shutdown_report {
            return report.repeated();
        }
        let mut report = TaskShutdownReport::default();
        for kind in TaskPoolKind::ALL {
            report
                .per_kind
                .insert(kind, self.executor_mut(kind).shutdown());
        }
        self.shutdown_report = Some(report.clone());
        report
    }

    fn allocate_task_id(&self) -> Option<TaskId> {
        self.next_id
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                next.checked_add(1)
            })
            .ok()
            .map(TaskId)
    }

    fn ownership_token(&self) -> Arc<TaskPoolInstanceToken> {
        self.instance_token.clone()
    }

    fn has_ownership_token(&self, token: &Arc<TaskPoolInstanceToken>) -> bool {
        Arc::ptr_eq(&self.instance_token, token)
    }

    #[cfg(test)]
    pub(crate) fn set_next_task_id_for_tests(&self, next_id: u64) {
        self.next_id.store(next_id, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub(crate) fn set_before_coalesce_cancel_hook_for_tests(
        &self,
        kind: TaskPoolKind,
        hook: Arc<dyn Fn() + Send + Sync>,
    ) {
        self.executor(kind)
            .shared
            .set_before_coalesce_cancel_hook(hook);
    }

    #[cfg(test)]
    pub(crate) fn shutdown_executor_for_tests(
        &mut self,
        kind: TaskPoolKind,
    ) -> TaskPoolShutdownReport {
        self.executor_mut(kind).shutdown()
    }

    fn executor(&self, kind: TaskPoolKind) -> &TaskPoolExecutor {
        match kind {
            TaskPoolKind::Io => &self.io,
            TaskPoolKind::Compute => &self.compute,
            TaskPoolKind::AsyncCompute => &self.async_compute,
        }
    }

    fn executor_mut(&mut self, kind: TaskPoolKind) -> &mut TaskPoolExecutor {
        match kind {
            TaskPoolKind::Io => &mut self.io,
            TaskPoolKind::Compute => &mut self.compute,
            TaskPoolKind::AsyncCompute => &mut self.async_compute,
        }
    }
}

impl Debug for TaskPools {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskPools")
            .field("config", &self.config)
            .field("stats", &self.stats())
            .finish_non_exhaustive()
    }
}

impl Drop for TaskPools {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TaskInlineRunReport {
    pub executed: usize,
    pub cancelled_before_start: usize,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TaskPoolShutdownReport {
    pub already_shutdown: bool,
    pub cancelled_pending: usize,
    pub cancellation_requests: usize,
    pub joined_workers: usize,
    pub detached_workers: usize,
    pub panicked_workers: usize,
    pub drain_timed_out: bool,
    pub cancel_timed_out: bool,
    pub join_timed_out: bool,
}

impl TaskPoolShutdownReport {
    #[must_use]
    pub const fn timed_out(self) -> bool {
        self.drain_timed_out || self.cancel_timed_out || self.join_timed_out
    }
}

#[derive(Resource, Debug, Clone, PartialEq, Eq)]
pub struct TaskShutdownReport {
    per_kind: BTreeMap<TaskPoolKind, TaskPoolShutdownReport>,
}

impl Default for TaskShutdownReport {
    fn default() -> Self {
        let mut per_kind = BTreeMap::new();
        for kind in TaskPoolKind::ALL {
            per_kind.insert(kind, TaskPoolShutdownReport::default());
        }
        Self { per_kind }
    }
}

impl TaskShutdownReport {
    #[must_use]
    pub fn for_kind(&self, kind: TaskPoolKind) -> TaskPoolShutdownReport {
        self.per_kind.get(&kind).copied().unwrap_or_default()
    }

    #[must_use]
    pub fn timed_out(&self) -> bool {
        TaskPoolKind::ALL
            .into_iter()
            .any(|kind| self.for_kind(kind).timed_out())
    }

    #[must_use]
    pub fn detached_workers(&self) -> usize {
        TaskPoolKind::ALL
            .into_iter()
            .map(|kind| self.for_kind(kind).detached_workers)
            .sum()
    }

    fn repeated(&self) -> Self {
        let mut report = self.clone();
        for pool in report.per_kind.values_mut() {
            pool.already_shutdown = true;
        }
        report
    }
}

trait ErasedJob: Send {
    fn run(&mut self);
}

struct TaskJob<T, F> {
    function: Option<F>,
    state: Arc<TaskState<T>>,
}

impl<T, F> ErasedJob for TaskJob<T, F>
where
    T: Send + 'static,
    F: FnOnce(TaskCancellationToken) -> T + Send + 'static,
{
    fn run(&mut self) {
        let Some(function) = self.function.take() else {
            self.state.fail(TaskFailure::Panicked);
            return;
        };
        let value = function(self.state.token.clone());
        self.state.complete(value);
    }
}

struct PendingJob {
    descriptor: TaskDescriptor,
    coalesce_key: Option<TaskCoalesceKey>,
    admitted_at: Instant,
    control: TaskControl,
    job: Box<dyn ErasedJob>,
}

struct RunningJob {
    control: TaskControl,
    started_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueueLifecycle {
    Open,
    Draining,
    Closed,
}

struct QueueState {
    lifecycle: QueueLifecycle,
    pending: VecDeque<PendingJob>,
    running: BTreeMap<TaskId, RunningJob>,
    live_workers: usize,
}

impl QueueState {
    fn new(live_workers: usize) -> Self {
        Self {
            lifecycle: QueueLifecycle::Open,
            pending: VecDeque::new(),
            running: BTreeMap::new(),
            live_workers,
        }
    }
}

struct ExecutorShared {
    pending_capacity: usize,
    queue: Mutex<QueueState>,
    changed: Condvar,
    counters: SharedTaskCounters,
    #[cfg(test)]
    before_coalesce_cancel_hook: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl ExecutorShared {
    fn new(pending_capacity: usize, live_workers: usize) -> Self {
        Self {
            pending_capacity,
            queue: Mutex::new(QueueState::new(live_workers)),
            changed: Condvar::new(),
            counters: Arc::new(Mutex::new(TaskCounters::default())),
            #[cfg(test)]
            before_coalesce_cancel_hook: Mutex::new(None),
        }
    }

    #[cfg(test)]
    fn set_before_coalesce_cancel_hook(&self, hook: Arc<dyn Fn() + Send + Sync>) {
        *lock_unpoisoned(&self.before_coalesce_cancel_hook) = Some(hook);
    }

    #[cfg(test)]
    fn run_before_coalesce_cancel_hook(&self) {
        let hook = lock_unpoisoned(&self.before_coalesce_cancel_hook).take();
        if let Some(hook) = hook {
            hook();
        }
    }

    fn stats(&self) -> TaskPoolStats {
        let queue = lock_unpoisoned(&self.queue);
        let now = Instant::now();
        let queued = queue
            .pending
            .iter()
            .filter(|pending| !pending.control.state.is_terminal())
            .count();
        let oldest_queued_age = queue
            .pending
            .iter()
            .filter(|pending| !pending.control.state.is_terminal())
            .map(|pending| now.saturating_duration_since(pending.admitted_at))
            .max();
        let running = queue.running.len();
        let oldest_running_age = queue
            .running
            .values()
            .map(|running| now.saturating_duration_since(running.started_at))
            .max();
        let counters = *lock_unpoisoned(&self.counters);
        TaskPoolStats {
            admitted: counters.admitted,
            rejected: counters.rejected,
            coalesced: counters.coalesced,
            queued,
            running,
            started: counters.started,
            completed: counters.completed,
            failed: counters.failed,
            cancelled: counters.cancelled,
            taken: counters.taken,
            oldest_queued_age,
            oldest_running_age,
            shutdowns: counters.shutdowns,
            shutdown_timeouts: counters.shutdown_timeouts,
            detached_workers: counters.detached_workers,
        }
    }

    fn take_next(&self, wait: bool) -> NextJob {
        let mut queue = lock_unpoisoned(&self.queue);
        loop {
            if let Some(pending) = queue.pending.pop_front() {
                if pending.control.state.mark_running() {
                    queue.running.insert(
                        pending.descriptor.id(),
                        RunningJob {
                            control: pending.control.clone(),
                            started_at: Instant::now(),
                        },
                    );
                    return NextJob::Ready(pending);
                }
                return NextJob::Discarded(pending);
            }
            if queue.lifecycle != QueueLifecycle::Open {
                return NextJob::Exit;
            }
            if !wait {
                return NextJob::Empty;
            }
            queue = wait_unpoisoned(&self.changed, queue);
        }
    }

    fn finish_running(&self, id: TaskId) {
        let mut queue = lock_unpoisoned(&self.queue);
        queue.running.remove(&id);
        self.changed.notify_all();
    }

    fn worker_exited(&self) {
        let mut queue = lock_unpoisoned(&self.queue);
        queue.live_workers = queue.live_workers.saturating_sub(1);
        self.changed.notify_all();
    }
}

enum AdmissionDecision {
    Accepted,
    Coalesced { replaced: TaskId },
    Rejected(TaskRejectReason),
}

enum NextJob {
    Ready(PendingJob),
    Discarded(PendingJob),
    Empty,
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InlineStep {
    Executed,
    DiscardedCancelled,
    Empty,
}

struct TaskPoolExecutor {
    shared: Arc<ExecutorShared>,
    workers: Vec<JoinHandle<()>>,
    shutdown_policy: TaskShutdownPolicy,
    inline: bool,
    shutdown_report: Option<TaskPoolShutdownReport>,
}

impl TaskPoolExecutor {
    fn threaded(kind: TaskPoolKind, config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        let kind_config = config.kind(kind);
        let thread_count = kind_config.workers().get();
        let shared = Arc::new(ExecutorShared::new(
            kind_config.pending().get(),
            thread_count,
        ));
        let mut workers: Vec<JoinHandle<()>> = Vec::with_capacity(thread_count);
        for index in 0..thread_count {
            let worker_shared = shared.clone();
            let worker = match spawn_task_worker(kind, index, worker_shared) {
                Ok(worker) => worker,
                Err(error) => {
                    {
                        let mut queue = lock_unpoisoned(&shared.queue);
                        queue.live_workers = workers.len();
                        queue.lifecycle = QueueLifecycle::Closed;
                        shared.changed.notify_all();
                    }
                    wait_for_workers_finished(
                        &workers,
                        config.shutdown_policy().join_timeout().get(),
                    );
                    for worker in workers {
                        if worker.is_finished() {
                            let _ = worker.join();
                        }
                    }
                    return Err(TaskPoolError::WorkerSpawnFailed {
                        kind,
                        message: error.to_string(),
                    });
                }
            };
            workers.push(worker);
        }
        Ok(Self {
            shared,
            workers,
            shutdown_policy: config.shutdown_policy(),
            inline: false,
            shutdown_report: None,
        })
    }

    fn inline(kind: TaskPoolKind, config: TaskPoolConfig) -> Self {
        Self {
            shared: Arc::new(ExecutorShared::new(config.kind(kind).pending().get(), 0)),
            workers: Vec::new(),
            shutdown_policy: config.shutdown_policy(),
            inline: true,
            shutdown_report: None,
        }
    }

    fn admit(&self, pending: PendingJob) -> AdmissionDecision {
        let mut pending = Some(pending);
        let mut discarded = Vec::new();
        let decision;
        {
            let mut queue = lock_unpoisoned(&self.shared.queue);
            let mut retained = VecDeque::with_capacity(queue.pending.len());
            while let Some(queued) = queue.pending.pop_front() {
                if queued.control.state.is_terminal() {
                    discarded.push(queued);
                } else {
                    retained.push_back(queued);
                }
            }
            queue.pending = retained;

            if queue.lifecycle != QueueLifecycle::Open {
                decision = AdmissionDecision::Rejected(TaskRejectReason::PoolClosed);
            } else {
                let coalesce_position = pending.as_ref().and_then(|new_job| {
                    let domain_key = new_job.descriptor.order_key().domain_key();
                    new_job.coalesce_key.and_then(|coalesce_key| {
                        queue.pending.iter().position(|queued| {
                            queued.descriptor.order_key().domain_key() == domain_key
                                && queued.coalesce_key == Some(coalesce_key)
                        })
                    })
                });
                let mut coalesced = None;
                if let Some(position) = coalesce_position
                    && let Some(replaced) = queue.pending.remove(position)
                {
                    #[cfg(test)]
                    self.shared.run_before_coalesce_cancel_hook();
                    let replaced_id = replaced.descriptor.id();
                    let cancellation_won = pending.as_ref().is_some_and(|new_job| {
                        replaced
                            .control
                            .state
                            .cancel(TaskCancellationReason::Coalesced {
                                replacement: new_job.descriptor.id(),
                            })
                    });
                    discarded.push(replaced);
                    if cancellation_won && let Some(new_job) = pending.take() {
                        queue.pending.insert(position, new_job);
                        coalesced = Some(AdmissionDecision::Coalesced {
                            replaced: replaced_id,
                        });
                    }
                }
                if let Some(coalesced) = coalesced {
                    decision = coalesced;
                } else if queue.pending.len() < self.shared.pending_capacity {
                    if let Some(new_job) = pending.take() {
                        queue.pending.push_back(new_job);
                        decision = AdmissionDecision::Accepted;
                    } else {
                        decision = AdmissionDecision::Rejected(TaskRejectReason::PoolClosed);
                    }
                } else {
                    decision = AdmissionDecision::Rejected(TaskRejectReason::QueueFull {
                        capacity: self.shared.pending_capacity,
                    });
                }
            }
            if matches!(
                decision,
                AdmissionDecision::Accepted | AdmissionDecision::Coalesced { .. }
            ) {
                self.shared.changed.notify_one();
            }
        }

        for stale in discarded {
            safe_drop_pending(stale);
        }
        if let Some(rejected) = pending {
            safe_drop_pending(rejected);
        }
        match decision {
            AdmissionDecision::Accepted => {
                update_counters(&self.shared.counters, |counters| {
                    counters.admitted = counters.admitted.saturating_add(1);
                });
            }
            AdmissionDecision::Coalesced { .. } => {
                update_counters(&self.shared.counters, |counters| {
                    counters.admitted = counters.admitted.saturating_add(1);
                    counters.coalesced = counters.coalesced.saturating_add(1);
                });
            }
            AdmissionDecision::Rejected(_) => self.record_rejected(),
        }
        decision
    }

    fn record_rejected(&self) {
        update_counters(&self.shared.counters, |counters| {
            counters.rejected = counters.rejected.saturating_add(1);
        });
    }

    fn stats(&self) -> TaskPoolStats {
        self.shared.stats()
    }

    fn run_one_inline(&self) -> InlineStep {
        if !self.inline {
            return InlineStep::Empty;
        }
        match self.shared.take_next(false) {
            NextJob::Ready(pending) => {
                execute_pending(&self.shared, pending);
                InlineStep::Executed
            }
            NextJob::Discarded(pending) => {
                safe_drop_pending(pending);
                InlineStep::DiscardedCancelled
            }
            NextJob::Empty | NextJob::Exit => InlineStep::Empty,
        }
    }

    fn shutdown(&mut self) -> TaskPoolShutdownReport {
        if let Some(mut report) = self.shutdown_report {
            report.already_shutdown = true;
            return report;
        }
        let mut report = TaskPoolShutdownReport::default();

        if self.inline {
            let pending = {
                let mut queue = lock_unpoisoned(&self.shared.queue);
                queue.lifecycle = QueueLifecycle::Closed;
                self.shared.changed.notify_all();
                queue.pending.drain(..).collect::<Vec<_>>()
            };
            for job in pending {
                if job
                    .control
                    .state
                    .cancel(TaskCancellationReason::PoolShutdown)
                {
                    report.cancelled_pending = report.cancelled_pending.saturating_add(1);
                }
                safe_drop_pending(job);
            }
            self.record_shutdown(report);
            self.shutdown_report = Some(report);
            return report;
        }

        {
            let mut queue = lock_unpoisoned(&self.shared.queue);
            queue.lifecycle = QueueLifecycle::Draining;
            self.shared.changed.notify_all();
        }
        let drained = self.wait_until(self.shutdown_policy.drain_timeout().get(), |queue| {
            queue.pending.is_empty() && queue.running.is_empty()
        });
        if drained {
            let mut queue = lock_unpoisoned(&self.shared.queue);
            queue.lifecycle = QueueLifecycle::Closed;
            self.shared.changed.notify_all();
        } else {
            report.drain_timed_out = true;
            let (pending, running) = {
                let mut queue = lock_unpoisoned(&self.shared.queue);
                queue.lifecycle = QueueLifecycle::Closed;
                let pending = queue.pending.drain(..).collect::<Vec<_>>();
                let running = queue
                    .running
                    .values()
                    .map(|running| running.control.clone())
                    .collect::<Vec<_>>();
                self.shared.changed.notify_all();
                (pending, running)
            };
            for job in pending {
                if job
                    .control
                    .state
                    .cancel(TaskCancellationReason::PoolShutdown)
                {
                    report.cancelled_pending = report.cancelled_pending.saturating_add(1);
                }
                safe_drop_pending(job);
            }
            for running in running {
                report.cancellation_requests = report.cancellation_requests.saturating_add(1);
                running.state.cancel(TaskCancellationReason::PoolShutdown);
            }
            if !self.wait_until(self.shutdown_policy.cancel_timeout().get(), |queue| {
                queue.running.is_empty()
            }) {
                report.cancel_timed_out = true;
            }
        }

        wait_for_workers_finished(&self.workers, self.shutdown_policy.join_timeout().get());
        for worker in self.workers.drain(..) {
            if worker.is_finished() {
                if worker.join().is_ok() {
                    report.joined_workers = report.joined_workers.saturating_add(1);
                } else {
                    report.panicked_workers = report.panicked_workers.saturating_add(1);
                }
            } else {
                report.detached_workers = report.detached_workers.saturating_add(1);
            }
        }
        report.join_timed_out = report.detached_workers > 0;
        self.record_shutdown(report);
        self.shutdown_report = Some(report);
        report
    }

    fn wait_until(&self, timeout: Duration, predicate: impl Fn(&QueueState) -> bool) -> bool {
        let deadline = deadline_after(timeout);
        let mut queue = lock_unpoisoned(&self.shared.queue);
        while !predicate(&queue) {
            let now = Instant::now();
            if now >= deadline {
                return false;
            }
            let (next, timed_out) = wait_timeout_unpoisoned(
                &self.shared.changed,
                queue,
                deadline.saturating_duration_since(now),
            );
            queue = next;
            if timed_out && !predicate(&queue) {
                return false;
            }
        }
        true
    }

    fn record_shutdown(&self, report: TaskPoolShutdownReport) {
        update_counters(&self.shared.counters, |counters| {
            counters.shutdowns = counters.shutdowns.saturating_add(1);
            if report.timed_out() {
                counters.shutdown_timeouts = counters.shutdown_timeouts.saturating_add(1);
            }
            counters.detached_workers = counters
                .detached_workers
                .saturating_add(report.detached_workers as u64);
        });
    }
}

impl Drop for TaskPoolExecutor {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

fn spawn_task_worker(
    kind: TaskPoolKind,
    index: usize,
    shared: Arc<ExecutorShared>,
) -> std::io::Result<JoinHandle<()>> {
    #[cfg(test)]
    let injection = worker_spawn_failure_injection(kind);
    #[cfg(test)]
    if let Some(injection) = injection
        .as_ref()
        .filter(|injection| injection.fail_index == index)
    {
        let deadline = Instant::now() + Duration::from_secs(2);
        while injection.started.load(Ordering::Acquire) < index && Instant::now() < deadline {
            thread::yield_now();
        }
        if injection.started.load(Ordering::Acquire) < index {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "injected worker did not start before the failure deadline",
            ));
        }
        return Err(std::io::Error::other("injected worker spawn failure"));
    }

    let name = format!("nara-{}-{index}", kind.as_str());
    thread::Builder::new().name(name).spawn(move || {
        #[cfg(test)]
        let _lifetime = injection.map(WorkerLifetimeProbe::start);
        worker_main(shared);
    })
}

#[cfg(test)]
#[derive(Clone)]
struct WorkerSpawnFailureInjection {
    kind: TaskPoolKind,
    fail_index: usize,
    started: Arc<AtomicUsize>,
    exited: Arc<AtomicUsize>,
}

#[cfg(test)]
thread_local! {
    static WORKER_SPAWN_FAILURE: RefCell<Option<WorkerSpawnFailureInjection>> = const {
        RefCell::new(None)
    };
}

#[cfg(test)]
fn worker_spawn_failure_injection(kind: TaskPoolKind) -> Option<WorkerSpawnFailureInjection> {
    WORKER_SPAWN_FAILURE.with(|injection| {
        injection
            .borrow()
            .as_ref()
            .filter(|injection| injection.kind == kind)
            .cloned()
    })
}

#[cfg(test)]
struct WorkerSpawnFailureReset;

#[cfg(test)]
impl Drop for WorkerSpawnFailureReset {
    fn drop(&mut self) {
        WORKER_SPAWN_FAILURE.with(|injection| {
            injection.take();
        });
    }
}

#[cfg(test)]
#[derive(Default)]
pub(crate) struct WorkerSpawnFailureProbe {
    started: Arc<AtomicUsize>,
    exited: Arc<AtomicUsize>,
}

#[cfg(test)]
impl WorkerSpawnFailureProbe {
    pub(crate) fn started(&self) -> usize {
        self.started.load(Ordering::Acquire)
    }

    pub(crate) fn exited(&self) -> usize {
        self.exited.load(Ordering::Acquire)
    }
}

#[cfg(test)]
struct WorkerLifetimeProbe(WorkerSpawnFailureInjection);

#[cfg(test)]
impl WorkerLifetimeProbe {
    fn start(injection: WorkerSpawnFailureInjection) -> Self {
        injection.started.fetch_add(1, Ordering::AcqRel);
        Self(injection)
    }
}

#[cfg(test)]
impl Drop for WorkerLifetimeProbe {
    fn drop(&mut self) {
        self.0.exited.fetch_add(1, Ordering::AcqRel);
    }
}

fn worker_main(shared: Arc<ExecutorShared>) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        loop {
            match shared.take_next(true) {
                NextJob::Ready(pending) => execute_pending(&shared, pending),
                NextJob::Discarded(pending) => safe_drop_pending(pending),
                NextJob::Exit => break,
                NextJob::Empty => {}
            }
        }
    }));
    shared.worker_exited();
}

fn execute_pending(shared: &ExecutorShared, mut pending: PendingJob) {
    let id = pending.descriptor.id();
    let control = pending.control.clone();
    match catch_unwind(AssertUnwindSafe(|| pending.job.run())) {
        Ok(()) => {}
        Err(payload) => {
            control.state.fail(TaskFailure::Panicked);
            let _ = catch_unwind(AssertUnwindSafe(|| drop(payload)));
        }
    }
    shared.finish_running(id);
    safe_drop_pending(pending);
}

fn safe_drop_pending(pending: PendingJob) {
    let _ = catch_unwind(AssertUnwindSafe(|| drop(pending)));
}

fn update_counters(counters: &SharedTaskCounters, update: impl FnOnce(&mut TaskCounters)) {
    let mut counters = lock_unpoisoned(counters);
    update(&mut counters);
}

fn deadline_after(duration: Duration) -> Instant {
    let now = Instant::now();
    now.checked_add(duration).unwrap_or(now)
}

fn wait_for_workers_finished(workers: &[JoinHandle<()>], timeout: Duration) {
    let deadline = deadline_after(timeout);
    while workers.iter().any(|worker| !worker.is_finished()) {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        thread::sleep(
            deadline
                .saturating_duration_since(now)
                .min(Duration::from_millis(1)),
        );
    }
}

fn item_limit_or_one(value: usize) -> ItemLimit {
    ItemLimit::new(value).unwrap_or(ItemLimit::ONE)
}

fn time_limit_or_min(value: Duration) -> TimeLimit {
    TimeLimit::new(value).unwrap_or(TimeLimit::MIN)
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn wait_unpoisoned<'a, T>(condvar: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    condvar
        .wait(guard)
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn wait_timeout_unpoisoned<'a, T>(
    condvar: &Condvar,
    guard: MutexGuard<'a, T>,
    timeout: Duration,
) -> (MutexGuard<'a, T>, bool) {
    let (guard, result) = condvar
        .wait_timeout(guard, timeout)
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    (guard, result.timed_out())
}

#[derive(Resource)]
struct TaskPluginOwnedPools {
    instance_token: Arc<TaskPoolInstanceToken>,
}

#[derive(Debug, Default, Clone)]
pub struct TaskPlugin {
    config: TaskPoolConfig,
}

impl TaskPlugin {
    #[must_use]
    pub const fn new(config: TaskPoolConfig) -> Self {
        Self { config }
    }
}

impl Plugin for TaskPlugin {
    fn metadata(&self) -> nara_app::PluginMetadata {
        nara_app::PluginMetadata::new(
            nara_app::PluginId::new("nara.tasks"),
            nara_app::PluginCategory::Core,
        )
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.configure_sets(
            CoreStage::TaskUpdate,
            (
                TaskUpdateSet::Poll,
                TaskUpdateSet::CoalesceAssetChanges,
                TaskUpdateSet::SpawnAssetJobs,
                TaskUpdateSet::ApplyAssetResults,
            )
                .chain(),
        )?;

        if !app.world().contains_resource::<TaskPools>() {
            let pools =
                TaskPools::try_new(self.config).map_err(|error| PluginError::SetupFailed {
                    plugin: self.plugin_id(),
                    message: error.to_string(),
                })?;
            app.insert_resource(TaskPluginOwnedPools {
                instance_token: pools.ownership_token(),
            })?;
            app.insert_resource(pools)?;
        }
        Ok(())
    }

    fn cleanup(&self, context: &mut PluginCleanupContext<'_>) -> Result<(), PluginError> {
        let Some(ownership) = context
            .world_mut()
            .remove_resource::<TaskPluginOwnedPools>()
        else {
            return Ok(());
        };
        let owns_current_pools = context
            .world()
            .get_resource::<TaskPools>()
            .is_some_and(|pools| pools.has_ownership_token(&ownership.instance_token));
        if owns_current_pools
            && let Some(mut pools) = context.world_mut().remove_resource::<TaskPools>()
        {
            let report = pools.shutdown();
            let timed_out = report.timed_out();
            let detached_workers = report.detached_workers();
            context.world_mut().insert_resource(report);
            if timed_out || detached_workers > 0 {
                return Err(PluginError::SetupFailed {
                    plugin: self.plugin_id(),
                    message: format!(
                        "task pool shutdown timed out; detached {detached_workers} workers"
                    ),
                });
            }
        }
        Ok(())
    }
}
