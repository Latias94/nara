//! Retained file-host custody is independent of a task's first terminal result.

use std::sync::{Arc, Mutex, TryLockError};

use nara_scene::AuthoringDocumentReadmission;
use nara_tasks::{TaskHandle, TaskPools};

use super::*;

#[cfg(all(test, any(windows, target_os = "linux")))]
mod tests;

pub(super) struct SaveWork {
    lane: PersistenceLane,
    candidate: Option<PendingSave>,
    authoring: Option<AuthoringDocumentReadmission>,
    started: bool,
    outcome: Option<PersistenceSaveOutcome>,
    #[cfg(test)]
    gate: Option<SaveTestGate>,
}

type SharedSaveWork = Arc<Mutex<Option<SaveWork>>>;

pub(crate) struct BackgroundSave {
    // Work retains a lane handle; the runtime independently retains the actual Host through
    // operation completion. The task's disposable terminal contains no file authority.
    work: SharedSaveWork,
    task: Option<TaskHandle<()>>,
}

impl BackgroundSave {
    pub(super) fn start(
        lane: PersistenceLane,
        candidate: PendingSave,
        authoring: Option<AuthoringDocumentReadmission>,
        pools: &TaskPools,
        #[cfg(test)] gate: Option<SaveTestGate>,
    ) -> Self {
        use nara_tasks::{TaskDomainKey, TaskPoolKind, TaskSpawnRequest};
        let work = Arc::new(Mutex::new(Some(SaveWork {
            lane,
            candidate: Some(candidate),
            authoring,
            started: false,
            outcome: None,
            #[cfg(test)]
            gate,
        })));
        let retained = Arc::clone(&work);
        let task = pools
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, TaskDomainKey::new(0x6564_6974_7361_7665)),
                move |_| {
                    let mut slot = retained.lock().unwrap_or_else(|error| error.into_inner());
                    let Some(work) = slot.as_mut() else { return };
                    work.started = true;
                    #[cfg(test)]
                    work.wait_at(SaveTestPhase::BeforePublication);
                    let candidate = work.candidate.take().expect("a save runs once");
                    let basis = work.authoring.take();
                    work.outcome = Some(match (&mut work.lane, candidate) {
                        (PersistenceLane::Scene(lane), PendingSave::Scene(candidate)) => lane
                            .host
                            .with_worker(|host| host.save_owned(candidate, basis))
                            .unwrap_or(PersistenceSaveOutcome::Rejected(
                                EditorPersistenceRejection::Busy,
                            )),
                        (PersistenceLane::Prefab(lane), PendingSave::Prefab(candidate)) => lane
                            .host
                            .with_worker(|host| host.save_owned(candidate, basis))
                            .unwrap_or(PersistenceSaveOutcome::Rejected(
                                EditorPersistenceRejection::Busy,
                            )),
                        (PersistenceLane::Extension(lane), PendingSave::Extension(checkpoint)) => {
                            lane.save(checkpoint)
                        }
                        _ => PersistenceSaveOutcome::Rejected(
                            EditorPersistenceRejection::DocumentKindMismatch,
                        ),
                    });
                    #[cfg(test)]
                    work.wait_at(SaveTestPhase::AfterPublication);
                },
            )
            .into_handle()
            .ok();
        Self { work, task }
    }

    pub(super) fn poll(&mut self) -> Option<(PersistenceLane, PersistenceSaveOutcome)> {
        if self.task.as_ref().is_some_and(|task| !task.is_finished()) {
            return None;
        }
        // Logical cancellation may arrive while the worker still owns the mutex. Never wait
        // for filesystem work on the caller, and never mistake cancellation for its return.
        let mut slot = match self.work.try_lock() {
            Ok(slot) => slot,
            Err(TryLockError::WouldBlock) => return None,
            Err(TryLockError::Poisoned(error)) => error.into_inner(),
        };
        let mut work = slot.take()?;
        let outcome = work.outcome.take().unwrap_or_else(|| {
            if work.started {
                // An unexpected unwind may follow a name switch. Require reconciliation.
                work.lane.mark_uncertain();
                PersistenceSaveOutcome::Rejected(EditorPersistenceRejection::RequiresReconcile)
            } else {
                PersistenceSaveOutcome::Rejected(EditorPersistenceRejection::Busy)
            }
        });
        Some((work.lane, outcome))
    }
}

#[cfg(test)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum SaveTestPhase {
    BeforePublication,
    AfterPublication,
}

#[cfg(test)]
pub(crate) struct SaveTestGate {
    phase: SaveTestPhase,
    started: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
}

#[cfg(test)]
pub(crate) struct SaveTestControl {
    started: std::sync::mpsc::Receiver<()>,
    release: Option<std::sync::mpsc::Sender<()>>,
}

#[cfg(test)]
impl SaveTestControl {
    pub(crate) fn wait_until_started(&self) -> bool {
        self.started.recv_timeout(Duration::from_secs(5)).is_ok()
    }
    pub(crate) fn release(mut self) {
        self.release.take().unwrap().send(()).unwrap();
    }
}

#[cfg(test)]
impl Drop for SaveTestControl {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

#[cfg(test)]
impl SaveTestGate {
    pub(super) fn new(phase: SaveTestPhase) -> (Self, SaveTestControl) {
        let (started_tx, started) = std::sync::mpsc::channel();
        let (release, release_rx) = std::sync::mpsc::channel();
        (
            Self {
                phase,
                started: started_tx,
                release: release_rx,
            },
            SaveTestControl {
                started,
                release: Some(release),
            },
        )
    }
}

#[cfg(test)]
impl SaveWork {
    fn wait_at(&mut self, phase: SaveTestPhase) {
        if self.gate.as_ref().is_some_and(|gate| gate.phase == phase) {
            let gate = self.gate.take().unwrap();
            let _ = gate.started.send(());
            gate.release
                .recv_timeout(Duration::from_secs(15))
                .expect("save test release guard must retire");
        }
    }
}
