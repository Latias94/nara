use std::sync::{Arc, atomic::Ordering};

use nara_ecs::Resource;
use thiserror::Error;

use super::{
    Activity, Facility, Instant, KindState, TaskCancellationReason, TaskPoolKind, drop_catching,
    lock,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum TaskShutdownError {
    #[error("this task facade does not own physical facility shutdown")]
    CloseOwnerTransferred,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct TaskPoolShutdownReport {
    pub cancelled_pending: usize,
    pub cancellation_requests: usize,
    pub joined_workers: usize,
    pub panicked_workers: usize,
    pub cleanup_failed: bool,
    pub drain_timed_out: bool,
    pub cancel_timed_out: bool,
    pub join_timed_out: bool,
}

impl TaskPoolShutdownReport {
    pub const fn timed_out(self) -> bool {
        self.drain_timed_out || self.cancel_timed_out || self.join_timed_out
    }
}

#[derive(Resource, Debug, Default, Clone, PartialEq, Eq)]
pub struct TaskShutdownReport {
    per_kind: [TaskPoolShutdownReport; 3],
    complete: bool,
}

impl TaskShutdownReport {
    /// Combines observations of disjoint physical facilities. Count each facility once.
    pub fn aggregate<'a>(reports: impl IntoIterator<Item = &'a Self>) -> Self {
        let mut result = Self {
            complete: true,
            ..Self::default()
        };
        for report in reports {
            result.complete &= report.complete;
            for (current, next) in result.per_kind.iter_mut().zip(report.per_kind) {
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

    pub const fn is_complete(&self) -> bool {
        self.complete
    }

    pub fn for_kind(&self, kind: TaskPoolKind) -> TaskPoolShutdownReport {
        self.per_kind[kind.index()]
    }

    pub fn timed_out(&self) -> bool {
        self.per_kind.iter().any(|report| report.timed_out())
    }

    pub fn has_cleanup_failure(&self) -> bool {
        self.per_kind.iter().any(|report| report.cleanup_failed)
    }
}

#[derive(Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum ClosePhase {
    #[default]
    Open,
    Drain(Instant),
    Cancel(Instant),
    Join(Instant),
    Complete,
}

pub(super) struct PhysicalOwner(pub(super) Arc<Facility>);

impl PhysicalOwner {
    pub(super) fn begin_close(&self) {
        self.0.close(false);
    }

    pub(super) fn poll(&self) -> (TaskShutdownReport, bool) {
        self.begin_close();
        self.0.poll_close()
    }
}

impl Drop for PhysicalOwner {
    fn drop(&mut self) {
        self.0.close(true);
    }
}

impl Facility {
    fn close(self: &Arc<Self>, abandoned: bool) {
        let mut state = lock(&self.state);
        if state.closed && !abandoned {
            return;
        }
        state.closed = true;
        let now = Instant::now();
        let policy = self.config.shutdown_policy();
        for kind in TaskPoolKind::ALL {
            let pool = &mut state.kinds[kind.index()];
            if pool.phase == ClosePhase::Complete {
                continue;
            }
            if pool.phase == ClosePhase::Open {
                lock(&pool.counters).shutdowns += 1;
            }
            if abandoned || self.inline {
                self.cancel(pool, None);
                pool.phase = ClosePhase::Join(now + policy.join_timeout().get());
                self.join_if_idle(kind, pool);
            } else {
                pool.phase = ClosePhase::Drain(now + policy.drain_timeout().get());
            }
        }
    }

    fn poll_close(self: &Arc<Self>) -> (TaskShutdownReport, bool) {
        let mut state = lock(&self.state);
        let now = Instant::now();
        let policy = self.config.shutdown_policy();
        let mut timed_out = false;
        let mut complete = true;
        for kind in TaskPoolKind::ALL {
            let pool = &mut state.kinds[kind.index()];
            loop {
                match pool.phase {
                    ClosePhase::Drain(deadline)
                        if (pool.pending.is_empty() && pool.running.is_empty())
                            || now >= deadline =>
                    {
                        pool.report.drain_timed_out =
                            !pool.pending.is_empty() || !pool.running.is_empty();
                        self.cancel(pool, None);
                        pool.phase = ClosePhase::Cancel(now + policy.cancel_timeout().get());
                    }
                    ClosePhase::Cancel(deadline) if !pool.has_work() || now >= deadline => {
                        pool.report.cancel_timed_out = pool.has_work();
                        pool.phase = ClosePhase::Join(now + policy.join_timeout().get());
                    }
                    ClosePhase::Join(deadline) => {
                        self.join_if_idle(kind, pool);
                        if pool.phase != ClosePhase::Complete && now >= deadline {
                            if !pool.report.join_timed_out {
                                pool.report.join_timed_out = true;
                                lock(&pool.counters).shutdown_timeouts += 1;
                            }
                            pool.phase = ClosePhase::Join(now + policy.join_timeout().get());
                            timed_out = true;
                        }
                        break;
                    }
                    _ => break,
                }
            }
            complete &= pool.phase == ClosePhase::Complete;
        }
        (
            TaskShutdownReport {
                per_kind: std::array::from_fn(|index| state.kinds[index].report),
                complete,
            },
            timed_out,
        )
    }

    pub(super) fn close_scope(self: &Arc<Self>, activity: &Arc<Activity>) {
        let mut state = lock(&self.state);
        if !activity.closed.swap(true, Ordering::Relaxed) {
            for pool in &mut state.kinds {
                self.cancel(pool, Some(activity));
            }
        }
    }

    fn cancel(self: &Arc<Self>, pool: &mut KindState, scope: Option<&Arc<Activity>>) {
        let reason = if scope.is_some() {
            TaskCancellationReason::ScopeClosed
        } else {
            TaskCancellationReason::PoolShutdown
        };
        let matches =
            |activity: &Arc<Activity>| scope.is_none_or(|scope| Arc::ptr_eq(scope, activity));
        let mut cancelled_pending = 0;
        let pending = pool.extract(|job| {
            if !matches(&job.activity) {
                return false;
            }
            cancelled_pending += usize::from(job.control.cancel(reason));
            true
        });
        let mut requests = 0;
        for running in pool.running.values() {
            if matches(&running.activity)
                && let Some(control) = &running.control
            {
                requests += usize::from(control.cancel(reason));
            }
        }
        if scope.is_none() {
            pool.report.cancelled_pending += cancelled_pending;
            pool.report.cancellation_requests += requests;
        }
        self.dispose_later(pending);
    }

    pub(super) fn join_if_idle(self: &Arc<Self>, kind: TaskPoolKind, pool: &mut KindState) {
        if !matches!(pool.phase, ClosePhase::Join(_))
            || pool.has_work()
            || pool.activating
            || pool.joining
        {
            return;
        }
        let Some(backend) = pool.backend.take() else {
            pool.phase = ClosePhase::Complete;
            return;
        };
        pool.joining = true;
        let facility = self.clone();
        self.retirement.spawn(async move {
            let joined = drop_catching(backend);
            let mut state = lock(&facility.state);
            let pool = &mut state.kinds[kind.index()];
            if joined {
                pool.report.joined_workers = pool.live_workers;
            } else {
                pool.report.panicked_workers = 1;
                pool.report.cleanup_failed = true;
            }
            pool.live_workers = 0;
            pool.phase = ClosePhase::Complete;
        });
    }
}
