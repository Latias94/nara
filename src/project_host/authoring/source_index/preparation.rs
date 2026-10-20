//! One read-only source scan borrowed by the existing managed runtime's I/O pool.

use std::sync::{
    Arc, Mutex, TryLockError,
    atomic::{AtomicBool, Ordering},
};

use nara_tasks::{
    TaskCancellationToken, TaskDomainKey, TaskHandle, TaskPoolKind, TaskPools, TaskSpawnRequest,
    TaskTerminal,
};

use super::{
    AuthoringSourceIndexBasis, ProjectSourceIndexBudgetUsage, ProjectSourceIndexDiagnosticPhase,
    ProjectSourceIndexFailure, ProjectSourceIndexGeneration, ProjectSourceIndexLimits,
    ProjectSourceIndexProgress, ProjectSourceIndexSnapshot,
    attempt::{AuthoringSourceIndexAttempt, SourceIndexAttemptProgress},
};

type ScanResult = Result<ProjectSourceIndexSnapshot, ProjectSourceIndexFailure>;

pub(super) struct ScanPreparation {
    generation: ProjectSourceIndexGeneration,
    task: TaskHandle<Option<ScanResult>>,
    progress: Arc<Mutex<ProjectSourceIndexProgress>>,
    returned: Arc<AtomicBool>,
}

pub(super) enum ScanPreparationPoll {
    Pending,
    Completed(ProjectSourceIndexSnapshot),
    Failed(ProjectSourceIndexFailure),
    Unavailable,
}

impl ScanPreparation {
    pub(super) fn start(
        basis: AuthoringSourceIndexBasis,
        generation: ProjectSourceIndexGeneration,
        limits: ProjectSourceIndexLimits,
        pools: &TaskPools,
        #[cfg(test)] mut gate: Option<ScanTestGate>,
    ) -> Option<Self> {
        let progress = Arc::new(Mutex::new(ProjectSourceIndexProgress::new(
            ProjectSourceIndexDiagnosticPhase::RootNormalization,
            ProjectSourceIndexBudgetUsage::default(),
        )));
        let returned = Arc::new(AtomicBool::new(false));
        // Construct before spawn: queued disposal or admission rejection must also drop this
        // guard. A cancelled TaskHandle alone never proves captured capabilities have returned.
        let request = ScanRequest {
            basis: Some(basis),
            generation,
            limits,
            _returned: PhysicalReturn(Arc::clone(&returned)),
        };
        let worker_progress = Arc::clone(&progress);
        let task = pools
            .spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(0, TaskDomainKey::new(0x6272_6f77_7365_7273)),
                move |cancellation| {
                    request.run(
                        &cancellation,
                        &worker_progress,
                        #[cfg(test)]
                        &mut gate,
                    )
                },
            )
            .into_handle()
            .ok()?;
        Some(Self {
            generation,
            task,
            progress,
            returned,
        })
    }

    pub(super) const fn generation(&self) -> ProjectSourceIndexGeneration {
        self.generation
    }

    pub(super) fn progress(&self) -> Option<ProjectSourceIndexProgress> {
        match self.progress.try_lock() {
            Ok(progress) => Some(*progress),
            Err(TryLockError::Poisoned(error)) => Some(*error.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }

    pub(super) fn poll(&mut self) -> ScanPreparationPoll {
        if !self.returned.load(Ordering::Acquire) {
            return ScanPreparationPoll::Pending;
        }
        match self.task.try_take() {
            None => ScanPreparationPoll::Pending,
            Some(TaskTerminal::Completed(Some(Ok(snapshot)))) => {
                ScanPreparationPoll::Completed(snapshot)
            }
            Some(TaskTerminal::Completed(Some(Err(failure)))) => {
                ScanPreparationPoll::Failed(failure)
            }
            Some(
                TaskTerminal::Completed(None)
                | TaskTerminal::Cancelled(_)
                | TaskTerminal::Failed(_),
            ) => ScanPreparationPoll::Unavailable,
        }
    }

    pub(super) fn cancel(&self) {
        let _ = self.task.cancel();
    }
}

impl Drop for ScanPreparation {
    fn drop(&mut self) {
        self.cancel();
    }
}

struct PhysicalReturn(Arc<AtomicBool>);
impl Drop for PhysicalReturn {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

struct ScanRequest {
    basis: Option<AuthoringSourceIndexBasis>,
    generation: ProjectSourceIndexGeneration,
    limits: ProjectSourceIndexLimits,
    // Field order also keeps queued closure disposal truthful: capabilities drop first.
    _returned: PhysicalReturn,
}

impl ScanRequest {
    fn run(
        mut self,
        cancellation: &TaskCancellationToken,
        progress: &Mutex<ProjectSourceIndexProgress>,
        #[cfg(test)] gate: &mut Option<ScanTestGate>,
    ) -> Option<ScanResult> {
        #[cfg(test)]
        wait_at(gate, ScanTestPhase::BeforeBegin);
        let result = prepare(
            self.basis.take().expect("a source scan request runs once"),
            self.generation,
            self.limits,
            cancellation,
            progress,
            #[cfg(test)]
            gate,
        );
        #[cfg(test)]
        wait_at(gate, ScanTestPhase::AfterPreparation);
        if cancellation.is_cancelled() {
            None
        } else {
            result
        }
    }
}

fn prepare(
    basis: AuthoringSourceIndexBasis,
    generation: ProjectSourceIndexGeneration,
    limits: ProjectSourceIndexLimits,
    cancellation: &TaskCancellationToken,
    progress: &Mutex<ProjectSourceIndexProgress>,
    #[cfg(test)] gate: &mut Option<ScanTestGate>,
) -> Option<ScanResult> {
    if cancellation.is_cancelled() {
        return None;
    }
    let mut attempt = match AuthoringSourceIndexAttempt::begin(basis, generation, limits) {
        Ok(attempt) => attempt,
        Err(failure) => return Some(Err(failure)),
    };
    loop {
        let current = attempt.progress();
        *progress.lock().unwrap_or_else(|error| error.into_inner()) = current;
        #[cfg(test)]
        if current.phase() == ProjectSourceIndexDiagnosticPhase::LineageRevalidation {
            wait_at(gate, ScanTestPhase::BeforeRevalidation);
        }
        if cancellation.is_cancelled() {
            return None;
        }
        match attempt.advance() {
            SourceIndexAttemptProgress::Pending => {}
            SourceIndexAttemptProgress::Published(snapshot) => return Some(Ok(snapshot)),
            SourceIndexAttemptProgress::Failed(failure) => return Some(Err(failure)),
        }
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScanTestPhase {
    BeforeBegin,
    BeforeRevalidation,
    AfterPreparation,
}

#[cfg(test)]
pub(in crate::project_host::authoring) struct ScanTestGate {
    phase: ScanTestPhase,
    started: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
    panic_after_release: bool,
}

#[cfg(test)]
pub(crate) struct ScanTestControl {
    started: std::sync::mpsc::Receiver<()>,
    release: Option<std::sync::mpsc::Sender<()>>,
}

#[cfg(test)]
impl ScanTestGate {
    pub(super) fn new(phase: ScanTestPhase, panic_after_release: bool) -> (Self, ScanTestControl) {
        let (started_tx, started) = std::sync::mpsc::channel();
        let (release, receiver) = std::sync::mpsc::channel();
        (
            Self {
                phase,
                started: started_tx,
                release: receiver,
                panic_after_release,
            },
            ScanTestControl {
                started,
                release: Some(release),
            },
        )
    }
}

#[cfg(test)]
impl ScanTestControl {
    pub(crate) fn wait_until_started(&self) -> bool {
        self.started
            .recv_timeout(std::time::Duration::from_secs(5))
            .is_ok()
    }
    pub(crate) fn release(mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

#[cfg(test)]
impl Drop for ScanTestControl {
    fn drop(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

#[cfg(test)]
fn wait_at(gate: &mut Option<ScanTestGate>, phase: ScanTestPhase) {
    if !gate.as_ref().is_some_and(|gate| gate.phase == phase) {
        return;
    }
    let gate = gate.take().expect("the matching scan gate is present");
    let _ = gate.started.send(());
    let _ = gate.release.recv();
    assert!(
        !gate.panic_after_release,
        "injected source scan preparation panic"
    );
}
