//! Engine-owned task execution for nara.

use std::{
    collections::BTreeMap,
    error::Error,
    fmt::{self, Debug, Display, Formatter},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{self, Sender},
    },
    thread::{self, JoinHandle},
};

use nara_app::{App, CoreStage, Plugin, PluginError, TaskUpdateSet};
use nara_ecs::{Resource, schedule::IntoScheduleConfigs};

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
pub enum TaskExecutionMode {
    Deterministic,
    Threaded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPoolConfig {
    execution_mode: TaskExecutionMode,
    io_threads: usize,
    compute_threads: usize,
    async_compute_threads: usize,
}

impl Default for TaskPoolConfig {
    fn default() -> Self {
        let parallelism = thread::available_parallelism().map_or(1, usize::from);
        Self {
            execution_mode: TaskExecutionMode::Threaded,
            io_threads: 2,
            compute_threads: parallelism.max(1),
            async_compute_threads: (parallelism / 2).max(1),
        }
    }
}

impl TaskPoolConfig {
    #[must_use]
    pub const fn deterministic() -> Self {
        Self {
            execution_mode: TaskExecutionMode::Deterministic,
            io_threads: 0,
            compute_threads: 0,
            async_compute_threads: 0,
        }
    }

    #[must_use]
    pub fn threaded(
        io_threads: usize,
        compute_threads: usize,
        async_compute_threads: usize,
    ) -> Self {
        Self {
            execution_mode: TaskExecutionMode::Threaded,
            io_threads: io_threads.max(1),
            compute_threads: compute_threads.max(1),
            async_compute_threads: async_compute_threads.max(1),
        }
    }

    #[must_use]
    pub const fn execution_mode(&self) -> TaskExecutionMode {
        self.execution_mode
    }

    #[must_use]
    pub const fn threads_for(&self, kind: TaskPoolKind) -> usize {
        match kind {
            TaskPoolKind::Io => self.io_threads,
            TaskPoolKind::Compute => self.compute_threads,
            TaskPoolKind::AsyncCompute => self.async_compute_threads,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TaskPoolError {
    WorkerSpawnFailed { kind: TaskPoolKind, message: String },
}

impl Display for TaskPoolError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
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

#[derive(Clone, Default)]
pub struct TaskCancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl TaskCancellationToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskResultState {
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskResult<T> {
    state: TaskResultState,
    value: T,
}

impl<T> TaskResult<T> {
    #[must_use]
    pub const fn completed(value: T) -> Self {
        Self {
            state: TaskResultState::Completed,
            value,
        }
    }

    #[must_use]
    pub const fn cancelled(value: T) -> Self {
        Self {
            state: TaskResultState::Cancelled,
            value,
        }
    }

    #[must_use]
    pub const fn state(&self) -> TaskResultState {
        self.state
    }

    #[must_use]
    pub const fn is_cancelled(&self) -> bool {
        matches!(self.state, TaskResultState::Cancelled)
    }

    #[must_use]
    pub const fn value(&self) -> &T {
        &self.value
    }

    pub fn into_value(self) -> T {
        self.value
    }
}

struct TaskState<T> {
    result: Mutex<Option<T>>,
    finished: AtomicBool,
}

impl<T> Default for TaskState<T> {
    fn default() -> Self {
        Self {
            result: Mutex::new(None),
            finished: AtomicBool::new(false),
        }
    }
}

pub struct TaskHandle<T> {
    id: TaskId,
    kind: TaskPoolKind,
    token: TaskCancellationToken,
    state: Arc<TaskState<T>>,
    stats: Arc<Mutex<TaskStats>>,
}

impl<T> TaskHandle<T> {
    #[must_use]
    pub const fn id(&self) -> TaskId {
        self.id
    }

    #[must_use]
    pub const fn kind(&self) -> TaskPoolKind {
        self.kind
    }

    #[must_use]
    pub fn cancellation_token(&self) -> TaskCancellationToken {
        self.token.clone()
    }

    pub fn cancel(&self) {
        self.token.cancel();
    }

    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.state.finished.load(Ordering::Acquire)
    }

    pub fn try_take(&mut self) -> Option<TaskResult<T>> {
        let value = self.state.result.lock().ok()?.take()?;
        let cancelled = self.token.is_cancelled();
        record_taken(&self.stats, self.kind, cancelled);
        if cancelled {
            Some(TaskResult::cancelled(value))
        } else {
            Some(TaskResult::completed(value))
        }
    }
}

impl<T> Debug for TaskHandle<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskHandle")
            .field("id", &self.id)
            .field("kind", &self.kind)
            .field("finished", &self.is_finished())
            .field("cancelled", &self.token.is_cancelled())
            .finish()
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TaskPoolStats {
    pub spawned: u64,
    pub completed: u64,
    pub taken: u64,
    pub cancelled_taken: u64,
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

    fn record_spawned(&mut self, kind: TaskPoolKind) {
        self.per_kind.entry(kind).or_default().spawned += 1;
    }

    fn record_completed(&mut self, kind: TaskPoolKind) {
        self.per_kind.entry(kind).or_default().completed += 1;
    }

    fn record_taken(&mut self, kind: TaskPoolKind, cancelled: bool) {
        let stats = self.per_kind.entry(kind).or_default();
        stats.taken += 1;
        if cancelled {
            stats.cancelled_taken += 1;
        }
    }
}

#[derive(Resource)]
pub struct TaskPools {
    config: TaskPoolConfig,
    next_id: AtomicU64,
    io: TaskPoolExecutor,
    compute: TaskPoolExecutor,
    async_compute: TaskPoolExecutor,
    stats: Arc<Mutex<TaskStats>>,
}

impl TaskPools {
    pub fn try_new(config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        let io = TaskPoolExecutor::new(TaskPoolKind::Io, &config)?;
        let compute = TaskPoolExecutor::new(TaskPoolKind::Compute, &config)?;
        let async_compute = TaskPoolExecutor::new(TaskPoolKind::AsyncCompute, &config)?;
        Ok(Self {
            config,
            next_id: AtomicU64::new(1),
            io,
            compute,
            async_compute,
            stats: Arc::new(Mutex::new(TaskStats::default())),
        })
    }

    #[must_use]
    pub fn deterministic() -> Self {
        Self::try_new(TaskPoolConfig::deterministic())
            .expect("deterministic task pools do not spawn workers")
    }

    #[must_use]
    pub const fn config(&self) -> &TaskPoolConfig {
        &self.config
    }

    #[must_use]
    pub fn stats(&self) -> TaskStats {
        self.stats
            .lock()
            .map_or_else(|_| TaskStats::default(), |stats| stats.clone())
    }

    pub fn spawn<T, F>(&self, kind: TaskPoolKind, function: F) -> TaskHandle<T>
    where
        T: Send + 'static,
        F: FnOnce(TaskCancellationToken) -> T + Send + 'static,
    {
        let id = TaskId(self.next_id.fetch_add(1, Ordering::Relaxed));
        let token = TaskCancellationToken::new();
        let state = Arc::<TaskState<T>>::default();
        record_spawned(&self.stats, kind);

        let handle = TaskHandle {
            id,
            kind,
            token: token.clone(),
            state: state.clone(),
            stats: self.stats.clone(),
        };
        let stats = self.stats.clone();
        let job = move || {
            let value = function(token);
            if let Ok(mut result) = state.result.lock() {
                *result = Some(value);
                state.finished.store(true, Ordering::Release);
                record_completed(&stats, kind);
            }
        };

        self.executor(kind).dispatch(Box::new(job));
        handle
    }

    fn executor(&self, kind: TaskPoolKind) -> &TaskPoolExecutor {
        match kind {
            TaskPoolKind::Io => &self.io,
            TaskPoolKind::Compute => &self.compute,
            TaskPoolKind::AsyncCompute => &self.async_compute,
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

type Job = Box<dyn FnOnce() + Send + 'static>;

enum WorkerMessage {
    Run(Job),
    Shutdown,
}

enum TaskPoolExecutor {
    Inline,
    Threaded(WorkerPool),
}

impl TaskPoolExecutor {
    fn new(kind: TaskPoolKind, config: &TaskPoolConfig) -> Result<Self, TaskPoolError> {
        match config.execution_mode() {
            TaskExecutionMode::Deterministic => Ok(Self::Inline),
            TaskExecutionMode::Threaded => {
                WorkerPool::new(kind, config.threads_for(kind)).map(Self::Threaded)
            }
        }
    }

    fn dispatch(&self, job: Job) {
        match self {
            Self::Inline => job(),
            Self::Threaded(pool) => pool.dispatch(job),
        }
    }
}

struct WorkerPool {
    sender: Option<Sender<WorkerMessage>>,
    workers: Vec<JoinHandle<()>>,
}

impl WorkerPool {
    fn new(kind: TaskPoolKind, thread_count: usize) -> Result<Self, TaskPoolError> {
        let (sender, receiver) = mpsc::channel::<WorkerMessage>();
        let receiver = Arc::new(Mutex::new(receiver));
        let mut workers = Vec::with_capacity(thread_count);

        for index in 0..thread_count {
            let receiver = receiver.clone();
            let name = format!("nara-{}-{index}", kind.as_str());
            let worker = thread::Builder::new()
                .name(name)
                .spawn(move || {
                    loop {
                        let message = {
                            let Ok(receiver) = receiver.lock() else {
                                break;
                            };
                            receiver.recv()
                        };
                        match message {
                            Ok(WorkerMessage::Run(job)) => job(),
                            Ok(WorkerMessage::Shutdown) | Err(_) => break,
                        }
                    }
                })
                .map_err(|error| TaskPoolError::WorkerSpawnFailed {
                    kind,
                    message: error.to_string(),
                })?;
            workers.push(worker);
        }

        Ok(Self {
            sender: Some(sender),
            workers,
        })
    }

    fn dispatch(&self, job: Job) {
        let Some(sender) = &self.sender else {
            job();
            return;
        };

        if let Err(error) = sender.send(WorkerMessage::Run(job)) {
            match error.0 {
                WorkerMessage::Run(job) => job(),
                WorkerMessage::Shutdown => {}
            }
        }
    }
}

impl Drop for WorkerPool {
    fn drop(&mut self) {
        if let Some(sender) = self.sender.take() {
            for _ in &self.workers {
                let _ = sender.send(WorkerMessage::Shutdown);
            }
        }

        for worker in self.workers.drain(..) {
            let _ = worker.join();
        }
    }
}

#[derive(Debug, Clone)]
pub struct TaskPlugin {
    config: TaskPoolConfig,
}

impl Default for TaskPlugin {
    fn default() -> Self {
        Self {
            config: TaskPoolConfig::default(),
        }
    }
}

impl TaskPlugin {
    #[must_use]
    pub const fn new(config: TaskPoolConfig) -> Self {
        Self { config }
    }

    #[must_use]
    pub const fn deterministic() -> Self {
        Self::new(TaskPoolConfig::deterministic())
    }
}

impl Plugin for TaskPlugin {
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
        );

        if !app.world().contains_resource::<TaskPools>() {
            let pools = TaskPools::try_new(self.config.clone()).map_err(|error| {
                PluginError::SetupFailed {
                    plugin: self.name(),
                    message: error.to_string(),
                }
            })?;
            app.insert_resource(pools);
        }
        Ok(())
    }
}

fn record_spawned(stats: &Arc<Mutex<TaskStats>>, kind: TaskPoolKind) {
    if let Ok(mut stats) = stats.lock() {
        stats.record_spawned(kind);
    }
}

fn record_completed(stats: &Arc<Mutex<TaskStats>>, kind: TaskPoolKind) {
    if let Ok(mut stats) = stats.lock() {
        stats.record_completed(kind);
    }
}

fn record_taken(stats: &Arc<Mutex<TaskStats>>, kind: TaskPoolKind, cancelled: bool) {
    if let Ok(mut stats) = stats.lock() {
        stats.record_taken(kind, cancelled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    use nara_app::{App, CoreStage};
    use nara_ecs::{Res, ResMut, Resource};

    #[derive(Debug, Default, Resource)]
    struct AppliedResults(Vec<u32>);

    fn apply_task_result(mut pending: ResMut<PendingTask>, mut applied: ResMut<AppliedResults>) {
        if let Some(result) = pending.handle.try_take() {
            applied.0.push(result.into_value());
        }
    }

    #[derive(Resource)]
    struct PendingTask {
        handle: TaskHandle<u32>,
    }

    fn observe_update(applied: Res<AppliedResults>, mut observed: ResMut<ObservedInUpdate>) {
        observed.0 = applied.0.clone();
    }

    #[derive(Debug, Default, Resource)]
    struct ObservedInUpdate(Vec<u32>);

    #[test]
    fn deterministic_tasks_return_typed_result_through_handle() {
        let pools = TaskPools::deterministic();
        let mut handle = pools.spawn(TaskPoolKind::Compute, |_| 7_u32);

        assert!(handle.is_finished());
        let result = handle.try_take().unwrap();

        assert_eq!(result.state(), TaskResultState::Completed);
        assert_eq!(result.into_value(), 7);
        let stats = pools.stats().for_kind(TaskPoolKind::Compute);
        assert_eq!(stats.spawned, 1);
        assert_eq!(stats.completed, 1);
        assert_eq!(stats.taken, 1);
    }

    #[test]
    fn threaded_io_task_completes_without_sleeping_forever() {
        let pools = TaskPools::try_new(TaskPoolConfig::threaded(1, 1, 1)).unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        let mut handle = pools.spawn(TaskPoolKind::Io, |_| 42_u32);

        while !handle.is_finished() && Instant::now() < deadline {
            thread::yield_now();
        }

        assert_eq!(handle.try_take().unwrap().into_value(), 42);
    }

    #[test]
    fn cancellation_marks_taken_result_as_cancelled() {
        let pools = TaskPools::deterministic();
        let mut handle = pools.spawn(TaskPoolKind::AsyncCompute, |token| {
            assert!(!token.is_cancelled());
            11_u32
        });

        handle.cancel();
        let result = handle.try_take().unwrap();

        assert!(result.is_cancelled());
        assert_eq!(result.into_value(), 11);
        assert_eq!(
            pools
                .stats()
                .for_kind(TaskPoolKind::AsyncCompute)
                .cancelled_taken,
            1
        );
    }

    #[test]
    fn task_plugin_installs_stage_sets_and_resources() {
        let mut app = App::new();
        app.add_plugin(TaskPlugin::deterministic()).unwrap();
        let handle = app
            .world()
            .resource::<TaskPools>()
            .spawn(TaskPoolKind::Compute, |_| 5_u32);
        app.insert_resource(PendingTask { handle })
            .insert_resource(AppliedResults::default())
            .insert_resource(ObservedInUpdate::default())
            .add_systems(
                CoreStage::TaskUpdate,
                apply_task_result.in_set(TaskUpdateSet::ApplyAssetResults),
            )
            .add_systems(CoreStage::Update, observe_update);

        app.update();

        assert_eq!(app.world().resource::<ObservedInUpdate>().0, vec![5]);
    }
}
