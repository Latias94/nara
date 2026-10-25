use std::{
    collections::{BTreeMap, VecDeque},
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    io,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU8, AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, Sender, SyncSender, TryRecvError, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

use nara_app::{
    App, Plugin, PluginError, RuntimeCloseContext, RuntimeCloseParticipant,
    RuntimeCloseParticipantError, RuntimeCloseParticipantId, RuntimeCloseProgress,
};
use nara_core::{ItemLimit, TimeLimit};
use nara_ecs::Resource;

mod scope;
mod workers;
pub use scope::{TaskFacilities, TaskScope, TaskScopeCloseReport};
use scope::{TaskScopeShared, prepare_admission, same_scope};
use workers::{TaskWorkers, WorkerActivationError};

#[cfg(test)]
use std::cell::RefCell;

pub const MAX_TASK_POOL_THREADS_PER_KIND: usize = 256;
pub const MAX_TASK_POOL_THREADS_TOTAL: usize = 512;
pub const MAX_TASK_POOL_PENDING_PER_KIND: usize = 1_048_576;
pub const MAX_TASK_POOL_PENDING_TOTAL: usize = 2_097_152;
pub const MAX_TASK_SHUTDOWN_PHASE_TIMEOUT: Duration = Duration::from_secs(30);
pub const MAX_TASK_SHUTDOWN_TOTAL_TIMEOUT: Duration = Duration::from_secs(60);
const TASK_CLOSE_CANCEL_BATCH: usize = 256;
const TASK_REAPER_INTAKE_BATCH: usize = 64;
const TASK_REAPER_OWNER_POLL_BATCH: usize = 64;
const TASK_REAPER_DROP_LANES: usize = 2;
const TASK_REAPER_ROUND_WAIT: Duration = Duration::from_millis(1);

// Drop cannot run user destructors or wait for uncooperative workers. A process-owned reaper keeps
// every transferred task owner observable until pending work is retired and worker handles join.
static TASK_OWNER_REAPER_SUPERVISOR: Mutex<TaskOwnerReaperSupervisor> =
    Mutex::new(TaskOwnerReaperSupervisor::new());
static ABANDONED_TASK_OWNER_COUNT: AtomicU64 = AtomicU64::new(0);

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
    /// A worker queue for background compute closures, not a Future executor.
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
    QueueFull {
        capacity: usize,
    },
    PoolClosed,
    ScopeClosed,
    /// This worker kind failed first-use activation; its physical owner retains partial workers.
    WorkerStartupFailed,
    TaskIdExhausted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskRejection {
    pub task: Option<TaskDescriptor>,
    pub kind: TaskPoolKind,
    pub admission_tick: u64,
    pub domain_key: TaskDomainKey,
    pub reason: TaskRejectReason,
    /// Capture disposal has completed before rejection returns; failure remains explicit.
    pub return_state: TaskReturnState,
}

/// Physical return of executor-owned work, independent of its first terminal result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskReturnState {
    /// Execution or executor-owned disposal is still in progress.
    Pending,
    /// Executor-owned disposal completed; transferred outputs retain their own owners.
    Returned,
    /// Executor-owned work returned, but a caught unwind prevents proving successful cleanup.
    CleanupFailed,
}

const RETURN_PENDING: u8 = 0;
const RETURN_COMPLETE: u8 = 1;
const RETURN_FAILED: u8 = 2;
const RETURN_DISPOSING_FAILED: u8 = 3;

/// Observes closure/capture and rejected-result disposal without retaining task output storage.
///
/// Published or transferred outputs, escaped clones and nested work retain their own owners.
/// This observation does not join a worker or prove native domain cleanup.
#[derive(Debug, Clone)]
pub struct TaskReturnObserver {
    observation: Arc<TaskReturnObservation>,
}

#[derive(Debug)]
struct TaskReturnObservation {
    state: AtomicU8,
    cleanup_failed: AtomicBool,
    admitted: AtomicBool,
    scope: Option<Arc<TaskScopeShared>>,
}

impl TaskReturnObserver {
    fn for_scope(scope: Option<Arc<TaskScopeShared>>) -> Self {
        Self {
            observation: Arc::new(TaskReturnObservation {
                state: AtomicU8::new(RETURN_PENDING),
                cleanup_failed: AtomicBool::new(false),
                admitted: AtomicBool::new(false),
                scope,
            }),
        }
    }

    #[must_use]
    pub fn state(&self) -> TaskReturnState {
        match self.observation.state.load(Ordering::Acquire) {
            RETURN_PENDING | RETURN_DISPOSING_FAILED => TaskReturnState::Pending,
            RETURN_COMPLETE => TaskReturnState::Returned,
            _ => TaskReturnState::CleanupFailed,
        }
    }

    fn cleanup_failed(&self) {
        self.observation
            .cleanup_failed
            .store(true, Ordering::Release);
        let _ = self.observation.state.compare_exchange(
            RETURN_PENDING,
            RETURN_DISPOSING_FAILED,
            Ordering::Release,
            Ordering::Relaxed,
        );
    }

    fn cleanup_outcome(&self) -> TaskCleanupOutcome {
        if self.observation.cleanup_failed.load(Ordering::Acquire) {
            TaskCleanupOutcome::Failed
        } else {
            TaskCleanupOutcome::Complete
        }
    }

    fn finish(&self) {
        let returned = self
            .observation
            .state
            .compare_exchange(
                RETURN_PENDING,
                RETURN_COMPLETE,
                Ordering::Release,
                Ordering::Relaxed,
            )
            .is_ok()
            || self
                .observation
                .state
                .compare_exchange(
                    RETURN_DISPOSING_FAILED,
                    RETURN_FAILED,
                    Ordering::Release,
                    Ordering::Relaxed,
                )
                .is_ok();
        if returned
            && self.observation.admitted.load(Ordering::Acquire)
            && let Some(scope) = &self.observation.scope
        {
            scope.finish_task(self.cleanup_outcome());
        }
    }

    fn record_admission(&self) {
        if let Some(scope) = &self.observation.scope {
            scope.admit_task();
            self.observation.admitted.store(true, Ordering::Release);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskCancellationReason {
    Requested,
    Coalesced { replacement: TaskId },
    PoolShutdown,
    ScopeClosed,
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

/// A cooperative cancellation signal shared with an owned task closure.
///
/// Cancellation does not interrupt a running closure. Work that can stop early must check this
/// signal at suitable boundaries and release its owned inputs before returning.
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
    completion_sequence: Option<u64>,
    admission_to_start: Option<Duration>,
}

struct TaskState<T> {
    inner: Mutex<TaskStateInner<T>>,
    finished: AtomicBool,
    token: TaskCancellationToken,
    counters: SharedTaskCounters,
    instance_token: Arc<TaskPoolInstanceToken>,
    returned: TaskReturnObserver,
}

impl<T> TaskState<T> {
    fn new(
        counters: SharedTaskCounters,
        instance_token: Arc<TaskPoolInstanceToken>,
        scope: Option<Arc<TaskScopeShared>>,
    ) -> Self {
        Self {
            inner: Mutex::new(TaskStateInner {
                phase: TaskPhase::Pending,
                terminal: None,
                completion_sequence: None,
                admission_to_start: None,
            }),
            finished: AtomicBool::new(false),
            token: TaskCancellationToken::new(),
            counters,
            instance_token,
            returned: TaskReturnObserver::for_scope(scope),
        }
    }

    fn mark_running(&self, admitted_at: Instant) -> bool {
        let mut inner = lock_unpoisoned(&self.inner);
        if inner.phase != TaskPhase::Pending {
            return false;
        }
        inner.phase = TaskPhase::Running;
        inner.admission_to_start = Some(admitted_at.elapsed());
        drop(inner);
        update_counters(&self.counters, |counters| {
            counters.started = counters.started.saturating_add(1);
        });
        true
    }

    fn complete(&self, value: T) -> bool {
        let mut publication = lock_unpoisoned(&self.instance_token.completion_publication);
        let mut inner = lock_unpoisoned(&self.inner);
        if inner.phase == TaskPhase::Terminal {
            drop(inner);
            drop(publication);
            if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(value))) {
                self.returned.cleanup_failed();
                let _ = catch_unwind(AssertUnwindSafe(|| drop(payload)));
            }
            return false;
        }
        let completion_sequence = publication.next_sequence();
        inner.phase = TaskPhase::Terminal;
        inner.terminal = Some(TaskTerminal::Completed(value));
        inner.completion_sequence = Some(completion_sequence);
        self.finished.store(true, Ordering::Release);
        drop(inner);
        drop(publication);
        update_counters(&self.counters, |counters| {
            counters.completed = counters.completed.saturating_add(1);
        });
        true
    }

    fn fail(&self, failure: TaskFailure) -> bool {
        let mut publication = lock_unpoisoned(&self.instance_token.completion_publication);
        let mut inner = lock_unpoisoned(&self.inner);
        if inner.phase == TaskPhase::Terminal {
            return false;
        }
        let completion_sequence = publication.next_sequence();
        inner.phase = TaskPhase::Terminal;
        inner.terminal = Some(TaskTerminal::Failed(failure));
        inner.completion_sequence = Some(completion_sequence);
        self.finished.store(true, Ordering::Release);
        drop(inner);
        drop(publication);
        update_counters(&self.counters, |counters| {
            counters.failed = counters.failed.saturating_add(1);
        });
        true
    }

    fn cancel(&self, reason: TaskCancellationReason) -> bool {
        let mut publication = lock_unpoisoned(&self.instance_token.completion_publication);
        let mut inner = lock_unpoisoned(&self.inner);
        if inner.phase == TaskPhase::Terminal {
            return false;
        }
        let before_start = inner.phase == TaskPhase::Pending;
        let completion_sequence = publication.next_sequence();
        inner.phase = TaskPhase::Terminal;
        inner.terminal = Some(TaskTerminal::Cancelled(TaskCancellation {
            reason,
            before_start,
        }));
        inner.completion_sequence = Some(completion_sequence);
        self.token.mark_cancelled();
        self.finished.store(true, Ordering::Release);
        drop(inner);
        drop(publication);
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

    fn belongs_to(&self, cutoff: &TaskCompletionCutoff) -> bool {
        Arc::ptr_eq(&self.instance_token, &cutoff.instance_token)
    }

    fn completed_by(&self, cutoff: &TaskCompletionCutoff) -> bool {
        lock_unpoisoned(&self.inner)
            .completion_sequence
            .is_some_and(|sequence| sequence <= cutoff.sequence)
    }
}

impl<T> Drop for TaskState<T> {
    fn drop(&mut self) {
        let inner = self
            .inner
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(terminal) = inner.terminal.take() {
            if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(terminal))) {
                self.returned.cleanup_failed();
                let _ = catch_unwind(AssertUnwindSafe(|| drop(payload)));
            }
        }
    }
}

trait ErasedTaskState: Send + Sync {
    fn return_observer(&self) -> TaskReturnObserver;
    fn mark_running(&self, admitted_at: Instant) -> bool;
    fn cancel(&self, reason: TaskCancellationReason) -> bool;
    fn fail(&self, failure: TaskFailure) -> bool;
    fn is_terminal(&self) -> bool;
}

impl<T: Send + 'static> ErasedTaskState for TaskState<T> {
    fn return_observer(&self) -> TaskReturnObserver {
        self.returned.clone()
    }

    fn mark_running(&self, admitted_at: Instant) -> bool {
        TaskState::mark_running(self, admitted_at)
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

/// Observes one task's first terminal result independently of its physical retirement.
///
/// Dropping a handle does not cancel the task. Queued or running work keeps its owned inputs until
/// the executor retires it, even when no caller remains interested in its result. Use
/// [`Self::cancel`] to request cancellation explicitly.
///
/// A cancelled handle can be finished while its closure or captured values are still being
/// retired. Pool shutdown and the managed runtime close protocol track that physical work.
/// Dropping the last owner of an untaken completed result runs that result's destructor on the
/// dropping thread; the destructor may block.
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

    #[must_use]
    pub fn return_observer(&self) -> TaskReturnObserver {
        self.state.returned.clone()
    }

    /// Time from accepted queue publication to execution authorization, when started.
    ///
    /// Rejected or cancelled-before-start work has no sample. This excludes submission setup,
    /// queue-lock acquisition and any replaced capture disposal on the submitting thread.
    #[must_use]
    pub fn admission_to_start(&self) -> Option<Duration> {
        lock_unpoisoned(&self.state.inner).admission_to_start
    }

    /// Publishes cancellation if no terminal result has already won.
    ///
    /// Returns `true` only when this call wins the first-terminal transition. `false` means the
    /// task was already completed, failed, or cancelled. Pending work that loses execution
    /// authority will not start; running work receives a cooperative signal and may continue
    /// until it checks the token or returns. Neither result proves physical retirement.
    pub fn cancel(&self) -> bool {
        self.state.cancel(TaskCancellationReason::Requested)
    }

    /// Returns whether a completed, failed, or cancelled terminal has been published.
    ///
    /// This remains `true` after [`Self::try_take`] consumes the result. It does not mean the
    /// closure, its captured values, or its worker have physically retired.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.state.is_terminal()
    }

    /// Takes the terminal result once, returning `None` before publication or after consumption.
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

#[derive(Debug, Clone)]
pub struct TaskCompletionCutoff {
    instance_token: Arc<TaskPoolInstanceToken>,
    sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskCompletionCutoffError {
    PoolMismatch { task: TaskDescriptor },
}

impl Display for TaskCompletionCutoffError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::PoolMismatch { .. } => formatter.write_str(
                "ordered task results contain a task from a different task-pool instance",
            ),
        }
    }
}

impl Error for TaskCompletionCutoffError {}

#[derive(Debug)]
pub struct OrderedTaskReadySnapshot<T> {
    handles: Vec<(TaskOrderKey, TaskHandle<T>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskReadySnapshotError {
    task: TaskDescriptor,
}

impl TaskReadySnapshotError {
    #[must_use]
    pub const fn task(self) -> TaskDescriptor {
        self.task
    }
}

impl Display for TaskReadySnapshotError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter.write_str("a captured task terminal was unavailable")
    }
}

impl Error for TaskReadySnapshotError {}

impl<T> OrderedTaskReadySnapshot<T> {
    pub fn into_terminals(self) -> Result<Vec<OrderedTaskTerminal<T>>, TaskReadySnapshotError> {
        let mut terminals = Vec::with_capacity(self.handles.len());
        for (order_key, mut handle) in self.handles {
            let task = handle.descriptor();
            let Some(terminal) = handle.try_take() else {
                return Err(TaskReadySnapshotError { task });
            };
            terminals.push(OrderedTaskTerminal {
                order_key,
                terminal,
            });
        }
        Ok(terminals)
    }
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

    pub fn validate_completion_cutoff(
        &self,
        cutoff: &TaskCompletionCutoff,
    ) -> Result<(), TaskCompletionCutoffError> {
        if let Some(handle) = self
            .pending
            .values()
            .find(|handle| !handle.state.belongs_to(cutoff))
        {
            return Err(TaskCompletionCutoffError::PoolMismatch {
                task: handle.descriptor(),
            });
        }
        Ok(())
    }

    pub fn capture_ready_prefix(
        &mut self,
        cutoff: &TaskCompletionCutoff,
    ) -> Result<OrderedTaskReadySnapshot<T>, TaskCompletionCutoffError> {
        self.validate_completion_cutoff(cutoff)?;
        let ready_count = self
            .pending
            .values()
            .take_while(|handle| handle.state.completed_by(cutoff))
            .count();
        let mut handles = Vec::with_capacity(ready_count);
        for _ in 0..ready_count {
            let Some((order_key, handle)) = self.pending.pop_first() else {
                break;
            };
            handles.push((order_key, handle));
        }
        Ok(OrderedTaskReadySnapshot { handles })
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TaskPoolStats {
    pub admitted: u64,
    pub rejected: u64,
    pub coalesced: u64,
    pub queued: usize,
    pub running: usize,
    /// Started workers plus in-flight OS spawn reservations; zero is not worker-join evidence.
    pub live_workers: usize,
    pub started: u64,
    pub completed: u64,
    pub failed: u64,
    pub cancelled: u64,
    pub taken: u64,
    pub oldest_queued_age: Option<Duration>,
    pub oldest_running_age: Option<Duration>,
    pub shutdowns: u64,
    pub shutdown_timeouts: u64,
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

#[derive(Debug, Default)]
struct TaskCompletionPublication {
    sequence: u64,
}

impl TaskCompletionPublication {
    fn next_sequence(&mut self) -> u64 {
        self.sequence += 1;
        self.sequence
    }
}

#[derive(Debug, Default)]
struct TaskPoolInstanceToken {
    completion_publication: Mutex<TaskCompletionPublication>,
}

/// Bounded worker queues that execute owned, potentially blocking closures.
///
/// Tasks return owned data for their domain to integrate on its owner thread. This API does not
/// accept or poll Futures; [`TaskPoolKind::AsyncCompute`] selects another closure worker queue.
///
/// Standalone pools retain their close owner. [`Self::shutdown_blocking`] drives finite,
/// retryable physical retirement. Dropping an unfinished standalone owner closes admission and
/// publishes cancellation for pending and running handles before transferring unfinished work
/// to process-owned retention. This cancellation visits the bounded pending queue and is O(n);
/// pending captures are disposed on internal drop lanes without waiting for uncooperative work.
/// Physical capture disposal and worker joins can remain incomplete after Drop returns. Releasing
/// the last owner of an untaken completed result can still run its destructor on the dropping
/// thread, so Drop does not promise bounded latency for arbitrary user destructors.
///
/// Plugin-created pools transfer close ownership to the managed runtime. Removing or replacing
/// their World-facing facade does not retire that owner's workers or close obligations.
/// [`TaskScope::spawner`] supplies the same submission interface with activity-local admission
/// and no physical worker-close authority. Its statistics describe the shared facilities, while
/// completion cutoffs and pending-work coalescing remain local to that activity.
#[derive(Resource)]
pub struct TaskPools {
    config: TaskPoolConfig,
    next_id: Arc<AtomicU64>,
    instance_token: Arc<TaskPoolInstanceToken>,
    io: TaskPoolExecutor,
    compute: TaskPoolExecutor,
    async_compute: TaskPoolExecutor,
    close_owner: Mutex<Option<TaskPoolsCloseOwner>>,
    scope: Option<Arc<TaskScopeShared>>,
}

pub(crate) struct TaskPoolsConstructionFailure {
    error: TaskPoolError,
    close_owner: Option<TaskPoolsCloseOwner>,
}

impl TaskPoolsConstructionFailure {
    fn invalid(error: TaskPoolError) -> Self {
        Self {
            error,
            close_owner: None,
        }
    }

    fn with_executor_failure(
        failure: TaskPoolExecutorConstructionFailure,
        mut close_owner: TaskPoolsCloseOwner,
    ) -> Self {
        let TaskPoolExecutorConstructionFailure {
            error,
            close_owner: executor_owner,
        } = failure;
        let kind = match &error {
            TaskPoolError::WorkerSpawnFailed { kind, .. } => *kind,
            TaskPoolError::InvalidConfig(_) => {
                unreachable!("executor construction receives validated configuration")
            }
        };
        close_owner.insert(kind, executor_owner);
        Self {
            error,
            close_owner: Some(close_owner),
        }
    }

    fn into_error(self) -> TaskPoolError {
        self.error
    }

    fn into_parts(self) -> (TaskPoolError, Option<TaskPoolsCloseOwner>) {
        (self.error, self.close_owner)
    }

    #[cfg(test)]
    pub(crate) fn error(&self) -> &TaskPoolError {
        &self.error
    }

    #[cfg(test)]
    pub(crate) fn drive_cleanup(&mut self) -> TaskPoolsCloseProgress {
        self.close_owner
            .as_mut()
            .map_or(TaskPoolsCloseProgress::Complete, |owner| owner.poll_close())
    }

    #[cfg(test)]
    pub(crate) fn report(&self) -> TaskShutdownReport {
        self.close_owner
            .as_ref()
            .map_or_else(TaskShutdownReport::default, TaskPoolsCloseOwner::report)
    }
}

impl TaskPools {
    pub fn try_new(config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        Self::try_new_retained(config).map_err(TaskPoolsConstructionFailure::into_error)
    }

    /// Creates bounded threaded queues whose workers start on the first submission to each kind.
    ///
    /// First use may block for OS thread creation. A partial startup is retained by the physical
    /// owner and rejects submission as [`TaskRejectReason::WorkerStartupFailed`]. It is not retried
    /// implicitly. Idle kinds create no workers; ordinary submission never executes work inline.
    pub fn try_new_lazy(config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        config.validate().map_err(TaskPoolError::InvalidConfig)?;
        prewarm_task_owner_reaper();
        let (io, io_owner) = TaskPoolExecutor::lazy(TaskPoolKind::Io, config);
        let (compute, compute_owner) = TaskPoolExecutor::lazy(TaskPoolKind::Compute, config);
        let (async_compute, async_owner) =
            TaskPoolExecutor::lazy(TaskPoolKind::AsyncCompute, config);
        Ok(Self {
            config,
            next_id: Arc::new(AtomicU64::new(1)),
            instance_token: Arc::new(TaskPoolInstanceToken::default()),
            io,
            compute,
            async_compute,
            close_owner: Mutex::new(Some(TaskPoolsCloseOwner::new(
                io_owner,
                compute_owner,
                async_owner,
            ))),
            scope: None,
        })
    }

    fn try_new_retained(config: TaskPoolConfig) -> Result<Self, TaskPoolsConstructionFailure> {
        config
            .validate()
            .map_err(TaskPoolError::InvalidConfig)
            .map_err(TaskPoolsConstructionFailure::invalid)?;
        prewarm_task_owner_reaper();
        let mut close_owner = TaskPoolsCloseOwner::empty();
        let (io, io_close_owner) = match TaskPoolExecutor::threaded(TaskPoolKind::Io, config) {
            Ok(executor) => executor,
            Err(error) => {
                return Err(TaskPoolsConstructionFailure::with_executor_failure(
                    error,
                    close_owner,
                ));
            }
        };
        close_owner.insert(TaskPoolKind::Io, io_close_owner);
        let (compute, compute_close_owner) =
            match TaskPoolExecutor::threaded(TaskPoolKind::Compute, config) {
                Ok(executor) => executor,
                Err(error) => {
                    return Err(TaskPoolsConstructionFailure::with_executor_failure(
                        error,
                        close_owner,
                    ));
                }
            };
        close_owner.insert(TaskPoolKind::Compute, compute_close_owner);
        let (async_compute, async_compute_close_owner) =
            match TaskPoolExecutor::threaded(TaskPoolKind::AsyncCompute, config) {
                Ok(executor) => executor,
                Err(error) => {
                    return Err(TaskPoolsConstructionFailure::with_executor_failure(
                        error,
                        close_owner,
                    ));
                }
            };
        close_owner.insert(TaskPoolKind::AsyncCompute, async_compute_close_owner);
        Ok(Self {
            config,
            next_id: Arc::new(AtomicU64::new(1)),
            instance_token: Arc::new(TaskPoolInstanceToken::default()),
            io,
            compute,
            async_compute,
            close_owner: Mutex::new(Some(close_owner)),
            scope: None,
        })
    }

    pub fn inline_for_tests(config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        config.validate().map_err(TaskPoolError::InvalidConfig)?;
        prewarm_task_owner_reaper();
        let (io, io_close_owner) = TaskPoolExecutor::inline(TaskPoolKind::Io, config);
        let (compute, compute_close_owner) =
            TaskPoolExecutor::inline(TaskPoolKind::Compute, config);
        let (async_compute, async_compute_close_owner) =
            TaskPoolExecutor::inline(TaskPoolKind::AsyncCompute, config);
        Ok(Self {
            config,
            next_id: Arc::new(AtomicU64::new(1)),
            instance_token: Arc::new(TaskPoolInstanceToken::default()),
            io,
            compute,
            async_compute,
            close_owner: Mutex::new(Some(TaskPoolsCloseOwner::new(
                io_close_owner,
                compute_close_owner,
                async_compute_close_owner,
            ))),
            scope: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn try_new_with_worker_spawn_failure_for_tests(
        config: TaskPoolConfig,
        kind: TaskPoolKind,
        fail_index: usize,
    ) -> (
        Result<Self, TaskPoolsConstructionFailure>,
        WorkerSpawnFailureProbe,
    ) {
        Self::with_worker_spawn_failure_for_tests(kind, fail_index, || {
            Self::try_new_retained(config)
        })
    }

    #[cfg(test)]
    pub(crate) fn with_worker_spawn_failure_for_tests<R>(
        kind: TaskPoolKind,
        fail_index: usize,
        operation: impl FnOnce() -> R,
    ) -> (R, WorkerSpawnFailureProbe) {
        let probe = WorkerSpawnFailureProbe::default();
        let previous = WORKER_SPAWN_FAILURE.with(|injection| {
            injection.replace(Some(WorkerSpawnFailureInjection {
                kind,
                fail_index,
                started: probe.started.clone(),
                exited: probe.exited.clone(),
                cleanup_polls: probe.cleanup_polls.clone(),
            }))
        });
        assert!(
            previous.is_none(),
            "worker spawn failure injection is nested"
        );
        let _reset = WorkerSpawnFailureReset;
        (operation(), probe)
    }

    #[must_use]
    pub const fn config(&self) -> &TaskPoolConfig {
        &self.config
    }

    /// Whether this pool executes admitted work on worker threads.
    ///
    /// This distinguishes production workers from [`Self::inline_for_tests`]. It does not
    /// promise that the pool is open or that a subsequent submission will be admitted.
    #[must_use]
    pub fn is_threaded(&self, kind: TaskPoolKind) -> bool {
        !self.executor(kind).inline
    }

    #[must_use]
    pub fn stats(&self) -> TaskStats {
        let mut stats = TaskStats::default();
        for kind in TaskPoolKind::ALL {
            stats.insert(kind, self.executor(kind).stats());
        }
        stats
    }

    #[must_use]
    pub fn capture_completion_cutoff(&self) -> TaskCompletionCutoff {
        let publication = lock_unpoisoned(&self.instance_token.completion_publication);
        TaskCompletionCutoff {
            instance_token: self.instance_token.clone(),
            sequence: publication.sequence,
        }
    }

    /// Submits an owned closure to a bounded worker queue and returns its admission outcome.
    ///
    /// Production workers execute admitted closures; [`Self::inline_for_tests`] provides an
    /// explicit test driver. There is no fallback execution on the submitting thread.
    /// Rejection, coalescing, and stale-entry cleanup can drop captured values on the submitting
    /// thread, so a blocking capture destructor can delay this call.
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
            let return_state = match catch_unwind(AssertUnwindSafe(|| drop(function))) {
                Ok(()) => TaskReturnState::Returned,
                Err(payload) => {
                    let _ = catch_unwind(AssertUnwindSafe(|| drop(payload)));
                    TaskReturnState::CleanupFailed
                }
            };
            return TaskSpawnOutcome::Rejected(TaskRejection {
                task: None,
                kind,
                admission_tick: request.admission_tick(),
                domain_key: request.domain_key(),
                reason: TaskRejectReason::TaskIdExhausted,
                return_state,
            });
        };
        let order_key = TaskOrderKey::new(request.admission_tick(), request.domain_key(), id);
        let descriptor = TaskDescriptor {
            id,
            kind,
            order_key,
        };
        let counters = self.executor(kind).shared.counters.clone();
        let state = Arc::new(TaskState::new(
            counters,
            self.instance_token.clone(),
            self.scope.clone(),
        ));
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
            disposal_owner: None,
            scope: self.scope.clone(),
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
                return_state: handle.return_observer().state(),
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

    /// Drives a standalone owner's finite drain, cancellation, and worker-join protocol.
    ///
    /// An incomplete report retains the same physical work for a later call. A complete report
    /// proves physical return, not successful cleanup; inspect its failure evidence separately.
    /// Terminal task handles alone prove neither. Plugin-managed pools return
    /// [`TaskShutdownError::CloseOwnerTransferred`] because their runtime owns this protocol.
    pub fn shutdown_blocking(&self) -> Result<TaskShutdownReport, TaskShutdownError> {
        let mut close_owner = self
            .close_owner
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(owner) = close_owner.as_mut() else {
            return Err(TaskShutdownError::CloseOwnerTransferred);
        };
        if owner.is_complete() {
            return Ok(owner.report());
        }
        owner.begin_close();
        loop {
            match owner.poll_close() {
                TaskPoolsCloseProgress::Complete | TaskPoolsCloseProgress::Incomplete => {
                    return Ok(owner.report());
                }
                TaskPoolsCloseProgress::Pending => thread::sleep(Duration::from_millis(1)),
            }
        }
    }

    fn allocate_task_id(&self) -> Option<TaskId> {
        self.next_id
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
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
    pub(crate) fn set_before_worker_activation_hook_for_tests(
        &self,
        hook: impl FnOnce() + Send + 'static,
    ) {
        self.io.workers.before_activation(hook);
    }

    #[cfg(test)]
    pub(crate) fn after_lazy_first_worker_for_tests(&self, hook: impl FnOnce() + Send + 'static) {
        self.io.workers.after_first_worker(hook);
    }

    #[cfg(test)]
    pub(crate) fn after_worker_reservation_for_tests(&self, hook: impl FnOnce() + Send + 'static) {
        self.io.workers.after_reservation(hook);
    }

    #[cfg(test)]
    pub(crate) fn worker_join_probe_for_tests(&self) -> Arc<AtomicUsize> {
        self.io.workers.join_probe()
    }

    #[cfg(test)]
    pub(crate) fn admission_queue_visits_for_tests(&self, kind: TaskPoolKind) -> usize {
        self.executor(kind)
            .shared
            .admission_queue_visits
            .load(Ordering::Relaxed)
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
    pub(crate) fn set_after_running_snapshot_worker_exit_hook_for_tests(
        &self,
        kind: TaskPoolKind,
        hook: impl FnOnce() + Send + 'static,
    ) {
        let shared = Arc::clone(&self.executor(kind).shared);
        let worker_shared = Arc::downgrade(&shared);
        *lock_unpoisoned(&shared.after_running_snapshot_hook) = Some(Box::new(move || {
            hook();
            // Keep the close snapshot alive until the real worker has recorded disposal and exited.
            let worker_shared = worker_shared
                .upgrade()
                .expect("close retains its worker owner");
            let queue = lock_unpoisoned(&worker_shared.queue);
            let (queue, _) = worker_shared
                .changed
                .wait_timeout_while(queue, Duration::from_secs(10), |queue| {
                    queue.live_workers != 0
                })
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            assert_eq!(queue.live_workers, 0, "the snapshotted worker did not exit");
            assert!(queue.running.is_empty());
        }));
    }

    #[cfg(test)]
    pub(crate) fn shutdown_executor_for_tests(
        &mut self,
        kind: TaskPoolKind,
    ) -> TaskPoolShutdownReport {
        self.close_owner
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .as_mut()
            .expect("standalone test pools retain their close owner")
            .executor_mut(kind)
            .expect("complete task pools retain every executor close owner")
            .shutdown()
    }

    fn executor(&self, kind: TaskPoolKind) -> &TaskPoolExecutor {
        match kind {
            TaskPoolKind::Io => &self.io,
            TaskPoolKind::Compute => &self.compute,
            TaskPoolKind::AsyncCompute => &self.async_compute,
        }
    }

    fn take_close_owner(&mut self) -> Option<TaskPoolsCloseOwner> {
        self.close_owner
            .get_mut()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .take()
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TaskInlineRunReport {
    pub executed: usize,
    pub cancelled_before_start: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskPoolsCloseProgress {
    Pending,
    Incomplete,
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskShutdownError {
    CloseOwnerTransferred,
}

impl Display for TaskShutdownError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::CloseOwnerTransferred => {
                formatter.write_str("this task facade does not own physical facility shutdown")
            }
        }
    }
}

impl Error for TaskShutdownError {}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TaskPoolShutdownReport {
    pub cancelled_pending: usize,
    pub cancellation_requests: usize,
    pub joined_workers: usize,
    pub panicked_workers: usize,
    /// A caught unwind prevents proving admitted, executor-owned cleanup succeeded.
    /// A returned task business error alone does not set this flag.
    pub cleanup_failed: bool,
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
    complete: bool,
}

impl Default for TaskShutdownReport {
    fn default() -> Self {
        let mut per_kind = BTreeMap::new();
        for kind in TaskPoolKind::ALL {
            per_kind.insert(kind, TaskPoolShutdownReport::default());
        }
        Self {
            per_kind,
            complete: false,
        }
    }
}

impl TaskShutdownReport {
    /// Combines observations of disjoint physical owners, preserving completion and failures.
    ///
    /// Count each owner once; successive polls of the same owner are not disjoint reports.
    /// Empty input describes no physical obligations and is complete. This is observation
    /// aggregation, not ownership transfer or proof that the caller supplied disjoint owners.
    #[must_use]
    pub fn aggregate<'a>(reports: impl IntoIterator<Item = &'a Self>) -> Self {
        let mut result = Self {
            complete: true,
            ..Self::default()
        };
        for report in reports {
            result.complete &= report.complete;
            for kind in TaskPoolKind::ALL {
                let next = report.for_kind(kind);
                let current = result
                    .per_kind
                    .get_mut(&kind)
                    .expect("report contains every worker kind");
                current.cancelled_pending = current
                    .cancelled_pending
                    .saturating_add(next.cancelled_pending);
                current.cancellation_requests = current
                    .cancellation_requests
                    .saturating_add(next.cancellation_requests);
                current.joined_workers = current.joined_workers.saturating_add(next.joined_workers);
                current.panicked_workers = current
                    .panicked_workers
                    .saturating_add(next.panicked_workers);
                current.cleanup_failed |= next.cleanup_failed;
                current.drain_timed_out |= next.drain_timed_out;
                current.cancel_timed_out |= next.cancel_timed_out;
                current.join_timed_out |= next.join_timed_out;
            }
        }
        result
    }

    /// Returns whether all workers joined and all admitted executor-owned disposal returned.
    /// Cleanup failure is independent of physical completion.
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.complete
    }

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
    pub fn has_cleanup_failure(&self) -> bool {
        TaskPoolKind::ALL
            .into_iter()
            .any(|kind| self.for_kind(kind).cleanup_failed)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TaskCleanupOutcome {
    Complete,
    Failed,
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
    disposal_owner: Option<Arc<ExecutorShared>>,
    scope: Option<Arc<TaskScopeShared>>,
    control: TaskControl,
    job: Box<dyn ErasedJob>,
}

struct RunningJob {
    control: Option<TaskControl>,
    started_at: Instant,
    scope: Option<Arc<TaskScopeShared>>,
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
    disposing: usize,
    cleanup_failed: bool,
}

impl QueueState {
    fn new(live_workers: usize) -> Self {
        Self {
            lifecycle: QueueLifecycle::Open,
            pending: VecDeque::new(),
            running: BTreeMap::new(),
            live_workers,
            disposing: 0,
            cleanup_failed: false,
        }
    }

    fn take_for_disposal(
        &mut self,
        position: usize,
        shared: &Arc<ExecutorShared>,
    ) -> Option<PendingJob> {
        let pending = self.pending.get_mut(position)?;
        // Publish custody under the queue lock before making removal visible to close.
        debug_assert!(pending.disposal_owner.is_none());
        pending.disposal_owner = Some(Arc::clone(shared));
        self.disposing += 1;
        self.pending.remove(position)
    }
}

struct ExecutorShared {
    pending_capacity: usize,
    queue: Mutex<QueueState>,
    changed: Condvar,
    counters: SharedTaskCounters,
    #[cfg(test)]
    admission_queue_visits: AtomicUsize,
    #[cfg(test)]
    before_coalesce_cancel_hook: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
    #[cfg(test)]
    after_running_snapshot_hook: Mutex<Option<Box<dyn FnOnce() + Send>>>,
}

impl ExecutorShared {
    fn new(pending_capacity: usize, live_workers: usize) -> Self {
        Self {
            pending_capacity,
            queue: Mutex::new(QueueState::new(live_workers)),
            changed: Condvar::new(),
            counters: Arc::new(Mutex::new(TaskCounters::default())),
            #[cfg(test)]
            admission_queue_visits: AtomicUsize::new(0),
            #[cfg(test)]
            before_coalesce_cancel_hook: Mutex::new(None),
            #[cfg(test)]
            after_running_snapshot_hook: Mutex::new(None),
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
            live_workers: queue.live_workers,
            started: counters.started,
            completed: counters.completed,
            failed: counters.failed,
            cancelled: counters.cancelled,
            taken: counters.taken,
            oldest_queued_age,
            oldest_running_age,
            shutdowns: counters.shutdowns,
            shutdown_timeouts: counters.shutdown_timeouts,
        }
    }

    fn take_next(self: &Arc<Self>, wait: bool) -> NextJob {
        let mut queue = lock_unpoisoned(&self.queue);
        loop {
            if queue.lifecycle == QueueLifecycle::Closed {
                return NextJob::Exit;
            }
            if let Some(pending) = queue.pending.front() {
                if pending.control.state.mark_running(pending.admitted_at) {
                    let pending = queue
                        .pending
                        .pop_front()
                        .expect("the running task remains queued until transfer");
                    queue.running.insert(
                        pending.descriptor.id(),
                        RunningJob {
                            control: Some(pending.control.clone()),
                            started_at: Instant::now(),
                            scope: pending.scope.clone(),
                        },
                    );
                    return NextJob::Ready(pending);
                }
                return NextJob::Discarded(
                    queue
                        .take_for_disposal(0, self)
                        .expect("the terminal task remains queued until disposal transfer"),
                );
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

    fn finish_running(&self, id: TaskId, outcome: TaskCleanupOutcome) {
        let running = {
            let mut queue = lock_unpoisoned(&self.queue);
            queue.cleanup_failed |= outcome == TaskCleanupOutcome::Failed;
            let running = queue.running.remove(&id);
            self.changed.notify_all();
            running
        };
        drop(running);
    }

    fn finish_disposal(&self, outcome: TaskCleanupOutcome) {
        let mut queue = lock_unpoisoned(&self.queue);
        queue.cleanup_failed |= outcome == TaskCleanupOutcome::Failed;
        debug_assert!(queue.disposing > 0, "admitted disposal count underflowed");
        queue.disposing -= 1;
        self.changed.notify_all();
    }

    fn release_running_control(&self, id: TaskId) -> Option<TaskControl> {
        lock_unpoisoned(&self.queue)
            .running
            .get_mut(&id)
            .and_then(|running| running.control.take())
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
    inline: bool,
    workers: Arc<TaskWorkers>,
}

struct TaskPoolCloseOwner {
    shared: Arc<ExecutorShared>,
    workers: Arc<TaskWorkers>,
    pending_reaps: Vec<TaskOwnerReapReceipt>,
    shutdown_policy: TaskShutdownPolicy,
    inline: bool,
    close_phase: TaskPoolClosePhase,
    shutdown_report: TaskPoolShutdownReport,
    shutdown_recorded: bool,
    #[cfg(test)]
    construction_cleanup_polls: Option<Arc<AtomicUsize>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskPoolClosePhase {
    Open,
    Draining { deadline: Instant },
    Cancelling { deadline: Instant },
    Joining { deadline: Instant },
    WaitingForRetry { resume: TaskPoolRetryPhase },
    Complete,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskPoolRetryPhase {
    Cancelling,
    Joining,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TaskPoolCloseProgress {
    Pending,
    Incomplete,
    Complete,
}

struct TaskPoolExecutorConstructionFailure {
    error: TaskPoolError,
    close_owner: TaskPoolCloseOwner,
}

impl TaskPoolExecutor {
    fn threaded(
        kind: TaskPoolKind,
        config: TaskPoolConfig,
    ) -> Result<(Self, TaskPoolCloseOwner), TaskPoolExecutorConstructionFailure> {
        let (executor, close_owner) = Self::lazy(kind, config);
        match executor.workers.activate(&executor.shared) {
            Ok(()) => Ok((executor, close_owner)),
            Err(activation) => {
                let message = match activation {
                    WorkerActivationError::Failed(Some(error)) => error.to_string(),
                    WorkerActivationError::Failed(None) | WorkerActivationError::Closed => {
                        "worker activation is unavailable".to_owned()
                    }
                };
                #[cfg(test)]
                let close_owner = {
                    let mut close_owner = close_owner;
                    close_owner.construction_cleanup_polls = worker_spawn_failure_injection(kind)
                        .map(|injection| injection.cleanup_polls);
                    close_owner
                };
                Err(TaskPoolExecutorConstructionFailure {
                    error: TaskPoolError::WorkerSpawnFailed { kind, message },
                    close_owner,
                })
            }
        }
    }

    fn lazy(kind: TaskPoolKind, config: TaskPoolConfig) -> (Self, TaskPoolCloseOwner) {
        let shared = Arc::new(ExecutorShared::new(config.kind(kind).pending().get(), 0));
        let workers = Arc::new(TaskWorkers::dormant(
            kind,
            config.kind(kind).workers().get(),
        ));
        (
            Self {
                shared: shared.clone(),
                inline: false,
                workers: workers.clone(),
            },
            TaskPoolCloseOwner::new(shared, workers, config.shutdown_policy(), false),
        )
    }

    fn inline(kind: TaskPoolKind, config: TaskPoolConfig) -> (Self, TaskPoolCloseOwner) {
        let shared = Arc::new(ExecutorShared::new(config.kind(kind).pending().get(), 0));
        let workers = Arc::new(TaskWorkers::dormant(kind, 0));
        (
            Self {
                shared: shared.clone(),
                inline: true,
                workers: workers.clone(),
            },
            TaskPoolCloseOwner::new(shared, workers, config.shutdown_policy(), true),
        )
    }

    fn admit(&self, pending: PendingJob) -> AdmissionDecision {
        let scope = pending.scope.clone();
        let activation_error = if self.inline
            || self.workers.is_active()
            || scope
                .as_ref()
                .is_some_and(|scope| scope.lock_admission().is_closed())
        {
            None
        } else {
            self.workers.activate(&self.shared).err()
        };
        let activity = scope.as_ref().map(|scope| scope.lock_admission());
        let mut pending = Some(pending);
        let mut discarded = Vec::new();
        let decision;
        {
            let mut queue = lock_unpoisoned(&self.shared.queue);
            let scope_closed = activity.as_ref().is_some_and(|scope| scope.is_closed());
            if !scope_closed
                && queue.lifecycle == QueueLifecycle::Open
                && queue.pending.len() >= self.shared.pending_capacity
            {
                let mut retained = VecDeque::with_capacity(queue.pending.len());
                while let Some(queued) = queue.pending.front() {
                    #[cfg(test)]
                    self.shared
                        .admission_queue_visits
                        .fetch_add(1, Ordering::Relaxed);
                    if queued.control.state.is_terminal() {
                        discarded.push(
                            queue
                                .take_for_disposal(0, &self.shared)
                                .expect("the terminal task remains queued until disposal transfer"),
                        );
                    } else {
                        retained.push_back(
                            queue
                                .pending
                                .pop_front()
                                .expect("the live task remains queued during reclamation"),
                        );
                    }
                }
                queue.pending = retained;
            }

            if scope_closed {
                decision = AdmissionDecision::Rejected(TaskRejectReason::ScopeClosed);
            } else if matches!(activation_error, Some(WorkerActivationError::Failed(_))) {
                decision = AdmissionDecision::Rejected(TaskRejectReason::WorkerStartupFailed);
            } else if queue.lifecycle != QueueLifecycle::Open {
                decision = AdmissionDecision::Rejected(TaskRejectReason::PoolClosed);
            } else {
                let coalesce_position = pending.as_ref().and_then(|new_job| {
                    let domain_key = new_job.descriptor.order_key().domain_key();
                    new_job.coalesce_key.and_then(|coalesce_key| {
                        queue.pending.iter().position(|queued| {
                            #[cfg(test)]
                            self.shared
                                .admission_queue_visits
                                .fetch_add(1, Ordering::Relaxed);
                            !queued.control.state.is_terminal()
                                && same_scope(&queued.scope, &new_job.scope)
                                && queued.descriptor.order_key().domain_key() == domain_key
                                && queued.coalesce_key == Some(coalesce_key)
                        })
                    })
                });
                let mut coalesced = None;
                if let Some(position) = coalesce_position
                    && let Some(replaced) = queue.take_for_disposal(position, &self.shared)
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
                    if cancellation_won && let Some(mut new_job) = pending.take() {
                        prepare_admission(&mut new_job);
                        queue.pending.insert(position, new_job);
                        coalesced = Some(AdmissionDecision::Coalesced {
                            replaced: replaced_id,
                        });
                    }
                }
                if let Some(coalesced) = coalesced {
                    decision = coalesced;
                } else if queue.pending.len() < self.shared.pending_capacity {
                    if let Some(mut new_job) = pending.take() {
                        prepare_admission(&mut new_job);
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
        // Scope close must never wait for a user destructor on the submitting thread.
        drop(activity);

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
}

impl TaskPoolCloseOwner {
    fn new(
        shared: Arc<ExecutorShared>,
        workers: Arc<TaskWorkers>,
        shutdown_policy: TaskShutdownPolicy,
        inline: bool,
    ) -> Self {
        Self {
            shared,
            workers,
            pending_reaps: Vec::new(),
            shutdown_policy,
            inline,
            close_phase: TaskPoolClosePhase::Open,
            shutdown_report: TaskPoolShutdownReport::default(),
            shutdown_recorded: false,
            #[cfg(test)]
            construction_cleanup_polls: None,
        }
    }

    fn begin_close(&mut self) {
        if self.close_phase != TaskPoolClosePhase::Open {
            return;
        }
        let lifecycle = if self.inline {
            QueueLifecycle::Closed
        } else {
            QueueLifecycle::Draining
        };
        self.workers.close_admission(&self.shared, lifecycle);
        self.close_phase = if self.inline {
            TaskPoolClosePhase::Cancelling {
                deadline: deadline_after(self.shutdown_policy.cancel_timeout().get()),
            }
        } else {
            TaskPoolClosePhase::Draining {
                deadline: deadline_after(self.shutdown_policy.drain_timeout().get()),
            }
        };
    }

    fn poll_close(&mut self) -> TaskPoolCloseProgress {
        #[cfg(test)]
        if let Some(polls) = &self.construction_cleanup_polls {
            polls.fetch_add(1, Ordering::AcqRel);
        }
        if self.close_phase == TaskPoolClosePhase::Open {
            self.begin_close();
        }

        loop {
            match self.close_phase {
                TaskPoolClosePhase::Open => unreachable!("close begins before polling"),
                TaskPoolClosePhase::Complete => return TaskPoolCloseProgress::Complete,
                TaskPoolClosePhase::WaitingForRetry { resume } => {
                    self.close_phase = match resume {
                        TaskPoolRetryPhase::Cancelling => TaskPoolClosePhase::Cancelling {
                            deadline: deadline_after(self.shutdown_policy.cancel_timeout().get()),
                        },
                        TaskPoolRetryPhase::Joining => TaskPoolClosePhase::Joining {
                            deadline: deadline_after(self.shutdown_policy.join_timeout().get()),
                        },
                    };
                }
                TaskPoolClosePhase::Draining { deadline } => {
                    let drained = {
                        let queue = lock_unpoisoned(&self.shared.queue);
                        queue.pending.is_empty() && queue.running.is_empty()
                    };
                    if drained {
                        self.close_queue_and_cancel_running();
                        self.close_phase = TaskPoolClosePhase::Joining {
                            deadline: deadline_after(self.shutdown_policy.join_timeout().get()),
                        };
                        continue;
                    }
                    if Instant::now() < deadline {
                        return TaskPoolCloseProgress::Pending;
                    }
                    self.shutdown_report.drain_timed_out = true;
                    self.close_queue_and_cancel_running();
                    self.close_phase = TaskPoolClosePhase::Cancelling {
                        deadline: deadline_after(self.shutdown_policy.cancel_timeout().get()),
                    };
                }
                TaskPoolClosePhase::Cancelling { deadline } => {
                    let pending_remain = self.cancel_pending_batch();
                    if pending_remain {
                        if Instant::now() >= deadline {
                            self.shutdown_report.cancel_timed_out = true;
                            self.close_phase = TaskPoolClosePhase::WaitingForRetry {
                                resume: TaskPoolRetryPhase::Cancelling,
                            };
                            self.record_shutdown();
                            return TaskPoolCloseProgress::Incomplete;
                        }
                        return TaskPoolCloseProgress::Pending;
                    }
                    let running_remain = !lock_unpoisoned(&self.shared.queue).running.is_empty();
                    if !running_remain {
                        self.close_phase = TaskPoolClosePhase::Joining {
                            deadline: deadline_after(self.shutdown_policy.join_timeout().get()),
                        };
                        continue;
                    }
                    if Instant::now() < deadline {
                        return TaskPoolCloseProgress::Pending;
                    }
                    self.shutdown_report.cancel_timed_out = true;
                    self.close_phase = TaskPoolClosePhase::Joining {
                        deadline: deadline_after(self.shutdown_policy.join_timeout().get()),
                    };
                }
                TaskPoolClosePhase::Joining { deadline } => {
                    self.join_finished_workers();
                    self.collect_completed_reaps();
                    let disposal_returned = {
                        let queue = lock_unpoisoned(&self.shared.queue);
                        self.shutdown_report.cleanup_failed |= queue.cleanup_failed;
                        queue.disposing == 0 && queue.pending.is_empty() && queue.running.is_empty()
                    };
                    if self.workers.is_empty() && self.pending_reaps.is_empty() && disposal_returned
                    {
                        self.close_phase = TaskPoolClosePhase::Complete;
                        self.record_shutdown();
                        return TaskPoolCloseProgress::Complete;
                    }
                    if Instant::now() >= deadline {
                        self.shutdown_report.join_timed_out = true;
                        self.close_phase = TaskPoolClosePhase::WaitingForRetry {
                            resume: TaskPoolRetryPhase::Joining,
                        };
                        self.record_shutdown();
                        return TaskPoolCloseProgress::Incomplete;
                    }
                    return TaskPoolCloseProgress::Pending;
                }
            }
        }
    }

    #[cfg(test)]
    fn shutdown(&mut self) -> TaskPoolShutdownReport {
        if self.close_phase == TaskPoolClosePhase::Complete {
            return self.shutdown_report;
        }
        self.begin_close();
        loop {
            match self.poll_close() {
                TaskPoolCloseProgress::Complete | TaskPoolCloseProgress::Incomplete => {
                    return self.shutdown_report;
                }
                TaskPoolCloseProgress::Pending => thread::sleep(Duration::from_millis(1)),
            }
        }
    }

    fn close_queue_and_cancel_running(&mut self) {
        let running = {
            let mut queue = lock_unpoisoned(&self.shared.queue);
            queue.lifecycle = QueueLifecycle::Closed;
            let running = queue
                .running
                .values()
                .filter_map(|running| running.control.clone())
                .collect::<Vec<_>>();
            queue.disposing += running.len();
            self.shared.changed.notify_all();
            running
        };
        #[cfg(test)]
        {
            let hook = lock_unpoisoned(&self.shared.after_running_snapshot_hook).take();
            if let Some(hook) = hook {
                hook();
            }
        }
        for running in running {
            self.shutdown_report.cancellation_requests =
                self.shutdown_report.cancellation_requests.saturating_add(1);
            running.state.cancel(TaskCancellationReason::PoolShutdown);
            drop_close_control(&self.shared, running);
        }
    }

    fn cancel_pending_batch(&mut self) -> bool {
        self.collect_completed_reaps();

        let (pending, pending_remain) = {
            let mut queue = lock_unpoisoned(&self.shared.queue);
            let take = queue.pending.len().min(TASK_CLOSE_CANCEL_BATCH);
            let pending = (0..take)
                .map(|_| {
                    queue
                        .take_for_disposal(0, &self.shared)
                        .expect("the cancellation batch is bounded by queued tasks")
                })
                .collect::<VecDeque<_>>();
            (pending, !queue.pending.is_empty())
        };
        for job in &pending {
            if job
                .control
                .state
                .cancel(TaskCancellationReason::PoolShutdown)
            {
                self.shutdown_report.cancelled_pending =
                    self.shutdown_report.cancelled_pending.saturating_add(1);
            }
        }
        if !pending.is_empty() {
            self.pending_reaps
                .push(retain_abandoned_task_owner(AbandonedTaskPoolOwner {
                    pending,
                    workers: None,
                    disposal_owner: None,
                }));
        }
        pending_remain || !self.pending_reaps.is_empty()
    }

    fn collect_completed_reaps(&mut self) {
        self.pending_reaps.retain(|receipt| {
            if !receipt.is_complete() {
                return true;
            }
            self.shutdown_report.cleanup_failed |= receipt.cleanup_failed();
            false
        });
        if !self.pending_reaps.is_empty() {
            retry_task_owner_reaper();
        }
    }

    fn join_finished_workers(&mut self) {
        self.workers.join_finished(&mut self.shutdown_report);
    }

    fn record_shutdown(&mut self) {
        if self.shutdown_recorded {
            return;
        }
        self.shutdown_recorded = true;
        let report = self.shutdown_report;
        update_counters(&self.shared.counters, |counters| {
            counters.shutdowns = counters.shutdowns.saturating_add(1);
            if report.timed_out() {
                counters.shutdown_timeouts = counters.shutdown_timeouts.saturating_add(1);
            }
        });
    }

    fn abandon(&mut self) {
        self.workers
            .close_admission(&self.shared, QueueLifecycle::Closed);
        let (pending, running) = {
            let mut queue = lock_unpoisoned(&self.shared.queue);
            queue.lifecycle = QueueLifecycle::Closed;
            let pending = (0..queue.pending.len())
                .map(|_| {
                    queue
                        .take_for_disposal(0, &self.shared)
                        .expect("abandonment retains every queued disposal obligation")
                })
                .collect::<VecDeque<_>>();
            let running = queue
                .running
                .values()
                .filter_map(|running| running.control.clone())
                .collect::<Vec<_>>();
            queue.disposing += running.len();
            self.shared.changed.notify_all();
            (pending, running)
        };
        // A completed task's snapshot may be its result's final owner. Retain every snapshot
        // until cancellation is published and pending captures have their disposal owner.
        for running in &running {
            running.state.cancel(TaskCancellationReason::PoolShutdown);
        }
        for pending in &pending {
            pending
                .control
                .state
                .cancel(TaskCancellationReason::PoolShutdown);
        }
        self.join_finished_workers();
        retain_abandoned_task_owner(AbandonedTaskPoolOwner {
            pending,
            workers: Some(self.workers.clone()),
            disposal_owner: Some(Arc::clone(&self.shared)),
        });
        for control in running {
            drop_close_control(&self.shared, control);
        }
    }
}

impl Drop for TaskPoolCloseOwner {
    fn drop(&mut self) {
        if self.close_phase != TaskPoolClosePhase::Complete {
            self.abandon();
        }
    }
}

struct TaskPoolsCloseOwner {
    io: Option<TaskPoolCloseOwner>,
    compute: Option<TaskPoolCloseOwner>,
    async_compute: Option<TaskPoolCloseOwner>,
}

impl TaskPoolsCloseOwner {
    const fn empty() -> Self {
        Self {
            io: None,
            compute: None,
            async_compute: None,
        }
    }

    fn new(
        io: TaskPoolCloseOwner,
        compute: TaskPoolCloseOwner,
        async_compute: TaskPoolCloseOwner,
    ) -> Self {
        Self {
            io: Some(io),
            compute: Some(compute),
            async_compute: Some(async_compute),
        }
    }

    fn insert(&mut self, kind: TaskPoolKind, owner: TaskPoolCloseOwner) {
        let destination = match kind {
            TaskPoolKind::Io => &mut self.io,
            TaskPoolKind::Compute => &mut self.compute,
            TaskPoolKind::AsyncCompute => &mut self.async_compute,
        };
        debug_assert!(destination.is_none());
        *destination = Some(owner);
    }

    fn begin_close(&mut self) {
        for kind in TaskPoolKind::ALL {
            if let Some(owner) = self.executor_mut(kind) {
                owner.begin_close();
            }
        }
    }

    fn poll_close(&mut self) -> TaskPoolsCloseProgress {
        self.begin_close();
        let mut aggregate = TaskPoolsCloseProgress::Complete;
        for kind in TaskPoolKind::ALL {
            let Some(owner) = self.executor_mut(kind) else {
                continue;
            };
            let progress = owner.poll_close();
            aggregate = match (aggregate, progress) {
                (TaskPoolsCloseProgress::Incomplete, _)
                | (_, TaskPoolCloseProgress::Incomplete) => TaskPoolsCloseProgress::Incomplete,
                (TaskPoolsCloseProgress::Pending, _) | (_, TaskPoolCloseProgress::Pending) => {
                    TaskPoolsCloseProgress::Pending
                }
                _ => TaskPoolsCloseProgress::Complete,
            };
        }
        aggregate
    }

    fn executor_mut(&mut self, kind: TaskPoolKind) -> Option<&mut TaskPoolCloseOwner> {
        match kind {
            TaskPoolKind::Io => self.io.as_mut(),
            TaskPoolKind::Compute => self.compute.as_mut(),
            TaskPoolKind::AsyncCompute => self.async_compute.as_mut(),
        }
    }

    fn executor(&self, kind: TaskPoolKind) -> Option<&TaskPoolCloseOwner> {
        match kind {
            TaskPoolKind::Io => self.io.as_ref(),
            TaskPoolKind::Compute => self.compute.as_ref(),
            TaskPoolKind::AsyncCompute => self.async_compute.as_ref(),
        }
    }

    fn report(&self) -> TaskShutdownReport {
        let mut report = TaskShutdownReport::default();
        for kind in TaskPoolKind::ALL {
            if let Some(owner) = self.executor(kind) {
                let mut pool_report = owner.shutdown_report;
                pool_report.cleanup_failed |= lock_unpoisoned(&owner.shared.queue).cleanup_failed;
                report.per_kind.insert(kind, pool_report);
            }
        }
        report.complete = self.is_complete();
        report
    }

    fn is_complete(&self) -> bool {
        TaskPoolKind::ALL.into_iter().all(|kind| {
            self.executor(kind)
                .is_none_or(|owner| owner.close_phase == TaskPoolClosePhase::Complete)
        })
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
    cleanup_polls: Arc<AtomicUsize>,
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
    cleanup_polls: Arc<AtomicUsize>,
}

#[cfg(test)]
impl WorkerSpawnFailureProbe {
    pub(crate) fn started(&self) -> usize {
        self.started.load(Ordering::Acquire)
    }

    pub(crate) fn exited(&self) -> usize {
        self.exited.load(Ordering::Acquire)
    }

    pub(crate) fn cleanup_polls(&self) -> usize {
        self.cleanup_polls.load(Ordering::Acquire)
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
                NextJob::Discarded(pending) => {
                    safe_drop_pending(pending);
                }
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
            // A closure body panic and capture destruction panic share this unwind boundary.
            control.state.return_observer().cleanup_failed();
            control.state.fail(TaskFailure::Panicked);
            let _ = catch_unwind(AssertUnwindSafe(|| drop(payload)));
        }
    }
    // Retain physical running evidence while releasing references that may own a result.
    // Destructors run outside the queue mutex so stats and close remain observable.
    drop(shared.release_running_control(id));
    drop(control);
    let outcome = safe_drop_pending(pending);
    shared.finish_running(id, outcome);
}

fn safe_drop_pending(mut pending: PendingJob) -> TaskCleanupOutcome {
    // Accounting must finish explicitly after capture destruction, not in an earlier field drop.
    let disposal_owner = pending.disposal_owner.take();
    let returned = pending.control.state.return_observer();
    if let Err(payload) = catch_unwind(AssertUnwindSafe(|| drop(pending))) {
        returned.cleanup_failed();
        let _ = catch_unwind(AssertUnwindSafe(|| drop(payload)));
    }
    returned.finish();
    let outcome = returned.cleanup_outcome();
    if let Some(owner) = disposal_owner {
        owner.finish_disposal(outcome);
    }
    outcome
}

fn drop_close_control(shared: &ExecutorShared, control: TaskControl) {
    let returned = control.state.return_observer();
    drop(control);
    shared.finish_disposal(returned.cleanup_outcome());
}

fn update_counters(counters: &SharedTaskCounters, update: impl FnOnce(&mut TaskCounters)) {
    let mut counters = lock_unpoisoned(counters);
    update(&mut counters);
}

fn deadline_after(duration: Duration) -> Instant {
    let now = Instant::now();
    now.checked_add(duration).unwrap_or(now)
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

struct AbandonedTaskPoolOwner {
    pending: VecDeque<PendingJob>,
    workers: Option<Arc<TaskWorkers>>,
    disposal_owner: Option<Arc<ExecutorShared>>,
}

impl AbandonedTaskPoolOwner {
    fn take_pending_for_reap(&mut self) -> Option<PendingJob> {
        self.pending.pop_front()
    }

    fn poll_finished_workers(&mut self) {
        if let Some(workers) = &self.workers {
            workers.join_finished(&mut TaskPoolShutdownReport::default());
        }
    }

    fn is_reaped(&self, pending_in_flight: &AtomicUsize) -> bool {
        self.pending.is_empty()
            && pending_in_flight.load(Ordering::Acquire) == 0
            && self
                .workers
                .as_ref()
                .is_none_or(|workers| workers.is_empty())
            && self
                .disposal_owner
                .as_ref()
                .is_none_or(|owner| lock_unpoisoned(&owner.queue).disposing == 0)
    }
}

struct PendingDropWork {
    pending: PendingJob,
    in_flight: PendingDropInFlight,
    receipt: TaskOwnerReapReceipt,
}

impl PendingDropWork {
    fn new(
        pending: PendingJob,
        owner_in_flight: Arc<AtomicUsize>,
        receipt: TaskOwnerReapReceipt,
    ) -> Self {
        Self {
            pending,
            in_flight: PendingDropInFlight::new(owner_in_flight),
            receipt,
        }
    }

    fn drop_pending(self) {
        let Self {
            pending,
            in_flight,
            receipt,
        } = self;
        receipt.record_outcome(safe_drop_pending(pending));
        drop(in_flight);
    }

    fn into_pending(self) -> PendingJob {
        let Self {
            pending, in_flight, ..
        } = self;
        drop(in_flight);
        pending
    }
}

struct PendingDropInFlight {
    owner_in_flight: Arc<AtomicUsize>,
}

impl PendingDropInFlight {
    fn new(owner_in_flight: Arc<AtomicUsize>) -> Self {
        owner_in_flight.fetch_add(1, Ordering::AcqRel);
        Self { owner_in_flight }
    }
}

impl Drop for PendingDropInFlight {
    fn drop(&mut self) {
        let previous = self.owner_in_flight.fetch_sub(1, Ordering::AcqRel);
        debug_assert!(previous > 0, "pending-drop in-flight count underflowed");
    }
}

struct PendingDropLane {
    sender: Option<SyncSender<PendingDropWork>>,
    worker: Option<JoinHandle<()>>,
}

struct PendingDropLaneSet {
    lanes: Vec<PendingDropLane>,
    next_lane: usize,
}

impl PendingDropLaneSet {
    fn new() -> Self {
        Self {
            lanes: Vec::with_capacity(TASK_REAPER_DROP_LANES),
            next_lane: 0,
        }
    }

    fn push(&mut self, lane: PendingDropLane) {
        self.lanes.push(lane);
    }

    fn try_dispatch(
        &mut self,
        pending: PendingJob,
        owner_in_flight: Arc<AtomicUsize>,
        receipt: TaskOwnerReapReceipt,
    ) -> Result<(), PendingJob> {
        if self.lanes.is_empty() {
            return Err(pending);
        }

        let lane_count = self.lanes.len();
        let mut work = PendingDropWork::new(pending, owner_in_flight, receipt);
        for offset in 0..lane_count {
            let lane_index = (self.next_lane + offset) % lane_count;
            let Some(sender) = self.lanes[lane_index].sender.as_ref() else {
                continue;
            };
            match sender.try_send(work) {
                Ok(()) => {
                    self.next_lane = (lane_index + 1) % lane_count;
                    return Ok(());
                }
                Err(TrySendError::Full(returned)) => work = returned,
                Err(TrySendError::Disconnected(returned)) => {
                    work = returned;
                    self.lanes[lane_index].sender = None;
                }
            }
        }
        Err(work.into_pending())
    }
}

impl Drop for PendingDropLaneSet {
    fn drop(&mut self) {
        for lane in &mut self.lanes {
            lane.sender = None;
        }
        for lane in &mut self.lanes {
            if let Some(worker) = lane.worker.take() {
                let _ = worker.join();
            }
        }
    }
}

#[cfg(not(test))]
fn spawn_pending_drop_lane(index: usize) -> io::Result<PendingDropLane> {
    let (sender, receiver) = mpsc::sync_channel(0);
    let worker = thread::Builder::new()
        .name(format!("nara-task-owner-drop-{index}"))
        .spawn(move || pending_drop_lane_main(receiver))?;
    Ok(PendingDropLane {
        sender: Some(sender),
        worker: Some(worker),
    })
}

#[cfg(test)]
fn spawn_pending_drop_lane(
    index: usize,
    probe: Option<PendingDropLaneProbe>,
) -> io::Result<PendingDropLane> {
    let (sender, receiver) = mpsc::sync_channel(0);
    let worker = thread::Builder::new()
        .name(format!("nara-task-owner-drop-{index}"))
        .spawn(move || pending_drop_lane_main(receiver, probe))?;
    Ok(PendingDropLane {
        sender: Some(sender),
        worker: Some(worker),
    })
}

#[cfg(not(test))]
fn pending_drop_lane_main(receiver: Receiver<PendingDropWork>) {
    while let Ok(work) = receiver.recv() {
        work.drop_pending();
    }
}

#[cfg(test)]
fn pending_drop_lane_main(
    receiver: Receiver<PendingDropWork>,
    probe: Option<PendingDropLaneProbe>,
) {
    let _lifetime = probe.map(PendingDropLaneLifetime::started);
    while let Ok(work) = receiver.recv() {
        work.drop_pending();
    }
}

#[cfg(test)]
#[derive(Clone, Default)]
pub(crate) struct PendingDropLaneProbe {
    live: Arc<AtomicUsize>,
}

#[cfg(test)]
impl PendingDropLaneProbe {
    pub(crate) fn live(&self) -> usize {
        self.live.load(Ordering::Acquire)
    }
}

#[cfg(test)]
struct PendingDropLaneLifetime(PendingDropLaneProbe);

#[cfg(test)]
impl PendingDropLaneLifetime {
    fn started(probe: PendingDropLaneProbe) -> Self {
        probe.live.fetch_add(1, Ordering::AcqRel);
        Self(probe)
    }
}

#[cfg(test)]
impl Drop for PendingDropLaneLifetime {
    fn drop(&mut self) {
        self.0.live.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Clone)]
pub(crate) struct TaskOwnerReapReceipt {
    complete: Arc<AtomicBool>,
    cleanup_failed: Arc<AtomicBool>,
}

impl TaskOwnerReapReceipt {
    fn pending() -> Self {
        Self {
            complete: Arc::new(AtomicBool::new(false)),
            cleanup_failed: Arc::new(AtomicBool::new(false)),
        }
    }

    fn completed() -> Self {
        Self {
            complete: Arc::new(AtomicBool::new(true)),
            cleanup_failed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub(crate) fn is_complete(&self) -> bool {
        self.complete.load(Ordering::Acquire)
    }

    pub(crate) fn cleanup_failed(&self) -> bool {
        self.cleanup_failed.load(Ordering::Acquire)
    }

    fn record_outcome(&self, outcome: TaskCleanupOutcome) {
        if outcome == TaskCleanupOutcome::Failed {
            self.cleanup_failed.store(true, Ordering::Release);
        }
    }

    fn mark_complete(&self) {
        self.complete.store(true, Ordering::Release);
    }
}

struct TaskOwnerReapRequest {
    owner: AbandonedTaskPoolOwner,
    receipt: TaskOwnerReapReceipt,
    pending_in_flight: Arc<AtomicUsize>,
    counted: bool,
}

impl TaskOwnerReapRequest {
    fn new(owner: AbandonedTaskPoolOwner, receipt: TaskOwnerReapReceipt, counted: bool) -> Self {
        Self {
            owner,
            receipt,
            pending_in_flight: Arc::new(AtomicUsize::new(0)),
            counted,
        }
    }

    fn poll_reap(&mut self, drop_lanes: &mut PendingDropLaneSet) -> bool {
        self.owner.poll_finished_workers();
        if self.pending_in_flight.load(Ordering::Acquire) == 0
            && let Some(pending) = self.owner.take_pending_for_reap()
            && let Err(pending) = drop_lanes.try_dispatch(
                pending,
                Arc::clone(&self.pending_in_flight),
                self.receipt.clone(),
            )
        {
            self.owner.pending.push_front(pending);
        }

        if !self.owner.is_reaped(&self.pending_in_flight) {
            return false;
        }
        if let Some(owner) = &self.owner.disposal_owner
            && lock_unpoisoned(&owner.queue).cleanup_failed
        {
            self.receipt.record_outcome(TaskCleanupOutcome::Failed);
        }
        self.receipt.mark_complete();
        if self.counted {
            ABANDONED_TASK_OWNER_COUNT.fetch_sub(1, Ordering::AcqRel);
        }
        true
    }
}

enum TaskOwnerReaperMessage {
    Retain(TaskOwnerReapRequest),
    RetainBatch(VecDeque<TaskOwnerReapRequest>),
    #[cfg(test)]
    Shutdown,
}

struct TaskOwnerReaperSupervisor {
    sender: Option<Sender<TaskOwnerReaperMessage>>,
    worker: Option<JoinHandle<()>>,
    fallback: VecDeque<TaskOwnerReapRequest>,
    #[cfg(test)]
    spawn_failures: usize,
    #[cfg(test)]
    lane_spawn_failure_at: Option<usize>,
    #[cfg(test)]
    lane_probe: Option<PendingDropLaneProbe>,
}

impl TaskOwnerReaperSupervisor {
    const fn new() -> Self {
        Self {
            sender: None,
            worker: None,
            fallback: VecDeque::new(),
            #[cfg(test)]
            spawn_failures: 0,
            #[cfg(test)]
            lane_spawn_failure_at: None,
            #[cfg(test)]
            lane_probe: None,
        }
    }

    #[cfg(test)]
    fn with_spawn_failures(spawn_failures: usize) -> Self {
        Self {
            spawn_failures,
            ..Self::new()
        }
    }

    #[cfg(test)]
    fn with_lane_spawn_failure(lane_index: usize) -> Self {
        let mut supervisor = Self::new();
        supervisor.lane_spawn_failure_at = Some(lane_index);
        supervisor.lane_probe = Some(PendingDropLaneProbe::default());
        supervisor
    }

    fn retain(&mut self, request: TaskOwnerReapRequest) {
        self.recover_finished_worker();
        if self.sender.is_none() && !self.try_start() {
            self.fallback.push_back(request);
            return;
        }

        let sender = self
            .sender
            .as_ref()
            .expect("a started task owner reaper retains its sender");
        if let Err(error) = sender.send(TaskOwnerReaperMessage::Retain(request)) {
            let TaskOwnerReaperMessage::Retain(request) = error.0 else {
                unreachable!("retain sends exactly one task owner")
            };
            self.fallback.push_back(request);
            self.sender = None;
            self.recover_finished_worker();
        }
    }

    fn retry(&mut self) {
        self.recover_finished_worker();
        if self.sender.is_none() {
            let _ = self.try_start();
        }
    }

    fn try_start(&mut self) -> bool {
        if self.sender.is_some() {
            return true;
        }
        if self.worker.is_some() {
            return false;
        }

        let Ok((sender, worker)) = self.spawn_worker() else {
            return false;
        };
        self.sender = Some(sender);
        self.worker = Some(worker);

        if self.fallback.is_empty() {
            return true;
        }
        let fallback = std::mem::take(&mut self.fallback);
        let sender = self
            .sender
            .as_ref()
            .expect("a started task owner reaper retains its sender");
        if let Err(error) = sender.send(TaskOwnerReaperMessage::RetainBatch(fallback)) {
            let TaskOwnerReaperMessage::RetainBatch(mut fallback) = error.0 else {
                unreachable!("fallback transfer sends exactly one retained batch")
            };
            self.fallback.append(&mut fallback);
            self.sender = None;
            self.recover_finished_worker();
            return false;
        }
        true
    }

    fn spawn_worker(&mut self) -> io::Result<(Sender<TaskOwnerReaperMessage>, JoinHandle<()>)> {
        #[cfg(test)]
        if self.spawn_failures > 0 {
            self.spawn_failures -= 1;
            return Err(io::Error::other("injected task owner reaper spawn failure"));
        }

        let mut drop_lanes = PendingDropLaneSet::new();
        for lane_index in 0..TASK_REAPER_DROP_LANES {
            #[cfg(test)]
            if self.lane_spawn_failure_at == Some(lane_index) {
                self.lane_spawn_failure_at = None;
                return Err(io::Error::other(
                    "injected task owner reaper drop-lane spawn failure",
                ));
            }
            #[cfg(test)]
            let lane = spawn_pending_drop_lane(lane_index, self.lane_probe.clone())?;
            #[cfg(not(test))]
            let lane = spawn_pending_drop_lane(lane_index)?;
            drop_lanes.push(lane);
        }

        let (sender, receiver) = mpsc::channel();
        let worker = thread::Builder::new()
            .name("nara-task-owner-reaper".to_owned())
            .spawn(move || reap_abandoned_task_owners(receiver, drop_lanes))?;
        Ok((sender, worker))
    }

    fn recover_finished_worker(&mut self) {
        if !self.worker.as_ref().is_some_and(JoinHandle::is_finished) {
            return;
        }
        self.sender = None;
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }

    #[cfg(test)]
    fn shutdown(&mut self) {
        self.retry();
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(TaskOwnerReaperMessage::Shutdown);
        }
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
    }
}

fn retain_abandoned_task_owner(owner: AbandonedTaskPoolOwner) -> TaskOwnerReapReceipt {
    if owner.is_reaped(&AtomicUsize::new(0)) {
        let receipt = TaskOwnerReapReceipt::completed();
        if let Some(shared) = &owner.disposal_owner
            && lock_unpoisoned(&shared.queue).cleanup_failed
        {
            receipt.record_outcome(TaskCleanupOutcome::Failed);
        }
        return receipt;
    }
    let receipt = TaskOwnerReapReceipt::pending();
    #[cfg(test)]
    let owner = {
        let mut owner = Some(owner);
        TEST_TASK_OWNER_REAPER.with(|reaper| {
            if let Some(reaper) = reaper.borrow_mut().as_mut() {
                reaper.retain(TaskOwnerReapRequest::new(
                    owner.take().unwrap(),
                    receipt.clone(),
                    false,
                ));
            }
        });
        let Some(owner) = owner else {
            return receipt;
        };
        owner
    };
    ABANDONED_TASK_OWNER_COUNT.fetch_add(1, Ordering::AcqRel);
    lock_unpoisoned(&TASK_OWNER_REAPER_SUPERVISOR).retain(TaskOwnerReapRequest::new(
        owner,
        receipt.clone(),
        true,
    ));
    receipt
}

fn prewarm_task_owner_reaper() {
    lock_unpoisoned(&TASK_OWNER_REAPER_SUPERVISOR).retry();
}

fn retry_task_owner_reaper() {
    #[cfg(test)]
    if TEST_TASK_OWNER_REAPER.with(|reaper| {
        let mut reaper = reaper.borrow_mut();
        if let Some(reaper) = reaper.as_mut() {
            reaper.retry();
            true
        } else {
            false
        }
    }) {
        return;
    }
    lock_unpoisoned(&TASK_OWNER_REAPER_SUPERVISOR).retry();
}

#[cfg(test)]
thread_local! {
    static TEST_TASK_OWNER_REAPER: RefCell<Option<TaskOwnerReaperSupervisor>> = const { RefCell::new(None) };
}

#[cfg(test)]
pub(crate) fn with_reaper_spawn_failure_for_tests(operation: impl FnOnce()) {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            let supervisor = TEST_TASK_OWNER_REAPER.with(|reaper| reaper.borrow_mut().take());
            if let Some(mut supervisor) = supervisor {
                supervisor.shutdown();
            }
        }
    }
    TEST_TASK_OWNER_REAPER.with(|reaper| {
        assert!(reaper.borrow().is_none(), "nested test reaper override");
        *reaper.borrow_mut() = Some(TaskOwnerReaperSupervisor::with_spawn_failures(1));
    });
    let _reset = Reset;
    operation();
}

fn reap_abandoned_task_owners(
    receiver: Receiver<TaskOwnerReaperMessage>,
    mut drop_lanes: PendingDropLaneSet,
) {
    let mut retained = VecDeque::new();
    let mut disconnected = false;
    let mut shutdown_requested = false;
    loop {
        for _ in 0..TASK_REAPER_INTAKE_BATCH {
            match receiver.try_recv() {
                Ok(message) => {
                    retain_reaper_message(message, &mut retained, &mut shutdown_requested)
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }

        let owners_to_poll = retained.len().min(TASK_REAPER_OWNER_POLL_BATCH);
        for _ in 0..owners_to_poll {
            let mut request = retained
                .pop_front()
                .expect("the reaper poll budget is bounded by retained owners");
            if !request.poll_reap(&mut drop_lanes) {
                retained.push_back(request);
            }
        }

        let stopping = shutdown_requested || disconnected;
        if stopping && retained.is_empty() {
            return;
        }
        if retained.is_empty() {
            match receiver.recv() {
                Ok(message) => {
                    retain_reaper_message(message, &mut retained, &mut shutdown_requested)
                }
                Err(_) => disconnected = true,
            }
        } else {
            match receiver.recv_timeout(TASK_REAPER_ROUND_WAIT) {
                Ok(message) => {
                    retain_reaper_message(message, &mut retained, &mut shutdown_requested)
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => disconnected = true,
            }
        }
    }
}

fn retain_reaper_message(
    message: TaskOwnerReaperMessage,
    retained: &mut VecDeque<TaskOwnerReapRequest>,
    _shutdown_requested: &mut bool,
) {
    match message {
        TaskOwnerReaperMessage::Retain(request) => retained.push_back(request),
        TaskOwnerReaperMessage::RetainBatch(mut batch) => retained.append(&mut batch),
        #[cfg(test)]
        TaskOwnerReaperMessage::Shutdown => *_shutdown_requested = true,
    }
}

#[cfg(test)]
pub(crate) fn retained_abandoned_worker_count_for_tests() -> usize {
    ABANDONED_TASK_OWNER_COUNT.load(Ordering::Acquire) as usize
}

#[cfg(test)]
pub(crate) fn reap_abandoned_workers_for_tests() {
    thread::yield_now();
}

#[cfg(test)]
pub(crate) struct TaskOwnerReaperHarness {
    supervisor: TaskOwnerReaperSupervisor,
}

#[cfg(test)]
struct ReaperTestDropJob {
    on_drop: Option<Box<dyn FnOnce() + Send>>,
}

#[cfg(test)]
impl ReaperTestDropJob {
    fn invoke(&mut self) {
        if let Some(on_drop) = self.on_drop.take() {
            on_drop();
        }
    }
}

#[cfg(test)]
impl ErasedJob for ReaperTestDropJob {
    fn run(&mut self) {
        self.invoke();
    }
}

#[cfg(test)]
impl Drop for ReaperTestDropJob {
    fn drop(&mut self) {
        self.invoke();
    }
}

#[cfg(test)]
impl TaskOwnerReaperHarness {
    pub(crate) fn new() -> Self {
        Self {
            supervisor: TaskOwnerReaperSupervisor::new(),
        }
    }

    pub(crate) fn with_spawn_failures(spawn_failures: usize) -> Self {
        Self {
            supervisor: TaskOwnerReaperSupervisor::with_spawn_failures(spawn_failures),
        }
    }

    pub(crate) fn with_lane_spawn_failure(lane_index: usize) -> Self {
        Self {
            supervisor: TaskOwnerReaperSupervisor::with_lane_spawn_failure(lane_index),
        }
    }

    pub(crate) fn lane_probe(&self) -> PendingDropLaneProbe {
        self.supervisor
            .lane_probe
            .clone()
            .expect("lane-spawn failure harness installs a lifetime probe")
    }

    pub(crate) fn retain_finished_worker(&mut self) -> TaskOwnerReapReceipt {
        let receipt = TaskOwnerReapReceipt::pending();
        self.supervisor.retain(TaskOwnerReapRequest::new(
            AbandonedTaskPoolOwner {
                pending: VecDeque::new(),
                workers: Some(Arc::new(TaskWorkers::retained_for_tests(vec![
                    thread::spawn(|| {}),
                ]))),
                disposal_owner: None,
            },
            receipt.clone(),
            false,
        ));
        receipt
    }

    pub(crate) fn retain_pending_drops(
        &mut self,
        on_drop: Vec<Box<dyn FnOnce() + Send>>,
    ) -> TaskOwnerReapReceipt {
        let counters = Arc::new(Mutex::new(TaskCounters::default()));
        let instance_token = Arc::new(TaskPoolInstanceToken::default());
        let pending = on_drop
            .into_iter()
            .enumerate()
            .map(|(index, on_drop)| {
                let id = TaskId((index as u64).saturating_add(1));
                let descriptor = TaskDescriptor {
                    id,
                    kind: TaskPoolKind::Io,
                    order_key: TaskOrderKey::new(0, TaskDomainKey::new(0), id),
                };
                let state = Arc::new(TaskState::<()>::new(
                    Arc::clone(&counters),
                    Arc::clone(&instance_token),
                    None,
                ));
                PendingJob {
                    descriptor,
                    coalesce_key: None,
                    admitted_at: Instant::now(),
                    disposal_owner: None,
                    scope: None,
                    control: TaskControl { state },
                    job: Box::new(ReaperTestDropJob {
                        on_drop: Some(on_drop),
                    }),
                }
            })
            .collect();
        let receipt = TaskOwnerReapReceipt::pending();
        self.supervisor.retain(TaskOwnerReapRequest::new(
            AbandonedTaskPoolOwner {
                pending,
                workers: None,
                disposal_owner: None,
            },
            receipt.clone(),
            false,
        ));
        receipt
    }

    pub(crate) fn retry(&mut self) {
        self.supervisor.retry();
    }

    pub(crate) fn fallback_len(&self) -> usize {
        self.supervisor.fallback.len()
    }
}

#[cfg(test)]
impl Drop for TaskOwnerReaperHarness {
    fn drop(&mut self) {
        self.supervisor.shutdown();
    }
}

fn wait_unpoisoned<'a, T>(condvar: &Condvar, guard: MutexGuard<'a, T>) -> MutexGuard<'a, T> {
    condvar
        .wait(guard)
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[derive(Debug, Default, Clone)]
pub struct TaskPlugin {
    config: TaskPoolConfig,
    facilities: TaskPluginFacilities,
}

#[derive(Debug, Default, Clone)]
enum TaskPluginFacilities {
    #[default]
    Eager,
    Lazy,
    Shared(Arc<TaskFacilities>),
}

pub const TASK_PLUGIN_ID: nara_app::PluginId = nara_app::PluginId::new("nara.tasks");
const TASK_PLUGIN_DEFINITION_ID: nara_app::PluginDefinitionId =
    nara_app::PluginDefinitionId::new("nara.tasks.configured", 1);
pub const TASK_POOLS_SHUTDOWN_OBLIGATION: nara_app::PluginShutdownObligationId =
    nara_app::PluginShutdownObligationId::new("nara.tasks.pools");
/// Identifies the Tasks participant in managed close acquisition evidence.
pub const TASK_POOLS_CLOSE_PARTICIPANT: RuntimeCloseParticipantId =
    RuntimeCloseParticipantId::new("nara.tasks.pools");
pub const TASK_PLUGIN_DECLARATION: nara_app::PluginDeclaration =
    nara_app::PluginDeclaration::new(TASK_PLUGIN_ID, nara_app::PluginCategory::Service)
        .shutdown_obligations(&[TASK_POOLS_SHUTDOWN_OBLIGATION]);

impl TaskPlugin {
    #[must_use]
    pub const fn new(config: TaskPoolConfig) -> Self {
        Self {
            config,
            facilities: TaskPluginFacilities::Eager,
        }
    }

    /// Installs private facilities whose workers start on first use of each kind.
    ///
    /// The plugin retains the physical close owner before exposing the facade. First submission
    /// may wait for OS worker creation or reject with [`TaskRejectReason::WorkerStartupFailed`];
    /// partial workers remain owned until physical retirement. Unused kinds acquire no workers.
    /// Installation requires an unoccupied task facade so an external owner cannot bypass this
    /// physical ownership contract.
    #[must_use]
    pub const fn lazy(config: TaskPoolConfig) -> Self {
        Self {
            config,
            facilities: TaskPluginFacilities::Lazy,
        }
    }

    /// Installs one independently retired activity on Host-selected shared facilities.
    ///
    /// The installed plugin retains this strong facilities owner. Keep an outer Host owner while
    /// other manually created activities still need the workers. Last-owner Drop uses the existing
    /// retained physical cleanup fallback; activity completion never reports shared worker join.
    #[must_use]
    pub fn shared(facilities: Arc<TaskFacilities>) -> Self {
        Self {
            config: *facilities.config(),
            facilities: TaskPluginFacilities::Shared(facilities),
        }
    }
}

/// Creates a repeatable task plugin definition with canonical pool settings.
#[must_use]
pub fn plugin(config: TaskPoolConfig) -> nara_app::PluginDefinition {
    task_plugin_definition(config, false)
}

/// Creates a repeatable owned-lazy task plugin with an explicit activation identity.
///
/// See [`TaskPlugin::lazy`] for first-use costs, failure and physical close ownership.
#[must_use]
pub fn lazy_plugin(config: TaskPoolConfig) -> nara_app::PluginDefinition {
    task_plugin_definition(config, true)
}

fn task_plugin_definition(config: TaskPoolConfig, lazy: bool) -> nara_app::PluginDefinition {
    let canonical_configuration = task_plugin_configuration(config, lazy);
    nara_app::PluginDefinition::infallible::<TaskPlugin, _>(
        TASK_PLUGIN_DEFINITION_ID,
        canonical_configuration,
        move || {
            if lazy {
                TaskPlugin::lazy(config)
            } else {
                TaskPlugin::new(config)
            }
        },
    )
}

fn task_plugin_configuration(config: TaskPoolConfig, lazy: bool) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(96);
    bytes.extend_from_slice(b"nara.tasks.plugin-config.v1\0");
    bytes.push(u8::from(lazy));
    for kind in TaskPoolKind::ALL {
        let kind = config.kind(kind);
        bytes.extend_from_slice(&(kind.workers().get() as u64).to_le_bytes());
        bytes.extend_from_slice(&(kind.pending().get() as u64).to_le_bytes());
    }
    let shutdown = config.shutdown_policy();
    for timeout in [
        shutdown.drain_timeout().get(),
        shutdown.cancel_timeout().get(),
        shutdown.join_timeout().get(),
    ] {
        bytes.extend_from_slice(&timeout.as_secs().to_le_bytes());
        bytes.extend_from_slice(&timeout.subsec_nanos().to_le_bytes());
    }
    bytes
}

impl Plugin for TaskPlugin {
    fn declaration() -> &'static nara_app::PluginDeclaration {
        &TASK_PLUGIN_DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let construction = match &self.facilities {
            TaskPluginFacilities::Shared(facilities) => {
                if app.world().contains_resource::<TaskPools>() {
                    return Err(PluginError::SetupFailed {
                        plugin: TASK_PLUGIN_ID,
                        message: "shared task installation requires an unoccupied task facade"
                            .to_owned(),
                    });
                }
                let scope = facilities.scope();
                let spawner = scope.spawner();
                app.register_plugin_runtime_close_participant(
                    TASK_POOLS_SHUTDOWN_OBLIGATION,
                    TASK_POOLS_CLOSE_PARTICIPANT,
                    scope,
                )?;
                app.insert_resource(spawner)?;
                return Ok(());
            }
            TaskPluginFacilities::Lazy if app.world().contains_resource::<TaskPools>() => {
                return Err(PluginError::SetupFailed {
                    plugin: TASK_PLUGIN_ID,
                    message: "owned lazy task installation requires an unoccupied task facade"
                        .to_owned(),
                });
            }
            TaskPluginFacilities::Eager if app.world().contains_resource::<TaskPools>() => {
                // A pre-existing pool is externally owned and participates only in shutdown.
                app.register_plugin_shutdown_obligation(TASK_POOLS_SHUTDOWN_OBLIGATION)?;
                return Ok(());
            }
            TaskPluginFacilities::Eager => TaskPools::try_new_retained(self.config),
            TaskPluginFacilities::Lazy => {
                TaskPools::try_new_lazy(self.config).map_err(TaskPoolsConstructionFailure::invalid)
            }
        };
        let mut pools = match construction {
            Ok(pools) => pools,
            Err(failure) => {
                let (error, close_owner) = failure.into_parts();
                if let Some(close_owner) = close_owner {
                    app.register_plugin_runtime_close_participant(
                        TASK_POOLS_SHUTDOWN_OBLIGATION,
                        TASK_POOLS_CLOSE_PARTICIPANT,
                        TaskPoolsCloseParticipant {
                            instance_token: None,
                            close_owner,
                        },
                    )?;
                }
                return Err(PluginError::SetupFailed {
                    plugin: TASK_PLUGIN_ID,
                    message: error.to_string(),
                });
            }
        };
        let instance_token = pools.ownership_token();
        let close_owner = pools
            .take_close_owner()
            .expect("new task pools retain their close owner");
        app.register_plugin_runtime_close_participant(
            TASK_POOLS_SHUTDOWN_OBLIGATION,
            TASK_POOLS_CLOSE_PARTICIPANT,
            TaskPoolsCloseParticipant {
                instance_token: Some(instance_token),
                close_owner,
            },
        )?;
        app.insert_resource(pools)?;
        Ok(())
    }
}

struct TaskPoolsCloseParticipant {
    instance_token: Option<Arc<TaskPoolInstanceToken>>,
    close_owner: TaskPoolsCloseOwner,
}

impl RuntimeCloseParticipant for TaskPoolsCloseParticipant {
    fn begin_close(
        &mut self,
        _context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        self.close_owner.begin_close();
        Ok(RuntimeCloseProgress::Pending)
    }

    fn poll_close(
        &mut self,
        context: &mut RuntimeCloseContext<'_>,
    ) -> Result<RuntimeCloseProgress, RuntimeCloseParticipantError> {
        let progress = self.close_owner.poll_close();
        context
            .insert_resource(self.close_owner.report())
            .map_err(|_| {
                RuntimeCloseParticipantError::terminal("nara.tasks.runtime-resource-access")
            })?;
        match progress {
            TaskPoolsCloseProgress::Pending => Ok(RuntimeCloseProgress::Pending),
            TaskPoolsCloseProgress::Incomplete => Err(RuntimeCloseParticipantError::new(
                "nara.tasks.close-deadline",
            )),
            TaskPoolsCloseProgress::Complete => {
                let owns_world_facade =
                    self.instance_token.as_ref().is_some_and(|instance_token| {
                        context
                            .world()
                            .get_resource::<TaskPools>()
                            .is_some_and(|pools| pools.has_ownership_token(instance_token))
                    });
                if owns_world_facade {
                    context.remove_resource::<TaskPools>().map_err(|_| {
                        RuntimeCloseParticipantError::terminal("nara.tasks.runtime-resource-access")
                    })?;
                }
                if self.close_owner.report().has_cleanup_failure() {
                    Ok(RuntimeCloseProgress::CompleteWithError {
                        code: "nara.tasks.cleanup-failed",
                    })
                } else {
                    Ok(RuntimeCloseProgress::Complete)
                }
            }
        }
    }
}
