use std::{process::Command, time::Duration};

use nara::{
    app::{PluginError, PluginLifecycleState},
    prelude::{App, ComponentRegistry, FixedTime, MinimalPlugins},
};
use nara_reference_game::{
    REFERENCE_GAME_PLUGIN_ID, ReferenceGameError, ReferenceGamePlugin, TracerSnapshot,
    run_headless_ticks,
};

#[test]
fn fixed_tick_tracer_is_deterministic_and_zero_time_does_not_advance() {
    let first = run_headless_ticks(3).unwrap();
    let second = run_headless_ticks(3).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.tick, 3);
    assert_eq!(first, TracerSnapshot::after_three_ticks());

    let mut app = App::new();
    app.add_plugins(MinimalPlugins).unwrap();
    app.add_plugin(ReferenceGamePlugin).unwrap();

    let startup = app.run_once(Duration::ZERO).unwrap();
    assert_eq!(startup.status.fixed_steps, 0);
    assert_eq!(
        TracerSnapshot::capture(app.world()).unwrap(),
        TracerSnapshot::initial()
    );

    for expected_tick in 1..=3 {
        let frame = app.run_once(FixedTime::DEFAULT_TIMESTEP).unwrap();
        assert_eq!(frame.status.fixed_steps, 1);
        assert_eq!(app.world().resource::<FixedTime>().tick(), expected_tick);
        if expected_tick == 2 {
            let before_zero = TracerSnapshot::capture(app.world()).unwrap();
            let zero = app.run_once(Duration::ZERO).unwrap();
            assert_eq!(zero.status.fixed_steps, 0);
            assert_eq!(TracerSnapshot::capture(app.world()).unwrap(), before_zero);
        }
    }
    assert_eq!(
        TracerSnapshot::capture(app.world()).unwrap(),
        TracerSnapshot::after_three_ticks()
    );
}

#[test]
fn headless_binary_runs_the_public_tracer() {
    let output = Command::new(env!("CARGO_BIN_EXE_headless"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "tick=3 enemy_hp=7\n"
    );
}

#[test]
fn missing_component_registry_fails_preflight_without_poisoning_the_app() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).unwrap();
    app.world_mut()
        .unwrap()
        .remove_resource::<ComponentRegistry>();

    let Err(error) = app.add_plugin(ReferenceGamePlugin) else {
        panic!("reference-game plugin unexpectedly accepted a missing component registry");
    };

    assert!(matches!(
        error,
        PluginError::ComponentRegistrationFailed {
            plugin,
            ref component,
            ..
        } if plugin == REFERENCE_GAME_PLUGIN_ID && component == "component-schema-catalog"
    ));
    assert_eq!(
        app.plugin_lifecycle_state(),
        PluginLifecycleState::Configuring
    );

    app.world_mut()
        .unwrap()
        .insert_resource(ComponentRegistry::new());
    app.add_plugin(ReferenceGamePlugin).unwrap();
}

#[test]
fn snapshot_capture_reports_a_missing_fixed_clock() {
    let mut app = App::new();
    app.add_plugins(MinimalPlugins).unwrap();
    app.add_plugin(ReferenceGamePlugin).unwrap();
    app.run_once(Duration::ZERO).unwrap();
    app.world_mut().unwrap().remove_resource::<FixedTime>();

    assert!(matches!(
        TracerSnapshot::capture(app.world()),
        Err(ReferenceGameError::MissingResource("FixedTime"))
    ));
}
