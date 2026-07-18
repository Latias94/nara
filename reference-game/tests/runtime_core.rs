use std::time::{Duration, Instant};

use nara::{
    app::{
        RuntimeCandidate, RuntimeControl, RuntimeControlRequestResult, RuntimeControlStatus,
        RuntimeState,
    },
    prelude::{App, FixedTime, HeadlessRuntimePlugins},
};
use nara_reference_game::{ReferenceGamePlugin, ReferenceTracerSeedPlugin, TracerSnapshot};

fn accepted_ticket(result: RuntimeControlRequestResult) -> nara::app::RuntimeControlTicket {
    match result {
        RuntimeControlRequestResult::Accepted(ticket) => ticket,
        RuntimeControlRequestResult::Rejected(rejection) => {
            panic!("runtime control rejected: {rejection:?}")
        }
    }
}

#[test]
fn manifest_free_code_first_runtime_pauses_steps_resumes_and_stops() {
    let mut app = App::new();
    app.add_plugins((
        HeadlessRuntimePlugins,
        ReferenceGamePlugin,
        ReferenceTracerSeedPlugin,
    ))
    .unwrap();
    let candidate = RuntimeCandidate::admit(app.seal().unwrap()).unwrap();
    let mut runtime = candidate.complete_startup().unwrap().promote();

    assert_eq!(runtime.state(), RuntimeState::Running);
    assert_eq!(
        TracerSnapshot::capture(runtime.world()).unwrap(),
        TracerSnapshot::initial()
    );

    let pause = accepted_ticket(runtime.request_control(RuntimeControl::Pause));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Paused);
    assert_eq!(
        runtime.control_status(pause),
        Some(RuntimeControlStatus::Applied)
    );

    let step = accepted_ticket(runtime.request_control(RuntimeControl::StepFixedTick));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(runtime.state(), RuntimeState::Paused);
    assert_eq!(
        runtime.control_status(step),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(runtime.world().resource::<FixedTime>().tick(), 1);

    accepted_ticket(runtime.request_control(RuntimeControl::Resume));
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    assert_eq!(runtime.world().resource::<FixedTime>().tick(), 2);

    accepted_ticket(runtime.request_control(RuntimeControl::Stop));
    let deadline = Instant::now() + Duration::from_secs(5);
    while runtime.state() != RuntimeState::Stopped && Instant::now() < deadline {
        runtime.drive(Duration::ZERO).unwrap();
        std::thread::yield_now();
    }
    assert_eq!(runtime.state(), RuntimeState::Stopped);
}
