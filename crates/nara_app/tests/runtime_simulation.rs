use std::time::Duration;

use nara_app::{
    App, CoreStage, FixedTime, RealTime, RuntimeControl, RuntimeControlRequestResult,
    RuntimeControlStatus, RuntimeInstance, RuntimeState, RuntimeTimeSettings, StartupStage,
    VirtualTime,
};
use nara_ecs::{ResMut, Resource};

#[derive(Default, Resource)]
struct Runs {
    tasks: u32,
    fixed: u32,
    render: u32,
    cleanup: u32,
}

fn startup_pause_fixture(configured: bool, startup: Option<bool>) -> RuntimeInstance {
    let mut app = App::new();
    app.insert_resource(RuntimeTimeSettings::default().with_paused(configured))
        .unwrap();
    app.init_resource::<Runs>().unwrap();
    if let Some(paused) = startup {
        app.add_systems(
            StartupStage::Core,
            move |mut settings: ResMut<RuntimeTimeSettings>| {
                settings.set_paused(paused);
            },
        )
        .unwrap();
    }
    app.add_systems(CoreStage::TaskUpdate, |mut runs: ResMut<Runs>| {
        runs.tasks += 1
    })
    .unwrap();
    app.add_systems(CoreStage::FixedUpdate, |mut runs: ResMut<Runs>| {
        runs.fixed += 1
    })
    .unwrap();
    app.add_systems(CoreStage::Render, |mut runs: ResMut<Runs>| runs.render += 1)
        .unwrap();
    app.add_systems(CoreStage::Cleanup, |mut runs: ResMut<Runs>| {
        runs.cleanup += 1
    })
    .unwrap();
    RuntimeInstance::start(app).unwrap()
}

fn assert_paused_start_controls(mut runtime: RuntimeInstance) {
    let step = FixedTime::DEFAULT_TIMESTEP;
    assert_eq!(runtime.state(), RuntimeState::Paused);
    assert!(runtime.world().resource::<RuntimeTimeSettings>().paused());
    let frame = runtime.drive(step * 4).unwrap();
    assert_eq!(frame.state(), RuntimeState::Paused);
    let status = frame.frame().unwrap().status;
    assert_eq!(status.fixed_steps, 0);
    assert_eq!(status.virtual_delta, Duration::ZERO);
    let runs = runtime.world().resource::<Runs>();
    assert_eq!(
        (runs.tasks, runs.fixed, runs.render, runs.cleanup),
        (1, 0, 1, 1)
    );
    assert_eq!(runtime.world().resource::<RealTime>().elapsed, step * 4);
    assert_eq!(
        runtime.world().resource::<VirtualTime>().elapsed,
        Duration::ZERO
    );

    let RuntimeControlRequestResult::Accepted(ticket) =
        runtime.request_control(RuntimeControl::StepFixedTick)
    else {
        panic!("a runtime paused at startup must accept an exact fixed step");
    };
    let stepped = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(stepped.state(), RuntimeState::Paused);
    assert_eq!(stepped.frame().unwrap().status.fixed_steps, 1);
    assert_eq!(
        runtime.control_status(ticket),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(runtime.world().resource::<Runs>().fixed, 1);
    assert_eq!(runtime.world().resource::<FixedTime>().tick(), 1);
    assert_eq!(runtime.world().resource::<VirtualTime>().elapsed, step);
    assert!(runtime.world().resource::<RuntimeTimeSettings>().paused());

    let RuntimeControlRequestResult::Accepted(ticket) =
        runtime.request_control(RuntimeControl::Resume)
    else {
        panic!("a runtime paused at startup must accept Resume");
    };
    let resumed = runtime.drive(step).unwrap();
    assert_eq!(resumed.state(), RuntimeState::Running);
    assert_eq!(resumed.frame().unwrap().status.fixed_steps, 1);
    assert_eq!(
        runtime.control_status(ticket),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(runtime.world().resource::<Runs>().fixed, 2);
    assert!(!runtime.world().resource::<RuntimeTimeSettings>().paused());
    runtime.finish().unwrap();
}

#[test]
fn configured_pause_survives_managed_start_and_accepts_step_and_resume() {
    assert_paused_start_controls(startup_pause_fixture(true, None));
}

#[test]
fn startup_pause_survives_managed_start_and_accepts_step_and_resume() {
    assert_paused_start_controls(startup_pause_fixture(false, Some(true)));
}

#[test]
fn startup_can_resume_a_runtime_configured_to_start_paused() {
    let mut runtime = startup_pause_fixture(true, Some(false));
    assert_eq!(runtime.state(), RuntimeState::Running);
    assert!(!runtime.world().resource::<RuntimeTimeSettings>().paused());
    assert_eq!(
        runtime
            .drive(FixedTime::DEFAULT_TIMESTEP)
            .unwrap()
            .frame()
            .unwrap()
            .status
            .fixed_steps,
        1
    );
    assert_eq!(runtime.world().resource::<Runs>().fixed, 1);
    runtime.finish().unwrap();
}
