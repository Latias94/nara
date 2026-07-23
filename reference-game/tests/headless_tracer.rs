#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{
    num::NonZeroU32,
    sync::{Arc, Mutex},
    time::Duration,
};

use nara::{
    app::{AddPluginsError, PluginLifecycleState, PluginPlanError},
    prelude::{App, HeadlessRuntimePlugins},
    project_host::HeadlessRunOutcome,
};
use nara_reference_game::{
    REFERENCE_GAME_PLUGIN_ID, ReferenceGamePlugin, WaveOutcome, bundled_wave_run,
    bundled_wave_run_with_completed_tick_observer,
};
use project_content_fixture::project_root_capability;

#[test]
fn typed_headless_run_returns_the_terminal_wave_snapshot() {
    let mut run = bundled_wave_run(
        project_root_capability(),
        NonZeroU32::new(96).expect("the maximum tick count is non-zero"),
    );

    let mut terminal_snapshot = None;
    for _ in 0..64 {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(snapshot) => {
                assert!(!report.diagnostics().has_errors(), "{report:#?}");
                terminal_snapshot = Some(snapshot.clone());
                break;
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                std::thread::park_timeout(Duration::from_millis(1));
            }
            HeadlessRunOutcome::Failed => panic!("typed reference-game run failed: {report:#?}"),
        }
    }
    let snapshot = terminal_snapshot.expect("typed reference-game cleanup should be bounded");

    assert_eq!(snapshot.outcome, WaveOutcome::Completed);
    assert_eq!(snapshot.tick, 49);
    assert_eq!(snapshot.score, 300);
    assert!(snapshot.enemies.is_empty());
}

#[test]
fn completed_tick_observer_receives_captured_wave_snapshots() {
    let observations = Arc::new(Mutex::new(Vec::new()));
    let observer = Arc::clone(&observations);
    let mut run = bundled_wave_run_with_completed_tick_observer(
        project_root_capability(),
        NonZeroU32::new(96).expect("the maximum tick count is non-zero"),
        move |snapshot| observer.lock().unwrap().push(snapshot.clone()),
    );

    let mut terminal_snapshot = None;
    for _ in 0..64 {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(snapshot) => {
                assert!(!report.diagnostics().has_errors(), "{report:#?}");
                terminal_snapshot = Some(snapshot.clone());
                break;
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                std::thread::park_timeout(Duration::from_millis(1));
            }
            HeadlessRunOutcome::Failed => panic!("typed reference-game run failed: {report:#?}"),
        }
    }
    let terminal_snapshot =
        terminal_snapshot.expect("typed reference-game cleanup should be bounded");
    let observations = observations.lock().unwrap();

    assert_eq!(observations.first().map(|snapshot| snapshot.tick), Some(1));
    assert_eq!(observations.last(), Some(&terminal_snapshot));
    assert!(terminal_snapshot.is_terminal());
}

#[test]
fn missing_declared_registry_plugin_fails_pure_planning_and_is_retryable() {
    let mut app = App::new();
    let Err(error) = app.add_plugin(ReferenceGamePlugin) else {
        panic!("reference-game plugin unexpectedly accepted a missing declared dependency");
    };

    assert!(matches!(
        error,
        AddPluginsError::Plan(PluginPlanError::MissingPlugin {
            plugin,
            required: _,
        }) if plugin == REFERENCE_GAME_PLUGIN_ID
    ));
    assert_eq!(
        app.plugin_lifecycle_state(),
        PluginLifecycleState::Configuring
    );

    app.add_plugins((HeadlessRuntimePlugins, ReferenceGamePlugin))
        .unwrap();
}
