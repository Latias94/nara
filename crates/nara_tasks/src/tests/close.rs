use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use nara_app::{
    AddPluginsError, App, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
    PluginLifecycleState, PluginPlan, RuntimeAdmissionReservation, RuntimeCandidateRetirementState,
    RuntimeClosePolicy, RuntimeControl, RuntimeControlRequestResult, RuntimeInstance,
    RuntimeObligationLedger, RuntimeState,
};

use super::*;

const FAILING_TASK_OWNER_PLUGIN_ID: PluginId =
    PluginId::new("nara.tasks.test.failing-caller-owner");
const FAILING_TASK_OWNER_PLUGIN_DECLARATION: PluginDeclaration =
    PluginDeclaration::new(FAILING_TASK_OWNER_PLUGIN_ID, PluginCategory::Runtime);

struct FailingTaskOwnerPlugin;

impl Plugin for FailingTaskOwnerPlugin {
    fn declaration() -> &'static PluginDeclaration {
        &FAILING_TASK_OWNER_PLUGIN_DECLARATION
    }

    fn build(&self, _app: &mut App) -> Result<(), PluginError> {
        Err(PluginError::SetupFailed {
            plugin: FAILING_TASK_OWNER_PLUGIN_ID,
            message: "injected build failure".to_owned(),
        })
    }
}

struct SlowPendingDrop(Duration);

impl Drop for SlowPendingDrop {
    fn drop(&mut self) {
        thread::sleep(self.0);
    }
}

struct SlowPendingDropThread {
    delay: Duration,
    dropped_on: mpsc::Sender<thread::ThreadId>,
}

impl Drop for SlowPendingDropThread {
    fn drop(&mut self) {
        let thread_id = thread::current().id();
        thread::sleep(self.delay);
        let _ = self.dropped_on.send(thread_id);
    }
}

fn start_runtime(app: App, close_timeout: Duration) -> RuntimeInstance {
    let sealed = app.seal().unwrap();
    let candidate = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            sealed,
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::new(close_timeout),
        )
        .unwrap();
    match candidate.complete_startup() {
        Ok(ready) => ready.promote(),
        Err(failure) => {
            panic!("candidate startup failed: {:?}", failure.fault())
        }
    }
}

fn request_stop(runtime: &mut RuntimeInstance) {
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
}

fn drive_until_state(runtime: &mut RuntimeInstance, expected: RuntimeState) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime.state() != expected && Instant::now() < deadline {
        runtime.drive(Duration::ZERO).unwrap();
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(runtime.state(), expected);
}

fn wait_finished<T>(handle: &TaskHandle<T>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !handle.is_finished() && Instant::now() < deadline {
        thread::yield_now();
    }
    assert!(handle.is_finished(), "task did not finish before deadline");
}

#[test]
fn caller_owned_pools_remain_explicitly_shutdown_capable_after_app_poison() {
    let pools = TaskPools::try_new(test_config(1)).unwrap();
    let mut app = App::new();
    app.insert_resource(pools).unwrap();

    let Err(error) = app.add_plugins(FailingTaskOwnerPlugin) else {
        panic!("the injected plugin build must fail");
    };

    assert!(matches!(
        error,
        AddPluginsError::Plugin(PluginError::SetupFailed {
            plugin: FAILING_TASK_OWNER_PLUGIN_ID,
            ..
        })
    ));
    assert_eq!(app.plugin_lifecycle_state(), PluginLifecycleState::Poisoned);
    let report = app
        .world()
        .resource::<TaskPools>()
        .shutdown_blocking()
        .unwrap();
    assert!(!report.timed_out());
    assert!(
        TaskPoolKind::ALL
            .into_iter()
            .all(|kind| report.for_kind(kind).joined_workers > 0)
    );
}

#[test]
fn shutdown_retains_an_uncooperative_worker_and_completes_on_retry() {
    let pools = TaskPools::try_new(test_config(1)).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (exited_tx, exited_rx) = mpsc::channel();
    let mut handle = accepted(pools.spawn(TaskPoolKind::Io, request(1), move |_| {
        started_tx.send(()).unwrap();
        let _ = release_rx.recv();
        exited_tx.send(()).unwrap();
        1_u32
    }));
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();

    let started = Instant::now();
    let report = pools.shutdown_blocking().unwrap();
    assert!(started.elapsed() < Duration::from_secs(3));
    let io = report.for_kind(TaskPoolKind::Io);
    assert!(io.drain_timed_out);
    assert!(io.cancel_timed_out);
    assert!(io.join_timed_out);
    assert!(matches!(
        handle.try_take(),
        Some(TaskTerminal::Cancelled(TaskCancellation {
            reason: TaskCancellationReason::PoolShutdown,
            before_start: false,
        }))
    ));
    drop(release_tx);
    exited_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    let completed = pools
        .shutdown_blocking()
        .unwrap()
        .for_kind(TaskPoolKind::Io);
    assert!(completed.drain_timed_out);
    assert!(completed.cancel_timed_out);
    assert!(completed.join_timed_out);
    assert_eq!(completed.joined_workers, 1);
    let repeated = pools
        .shutdown_blocking()
        .unwrap()
        .for_kind(TaskPoolKind::Io);
    assert_eq!(repeated, completed);
    assert_eq!(pools.stats().for_kind(TaskPoolKind::Io).shutdowns, 1);
    let drop_started = Instant::now();
    drop(pools);
    assert!(drop_started.elapsed() < Duration::from_millis(50));
    thread::sleep(Duration::from_millis(20));
}

#[test]
fn dropping_an_unfinished_close_owner_retains_worker_handles_until_exit() {
    super::runtime::reap_abandoned_workers_for_tests();
    assert_eq!(
        super::runtime::retained_abandoned_worker_count_for_tests(),
        0
    );

    let pools = TaskPools::try_new(test_config(1)).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (exited_tx, exited_rx) = mpsc::channel();
    let _handle = accepted(pools.spawn(TaskPoolKind::Io, request(1), move |_| {
        started_tx.send(()).unwrap();
        let _ = release_rx.recv();
        exited_tx.send(()).unwrap();
    }));
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();

    drop(pools);

    assert!(super::runtime::retained_abandoned_worker_count_for_tests() >= 1);
    drop(release_tx);
    exited_rx.recv_timeout(Duration::from_secs(10)).unwrap();

    let deadline = Instant::now() + Duration::from_secs(10);
    while super::runtime::retained_abandoned_worker_count_for_tests() != 0
        && Instant::now() < deadline
    {
        super::runtime::reap_abandoned_workers_for_tests();
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(
        super::runtime::retained_abandoned_worker_count_for_tests(),
        0
    );
}

#[test]
fn cancellation_deadline_interrupts_large_pending_queue_and_retry_resumes_it() {
    let kind = TaskKindConfig::new(ItemLimit::ONE, items(257));
    let config = TaskPoolConfig::new(
        kind,
        kind,
        kind,
        TaskShutdownPolicy::new(TimeLimit::MIN, TimeLimit::MIN, TimeLimit::MIN),
    )
    .unwrap();
    let mut pools = TaskPools::inline_for_tests(config).unwrap();

    for domain in 0..257 {
        drop(accepted(pools.spawn(
            TaskPoolKind::Io,
            request(domain),
            |_| (),
        )));
    }

    let incomplete = pools.shutdown_executor_for_tests(TaskPoolKind::Io);
    assert!(incomplete.cancel_timed_out);
    assert!(incomplete.cancelled_pending < 257);

    let completed = pools.shutdown_executor_for_tests(TaskPoolKind::Io);
    assert!(completed.cancel_timed_out);
    assert_eq!(completed.cancelled_pending, 257);
}

#[test]
fn dropping_pending_task_owners_transfers_slow_destructors_without_blocking() {
    let pools = inline_pools(64);
    for domain in 0..64 {
        let slow_drop = SlowPendingDrop(Duration::from_millis(10));
        drop(accepted(pools.spawn(
            TaskPoolKind::Io,
            request(domain),
            move |_| {
                drop(slow_drop);
            },
        )));
    }

    let started = Instant::now();
    drop(pools);
    assert!(
        started.elapsed() < Duration::from_millis(200),
        "dropping a task owner synchronously ran pending destructors"
    );
}

#[test]
fn a_closed_pool_rejects_and_never_falls_back_to_inline_execution() {
    let pools = inline_pools(1);
    let _ = pools.shutdown_blocking().unwrap();
    let calls = Arc::new(AtomicUsize::new(0));
    let task_calls = calls.clone();

    let outcome = pools.spawn(TaskPoolKind::Io, request(1), move |_| {
        task_calls.fetch_add(1, Ordering::SeqCst);
    });

    assert!(matches!(
        outcome,
        TaskSpawnOutcome::Rejected(TaskRejection {
            reason: TaskRejectReason::PoolClosed,
            ..
        })
    ));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(pools.run_pending_for_tests().executed, 0);
}

#[test]
fn executor_repeated_shutdown_preserves_the_first_report() {
    let mut pools = inline_pools(1);
    let mut handle = accepted(pools.spawn(TaskPoolKind::Io, request(1), |_| 1_u32));

    let first = pools.shutdown_executor_for_tests(TaskPoolKind::Io);
    let repeated = pools.shutdown_executor_for_tests(TaskPoolKind::Io);

    assert_eq!(first.cancelled_pending, 1);
    assert_eq!(repeated, first);
    assert_eq!(repeated.cancelled_pending, 1);
    assert!(matches!(
        handle.try_take(),
        Some(TaskTerminal::Cancelled(TaskCancellation {
            reason: TaskCancellationReason::PoolShutdown,
            before_start: true,
        }))
    ));
}

#[test]
fn plugin_shutdown_shuts_down_only_plugin_owned_pools() {
    let mut owned_app = App::new();
    owned_app
        .add_plugin(TaskPlugin::new(test_config(1)))
        .unwrap();
    assert!(owned_app.world().contains_resource::<TaskPools>());
    let mut owned_runtime = start_runtime(owned_app, Duration::from_secs(2));
    request_stop(&mut owned_runtime);
    drive_until_state(&mut owned_runtime, RuntimeState::Stopped);
    assert!(!owned_runtime.world().contains_resource::<TaskPools>());
    assert!(
        owned_runtime
            .world()
            .contains_resource::<TaskShutdownReport>()
    );

    let mut external_app = App::new();
    external_app.insert_resource(inline_pools(1)).unwrap();
    external_app
        .add_plugin(TaskPlugin::new(test_config(1)))
        .unwrap();
    let mut external_runtime = start_runtime(external_app, Duration::from_secs(2));
    request_stop(&mut external_runtime);
    drive_until_state(&mut external_runtime, RuntimeState::Stopped);
    assert!(external_runtime.world().contains_resource::<TaskPools>());
    assert!(
        !external_runtime
            .world()
            .contains_resource::<TaskShutdownReport>()
    );

    let mut replaced_app = App::new();
    replaced_app
        .add_plugin(TaskPlugin::new(test_config(1)))
        .unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (exited_tx, exited_rx) = mpsc::channel();
    let _original_handle = accepted(replaced_app.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Io,
        request(1),
        move |_| {
            started_tx.send(()).unwrap();
            let _ = release_rx.recv();
            exited_tx.send(()).unwrap();
        },
    ));
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    replaced_app.insert_resource(inline_pools(2)).unwrap();
    let mut replaced_runtime = start_runtime(replaced_app, Duration::from_secs(2));
    request_stop(&mut replaced_runtime);
    drive_until_state(&mut replaced_runtime, RuntimeState::CloseIncomplete);
    assert_eq!(
        replaced_runtime
            .world()
            .resource::<TaskPools>()
            .config()
            .kind(TaskPoolKind::Io)
            .pending()
            .get(),
        2
    );
    assert!(
        !replaced_runtime
            .world()
            .contains_resource::<TaskShutdownReport>()
    );

    drop(release_tx);
    exited_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(matches!(
        replaced_runtime.request_control(RuntimeControl::RetryClose),
        RuntimeControlRequestResult::Accepted(_)
    ));
    drive_until_state(&mut replaced_runtime, RuntimeState::Stopped);
    assert_eq!(
        replaced_runtime
            .world()
            .resource::<TaskPools>()
            .config()
            .kind(TaskPoolKind::Io)
            .pending()
            .get(),
        2
    );
    assert!(
        replaced_runtime
            .world()
            .contains_resource::<TaskShutdownReport>()
    );
}

#[test]
fn task_plugin_construction_failure_keeps_partial_workers_in_the_runtime_ledger() {
    let config = test_config(1)
        .with_kind(
            TaskPoolKind::Io,
            TaskKindConfig::new(items(2), ItemLimit::ONE),
        )
        .unwrap();
    let plan = PluginPlan::resolve(plugin(config)).unwrap();
    let reservation = RuntimeAdmissionReservation::try_acquire().unwrap();
    let (result, probe) =
        TaskPools::with_worker_spawn_failure_for_tests(TaskPoolKind::Io, 1, || {
            plan.instantiate_runtime_candidate(
                reservation,
                RuntimeObligationLedger::new(),
                RuntimeClosePolicy::default(),
            )
        });
    let mut failure = result.unwrap_err();

    assert_eq!(probe.started(), 1);
    assert_eq!(probe.cleanup_polls(), 0);
    let deadline = Instant::now() + Duration::from_secs(10);
    while failure.retirement_state() != RuntimeCandidateRetirementState::Retired
        && Instant::now() < deadline
    {
        failure.drive_retirement();
        std::thread::yield_now();
    }

    assert_eq!(
        failure.retirement_state(),
        RuntimeCandidateRetirementState::Retired
    );
    assert!(probe.cleanup_polls() > 0);
    assert_eq!(probe.exited(), 1);
}

#[test]
fn runtime_close_retains_timed_out_task_owners_until_retry_completes() {
    let mut app = App::new();
    app.add_plugin(TaskPlugin::new(test_config(1))).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let (exited_tx, exited_rx) = mpsc::channel();
    let _handle = accepted(app.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Io,
        request(1),
        move |_| {
            started_tx.send(()).unwrap();
            let _ = release_rx.recv();
            exited_tx.send(()).unwrap();
        },
    ));
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();

    let mut runtime = start_runtime(app, Duration::from_secs(2));
    request_stop(&mut runtime);
    drive_until_state(&mut runtime, RuntimeState::CloseIncomplete);
    assert!(runtime.world().contains_resource::<TaskPools>());
    assert!(!runtime.world().contains_resource::<TaskShutdownReport>());

    drop(release_tx);
    exited_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::RetryClose),
        RuntimeControlRequestResult::Accepted(_)
    ));
    drive_until_state(&mut runtime, RuntimeState::Stopped);

    let report = runtime.world().resource::<TaskShutdownReport>();
    assert!(report.timed_out());
    assert_eq!(report.for_kind(TaskPoolKind::Io).joined_workers, 1);
    assert!(!runtime.world().contains_resource::<TaskPools>());
    thread::sleep(Duration::from_millis(20));
}

#[test]
fn managed_close_reaps_pending_jobs_off_the_driver_thread_before_completing() {
    let kind = TaskKindConfig::new(ItemLimit::ONE, items(2));
    let config = TaskPoolConfig::new(
        kind,
        kind,
        kind,
        TaskShutdownPolicy::new(
            TimeLimit::MIN,
            time(Duration::from_secs(1)),
            time(Duration::from_secs(1)),
        ),
    )
    .unwrap();
    let mut app = App::new();
    app.add_plugin(TaskPlugin::new(config)).unwrap();

    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let _running = accepted(app.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Io,
        request(1),
        move |_| {
            started_tx.send(()).unwrap();
            let _ = release_rx.recv();
        },
    ));
    started_rx.recv_timeout(Duration::from_secs(10)).unwrap();

    let (dropped_on_tx, dropped_on_rx) = mpsc::channel();
    let slow_drop = SlowPendingDropThread {
        delay: Duration::from_millis(300),
        dropped_on: dropped_on_tx,
    };
    let _pending = accepted(app.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Io,
        request(2),
        move |_| drop(slow_drop),
    ));

    let driver_thread = thread::current().id();
    let mut runtime = start_runtime(app, Duration::from_millis(150));
    request_stop(&mut runtime);
    let drive_started = Instant::now();
    runtime.drive(Duration::ZERO).unwrap();

    assert!(
        drive_started.elapsed() < Duration::from_millis(100),
        "managed close synchronously ran a slow pending-job destructor"
    );
    assert_eq!(runtime.state(), RuntimeState::Stopping);
    drive_until_state(&mut runtime, RuntimeState::CloseIncomplete);
    let dropped_on = dropped_on_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("the process reaper did not finish the pending-job destructor");
    assert_ne!(dropped_on, driver_thread);

    drop(release_tx);
    assert!(matches!(
        runtime.request_control(RuntimeControl::RetryClose),
        RuntimeControlRequestResult::Accepted(_)
    ));
    drive_until_state(&mut runtime, RuntimeState::Stopped);
    assert_eq!(
        runtime
            .world()
            .resource::<TaskShutdownReport>()
            .for_kind(TaskPoolKind::Io)
            .cancelled_pending,
        1
    );
}

#[test]
fn plugin_managed_task_pools_reject_standalone_shutdown() {
    let mut app = App::new();
    app.add_plugin(TaskPlugin::new(test_config(1))).unwrap();

    assert_eq!(
        app.world_mut()
            .unwrap()
            .resource_mut::<TaskPools>()
            .shutdown_blocking(),
        Err(TaskShutdownError::CloseOwnerTransferred)
    );
}

#[test]
fn reaper_spawn_failure_is_retryable_and_drains_fallback() {
    let mut harness = super::runtime::TaskOwnerReaperHarness::with_spawn_failures(1);
    let receipt = harness.retain_finished_worker();

    assert!(!receipt.is_complete());
    assert_eq!(harness.fallback_len(), 1);

    harness.retry();
    let deadline = Instant::now() + Duration::from_secs(2);
    while !receipt.is_complete() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }

    assert!(receipt.is_complete());
    assert_eq!(harness.fallback_len(), 0);
}

#[test]
fn partial_reaper_lane_spawn_failure_reclaims_started_lanes_before_retry() {
    let mut harness = super::runtime::TaskOwnerReaperHarness::with_lane_spawn_failure(1);
    let lane_probe = harness.lane_probe();
    let receipt = harness.retain_finished_worker();

    assert!(!receipt.is_complete());
    assert_eq!(harness.fallback_len(), 1);
    assert_eq!(lane_probe.live(), 0, "a partially started lane survived");

    harness.retry();
    let receipt_deadline = Instant::now() + Duration::from_secs(2);
    while !receipt.is_complete() && Instant::now() < receipt_deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert!(receipt.is_complete());
    assert_eq!(harness.fallback_len(), 0);

    let lane_start_deadline = Instant::now() + Duration::from_secs(2);
    while lane_probe.live() != 2 && Instant::now() < lane_start_deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(lane_probe.live(), 2);

    drop(harness);
    assert_eq!(
        lane_probe.live(),
        0,
        "harness shutdown did not join its drop lanes"
    );
}

#[test]
fn reaper_round_robins_owners_under_sustained_intake() {
    let mut harness = super::runtime::TaskOwnerReaperHarness::new();
    let (first_started_tx, first_started_rx) = mpsc::channel();
    let (first_release_tx, first_release_rx) = mpsc::channel::<()>();
    let mut first_drops: Vec<Box<dyn FnOnce() + Send>> = vec![Box::new(move || {
        let _ = first_started_tx.send(());
        let _ = first_release_rx.recv();
    })];
    for _ in 0..7 {
        first_drops.push(Box::new(|| thread::sleep(Duration::from_millis(40))));
    }
    let first_receipt = harness.retain_pending_drops(first_drops);
    first_started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("the reaper did not start the first owner");

    let (second_dropped_tx, second_dropped_rx) = mpsc::channel();
    let second_receipt = harness.retain_pending_drops(vec![Box::new(move || {
        let _ = second_dropped_tx.send(());
    })]);
    drop(first_release_tx);

    second_dropped_rx
        .recv_timeout(Duration::from_millis(150))
        .expect("one retained owner monopolized the reaper");
    let receipt_deadline = Instant::now() + Duration::from_secs(2);
    while !second_receipt.is_complete() && Instant::now() < receipt_deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert!(second_receipt.is_complete());

    let first_deadline = Instant::now() + Duration::from_secs(2);
    while !first_receipt.is_complete() && Instant::now() < first_deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert!(first_receipt.is_complete());
}

#[test]
fn one_blocked_reaper_owner_does_not_block_another_owner_receipt() {
    let mut harness = super::runtime::TaskOwnerReaperHarness::new();
    let (first_started_tx, first_started_rx) = mpsc::channel();
    let (first_release_tx, first_release_rx) = mpsc::channel::<()>();
    let first_receipt = harness.retain_pending_drops(vec![Box::new(move || {
        let _ = first_started_tx.send(());
        let _ = first_release_rx.recv();
    })]);
    first_started_rx
        .recv_timeout(Duration::from_secs(2))
        .expect("the reaper did not start the blocking owner");

    let (second_dropped_tx, second_dropped_rx) = mpsc::channel();
    let second_receipt = harness.retain_pending_drops(vec![Box::new(move || {
        let _ = second_dropped_tx.send(());
    })]);

    second_dropped_rx
        .recv_timeout(Duration::from_millis(500))
        .expect("a blocked owner prevented another owner from reaching its receipt");
    let second_receipt_deadline = Instant::now() + Duration::from_secs(2);
    while !second_receipt.is_complete() && Instant::now() < second_receipt_deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert!(
        second_receipt.is_complete(),
        "the second owner destructor ran without completing its receipt"
    );
    assert!(!first_receipt.is_complete());

    drop(first_release_tx);
    let cleanup_deadline = Instant::now() + Duration::from_secs(10);
    while !first_receipt.is_complete() && Instant::now() < cleanup_deadline {
        thread::sleep(Duration::from_millis(1));
    }
    assert!(first_receipt.is_complete());
}

#[test]
fn task_plugin_runtime_instances_keep_ownership_queues_and_counters_isolated() {
    let mut first_app = App::new();
    first_app
        .add_plugin(TaskPlugin::new(test_config(1)))
        .unwrap();
    let (first_started_tx, first_started_rx) = mpsc::channel();
    let (first_release_tx, first_release_rx) = mpsc::channel::<()>();
    let (first_exited_tx, first_exited_rx) = mpsc::channel();
    let first_handle = accepted(first_app.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Io,
        request(1),
        move |_| {
            first_started_tx.send(()).unwrap();
            let _ = first_release_rx.recv();
            first_exited_tx.send(()).unwrap();
        },
    ));
    first_started_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the first runtime task did not start");

    let mut second_app = App::new();
    second_app
        .add_plugin(TaskPlugin::new(test_config(2)))
        .unwrap();
    let mut first_runtime = start_runtime(first_app, Duration::from_millis(50));
    let mut second_runtime = start_runtime(second_app, Duration::from_secs(2));
    assert_ne!(first_runtime.generation(), second_runtime.generation());
    assert_eq!(first_handle.id().raw(), 1);
    assert_eq!(
        first_runtime
            .world()
            .resource::<TaskPools>()
            .stats()
            .for_kind(TaskPoolKind::Io)
            .running,
        1
    );
    assert_eq!(
        second_runtime
            .world()
            .resource::<TaskPools>()
            .stats()
            .for_kind(TaskPoolKind::Io)
            .started,
        0
    );

    request_stop(&mut first_runtime);
    drive_until_state(&mut first_runtime, RuntimeState::CloseIncomplete);
    assert!(first_runtime.world().contains_resource::<TaskPools>());
    assert!(second_runtime.world().contains_resource::<TaskPools>());

    let (second_completed_tx, second_completed_rx) = mpsc::channel();
    let mut second_handle = accepted(second_runtime.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Io,
        request(1),
        move |_| {
            second_completed_tx.send(()).unwrap();
            41_u32
        },
    ));
    assert_eq!(second_handle.id().raw(), 1);
    second_completed_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the second runtime did not complete work while the first was closing");
    wait_finished(&second_handle);
    assert!(matches!(
        second_handle.try_take(),
        Some(TaskTerminal::Completed(41))
    ));
    assert_eq!(
        first_runtime
            .world()
            .resource::<TaskPools>()
            .stats()
            .for_kind(TaskPoolKind::Io)
            .completed,
        0
    );
    assert_eq!(
        second_runtime
            .world()
            .resource::<TaskPools>()
            .stats()
            .for_kind(TaskPoolKind::Io)
            .completed,
        1
    );

    drop(first_release_tx);
    first_exited_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the first runtime task did not exit after release");
    assert!(matches!(
        first_runtime.request_control(RuntimeControl::RetryClose),
        RuntimeControlRequestResult::Accepted(_)
    ));
    drive_until_state(&mut first_runtime, RuntimeState::Stopped);
    assert!(!first_runtime.world().contains_resource::<TaskPools>());
    assert!(
        first_runtime
            .world()
            .contains_resource::<TaskShutdownReport>()
    );
    assert!(second_runtime.world().contains_resource::<TaskPools>());

    let (second_after_retire_tx, second_after_retire_rx) = mpsc::channel();
    let mut second_after_retire = accepted(second_runtime.world().resource::<TaskPools>().spawn(
        TaskPoolKind::Io,
        request(2),
        move |_| {
            second_after_retire_tx.send(()).unwrap();
            42_u32
        },
    ));
    assert_eq!(second_after_retire.id().raw(), 2);
    second_after_retire_rx
        .recv_timeout(Duration::from_secs(10))
        .expect("the second runtime stopped accepting work after the first retired");
    wait_finished(&second_after_retire);
    assert!(matches!(
        second_after_retire.try_take(),
        Some(TaskTerminal::Completed(42))
    ));
    assert_eq!(
        second_runtime
            .world()
            .resource::<TaskPools>()
            .stats()
            .for_kind(TaskPoolKind::Io)
            .completed,
        2
    );

    request_stop(&mut second_runtime);
    drive_until_state(&mut second_runtime, RuntimeState::Stopped);
    assert!(!second_runtime.world().contains_resource::<TaskPools>());
    assert!(
        second_runtime
            .world()
            .contains_resource::<TaskShutdownReport>()
    );
}
