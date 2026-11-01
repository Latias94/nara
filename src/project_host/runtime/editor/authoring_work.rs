//! Finite authoring work retains its actual result until the worker physically returns.

use std::sync::{Arc, Mutex, TryLockError};

use nara_tasks::{TaskHandle, TaskPoolKind, TaskPools, TaskReturnState, TaskSpawnRequest};

pub(super) struct Job<T> {
    pub(super) run: Option<Box<dyn FnOnce() -> T + Send>>,
    pub(super) started: bool,
    pub(super) result: Option<T>,
}

pub(super) struct Pending<T> {
    pub(super) job: Arc<Mutex<Job<T>>>,
    pub(super) task: Option<TaskHandle<()>>,
    pub(super) rejection: Option<nara_tasks::TaskRejection>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum WorkError {
    CancelledBeforeStart,
    Unavailable,
    Unobserved,
}

impl<T: Send + 'static> Pending<T> {
    pub(super) fn start(
        pools: &TaskPools,
        request: TaskSpawnRequest,
        run: impl FnOnce() -> T + Send + 'static,
    ) -> Self {
        let job = Arc::new(Mutex::new(Job {
            run: Some(Box::new(run)),
            started: false,
            result: None,
        }));
        let worker = Arc::clone(&job);
        let admitted = pools
            .spawn(TaskPoolKind::Io, request, move |context| {
                let mut job = worker.lock().unwrap_or_else(|error| error.into_inner());
                if context.is_cancelled() {
                    return;
                }
                job.started = true;
                let run = job.run.take().expect("finite operation executes once");
                job.result = Some(run());
            })
            .into_handle();
        let (task, rejection) = match admitted {
            Ok(task) => (Some(task), None),
            Err(rejection) => (None, Some(rejection)),
        };
        Self {
            job,
            task,
            rejection,
        }
    }

    pub(super) fn poll(&mut self) -> Option<Result<T, WorkError>> {
        if self
            .task
            .as_ref()
            .is_some_and(|task| task.return_observer().state() == TaskReturnState::Pending)
        {
            return None;
        }
        let mut job = match self.job.try_lock() {
            Ok(job) => job,
            Err(TryLockError::WouldBlock) => return None,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        Some(match job.result.take() {
            Some(result) => Ok(result),
            None if job.started => Err(WorkError::Unobserved),
            None if self
                .task
                .as_ref()
                .is_some_and(|task| task.cancellation_token().is_cancelled()) =>
            {
                Err(WorkError::CancelledBeforeStart)
            }
            None => Err(WorkError::Unavailable),
        })
    }

    pub(super) fn cancel(&self) {
        if let Some(task) = &self.task {
            task.cancel();
        }
    }
}

/// Retains a move-only input outside the disposable Task callback until execution starts.
/// Publication and proposal preparation share this custody; Tasks still own execution.
pub(super) struct OwnedJob<I, O> {
    pub(super) pending: Pending<()>,
    owner: Arc<Mutex<OwnerState<I, O>>>,
}

enum OwnerState<I, O> {
    Ready(I),
    Running,
    Returned(O),
}

pub(super) enum OwnedReturn<I, O> {
    Unstarted(I),
    Returned(O),
}

struct OwnerRun<I, O, F> {
    owner: Arc<Mutex<OwnerState<I, O>>>,
    advance: F,
}

impl<I, O, F: FnOnce(I) -> O> OwnerRun<I, O, F> {
    fn run(self) {
        let input = {
            let mut state = self.owner.lock().unwrap_or_else(|error| error.into_inner());
            let OwnerState::Ready(input) = std::mem::replace(&mut *state, OwnerState::Running)
            else {
                unreachable!("a finite owner job starts once");
            };
            input
        };
        let output = (self.advance)(input);
        *self.owner.lock().unwrap_or_else(|error| error.into_inner()) =
            OwnerState::Returned(output);
    }
}

impl<I: Send + 'static, O: Send + 'static> OwnedJob<I, O> {
    pub(super) fn storage_bytes<F>(_advance: &F) -> usize {
        // Both Arc allocations, their mutex payloads, the boxed Pending callback and the
        // single-Arc Task callback. Inline work storage belongs to the enclosing owner.
        4 * size_of::<usize>()
            + size_of::<Mutex<OwnerState<I, O>>>()
            + size_of::<Mutex<Job<()>>>()
            + size_of::<OwnerRun<I, O, F>>()
            + size_of::<Arc<Mutex<Job<()>>>>()
    }

    pub(super) fn start<F: FnOnce(I) -> O + Send + 'static>(
        pools: &TaskPools,
        request: TaskSpawnRequest,
        input: I,
        advance: F,
    ) -> Self {
        let owner = Arc::new(Mutex::new(OwnerState::Ready(input)));
        let run = OwnerRun {
            owner: Arc::clone(&owner),
            advance,
        };
        let pending = Pending::start(pools, request, move || run.run());
        Self { pending, owner }
    }

    /// Call only after physical return or synchronous spawn rejection. Dropping Pending
    /// disposes its callback and proposal captures before the caller refunds any storage.
    pub(super) fn into_owner(self) -> Result<OwnedReturn<I, O>, WorkError> {
        let Self { pending, owner } = self;
        drop(pending);
        let state = Arc::into_inner(owner)
            .expect("physical return and Job disposal release the worker's shared slot")
            .into_inner()
            .unwrap_or_else(|error| error.into_inner());
        match state {
            OwnerState::Ready(input) => Ok(OwnedReturn::Unstarted(input)),
            OwnerState::Returned(output) => Ok(OwnedReturn::Returned(output)),
            OwnerState::Running => Err(WorkError::Unobserved),
        }
    }
}
