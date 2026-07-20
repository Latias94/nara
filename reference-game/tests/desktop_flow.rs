#![cfg(feature = "desktop")]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{num::NonZeroU64, time::Duration};

use nara::{
    app::{RuntimeCandidate, RuntimeFaultKind, RuntimeInstance, RuntimeState},
    core::ItemLimit,
    gameplay::{GameplayCommandQueue, GameplayCommandQueueSettings},
    input::{ButtonDriverInput, KeyCode, apply_keyboard_driver_input},
    prelude::FixedTime,
    project_host::ProjectContentLoader,
    scene::spawn_scene,
};
use nara_reference_game::{
    MovementDirection, WaveOutcome, WaveRetryPhase, WaveRetryRejection, WaveRetryStatus,
    WaveRunGeneration, WaveSnapshot, movement_command,
};
use project_content_fixture::{desktop_candidate_plan_and_root, stop_runtime};

const MINIMUM_DESKTOP_INPUT_WINDOW_TICKS: usize = 250;

#[test]
fn desktop_wave_leaves_an_input_window_and_applies_wasd_movement() {
    let mut runtime = desktop_runtime();

    for _ in 0..MINIMUM_DESKTOP_INPUT_WINDOW_TICKS {
        drive_fixed(&mut runtime);
    }
    let waiting = runtime.world().resource::<WaveSnapshot>();
    assert_eq!(
        waiting.outcome,
        WaveOutcome::Running,
        "the desktop wave must remain interactive for at least five seconds"
    );
    assert_eq!(waiting.tick, 0, "waiting must not advance the wave clock");
    assert!(waiting.enemies.iter().all(|enemy| !enemy.active));

    let before = player_position(&runtime);
    keyboard(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    drive_fixed(&mut runtime);
    let after = player_position(&runtime);
    assert_eq!(runtime.world().resource::<WaveSnapshot>().tick, 1);

    assert!(
        after.y > before.y,
        "W must move the player upward: before={before:?}, after={after:?}"
    );
    stop_runtime(runtime);
}

#[test]
fn wasd_edges_lower_to_semantic_commands_and_focus_release_stops() {
    let mut runtime = desktop_runtime();

    keyboard(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    let pressed = runtime.world().resource::<GameplayCommandQueue>().stats();
    assert_eq!(pressed.accepted, 1);
    assert_eq!(pressed.pending_commands, 1);

    keyboard(&mut runtime, ButtonDriverInput::ReleaseAll);
    runtime.drive(Duration::ZERO).unwrap();
    let released = runtime.world().resource::<GameplayCommandQueue>().stats();
    assert_eq!(released.accepted, 2);
    assert_eq!(released.pending_commands, 2);
    stop_runtime(runtime);
}

#[test]
fn enter_is_terminal_only_and_admits_at_most_one_pending_retry() {
    let mut running = desktop_runtime();
    keyboard(&mut running, ButtonDriverInput::Press(KeyCode::Enter));
    running.drive(Duration::ZERO).unwrap();
    assert_eq!(
        running.world().resource::<WaveRetryStatus>().phase(),
        WaveRetryPhase::Idle
    );
    drive_fixed(&mut running);
    assert_eq!(
        running
            .world()
            .resource::<WaveRetryStatus>()
            .last_rejection(),
        Some(WaveRetryRejection::WhileRunning)
    );
    stop_runtime(running);

    let mut terminal = desktop_runtime();
    start_desktop_wave(&mut terminal);
    drive_to_terminal(&mut terminal);
    let runtime_generation = terminal.generation();
    let run_generation = terminal.world().resource::<WaveRunGeneration>().get();
    keyboard(&mut terminal, ButtonDriverInput::Press(KeyCode::Enter));
    terminal.drive(Duration::ZERO).unwrap();
    assert_eq!(
        terminal.world().resource::<WaveRetryStatus>().phase(),
        WaveRetryPhase::Idle
    );

    keyboard(&mut terminal, ButtonDriverInput::Release(KeyCode::Enter));
    terminal.drive(Duration::ZERO).unwrap();
    keyboard(&mut terminal, ButtonDriverInput::Press(KeyCode::Enter));
    terminal.drive(Duration::ZERO).unwrap();
    drive_fixed(&mut terminal);
    assert_eq!(
        terminal.world().resource::<WaveRetryStatus>().phase(),
        WaveRetryPhase::Pending
    );
    assert_eq!(
        terminal
            .world()
            .resource::<WaveRetryStatus>()
            .pending_generation(),
        Some(run_generation)
    );
    assert_eq!(
        terminal
            .world()
            .resource::<WaveRetryStatus>()
            .last_rejection(),
        Some(WaveRetryRejection::AlreadyPending)
    );
    assert_eq!(terminal.generation(), runtime_generation);
    assert_eq!(
        terminal.world().resource::<WaveRunGeneration>().get(),
        run_generation
    );
    assert!(terminal.world().resource::<WaveSnapshot>().is_terminal());

    drive_fixed(&mut terminal);
    let snapshot = terminal.world().resource::<WaveSnapshot>();
    assert_eq!(terminal.generation(), runtime_generation);
    assert_eq!(snapshot.run_generation, run_generation + 1);
    assert_eq!(snapshot.tick, 0);
    assert_eq!(snapshot.outcome, WaveOutcome::Running);
    assert_eq!(snapshot.player.hit_points, 20);
    assert_eq!(snapshot.enemies.len(), 3);
    assert_eq!(
        terminal.world().resource::<WaveRetryStatus>().phase(),
        WaveRetryPhase::Applied
    );
    assert_eq!(
        terminal
            .world()
            .resource::<WaveRetryStatus>()
            .last_rejection(),
        Some(WaveRetryRejection::AlreadyPending)
    );
    stop_runtime(terminal);
}

#[test]
fn repeated_retry_keeps_one_runtime_generation() {
    let mut runtime = desktop_runtime();
    let runtime_generation = runtime.generation();

    for expected_generation in 2..=3 {
        start_desktop_wave(&mut runtime);
        drive_to_terminal(&mut runtime);
        keyboard(&mut runtime, ButtonDriverInput::Press(KeyCode::Enter));
        runtime.drive(Duration::ZERO).unwrap();
        drive_fixed(&mut runtime);
        assert_eq!(
            runtime.world().resource::<WaveRetryStatus>().phase(),
            WaveRetryPhase::Pending
        );
        drive_fixed(&mut runtime);
        assert_eq!(runtime.generation(), runtime_generation);
        assert_eq!(
            runtime.world().resource::<WaveRunGeneration>().get(),
            expected_generation
        );
        keyboard(&mut runtime, ButtonDriverInput::Release(KeyCode::Enter));
        runtime.drive(Duration::ZERO).unwrap();
    }

    stop_runtime(runtime);
}

#[test]
fn rejected_physical_local_command_faults_the_managed_runtime() {
    let (_candidate, plan, _root) = desktop_candidate_plan_and_root();
    let sealed = plan.plugin_plan().instantiate().unwrap();
    let mut candidate = RuntimeCandidate::admit(sealed).unwrap();
    candidate
        .with_admission_scope(|scope| {
            scope.apply_command(|world: &mut nara::prelude::World| {
                let defaults = GameplayCommandQueueSettings::default();
                let settings = GameplayCommandQueueSettings::new(
                    ItemLimit::new(1).unwrap(),
                    defaults.retained_bytes(),
                    defaults.command_bytes(),
                    defaults.payload_items(),
                    defaults.payload_bytes(),
                    NonZeroU64::new(1).unwrap(),
                )
                .unwrap();
                let mut queue = GameplayCommandQueue::new(settings);
                queue
                    .submit(movement_command(1, 1, MovementDirection::Right).unwrap())
                    .unwrap();
                world.insert_resource(queue);
            });
        })
        .unwrap();
    let mut runtime = candidate.complete_startup().unwrap().promote();

    keyboard(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('a')),
    );
    let failure = runtime.drive(Duration::ZERO).unwrap_err();

    assert_eq!(failure.fault().kind(), RuntimeFaultKind::LocalIntentLoss);
    assert_eq!(failure.fault().source(), "nara.gameplay.local-action");
    assert_eq!(runtime.state(), RuntimeState::Faulted);
    assert_eq!(runtime.fault(), Some(failure.fault()));
    stop_runtime(runtime);
}

fn desktop_runtime() -> RuntimeInstance {
    let (candidate, plan, root) = desktop_candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let scene = snapshot.expanded_startup_scene().clone();
    let sealed = plan.plugin_plan().instantiate().unwrap();
    let mut candidate = RuntimeCandidate::admit(sealed).unwrap();
    candidate
        .with_admission_scope(move |scope| {
            scope.apply_command(move |world: &mut nara::prelude::World| {
                let report = spawn_scene(world, plan.schema_validation().registry(), &scene);
                assert!(
                    !report.diagnostics.has_errors(),
                    "{:#?}",
                    report.diagnostics
                );
                assert!(report.instance.is_some());
            });
        })
        .unwrap();
    candidate.complete_startup().unwrap().promote()
}

fn drive_to_terminal(runtime: &mut RuntimeInstance) {
    for _ in 0..96 {
        if runtime.world().resource::<WaveSnapshot>().is_terminal() {
            return;
        }
        drive_fixed(runtime);
    }
    panic!("reference wave did not reach a terminal state");
}

fn start_desktop_wave(runtime: &mut RuntimeInstance) {
    keyboard(runtime, ButtonDriverInput::Release(KeyCode::Character('a')));
    runtime.drive(Duration::ZERO).unwrap();
    keyboard(runtime, ButtonDriverInput::Press(KeyCode::Character('a')));
    runtime.drive(Duration::ZERO).unwrap();
}

fn drive_fixed(runtime: &mut RuntimeInstance) {
    let timestep = runtime.world().resource::<FixedTime>().timestep();
    runtime.drive(timestep).unwrap();
}

fn keyboard(runtime: &mut RuntimeInstance, input: ButtonDriverInput<KeyCode>) {
    runtime
        .with_driver_scope(|scope| apply_keyboard_driver_input(scope, input))
        .unwrap()
        .unwrap();
}

fn player_position(runtime: &RuntimeInstance) -> nara::prelude::Vec2 {
    runtime
        .world()
        .iter_entities()
        .find_map(|entity| entity.get::<nara_reference_game::Player>())
        .map(|player| player.position)
        .expect("desktop fixture must contain one player")
}
