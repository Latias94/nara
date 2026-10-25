use std::{
    sync::{
        Arc, Barrier,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use nara_app::{App, PluginPlan, PluginPlanError, RuntimeState};

use super::{
    close::{drive_until_state, request_stop, start_runtime},
    *,
};

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
fn untouched_lazy_plugin_registers_physical_close_without_starting_workers() {
    let mut app = App::new();
    app.add_plugin(TaskPlugin::lazy(test_config(4))).unwrap();
    let pools = app.world().resource::<TaskPools>();
    for kind in TaskPoolKind::ALL {
        assert!(pools.is_threaded(kind));
        assert_eq!(workers(pools, kind), 0);
    }
    assert_eq!(
        pools.shutdown_blocking(),
        Err(TaskShutdownError::CloseOwnerTransferred)
    );
    assert_eq!(
        pools.poll_shutdown(),
        Err(TaskShutdownError::CloseOwnerTransferred)
    );

    let mut runtime = start_runtime(app, Duration::from_secs(2));
    for kind in TaskPoolKind::ALL {
        assert_eq!(workers(runtime.world().resource::<TaskPools>(), kind), 0);
    }
    request_stop(&mut runtime);
    drive_until_state(&mut runtime, RuntimeState::Stopped);

    assert!(!runtime.world().contains_resource::<TaskPools>());
    let report = runtime.world().resource::<TaskShutdownReport>();
    assert!(report.is_complete());
    for kind in TaskPoolKind::ALL {
        assert_eq!(report.for_kind(kind), TaskPoolShutdownReport::default());
    }
}

#[test]
fn lazy_plugin_first_use_activates_only_selected_kind_and_runtime_joins_its_workers() {
    let config = test_config(4)
        .with_kind(TaskPoolKind::Io, TaskKindConfig::new(items(2), items(4)))
        .unwrap();
    let mut app = App::new();
    app.add_plugins(lazy_plugin(config)).unwrap();
    app.add_plugins(lazy_plugin(config)).unwrap();
    assert_eq!(app.installed_plugin_entries().count(), 1);
    let mut runtime = start_runtime(app, Duration::from_secs(2));
    let pools = runtime.world().resource::<TaskPools>();
    for kind in TaskPoolKind::ALL {
        assert_eq!(workers(pools, kind), 0);
    }

    let mut handle =
        accepted(pools.spawn(TaskPoolKind::Io, request(1), |_| thread::current().id()));
    wait_until(|| handle.return_observer().state() == TaskReturnState::Returned);
    let Some(TaskTerminal::Completed(executed_on)) = handle.try_take() else {
        panic!("lazy plugin task did not complete successfully");
    };
    assert_ne!(executed_on, thread::current().id());
    assert_eq!(workers(pools, TaskPoolKind::Io), 2);
    assert_eq!(workers(pools, TaskPoolKind::Compute), 0);
    assert_eq!(workers(pools, TaskPoolKind::AsyncCompute), 0);

    request_stop(&mut runtime);
    drive_until_state(&mut runtime, RuntimeState::Stopped);
    assert!(!runtime.world().contains_resource::<TaskPools>());
    let report = runtime.world().resource::<TaskShutdownReport>();
    assert!(report.is_complete());
    for kind in TaskPoolKind::ALL {
        assert_eq!(
            report.for_kind(kind),
            TaskPoolShutdownReport {
                joined_workers: if kind == TaskPoolKind::Io { 2 } else { 0 },
                ..TaskPoolShutdownReport::default()
            }
        );
    }
}

#[test]
fn lazy_plugin_refuses_an_externally_owned_facade_before_acquiring_facilities() {
    let mut app = App::new();
    app.insert_resource(inline_pools(7)).unwrap();
    assert!(app.add_plugin(TaskPlugin::lazy(test_config(4))).is_err());
    let external = app.world().resource::<TaskPools>();
    assert_eq!(external.config().kind(TaskPoolKind::Io).pending().get(), 7);
    let handle = accepted(external.spawn(TaskPoolKind::Io, request(1), |_| ()));
    assert_eq!(external.run_pending_for_tests().executed, 1);
    assert!(handle.is_finished());
    assert!(external.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn lazy_plugin_retains_partial_startup_workers_after_facade_removal_or_replacement() {
    let config = test_config(4)
        .with_kind(TaskPoolKind::Io, TaskKindConfig::new(items(2), items(4)))
        .unwrap();
    for replace in [false, true] {
        let mut app = App::new();
        app.add_plugin(TaskPlugin::lazy(config)).unwrap();
        let pools = app.world().resource::<TaskPools>();
        let joined = pools.worker_join_probe_for_tests();
        let called = Arc::new(AtomicBool::new(false));
        let calls = called.clone();
        let (outcome, probe) =
            TaskPools::with_worker_spawn_failure_for_tests(TaskPoolKind::Io, 1, || {
                pools.spawn(TaskPoolKind::Io, request(1), move |_| {
                    calls.store(true, Ordering::Release);
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
        assert!(!called.load(Ordering::Acquire));
        assert_eq!(pools.stats().for_kind(TaskPoolKind::Io).admitted, 0);

        let mut independent = accepted(pools.spawn(TaskPoolKind::Compute, request(1), |_| 4));
        wait_until(|| independent.return_observer().state() == TaskReturnState::Returned);
        assert!(matches!(
            independent.try_take(),
            Some(TaskTerminal::Completed(4))
        ));
        assert_eq!(workers(pools, TaskPoolKind::Compute), 1);
        assert_eq!(workers(pools, TaskPoolKind::AsyncCompute), 0);

        if replace {
            app.insert_resource(inline_pools(7)).unwrap();
        } else {
            drop(
                app.world_mut()
                    .unwrap()
                    .remove_resource::<TaskPools>()
                    .unwrap(),
            );
        }
        // Even an exited partial worker stays with the registered physical close owner.
        wait_until(|| probe.exited() == 1);
        assert_eq!(joined.load(Ordering::Acquire), 0);

        let mut runtime = start_runtime(app, Duration::from_secs(2));
        request_stop(&mut runtime);
        drive_until_state(&mut runtime, RuntimeState::Stopped);

        assert_eq!(joined.load(Ordering::Acquire), 1);
        assert_eq!(probe.started(), 1);
        assert_eq!(probe.exited(), 1);
        assert!(!called.load(Ordering::Acquire));
        let report = runtime.world().resource::<TaskShutdownReport>();
        assert!(report.is_complete());
        for kind in TaskPoolKind::ALL {
            assert_eq!(
                report.for_kind(kind),
                TaskPoolShutdownReport {
                    joined_workers: usize::from(kind != TaskPoolKind::AsyncCompute),
                    ..TaskPoolShutdownReport::default()
                }
            );
        }
        assert_eq!(runtime.world().contains_resource::<TaskPools>(), replace);
        if replace {
            let replacement = runtime.world().resource::<TaskPools>();
            assert_eq!(
                replacement.config().kind(TaskPoolKind::Io).pending().get(),
                7
            );
            let handle = accepted(replacement.spawn(TaskPoolKind::Io, request(1), |_| ()));
            assert_eq!(replacement.run_pending_for_tests().executed, 1);
            assert!(handle.is_finished());
            assert!(replacement.shutdown_blocking().unwrap().is_complete());
        }
    }
}

#[test]
fn eager_and_lazy_plugin_definitions_are_repeatable_with_distinct_configuration_identity() {
    let config = test_config(4);
    let eager = PluginPlan::resolve(plugin(config)).unwrap();
    let repeated_eager = PluginPlan::resolve((plugin(config), plugin(config))).unwrap();
    let lazy = PluginPlan::resolve(lazy_plugin(config)).unwrap();
    let repeated_lazy = PluginPlan::resolve((lazy_plugin(config), lazy_plugin(config))).unwrap();

    assert_eq!(eager.entries().len(), 1);
    assert_eq!(lazy.entries().len(), 1);
    assert_eq!(eager.entries(), repeated_eager.entries());
    assert_eq!(lazy.entries(), repeated_lazy.entries());
    assert_eq!(eager.fingerprint(), repeated_eager.fingerprint());
    assert_eq!(lazy.fingerprint(), repeated_lazy.fingerprint());
    assert_ne!(eager.fingerprint(), lazy.fingerprint());
    let eager_key = eager.entries()[0].definition_key().unwrap();
    let lazy_key = lazy.entries()[0].definition_key().unwrap();
    assert_eq!(eager_key.definition(), lazy_key.definition());
    assert_ne!(eager_key.configuration(), lazy_key.configuration());

    for definitions in [
        (plugin(config), lazy_plugin(config)),
        (lazy_plugin(config), plugin(config)),
    ] {
        assert_eq!(
            PluginPlan::resolve(definitions).unwrap_err(),
            PluginPlanError::DivergentDefinition {
                plugin: TASK_PLUGIN_ID
            }
        );
    }
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
    let report = pools.poll_shutdown().unwrap();
    assert!(report.is_complete());
    for kind in TaskPoolKind::ALL {
        assert_eq!(report.for_kind(kind), TaskPoolShutdownReport::default());
        assert_eq!(workers(&pools, kind), 0);
    }
    assert_eq!(pools.poll_shutdown().unwrap(), report);
    assert_eq!(pools.shutdown_blocking().unwrap(), report);
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
fn scoped_facade_cannot_poll_physical_shutdown_or_close_lazy_admission() {
    let facilities = TaskFacilities::try_new_lazy(test_config(2)).unwrap();
    let mut scope = facilities.scope();
    let spawner = scope.spawner();
    assert_eq!(
        spawner.poll_shutdown(),
        Err(TaskShutdownError::CloseOwnerTransferred)
    );
    for kind in TaskPoolKind::ALL {
        assert_eq!(facilities.stats().for_kind(kind).live_workers, 0);
    }
    let handle = accepted(spawner.spawn(TaskPoolKind::Io, request(1), |_| ()));
    wait_until(|| handle.return_observer().state() == TaskReturnState::Returned);
    wait_until(|| scope.poll_close().is_complete());
    let report = facilities.shutdown_blocking().unwrap();
    assert!(report.is_complete());
    assert_eq!(report.for_kind(TaskPoolKind::Io).joined_workers, 1);
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
    let (release_tx, release_rx) = mpsc::channel();
    let called = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let calls = called.clone();
    let (outcome, probe) =
        TaskPools::with_worker_spawn_failure_for_tests(TaskPoolKind::Io, 1, || {
            TaskPools::hold_partial_worker_exit_for_tests(release_rx);
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
    let retained = pools.poll_shutdown().unwrap();
    assert!(!retained.is_complete());
    assert_eq!(retained.for_kind(TaskPoolKind::Io).joined_workers, 0);
    assert_eq!(probe.exited(), 0);
    release_tx.send(()).unwrap();
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
