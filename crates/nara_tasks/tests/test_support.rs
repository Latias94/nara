#![cfg(feature = "test-support")]

use std::{
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

use nara_core::ItemLimit;
use nara_tasks::{
    TaskDomainKey, TaskKindConfig, TaskPoolConfig, TaskPoolKind, TaskPools, TaskRejectReason,
    TaskRejection, TaskReturnState, TaskShutdownPolicy, TaskSpawnOutcome, TaskSpawnRequest,
    WorkerSpawnFailureProbe,
};

#[test]
fn public_test_support_injects_real_partial_worker_failure() {
    let kind = TaskKindConfig::new(ItemLimit::ONE, ItemLimit::ONE);
    let config = TaskPoolConfig::new(kind, kind, kind, TaskShutdownPolicy::default())
        .unwrap()
        .with_kind(
            TaskPoolKind::Io,
            TaskKindConfig::new(ItemLimit::new(2).unwrap(), ItemLimit::ONE),
        )
        .unwrap();
    let pools = TaskPools::try_new_lazy(config).unwrap();
    let (release_tx, release_rx) = mpsc::channel();
    let (outcome, probe): (_, WorkerSpawnFailureProbe) =
        TaskPools::with_worker_spawn_failure_for_tests(TaskPoolKind::Io, 1, || {
            TaskPools::hold_partial_worker_exit_for_tests(release_rx);
            pools.spawn(
                TaskPoolKind::Io,
                TaskSpawnRequest::new(17, TaskDomainKey::new(1)),
                |_| (),
            )
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
    assert_eq!(probe.cleanup_polls(), 0);
    let retained = pools.poll_shutdown().unwrap();
    assert!(!retained.is_complete());
    assert_eq!(retained.for_kind(TaskPoolKind::Io).joined_workers, 0);
    assert_eq!(probe.exited(), 0);
    release_tx.send(()).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    let completed = loop {
        let report = pools.poll_shutdown().unwrap();
        if report.is_complete() {
            break report;
        }
        assert!(Instant::now() < deadline, "partial worker did not join");
        thread::yield_now();
    };
    assert_eq!(probe.exited(), 1);
    assert_eq!(completed.for_kind(TaskPoolKind::Io).joined_workers, 1);
    assert_eq!(completed.for_kind(TaskPoolKind::Compute).joined_workers, 0);
    assert_eq!(
        completed
            .for_kind(TaskPoolKind::AsyncCompute)
            .joined_workers,
        0
    );
    assert_eq!(pools.poll_shutdown().unwrap(), completed);
}
