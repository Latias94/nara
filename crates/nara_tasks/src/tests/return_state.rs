use std::{
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

use super::*;

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(
            Instant::now() < deadline,
            "executor-owned work did not return"
        );
        thread::yield_now();
    }
}

struct HeldDrop {
    entered: mpsc::Sender<thread::ThreadId>,
    release: mpsc::Receiver<()>,
}

impl Drop for HeldDrop {
    fn drop(&mut self) {
        let _ = self.entered.send(thread::current().id());
        let _ = self.release.recv_timeout(Duration::from_secs(10));
    }
}

fn held_drop() -> (HeldDrop, mpsc::Receiver<thread::ThreadId>, mpsc::Sender<()>) {
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    (
        HeldDrop {
            entered,
            release: released,
        },
        started,
        release,
    )
}

struct DropNotice(mpsc::Sender<thread::ThreadId>);

impl Drop for DropNotice {
    fn drop(&mut self) {
        let _ = self.0.send(thread::current().id());
    }
}

#[test]
fn idle_threaded_and_lazy_snapshots_preserve_admission_and_live_workers() {
    for pools in [
        TaskPools::try_new(test_config(1)).unwrap(),
        TaskPools::try_new_lazy(test_config(1)).unwrap(),
    ] {
        assert_eq!(pools.return_state(), TaskReturnState::Returned);
        let mut tasks =
            TaskPoolKind::ALL.map(|kind| accepted(pools.spawn(kind, request(1), move |_| kind)));
        wait_until(|| pools.return_state() != TaskReturnState::Pending);
        assert_eq!(pools.return_state(), TaskReturnState::Returned);
        for (kind, task) in TaskPoolKind::ALL.into_iter().zip(&mut tasks) {
            assert_eq!(task.try_take(), Some(TaskTerminal::Completed(kind)));
            let stats = pools.stats().for_kind(kind);
            assert_eq!(stats.live_workers, 1);
            assert_eq!(stats.shutdowns, 0);
        }
        let report = pools.shutdown_blocking().unwrap();
        assert!(report.is_complete());
        assert!(!report.has_cleanup_failure());
        for kind in TaskPoolKind::ALL {
            assert_eq!(report.for_kind(kind).joined_workers, 1);
        }
        assert_eq!(pools.return_state(), TaskReturnState::Returned);
    }
}

#[test]
fn cancelled_inline_entries_stay_pending_after_terminal_and_observer_drop() {
    let pools = inline_pools(1);
    assert_eq!(pools.return_state(), TaskReturnState::Returned);
    for kind in TaskPoolKind::ALL {
        let (dropped, observation) = mpsc::channel();
        let capture = DropNotice(dropped);
        let mut task = accepted(pools.spawn(kind, request(1), move |_| drop(capture)));
        let returned = task.return_observer();
        assert_eq!(pools.return_state(), TaskReturnState::Pending);
        assert_eq!(pools.stats().for_kind(kind).queued, 1);
        assert!(task.cancel());
        let terminal = task.try_take().unwrap();
        assert!(matches!(
            &terminal,
            TaskTerminal::Cancelled(TaskCancellation {
                before_start: true,
                ..
            })
        ));
        assert_eq!(pools.stats().for_kind(kind).queued, 0);
        assert_eq!(pools.stats().for_kind(kind).running, 0);
        assert_eq!(pools.return_state(), TaskReturnState::Pending);
        assert_eq!(returned.state(), TaskReturnState::Pending);
        drop(task);
        drop(returned);
        assert!(observation.try_recv().is_err());
        assert_eq!(pools.return_state(), TaskReturnState::Pending);
        assert_eq!(pools.run_pending_for_tests().cancelled_before_start, 1);
        assert_eq!(
            observation.recv_timeout(Duration::from_secs(10)).unwrap(),
            thread::current().id()
        );
        assert_eq!(pools.return_state(), TaskReturnState::Returned);
        assert_eq!(terminal.state(), TaskTerminalState::Cancelled);
    }
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

fn observe_cancelled_threaded_capture(panic_on_drop: bool) {
    let pools = TaskPools::try_new(test_config(2)).unwrap();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let running = accepted(pools.spawn(TaskPoolKind::Io, request(1), move |_| {
        entered.send(()).unwrap();
        let _ = released.recv_timeout(Duration::from_secs(10));
    }));
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(pools.return_state(), TaskReturnState::Pending);

    let (capture, disposal_started, dispose) = held_drop();
    let capture = (capture, panic_on_drop.then(|| PanicOnDrop));
    let mut pending = accepted(pools.spawn(TaskPoolKind::Io, request(2), move |_| drop(capture)));
    let returned = pending.return_observer();
    assert!(pending.cancel());
    let terminal = pending.try_take().unwrap();
    assert!(matches!(
        &terminal,
        TaskTerminal::Cancelled(TaskCancellation {
            before_start: true,
            ..
        })
    ));
    release.send(()).unwrap();
    assert_ne!(
        disposal_started
            .recv_timeout(Duration::from_secs(10))
            .unwrap(),
        thread::current().id()
    );
    assert_eq!(running.return_observer().state(), TaskReturnState::Returned);
    for kind in TaskPoolKind::ALL {
        let stats = pools.stats().for_kind(kind);
        assert_eq!(stats.queued, 0);
        assert_eq!(stats.running, 0);
        assert_eq!(stats.shutdowns, 0);
        assert_eq!(stats.live_workers, 1);
    }
    assert_eq!(returned.state(), TaskReturnState::Pending);
    assert_eq!(pools.return_state(), TaskReturnState::Pending);
    drop(pending);
    drop(returned);
    assert_eq!(pools.return_state(), TaskReturnState::Pending);

    dispose.send(()).unwrap();
    wait_until(|| pools.return_state() != TaskReturnState::Pending);
    let expected = if panic_on_drop {
        TaskReturnState::CleanupFailed
    } else {
        TaskReturnState::Returned
    };
    assert_eq!(pools.return_state(), expected);
    assert_eq!(terminal.state(), TaskTerminalState::Cancelled);
    assert_eq!(pools.has_cleanup_failure(), panic_on_drop);
    let mut next = accepted(pools.spawn(TaskPoolKind::Io, request(3), |_| 7));
    wait_until(|| pools.return_state() != TaskReturnState::Pending);
    assert_eq!(next.try_take(), Some(TaskTerminal::Completed(7)));
    assert_eq!(pools.return_state(), expected);
    let report = pools.shutdown_blocking().unwrap();
    assert!(report.is_complete());
    assert_eq!(report.has_cleanup_failure(), panic_on_drop);
    assert_eq!(pools.return_state(), expected);
}

#[test]
fn cancelled_threaded_capture_disposal_stays_pending_with_zero_work_stats() {
    observe_cancelled_threaded_capture(false);
}

#[test]
fn cancelled_capture_cleanup_failure_survives_handle_drop_and_original_shutdown() {
    observe_cancelled_threaded_capture(true);
}

#[test]
fn root_return_waits_for_original_running_custody_after_task_return() {
    let pools = TaskPools::try_new(test_config(1)).unwrap();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    pools.set_after_task_return_hook_for_tests(TaskPoolKind::Io, move || {
        entered.send(()).unwrap();
        let _ = released.recv_timeout(Duration::from_secs(10));
    });
    let mut task = accepted(pools.spawn(TaskPoolKind::Io, request(1), |_| 7));
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(task.return_observer().state(), TaskReturnState::Returned);
    assert_eq!(task.try_take(), Some(TaskTerminal::Completed(7)));
    drop(task);
    assert_eq!(pools.stats().for_kind(TaskPoolKind::Io).running, 1);
    assert_eq!(pools.return_state(), TaskReturnState::Pending);
    release.send(()).unwrap();
    wait_until(|| pools.return_state() != TaskReturnState::Pending);
    assert_eq!(pools.return_state(), TaskReturnState::Returned);
    assert!(pools.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn root_return_observes_one_cut_during_compute_to_io_child_handoff() {
    let pools = Arc::new(TaskPools::try_new(test_config(2)).unwrap());
    let (parent_entered, parent_started) = mpsc::channel();
    let (handoff, handoff_released) = mpsc::channel();
    let (child_entered, child_started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let child_pools = Arc::clone(&pools);
    let mut parent = accepted(pools.spawn(TaskPoolKind::Compute, request(1), move |_| {
        parent_entered.send(()).unwrap();
        handoff_released
            .recv_timeout(Duration::from_secs(10))
            .unwrap();
        accepted(child_pools.spawn(TaskPoolKind::Io, request(2), move |_| {
            child_entered.send(()).unwrap();
            let _ = released.recv_timeout(Duration::from_secs(10));
        }))
    }));
    parent_started
        .recv_timeout(Duration::from_secs(10))
        .unwrap();
    let parent_returned = parent.return_observer();
    let observed_pools = Arc::clone(&pools);
    pools.set_after_io_return_observation_hook_for_tests(move || {
        handoff.send(()).unwrap();
        child_started.recv_timeout(Duration::from_secs(10)).unwrap();
        wait_until(|| {
            observed_pools
                .stats()
                .for_kind(TaskPoolKind::Compute)
                .running
                == 0
        });
    });

    let during_handoff = pools.return_state();
    let Some(TaskTerminal::Completed(child)) = parent.try_take() else {
        panic!("parent did not publish its admitted child");
    };
    assert_eq!(parent_returned.state(), TaskReturnState::Returned);
    assert_eq!(child.return_observer().state(), TaskReturnState::Pending);
    assert_eq!(pools.return_state(), TaskReturnState::Pending);
    release.send(()).unwrap();
    wait_until(|| pools.return_state() != TaskReturnState::Pending);
    assert_eq!(child.return_observer().state(), TaskReturnState::Returned);
    assert_eq!(pools.return_state(), TaskReturnState::Returned);
    let report = pools.shutdown_blocking().unwrap();
    assert!(report.is_complete());
    assert!(!report.has_cleanup_failure());
    assert_eq!(
        during_handoff,
        TaskReturnState::Pending,
        "root observation lost continuously owned work during Compute-to-Io handoff"
    );
}

#[test]
fn root_return_stays_pending_through_bounded_cross_kind_child_handoffs() {
    const LINKS: usize = 128;

    fn admit_link(
        pools: Arc<TaskPools>,
        index: usize,
        last_started: mpsc::Sender<()>,
        release: mpsc::Receiver<()>,
    ) {
        let child_pools = Arc::clone(&pools);
        let kind = TaskPoolKind::ALL[index % TaskPoolKind::ALL.len()];
        drop(accepted(pools.spawn(
            kind,
            request(index as u64),
            move |_| {
                if index + 1 < LINKS {
                    admit_link(child_pools, index + 1, last_started, release);
                } else {
                    last_started.send(()).unwrap();
                    let _ = release.recv_timeout(Duration::from_secs(10));
                }
            },
        )));
    }

    let pools = Arc::new(TaskPools::try_new(test_config(2)).unwrap());
    let (start, started) = mpsc::channel();
    let (last_started, last_entered) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let child_pools = Arc::clone(&pools);
    let first = accepted(pools.spawn(TaskPoolKind::Io, request(0), move |_| {
        let _ = started.recv_timeout(Duration::from_secs(10));
        admit_link(child_pools, 1, last_started, released);
    }));
    assert_eq!(pools.return_state(), TaskReturnState::Pending);
    start.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        assert_eq!(pools.return_state(), TaskReturnState::Pending);
        match last_entered.try_recv() {
            Ok(()) => break,
            Err(mpsc::TryRecvError::Empty) => {
                assert!(
                    Instant::now() < deadline,
                    "child handoff chain did not finish"
                );
                thread::yield_now();
            }
            Err(mpsc::TryRecvError::Disconnected) => panic!("child handoff chain failed"),
        }
    }
    assert!(first.is_finished());
    assert_eq!(pools.return_state(), TaskReturnState::Pending);
    release.send(()).unwrap();
    wait_until(|| pools.return_state() != TaskReturnState::Pending);
    assert_eq!(pools.return_state(), TaskReturnState::Returned);
    let report = pools.shutdown_blocking().unwrap();
    assert!(report.is_complete());
    assert!(!report.has_cleanup_failure());
}

#[test]
fn root_pending_returns_take_precedence_over_previous_cleanup_failure() {
    let pools = inline_pools(1);
    let capture = PanicOnDrop;
    let failed = accepted(pools.spawn(TaskPoolKind::Io, request(1), move |_| drop(capture)));
    assert_eq!(pools.run_pending_for_tests().executed, 1);
    assert_eq!(pools.return_state(), TaskReturnState::CleanupFailed);
    drop(failed);

    let pending = accepted(pools.spawn(TaskPoolKind::AsyncCompute, request(2), |_| ()));
    assert!(pending.cancel());
    drop(pending);
    assert!(pools.has_cleanup_failure());
    assert_eq!(pools.stats().for_kind(TaskPoolKind::AsyncCompute).queued, 0);
    assert_eq!(pools.return_state(), TaskReturnState::Pending);
    assert_eq!(pools.run_pending_for_tests().cancelled_before_start, 1);
    assert_eq!(pools.return_state(), TaskReturnState::CleanupFailed);
    let report = pools.shutdown_blocking().unwrap();
    assert!(report.is_complete());
    assert!(report.has_cleanup_failure());
}

#[test]
fn scope_return_counts_own_disposal_and_excludes_a_held_sibling() {
    let facilities = TaskFacilities::try_new(test_config(2)).unwrap();
    let mut own = facilities.scope();
    let mut sibling = facilities.scope();
    let own_spawner = own.spawner();
    let sibling_spawner = sibling.spawner();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let running = accepted(
        sibling_spawner.spawn(TaskPoolKind::Io, request(1), move |_| {
            entered.send(()).unwrap();
            let _ = released.recv_timeout(Duration::from_secs(10));
        }),
    );
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(own_spawner.return_state(), TaskReturnState::Returned);
    assert_eq!(sibling_spawner.return_state(), TaskReturnState::Pending);

    let (capture, disposal_started, dispose) = held_drop();
    let mut pending =
        accepted(own_spawner.spawn(TaskPoolKind::Io, request(2), move |_| drop(capture)));
    let returned = pending.return_observer();
    assert_eq!(own_spawner.return_state(), TaskReturnState::Pending);
    own.begin_close();
    assert_ne!(
        disposal_started
            .recv_timeout(Duration::from_secs(10))
            .unwrap(),
        thread::current().id()
    );
    assert!(matches!(
        pending.try_take(),
        Some(TaskTerminal::Cancelled(TaskCancellation {
            reason: TaskCancellationReason::ScopeClosed,
            before_start: true,
        }))
    ));
    assert_eq!(returned.state(), TaskReturnState::Pending);
    drop(pending);
    drop(returned);
    assert_eq!(own_spawner.return_state(), TaskReturnState::Pending);
    assert_eq!(facilities.stats().for_kind(TaskPoolKind::Io).queued, 0);
    assert_eq!(facilities.stats().for_kind(TaskPoolKind::Io).running, 1);

    dispose.send(()).unwrap();
    wait_until(|| own_spawner.return_state() != TaskReturnState::Pending);
    assert_eq!(own_spawner.return_state(), TaskReturnState::Returned);
    assert!(own.poll_close().is_complete());
    assert_eq!(sibling_spawner.return_state(), TaskReturnState::Pending);
    assert_eq!(facilities.stats().for_kind(TaskPoolKind::Io).running, 1);
    release.send(()).unwrap();
    wait_until(|| sibling_spawner.return_state() != TaskReturnState::Pending);
    assert_eq!(running.return_observer().state(), TaskReturnState::Returned);
    assert!(sibling.poll_close().is_complete());
    let report = facilities.shutdown_blocking().unwrap();
    assert!(report.is_complete());
    assert!(!report.has_cleanup_failure());
}

#[test]
fn scope_return_preserves_own_cleanup_failure_without_poisoning_siblings() {
    let facilities = TaskFacilities::inline_for_tests(test_config(2)).unwrap();
    let mut failed = facilities.scope();
    let mut healthy = facilities.scope();
    let failed_spawner = failed.spawner();
    let healthy_spawner = healthy.spawner();
    let capture = PanicOnDrop;
    let task = accepted(failed_spawner.spawn(TaskPoolKind::Io, request(1), move |_| drop(capture)));
    let returned = task.return_observer();
    assert!(task.cancel());
    assert_eq!(failed_spawner.return_state(), TaskReturnState::Pending);
    assert_eq!(healthy_spawner.return_state(), TaskReturnState::Returned);
    assert_eq!(facilities.run_pending_for_tests().cancelled_before_start, 1);
    assert_eq!(returned.state(), TaskReturnState::CleanupFailed);
    assert_eq!(
        failed_spawner.return_state(),
        TaskReturnState::CleanupFailed
    );
    assert_eq!(healthy_spawner.return_state(), TaskReturnState::Returned);
    drop(task);
    drop(returned);
    assert_eq!(
        failed.spawner().return_state(),
        TaskReturnState::CleanupFailed
    );

    let mut next = accepted(failed_spawner.spawn(TaskPoolKind::Io, request(2), |_| 3));
    let mut sibling = accepted(healthy_spawner.spawn(TaskPoolKind::Io, request(3), |_| {
        Err::<(), _>("ordinary business error")
    }));
    assert_eq!(failed_spawner.return_state(), TaskReturnState::Pending);
    assert_eq!(healthy_spawner.return_state(), TaskReturnState::Pending);
    assert_eq!(facilities.run_pending_for_tests().executed, 2);
    assert_eq!(next.try_take(), Some(TaskTerminal::Completed(3)));
    assert_eq!(
        sibling.try_take(),
        Some(TaskTerminal::Completed(Err("ordinary business error")))
    );
    assert_eq!(
        failed_spawner.return_state(),
        TaskReturnState::CleanupFailed
    );
    assert_eq!(healthy_spawner.return_state(), TaskReturnState::Returned);
    assert!(failed.poll_close().cleanup_failed);
    let healthy_report = healthy.poll_close();
    assert!(healthy_report.is_complete());
    assert!(!healthy_report.cleanup_failed);
    let report = facilities.shutdown_blocking().unwrap();
    assert!(report.is_complete());
    assert!(report.has_cleanup_failure());
}

#[test]
fn published_terminal_output_remains_owned_after_root_return_and_shutdown() {
    let pools = inline_pools(2);
    let (dropped, observation) = mpsc::channel();
    let mut task =
        accepted(pools.spawn(TaskPoolKind::Io, request(1), move |_| DropNotice(dropped)));
    let returned = task.return_observer();
    assert_eq!(pools.run_pending_for_tests().executed, 1);
    assert_eq!(pools.return_state(), TaskReturnState::Returned);
    assert!(observation.try_recv().is_err());
    let terminal = task.try_take().unwrap();
    drop(task);
    assert_eq!(pools.return_state(), TaskReturnState::Returned);
    assert!(observation.try_recv().is_err());

    let discarded = accepted(pools.spawn(TaskPoolKind::Io, request(2), |_| PanicOnDrop));
    assert_eq!(pools.run_pending_for_tests().executed, 1);
    drop(discarded);
    assert_eq!(pools.return_state(), TaskReturnState::Returned);
    let report = pools.shutdown_blocking().unwrap();
    assert!(report.is_complete());
    assert!(!report.has_cleanup_failure());
    assert!(observation.try_recv().is_err());
    let dropper = thread::spawn(move || {
        drop(terminal);
        thread::current().id()
    })
    .join()
    .unwrap();
    assert_eq!(
        observation.recv_timeout(Duration::from_secs(10)).unwrap(),
        dropper
    );
    assert_eq!(returned.state(), TaskReturnState::Returned);
    assert_eq!(pools.return_state(), TaskReturnState::Returned);
}

#[test]
fn published_handle_output_remains_owned_after_scope_return_and_close() {
    let facilities = TaskFacilities::inline_for_tests(test_config(1)).unwrap();
    let mut scope = facilities.scope();
    let spawner = scope.spawner();
    let (dropped, observation) = mpsc::channel();
    let task = accepted(spawner.spawn(TaskPoolKind::Io, request(1), move |_| DropNotice(dropped)));
    let returned = task.return_observer();
    assert_eq!(facilities.run_pending_for_tests().executed, 1);
    assert_eq!(spawner.return_state(), TaskReturnState::Returned);
    assert!(scope.poll_close().is_complete());
    drop(scope);
    assert!(facilities.shutdown_blocking().unwrap().is_complete());
    assert_eq!(spawner.return_state(), TaskReturnState::Returned);
    assert!(observation.try_recv().is_err());
    drop(task);
    assert_eq!(
        observation.recv_timeout(Duration::from_secs(10)).unwrap(),
        thread::current().id()
    );
    assert_eq!(returned.state(), TaskReturnState::Returned);
    assert_eq!(spawner.return_state(), TaskReturnState::Returned);
}
