#![cfg(feature = "desktop")]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{sync::Arc, time::Duration};

use nara::{
    advanced_prelude::{StartupSceneSource, materialize_startup_scene},
    app::{
        RuntimeAdmissionReservation, RuntimeClosePolicy, RuntimeInstance, RuntimeObligationLedger,
    },
    core::ByteLimit,
    gameplay::{GameplayCommandQueue, GameplayCommandSubmission},
    input::{ButtonDriverInput, KeyCode, apply_keyboard_driver_input},
    prelude::FixedTime,
    project_host::{ProjectContentLoader, ProjectSettingsCandidate, RuntimePlan},
};
use nara_reference_game::{MovementDirection, WaveSnapshot, movement_command};
use project_content_fixture::{
    desktop_candidate_plan_and_root, headless_wave_candidate_plan_and_root, stop_runtime,
};
#[test]
fn physical_desktop_and_direct_headless_commands_remain_equal_through_terminal() {
    let (headless_candidate, headless_plan, headless_root) =
        headless_wave_candidate_plan_and_root();
    let mut headless = runtime_with_scene(
        headless_candidate,
        headless_plan,
        headless_root,
        vec![movement_command(1, 1, MovementDirection::Left).unwrap()],
    );

    let (desktop_candidate, desktop_plan, desktop_root) = desktop_candidate_plan_and_root();
    let mut desktop = runtime_with_scene(desktop_candidate, desktop_plan, desktop_root, Vec::new());
    desktop
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input(scope, ButtonDriverInput::Press(KeyCode::Character('a')))
        })
        .unwrap()
        .unwrap();
    desktop.drive(Duration::ZERO).unwrap();
    assert_eq!(
        desktop
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .pending_commands,
        1,
    );

    let headless_fixed_step = headless.world().resource::<FixedTime>().timestep();
    let desktop_fixed_step = desktop.world().resource::<FixedTime>().timestep();
    let terminal = (0..96).find_map(|_| {
        headless.drive(headless_fixed_step).unwrap();
        desktop.drive(desktop_fixed_step).unwrap();
        let headless_snapshot = headless.world().resource::<WaveSnapshot>().clone();
        let desktop_snapshot = desktop.world().resource::<WaveSnapshot>().clone();

        assert_eq!(desktop_snapshot, headless_snapshot);
        headless_snapshot.is_terminal().then_some(headless_snapshot)
    });
    let terminal =
        terminal.expect("the parity wave should terminate within its bounded tick limit");
    assert!(terminal.is_terminal());

    stop_runtime(headless);
    stop_runtime(desktop);
}

fn runtime_with_scene(
    candidate: ProjectSettingsCandidate,
    plan: RuntimePlan,
    root: nara::fs::DirectoryCapability,
    commands: Vec<GameplayCommandSubmission>,
) -> RuntimeInstance {
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let scene = snapshot.expanded_startup_scene().clone();
    let runtime_time = plan.settings().runtime.runtime_time_settings();
    let fixed_time = plan.settings().runtime.fixed_time();
    let source = StartupSceneSource::direct(
        Arc::new(scene),
        ByteLimit::new(16 * 1024 * 1024).expect("the direct retained-scene limit is non-zero"),
    )
    .expect("the parity scene should fit the bounded direct retention limit");
    let sealed = plan.plugin_plan().instantiate().unwrap();
    let mut runtime = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            sealed,
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
    runtime
        .with_admission_scope(move |scope| {
            scope.apply_command(move |world: &mut nara::prelude::World| {
                world.insert_resource(runtime_time);
                world.insert_resource(fixed_time);
                let report = materialize_startup_scene(world, source)
                    .expect("the retained parity scene should materialize");
                assert!(!report.has_errors(), "{report:#?}");
                let mut queue = world.resource_mut::<GameplayCommandQueue>();
                for command in commands {
                    queue.submit(command).unwrap();
                }
            });
        })
        .unwrap();
    runtime.complete_startup().unwrap().promote()
}
