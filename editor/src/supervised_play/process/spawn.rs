use std::{
    io,
    sync::mpsc::{self, Receiver, TryRecvError},
    thread::{self, JoinHandle},
    time::Instant,
};

use thiserror::Error;

use super::{
    SupervisedProcessCommand, SupervisedProcessError, SupervisedProcessOwner,
    SupervisedProductProcessCommand,
};
use crate::supervised_play::{log::ProcessLogLimits, wake::SupervisedPlayWake};

struct SpawnWorkerOutcome {
    completed_at: Instant,
    result: Result<SupervisedProcessOwner, SupervisedProcessError>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SupervisedProcessSpawnRetirement {
    Cancelled,
    TimedOut,
}

#[derive(Debug, Error)]
pub(crate) enum SupervisedProcessSpawnFailure {
    #[error("the supervised process spawn worker could not start")]
    Dispatch(#[source] io::Error),
    #[error("the supervised process spawn worker disconnected")]
    WorkerDisconnected,
    #[error("the supervised process spawn worker panicked")]
    WorkerPanicked,
    #[error("the supervised process could not be spawned")]
    Process(#[source] SupervisedProcessError),
    #[error("the late supervised process could not be cleaned up")]
    Cleanup(#[source] SupervisedProcessError),
}

pub(crate) enum SupervisedProcessSpawnTerminal {
    Failed(SupervisedProcessSpawnFailure),
    Retired(SupervisedProcessSpawnRetirement),
}

pub(crate) struct AcquiredSupervisedProcess {
    process: SupervisedProcessOwner,
    spawned_at: Instant,
}

impl AcquiredSupervisedProcess {
    pub(crate) fn into_parts(self) -> (SupervisedProcessOwner, Instant) {
        (self.process, self.spawned_at)
    }
}

/// Private owner of one bounded background OS spawn and any authority that arrives late.
///
/// The worker may block inside platform process creation or rollback without blocking the UI
/// thread. A timeout or cancellation retains this owner until a late process has produced an exact
/// cleanup receipt; no caller may infer that a pending worker created no process.
pub(crate) struct SupervisedProcessSpawnOwner {
    receiver: Receiver<SpawnWorkerOutcome>,
    worker: Option<JoinHandle<()>>,
    spawn_deadline: Instant,
    acquired: Option<AcquiredSupervisedProcess>,
    late_process: Option<SupervisedProcessOwner>,
    retirement: Option<SupervisedProcessSpawnRetirement>,
    cleanup_incomplete: bool,
    terminal: Option<SupervisedProcessSpawnTerminal>,
}

impl SupervisedProcessSpawnOwner {
    pub(crate) fn start_command(
        command: SupervisedProcessCommand,
        log_limits: ProcessLogLimits,
        spawn_deadline: Instant,
        wake: SupervisedPlayWake,
    ) -> Result<Self, SupervisedProcessSpawnFailure> {
        Self::dispatch(spawn_deadline, wake.clone(), move || {
            SupervisedProcessOwner::spawn_with_wake(&command, log_limits, wake)
        })
    }

    pub(crate) fn start_exact_generation(
        generation: crate::supervised_play::generation::ImmutableExecutableGeneration,
        command: SupervisedProductProcessCommand,
        log_limits: ProcessLogLimits,
        spawn_deadline: Instant,
        wake: SupervisedPlayWake,
    ) -> Result<Self, SupervisedProcessSpawnFailure> {
        Self::dispatch(spawn_deadline, wake.clone(), move || {
            let authority = generation
                .revalidate_for_launch()
                .map_err(SupervisedProcessError::Generation)?
                .into_authority();
            SupervisedProcessOwner::spawn_exact_generation_with_wake(
                authority, &command, log_limits, wake,
            )
        })
    }

    fn dispatch(
        spawn_deadline: Instant,
        wake: SupervisedPlayWake,
        spawn: impl FnOnce() -> Result<SupervisedProcessOwner, SupervisedProcessError> + Send + 'static,
    ) -> Result<Self, SupervisedProcessSpawnFailure> {
        let (sender, receiver) = mpsc::sync_channel(1);
        let worker_wake = wake;
        let worker = thread::Builder::new()
            .name("nara-supervised-process-spawn".to_owned())
            .spawn(move || {
                let result = spawn();
                let completed_at = Instant::now();
                let _ = sender.send(SpawnWorkerOutcome {
                    completed_at,
                    result,
                });
                worker_wake.notify();
            })
            .map_err(SupervisedProcessSpawnFailure::Dispatch)?;
        Ok(Self {
            receiver,
            worker: Some(worker),
            spawn_deadline,
            acquired: None,
            late_process: None,
            retirement: None,
            cleanup_incomplete: false,
            terminal: None,
        })
    }

    pub(crate) fn request_cancel(
        &mut self,
        now: Instant,
    ) -> Result<(), SupervisedProcessSpawnFailure> {
        if self.terminal.is_some() {
            return Ok(());
        }
        self.retirement
            .get_or_insert(SupervisedProcessSpawnRetirement::Cancelled);
        if let Some(acquired) = self.acquired.take() {
            let (process, _) = acquired.into_parts();
            self.late_process = Some(process);
        }
        if let Some(process) = self.late_process.as_mut()
            && let Err(error) = process.request_terminate(now)
        {
            self.cleanup_incomplete = true;
            return Err(SupervisedProcessSpawnFailure::Cleanup(error));
        }
        Ok(())
    }

    pub(crate) fn retry_cleanup(
        &mut self,
        now: Instant,
    ) -> Result<(), SupervisedProcessSpawnFailure> {
        let Some(process) = self.late_process.as_mut() else {
            self.cleanup_incomplete = false;
            return Ok(());
        };
        process
            .retry_cleanup(now)
            .map_err(SupervisedProcessSpawnFailure::Cleanup)?;
        self.cleanup_incomplete = false;
        Ok(())
    }

    pub(crate) fn advance(&mut self, now: Instant) -> Result<(), SupervisedProcessSpawnFailure> {
        if self.terminal.is_some() || self.acquired.is_some() {
            return Ok(());
        }

        self.poll_worker(now)?;
        if self.terminal.is_some() || self.acquired.is_some() {
            return Ok(());
        }

        if self.retirement.is_none() && now >= self.spawn_deadline {
            self.retirement = Some(SupervisedProcessSpawnRetirement::TimedOut);
        }

        let Some(process) = self.late_process.as_mut() else {
            return Ok(());
        };
        if let Err(error) = process.advance(now) {
            self.cleanup_incomplete = true;
            return Err(SupervisedProcessSpawnFailure::Cleanup(error));
        }
        if process.cleanup_receipt().is_some() {
            drop(self.late_process.take());
            let retirement = self
                .retirement
                .expect("a late process exists only for a retirement disposition");
            self.terminal = Some(SupervisedProcessSpawnTerminal::Retired(retirement));
            self.cleanup_incomplete = false;
        }
        Ok(())
    }

    fn poll_worker(&mut self, now: Instant) -> Result<(), SupervisedProcessSpawnFailure> {
        let outcome = match self.receiver.try_recv() {
            Ok(outcome) => outcome,
            Err(TryRecvError::Empty) => return Ok(()),
            Err(TryRecvError::Disconnected) => {
                let failure = self
                    .join_worker_failure()
                    .unwrap_or(SupervisedProcessSpawnFailure::WorkerDisconnected);
                self.terminal = Some(SupervisedProcessSpawnTerminal::Failed(failure));
                return Ok(());
            }
        };
        self.join_finished_worker()?;

        match outcome.result {
            Ok(process)
                if self.retirement != Some(SupervisedProcessSpawnRetirement::Cancelled)
                    && outcome.completed_at <= self.spawn_deadline =>
            {
                self.retirement = None;
                self.acquired = Some(AcquiredSupervisedProcess {
                    process,
                    spawned_at: outcome.completed_at,
                });
            }
            Ok(process) => {
                self.retirement
                    .get_or_insert(SupervisedProcessSpawnRetirement::TimedOut);
                self.late_process = Some(process);
                if let Err(error) = self
                    .late_process
                    .as_mut()
                    .expect("the late process remains retained")
                    .request_terminate(now)
                {
                    self.cleanup_incomplete = true;
                    return Err(SupervisedProcessSpawnFailure::Cleanup(error));
                }
            }
            Err(error) => {
                let terminal = match self.retirement {
                    Some(retirement) => SupervisedProcessSpawnTerminal::Retired(retirement),
                    None if outcome.completed_at > self.spawn_deadline => {
                        SupervisedProcessSpawnTerminal::Retired(
                            SupervisedProcessSpawnRetirement::TimedOut,
                        )
                    }
                    None => SupervisedProcessSpawnTerminal::Failed(
                        SupervisedProcessSpawnFailure::Process(error),
                    ),
                };
                self.terminal = Some(terminal);
            }
        }
        Ok(())
    }

    fn join_finished_worker(&mut self) -> Result<(), SupervisedProcessSpawnFailure> {
        let Some(worker) = self.worker.take() else {
            return Ok(());
        };
        if !worker.is_finished() {
            self.worker = Some(worker);
            return Ok(());
        }
        worker
            .join()
            .map_err(|_| SupervisedProcessSpawnFailure::WorkerPanicked)
    }

    fn join_worker_failure(&mut self) -> Option<SupervisedProcessSpawnFailure> {
        let worker = self.worker.take()?;
        if !worker.is_finished() {
            self.worker = Some(worker);
            return None;
        }
        worker
            .join()
            .err()
            .map(|_| SupervisedProcessSpawnFailure::WorkerPanicked)
    }

    pub(crate) fn take_acquired(&mut self) -> Option<AcquiredSupervisedProcess> {
        self.acquired.take()
    }

    pub(crate) fn take_terminal(&mut self) -> Option<SupervisedProcessSpawnTerminal> {
        self.terminal.take()
    }

    pub(crate) const fn retirement(&self) -> Option<SupervisedProcessSpawnRetirement> {
        self.retirement
    }

    pub(crate) const fn cleanup_incomplete(&self) -> bool {
        self.cleanup_incomplete
    }

    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        if self.acquired.is_some() || self.terminal.is_some() || self.cleanup_incomplete {
            None
        } else if let Some(process) = self.late_process.as_ref() {
            process.next_deadline()
        } else if self.retirement.is_none() {
            Some(self.spawn_deadline)
        } else {
            None
        }
    }
}

impl Drop for SupervisedProcessSpawnOwner {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take()
            && worker.is_finished()
        {
            let _ = worker.join();
        }
    }
}
