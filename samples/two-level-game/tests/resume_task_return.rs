#![cfg(feature = "local-player")]
#![forbid(unsafe_code)]

#[path = "support/resume_gameplay.rs"]
#[allow(dead_code)]
mod support;

use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    thread,
    time::{Duration, Instant},
};

use nara::tasks::{
    TaskCancellationReason, TaskDomainKey, TaskFailure, TaskHandle, TaskPoolKind, TaskPools,
    TaskReturnState, TaskSpawnRequest, TaskTerminal,
};
use support::{DEADLINE, Game, MARKER, action, movement};
use two_level_game::{GameResumeError, capture_resume, request_resume, resume::encode_resume};

#[derive(Default)]
struct Gate {
    released: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    fn wait(&self) {
        let (released, _) = self
            .changed
            .wait_timeout_while(self.released.lock().unwrap(), DEADLINE, |released| {
                !*released
            })
            .unwrap();
        assert!(*released, "the original task gate was not released");
    }

    fn release(&self) {
        *self.released.lock().unwrap() = true;
        self.changed.notify_all();
    }
}

struct Release(Arc<Gate>);

impl Default for Release {
    fn default() -> Self {
        Self(Arc::new(Gate::default()))
    }
}

impl Drop for Release {
    fn drop(&mut self) {
        self.0.release();
    }
}

struct HeldCapture {
    entered: SyncSender<()>,
    release: Arc<Gate>,
}

impl Drop for HeldCapture {
    fn drop(&mut self) {
        let _ = self.entered.send(());
        self.release.wait();
    }
}

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + DEADLINE;
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "original task observation timed out"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

fn request(domain: u64) -> TaskSpawnRequest {
    TaskSpawnRequest::new(1, TaskDomainKey::new(domain))
}

#[test]
fn cancelled_capture_disposal_refuses_game_state_until_original_physical_return() {
    let mut original = Game::source();
    original.command(action(MARKER));
    original.command(movement(1, true));
    let state = original.capture();
    let frozen = original.frozen();
    let workers = original
        .session
        .observe(|world| {
            world
                .resource::<TaskPools>()
                .config()
                .kind(TaskPoolKind::Io)
                .workers()
                .get()
        })
        .unwrap();
    let blockers = Release::default();
    let (entered, started) = mpsc::sync_channel(workers);
    let running: Vec<TaskHandle<()>> = original
        .session
        .observe(|world| {
            let pools = world.resource::<TaskPools>();
            (0..workers)
                .map(|index| {
                    let entered = entered.clone();
                    let release = Arc::clone(&blockers.0);
                    pools
                        .spawn(TaskPoolKind::Io, request(index as u64), move |_| {
                            entered.send(()).unwrap();
                            release.wait();
                        })
                        .into_handle()
                        .unwrap()
                })
                .collect()
        })
        .unwrap();
    for _ in 0..workers {
        started.recv_timeout(DEADLINE).unwrap();
    }

    let disposal = Release::default();
    let (entered, disposing) = mpsc::sync_channel(1);
    let capture = HeldCapture {
        entered,
        release: Arc::clone(&disposal.0),
    };
    let executed = Arc::new(AtomicBool::new(false));
    let body = Arc::clone(&executed);
    let mut cancelled = original
        .session
        .observe(|world| {
            world
                .resource::<TaskPools>()
                .spawn(TaskPoolKind::Io, request(10_000), move |_| {
                    body.store(true, Ordering::Release);
                    drop(capture);
                })
                .into_handle()
                .unwrap()
        })
        .unwrap();
    let returned = cancelled.return_observer();
    assert!(cancelled.cancel());
    assert!(
        matches!(cancelled.try_take(), Some(TaskTerminal::Cancelled(reason))
        if reason.before_start && reason.reason == TaskCancellationReason::Requested)
    );
    drop(cancelled);
    blockers.0.release();
    disposing.recv_timeout(DEADLINE).unwrap();
    wait_until(|| {
        running
            .iter()
            .all(|handle| handle.return_observer().state() == TaskReturnState::Returned)
    });
    wait_until(|| {
        original
            .session
            .observe(|world| {
                let stats = world.resource::<TaskPools>().stats();
                TaskPoolKind::ALL.into_iter().all(|kind| {
                    let stats = stats.for_kind(kind);
                    stats.queued == 0 && stats.running == 0
                })
            })
            .unwrap()
    });
    assert_eq!(returned.state(), TaskReturnState::Pending);
    assert!(!executed.load(Ordering::Acquire));
    assert!(
        matches!(capture_resume(&original.session), Err(GameResumeError::Ineligible(reason))
        if reason.contains("task")),
        "cancelled capture disposal is not a completed authoritative task"
    );
    assert_eq!(original.tick(), frozen.tick);
    assert_eq!(
        original.session.runtime_generation(),
        Some(frozen.generation)
    );
    assert_eq!(original.snapshot().current_instance, Some(frozen.instance));

    disposal.0.release();
    wait_until(|| returned.state() == TaskReturnState::Returned);
    wait_until(|| {
        original
            .session
            .observe(|world| {
                world.resource::<TaskPools>().return_state() == TaskReturnState::Returned
            })
            .unwrap()
    });
    assert_eq!(original.frozen(), frozen);
    assert_eq!(
        encode_resume(&original.capture()).unwrap(),
        encode_resume(&state).unwrap()
    );
    let mut candidate = Game::source();
    request_resume(&mut candidate.session, &state).unwrap();
    candidate.settle();
    assert_eq!(
        encode_resume(&candidate.capture()).unwrap(),
        encode_resume(&state).unwrap()
    );
    let a = original.command(movement(1, false));
    let b = candidate.command(movement(1, false));
    assert_eq!(a.logical_tick, state.clock.ticks + 1);
    assert_eq!(a.player, b.player);
    assert_eq!(
        encode_resume(&original.capture()).unwrap(),
        encode_resume(&candidate.capture()).unwrap()
    );
    candidate.close();
    original.close();
}

struct FailedCapture;

impl Drop for FailedCapture {
    fn drop(&mut self) {
        panic!("intentional original task capture cleanup failure");
    }
}

#[test]
fn returned_task_cleanup_failure_refuses_capture_and_preserves_failed_physical_close() {
    let mut original = Game::source();
    original.command(action(MARKER));
    let before = original.command(movement(1, true));
    let generation = original.session.runtime_generation();
    let queue = original.queue();
    let capture = FailedCapture;
    let mut task = original
        .session
        .observe(|world| {
            world
                .resource::<TaskPools>()
                .spawn(TaskPoolKind::Io, request(20_000), move |_| {
                    drop(capture);
                    7_u8
                })
                .into_handle()
                .unwrap()
        })
        .unwrap();
    wait_until(|| task.return_observer().state() == TaskReturnState::CleanupFailed);
    wait_until(|| {
        original
            .session
            .observe(|world| {
                world.resource::<TaskPools>().return_state() == TaskReturnState::CleanupFailed
            })
            .unwrap()
    });
    original.expect_failed_close();
    assert!(matches!(
        task.try_take(),
        Some(TaskTerminal::Failed(TaskFailure::Panicked))
    ));
    drop(task);
    for kind in TaskPoolKind::ALL {
        let stats = original
            .session
            .observe(|world| world.resource::<TaskPools>().stats().for_kind(kind))
            .unwrap();
        assert_eq!(stats.queued, 0);
        assert_eq!(stats.running, 0);
    }
    assert!(
        matches!(capture_resume(&original.session), Err(GameResumeError::Ineligible(reason))
        if reason.contains("task") && reason.contains("cleanup"))
    );
    let unchanged = original.snapshot();
    assert_eq!(original.session.runtime_generation(), generation);
    assert_eq!(original.queue(), queue);
    assert_eq!(unchanged.current_instance, before.current_instance);
    assert_eq!(unchanged.player, before.player);
    assert_eq!(unchanged.held, before.held);
    assert_eq!(unchanged.logical_tick, before.logical_tick);
    assert_eq!(unchanged.markers, before.markers);
    assert!(unchanged.error.is_none());
    assert_eq!(original.session.step().unwrap(), None);
    let continued = original.snapshot();
    assert_eq!(continued.logical_tick, before.logical_tick + 1);
    assert!(continued.player.x > before.player.x);
    assert!(continued.error.is_none());
    assert!(
        matches!(capture_resume(&original.session), Err(GameResumeError::Ineligible(reason))
        if reason.contains("cleanup"))
    );
    original.close();
    assert!(original.session.is_closed());
}
