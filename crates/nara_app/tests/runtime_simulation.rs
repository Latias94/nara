use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::Duration;

use nara_app::{
    App, CoreStage, FixedTime, RealTime, RuntimeControl, RuntimeControlFailure,
    RuntimeControlRejection, RuntimeControlRequestResult, RuntimeControlStatus, RuntimeInstance,
    RuntimeState, VirtualTime,
};
use nara_ecs::{Res, ResMut, Resource, World};

#[derive(Resource)]
struct Readiness(Arc<AtomicBool>);

#[derive(Default, Resource)]
struct Runs {
    tasks: u32,
    simulation: u32,
    fixed: u32,
    render: u32,
    cleanup: u32,
}

fn simulation_ready(world: &World) -> bool {
    world.resource::<Readiness>().0.load(Ordering::Acquire)
}

fn fixture(ready: Arc<AtomicBool>, restore_in_tasks: bool) -> RuntimeInstance {
    let mut app = App::new();
    app.insert_resource(Readiness(ready)).unwrap();
    app.init_resource::<Runs>().unwrap();
    app.add_simulation_condition(simulation_ready).unwrap();
    app.add_systems(
        CoreStage::TaskUpdate,
        move |ready: Res<Readiness>, mut runs: ResMut<Runs>| {
            runs.tasks += 1;
            if restore_in_tasks {
                ready.0.store(true, Ordering::Release);
            }
        },
    )
    .unwrap();
    for stage in [
        CoreStage::PreUpdate,
        CoreStage::Update,
        CoreStage::PostUpdate,
    ] {
        app.add_systems(stage, |mut runs: ResMut<Runs>| runs.simulation += 1)
            .unwrap();
    }
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

#[test]
fn simulation_readiness_is_latched_before_clocks_and_maintenance_can_restore_it() {
    let ready = Arc::new(AtomicBool::new(false));
    let mut runtime = fixture(ready, true);
    let step = FixedTime::DEFAULT_TIMESTEP;
    let held = runtime.drive(step * 10).unwrap().frame().unwrap();
    assert_eq!(held.status.fixed_steps, 0);
    assert_eq!(held.status.virtual_delta, Duration::ZERO);
    let runs = runtime.world().resource::<Runs>();
    assert_eq!(
        (
            runs.tasks,
            runs.simulation,
            runs.fixed,
            runs.render,
            runs.cleanup
        ),
        (1, 0, 0, 1, 1)
    );
    assert_eq!(runtime.world().resource::<RealTime>().elapsed, step * 10);
    assert_eq!(
        runtime.world().resource::<VirtualTime>().elapsed,
        Duration::ZERO
    );

    let resumed = runtime.drive(step).unwrap().frame().unwrap();
    assert_eq!(resumed.status.fixed_steps, 1);
    assert_eq!(resumed.status.virtual_delta, step);
    let runs = runtime.world().resource::<Runs>();
    assert_eq!(
        (
            runs.tasks,
            runs.simulation,
            runs.fixed,
            runs.render,
            runs.cleanup
        ),
        (2, 3, 1, 2, 2)
    );
    assert_eq!(runtime.world().resource::<VirtualTime>().elapsed, step);
    runtime.finish().unwrap();
}

#[test]
fn simulation_readiness_rejects_step_and_rechecks_an_already_admitted_step() {
    let ready = Arc::new(AtomicBool::new(false));
    let mut runtime = fixture(Arc::clone(&ready), false);
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Paused);
    assert_eq!(
        runtime.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Rejected(RuntimeControlRejection::SimulationUnavailable)
    );

    ready.store(true, Ordering::Release);
    let RuntimeControlRequestResult::Accepted(ticket) =
        runtime.request_control(RuntimeControl::StepFixedTick)
    else {
        panic!("a ready paused runtime accepts Step");
    };
    ready.store(false, Ordering::Release);
    let outcome = runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert_eq!(outcome.state(), RuntimeState::Paused);
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 0);
    assert_eq!(
        runtime.control_status(ticket),
        Some(RuntimeControlStatus::Failed(
            RuntimeControlFailure::SimulationUnavailable
        ))
    );
    assert!(runtime.fault().is_none());
    assert_eq!(runtime.world().resource::<Runs>().fixed, 0);

    ready.store(true, Ordering::Release);
    assert!(matches!(
        runtime.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Accepted(_)
    ));
    assert_eq!(
        runtime
            .drive(Duration::ZERO)
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
