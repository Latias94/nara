use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
};

use nara_core::ItemLimit;
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskKindConfig, TaskPoolConfig, TaskPoolKind, TaskPools,
    TaskRejectReason, TaskRejection, TaskReturnState, TaskShutdownPolicy, TaskShutdownReport,
    TaskSpawnRequest, TaskTerminal,
};

use crate::project_host::authoring::{
    AuthoringProjectCandidate, AuthoringProjectSourceAdmissionError, PreparedAuthoringProject,
};

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

const STARTUP_PENDING_LIMIT: usize = 4;
const STARTUP_CAPTURE_LIMIT: usize = STARTUP_PENDING_LIMIT + 1;

type StartupResult = Result<AuthoringProjectCandidate, AuthoringProjectSourceAdmissionError>;

#[derive(Default)]
pub(super) struct StartupPreparationHost {
    pools: Option<TaskPools>,
    retiring: Option<StartupRetirement>,
    completed: Option<TaskShutdownReport>,
    rejected_input_cleanup_failed: bool,
    closing: bool,
    active: Arc<AtomicUsize>,
    #[cfg(test)]
    pub(super) gate: Option<StartupTestGate>,
    #[cfg(test)]
    config: Option<TaskPoolConfig>,
    #[cfg(test)]
    panic_on_capture_drop: bool,
}

impl StartupPreparationHost {
    pub(super) const fn is_closing(&self) -> bool {
        self.closing
    }

    pub(super) fn start(
        &mut self,
        prepared: PreparedAuthoringProject,
    ) -> Result<StartupPreparation, StartupPreparationFailure> {
        self.advance_retirement();
        if self.closing || self.retiring.is_some() {
            return Err(StartupPreparationFailure::unavailable());
        }
        let permit = StartupPermit::acquire(&self.active)
            .ok_or_else(StartupPreparationFailure::unavailable)?;
        if self.pools.is_none() {
            let config =
                startup_pool_config().ok_or_else(StartupPreparationFailure::unavailable)?;
            #[cfg(test)]
            let config = self.config.unwrap_or(config);
            self.pools = Some(
                TaskPools::try_new_lazy(config)
                    .map_err(|_| StartupPreparationFailure::unavailable())?,
            );
        }
        #[cfg(test)]
        let mut gate = self.gate.take();
        #[cfg(test)]
        let capture_drop = CaptureDropProbe(std::mem::take(&mut self.panic_on_capture_drop));
        let task = self
            .pools
            .as_ref()
            .expect("startup facilities were acquired")
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, TaskDomainKey::new(0x7374_6172_7475_70)),
                move |cancellation| {
                    // Logical cancellation is not physical retirement. This guard stays with
                    // the closure, including queued disposal and unwinding after a panic.
                    let _permit = permit;
                    #[cfg(test)]
                    let _capture_drop = capture_drop;
                    if cancellation.is_cancelled() {
                        return None;
                    }
                    #[cfg(test)]
                    wait_at(&mut gate, StartupTestPhase::BeforeRead);
                    if cancellation.is_cancelled() {
                        return None;
                    }
                    let result = prepared.admit_startup_closure();
                    #[cfg(test)]
                    wait_at(&mut gate, StartupTestPhase::AfterPreparation);
                    if cancellation.is_cancelled() {
                        return None;
                    }
                    Some(result)
                },
            )
            .into_handle();
        match task {
            Ok(task) => Ok(StartupPreparation { task }),
            Err(rejection) => {
                self.rejected_input_cleanup_failed |=
                    rejection.return_state == TaskReturnState::CleanupFailed;
                let retirement = if rejection.reason == TaskRejectReason::WorkerStartupFailed {
                    let retirement = StartupRetirement::new(
                        self.pools
                            .take()
                            .expect("rejected startup retains its facility"),
                    );
                    self.retiring = Some(retirement.clone());
                    Some(retirement)
                } else {
                    None
                };
                Err(StartupPreparationFailure {
                    rejection: Some(rejection),
                    retirement,
                })
            }
        }
    }

    pub(super) fn retirement_pending(&self) -> bool {
        self.retiring
            .as_ref()
            .is_some_and(|owner| !owner.is_complete())
    }

    pub(super) fn advance_retirement(&mut self) -> super::EditorProjectOpenTasksCloseReport {
        if let Some(owner) = &self.retiring {
            owner.poll();
            if owner.is_complete() {
                let report = owner.report();
                self.completed = Some(TaskShutdownReport::aggregate(
                    self.completed.iter().chain(std::iter::once(&report)),
                ));
                self.retiring = None;
            }
        }
        self.report()
    }

    pub(super) fn poll_shutdown(&mut self) -> super::EditorProjectOpenTasksCloseReport {
        self.closing = true;
        if let Some(pools) = self.pools.take() {
            debug_assert!(self.retiring.is_none());
            self.retiring = Some(StartupRetirement::new(pools));
        }
        self.advance_retirement()
    }

    pub(super) fn report(&self) -> super::EditorProjectOpenTasksCloseReport {
        let retiring = self.retiring.as_ref().map(StartupRetirement::report);
        let live = self.pools.as_ref().map(|_| TaskShutdownReport::default());
        super::EditorProjectOpenTasksCloseReport {
            tasks: TaskShutdownReport::aggregate(
                self.completed
                    .iter()
                    .chain(retiring.iter())
                    .chain(live.iter()),
            ),
            rejected_input_cleanup_failed: self.rejected_input_cleanup_failed,
        }
    }
}

#[derive(Clone)]
struct StartupRetirement(Arc<Mutex<RetiringStartup>>);

struct RetiringStartup {
    pools: TaskPools,
    report: TaskShutdownReport,
}

impl StartupRetirement {
    fn new(pools: TaskPools) -> Self {
        Self(Arc::new(Mutex::new(RetiringStartup {
            pools,
            report: TaskShutdownReport::default(),
        })))
    }

    fn poll(&self) {
        let mut owner = self.0.lock().unwrap_or_else(|error| error.into_inner());
        owner.report = owner
            .pools
            .poll_shutdown()
            .expect("startup preparation retains standalone physical ownership");
    }

    fn is_complete(&self) -> bool {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .report
            .is_complete()
    }

    fn report(&self) -> TaskShutdownReport {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .report
            .clone()
    }
}

impl std::fmt::Debug for StartupRetirement {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.report().fmt(formatter)
    }
}

#[derive(Debug)]
pub(super) struct StartupPreparationFailure {
    pub(super) rejection: Option<TaskRejection>,
    retirement: Option<StartupRetirement>,
}

impl StartupPreparationFailure {
    fn unavailable() -> Self {
        Self {
            rejection: None,
            retirement: None,
        }
    }

    pub(super) fn is_complete(&self) -> bool {
        self.retirement
            .as_ref()
            .is_none_or(StartupRetirement::is_complete)
    }

    pub(super) fn report(&self) -> super::EditorProjectOpenTasksCloseReport {
        super::EditorProjectOpenTasksCloseReport {
            tasks: self.retirement.as_ref().map_or_else(
                || TaskShutdownReport::aggregate([]),
                StartupRetirement::report,
            ),
            rejected_input_cleanup_failed: self
                .rejection
                .is_some_and(|rejection| rejection.return_state == TaskReturnState::CleanupFailed),
        }
    }

    pub(super) fn advance_retirement(&mut self) {
        if let Some(owner) = &self.retirement {
            owner.poll();
        }
    }
}

#[cfg(test)]
struct CaptureDropProbe(bool);

#[cfg(test)]
impl Drop for CaptureDropProbe {
    fn drop(&mut self) {
        assert!(!self.0, "injected startup capture cleanup failure");
    }
}

pub(super) struct StartupPreparation {
    task: TaskHandle<Option<StartupResult>>,
}

pub(super) enum StartupPreparationProgress {
    Pending,
    Completed(StartupResult),
    Unavailable,
}

impl StartupPreparation {
    pub(super) fn poll(&mut self) -> StartupPreparationProgress {
        match self.task.try_take() {
            None => StartupPreparationProgress::Pending,
            Some(TaskTerminal::Completed(Some(result))) => {
                StartupPreparationProgress::Completed(result)
            }
            Some(
                TaskTerminal::Completed(None)
                | TaskTerminal::Cancelled(_)
                | TaskTerminal::Failed(_),
            ) => StartupPreparationProgress::Unavailable,
        }
    }
}

impl Drop for StartupPreparation {
    fn drop(&mut self) {
        self.task.cancel();
    }
}

struct StartupPermit {
    active: Arc<AtomicUsize>,
}

impl StartupPermit {
    fn acquire(active: &Arc<AtomicUsize>) -> Option<Self> {
        active
            .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < STARTUP_CAPTURE_LIMIT).then_some(count + 1)
            })
            .ok()?;
        Some(Self {
            active: Arc::clone(active),
        })
    }
}

impl Drop for StartupPermit {
    fn drop(&mut self) {
        self.active.fetch_sub(1, Ordering::AcqRel);
    }
}

fn startup_pool_config() -> Option<TaskPoolConfig> {
    let kind = TaskKindConfig::new(ItemLimit::ONE, ItemLimit::new(STARTUP_PENDING_LIMIT)?);
    TaskPoolConfig::new(kind, kind, kind, TaskShutdownPolicy::default()).ok()
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum StartupTestPhase {
    BeforeRead,
    AfterPreparation,
}

#[cfg(test)]
pub(super) struct StartupTestGate {
    pub(super) phase: StartupTestPhase,
    pub(super) started: std::sync::mpsc::Sender<()>,
    pub(super) release: std::sync::mpsc::Receiver<()>,
    pub(super) panic_after_release: bool,
}

#[cfg(test)]
fn wait_at(gate: &mut Option<StartupTestGate>, phase: StartupTestPhase) {
    if !gate.as_ref().is_some_and(|gate| gate.phase == phase) {
        return;
    }
    let gate = gate.take().expect("the matching startup gate is present");
    let _ = gate.started.send(());
    let _ = gate.release.recv();
    assert!(
        !gate.panic_after_release,
        "injected startup preparation panic"
    );
}
