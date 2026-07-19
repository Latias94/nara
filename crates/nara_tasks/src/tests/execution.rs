use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Barrier,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use crate::runtime::default_worker_allocation;

use super::*;

struct PanicOnDrop;

impl Drop for PanicOnDrop {
    fn drop(&mut self) {
        panic!("intentional test drop panic");
    }
}

fn config_with_compute_workers(pending: usize, workers: usize) -> TaskPoolConfig {
    test_config(pending)
        .with_kind(
            TaskPoolKind::Compute,
            TaskKindConfig::new(items(workers), items(pending)),
        )
        .expect("test task configuration is valid")
}

fn wait_finished<T>(handle: &TaskHandle<T>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !handle.is_finished() && Instant::now() < deadline {
        thread::yield_now();
    }
    assert!(handle.is_finished(), "task did not finish before deadline");
}

#[test]
fn bounded_queue_rejects_without_running_on_the_caller() {
    let pools = inline_pools(1);
    let calls = Arc::new(AtomicUsize::new(0));
    let first_calls = calls.clone();
    let first = accepted(pools.spawn(TaskPoolKind::Compute, request(1), move |_| {
        first_calls.fetch_add(1, Ordering::SeqCst);
        1_u32
    }));
    let second_calls = calls.clone();
    let second = pools.spawn(TaskPoolKind::Compute, request(2), move |_| {
        second_calls.fetch_add(1, Ordering::SeqCst);
        2_u32
    });

    assert!(matches!(
        second,
        TaskSpawnOutcome::Rejected(TaskRejection {
            reason: TaskRejectReason::QueueFull { capacity: 1 },
            ..
        })
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    let before = pools.stats().for_kind(TaskPoolKind::Compute);
    assert_eq!(before.admitted, 1);
    assert_eq!(before.rejected, 1);
    assert_eq!(before.queued, 1);
    assert_eq!(before.running, 0);
    assert!(before.oldest_queued_age.is_some());

    assert_eq!(pools.run_pending_for_tests().executed, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(first.is_finished());
}

#[test]
fn coalescing_replaces_a_same_domain_pending_task_even_below_capacity() {
    let pools = inline_pools(2);
    let policy = TaskOverloadPolicy::CoalescePending(TaskCoalesceKey::new(55));
    let mut old = accepted(
        pools.spawn(TaskPoolKind::Io, request(7).with_overload(policy), |_| {
            1_u32
        }),
    );
    let old_id = old.id();
    let outcome = pools.spawn(TaskPoolKind::Io, request(7).with_overload(policy), |_| {
        2_u32
    });
    let (mut replacement, replacement_id) = match outcome {
        TaskSpawnOutcome::Coalesced { handle, replaced } => {
            let id = handle.id();
            assert_eq!(replaced, old_id);
            (handle, id)
        }
        _ => panic!("expected pending coalescence"),
    };

    assert!(matches!(
        old.try_take(),
        Some(TaskTerminal::Cancelled(TaskCancellation {
            reason: TaskCancellationReason::Coalesced { replacement },
            before_start: true,
        })) if replacement == replacement_id
    ));
    assert_eq!(pools.run_pending_for_tests().executed, 1);
    assert!(matches!(
        replacement.try_take(),
        Some(TaskTerminal::Completed(2))
    ));
    let stats = pools.stats().for_kind(TaskPoolKind::Io);
    assert_eq!(stats.admitted, 2);
    assert_eq!(stats.coalesced, 1);
    assert_eq!(stats.cancelled, 1);
}

#[test]
fn user_cancellation_winning_the_coalesce_race_falls_back_to_normal_admission() {
    let pools = inline_pools(2);
    let policy = TaskOverloadPolicy::CoalescePending(TaskCoalesceKey::new(91));
    let mut old = accepted(
        pools.spawn(TaskPoolKind::Io, request(3).with_overload(policy), |_| {
            1_u32
        }),
    );
    let entered = Arc::new(Barrier::new(2));
    let resume = Arc::new(Barrier::new(2));
    let hook_entered = entered.clone();
    let hook_resume = resume.clone();
    pools.set_before_coalesce_cancel_hook_for_tests(
        TaskPoolKind::Io,
        Arc::new(move || {
            hook_entered.wait();
            hook_resume.wait();
        }),
    );

    let outcome = thread::scope(|scope| {
        let spawn = scope.spawn(|| {
            pools.spawn(TaskPoolKind::Io, request(3).with_overload(policy), |_| {
                2_u32
            })
        });
        entered.wait();
        assert!(old.cancel());
        resume.wait();
        spawn.join().unwrap()
    });
    let mut replacement = match outcome {
        TaskSpawnOutcome::Accepted(handle) => handle,
        TaskSpawnOutcome::Coalesced { .. } => {
            panic!("user cancellation won, so the new task cannot be coalesced")
        }
        TaskSpawnOutcome::Rejected(rejection) => {
            panic!("stale removal should leave capacity: {rejection:?}")
        }
    };

    assert!(matches!(
        old.try_take(),
        Some(TaskTerminal::Cancelled(TaskCancellation {
            reason: TaskCancellationReason::Requested,
            before_start: true,
        }))
    ));
    assert_eq!(pools.run_pending_for_tests().executed, 1);
    assert!(matches!(
        replacement.try_take(),
        Some(TaskTerminal::Completed(2))
    ));
    let stats = pools.stats().for_kind(TaskPoolKind::Io);
    assert_eq!(stats.admitted, 2);
    assert_eq!(stats.coalesced, 0);
    assert_eq!(stats.cancelled, 1);
}

#[test]
fn coalescing_key_is_scoped_by_domain() {
    let pools = inline_pools(2);
    let policy = TaskOverloadPolicy::CoalescePending(TaskCoalesceKey::new(8));
    let first = pools.spawn(TaskPoolKind::Io, request(1).with_overload(policy), |_| {
        1_u32
    });
    let second = pools.spawn(TaskPoolKind::Io, request(2).with_overload(policy), |_| {
        2_u32
    });

    assert!(matches!(first, TaskSpawnOutcome::Accepted(_)));
    assert!(matches!(second, TaskSpawnOutcome::Accepted(_)));
    assert_eq!(pools.stats().for_kind(TaskPoolKind::Io).coalesced, 0);
    assert_eq!(pools.run_pending_for_tests().executed, 2);
}

#[test]
fn a_running_task_with_the_same_key_is_never_coalesced() {
    let pools = TaskPools::try_new(test_config(1)).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let policy = TaskOverloadPolicy::CoalescePending(TaskCoalesceKey::new(4));
    let mut running = accepted(pools.spawn(
        TaskPoolKind::AsyncCompute,
        request(1).with_overload(policy),
        move |_| {
            started_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            1_u32
        },
    ));
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let mut queued = accepted(pools.spawn(
        TaskPoolKind::AsyncCompute,
        request(1).with_overload(policy),
        |_| 2_u32,
    ));

    assert_ne!(running.id(), queued.id());
    release_tx.send(()).unwrap();
    wait_finished(&running);
    wait_finished(&queued);
    assert!(matches!(
        running.try_take(),
        Some(TaskTerminal::Completed(1))
    ));
    assert!(matches!(
        queued.try_take(),
        Some(TaskTerminal::Completed(2))
    ));
    assert_eq!(
        pools.stats().for_kind(TaskPoolKind::AsyncCompute).coalesced,
        0
    );
    let _ = pools.shutdown_blocking().unwrap();
}

#[test]
fn task_panics_fail_only_the_handle_and_the_worker_survives() {
    let pools = TaskPools::try_new(test_config(2)).unwrap();
    let mut panicked = accepted(pools.spawn(TaskPoolKind::Compute, request(1), |_| -> u32 {
        panic!("must not escape the worker")
    }));
    let mut next = accepted(pools.spawn(TaskPoolKind::Compute, request(2), |_| 9_u32));

    wait_finished(&panicked);
    wait_finished(&next);
    assert!(matches!(
        panicked.try_take(),
        Some(TaskTerminal::Failed(TaskFailure::Panicked))
    ));
    assert!(matches!(next.try_take(), Some(TaskTerminal::Completed(9))));
    assert_eq!(pools.stats().for_kind(TaskPoolKind::Compute).failed, 1);
    let _ = pools.shutdown_blocking().unwrap();
}

#[test]
fn rejected_closure_drop_panics_are_contained() {
    let pools = inline_pools(1);
    let _first = accepted(pools.spawn(TaskPoolKind::Compute, request(1), |_| 1_u32));
    let dropper = PanicOnDrop;

    let spawn = catch_unwind(AssertUnwindSafe(|| {
        pools.spawn(TaskPoolKind::Compute, request(2), move |_| {
            let _captured = &dropper;
            2_u32
        })
    }));

    assert!(matches!(
        spawn,
        Ok(TaskSpawnOutcome::Rejected(TaskRejection {
            reason: TaskRejectReason::QueueFull { capacity: 1 },
            ..
        }))
    ));
    assert_eq!(pools.run_pending_for_tests().executed, 1);
    let mut next = accepted(pools.spawn(TaskPoolKind::Compute, request(3), |_| 3_u32));
    assert_eq!(pools.run_pending_for_tests().executed, 1);
    assert!(matches!(next.try_take(), Some(TaskTerminal::Completed(3))));
}

#[test]
fn cancelled_result_drop_panics_do_not_kill_the_worker() {
    let pools = TaskPools::try_new(test_config(2)).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let mut cancelled = accepted(pools.spawn(TaskPoolKind::Compute, request(1), move |_| {
        started_tx.send(()).unwrap();
        release_rx.recv().unwrap();
        PanicOnDrop
    }));
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(cancelled.cancel());
    release_tx.send(()).unwrap();

    let mut next = accepted(pools.spawn(TaskPoolKind::Compute, request(2), |_| 4_u32));
    wait_finished(&next);
    assert!(matches!(
        cancelled.try_take(),
        Some(TaskTerminal::Cancelled(TaskCancellation {
            reason: TaskCancellationReason::Requested,
            before_start: false,
        }))
    ));
    assert!(matches!(next.try_take(), Some(TaskTerminal::Completed(4))));
    let _ = pools.shutdown_blocking().unwrap();
}

#[test]
fn completed_result_drop_panics_do_not_escape_handle_drop() {
    let pools = inline_pools(1);
    let handle = accepted(pools.spawn(TaskPoolKind::Compute, request(1), |_| PanicOnDrop));
    assert_eq!(pools.run_pending_for_tests().executed, 1);

    assert!(catch_unwind(AssertUnwindSafe(|| drop(handle))).is_ok());
}

#[test]
fn first_terminal_state_wins_cancellation_result_races() {
    let pools = inline_pools(2);
    let mut completed = accepted(pools.spawn(TaskPoolKind::Compute, request(1), |_| 5_u32));
    assert_eq!(pools.run_pending_for_tests().executed, 1);
    assert!(!completed.cancel());
    assert!(matches!(
        completed.try_take(),
        Some(TaskTerminal::Completed(5))
    ));

    let mut cancelled = accepted(pools.spawn(TaskPoolKind::Compute, request(2), |_| 6_u32));
    assert!(cancelled.cancel());
    assert_eq!(pools.run_pending_for_tests().executed, 0);
    assert!(matches!(
        cancelled.try_take(),
        Some(TaskTerminal::Cancelled(TaskCancellation {
            reason: TaskCancellationReason::Requested,
            before_start: true,
        }))
    ));
}

#[test]
fn concurrent_completion_and_cancellation_publish_exactly_one_terminal() {
    const ATTEMPTS: u64 = 128;

    let pools = TaskPools::try_new(test_config(1)).unwrap();
    let mut completed = 0_u64;
    let mut cancelled = 0_u64;

    for value in 0..ATTEMPTS {
        let start = Arc::new(Barrier::new(2));
        let start_in_task = start.clone();
        let mut handle = accepted(
            pools.spawn(TaskPoolKind::Compute, request(value), move |_| {
                start_in_task.wait();
                value
            }),
        );

        start.wait();
        let cancellation_won = handle.cancel();
        wait_finished(&handle);

        match (cancellation_won, handle.try_take()) {
            (true, Some(TaskTerminal::Cancelled(cancellation))) => {
                assert!(!cancellation.before_start);
                cancelled += 1;
            }
            (false, Some(TaskTerminal::Completed(actual))) => {
                assert_eq!(actual, value);
                completed += 1;
            }
            outcome => panic!("terminal must agree with the winning transition: {outcome:?}"),
        }
        assert!(handle.try_take().is_none(), "a terminal can be taken once");
    }

    let stats = pools.stats().for_kind(TaskPoolKind::Compute);
    assert_eq!(completed + cancelled, ATTEMPTS);
    assert_eq!(stats.completed, completed);
    assert_eq!(stats.cancelled, cancelled);
    assert_eq!(stats.failed, 0);
    assert_eq!(stats.taken, ATTEMPTS);
    let _ = pools.shutdown_blocking().unwrap();
}

#[test]
fn running_stats_track_physical_work_after_terminal_cancellation() {
    let pools = TaskPools::try_new(test_config(1)).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let mut handle = accepted(pools.spawn(TaskPoolKind::Io, request(1), move |_| {
        started_tx.send(()).unwrap();
        let _ = release_rx.recv();
        1_u32
    }));
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(handle.cancel());
    thread::sleep(Duration::from_millis(2));

    let stats = pools.stats().for_kind(TaskPoolKind::Io);
    assert_eq!(stats.running, 1);
    assert!(
        stats
            .oldest_running_age
            .is_some_and(|age| age > Duration::ZERO)
    );
    assert!(matches!(
        handle.try_take(),
        Some(TaskTerminal::Cancelled(TaskCancellation {
            before_start: false,
            ..
        }))
    ));
    drop(release_tx);
    let _ = pools.shutdown_blocking().unwrap();
}

#[test]
fn task_ids_and_order_keys_are_monotonic_and_explicit() {
    let pools = inline_pools(2);
    let first = accepted(pools.spawn(TaskPoolKind::Io, request(80), |_| 1_u32));
    let second = accepted(pools.spawn(TaskPoolKind::Io, request(40), |_| 2_u32));

    assert!(first.id() < second.id());
    assert_eq!(first.admission_tick(), 17);
    assert_eq!(first.domain_key(), TaskDomainKey::new(80));
    assert_eq!(
        first.order_key(),
        TaskOrderKey::new(17, TaskDomainKey::new(80), first.id())
    );
}

#[test]
fn task_id_exhaustion_rejects_without_running_the_closure() {
    let pools = inline_pools(2);
    pools.set_next_task_id_for_tests(u64::MAX - 1);
    let first = accepted(pools.spawn(TaskPoolKind::Io, request(1), |_| 1_u32));
    assert_eq!(first.id().raw(), u64::MAX - 1);
    let calls = Arc::new(AtomicUsize::new(0));
    let task_calls = calls.clone();

    let exhausted = pools.spawn(TaskPoolKind::Io, request(2), move |_| {
        task_calls.fetch_add(1, Ordering::SeqCst);
        2_u32
    });

    assert!(matches!(
        exhausted,
        TaskSpawnOutcome::Rejected(TaskRejection {
            task: None,
            reason: TaskRejectReason::TaskIdExhausted,
            ..
        })
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn ordered_results_hold_later_completions_until_the_prefix_is_ready() {
    let pools = TaskPools::try_new(config_with_compute_workers(2, 2)).unwrap();
    let (first_release_tx, first_release_rx) = mpsc::channel();
    let (second_done_tx, second_done_rx) = mpsc::channel();
    let first = accepted(pools.spawn(TaskPoolKind::Compute, request(1), move |_| {
        first_release_rx.recv().unwrap();
        1_u32
    }));
    let second = accepted(pools.spawn(TaskPoolKind::Compute, request(1), move |_| {
        second_done_tx.send(()).unwrap();
        2_u32
    }));
    let mut ordered = OrderedTaskResults::default();
    ordered.push(first).unwrap();
    ordered.push(second).unwrap();
    second_done_rx
        .recv_timeout(Duration::from_secs(10))
        .unwrap();

    let cutoff = pools.capture_completion_cutoff();
    assert!(
        ordered
            .capture_ready_prefix(&cutoff)
            .unwrap()
            .into_terminals()
            .unwrap()
            .is_empty()
    );
    first_release_tx.send(()).unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let ready = loop {
        let cutoff = pools.capture_completion_cutoff();
        let ready = ordered
            .capture_ready_prefix(&cutoff)
            .unwrap()
            .into_terminals()
            .unwrap();
        if !ready.is_empty() || Instant::now() >= deadline {
            break ready;
        }
        thread::yield_now();
    };
    assert_eq!(ready.len(), 2);
    assert!(matches!(ready[0].terminal, TaskTerminal::Completed(1)));
    assert!(matches!(ready[1].terminal, TaskTerminal::Completed(2)));
    let _ = pools.shutdown_blocking().unwrap();
}

#[test]
fn ordered_ready_snapshot_excludes_completion_after_cutoff_before_capture() {
    let pools = TaskPools::try_new(config_with_compute_workers(1, 1)).unwrap();
    let (release_tx, release_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    let task = accepted(pools.spawn(TaskPoolKind::Compute, request(1), move |_| {
        release_rx.recv().unwrap();
        done_tx.send(()).unwrap();
        1_u32
    }));
    let mut ordered = OrderedTaskResults::default();
    ordered.push(task).unwrap();

    let first_cutoff = pools.capture_completion_cutoff();
    release_tx.send(()).unwrap();
    done_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let completion_deadline = Instant::now() + Duration::from_secs(10);
    while pools.stats().for_kind(TaskPoolKind::Compute).completed < 1
        && Instant::now() < completion_deadline
    {
        thread::yield_now();
    }
    assert_eq!(
        pools.stats().for_kind(TaskPoolKind::Compute).completed,
        1,
        "task completion must publish before the stream captures against the old cutoff"
    );

    let first_poll = ordered
        .capture_ready_prefix(&first_cutoff)
        .unwrap()
        .into_terminals()
        .unwrap();
    assert!(first_poll.is_empty());
    assert_eq!(ordered.len(), 1);

    let second_cutoff = pools.capture_completion_cutoff();
    let second_poll = ordered
        .capture_ready_prefix(&second_cutoff)
        .unwrap()
        .into_terminals()
        .unwrap();
    assert_eq!(second_poll.len(), 1);
    assert!(matches!(
        second_poll[0].terminal,
        TaskTerminal::Completed(1)
    ));
    let _ = pools.shutdown_blocking().unwrap();
}

#[test]
fn ordered_ready_snapshot_rejects_a_foreign_cutoff_without_mutation() {
    let own_pools = inline_pools(1);
    let foreign_pools = inline_pools(1);
    let foreign_task = accepted(foreign_pools.spawn(TaskPoolKind::Io, request(1), |_| 7_u32));
    let foreign_descriptor = foreign_task.descriptor();
    let mut ordered = OrderedTaskResults::default();
    ordered.push(foreign_task).unwrap();
    assert_eq!(foreign_pools.run_pending_for_tests().executed, 1);

    let error = ordered
        .capture_ready_prefix(&own_pools.capture_completion_cutoff())
        .unwrap_err();

    assert_eq!(
        error,
        TaskCompletionCutoffError::PoolMismatch {
            task: foreign_descriptor
        }
    );
    assert_eq!(ordered.len(), 1);
    let ready = ordered
        .capture_ready_prefix(&foreign_pools.capture_completion_cutoff())
        .unwrap()
        .into_terminals()
        .unwrap();
    assert_eq!(ready.len(), 1);
    assert!(matches!(ready[0].terminal, TaskTerminal::Completed(7)));
}

#[test]
fn queue_and_running_ages_are_observable_from_consistent_snapshots() {
    let pools = inline_pools(1);
    let _handle = accepted(pools.spawn(TaskPoolKind::Io, request(1), |_| 1_u32));
    thread::sleep(Duration::from_millis(2));

    let stats = pools.stats().for_kind(TaskPoolKind::Io);
    assert_eq!((stats.queued, stats.running), (1, 0));
    assert!(
        stats
            .oldest_queued_age
            .is_some_and(|age| age > Duration::ZERO)
    );
    assert_eq!(stats.oldest_running_age, None);
}

#[test]
fn excessive_worker_counts_are_rejected_instead_of_clamped() {
    let excessive = TaskKindConfig::new(items(MAX_TASK_POOL_THREADS_PER_KIND + 1), ItemLimit::ONE);
    assert!(matches!(
        TaskPoolConfig::new(
            excessive,
            TaskKindConfig::default(),
            TaskKindConfig::default(),
            TaskShutdownPolicy::default(),
        ),
        Err(TaskConfigError::TooManyWorkers {
            kind: TaskPoolKind::Io,
            ..
        })
    ));
}

#[test]
fn default_worker_allocation_respects_aggregate_limits_on_large_hosts() {
    for parallelism in [510, 512, usize::MAX] {
        let allocation = default_worker_allocation(parallelism);
        assert!(
            allocation
                .into_iter()
                .all(|workers| { (1..=MAX_TASK_POOL_THREADS_PER_KIND).contains(&workers) })
        );
        assert!(allocation.into_iter().sum::<usize>() <= MAX_TASK_POOL_THREADS_TOTAL);
    }
    assert_eq!(default_worker_allocation(510), [2, 256, 254]);
    assert_eq!(default_worker_allocation(512), [2, 256, 254]);
    assert_eq!(default_worker_allocation(usize::MAX), [2, 256, 254]);
}

#[test]
fn task_pool_constructors_defensively_revalidate_configuration() {
    let invalid_kind =
        TaskKindConfig::new(items(MAX_TASK_POOL_THREADS_PER_KIND + 1), ItemLimit::ONE);
    let invalid = TaskPoolConfig::unchecked_for_tests(
        invalid_kind,
        TaskKindConfig::default(),
        TaskKindConfig::default(),
        TaskShutdownPolicy::default(),
    );

    assert!(matches!(
        TaskPools::try_new(invalid),
        Err(TaskPoolError::InvalidConfig(
            TaskConfigError::TooManyWorkers {
                kind: TaskPoolKind::Io,
                ..
            }
        ))
    ));
    assert!(matches!(
        TaskPools::inline_for_tests(invalid),
        Err(TaskPoolError::InvalidConfig(
            TaskConfigError::TooManyWorkers {
                kind: TaskPoolKind::Io,
                ..
            }
        ))
    ));
}

#[test]
fn partial_worker_spawn_failure_returns_a_pollable_owner_without_waiting() {
    let config = test_config(1)
        .with_kind(
            TaskPoolKind::Io,
            TaskKindConfig::new(items(2), ItemLimit::ONE),
        )
        .unwrap();
    let started_at = Instant::now();

    let (result, probe) =
        TaskPools::try_new_with_worker_spawn_failure_for_tests(config, TaskPoolKind::Io, 1);
    let mut failure = result.unwrap_err();

    assert!(matches!(
        failure.error(),
        TaskPoolError::WorkerSpawnFailed {
            kind: TaskPoolKind::Io,
            ..
        }
    ));
    assert!(started_at.elapsed() < Duration::from_secs(2));
    assert_eq!(probe.started(), 1);
    let deadline = Instant::now() + Duration::from_secs(10);
    while failure.drive_cleanup() != TaskPoolsCloseProgress::Complete && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert_eq!(probe.exited(), 1);
}

#[test]
fn cross_executor_construction_failure_retains_earlier_pool_owners() {
    let config = test_config(1);
    let (result, probe) =
        TaskPools::try_new_with_worker_spawn_failure_for_tests(config, TaskPoolKind::Compute, 0);
    let mut failure = result.unwrap_err();

    assert!(matches!(
        failure.error(),
        TaskPoolError::WorkerSpawnFailed {
            kind: TaskPoolKind::Compute,
            ..
        }
    ));
    let deadline = Instant::now() + Duration::from_secs(10);
    while failure.drive_cleanup() != TaskPoolsCloseProgress::Complete && Instant::now() < deadline {
        std::thread::yield_now();
    }

    assert_eq!(
        failure.report().for_kind(TaskPoolKind::Io).joined_workers,
        1
    );
    assert_eq!(probe.started(), 0);
    assert_eq!(probe.exited(), 0);
}

#[test]
fn aggregate_worker_limit_is_enforced() {
    let many = TaskKindConfig::new(items(200), ItemLimit::ONE);
    assert!(matches!(
        TaskPoolConfig::new(many, many, many, TaskShutdownPolicy::default(),),
        Err(TaskConfigError::TooManyTotalWorkers { .. })
    ));
}

#[test]
fn pending_limits_are_enforced_per_kind_and_in_aggregate() {
    let excessive = TaskKindConfig::new(ItemLimit::ONE, items(MAX_TASK_POOL_PENDING_PER_KIND + 1));
    assert!(matches!(
        TaskPoolConfig::new(
            excessive,
            TaskKindConfig::default(),
            TaskKindConfig::default(),
            TaskShutdownPolicy::default(),
        ),
        Err(TaskConfigError::TooManyPending {
            kind: TaskPoolKind::Io,
            ..
        })
    ));

    let many = TaskKindConfig::new(ItemLimit::ONE, items(800_000));
    assert!(matches!(
        TaskPoolConfig::new(many, many, many, TaskShutdownPolicy::default()),
        Err(TaskConfigError::TooManyTotalPending { .. })
    ));
}

#[test]
fn shutdown_timeouts_are_bounded_per_phase_and_in_aggregate() {
    let too_long = TaskShutdownPolicy::new(
        time(MAX_TASK_SHUTDOWN_PHASE_TIMEOUT + Duration::from_secs(1)),
        TimeLimit::MIN,
        TimeLimit::MIN,
    );
    assert!(matches!(
        TaskPoolConfig::new(
            TaskKindConfig::default(),
            TaskKindConfig::default(),
            TaskKindConfig::default(),
            too_long,
        ),
        Err(TaskConfigError::ShutdownPhaseTooLong {
            phase: TaskShutdownPhase::Drain,
            ..
        })
    ));

    let aggregate = TaskShutdownPolicy::new(
        time(Duration::from_secs(25)),
        time(Duration::from_secs(25)),
        time(Duration::from_secs(25)),
    );
    assert!(matches!(
        TaskPoolConfig::new(
            TaskKindConfig::default(),
            TaskKindConfig::default(),
            TaskKindConfig::default(),
            aggregate,
        ),
        Err(TaskConfigError::ShutdownTotalTooLong { .. })
    ));
}
