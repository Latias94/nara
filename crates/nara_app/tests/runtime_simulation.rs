use nara_app::{App, FixedTime, RuntimeInstance, RuntimeState, RuntimeTimeSettings, StartupStage};
use nara_ecs::ResMut;

fn startup_pause_fixture(configured: bool, startup: Option<bool>) -> RuntimeInstance {
    let mut app = App::new();
    app.insert_resource(RuntimeTimeSettings::default().with_paused(configured))
        .unwrap();
    if let Some(paused) = startup {
        app.add_systems(
            StartupStage::Core,
            move |mut settings: ResMut<RuntimeTimeSettings>| settings.set_paused(paused),
        )
        .unwrap();
    }
    RuntimeInstance::start(app).unwrap()
}

#[test]
fn configured_pause_survives_managed_start() {
    let mut runtime = startup_pause_fixture(true, None);
    assert_eq!(runtime.state(), RuntimeState::Paused);
    let frame = runtime
        .drive(FixedTime::DEFAULT_TIMESTEP)
        .unwrap()
        .frame()
        .unwrap();
    assert_eq!(frame.status.fixed_steps, 0);
    runtime.finish().unwrap();
}

#[test]
fn the_startup_schedule_sets_the_initial_runtime_mode() {
    for (configured, startup, expected) in [
        (false, true, RuntimeState::Paused),
        (true, false, RuntimeState::Running),
    ] {
        let mut runtime = startup_pause_fixture(configured, Some(startup));
        assert_eq!(runtime.state(), expected);
        assert_eq!(
            runtime.world().resource::<RuntimeTimeSettings>().paused(),
            startup
        );
        let frame = runtime
            .drive(FixedTime::DEFAULT_TIMESTEP)
            .unwrap()
            .frame()
            .unwrap();
        assert_eq!(frame.status.fixed_steps, u32::from(!startup));
        runtime.finish().unwrap();
    }
}
