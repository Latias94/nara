use std::{
    sync::{Arc, Barrier, mpsc},
    thread,
    time::{Duration, Instant},
};

use nara_app::{App, RuntimeCloseParticipantId, RuntimeControl, RuntimeState};

use super::{
    close::{drive_until_state, request_stop, start_runtime},
    *,
};

fn wait_until(mut predicate: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !predicate() {
        assert!(Instant::now() < deadline, "activity did not return");
        thread::yield_now();
    }
}

#[test]
fn coalescing_and_completion_cutoffs_are_activity_local() {
    let facilities = TaskFacilities::inline_for_tests(test_config(4)).unwrap();
    let mut first = facilities.scope();
    let mut second = facilities.scope();
    let a = first.spawner();
    let b = second.spawner();
    let keyed =
        request(7).with_overload(TaskOverloadPolicy::CoalescePending(TaskCoalesceKey::new(9)));
    let old = accepted(a.spawn(TaskPoolKind::Io, keyed, |_| 1));
    let other = accepted(b.spawn(TaskPoolKind::Io, keyed, |_| 2));
    assert!(!old.is_finished());
    let mut replacement = match a.spawn(TaskPoolKind::Io, keyed, |_| 3) {
        TaskSpawnOutcome::Coalesced { handle, replaced } => {
            assert_eq!(replaced, old.id());
            handle
        }
        _ => panic!("expected same-activity coalescing"),
    };
    assert!(!other.is_finished());
    assert_ne!(replacement.id(), other.id());
    assert_eq!(facilities.run_pending_for_tests().executed, 2);
    let cutoff = a.capture_completion_cutoff();
    let mut unrelated = OrderedTaskResults::default();
    unrelated.push(other).unwrap();
    assert!(unrelated.capture_ready_prefix(&cutoff).is_err());
    assert_eq!(replacement.try_take(), Some(TaskTerminal::Completed(3)));
    assert!(first.poll_close().is_complete());
    assert!(second.poll_close().is_complete());
    assert!(a.shutdown_blocking().is_err());
}

#[test]
fn closing_interleaved_activity_preserves_sibling_queue_order() {
    let facilities = TaskFacilities::inline_for_tests(test_config(8)).unwrap();
    let mut retired = facilities.scope();
    let sibling = facilities.scope();
    let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
    for number in 0..6 {
        let owner = if number % 2 == 0 { &retired } else { &sibling };
        let observed = observed.clone();
        accepted(
            owner
                .spawner()
                .spawn(TaskPoolKind::Io, request(1), move |_| {
                    observed.lock().unwrap().push(number);
                }),
        );
    }
    retired.begin_close();
    assert_eq!(facilities.run_pending_for_tests().executed, 3);
    assert_eq!(*observed.lock().unwrap(), [1, 3, 5]);
    wait_until(|| retired.poll_close().is_complete());
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

#[test]
fn pending_activity_retires_off_thread_while_a_sibling_occupies_the_worker() {
    let facilities = TaskFacilities::try_new(test_config(4)).unwrap();
    let mut blocked = facilities.scope();
    let mut queued = facilities.scope();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel::<()>();
    let _running = accepted(
        blocked
            .spawner()
            .spawn(TaskPoolKind::Io, request(1), move |_| {
                entered.send(()).unwrap();
                let _ = released.recv_timeout(Duration::from_secs(10));
            }),
    );
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    let (entered, disposal_started) = mpsc::channel();
    let (dispose, dispose_release) = mpsc::channel();
    let capture = HeldDrop {
        entered,
        release: dispose_release,
    };
    let mut pending = accepted(
        queued
            .spawner()
            .spawn(TaskPoolKind::Io, request(1), move |_| drop(capture)),
    );
    let owner_thread = thread::current().id();
    assert!(!queued.poll_close().is_complete());
    assert_ne!(
        disposal_started
            .recv_timeout(Duration::from_secs(10))
            .unwrap(),
        owner_thread
    );
    assert!(matches!(
        pending.try_take(),
        Some(TaskTerminal::Cancelled(TaskCancellation {
            reason: TaskCancellationReason::ScopeClosed,
            before_start: true,
        }))
    ));
    assert!(!queued.poll_close().is_complete());
    dispose.send(()).unwrap();
    wait_until(|| queued.poll_close().is_complete());
    assert_eq!(facilities.stats().for_kind(TaskPoolKind::Io).running, 1);
    assert_eq!(
        facilities.stats().for_kind(TaskPoolKind::Io).live_workers,
        1
    );
    release.send(()).unwrap();
    wait_until(|| blocked.poll_close().is_complete());
    assert!(facilities.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn scope_waits_for_a_cancelled_running_tasks_late_result_disposal() {
    let facilities = TaskFacilities::try_new(test_config(2)).unwrap();
    let mut activity = facilities.scope();
    let spawner = activity.spawner();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let (drop_entered, drop_started) = mpsc::channel();
    let (dispose, disposed) = mpsc::channel();
    let output = HeldDrop {
        entered: drop_entered,
        release: disposed,
    };
    let handle = accepted(spawner.spawn(TaskPoolKind::Io, request(1), move |_| {
        entered.send(()).unwrap();
        let _ = released.recv_timeout(Duration::from_secs(10));
        output
    }));
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(!activity.poll_close().is_complete());
    assert!(handle.is_finished());
    assert_eq!(handle.return_observer().state(), TaskReturnState::Pending);
    assert!(matches!(
        spawner.spawn(TaskPoolKind::Io, request(1), |_| ()),
        TaskSpawnOutcome::Rejected(TaskRejection {
            reason: TaskRejectReason::ScopeClosed,
            ..
        })
    ));
    release.send(()).unwrap();
    drop_started.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(!activity.poll_close().is_complete());
    dispose.send(()).unwrap();
    wait_until(|| activity.poll_close().is_complete());
    assert!(!activity.poll_close().cleanup_failed);
}

#[test]
fn coalesced_disposal_stays_counted_after_its_queue_entry_is_replaced() {
    let facilities = TaskFacilities::inline_for_tests(test_config(4)).unwrap();
    let mut activity = facilities.scope();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let capture = HeldDrop {
        entered,
        release: released,
    };
    let keyed =
        request(1).with_overload(TaskOverloadPolicy::CoalescePending(TaskCoalesceKey::new(1)));
    let old = accepted(
        activity
            .spawner()
            .spawn(TaskPoolKind::Io, keyed, move |_| drop(capture)),
    );
    let spawner = activity.spawner();
    let replacement = thread::spawn(move || {
        spawner
            .spawn(TaskPoolKind::Io, keyed, |_| ())
            .into_handle()
            .unwrap()
    });
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(old.is_finished());
    assert!(!activity.poll_close().is_complete());
    assert_eq!(old.return_observer().state(), TaskReturnState::Pending);
    release.send(()).unwrap();
    let replaced = replacement.join().unwrap();
    wait_until(|| activity.poll_close().is_complete());
    assert!(replaced.is_finished());
    assert_eq!(facilities.run_pending_for_tests().executed, 0);
}

#[test]
fn rejection_disposal_cannot_expand_an_already_closed_activity() {
    let facilities = TaskFacilities::inline_for_tests(test_config(2)).unwrap();
    let mut activity = facilities.scope();
    assert!(activity.poll_close().is_complete());
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let capture = HeldDrop {
        entered,
        release: released,
    };
    let spawner = activity.spawner();
    let submitter = thread::spawn(move || {
        spawner
            .spawn(TaskPoolKind::Io, request(1), move |_| drop(capture))
            .into_handle()
            .unwrap_err()
    });
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(activity.poll_close().is_complete());
    release.send(()).unwrap();
    let rejection = submitter.join().unwrap();
    assert_eq!(rejection.reason, TaskRejectReason::ScopeClosed);
    assert_eq!(rejection.return_state, TaskReturnState::Returned);
}

#[test]
fn nested_same_activity_work_is_admitted_before_close_and_refused_after_close() {
    let facilities = TaskFacilities::try_new(test_config(4)).unwrap();
    let mut activity = facilities.scope();
    let nested = activity.spawner();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let (result, outcomes) = mpsc::channel();
    let handle = accepted(
        activity
            .spawner()
            .spawn(TaskPoolKind::Io, request(1), move |_| {
                let before = nested
                    .spawn(TaskPoolKind::Compute, request(2), |_| 7)
                    .into_handle()
                    .unwrap();
                entered.send(before).unwrap();
                let _ = released.recv_timeout(Duration::from_secs(10));
                result
                    .send(
                        nested
                            .spawn(TaskPoolKind::Compute, request(3), |_| 9)
                            .into_handle()
                            .unwrap_err(),
                    )
                    .unwrap();
            }),
    );
    let child = started.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(!activity.poll_close().is_complete());
    release.send(()).unwrap();
    assert_eq!(
        outcomes
            .recv_timeout(Duration::from_secs(10))
            .unwrap()
            .reason,
        TaskRejectReason::ScopeClosed
    );
    wait_until(|| activity.poll_close().is_complete());
    assert!(handle.is_finished());
    assert!(child.is_finished());
}

#[test]
fn concurrent_admission_and_close_never_lose_accepted_work() {
    let facilities = Arc::new(TaskFacilities::inline_for_tests(test_config(1024)).unwrap());
    let mut activity = facilities.scope();
    let start = Arc::new(Barrier::new(2));
    let barrier = start.clone();
    let spawner = activity.spawner();
    let submitter = thread::spawn(move || {
        let mut handles = vec![accepted(spawner.spawn(
            TaskPoolKind::Io,
            request(1),
            |_| (),
        ))];
        barrier.wait();
        handles.extend((0..1024).filter_map(|_| {
            spawner
                .spawn(TaskPoolKind::Io, request(1), |_| ())
                .into_handle()
                .ok()
        }));
        handles
    });
    start.wait();
    activity.begin_close();
    let handles = submitter.join().unwrap();
    assert!(!handles.is_empty());
    wait_until(|| activity.poll_close().is_complete());
    assert!(handles.iter().all(TaskHandle::is_finished));
    assert!(
        handles
            .iter()
            .all(|task| task.return_observer().state() == TaskReturnState::Returned)
    );
    assert_eq!(facilities.run_pending_for_tests().executed, 0);
}

#[test]
fn completed_output_is_not_retained_or_destroyed_by_the_activity_owner() {
    let facilities = TaskFacilities::inline_for_tests(test_config(2)).unwrap();
    let mut activity = facilities.scope();
    let (dropped, observation) = mpsc::channel();
    struct Output(mpsc::Sender<thread::ThreadId>);
    impl Drop for Output {
        fn drop(&mut self) {
            self.0.send(thread::current().id()).unwrap();
        }
    }
    let handle = accepted(
        activity
            .spawner()
            .spawn(TaskPoolKind::Io, request(1), move |_| Output(dropped)),
    );
    facilities.run_pending_for_tests();
    assert!(activity.poll_close().is_complete());
    drop(activity);
    assert!(observation.try_recv().is_err());
    let dropper = thread::spawn(move || {
        drop(handle);
        thread::current().id()
    })
    .join()
    .unwrap();
    assert_eq!(observation.recv().unwrap(), dropper);
}

#[test]
fn detached_completed_output_disposal_stays_on_worker_and_blocks_activity_return() {
    let facilities = TaskFacilities::try_new(test_config(2)).unwrap();
    let mut activity = facilities.scope();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let (drop_entered, drop_started) = mpsc::channel();
    let (dispose, disposed) = mpsc::channel();
    let output = HeldDrop {
        entered: drop_entered,
        release: disposed,
    };
    let handle = accepted(
        activity
            .spawner()
            .spawn(TaskPoolKind::Io, request(1), move |_| {
                entered.send(()).unwrap();
                let _ = released.recv_timeout(Duration::from_secs(10));
                output
            }),
    );
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    let returned = handle.return_observer();
    drop(handle);
    release.send(()).unwrap();
    assert_ne!(
        drop_started.recv_timeout(Duration::from_secs(10)).unwrap(),
        thread::current().id()
    );
    assert_eq!(facilities.stats().for_kind(TaskPoolKind::Io).completed, 1);
    assert!(!activity.poll_close().is_complete());
    assert_eq!(returned.state(), TaskReturnState::Pending);
    dispose.send(()).unwrap();
    wait_until(|| activity.poll_close().is_complete());
    assert_eq!(returned.state(), TaskReturnState::Returned);
}

#[test]
fn live_cleanup_failure_observation_is_activity_local_and_survives_task_drop() {
    let facilities = TaskFacilities::inline_for_tests(test_config(2)).unwrap();
    let failed = facilities.scope();
    let healthy = facilities.scope();
    let failed_spawner = failed.spawner();
    let healthy_spawner = healthy.spawner();
    assert!(!failed_spawner.has_cleanup_failure());
    assert!(!healthy_spawner.has_cleanup_failure());

    let capture = PanicOnDrop;
    let task = accepted(failed_spawner.spawn(TaskPoolKind::Io, request(1), move |_| drop(capture)));
    let returned = task.return_observer();
    assert_eq!(returned.state(), TaskReturnState::Pending);
    assert!(!failed_spawner.has_cleanup_failure());
    assert_eq!(facilities.run_pending_for_tests().executed, 1);
    assert_eq!(returned.state(), TaskReturnState::CleanupFailed);
    assert!(failed_spawner.has_cleanup_failure());
    assert!(!healthy_spawner.has_cleanup_failure());
    drop(task);
    drop(returned);
    assert!(failed.spawner().has_cleanup_failure());

    let mut retry = accepted(failed_spawner.spawn(TaskPoolKind::Io, request(2), |_| 3));
    let mut sibling = accepted(healthy_spawner.spawn(TaskPoolKind::Io, request(2), |_| 4));
    assert_eq!(facilities.run_pending_for_tests().executed, 2);
    assert_eq!(retry.try_take(), Some(TaskTerminal::Completed(3)));
    assert_eq!(sibling.try_take(), Some(TaskTerminal::Completed(4)));
    assert!(failed_spawner.has_cleanup_failure());
    assert!(!healthy_spawner.has_cleanup_failure());
    assert!(
        facilities
            .shutdown_blocking()
            .unwrap()
            .has_cleanup_failure()
    );
}

#[test]
fn cleanup_failure_is_sticky_after_activity_return_and_does_not_poison_siblings() {
    let facilities = TaskFacilities::inline_for_tests(test_config(2)).unwrap();
    let mut failed = facilities.scope();
    let mut healthy = facilities.scope();
    let capture = PanicOnDrop;
    let _handle = accepted(
        failed
            .spawner()
            .spawn(TaskPoolKind::Io, request(1), move |_| drop(capture)),
    );
    wait_until(|| failed.poll_close().is_complete());
    assert!(failed.poll_close().cleanup_failed);
    let mut ordinary = accepted(healthy.spawner().spawn(TaskPoolKind::Io, request(1), |_| 4));
    facilities.run_pending_for_tests();
    assert_eq!(ordinary.try_take(), Some(TaskTerminal::Completed(4)));
    assert!(!healthy.poll_close().cleanup_failed);
    assert!(
        facilities
            .shutdown_blocking()
            .unwrap()
            .has_cleanup_failure()
    );
}

#[test]
fn dropping_activity_cancels_queued_work_and_keeps_shared_workers_alive() {
    let facilities = TaskFacilities::inline_for_tests(test_config(2)).unwrap();
    let activity = facilities.scope();
    let stale_spawner = activity.spawner();
    let pending = accepted(stale_spawner.spawn(TaskPoolKind::Io, request(1), |_| ()));
    drop(activity);
    assert!(pending.is_finished());
    wait_until(|| pending.return_observer().state() == TaskReturnState::Returned);
    assert!(matches!(
        stale_spawner.spawn(TaskPoolKind::Io, request(1), |_| ()),
        TaskSpawnOutcome::Rejected(TaskRejection {
            reason: TaskRejectReason::ScopeClosed,
            ..
        })
    ));
    let sibling = facilities.scope();
    let handle = accepted(
        sibling
            .spawner()
            .spawn(TaskPoolKind::Io, request(1), |_| ()),
    );
    assert_eq!(facilities.run_pending_for_tests().executed, 1);
    assert!(handle.is_finished());
}

#[test]
fn runtime_scope_close_waits_for_its_work_and_preserves_a_sibling_runtime() {
    let facilities = Arc::new(TaskFacilities::try_new(test_config(4)).unwrap());
    let mut a = App::new();
    a.add_plugin(TaskPlugin::shared(facilities.clone()))
        .unwrap();
    let mut b = App::new();
    b.add_plugin(TaskPlugin::shared(facilities.clone()))
        .unwrap();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let running = accepted(a.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Io,
        request(1),
        move |_| {
            entered.send(()).unwrap();
            let _ = released.recv_timeout(Duration::from_secs(10));
        },
    ));
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    // Replacing the facade cannot replace the already registered activity's close responsibility.
    a.insert_resource(inline_pools(2)).unwrap();
    let mut first = start_runtime(a, Duration::from_millis(30));
    let mut second = start_runtime(b, Duration::from_secs(1));
    request_stop(&mut first);
    drive_until_state(&mut first, RuntimeState::CloseIncomplete);
    assert!(
        !first
            .world()
            .resource::<TaskScopeCloseReport>()
            .is_complete()
    );
    assert!(running.is_finished());
    let sibling = accepted(second.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Compute,
        request(1),
        |_| 8,
    ));
    wait_until(|| sibling.is_finished());
    release.send(()).unwrap();
    first.request_control(RuntimeControl::RetryClose);
    drive_until_state(&mut first, RuntimeState::Stopped);
    assert!(first.world().contains_resource::<TaskPools>());
    assert!(!first.world().contains_resource::<TaskShutdownReport>());
    assert!(
        first
            .world()
            .resource::<TaskScopeCloseReport>()
            .is_complete()
    );
    let after = accepted(second.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Io,
        request(1),
        |_| 9,
    ));
    wait_until(|| after.is_finished());
    request_stop(&mut second);
    drive_until_state(&mut second, RuntimeState::Stopped);
    assert!(!second.world().contains_resource::<TaskPools>());
    assert!(facilities.stats().for_kind(TaskPoolKind::Io).live_workers > 0);
    assert!(facilities.shutdown_blocking().unwrap().is_complete());
}

#[test]
fn rejected_scope_registration_retains_accepted_work_in_runtime_retirement() {
    let facilities = TaskFacilities::try_new(test_config(2)).unwrap();
    let scope = facilities.scope();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let handle = accepted(
        scope
            .spawner()
            .spawn(TaskPoolKind::Io, request(1), move |_| {
                entered.send(()).unwrap();
                let _ = released.recv_timeout(Duration::from_secs(10));
            }),
    );
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    let mut app = App::new();
    assert!(
        app.register_plugin_runtime_close_participant(
            TASK_POOLS_SHUTDOWN_OBLIGATION,
            RuntimeCloseParticipantId::new("test.rejected-scope"),
            scope,
        )
        .is_err()
    );
    assert!(!handle.is_finished());
    let mut runtime = start_runtime(app, Duration::from_millis(30));
    request_stop(&mut runtime);
    drive_until_state(&mut runtime, RuntimeState::CloseIncomplete);
    assert!(handle.is_finished());
    assert!(
        !runtime
            .world()
            .resource::<TaskScopeCloseReport>()
            .is_complete()
    );
    release.send(()).unwrap();
    runtime.request_control(RuntimeControl::RetryClose);
    drive_until_state(&mut runtime, RuntimeState::Stopped);
    assert!(
        runtime
            .world()
            .resource::<TaskScopeCloseReport>()
            .is_complete()
    );
}

#[test]
fn managed_scope_cleanup_failure_completes_physically_and_fails_stop() {
    let facilities = Arc::new(TaskFacilities::inline_for_tests(test_config(2)).unwrap());
    let mut app = App::new();
    app.add_plugin(TaskPlugin::shared(facilities.clone()))
        .unwrap();
    let capture = PanicOnDrop;
    let _pending = accepted(app.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Io,
        request(1),
        move |_| drop(capture),
    ));
    let mut runtime = start_runtime(app, Duration::from_secs(2));
    request_stop(&mut runtime);
    drive_until_state(&mut runtime, RuntimeState::Stopped);
    let report = runtime.world().resource::<TaskScopeCloseReport>();
    assert!(report.is_complete());
    assert!(report.cleanup_failed);
    assert!(runtime.close_evidence().has_terminal_failure());
    assert!(runtime.close_evidence().pending_participants().is_empty());
}

#[test]
fn physical_facility_drop_retains_disposal_without_forging_activity_return() {
    let facilities = TaskFacilities::inline_for_tests(test_config(2)).unwrap();
    let mut scope = facilities.scope();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let capture = HeldDrop {
        entered,
        release: released,
    };
    let pending = accepted(
        scope
            .spawner()
            .spawn(TaskPoolKind::Io, request(1), move |_| drop(capture)),
    );
    drop(facilities);
    started.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(pending.is_finished());
    assert!(!scope.poll_close().is_complete());
    release.send(()).unwrap();
    wait_until(|| scope.poll_close().is_complete());
}

#[test]
fn scope_poll_alone_recovers_a_failed_disposal_reaper_start() {
    let facilities = TaskFacilities::inline_for_tests(test_config(2)).unwrap();
    let mut scope = facilities.scope();
    let drops = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    struct CountDrop(Arc<std::sync::atomic::AtomicUsize>);
    impl Drop for CountDrop {
        fn drop(&mut self) {
            self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
    let capture = CountDrop(drops.clone());
    let pending = accepted(
        scope
            .spawner()
            .spawn(TaskPoolKind::Io, request(1), move |_| drop(capture)),
    );
    super::runtime::with_reaper_spawn_failure_for_tests(|| {
        scope.begin_close();
        assert_eq!(pending.return_observer().state(), TaskReturnState::Pending);
        wait_until(|| scope.poll_close().is_complete());
        assert_eq!(pending.return_observer().state(), TaskReturnState::Returned);
        assert_eq!(drops.load(std::sync::atomic::Ordering::SeqCst), 1);
    });
}
