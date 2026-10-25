use std::{
    sync::{Arc, Barrier, mpsc},
    thread,
    time::{Duration, Instant},
};

use super::*;

fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !ready() {
        assert!(Instant::now() < deadline, "lazy worker did not return");
        thread::yield_now();
    }
}

fn workers(pools: &TaskPools, kind: TaskPoolKind) -> usize {
    pools.stats().for_kind(kind).live_workers
}

#[test]
fn lazy_facilities_activate_only_used_kinds_and_join_only_created_workers() {
    let pools = TaskPools::try_new_lazy(test_config(4)).unwrap();
    for kind in TaskPoolKind::ALL {
        assert_eq!(workers(&pools, kind), 0);
    }
    let handle = accepted(pools.spawn(TaskPoolKind::Io, request(1), |_| 3));
    wait_until(|| handle.return_observer().state() == TaskReturnState::Returned);
    assert!(handle.admission_to_start().is_some());
    assert_eq!(workers(&pools, TaskPoolKind::Io), 1);
    assert_eq!(workers(&pools, TaskPoolKind::Compute), 0);
    assert_eq!(workers(&pools, TaskPoolKind::AsyncCompute), 0);
    let report = pools.shutdown_blocking().unwrap();
    assert!(report.is_complete());
    assert_eq!(report.for_kind(TaskPoolKind::Io).joined_workers, 1);
    assert_eq!(report.for_kind(TaskPoolKind::Compute).joined_workers, 0);
}

#[test]
fn untouched_lazy_facilities_close_without_starting_a_worker() {
    let pools = TaskPools::try_new_lazy(test_config(4)).unwrap();
    let report = pools.shutdown_blocking().unwrap();
    assert!(report.is_complete());
    for kind in TaskPoolKind::ALL {
        assert_eq!(report.for_kind(kind).joined_workers, 0);
        assert_eq!(workers(&pools, kind), 0);
    }
    assert!(matches!(
        pools.spawn(TaskPoolKind::Io, request(1), |_| ()),
        TaskSpawnOutcome::Rejected(TaskRejection {
            reason: TaskRejectReason::PoolClosed,
            ..
        })
    ));
    assert_eq!(workers(&pools, TaskPoolKind::Io), 0);
}

#[test]
fn concurrent_first_submissions_start_one_configured_worker_set() {
    let config = test_config(32)
        .with_kind(TaskPoolKind::Io, TaskKindConfig::new(items(2), items(32)))
        .unwrap();
    let pools = Arc::new(TaskPools::try_new_lazy(config).unwrap());
    let barrier = Arc::new(Barrier::new(17));
    let mut submitters = Vec::new();
    for _ in 0..16 {
        let pools = pools.clone();
        let barrier = barrier.clone();
        submitters.push(thread::spawn(move || {
            barrier.wait();
            accepted(pools.spawn(TaskPoolKind::Io, request(1), |_| ()))
        }));
    }
    barrier.wait();
    let handles = submitters
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect::<Vec<_>>();
    wait_until(|| {
        handles
            .iter()
            .all(|handle| handle.return_observer().state() == TaskReturnState::Returned)
    });
    assert_eq!(workers(&pools, TaskPoolKind::Io), 2);
    let report = pools.shutdown_blocking().unwrap();
    assert!(report.is_complete());
    assert_eq!(report.for_kind(TaskPoolKind::Io).joined_workers, 2);
    assert_eq!(report.for_kind(TaskPoolKind::Compute).joined_workers, 0);
}

#[test]
fn partial_first_use_failure_rejects_without_fallback_and_retains_its_workers() {
    let config = test_config(4)
        .with_kind(TaskPoolKind::Io, TaskKindConfig::new(items(2), items(4)))
        .unwrap();
    let pools = TaskPools::try_new_lazy(config).unwrap();
    let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let calls = called.clone();
    let (outcome, probe) =
        TaskPools::with_worker_spawn_failure_for_tests(TaskPoolKind::Io, 1, || {
            pools.spawn(TaskPoolKind::Io, request(1), move |_| {
                calls.store(true, std::sync::atomic::Ordering::Release)
            })
        });
    assert!(matches!(
        outcome,
        TaskSpawnOutcome::Rejected(TaskRejection {
            reason: TaskRejectReason::WorkerStartupFailed,
            return_state: TaskReturnState::Returned,
            ..
        })
    ));
    assert_eq!(probe.started(), 1);
    assert!(!called.load(std::sync::atomic::Ordering::Acquire));
    assert!(matches!(
        pools.spawn(TaskPoolKind::Io, request(1), |_| ()),
        TaskSpawnOutcome::Rejected(TaskRejection {
            reason: TaskRejectReason::WorkerStartupFailed,
            ..
        })
    ));
    let independent = accepted(pools.spawn(TaskPoolKind::Compute, request(1), |_| 4));
    wait_until(|| independent.is_finished());
    let report = pools.shutdown_blocking().unwrap();
    assert!(report.is_complete());
    assert_eq!(report.for_kind(TaskPoolKind::Io).joined_workers, 1);
    assert_eq!(probe.exited(), 1);
    assert_eq!(pools.stats().for_kind(TaskPoolKind::Io).admitted, 0);
}

#[test]
fn already_closed_activity_cannot_activate_lazy_facilities() {
    let facilities = TaskFacilities::try_new_lazy(test_config(2)).unwrap();
    let mut scope = facilities.scope();
    let spawner = scope.spawner();
    assert!(scope.poll_close().is_complete());
    assert!(matches!(
        spawner.spawn(TaskPoolKind::Io, request(1), |_| ()),
        TaskSpawnOutcome::Rejected(TaskRejection {
            reason: TaskRejectReason::ScopeClosed,
            ..
        })
    ));
    for kind in TaskPoolKind::ALL {
        assert_eq!(facilities.stats().for_kind(kind).live_workers, 0);
    }
    assert!(facilities.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn physical_close_does_not_wait_for_or_lose_inflight_activation() {
    let policy = TaskShutdownPolicy::new(
        time(Duration::from_millis(1)),
        time(Duration::from_millis(1)),
        time(Duration::from_millis(5)),
    );
    let pools = Arc::new(
        TaskPools::try_new_lazy(test_config(2).with_shutdown_policy(policy).unwrap()).unwrap(),
    );
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    pools.set_before_worker_activation_hook_for_tests(move || {
        entered.send(()).unwrap();
        let _ = released.recv_timeout(Duration::from_secs(10));
    });
    let submitter = pools.clone();
    let submitting = thread::spawn(move || {
        submitter
            .spawn(TaskPoolKind::Io, request(1), |_| ())
            .into_handle()
    });
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    let before = Instant::now();
    let report = pools.shutdown_blocking().unwrap();
    let elapsed = before.elapsed();
    release.send(()).unwrap();
    assert!(!report.is_complete());
    assert!(elapsed < Duration::from_secs(1));
    assert_eq!(
        submitting.join().unwrap().unwrap_err().reason,
        TaskRejectReason::PoolClosed
    );
    wait_until(|| pools.shutdown_blocking().unwrap().is_complete());
    assert_eq!(workers(&pools, TaskPoolKind::Io), 0);
}

#[test]
fn activity_close_is_independent_of_unadmitted_first_use_activation() {
    let facilities = TaskFacilities::try_new_lazy(test_config(2)).unwrap();
    let mut scope = facilities.scope();
    let spawner = scope.spawner();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    spawner.set_before_worker_activation_hook_for_tests(move || {
        entered.send(()).unwrap();
        let _ = released.recv_timeout(Duration::from_secs(10));
    });
    let submitting = thread::spawn(move || {
        spawner
            .spawn(TaskPoolKind::Io, request(1), |_| ())
            .into_handle()
    });
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    let report = scope.poll_close();
    release.send(()).unwrap();
    assert!(report.is_complete());
    assert_eq!(
        submitting.join().unwrap().unwrap_err().reason,
        TaskRejectReason::ScopeClosed
    );
    assert_eq!(facilities.stats().for_kind(TaskPoolKind::Io).admitted, 0);
    assert!(facilities.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn physical_owner_drop_keeps_partial_activation_observable_without_waiting() {
    let config = test_config(2)
        .with_kind(TaskPoolKind::Io, TaskKindConfig::new(items(2), items(2)))
        .unwrap();
    let facilities = TaskFacilities::try_new_lazy(config).unwrap();
    let mut scope = facilities.scope();
    let spawner = scope.spawner();
    let joined = spawner.worker_join_probe_for_tests();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    spawner.after_lazy_first_worker_for_tests(move || {
        entered.send(()).unwrap();
        let _ = released.recv_timeout(Duration::from_secs(10));
    });
    let submitting = thread::spawn(move || {
        TaskPools::with_worker_spawn_failure_for_tests(TaskPoolKind::Io, usize::MAX, || {
            spawner
                .spawn(TaskPoolKind::Io, request(1), |_| ())
                .into_handle()
        })
    });
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    let before = Instant::now();
    drop(facilities);
    let elapsed = before.elapsed();
    assert_eq!(joined.load(std::sync::atomic::Ordering::Acquire), 0);
    assert!(scope.poll_close().is_complete());
    release.send(()).unwrap();
    let (outcome, probe) = submitting.join().unwrap();
    assert!(elapsed < Duration::from_secs(1));
    assert_eq!(outcome.unwrap_err().reason, TaskRejectReason::ScopeClosed);
    wait_until(|| probe.exited() == 1);
    assert_eq!(probe.started(), 1);
    wait_until(|| joined.load(std::sync::atomic::Ordering::Acquire) == 1);
}

#[test]
fn physical_close_retains_an_os_spawn_inflight_after_worker_reservation() {
    for fail in [false, true] {
        let policy = TaskShutdownPolicy::new(
            time(Duration::from_millis(1)),
            time(Duration::from_millis(1)),
            time(Duration::from_millis(5)),
        );
        let pools = Arc::new(
            TaskPools::try_new_lazy(test_config(2).with_shutdown_policy(policy).unwrap()).unwrap(),
        );
        let (entered, started) = mpsc::channel();
        let (release, released) = mpsc::channel();
        pools.after_worker_reservation_for_tests(move || {
            entered.send(()).unwrap();
            let _ = released.recv_timeout(Duration::from_secs(10));
        });
        let submitter = pools.clone();
        let submitting = thread::spawn(move || {
            TaskPools::with_worker_spawn_failure_for_tests(
                TaskPoolKind::Io,
                if fail { 0 } else { usize::MAX },
                || {
                    submitter
                        .spawn(TaskPoolKind::Io, request(1), |_| ())
                        .into_handle()
                },
            )
        });
        started.recv_timeout(Duration::from_secs(10)).unwrap();
        assert_eq!(workers(&pools, TaskPoolKind::Io), 1);
        let first = pools.shutdown_blocking().unwrap();
        release.send(()).unwrap();
        assert!(!first.is_complete());
        let (outcome, probe) = submitting.join().unwrap();
        assert!(outcome.is_err());
        wait_until(|| pools.shutdown_blocking().unwrap().is_complete());
        let report = pools.shutdown_blocking().unwrap();
        let expected = usize::from(!fail);
        assert_eq!(report.for_kind(TaskPoolKind::Io).joined_workers, expected);
        assert_eq!(probe.started(), expected);
        assert_eq!(probe.exited(), expected);
        assert_eq!(workers(&pools, TaskPoolKind::Io), 0);
        assert_eq!(pools.stats().for_kind(TaskPoolKind::Io).admitted, 0);
    }
}
