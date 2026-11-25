use std::time::Duration;

use nara_app::{
    App, CoreStage, FixedTime, RealTime, RuntimeControl, RuntimeControlFailure,
    RuntimeControlRequestResult, RuntimeControlStatus, RuntimeFaultKind, RuntimeFrameKind,
    RuntimeFrameStatus, RuntimeInstance, RuntimeState,
};
use nara_ecs::{ResMut, Resource, bevy_ecs::error::BevyError};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Resource)]
struct Runs {
    tasks: u32,
    fixed: u32,
    render: u32,
    cleanup: u32,
}

fn instrumented_app() -> App {
    let mut app = App::new();
    app.init_resource::<Runs>().unwrap();
    app.add_systems(CoreStage::TaskUpdate, |mut runs: ResMut<Runs>| {
        runs.tasks += 1;
    })
    .unwrap();
    app.add_systems(CoreStage::FixedUpdate, |mut runs: ResMut<Runs>| {
        runs.fixed += 1;
    })
    .unwrap();
    app.add_systems(CoreStage::Render, |mut runs: ResMut<Runs>| {
        runs.render += 1;
    })
    .unwrap();
    app.add_systems(CoreStage::Cleanup, |mut runs: ResMut<Runs>| {
        runs.cleanup += 1;
    })
    .unwrap();
    app
}

#[test]
fn running_paused_and_exact_step_outcomes_retain_their_original_frame_kind() {
    let mut runtime = RuntimeInstance::start(instrumented_app()).unwrap();
    let step = FixedTime::DEFAULT_TIMESTEP;

    let running = runtime.drive(step).unwrap();
    assert_eq!(running.state(), RuntimeState::Running);
    assert_eq!(running.frame_kind(), Some(RuntimeFrameKind::FullFrame));
    assert_eq!(running.frame().unwrap().status.fixed_steps, 1);
    assert_eq!(runtime.world().resource::<FixedTime>().tick(), 1);

    let RuntimeControlRequestResult::Accepted(pause) =
        runtime.request_control(RuntimeControl::Pause)
    else {
        panic!("a running runtime must accept Pause");
    };
    let paused = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(paused.state(), RuntimeState::Paused);
    assert_eq!(paused.frame_kind(), Some(RuntimeFrameKind::FullFrame));
    assert_eq!(paused.frame().unwrap().status.fixed_steps, 0);
    assert_eq!(
        runtime.control_status(pause),
        Some(RuntimeControlStatus::Applied)
    );
    let paused_runs = *runtime.world().resource::<Runs>();
    assert_eq!(
        paused_runs,
        Runs {
            tasks: 2,
            fixed: 1,
            render: 2,
            cleanup: 2,
        }
    );

    let RuntimeControlRequestResult::Accepted(ticket) =
        runtime.request_control(RuntimeControl::StepFixedTick)
    else {
        panic!("a paused runtime must accept an exact fixed step");
    };
    let exact = runtime.drive(step).unwrap();
    let exact_frame = exact.frame().unwrap();
    assert_eq!(exact.state(), RuntimeState::Paused);
    assert_eq!(exact.frame_kind(), Some(RuntimeFrameKind::ExactFixedTick));
    assert_eq!(exact_frame.status.fixed_steps, 1);
    assert_eq!(exact_frame.status.fixed_tick, 2);
    assert_eq!(exact_frame.status.real_delta, step);
    assert_eq!(exact_frame.status.virtual_delta, step);
    assert_eq!(runtime.world().resource::<FixedTime>().tick(), 2);
    assert_eq!(
        runtime.control_status(ticket),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(
        *runtime.world().resource::<Runs>(),
        Runs {
            fixed: paused_runs.fixed + 1,
            ..paused_runs
        }
    );

    let next_paused = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(next_paused.state(), RuntimeState::Paused);
    assert_eq!(next_paused.frame_kind(), Some(RuntimeFrameKind::FullFrame));
    assert_eq!(next_paused.frame().unwrap().status.fixed_steps, 0);
    assert_eq!(runtime.world().resource::<FixedTime>().tick(), 2);
    assert_eq!(
        *runtime.world().resource::<Runs>(),
        Runs {
            tasks: 3,
            fixed: 2,
            render: 3,
            cleanup: 3,
        }
    );
    assert_eq!(exact.frame_kind(), Some(RuntimeFrameKind::ExactFixedTick));
    assert_eq!(exact.frame(), Some(exact_frame));
    assert_eq!(paused.frame_kind(), Some(RuntimeFrameKind::FullFrame));
    assert_eq!(running.frame_kind(), Some(RuntimeFrameKind::FullFrame));
    runtime.finish().unwrap();
}

#[test]
fn stopping_and_stopped_drives_have_no_frame_or_frame_kind() {
    let mut runtime = RuntimeInstance::start(instrumented_app()).unwrap();
    let step = FixedTime::DEFAULT_TIMESTEP;
    runtime.drive(step).unwrap();
    let time = *runtime.world().resource::<RealTime>();
    let status = *runtime.world().resource::<RuntimeFrameStatus>();
    let runs = *runtime.world().resource::<Runs>();
    let RuntimeControlRequestResult::Accepted(ticket) =
        runtime.request_control(RuntimeControl::Stop)
    else {
        panic!("a running runtime must accept Stop");
    };

    for state in [
        RuntimeState::Stopping,
        RuntimeState::Stopped,
        RuntimeState::Stopped,
    ] {
        let outcome = runtime.drive(step * 4).unwrap();
        assert_eq!(outcome.state(), state);
        assert_eq!(outcome.frame(), None);
        assert_eq!(outcome.frame_kind(), None);
    }

    assert_eq!(
        runtime.control_status(ticket),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(*runtime.world().resource::<RealTime>(), time);
    assert_eq!(*runtime.world().resource::<RuntimeFrameStatus>(), status);
    assert_eq!(*runtime.world().resource::<Runs>(), runs);
    runtime.finish().unwrap();
}

fn fail_fixed_tick() -> Result<(), BevyError> {
    Err(BevyError::error(std::io::Error::other(
        "injected exact fixed-tick failure",
    )))
}

#[test]
fn failed_exact_step_preserves_its_error_failed_control_and_prior_frame_kind() {
    let mut app = App::new();
    app.add_systems(CoreStage::FixedUpdate, fail_fixed_tick)
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    let paused = runtime.drive(Duration::ZERO).unwrap();
    let paused_frame = paused.frame().unwrap();
    let RuntimeControlRequestResult::Accepted(ticket) =
        runtime.request_control(RuntimeControl::StepFixedTick)
    else {
        panic!("a paused runtime must accept an exact fixed step");
    };

    let error = runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap_err();
    assert_eq!(error.fault().kind(), RuntimeFaultKind::System);
    assert_eq!(runtime.state(), RuntimeState::Faulted);
    assert_eq!(runtime.fault(), Some(error.fault()));
    assert_eq!(runtime.drive(Duration::ZERO).unwrap_err(), error);
    assert_eq!(
        runtime.control_status(ticket),
        Some(RuntimeControlStatus::Failed(
            RuntimeControlFailure::RuntimeFaulted
        ))
    );
    assert_eq!(paused.frame_kind(), Some(RuntimeFrameKind::FullFrame));
    assert_eq!(paused.frame(), Some(paused_frame));

    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    let stopping = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(stopping.state(), RuntimeState::Stopping);
    assert_eq!(stopping.frame(), None);
    assert_eq!(stopping.frame_kind(), None);
    assert_eq!(runtime.fault(), Some(error.fault()));
    runtime.finish().unwrap();
}
