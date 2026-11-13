use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Duration,
};

use nara_app::{
    App, CoreStage, FixedCatchUpPolicy, FixedTime, PluginId, RealTime, RenderTime, RuntimeControl,
    RuntimeControlRequestResult, RuntimeControlStatus, RuntimeFault, RuntimeFaultKind,
    RuntimeFaultReporter, RuntimeFrameKind, RuntimeInstance, RuntimeState, RuntimeTimeContinuation,
    RuntimeTimeContinuationError, RuntimeTimeContinuationRejection, RuntimeTimeSettings,
    VirtualTime,
};
use nara_ecs::{ResMut, Resource, World};

const STEP: Duration = Duration::from_millis(20);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
struct Runs {
    tasks: u32,
    fixed: u32,
    update: u32,
}

fn fixed(policy: FixedCatchUpPolicy) -> FixedTime {
    FixedTime::new(STEP)
        .unwrap()
        .with_max_steps_per_frame(2)
        .unwrap()
        .with_max_debt_steps(4)
        .unwrap()
        .with_catch_up_policy(policy)
}

fn app(paused: bool, policy: FixedCatchUpPolicy) -> App {
    let mut app = App::new();
    app.insert_resource(RuntimeTimeSettings::default().with_paused(paused))
        .unwrap();
    app.insert_resource(fixed(policy)).unwrap();
    app.init_resource::<Runs>().unwrap();
    app.add_systems(CoreStage::TaskUpdate, |mut runs: ResMut<Runs>| {
        runs.tasks += 1
    })
    .unwrap();
    app.add_systems(CoreStage::FixedUpdate, |mut runs: ResMut<Runs>| {
        runs.fixed += 1
    })
    .unwrap();
    app.add_systems(CoreStage::Update, |mut runs: ResMut<Runs>| runs.update += 1)
        .unwrap();
    app
}

fn control(runtime: &mut RuntimeInstance, control: RuntimeControl) {
    assert!(matches!(
        runtime.request_control(control),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
}

fn phase(runtime: &RuntimeInstance) -> RuntimeTimeContinuation {
    runtime.capture_time_continuation().unwrap()
}

#[test]
fn nonzero_phase_preserves_local_history_and_matches_subsequent_tick_timing() {
    let mut source = RuntimeInstance::start(app(false, FixedCatchUpPolicy::DiscardExcess)).unwrap();
    source.drive(Duration::from_millis(13)).unwrap();
    control(&mut source, RuntimeControl::Pause);
    let captured = phase(&source);
    assert_eq!(captured.pending(), Duration::from_millis(13));
    let mut candidate =
        RuntimeInstance::start(app(true, FixedCatchUpPolicy::DiscardExcess)).unwrap();
    candidate.drive(Duration::ZERO).unwrap();
    let generation = candidate.generation();
    let history = (
        *candidate.world().resource::<RealTime>(),
        *candidate.world().resource::<VirtualTime>(),
        candidate.world().resource::<FixedTime>().tick(),
        candidate.world().resource::<FixedTime>().elapsed(),
        *candidate.world().resource::<Runs>(),
    );
    candidate.restore_time_continuation(captured).unwrap();
    assert_eq!(candidate.generation(), generation);
    assert_ne!(generation, source.generation());
    assert_eq!(
        history,
        (
            *candidate.world().resource::<RealTime>(),
            *candidate.world().resource::<VirtualTime>(),
            candidate.world().resource::<FixedTime>().tick(),
            candidate.world().resource::<FixedTime>().elapsed(),
            *candidate.world().resource::<Runs>(),
        )
    );
    let render = candidate.world().resource::<RenderTime>();
    assert_eq!(render.remainder, captured.remainder());
    assert!((render.interpolation_alpha - 0.65).abs() < f32::EPSILON);
    assert!(matches!(
        candidate.capture_time_continuation(),
        Err(RuntimeTimeContinuationError::Rejected(
            RuntimeTimeContinuationRejection::IncompletePausedFrame
        ))
    ));
    candidate.drive(Duration::ZERO).unwrap();
    assert_eq!(phase(&candidate), captured);
    control(&mut candidate, RuntimeControl::StepFixedTick);
    assert_eq!(
        candidate.world().resource::<FixedTime>().remainder(),
        captured.remainder()
    );
    assert!(candidate.capture_time_continuation().is_err());
    candidate.drive(Duration::ZERO).unwrap();
    assert_eq!(phase(&candidate), captured);
    let source_tick = source.world().resource::<FixedTime>().tick();
    let candidate_tick = candidate.world().resource::<FixedTime>().tick();
    control(&mut source, RuntimeControl::Resume);
    control(&mut candidate, RuntimeControl::Resume);
    for runtime in [&mut source, &mut candidate] {
        let next = runtime.drive(Duration::from_millis(7)).unwrap();
        assert_eq!(next.frame().unwrap().status.fixed_steps, 1);
        assert_eq!(
            runtime.world().resource::<FixedTime>().remainder(),
            Duration::ZERO
        );
    }
    assert_eq!(
        source.world().resource::<FixedTime>().tick(),
        source_tick + 1
    );
    assert_eq!(
        candidate.world().resource::<FixedTime>().tick(),
        candidate_tick + 1
    );
    source.finish().unwrap();
    candidate.finish().unwrap();
}

#[test]
fn elapsed_resume_preserves_whole_debt_and_real_wait_then_runs_the_original_cap() {
    let mut source = RuntimeInstance::start(app(false, FixedCatchUpPolicy::PreserveDebt)).unwrap();
    source.drive(Duration::from_millis(113)).unwrap();
    control(&mut source, RuntimeControl::Pause);
    let captured = phase(&source);
    assert_eq!(captured.pending(), Duration::from_millis(73));
    assert_eq!(captured.debt(), STEP * 3);
    let mut candidate =
        RuntimeInstance::start(app(true, FixedCatchUpPolicy::PreserveDebt)).unwrap();
    candidate.drive(Duration::ZERO).unwrap();
    candidate.restore_time_continuation(captured).unwrap();
    candidate.drive(Duration::ZERO).unwrap();
    for runtime in [&mut source, &mut candidate] {
        let real = runtime.world().resource::<RealTime>().elapsed;
        let virtual_time = *runtime.world().resource::<VirtualTime>();
        let runs = *runtime.world().resource::<Runs>();
        let RuntimeControlRequestResult::Accepted(ticket) =
            runtime.request_control(RuntimeControl::Resume)
        else {
            panic!("Resume must be accepted");
        };
        let resumed = runtime.drive_elapsed(Duration::from_secs(60)).unwrap();
        assert_eq!(resumed.state(), RuntimeState::Running);
        assert_eq!(resumed.frame_kind(), Some(RuntimeFrameKind::FullFrame));
        assert_eq!(
            runtime.control_status(ticket),
            Some(RuntimeControlStatus::Applied)
        );
        assert_eq!(
            runtime.world().resource::<RealTime>().elapsed,
            real + Duration::from_secs(60)
        );
        assert_eq!(
            runtime.world().resource::<VirtualTime>().elapsed,
            virtual_time.elapsed
        );
        assert_eq!(
            runtime.world().resource::<FixedTime>().debt(),
            captured.debt()
        );
        assert_eq!(runtime.world().resource::<Runs>().fixed, runs.fixed);
        assert_eq!(runtime.world().resource::<Runs>().update, runs.update);
        assert_eq!(runtime.world().resource::<Runs>().tasks, runs.tasks + 1);
        assert_eq!(
            runtime
                .drive_elapsed(Duration::ZERO)
                .unwrap()
                .frame()
                .unwrap()
                .status
                .fixed_steps,
            2
        );
        assert_eq!(runtime.world().resource::<FixedTime>().debt(), STEP);
        assert_eq!(
            runtime
                .drive_elapsed(Duration::from_millis(7))
                .unwrap()
                .frame()
                .unwrap()
                .status
                .fixed_steps,
            2
        );
        assert_eq!(
            runtime.world().resource::<FixedTime>().remainder(),
            Duration::ZERO
        );
    }
    source.finish().unwrap();
    candidate.finish().unwrap();
}

#[test]
fn explicit_next_interval_resume_and_elapsed_resume_keep_distinct_existing_policies() {
    for elapsed in [false, true] {
        let mut runtime =
            RuntimeInstance::start(app(true, FixedCatchUpPolicy::DiscardExcess)).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        let RuntimeControlRequestResult::Accepted(ticket) =
            runtime.request_control(RuntimeControl::Resume)
        else {
            panic!("Resume must be accepted");
        };
        let result = if elapsed {
            runtime.drive_elapsed(STEP)
        } else {
            runtime.drive(STEP)
        }
        .unwrap();
        assert_eq!(
            result.frame().unwrap().status.fixed_steps,
            u32::from(!elapsed)
        );
        assert_eq!(
            runtime.control_status(ticket),
            Some(RuntimeControlStatus::Applied)
        );
        assert_eq!(runtime.world().resource::<RealTime>().elapsed, STEP);
        assert_eq!(
            runtime.world().resource::<VirtualTime>().elapsed,
            if elapsed { Duration::ZERO } else { STEP }
        );
        runtime.finish().unwrap();
    }
}

#[test]
fn boundary_and_control_refusals_preserve_phase_and_stopped_state() {
    let mut runtime = RuntimeInstance::start(app(true, FixedCatchUpPolicy::DiscardExcess)).unwrap();
    assert!(matches!(
        runtime.capture_time_continuation(),
        Err(RuntimeTimeContinuationError::Rejected(
            RuntimeTimeContinuationRejection::IncompletePausedFrame
        ))
    ));
    runtime.drive(Duration::ZERO).unwrap();
    let captured = phase(&runtime);
    let RuntimeControlRequestResult::Accepted(resume) =
        runtime.request_control(RuntimeControl::Resume)
    else {
        panic!("Resume must be accepted");
    };
    assert!(matches!(
        runtime.restore_time_continuation(captured),
        Err(RuntimeTimeContinuationError::Rejected(
            RuntimeTimeContinuationRejection::ControlPending
        ))
    ));
    let RuntimeControlRequestResult::Accepted(stop) = runtime.request_control(RuntimeControl::Stop)
    else {
        panic!("Stop must supersede Resume");
    };
    assert!(
        runtime
            .drive_elapsed(Duration::from_secs(60))
            .unwrap()
            .frame()
            .is_none()
    );
    while runtime.state() != RuntimeState::Stopped {
        runtime.drive_elapsed(Duration::ZERO).unwrap();
    }
    assert_eq!(
        runtime.control_status(stop),
        Some(RuntimeControlStatus::Applied)
    );
    assert_ne!(
        runtime.control_status(resume),
        Some(RuntimeControlStatus::Applied)
    );
    assert!(matches!(
        runtime.capture_time_continuation(),
        Err(RuntimeTimeContinuationError::Rejected(
            RuntimeTimeContinuationRejection::Unavailable {
                state: RuntimeState::Stopped
            }
        ))
    ));
    assert!(runtime.fault().is_none());
    runtime.finish().unwrap();
}

#[test]
fn every_policy_mismatch_refuses_without_clock_or_schedule_mutation() {
    let mut runtime = RuntimeInstance::start(app(true, FixedCatchUpPolicy::DiscardExcess)).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let original = phase(&runtime);
    let runs = *runtime.world().resource::<Runs>();
    for changed in 0..6 {
        let mut settings = RuntimeTimeSettings::default();
        let mut config = fixed(FixedCatchUpPolicy::DiscardExcess);
        match changed {
            0 => config.set_timestep(Duration::from_millis(21)).unwrap(),
            1 => config.set_max_steps_per_frame(3).unwrap(),
            2 => config.set_max_debt_steps(5).unwrap(),
            3 => config
                .set_catch_up_policy(FixedCatchUpPolicy::PreserveDebt)
                .unwrap(),
            4 => settings.set_time_scale(0.5).unwrap(),
            5 => settings.set_max_delta(Duration::from_millis(251)).unwrap(),
            _ => unreachable!(),
        }
        let incompatible =
            RuntimeTimeContinuation::new(&settings, &config, Duration::from_millis(7)).unwrap();
        assert_eq!(
            runtime.restore_time_continuation(incompatible),
            Err(RuntimeTimeContinuationError::Rejected(
                RuntimeTimeContinuationRejection::PolicyMismatch
            ))
        );
        assert_eq!(phase(&runtime), original);
        assert_eq!(*runtime.world().resource::<Runs>(), runs);
    }
    runtime.finish().unwrap();
}

#[test]
fn portable_value_checks_preserved_debt_and_exact_scale_bits() {
    let settings = RuntimeTimeSettings::default();
    let config = fixed(FixedCatchUpPolicy::PreserveDebt);
    assert!(
        RuntimeTimeContinuation::new(&settings, &config, STEP * 4 + Duration::from_millis(19))
            .is_ok()
    );
    assert!(matches!(
        RuntimeTimeContinuation::new(&settings, &config, STEP * 5),
        Err(RuntimeTimeContinuationRejection::DebtExceeded {
            whole_ticks: 5,
            maximum: 4
        })
    ));
    assert!(
        RuntimeTimeContinuation::new(
            &settings,
            &fixed(FixedCatchUpPolicy::DiscardExcess),
            Duration::MAX
        )
        .is_ok()
    );
    let zero = RuntimeTimeContinuation::new(
        &settings.with_time_scale(0.0).unwrap(),
        &config,
        Duration::ZERO,
    )
    .unwrap();
    let negative_zero = RuntimeTimeContinuation::new(
        &settings.with_time_scale(-0.0).unwrap(),
        &config,
        Duration::ZERO,
    )
    .unwrap();
    assert_ne!(zero, negative_zero);
}

#[test]
fn post_application_authority_fault_retains_the_actual_applied_phase() {
    let armed = Arc::new(AtomicBool::new(false));
    let guard = Arc::clone(&armed);
    let mut configured = app(true, FixedCatchUpPolicy::DiscardExcess);
    configured
        .__install_runtime_authority_validator(
            PluginId::new("nara.test.time-application-guard"),
            Arc::new(move |world, _| {
                if guard.load(Ordering::Acquire)
                    && world.resource::<FixedTime>().remainder() == Duration::from_millis(13)
                {
                    world
                        .resource::<RuntimeFaultReporter>()
                        .report(RuntimeFault::engine(
                            RuntimeFaultKind::RuntimeAuthority,
                            "nara.test.time-original-reported-fault",
                        ));
                    Err(RuntimeFault::engine(
                        RuntimeFaultKind::RuntimeAuthority,
                        "nara.test.time-application-guard",
                    ))
                } else {
                    Ok(())
                }
            }),
        )
        .unwrap();
    let mut runtime = RuntimeInstance::start(configured).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let value = RuntimeTimeContinuation::new(
        &RuntimeTimeSettings::default(),
        &fixed(FixedCatchUpPolicy::DiscardExcess),
        Duration::from_millis(13),
    )
    .unwrap();
    armed.store(true, Ordering::Release);
    let error = runtime.restore_time_continuation(value).unwrap_err();
    assert!(matches!(
        error,
        RuntimeTimeContinuationError::Faulted {
            applied: true,
            rejection: None,
            ..
        }
    ));
    assert_eq!(error.fault(), runtime.fault());
    assert_eq!(
        error.fault().unwrap().source(),
        "nara.test.time-original-reported-fault"
    );
    assert_eq!(runtime.state(), RuntimeState::Faulted);
    assert_eq!(
        runtime.world().resource::<FixedTime>().remainder(),
        value.remainder()
    );
    assert_eq!(runtime.world().resource::<Runs>().fixed, 0);
    assert!(!runtime.capture_time_continuation().unwrap_err().applied());
    runtime.finish().unwrap();
}

#[test]
fn a_validator_reporting_a_fault_with_ok_cannot_return_capture_or_initialize_time() {
    for restore in [false, true] {
        let checks = Arc::new(AtomicUsize::new(usize::MAX));
        let observed = Arc::clone(&checks);
        let original = RuntimeFault::engine(
            RuntimeFaultKind::RequiredService,
            "nara.test.time-reported-before-application",
        );
        let reported = original.clone();
        let mut configured = app(true, FixedCatchUpPolicy::DiscardExcess);
        configured
            .__install_runtime_authority_validator(
                PluginId::new("nara.test.time-fault-with-ok"),
                Arc::new(move |world, _| {
                    if observed.load(Ordering::Acquire) != usize::MAX
                        && observed.fetch_add(1, Ordering::AcqRel) == 1
                    {
                        world
                            .resource::<RuntimeFaultReporter>()
                            .report(reported.clone());
                    }
                    Ok(())
                }),
            )
            .unwrap();
        let mut runtime = RuntimeInstance::start(configured).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        let fixed_before = *runtime.world().resource::<FixedTime>();
        let render_before = *runtime.world().resource::<RenderTime>();
        let real_before = *runtime.world().resource::<RealTime>();
        let virtual_before = *runtime.world().resource::<VirtualTime>();
        let runs_before = *runtime.world().resource::<Runs>();
        checks.store(0, Ordering::Release);
        let error = if restore {
            let value = RuntimeTimeContinuation::new(
                &RuntimeTimeSettings::default(),
                &fixed(FixedCatchUpPolicy::DiscardExcess),
                Duration::from_millis(13),
            )
            .unwrap();
            runtime.restore_time_continuation(value).unwrap_err()
        } else {
            runtime.capture_time_continuation().unwrap_err()
        };
        assert!(!error.applied());
        assert_eq!(error.fault(), Some(&original));
        assert_eq!(runtime.fault(), Some(&original));
        assert_eq!(runtime.state(), RuntimeState::Faulted);
        assert_eq!(*runtime.world().resource::<FixedTime>(), fixed_before);
        assert_eq!(*runtime.world().resource::<RenderTime>(), render_before);
        assert_eq!(*runtime.world().resource::<RealTime>(), real_before);
        assert_eq!(*runtime.world().resource::<VirtualTime>(), virtual_before);
        assert_eq!(*runtime.world().resource::<Runs>(), runs_before);
        runtime.finish().unwrap();
    }
}

#[test]
fn current_policy_changes_at_last_are_captured_and_old_values_refuse() {
    let mut configured = app(true, FixedCatchUpPolicy::DiscardExcess);
    configured
        .add_systems(CoreStage::Last, |mut time: ResMut<FixedTime>| {
            time.set_max_steps_per_frame(3).unwrap();
        })
        .unwrap();
    let mut runtime = RuntimeInstance::start(configured).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let current = phase(&runtime);
    assert_eq!(current.max_steps_per_frame(), 3);
    let stale = RuntimeTimeContinuation::new(
        &RuntimeTimeSettings::default(),
        &fixed(FixedCatchUpPolicy::DiscardExcess),
        Duration::from_millis(13),
    )
    .unwrap();
    assert_eq!(
        runtime.restore_time_continuation(stale),
        Err(RuntimeTimeContinuationError::Rejected(
            RuntimeTimeContinuationRejection::PolicyMismatch
        ))
    );
    assert_eq!(phase(&runtime), current);
    runtime.finish().unwrap();
}

#[test]
fn missing_time_resources_refuse_without_partial_phase_or_schedule_changes() {
    for missing in 0..3 {
        let mut configured = app(true, FixedCatchUpPolicy::DiscardExcess);
        configured
            .add_systems(CoreStage::Last, move |world: &mut World| match missing {
                0 => {
                    world.remove_resource::<RuntimeTimeSettings>();
                }
                1 => {
                    world.remove_resource::<FixedTime>();
                }
                2 => {
                    world.remove_resource::<RenderTime>();
                }
                _ => unreachable!(),
            })
            .unwrap();
        let mut runtime = RuntimeInstance::start(configured).unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        let clocks = (
            *runtime.world().resource::<RealTime>(),
            *runtime.world().resource::<VirtualTime>(),
            runtime.world().get_resource::<FixedTime>().copied(),
            runtime.world().get_resource::<RenderTime>().copied(),
            *runtime.world().resource::<Runs>(),
        );
        let value = RuntimeTimeContinuation::new(
            &RuntimeTimeSettings::default(),
            &fixed(FixedCatchUpPolicy::DiscardExcess),
            Duration::from_millis(13),
        )
        .unwrap();
        assert_eq!(
            runtime.restore_time_continuation(value),
            Err(RuntimeTimeContinuationError::Rejected(
                RuntimeTimeContinuationRejection::MissingTimeResources
            ))
        );
        assert_eq!(
            clocks,
            (
                *runtime.world().resource::<RealTime>(),
                *runtime.world().resource::<VirtualTime>(),
                runtime.world().get_resource::<FixedTime>().copied(),
                runtime.world().get_resource::<RenderTime>().copied(),
                *runtime.world().resource::<Runs>(),
            )
        );
        if missing < 2 {
            assert_eq!(
                runtime.capture_time_continuation(),
                Err(RuntimeTimeContinuationError::Rejected(
                    RuntimeTimeContinuationRejection::MissingTimeResources
                ))
            );
        }
        runtime.finish().unwrap();
    }
}
