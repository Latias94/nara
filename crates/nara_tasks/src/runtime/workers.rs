use super::*;

const DORMANT: u8 = 0;
const ACTIVE: u8 = 1;
const FAILED: u8 = 2;
const CLOSED: u8 = 3;

/// Shared custody is private: facades may activate, but only the physical owner may retire.
pub(super) struct TaskWorkers {
    kind: TaskPoolKind,
    count: usize,
    phase: AtomicU8,
    handles: Mutex<Vec<JoinHandle<()>>>,
    #[cfg(test)]
    before_activation: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    #[cfg(test)]
    after_first_worker: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    #[cfg(test)]
    after_reservation: Mutex<Option<Box<dyn FnOnce() + Send>>>,
    #[cfg(test)]
    joined: Arc<AtomicUsize>,
}

pub(super) enum WorkerActivationError {
    Closed,
    Failed(Option<io::Error>),
}

impl TaskWorkers {
    pub(super) fn dormant(kind: TaskPoolKind, count: usize) -> Self {
        Self {
            kind,
            count,
            phase: AtomicU8::new(DORMANT),
            handles: Mutex::new(Vec::new()),
            #[cfg(test)]
            before_activation: Mutex::new(None),
            #[cfg(test)]
            after_first_worker: Mutex::new(None),
            #[cfg(test)]
            after_reservation: Mutex::new(None),
            #[cfg(test)]
            joined: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub(super) fn is_active(&self) -> bool {
        self.phase.load(Ordering::Acquire) == ACTIVE
    }

    pub(super) fn activate(
        &self,
        shared: &Arc<ExecutorShared>,
    ) -> Result<(), WorkerActivationError> {
        if self.phase.load(Ordering::Acquire) == ACTIVE {
            return Ok(());
        }
        let mut handles = lock_unpoisoned(&self.handles);
        match self.phase.load(Ordering::Acquire) {
            ACTIVE => return Ok(()),
            FAILED => return Err(WorkerActivationError::Failed(None)),
            CLOSED => return Err(WorkerActivationError::Closed),
            _ => {}
        }
        if lock_unpoisoned(&shared.queue).lifecycle != QueueLifecycle::Open {
            return Err(WorkerActivationError::Closed);
        }
        #[cfg(test)]
        if let Some(hook) = lock_unpoisoned(&self.before_activation).take() {
            hook();
        }
        for index in 0..self.count {
            // Reserve before spawn: an exiting worker must never race an unrecorded live count.
            {
                let mut queue = lock_unpoisoned(&shared.queue);
                if self.phase.load(Ordering::Acquire) == CLOSED
                    || queue.lifecycle != QueueLifecycle::Open
                {
                    return Err(WorkerActivationError::Closed);
                }
                queue.live_workers += 1;
            }
            #[cfg(test)]
            if let Some(hook) = lock_unpoisoned(&self.after_reservation).take() {
                hook();
            }
            match spawn_task_worker(self.kind, index, shared.clone()) {
                Ok(worker) => {
                    handles.push(worker);
                    #[cfg(test)]
                    if index == 0
                        && let Some(hook) = lock_unpoisoned(&self.after_first_worker).take()
                    {
                        hook();
                    }
                }
                Err(error) => {
                    let mut queue = lock_unpoisoned(&shared.queue);
                    queue.live_workers -= 1;
                    queue.lifecycle = QueueLifecycle::Closed;
                    let _ = self.phase.compare_exchange(
                        DORMANT,
                        FAILED,
                        Ordering::Release,
                        Ordering::Relaxed,
                    );
                    shared.changed.notify_all();
                    return Err(WorkerActivationError::Failed(Some(error)));
                }
            }
        }
        self.phase
            .compare_exchange(DORMANT, ACTIVE, Ordering::Release, Ordering::Relaxed)
            .map(|_| ())
            .map_err(|_| WorkerActivationError::Closed)
    }

    pub(super) fn close_admission(&self, shared: &ExecutorShared, lifecycle: QueueLifecycle) {
        self.phase.store(CLOSED, Ordering::Release);
        let mut queue = lock_unpoisoned(&shared.queue);
        if queue.lifecycle == QueueLifecycle::Open {
            queue.lifecycle = lifecycle;
        }
        shared.changed.notify_all();
    }

    pub(super) fn is_empty(&self) -> bool {
        self.try_handles().is_some_and(|handles| handles.is_empty())
    }

    pub(super) fn join_finished(&self, report: &mut TaskPoolShutdownReport) {
        let Some(mut handles) = self.try_handles() else {
            return;
        };
        let mut index = 0;
        while index < handles.len() {
            if !handles[index].is_finished() {
                index += 1;
                continue;
            }
            if handles.swap_remove(index).join().is_ok() {
                report.joined_workers = report.joined_workers.saturating_add(1);
            } else {
                report.panicked_workers = report.panicked_workers.saturating_add(1);
            }
            #[cfg(test)]
            self.joined.fetch_add(1, Ordering::Release);
        }
    }

    fn try_handles(&self) -> Option<MutexGuard<'_, Vec<JoinHandle<()>>>> {
        match self.handles.try_lock() {
            Ok(handles) => Some(handles),
            Err(std::sync::TryLockError::Poisoned(poisoned)) => Some(poisoned.into_inner()),
            Err(std::sync::TryLockError::WouldBlock) => None,
        }
    }

    #[cfg(test)]
    pub(super) fn before_activation(&self, hook: impl FnOnce() + Send + 'static) {
        *lock_unpoisoned(&self.before_activation) = Some(Box::new(hook));
    }

    #[cfg(test)]
    pub(super) fn after_first_worker(&self, hook: impl FnOnce() + Send + 'static) {
        *lock_unpoisoned(&self.after_first_worker) = Some(Box::new(hook));
    }

    #[cfg(test)]
    pub(super) fn after_reservation(&self, hook: impl FnOnce() + Send + 'static) {
        *lock_unpoisoned(&self.after_reservation) = Some(Box::new(hook));
    }

    #[cfg(test)]
    pub(super) fn join_probe(&self) -> Arc<AtomicUsize> {
        self.joined.clone()
    }

    #[cfg(test)]
    pub(super) fn retained_for_tests(handles: Vec<JoinHandle<()>>) -> Self {
        Self {
            kind: TaskPoolKind::Io,
            count: 0,
            phase: AtomicU8::new(CLOSED),
            handles: Mutex::new(handles),
            before_activation: Mutex::new(None),
            after_first_worker: Mutex::new(None),
            after_reservation: Mutex::new(None),
            joined: Arc::new(AtomicUsize::new(0)),
        }
    }
}
