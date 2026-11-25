use std::{
    collections::BTreeMap,
    fmt::{self, Debug, Formatter},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
    task::{Context, Waker},
    time::Duration,
};

use thiserror::Error;

use super::{Instant, TaskPoolKind, TaskPoolStats, drop_catching, lock};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(pub(super) u64);

impl TaskId {
    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskDomainKey(u64);

impl TaskDomainKey {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskCoalesceKey(u64);

impl TaskCoalesceKey {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

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
    pub const fn new(admission_tick: u64, domain_key: TaskDomainKey, task_id: TaskId) -> Self {
        Self {
            admission_tick,
            domain_key,
            task_id,
        }
    }

    pub const fn admission_tick(self) -> u64 {
        self.admission_tick
    }

    pub const fn domain_key(self) -> TaskDomainKey {
        self.domain_key
    }

    pub const fn task_id(self) -> TaskId {
        self.task_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TaskDescriptor {
    pub(super) kind: TaskPoolKind,
    pub(super) order_key: TaskOrderKey,
}

impl TaskDescriptor {
    pub const fn id(self) -> TaskId {
        self.order_key.task_id()
    }

    pub const fn kind(self) -> TaskPoolKind {
        self.kind
    }

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
    pub const fn new(admission_tick: u64, domain_key: TaskDomainKey) -> Self {
        Self {
            admission_tick,
            domain_key,
            overload: TaskOverloadPolicy::Reject,
        }
    }

    pub const fn with_overload(mut self, overload: TaskOverloadPolicy) -> Self {
        self.overload = overload;
        self
    }

    pub const fn admission_tick(self) -> u64 {
        self.admission_tick
    }

    pub const fn domain_key(self) -> TaskDomainKey {
        self.domain_key
    }

    pub const fn overload(self) -> TaskOverloadPolicy {
        self.overload
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskRejectReason {
    QueueFull { capacity: usize },
    PoolClosed,
    ScopeClosed,
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
    /// Capture disposal has completed before rejection returns.
    pub return_state: TaskReturnState,
}

/// Physical return of executor-owned work, independent of its first terminal result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskReturnState {
    Pending,
    Returned,
    CleanupFailed,
}

const RETURNED: u8 = 1;
const CLEANUP_FAILED: u8 = 2;

/// Observes the return of admitted work without retaining its output or facility.
#[derive(Debug, Clone, Default)]
pub struct TaskReturnObserver(Arc<AtomicU8>);

impl TaskReturnObserver {
    pub fn state(&self) -> TaskReturnState {
        let state = self.0.load(Ordering::Acquire);
        if state & RETURNED == 0 {
            TaskReturnState::Pending
        } else if state & CLEANUP_FAILED != 0 {
            TaskReturnState::CleanupFailed
        } else {
            TaskReturnState::Returned
        }
    }

    pub(super) fn fail_cleanup(&self) {
        let _ = self
            .0
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |state| {
                (state & RETURNED == 0).then_some(state | CLEANUP_FAILED)
            });
    }

    pub(super) fn finish(&self) -> bool {
        self.0.fetch_or(RETURNED, Ordering::AcqRel) & CLEANUP_FAILED != 0
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
            _ => None,
        }
    }
}

/// A cancellation signal for owned work. A synchronous poll can check it at safe boundaries.
#[derive(Clone, Default)]
pub struct TaskCancellationToken(Arc<Cancellation>);

#[derive(Default)]
struct Cancellation {
    cancelled: AtomicBool,
    waker: Mutex<Option<Waker>>,
}

impl TaskCancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::Acquire)
    }

    fn mark_cancelled(&self) {
        self.0.cancelled.store(true, Ordering::Release);
        let waker = lock(&self.0.waker).take();
        if let Some(waker) = waker {
            waker.wake();
        }
    }

    pub(super) fn register(&self, context: &Context<'_>) -> bool {
        let mut waker = lock(&self.0.waker);
        if self.is_cancelled() {
            return false;
        }
        if waker
            .as_ref()
            .is_none_or(|waker| !waker.will_wake(context.waker()))
        {
            *waker = Some(context.waker().clone());
        }
        true
    }

    pub(super) fn clear_waker(&self) {
        lock(&self.0.waker).take();
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

#[derive(Debug, Default)]
pub(super) struct Activity {
    pub(super) publication: Mutex<u64>,
    pub(super) closed: AtomicBool,
    pub(super) pending: std::sync::atomic::AtomicUsize,
    pub(super) cleanup_failed: AtomicBool,
}

impl Activity {
    pub(super) fn cutoff(self: &Arc<Self>) -> TaskCompletionCutoff {
        TaskCompletionCutoff {
            activity: self.clone(),
            sequence: *lock(&self.publication),
        }
    }
}

struct Published<T> {
    terminal: Option<TaskTerminal<T>>,
    sequence: Option<u64>,
    admission_to_start: Option<Duration>,
}

pub(super) struct TaskState<T> {
    published: Mutex<Published<T>>,
    pub(super) token: TaskCancellationToken,
    pub(super) returned: TaskReturnObserver,
    pub(super) activity: Arc<Activity>,
    counters: Arc<Mutex<TaskPoolStats>>,
}

impl<T> TaskState<T> {
    pub(super) fn new(activity: Arc<Activity>, counters: Arc<Mutex<TaskPoolStats>>) -> Self {
        Self {
            published: Mutex::new(Published {
                terminal: None,
                sequence: None,
                admission_to_start: None,
            }),
            token: TaskCancellationToken::new(),
            returned: TaskReturnObserver::default(),
            activity,
            counters,
        }
    }

    pub(super) fn complete(&self, value: T) {
        if let Err(terminal) = self.publish(|_| TaskTerminal::Completed(value))
            && !drop_catching(terminal)
        {
            self.returned.fail_cleanup();
        }
    }

    fn publish(
        &self,
        terminal: impl FnOnce(bool) -> TaskTerminal<T>,
    ) -> Result<(), TaskTerminal<T>> {
        let mut sequence = lock(&self.activity.publication);
        let mut current = lock(&self.published);
        let before_start = current.admission_to_start.is_none();
        let terminal = terminal(before_start);
        if current.sequence.is_some() {
            return Err(terminal);
        }
        let mut counters = lock(&self.counters);
        let counter = match &terminal {
            TaskTerminal::Completed(_) => &mut counters.completed,
            TaskTerminal::Failed(_) => &mut counters.failed,
            TaskTerminal::Cancelled(_) => {
                self.token.mark_cancelled();
                &mut counters.cancelled
            }
        };
        *counter = counter.saturating_add(1);
        *sequence = sequence.saturating_add(1);
        current.sequence = Some(*sequence);
        current.terminal = Some(terminal);
        Ok(())
    }

    fn cancel(&self, reason: TaskCancellationReason) -> bool {
        self.publish(|before_start| {
            TaskTerminal::Cancelled(TaskCancellation {
                reason,
                before_start,
            })
        })
        .is_ok()
    }
}

impl<T> Drop for TaskState<T> {
    fn drop(&mut self) {
        let current = self
            .published
            .get_mut()
            .unwrap_or_else(|err| err.into_inner());
        if !drop_catching(current.terminal.take()) {
            self.returned.fail_cleanup();
        }
    }
}

pub(super) trait TaskControl: Send + Sync {
    fn start(&self, admitted_at: Instant) -> bool;
    fn cancel(&self, reason: TaskCancellationReason) -> bool;
    fn fail(&self);
}

impl<T: Send + 'static> TaskControl for TaskState<T> {
    fn start(&self, admitted_at: Instant) -> bool {
        let mut current = lock(&self.published);
        if current.sequence.is_some() {
            return false;
        }
        current.admission_to_start = Some(admitted_at.elapsed());
        let mut counters = lock(&self.counters);
        counters.started = counters.started.saturating_add(1);
        true
    }

    fn cancel(&self, reason: TaskCancellationReason) -> bool {
        self.cancel(reason)
    }

    fn fail(&self) {
        let _ = self.publish(|_| TaskTerminal::Failed(TaskFailure::Panicked));
    }
}

/// A single-consumer result handle. Dropping it leaves the admitted work owned by its facility.
pub struct TaskHandle<T> {
    pub(super) descriptor: TaskDescriptor,
    pub(super) state: Arc<TaskState<T>>,
}

impl<T> TaskHandle<T> {
    pub const fn id(&self) -> TaskId {
        self.descriptor.id()
    }

    pub const fn kind(&self) -> TaskPoolKind {
        self.descriptor.kind()
    }

    pub const fn descriptor(&self) -> TaskDescriptor {
        self.descriptor
    }

    pub const fn admission_tick(&self) -> u64 {
        self.order_key().admission_tick()
    }

    pub const fn domain_key(&self) -> TaskDomainKey {
        self.order_key().domain_key()
    }

    pub const fn order_key(&self) -> TaskOrderKey {
        self.descriptor.order_key()
    }

    pub fn cancellation_token(&self) -> TaskCancellationToken {
        self.state.token.clone()
    }

    pub fn return_observer(&self) -> TaskReturnObserver {
        self.state.returned.clone()
    }

    pub fn admission_to_start(&self) -> Option<Duration> {
        lock(&self.state.published).admission_to_start
    }

    /// Publishes cancellation once. A suspended future is woken for cooperative disposal.
    pub fn cancel(&self) -> bool {
        self.state.cancel(TaskCancellationReason::Requested)
    }

    pub fn is_finished(&self) -> bool {
        lock(&self.state.published).sequence.is_some()
    }

    pub fn try_take(&mut self) -> Option<TaskTerminal<T>> {
        let terminal = lock(&self.state.published).terminal.take()?;
        let mut counters = lock(&self.state.counters);
        counters.taken = counters.taken.saturating_add(1);
        Some(terminal)
    }
}

impl<T> Debug for TaskHandle<T> {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TaskHandle")
            .field("descriptor", &self.descriptor)
            .field("finished", &self.is_finished())
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

/// Publishes the ready prefix while keeping later completions and other activities separate.
pub struct OrderedTaskResults<T> {
    pending: BTreeMap<TaskOrderKey, TaskHandle<T>>,
}

#[derive(Debug, Clone)]
pub struct TaskCompletionCutoff {
    activity: Arc<Activity>,
    sequence: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TaskCompletionCutoffError {
    #[error("ordered task results contain a task from a different completion owner")]
    PoolMismatch { task: TaskDescriptor },
}

#[derive(Debug)]
pub struct OrderedTaskReadySnapshot<T> {
    handles: Vec<(TaskOrderKey, TaskHandle<T>)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
#[error("a captured task terminal was unavailable")]
pub struct TaskReadySnapshotError {
    task: TaskDescriptor,
}

impl TaskReadySnapshotError {
    pub const fn task(self) -> TaskDescriptor {
        self.task
    }
}

impl<T> OrderedTaskReadySnapshot<T> {
    pub fn into_terminals(self) -> Result<Vec<OrderedTaskTerminal<T>>, TaskReadySnapshotError> {
        self.handles
            .into_iter()
            .map(|(order_key, mut handle)| {
                let terminal = handle.try_take().ok_or(TaskReadySnapshotError {
                    task: handle.descriptor(),
                })?;
                Ok(OrderedTaskTerminal {
                    order_key,
                    terminal,
                })
            })
            .collect()
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
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    pub fn push(&mut self, handle: TaskHandle<T>) -> Result<(), TaskHandle<T>> {
        if self
            .pending
            .first_key_value()
            .is_some_and(|(_, first)| !Arc::ptr_eq(&first.state.activity, &handle.state.activity))
            || self.pending.contains_key(&handle.order_key())
        {
            return Err(handle);
        }
        let spent = {
            let published = lock(&handle.state.published);
            published.sequence.is_some() && published.terminal.is_none()
        };
        if spent {
            return Err(handle);
        }
        self.pending.insert(handle.order_key(), handle);
        Ok(())
    }

    pub fn validate_completion_cutoff(
        &self,
        cutoff: &TaskCompletionCutoff,
    ) -> Result<(), TaskCompletionCutoffError> {
        if let Some((_, handle)) = self.pending.first_key_value()
            && !Arc::ptr_eq(&handle.state.activity, &cutoff.activity)
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
        let mut handles = Vec::new();
        while self.pending.first_key_value().is_some_and(|(_, handle)| {
            lock(&handle.state.published)
                .sequence
                .is_some_and(|sequence| sequence <= cutoff.sequence)
        }) {
            handles.extend(self.pending.pop_first());
        }
        Ok(OrderedTaskReadySnapshot { handles })
    }
}
