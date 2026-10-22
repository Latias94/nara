use std::sync::{Arc, Mutex, TryLockError};

use nara_project::ProjectPath;
use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskPoolKind, TaskPools, TaskReturnState, TaskSpawnRequest,
    TaskTerminal,
};

use super::{
    Context, DiagnosticReport, ProjectContentSnapshot, ProjectSceneRequestError, error_diagnostics,
};

#[derive(Debug)]
pub(super) struct PreparationFailure {
    pub(super) error: ProjectSceneRequestError,
    pub(super) diagnostics: DiagnosticReport,
}

impl PreparationFailure {
    fn unavailable() -> Self {
        let error = ProjectSceneRequestError::TaskUnavailable;
        Self {
            error,
            diagnostics: error_diagnostics(error),
        }
    }
}

pub(super) struct Preparation {
    task: TaskHandle<()>,
    work: Arc<Mutex<Option<Work>>>,
}

impl Preparation {
    pub(super) fn start(
        context: Arc<Context>,
        path: ProjectPath,
        pools: &TaskPools,
        #[cfg(test)] gate: Option<TestGate>,
    ) -> Result<Self, ProjectSceneRequestError> {
        if !pools.is_threaded(TaskPoolKind::Io) {
            return Err(ProjectSceneRequestError::TaskUnavailable);
        }
        let work = Arc::new(Mutex::new(Some(Work {
            context,
            path,
            result: None,
            #[cfg(test)]
            gate,
        })));
        let input = Input {
            work: Arc::clone(&work),
        };
        let task = pools
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, TaskDomainKey::new(0x7363_656e_65)),
                move |cancellation| {
                    let input = input;
                    let mut slot = input.work.lock().unwrap_or_else(|error| error.into_inner());
                    let Some(work) = slot.as_mut() else { return };
                    #[cfg(test)]
                    wait_at(&mut work.gate, TestPhase::BeforeRead);
                    if cancellation.is_cancelled() {
                        return;
                    }
                    // Startup already established executable schema agreement. Ordinary targets use the
                    // same frozen native plan; they are not new startup attestations or schema owners.
                    work.result = Some(
                        work.context
                            .loader
                            .load_scene(&work.context.candidate, &work.context.plan, &work.path)
                            .map_err(|error| PreparationFailure {
                                error: ProjectSceneRequestError::Content(error.kind()),
                                diagnostics: error.diagnostics().clone(),
                            }),
                    );
                    #[cfg(test)]
                    wait_at(&mut work.gate, TestPhase::AfterPreparation);
                },
            )
            .into_handle()
            .map_err(|_| ProjectSceneRequestError::TaskUnavailable)?;
        Ok(Self { task, work })
    }

    pub(super) fn cancel(&self) {
        self.task.cancel();
    }

    pub(super) fn poll(&mut self) -> Option<Result<ProjectContentSnapshot, PreparationFailure>> {
        let returned = self.task.return_observer().state();
        if returned == TaskReturnState::Pending {
            return None;
        }
        if !self.task.is_finished() {
            return None;
        }
        let mut slot = match self.work.try_lock() {
            Ok(slot) => slot,
            Err(TryLockError::WouldBlock) => return None,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        let terminal = self.task.try_take()?;
        let work = slot.take()?;
        if returned == TaskReturnState::CleanupFailed {
            return Some(Err(PreparationFailure::unavailable()));
        }
        Some(match terminal {
            TaskTerminal::Completed(()) => work
                .result
                .unwrap_or_else(|| Err(PreparationFailure::unavailable())),
            TaskTerminal::Cancelled(_) | TaskTerminal::Failed(_) => {
                Err(PreparationFailure::unavailable())
            }
        })
    }
}

impl Drop for Preparation {
    fn drop(&mut self) {
        self.cancel();
    }
}

struct Work {
    context: Arc<Context>,
    path: ProjectPath,
    result: Option<Result<ProjectContentSnapshot, PreparationFailure>>,
    #[cfg(test)]
    gate: Option<TestGate>,
}

// Keep the potentially large result in Work, not the task terminal: cancellation cannot race an
// unobserved result destructor after the slot has been released. Tasks observes capture disposal.
struct Input {
    work: Arc<Mutex<Option<Work>>>,
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum TestPhase {
    BeforeRead,
    AfterPreparation,
}

#[cfg(test)]
pub(super) struct TestGate {
    phase: TestPhase,
    started: std::sync::mpsc::Sender<()>,
    release: Mutex<std::sync::mpsc::Receiver<()>>,
    panic_after_release: bool,
}

#[cfg(test)]
pub(super) struct TestControl {
    started: std::sync::mpsc::Receiver<()>,
    release: Option<std::sync::mpsc::Sender<()>>,
}

#[cfg(test)]
impl TestGate {
    pub(super) fn new(phase: TestPhase) -> (Self, TestControl) {
        let (started_tx, started) = std::sync::mpsc::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        (
            Self {
                phase,
                started: started_tx,
                release: Mutex::new(release_rx),
                panic_after_release: false,
            },
            TestControl {
                started,
                release: Some(release),
            },
        )
    }

    pub(super) fn panicking(phase: TestPhase) -> (Self, TestControl) {
        let (mut gate, control) = Self::new(phase);
        gate.panic_after_release = true;
        (gate, control)
    }
}

#[cfg(test)]
impl TestControl {
    pub(super) fn wait_until_started(&self) -> bool {
        self.started
            .recv_timeout(std::time::Duration::from_secs(5))
            .is_ok()
    }
    pub(super) fn release(mut self) {
        self.release.take();
    }
    pub(super) fn into_channels(
        mut self,
    ) -> (std::sync::mpsc::Receiver<()>, std::sync::mpsc::Sender<()>) {
        (
            self.started,
            self.release
                .take()
                .expect("the test gate has one release sender"),
        )
    }
}

#[cfg(test)]
fn wait_at(gate: &mut Option<TestGate>, phase: TestPhase) {
    if gate.as_ref().is_some_and(|gate| gate.phase == phase) {
        let gate = gate.take().expect("matching test gate");
        let _ = gate.started.send(());
        let _ = gate.release.lock().unwrap().recv();
        assert!(
            !gate.panic_after_release,
            "project Scene preparation panic probe"
        );
    }
}
