#![cfg(not(target_arch = "wasm32"))]

use std::{
    future::{pending, poll_fn},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, SyncSender},
    },
    task::Poll,
    thread,
    time::{Duration, Instant},
};

use bevy_tasks::futures_lite::future::yield_now;
use nara_core::{ItemLimit, TimeLimit};

use super::*;
use crate::{App, RuntimeControl, RuntimeControlRequestResult, RuntimeInstance};

mod ordered_results;
mod runtime_close;

fn config(pending: usize) -> TaskPoolConfig {
    let kind = TaskKindConfig::new(ItemLimit::ONE, ItemLimit::new(pending).unwrap());
    let phase = TimeLimit::new(Duration::from_millis(30)).unwrap();
    TaskPoolConfig::new(
        kind,
        kind,
        kind,
        TaskShutdownPolicy::new(phase, phase, phase),
    )
    .unwrap()
}

fn request(tick: u64) -> TaskSpawnRequest {
    TaskSpawnRequest::new(tick, TaskDomainKey::new(1))
}

fn wait_until(mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(
            Instant::now() < deadline,
            "task did not progress before deadline"
        );
        thread::sleep(Duration::from_millis(1));
    }
}

struct HeldCapture {
    started: SyncSender<()>,
    released: mpsc::Receiver<()>,
}

impl Drop for HeldCapture {
    fn drop(&mut self) {
        self.started.send(()).unwrap();
        let _ = self.released.recv_timeout(Duration::from_secs(5));
    }
}

#[test]
fn a_bevy_future_yields_and_resumes_in_its_selected_pool() {
    let pools = TaskPools::try_new_lazy(config(1)).unwrap();
    assert_eq!(
        pools
            .stats()
            .for_kind(TaskPoolKind::AsyncCompute)
            .live_workers,
        0
    );
    let owner = thread::current().id();
    let mut task = pools
        .spawn_future(
            TaskPoolKind::AsyncCompute,
            request(1),
            move |_| async move {
                let worker = thread::current();
                assert_ne!(worker.id(), owner);
                assert!(
                    worker
                        .name()
                        .unwrap()
                        .starts_with("nara-task-async_compute")
                );
                yield_now().await;
                42
            },
        )
        .into_handle()
        .unwrap();
    wait_until(|| task.return_observer().state() == TaskReturnState::Returned);
    assert_eq!(task.try_take(), Some(TaskTerminal::Completed(42)));
    assert_eq!(task.try_take(), None);
    assert_eq!(
        pools.stats().for_kind(TaskPoolKind::Compute).live_workers,
        0
    );
    let shutdown = pools.shutdown_blocking().unwrap();
    assert!(shutdown.is_complete());
    assert_eq!(
        shutdown.for_kind(TaskPoolKind::AsyncCompute).joined_workers,
        1
    );
}

#[test]
fn cancellation_wakes_a_suspended_future_and_waits_for_capture_disposal() {
    let pools = TaskPools::try_new_lazy(config(1)).unwrap();
    let (started, first_poll) = mpsc::sync_channel(1);
    let (dropping, disposal_started) = mpsc::sync_channel(1);
    let (release, released) = mpsc::channel();
    let capture = HeldCapture {
        started: dropping,
        released,
    };
    let mut handle = pools
        .spawn_future(TaskPoolKind::Io, request(1), |_| async move {
            started.send(()).unwrap();
            pending::<()>().await;
            drop(capture);
        })
        .into_handle()
        .unwrap();
    first_poll.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(handle.cancel());
    assert_eq!(
        handle.try_take(),
        Some(TaskTerminal::Cancelled(TaskCancellation {
            reason: TaskCancellationReason::Requested,
            before_start: false,
        }))
    );
    disposal_started
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    assert_eq!(handle.return_observer().state(), TaskReturnState::Pending);
    assert_eq!(pools.return_state(), TaskReturnState::Pending);
    release.send(()).unwrap();
    wait_until(|| pools.return_state() == TaskReturnState::Returned);
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn suspended_futures_share_a_worker_and_still_count_against_admission() {
    let pools = TaskPools::try_new_lazy(config(1)).unwrap();
    let (entered, first) = mpsc::sync_channel(1);
    let first_handle = pools
        .spawn_future(TaskPoolKind::Io, request(1), |_| async move {
            entered.send(()).unwrap();
            pending::<()>().await;
        })
        .into_handle()
        .unwrap();
    first.recv_timeout(Duration::from_secs(5)).unwrap();
    let (entered, second) = mpsc::sync_channel(1);
    let second_handle = pools
        .spawn_future(TaskPoolKind::Io, request(2), |_| async move {
            entered.send(()).unwrap();
            pending::<()>().await;
        })
        .into_handle()
        .unwrap();
    second.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(
        pools
            .spawn(TaskPoolKind::Io, request(3), |_| ())
            .into_handle()
            .unwrap_err()
            .reason,
        TaskRejectReason::QueueFull { capacity: 1 }
    );
    assert!(first_handle.cancel());
    assert!(second_handle.cancel());
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn admission_bounds_pending_work_and_replaces_only_a_matching_queued_task() {
    let pools = TaskPools::inline_for_tests(config(1)).unwrap();
    let ran = Arc::new(AtomicBool::new(false));
    let original = Arc::clone(&ran);
    let coalescing =
        request(1).with_overload(TaskOverloadPolicy::CoalescePending(TaskCoalesceKey::new(9)));
    let mut previous = pools
        .spawn(TaskPoolKind::Io, coalescing, move |_| {
            original.store(true, Ordering::Relaxed)
        })
        .into_handle()
        .unwrap();
    assert_eq!(
        pools
            .spawn(TaskPoolKind::Io, request(2), |_| ())
            .into_handle()
            .unwrap_err()
            .reason,
        TaskRejectReason::QueueFull { capacity: 1 }
    );
    let replacement = pools.spawn(TaskPoolKind::Io, coalescing, |_| 3);
    assert!(
        matches!(&replacement, TaskSpawnOutcome::Coalesced { replaced, .. } if *replaced == previous.id())
    );
    let mut replacement = replacement.into_handle().unwrap();
    assert_eq!(
        previous.try_take(),
        Some(TaskTerminal::Cancelled(TaskCancellation {
            reason: TaskCancellationReason::Coalesced {
                replacement: replacement.id()
            },
            before_start: true,
        }))
    );
    assert_eq!(
        previous.return_observer().state(),
        TaskReturnState::Returned
    );
    assert_eq!(pools.run_pending_for_tests().executed, 1);
    assert_eq!(replacement.try_take(), Some(TaskTerminal::Completed(3)));
    assert!(!ran.load(Ordering::Relaxed));
}

#[test]
fn cancelled_pending_work_holds_capture_custody_without_queue_demand() {
    let pools = TaskPools::inline_for_tests(config(1)).unwrap();
    let handle = pools
        .spawn(TaskPoolKind::Io, request(1), |_| ())
        .into_handle()
        .unwrap();
    let returned = handle.return_observer();
    assert!(handle.cancel());
    drop(handle);
    assert_eq!(pools.stats().for_kind(TaskPoolKind::Io).queued, 0);
    assert_eq!(pools.return_state(), TaskReturnState::Pending);
    assert_eq!(returned.state(), TaskReturnState::Pending);
    assert_eq!(pools.run_pending_for_tests().cancelled_before_start, 1);
    assert_eq!(returned.state(), TaskReturnState::Returned);
}

#[test]
fn scope_close_retires_pending_captures_without_taking_siblings_or_workers() {
    let facilities = TaskFacilities::try_new_lazy(config(2)).unwrap();
    let mut closing = facilities.scope();
    let sibling = facilities.scope();
    let first = closing.spawner();
    let second = sibling.spawner();
    let (entered, started) = mpsc::sync_channel(1);
    let (release_worker, worker_held) = mpsc::channel();
    let blocking = second
        .spawn(TaskPoolKind::Io, request(1), move |_| {
            entered.send(()).unwrap();
            let _ = worker_held.recv_timeout(Duration::from_secs(5));
        })
        .into_handle()
        .unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let (entered, disposed) = mpsc::sync_channel(1);
    let (release, held) = mpsc::channel();
    let capture = HeldCapture {
        started: entered,
        released: held,
    };
    let mut retired = first
        .spawn(TaskPoolKind::Io, request(2), move |_| drop(capture))
        .into_handle()
        .unwrap();
    closing.begin_close();
    disposed.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(matches!(
        retired.try_take(),
        Some(TaskTerminal::Cancelled(TaskCancellation {
            reason: TaskCancellationReason::ScopeClosed,
            before_start: true,
        }))
    ));
    assert_eq!(closing.poll_close().pending, 1);
    assert_eq!(
        first
            .spawn(TaskPoolKind::Compute, request(3), |_| ())
            .into_handle()
            .unwrap_err()
            .reason,
        TaskRejectReason::ScopeClosed
    );
    let subsequent = second
        .spawn(TaskPoolKind::Io, request(3), |_| ())
        .into_handle()
        .unwrap();
    release.send(()).unwrap();
    wait_until(|| closing.poll_close().is_complete());
    assert_eq!(
        facilities.stats().for_kind(TaskPoolKind::Io).live_workers,
        1
    );
    assert_eq!(blocking.return_observer().state(), TaskReturnState::Pending);
    assert!(!subsequent.is_finished());
    release_worker.send(()).unwrap();
    assert!(facilities.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn physical_shutdown_times_out_and_retries_after_uncooperative_work_returns() {
    let pools = TaskPools::try_new_lazy(config(1)).unwrap();
    let (entered, started) = mpsc::sync_channel(1);
    let (release, held) = mpsc::channel();
    let mut task = pools
        .spawn(TaskPoolKind::Io, request(1), move |_| {
            entered.send(()).unwrap();
            let _ = held.recv_timeout(Duration::from_secs(5));
            7
        })
        .into_handle()
        .unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let original = pools.shutdown_blocking().unwrap();
    assert!(!original.is_complete());
    assert!(original.timed_out());
    assert_eq!(task.return_observer().state(), TaskReturnState::Pending);
    assert_eq!(
        task.try_take(),
        Some(TaskTerminal::Cancelled(TaskCancellation {
            reason: TaskCancellationReason::PoolShutdown,
            before_start: false,
        }))
    );
    assert_eq!(
        pools
            .spawn(TaskPoolKind::Io, request(2), |_| ())
            .into_handle()
            .unwrap_err()
            .reason,
        TaskRejectReason::PoolClosed
    );
    release.send(()).unwrap();
    wait_until(|| pools.poll_shutdown().unwrap().is_complete());
    let retry = pools.poll_shutdown().unwrap();
    assert!(retry.timed_out());
    assert_eq!(retry.for_kind(TaskPoolKind::Io).joined_workers, 1);
    assert_eq!(task.return_observer().state(), TaskReturnState::Returned);
}

#[test]
fn plugin_close_reports_the_original_facility_after_world_access() {
    let mut app = App::new();
    app.add_plugins(plugin(config(1))).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let pools = runtime.world().resource::<TaskPools>();
    assert_eq!(
        pools.poll_shutdown(),
        Err(TaskShutdownError::CloseOwnerTransferred)
    );
    let task = pools
        .spawn(TaskPoolKind::Io, request(1), |_| ())
        .into_handle()
        .unwrap();
    wait_until(|| task.return_observer().state() == TaskReturnState::Returned);
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    wait_until(|| {
        runtime.drive(Duration::ZERO).unwrap();
        !runtime.world().contains_resource::<TaskPools>()
    });
    let report = runtime.world().resource::<TaskShutdownReport>();
    assert!(report.is_complete());
    assert_eq!(report.for_kind(TaskPoolKind::Io).joined_workers, 1);
}

#[test]
fn a_task_panic_reports_cleanup_failure_and_keeps_other_work_runnable() {
    let pools = TaskPools::try_new_lazy(config(2)).unwrap();
    let mut panicked = pools
        .spawn_future(TaskPoolKind::Io, request(1), |_| async {
            yield_now().await;
            panic!("test task panic");
        })
        .into_handle()
        .unwrap();
    let next = pools
        .spawn_future(TaskPoolKind::Io, request(2), |_| {
            poll_fn(|_| Poll::Ready(6))
        })
        .into_handle()
        .unwrap();
    wait_until(|| pools.return_state() == TaskReturnState::CleanupFailed);
    assert_eq!(
        panicked.try_take(),
        Some(TaskTerminal::Failed(TaskFailure::Panicked))
    );
    assert_eq!(next.return_observer().state(), TaskReturnState::Returned);
    let report = pools.shutdown_blocking().unwrap();
    assert!(report.is_complete());
    assert!(report.has_cleanup_failure());
}
