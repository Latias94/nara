use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use nara_core::ItemLimit;
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskKindConfig, TaskPoolConfig, TaskPoolKind, TaskPools,
    TaskShutdownPolicy, TaskSpawnRequest, TaskTerminal,
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
    active: Arc<AtomicUsize>,
    #[cfg(test)]
    pub(super) gate: Option<StartupTestGate>,
}

impl StartupPreparationHost {
    pub(super) fn start(
        &mut self,
        prepared: PreparedAuthoringProject,
    ) -> Option<StartupPreparation> {
        let permit = StartupPermit::acquire(&self.active)?;
        if self.pools.is_none() {
            self.pools = Some(TaskPools::try_new(startup_pool_config()?).ok()?);
        }
        #[cfg(test)]
        let mut gate = self.gate.take();
        let task = self
            .pools
            .as_ref()?
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, TaskDomainKey::new(0x7374_6172_7475_70)),
                move |cancellation| {
                    // Logical cancellation is not physical retirement. This guard stays with
                    // the closure, including queued disposal and unwinding after a panic.
                    let _permit = permit;
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
            .into_handle()
            .ok()?;
        Some(StartupPreparation { task })
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
