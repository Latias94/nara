use std::sync::{Arc, Mutex, TryLockError};

use nara_tasks::{
    TaskDomainKey, TaskHandle, TaskPoolKind, TaskPools, TaskReturnState, TaskSpawnRequest,
    TaskTerminal,
};

use super::{
    Context, Diagnostic, DiagnosticCode, DiagnosticReport, PreparedSceneContent,
    ProjectContentErrorKind, ProjectSceneRequestError, ProjectSceneSelection, SafeSummary,
    error_diagnostics,
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

    fn source_content(error: crate::project_host::ProjectContentError) -> Self {
        let undeclared = error.kind() == ProjectContentErrorKind::HostAuthorityRejected
            && error.diagnostics().iter().any(|diagnostic| {
                diagnostic.code().as_str() == "project.content.scene-not-declared"
            });
        Self {
            error: if undeclared {
                ProjectSceneRequestError::UndeclaredScene
            } else {
                ProjectSceneRequestError::Content(error.kind())
            },
            diagnostics: error.diagnostics().clone(),
        }
    }

    fn runtime_content(error: crate::runtime_content::RuntimeContentError) -> Self {
        let kind = ProjectSceneRequestError::RuntimeContent;
        let mut diagnostics = DiagnosticReport::default();
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::new(kind.code()).expect("engine-owned diagnostic code is valid"),
            SafeSummary::new(error.summary()).expect("Runtime content errors have safe summaries"),
        ));
        Self {
            error: kind,
            diagnostics,
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
        selection: impl Into<ProjectSceneSelection>,
        pools: &TaskPools,
        #[cfg(test)] gate: Option<TestGate>,
    ) -> Result<Self, ProjectSceneRequestError> {
        if !pools.is_threaded(TaskPoolKind::Io) {
            return Err(ProjectSceneRequestError::TaskUnavailable);
        }
        let work = Arc::new(Mutex::new(Some(Work {
            context,
            selection: selection.into(),
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
                    work.result = Some(match (work.context.as_ref(), &work.selection) {
                        (
                            Context::Source {
                                candidate,
                                plan,
                                loader,
                            },
                            selection,
                        ) => {
                            let result = match selection {
                                ProjectSceneSelection::Source(path) => {
                                    loader.load_scene(candidate, plan, path)
                                }
                                ProjectSceneSelection::Product(product) => {
                                    loader.load_scene_product(candidate, plan, *product)
                                }
                            };
                            result.map_err(PreparationFailure::source_content).and_then(
                                |snapshot| {
                                    // Required dependencies are data, not public travel roots.
                                    if !work
                                        .context
                                        .declares_source_path(snapshot.selected_scene_path())
                                    {
                                        let error = ProjectSceneRequestError::UndeclaredScene;
                                        return Err(PreparationFailure {
                                            error,
                                            diagnostics: error_diagnostics(error),
                                        });
                                    }
                                    Ok(PreparedSceneContent::Source(snapshot))
                                },
                            )
                        }
                        (Context::Processed { base }, ProjectSceneSelection::Product(product)) => {
                            base.prepare_scene_content(*product)
                                .map(PreparedSceneContent::Processed)
                                .map_err(PreparationFailure::runtime_content)
                        }
                        _ => Err(PreparationFailure {
                            error: ProjectSceneRequestError::UndeclaredScene,
                            diagnostics: error_diagnostics(
                                ProjectSceneRequestError::UndeclaredScene,
                            ),
                        }),
                    });
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

    pub(super) fn poll(&mut self) -> Option<Result<PreparedSceneContent, PreparationFailure>> {
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
    selection: ProjectSceneSelection,
    result: Option<Result<PreparedSceneContent, PreparationFailure>>,
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
