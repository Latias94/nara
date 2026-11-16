use std::{
    sync::{
        Condvar,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    thread::ThreadId,
    time::{Duration, Instant},
};

use bevy_tasks::{ComputeTaskPool, TaskPoolBuilder};
use nara_app::{
    RuntimeCloseCause, RuntimeClosePolicy, RuntimeControl, RuntimeControlRequestResult,
    RuntimeFaultKind, RuntimeState, runtime_quarantine_status,
};
use nara_ecs::{
    ScheduleLabel,
    bevy_ecs::{
        error::BevyError,
        schedule::{MultiThreadedExecutor, SingleThreadedExecutor},
    },
};

use super::*;

#[derive(Debug, Default)]
struct PeerState {
    threads: [Option<ThreadId>; 2],
    active: usize,
    max_active: usize,
    completed: usize,
    timed_out: bool,
}

struct PeerProbe {
    state: Mutex<PeerState>,
    changed: Condvar,
    deadline: Instant,
}

impl PeerProbe {
    fn new(deadline: Instant) -> Self {
        Self {
            state: Mutex::new(PeerState::default()),
            changed: Condvar::new(),
            deadline,
        }
    }

    fn run(&self, peer: usize) {
        let mut state = self.state.lock().unwrap();
        state.threads[peer] = Some(thread::current().id());
        state.active += 1;
        state.max_active = state.max_active.max(state.active);
        self.changed.notify_all();
        while state.threads.iter().any(Option::is_none) {
            let Some(remaining) = self.deadline.checked_duration_since(Instant::now()) else {
                state.timed_out = true;
                break;
            };
            let (next, _) = self.changed.wait_timeout(state, remaining).unwrap();
            state = next;
        }
        state.active -= 1;
        state.completed += 1;
    }

    fn assert_overlap(&self, schedule: &str) {
        let state = self.state.lock().unwrap();
        assert!(
            !state.timed_out,
            "peer deadline exceeded in {schedule}: {state:?}"
        );
        assert_eq!(
            state.completed, 2,
            "incomplete peers in {schedule}: {state:?}"
        );
        assert_eq!(state.active, 0, "unjoined peers in {schedule}: {state:?}");
        assert_eq!(
            state.max_active, 2,
            "serialized peers in {schedule}: {state:?}"
        );
        assert!(
            state.threads.iter().all(Option::is_some),
            "{schedule}: {state:?}"
        );
        assert_ne!(state.threads[0], state.threads[1], "{schedule}: {state:?}");
    }
}

fn add_peers(app: &mut App, schedule: impl ScheduleLabel, deadline: Instant) -> Arc<PeerProbe> {
    let probe = Arc::new(PeerProbe::new(deadline));
    let left = Arc::clone(&probe);
    let right = Arc::clone(&probe);
    app.add_systems(schedule, (move || left.run(0), move || right.run(1)))
        .unwrap();
    probe
}

fn assert_parallel_managed_execution() {
    let driver = thread::current().id();
    let mut configuration = App::new();
    configuration
        .set_execution_profile(ExecutionProfile::Parallel)
        .unwrap();
    let identity = configuration.configuration_fingerprint();
    let plan = configuration.plugin_plan_fingerprint();
    let sealed = configuration.seal().unwrap();
    assert_eq!(sealed.execution_profile(), ExecutionProfile::Parallel);
    assert_eq!(sealed.configuration_fingerprint(), identity);
    assert_eq!(sealed.plugin_plan_fingerprint(), plan);
    drop(sealed);

    let (mut app, owner_trace) = tracked_app();
    app.set_execution_profile(ExecutionProfile::Parallel)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut peers = Vec::new();
    for stage in StartupStage::ALL {
        peers.push((format!("{stage:?}"), add_peers(&mut app, stage, deadline)));
    }
    for stage in CoreStage::ALL {
        peers.push((format!("{stage:?}"), add_peers(&mut app, stage, deadline)));
    }
    peers.push((
        "frame intent".to_owned(),
        add_peers(&mut app, nara_app::__private::FrameIntent, deadline),
    ));
    peers.push((
        "frame cleanup".to_owned(),
        add_peers(&mut app, nara_app::__private::FrameCleanup, deadline),
    ));

    let mut runtime = RuntimeInstance::start(app).unwrap();
    assert_eq!(runtime.execution_profile(), ExecutionProfile::Parallel);
    let outcome = runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 1);
    assert_eq!(runtime.execution_profile(), ExecutionProfile::Parallel);
    let before_close = owner_trace.lock().unwrap().clone();
    let stop = runtime.request_control(RuntimeControl::Stop);
    for _ in 0..4 {
        if runtime.state() == RuntimeState::Stopped {
            break;
        }
        runtime.drive(Duration::ZERO).unwrap();
        assert_eq!(runtime.execution_profile(), ExecutionProfile::Parallel);
    }
    let stopped = runtime.state();
    let evidence = runtime.finish().unwrap();

    assert!(matches!(stop, RuntimeControlRequestResult::Accepted(_)));
    assert_eq!(stopped, RuntimeState::Stopped);
    assert!(evidence.is_empty());
    assert_eq!(before_close.startup, [driver]);
    assert_eq!(before_close.update, [driver]);
    assert!(before_close.drops.is_empty());
    assert_eq!(owner_trace.lock().unwrap().drops, [driver]);
    for (schedule, probe) in peers {
        probe.assert_overlap(&schedule);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ScheduleLabel)]
struct CustomSchedule;

fn assert_custom_executors_are_preserved() {
    let driver = thread::current().id();
    let mut serial_app = App::new();
    let parallel_peers = add_peers(
        &mut serial_app,
        CustomSchedule,
        Instant::now() + Duration::from_secs(10),
    );
    serial_app
        .get_schedule_mut(CustomSchedule)
        .unwrap()
        .unwrap()
        .set_executor(MultiThreadedExecutor::new());
    let mut serial_runtime = RuntimeInstance::start(serial_app).unwrap();
    assert_eq!(serial_runtime.execution_profile(), ExecutionProfile::Serial);
    serial_runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert_eq!(parallel_peers.state.lock().unwrap().completed, 0);
    serial_runtime.run_schedule(CustomSchedule).unwrap();
    assert_eq!(serial_runtime.execution_profile(), ExecutionProfile::Serial);
    assert!(serial_runtime.finish().unwrap().is_empty());
    parallel_peers.assert_overlap("custom parallel schedule in a Serial App");

    let calls = Arc::new(Mutex::new(Vec::new()));
    let first = Arc::clone(&calls);
    let second = Arc::clone(&calls);
    let mut parallel_app = App::new();
    parallel_app
        .set_execution_profile(ExecutionProfile::Parallel)
        .unwrap()
        .add_systems(
            CustomSchedule,
            (
                move || first.lock().unwrap().push(thread::current().id()),
                move || second.lock().unwrap().push(thread::current().id()),
            ),
        )
        .unwrap();
    parallel_app
        .get_schedule_mut(CustomSchedule)
        .unwrap()
        .unwrap()
        .set_executor(SingleThreadedExecutor::new());
    let mut parallel_runtime = RuntimeInstance::start(parallel_app).unwrap();
    parallel_runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert!(calls.lock().unwrap().is_empty());
    parallel_runtime.run_schedule(CustomSchedule).unwrap();
    assert_eq!(
        parallel_runtime.execution_profile(),
        ExecutionProfile::Parallel
    );
    assert!(parallel_runtime.finish().unwrap().is_empty());
    assert_eq!(*calls.lock().unwrap(), [driver, driver]);
}

fn assert_parallel_fault_retains_finite_close_custody() {
    let driver = thread::current().id();
    let quarantine_before = runtime_quarantine_status().current_thread_retained();
    let gate = CloseGate::default();
    let failures = Arc::new(AtomicUsize::new(0));
    let later_runs = Arc::new(AtomicUsize::new(0));
    let (mut app, owner_trace) = tracked_app();
    let failed = Arc::clone(&failures);
    let later = Arc::clone(&later_runs);
    app.set_execution_profile(ExecutionProfile::Parallel)
        .unwrap()
        .add_systems(CoreStage::Update, move || -> Result<(), BevyError> {
            failed.fetch_add(1, Ordering::Relaxed);
            Err(BevyError::error(std::io::Error::other(
                "injected parallel profile failure",
            )))
        })
        .unwrap()
        .add_systems(CoreStage::PostUpdate, move || {
            later.fetch_add(1, Ordering::Relaxed);
        })
        .unwrap();
    let mut runtime = RuntimeInstance::start_with_options(
        app,
        gate.obligations(),
        RuntimeClosePolicy::new(Duration::ZERO),
    )
    .unwrap();

    let first = runtime.drive(Duration::ZERO).unwrap_err();
    let repeated = runtime.drive(Duration::ZERO).unwrap_err();
    let faulted = runtime.state();
    let faulted_profile = runtime.execution_profile();
    let reporter_fault = runtime.fault_reporter().fault();
    let stop = runtime.request_control(RuntimeControl::Stop);
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let incomplete = runtime.state();
    let incomplete_profile = runtime.execution_profile();
    let incomplete_evidence = runtime.close_evidence().clone();
    let before_release = owner_trace.lock().unwrap().clone();
    gate.released.store(true, Ordering::Release);
    let retry = runtime.request_control(RuntimeControl::RetryClose);
    for _ in 0..4 {
        if runtime.state() == RuntimeState::Stopped {
            break;
        }
        runtime.drive(Duration::ZERO).unwrap();
    }
    let stopped = runtime.state();
    let stopped_profile = runtime.execution_profile();
    let retained_fault = runtime.fault().cloned();
    let evidence = runtime.finish().unwrap();

    assert_eq!(first.fault().kind(), RuntimeFaultKind::System);
    assert_eq!(first, repeated);
    assert_eq!(reporter_fault.as_ref(), Some(first.fault()));
    assert_eq!(retained_fault.as_ref(), Some(first.fault()));
    assert_eq!(failures.load(Ordering::Relaxed), 1);
    assert_eq!(later_runs.load(Ordering::Relaxed), 0);
    assert_eq!(faulted, RuntimeState::Faulted);
    assert_eq!(faulted_profile, ExecutionProfile::Parallel);
    assert!(matches!(stop, RuntimeControlRequestResult::Accepted(_)));
    assert_eq!(incomplete, RuntimeState::CloseIncomplete);
    assert_eq!(incomplete_profile, ExecutionProfile::Parallel);
    assert_eq!(
        incomplete_evidence.causes(),
        &[RuntimeCloseCause::DeadlineExceeded]
    );
    assert_eq!(incomplete_evidence.pending_participants().len(), 1);
    assert_eq!(
        incomplete_evidence.pending_participants()[0].participant(),
        CLOSE_PARTICIPANT
    );
    assert!(before_release.drops.is_empty());
    assert!(matches!(retry, RuntimeControlRequestResult::Accepted(_)));
    assert_eq!(stopped, RuntimeState::Stopped);
    assert_eq!(stopped_profile, ExecutionProfile::Parallel);
    assert_eq!(evidence.causes(), incomplete_evidence.causes());
    assert!(evidence.pending_participants().is_empty());
    assert!(!evidence.has_terminal_failure());
    assert_eq!(gate.begins.load(Ordering::Relaxed), 1);
    assert!(gate.polls.load(Ordering::Relaxed) > 0);
    assert_eq!(*gate.completions.lock().unwrap(), [driver]);
    let trace = owner_trace.lock().unwrap();
    assert_eq!(trace.startup, [driver]);
    assert_eq!(trace.update, [driver]);
    assert_eq!(trace.drops, [driver]);
    assert_eq!(
        runtime_quarantine_status().current_thread_retained(),
        quarantine_before
    );
}

fn assert_original_process_global_pool(pool: &'static ComputeTaskPool) {
    let current = ComputeTaskPool::try_get()
        .expect("runtime cleanup must retain the Host-initialized process-global pool");
    assert!(std::ptr::eq(pool, current));
    assert_eq!(current.thread_num(), 2);
}

#[test]
fn host_initialized_process_global_pool_admits_parallel_and_survives_runtime_cleanup() {
    assert!(ComputeTaskPool::try_get().is_none());
    assert_parallel_refused_before_startup(ExecutionProfileError::ComputePoolUnavailable);
    assert!(
        ComputeTaskPool::try_get().is_none(),
        "refused Parallel admission must not initialize a default pool"
    );

    let pool = ComputeTaskPool::get_or_init(|| {
        TaskPoolBuilder::new()
            .num_threads(2)
            .thread_name("nara-execution-profile".to_owned())
            .build()
    });
    assert_original_process_global_pool(pool);
    assert_serial_default();
    assert_update_final_deferred(ExecutionProfile::Serial);
    assert_original_process_global_pool(pool);
    assert_parallel_managed_execution();
    assert_update_final_deferred(ExecutionProfile::Parallel);
    assert_original_process_global_pool(pool);
    assert_custom_executors_are_preserved();
    assert_original_process_global_pool(pool);
    assert_parallel_fault_retains_finite_close_custody();
    assert_original_process_global_pool(pool);

    let (sender, receiver) = mpsc::sync_channel(1);
    pool.spawn(async move {
        let _ = sender.send(thread::current().id());
    })
    .detach();
    let worker = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("the original process-global pool must execute work after runtime cleanup");
    assert_ne!(worker, thread::current().id());
}
