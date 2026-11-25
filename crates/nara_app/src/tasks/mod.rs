//! Bevy task execution with bounded admission and explicit runtime ownership.
//!
//! The raw Bevy pool types use Bevy's process-global initialization. Use [`TaskPools`] for
//! work whose admission, completion and shutdown belong to a Nara runtime or host facility.

use std::{
    collections::{BTreeMap, VecDeque},
    fmt::{self, Debug, Formatter},
    future::{Future, poll_fn},
    panic::{AssertUnwindSafe, catch_unwind},
    pin::Pin,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicU64, Ordering},
    },
    task::Poll,
    time::Duration,
};

use bevy_platform::time::Instant;
use bevy_tasks::futures_lite::FutureExt;
use nara_ecs::Resource;

mod backend;
mod close;
mod config;
mod plugin;
mod result;

pub use bevy_tasks::{AsyncComputeTaskPool, ComputeTaskPool, IoTaskPool, Task};
pub use close::{TaskPoolShutdownReport, TaskShutdownError, TaskShutdownReport};
pub use config::*;
pub use plugin::*;
pub use result::*;

use backend::{PoolBackend, build_pool, retirement_pool};
use close::{ClosePhase, PhysicalOwner};
use result::{Activity, TaskControl, TaskState};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TaskPoolStats {
    pub admitted: u64,
    pub rejected: u64,
    pub coalesced: u64,
    pub queued: usize,
    pub running: usize,
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

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TaskStats([TaskPoolStats; 3]);

impl TaskStats {
    pub fn for_kind(&self, kind: TaskPoolKind) -> TaskPoolStats {
        self.0[kind.index()]
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TaskInlineRunReport {
    pub executed: usize,
    pub cancelled_before_start: usize,
}

/// Bounded access to owned I/O, compute and background compute work.
///
/// A runtime or host retains the physical owner separately from its spawner. On the web, Bevy
/// polls work cooperatively on the event loop. Split expensive work into short polls there.
#[derive(Resource)]
pub struct TaskPools {
    facility: Arc<Facility>,
    activity: Arc<Activity>,
    scoped: bool,
    owner: Option<PhysicalOwner>,
}

impl Debug for TaskPools {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskPools")
            .field("config", self.config())
            .field("scoped", &self.scoped)
            .finish_non_exhaustive()
    }
}

impl TaskPools {
    pub fn try_new(config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        let pools = Self::try_new_lazy(config)?;
        for kind in TaskPoolKind::ALL {
            let backend = build_pool(kind, config.kind(kind).workers().get())?;
            lock(&pools.facility.state).kinds[kind.index()].install(backend);
        }
        Ok(pools)
    }

    pub fn try_new_lazy(config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        Self::new(config, false)
    }

    /// Creates a manually driven pool for deterministic synchronous fixtures.
    pub fn inline_for_tests(config: TaskPoolConfig) -> Result<Self, TaskPoolError> {
        Self::new(config, true)
    }

    fn new(config: TaskPoolConfig, inline: bool) -> Result<Self, TaskPoolError> {
        let facility = Arc::new(Facility {
            config,
            inline,
            next_id: AtomicU64::new(1),
            state: Mutex::default(),
            activation: std::array::from_fn(|_| Mutex::new(())),
            retirement: retirement_pool()?,
        });
        Ok(Self {
            owner: Some(PhysicalOwner(facility.clone())),
            facility,
            activity: Arc::default(),
            scoped: false,
        })
    }

    pub fn config(&self) -> &TaskPoolConfig {
        &self.facility.config
    }

    pub fn is_threaded(&self, _kind: TaskPoolKind) -> bool {
        !cfg!(target_arch = "wasm32") && !self.facility.inline
    }

    pub fn stats(&self) -> TaskStats {
        let state = lock(&self.facility.state);
        TaskStats(std::array::from_fn(|index| state.kinds[index].stats()))
    }

    /// Observes physical return without closing admission. Published outputs retain their owners.
    pub fn return_state(&self) -> TaskReturnState {
        let state = lock(&self.facility.state);
        let pending = if self.scoped {
            self.activity.pending.load(Ordering::Relaxed) != 0
        } else {
            state.kinds.iter().any(KindState::has_work)
        };
        if pending {
            TaskReturnState::Pending
        } else if self.cleanup_failed(&state) {
            TaskReturnState::CleanupFailed
        } else {
            TaskReturnState::Returned
        }
    }

    pub fn has_cleanup_failure(&self) -> bool {
        self.cleanup_failed(&lock(&self.facility.state))
    }

    fn cleanup_failed(&self, state: &FacilityState) -> bool {
        let worker_failed = state
            .kinds
            .iter()
            .any(|kind| kind.report.panicked_workers != 0);
        worker_failed
            || if self.scoped {
                self.activity.cleanup_failed.load(Ordering::Relaxed)
            } else {
                state.kinds.iter().any(|kind| kind.report.cleanup_failed)
            }
    }

    pub fn capture_completion_cutoff(&self) -> TaskCompletionCutoff {
        self.activity.cutoff()
    }

    /// Submits an owned synchronous operation through the same admission policy as a future.
    pub fn spawn<T: Send + 'static>(
        &self,
        kind: TaskPoolKind,
        request: TaskSpawnRequest,
        operation: impl FnOnce(TaskCancellationToken) -> T + Send + 'static,
    ) -> TaskSpawnOutcome<T> {
        self.spawn_future(kind, request, move |token| async move { operation(token) })
    }

    /// Submits a future factory. The facility invokes the factory on its executor after admission.
    /// A suspended future lets other admitted work start. The pool still bounds outstanding work.
    pub fn spawn_future<T, F>(
        &self,
        kind: TaskPoolKind,
        request: TaskSpawnRequest,
        operation: impl FnOnce(TaskCancellationToken) -> F + Send + 'static,
    ) -> TaskSpawnOutcome<T>
    where
        T: Send + 'static,
        F: Future<Output = T> + Send + 'static,
    {
        let counters = lock(&self.facility.state).kinds[kind.index()]
            .counters
            .clone();
        let mut rejection = TaskRejection {
            task: None,
            kind,
            admission_tick: request.admission_tick(),
            domain_key: request.domain_key(),
            reason: TaskRejectReason::TaskIdExhausted,
            return_state: TaskReturnState::Returned,
        };
        let Ok(next_id) =
            self.facility
                .next_id
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |next| {
                    next.checked_add(1)
                })
        else {
            if !drop_catching(operation) {
                rejection.return_state = TaskReturnState::CleanupFailed;
            }
            lock(&counters).rejected += 1;
            return TaskSpawnOutcome::Rejected(rejection);
        };

        let descriptor = TaskDescriptor {
            kind,
            order_key: TaskOrderKey::new(
                request.admission_tick(),
                request.domain_key(),
                TaskId(next_id),
            ),
        };
        let state = Arc::new(TaskState::new(self.activity.clone(), counters.clone()));
        let handle = TaskHandle {
            descriptor,
            state: state.clone(),
        };
        let token = state.token.clone();
        let work_state = state.clone();
        let job = Job {
            descriptor,
            overload: request.overload(),
            admitted_at: Instant::now(),
            activity: self.activity.clone(),
            control: state.clone(),
            returned: state.returned.clone(),
            token,
            work: Box::pin(async move {
                let value = operation(work_state.token.clone()).await;
                work_state.complete(value);
            }),
        };
        match self.facility.admit(job) {
            Ok(Some(replaced)) => TaskSpawnOutcome::Coalesced { handle, replaced },
            Ok(None) => TaskSpawnOutcome::Accepted(handle),
            Err(reason) => {
                rejection.task = Some(descriptor);
                rejection.reason = reason;
                rejection.return_state = state.returned.state();
                lock(&counters).rejected += 1;
                TaskSpawnOutcome::Rejected(rejection)
            }
        }
    }

    pub fn run_pending_for_tests(&self) -> TaskInlineRunReport {
        let mut report = TaskInlineRunReport::default();
        if !self.facility.inline {
            return report;
        }
        loop {
            let mut progressed = false;
            for kind in TaskPoolKind::ALL {
                let next = {
                    let mut state = lock(&self.facility.state);
                    state.kinds[kind.index()].take_next()
                };
                match next {
                    Some((job, true)) => {
                        bevy_tasks::block_on(self.facility.clone().run(job));
                        report.executed += 1;
                    }
                    Some((job, false)) => {
                        self.facility.dispose(job);
                        report.cancelled_before_start += 1;
                    }
                    None => continue,
                }
                progressed = true;
            }
            if !progressed {
                return report;
            }
        }
    }

    pub fn poll_shutdown(&self) -> Result<TaskShutdownReport, TaskShutdownError> {
        let owner = self
            .owner
            .as_ref()
            .ok_or(TaskShutdownError::CloseOwnerTransferred)?;
        Ok(owner.poll().0)
    }

    /// Drives the native shutdown deadlines. On the web this performs a single nonblocking poll.
    pub fn shutdown_blocking(&self) -> Result<TaskShutdownReport, TaskShutdownError> {
        let owner = self
            .owner
            .as_ref()
            .ok_or(TaskShutdownError::CloseOwnerTransferred)?;
        #[cfg(target_arch = "wasm32")]
        return Ok(owner.poll().0);
        #[cfg(not(target_arch = "wasm32"))]
        loop {
            let (report, timed_out) = owner.poll();
            if report.is_complete() || timed_out {
                return Ok(report);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    fn facade(&self, activity: Arc<Activity>, scoped: bool) -> Self {
        Self {
            facility: self.facility.clone(),
            activity,
            scoped,
            owner: None,
        }
    }
}

struct Job {
    descriptor: TaskDescriptor,
    overload: TaskOverloadPolicy,
    admitted_at: Instant,
    activity: Arc<Activity>,
    control: Arc<dyn TaskControl>,
    returned: TaskReturnObserver,
    token: TaskCancellationToken,
    work: Pin<Box<dyn Future<Output = ()> + Send>>,
}

struct RunningJob {
    activity: Arc<Activity>,
    started_at: Instant,
    control: Option<Arc<dyn TaskControl>>,
    has_slot: bool,
}

#[derive(Default)]
struct KindState {
    pending: VecDeque<Job>,
    running: BTreeMap<TaskId, RunningJob>,
    occupied_slots: usize,
    disposing: usize,
    counters: Arc<Mutex<TaskPoolStats>>,
    backend: Option<PoolBackend>,
    activating: bool,
    startup_failed: bool,
    joining: bool,
    live_workers: usize,
    phase: ClosePhase,
    report: TaskPoolShutdownReport,
}

impl KindState {
    fn install(&mut self, backend: PoolBackend) {
        self.live_workers = backend.workers();
        self.backend = Some(backend);
    }

    fn has_work(&self) -> bool {
        !self.pending.is_empty() || !self.running.is_empty() || self.disposing != 0
    }

    fn stats(&self) -> TaskPoolStats {
        let mut queued = 0;
        let oldest_queued = self
            .pending
            .iter()
            .filter(|job| !job.token.is_cancelled())
            .map(|job| {
                queued += 1;
                job.admitted_at
            })
            .min();
        let oldest_running = self.running.values().map(|job| job.started_at).min();
        TaskPoolStats {
            queued,
            running: self.running.len(),
            live_workers: self.live_workers,
            oldest_queued_age: oldest_queued.map(|admitted| admitted.elapsed()),
            oldest_running_age: oldest_running.map(|started| started.elapsed()),
            ..*lock(&self.counters)
        }
    }

    fn take_next(&mut self) -> Option<(Job, bool)> {
        let job = self.pending.pop_front()?;
        let started = job.control.start(job.admitted_at);
        if started {
            self.occupied_slots += 1;
            self.running.insert(
                job.descriptor.id(),
                RunningJob {
                    activity: job.activity.clone(),
                    started_at: Instant::now(),
                    control: Some(job.control.clone()),
                    has_slot: true,
                },
            );
        } else {
            self.disposing += 1;
        }
        Some((job, started))
    }

    fn extract(&mut self, mut remove: impl FnMut(&Job) -> bool) -> Vec<Job> {
        let mut removed = Vec::new();
        for job in std::mem::take(&mut self.pending) {
            if remove(&job) {
                removed.push(job);
            } else {
                self.pending.push_back(job);
            }
        }
        self.disposing += removed.len();
        removed
    }
}

#[derive(Default)]
struct FacilityState {
    closed: bool,
    kinds: [KindState; 3],
}

struct Facility {
    config: TaskPoolConfig,
    inline: bool,
    next_id: AtomicU64,
    state: Mutex<FacilityState>,
    activation: [Mutex<()>; 3],
    retirement: &'static PoolBackend,
}

impl Facility {
    fn activate(
        self: &Arc<Self>,
        kind: TaskPoolKind,
        activity: &Activity,
    ) -> Result<(), TaskRejectReason> {
        let index = kind.index();
        if self.inline || lock(&self.state).kinds[index].backend.is_some() {
            return Ok(());
        }
        let _activation = lock(&self.activation[index]);
        {
            let mut state = lock(&self.state);
            state.can_admit(activity)?;
            let pool = &mut state.kinds[index];
            if pool.backend.is_some() {
                return Ok(());
            }
            if pool.startup_failed {
                return Err(TaskRejectReason::WorkerStartupFailed);
            }
            pool.activating = true;
        }
        let result = build_pool(kind, self.config.kind(kind).workers().get());
        let mut state = lock(&self.state);
        let pool = &mut state.kinds[index];
        pool.activating = false;
        match result {
            Ok(backend) => {
                pool.install(backend);
                self.join_if_idle(kind, pool);
                Ok(())
            }
            Err(_) => {
                pool.startup_failed = true;
                self.join_if_idle(kind, pool);
                Err(TaskRejectReason::WorkerStartupFailed)
            }
        }
    }

    fn admit(self: &Arc<Self>, job: Job) -> Result<Option<TaskId>, TaskRejectReason> {
        let kind = job.descriptor.kind();
        let mut job = Some(job);
        let mut disposed = Vec::new();
        let admission = (|| {
            let pending = job.as_ref().expect("admission still owns the job");
            self.activate(kind, &pending.activity)?;
            let mut state = lock(&self.state);
            state.can_admit(&pending.activity)?;
            let pool = &mut state.kinds[kind.index()];
            let config = self.config.kind(kind);
            let capacity = config.pending().get();
            let full = |pool: &KindState| {
                pool.pending.len() >= capacity
                    || pool.pending.len() + pool.running.len() >= capacity + config.workers().get()
            };
            if full(pool) {
                disposed.extend(pool.extract(|queued| queued.token.is_cancelled()));
            }
            let replacement = match pending.overload {
                TaskOverloadPolicy::Reject => None,
                TaskOverloadPolicy::CoalescePending(key) => {
                    pool.pending.iter().position(|queued| {
                        Arc::ptr_eq(&pending.activity, &queued.activity)
                            && pending.descriptor.order_key().domain_key()
                                == queued.descriptor.order_key().domain_key()
                            && queued.overload == TaskOverloadPolicy::CoalescePending(key)
                            && !queued.token.is_cancelled()
                    })
                }
            };
            let mut replaced = None;
            if let Some(index) = replacement
                && let Some(previous) = pool.pending.remove(index)
            {
                if previous.control.cancel(TaskCancellationReason::Coalesced {
                    replacement: pending.descriptor.id(),
                }) {
                    replaced = Some((index, previous.descriptor.id()));
                }
                disposed.push(previous);
                pool.disposing += 1;
            }
            if full(pool) {
                return Err(TaskRejectReason::QueueFull { capacity });
            }
            let mut pending = job.take().expect("admission still owns the job");
            pending.admitted_at = Instant::now();
            pending.activity.pending.fetch_add(1, Ordering::Relaxed);
            pool.pending.insert(
                replaced.map_or(pool.pending.len(), |(index, _)| index),
                pending,
            );
            {
                let mut counters = lock(&pool.counters);
                counters.admitted += 1;
                counters.coalesced += u64::from(replaced.is_some());
            }
            self.dispatch(kind, pool);
            Ok(replaced.map(|(_, id)| id))
        })();
        for discarded in disposed {
            self.dispose(discarded);
        }
        if let Some(rejected) = job {
            let observer = rejected.returned.clone();
            if !drop_catching(rejected) {
                observer.fail_cleanup();
            }
            observer.finish();
        }
        admission
    }

    fn dispatch(self: &Arc<Self>, kind: TaskPoolKind, pool: &mut KindState) {
        if pool.backend.is_none() {
            return;
        }
        let concurrency = self.config.kind(kind).workers().get();
        while pool.occupied_slots < concurrency {
            match pool.take_next() {
                Some((job, true)) => {
                    if let Some(backend) = &pool.backend {
                        backend.spawn(self.clone().run(job));
                    }
                }
                Some((job, false)) => self.dispose_later(vec![job]),
                None => break,
            }
        }
    }

    async fn run(self: Arc<Self>, mut job: Job) {
        let kind = job.descriptor.kind();
        let id = job.descriptor.id();
        let mut yielded = false;
        let outcome = AssertUnwindSafe(poll_fn(|context| {
            if job.token.register(context) {
                let poll = job.work.as_mut().poll(context);
                if poll.is_pending() && !yielded {
                    yielded = true;
                    let mut state = lock(&self.state);
                    let pool = &mut state.kinds[kind.index()];
                    if let Some(running) = pool.running.get_mut(&id) {
                        running.has_slot = false;
                        pool.occupied_slots -= 1;
                        self.dispatch(kind, pool);
                    }
                }
                poll
            } else {
                Poll::Ready(())
            }
        }))
        .catch_unwind()
        .await;
        if let Err(payload) = outcome {
            job.returned.fail_cleanup();
            job.control.fail();
            drop_catching(payload);
        }
        job.token.clear_waker();
        let control = lock(&self.state).kinds[kind.index()]
            .running
            .get_mut(&id)
            .and_then(|running| running.control.take());
        drop(control);
        self.finish(job, true);
    }

    fn dispose(self: &Arc<Self>, job: Job) {
        self.finish(job, false);
    }

    fn dispose_later(self: &Arc<Self>, jobs: Vec<Job>) {
        if jobs.is_empty() {
            return;
        }
        let facility = self.clone();
        self.retirement.spawn(async move {
            for job in jobs {
                facility.dispose(job);
            }
        });
    }

    fn finish(self: &Arc<Self>, job: Job, ran: bool) {
        let kind = job.descriptor.kind();
        let id = job.descriptor.id();
        let activity = job.activity.clone();
        let observer = job.returned.clone();
        if !drop_catching(job) {
            observer.fail_cleanup();
        }
        let mut state = lock(&self.state);
        let pool = &mut state.kinds[kind.index()];
        if observer.finish() {
            activity.cleanup_failed.store(true, Ordering::Relaxed);
            pool.report.cleanup_failed = true;
        }
        activity.pending.fetch_sub(1, Ordering::Relaxed);
        if ran {
            if pool
                .running
                .remove(&id)
                .is_some_and(|running| running.has_slot)
            {
                pool.occupied_slots -= 1;
            }
            self.dispatch(kind, pool);
        } else {
            pool.disposing -= 1;
        }
        self.join_if_idle(kind, pool);
    }
}

impl FacilityState {
    fn can_admit(&self, activity: &Activity) -> Result<(), TaskRejectReason> {
        if activity.closed.load(Ordering::Relaxed) {
            Err(TaskRejectReason::ScopeClosed)
        } else if self.closed {
            Err(TaskRejectReason::PoolClosed)
        } else {
            Ok(())
        }
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn drop_catching(value: impl Sized) -> bool {
    match catch_unwind(AssertUnwindSafe(|| drop(value))) {
        Ok(()) => true,
        Err(payload) => {
            let _ = catch_unwind(AssertUnwindSafe(|| drop(payload)));
            false
        }
    }
}

#[cfg(test)]
mod tests;
