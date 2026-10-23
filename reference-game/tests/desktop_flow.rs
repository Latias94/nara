#![cfg(feature = "desktop")]

#[path = "support/project_content_fixture.rs"]
mod project_content_fixture;

use std::{
    num::NonZeroU64,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use nara::{
    advanced_prelude::materialize_startup_scene,
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginDefinition,
        PluginDefinitionId, PluginError, PluginId, RuntimeAdmissionReservation, RuntimeClosePolicy,
        RuntimeFaultKind, RuntimeInstance, RuntimeObligationLedger, RuntimeState,
        RuntimeTimeSettings,
    },
    core::ItemLimit,
    ecs::schedule::IntoScheduleConfigs,
    gameplay::{GameplayCommandQueue, GameplayCommandQueueSettings},
    input::{ActionContext, ActionMap, ButtonDriverInput, KeyCode, apply_keyboard_driver_input},
    prelude::{FixedTime, ResMut, Transform2d},
    project_host::{
        ProjectContentLoader,
        advanced::{RuntimeProbe, resolve_product_recipe_with_probe},
    },
};
use nara_reference_game::{
    MovementDirection, PlayerRole, WaveOutcome, WaveRetryPhase, WaveRetryStatus, WaveRunGeneration,
    WaveSnapshot, desktop_wave_recipe, movement_command,
};
use project_content_fixture::{desktop_candidate_plan_and_root, stop_runtime};

const DESKTOP_FRAME_DELTA: Duration = Duration::from_millis(10);
const DESKTOP_FIXED_TIMESTEP: Duration = Duration::from_millis(100);

#[derive(Debug)]
struct InputContextProbe(Arc<AtomicBool>);

impl Plugin for InputContextProbe {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("reference-game.test.input-context"),
            PluginCategory::Tooling,
        )
        .requires_plugins(&[nara::input::INPUT_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let enabled = Arc::clone(&self.0);
        app.add_systems(
            CoreStage::First,
            (move |mut actions: ResMut<ActionMap>| {
                if enabled.load(Ordering::Relaxed) {
                    actions.enable_context(&ActionContext::gameplay());
                } else {
                    actions.disable_context(ActionContext::gameplay());
                }
            })
            .before(nara::input::__private::InputSet::StageActions),
        )?;
        Ok(())
    }
}

#[test]
fn semantic_movement_aggregates_alternate_keys_and_context_restore_requires_fresh_press() {
    let enabled = Arc::new(AtomicBool::new(true));
    let mut runtime = desktop_runtime_with_context(Some(Arc::clone(&enabled)));
    // The desktop clock clamps each real frame to 250 ms. Accumulate half of the 100 ms
    // timestep first so one unclamped 250 ms frame can exercise three catch-up ticks.
    let half_step = DESKTOP_FIXED_TIMESTEP / 2;
    let priming = runtime.drive(half_step).unwrap().frame().unwrap();
    assert_eq!(priming.status.fixed_steps, 0);
    assert_eq!(priming.status.fixed_tick, 0);
    assert_eq!(priming.status.fixed_remainder, half_step);
    assert_eq!(player_position(&runtime), nara::prelude::Vec2::ZERO);
    keyboard(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    keyboard(&mut runtime, ButtonDriverInput::Press(KeyCode::ArrowUp));
    let captured = runtime.drive(Duration::ZERO).unwrap().frame().unwrap();
    assert_eq!(captured.status.fixed_steps, 0);
    assert_eq!(captured.status.fixed_tick, 0);
    assert_eq!(captured.status.fixed_remainder, half_step);
    assert_eq!(player_position(&runtime), nara::prelude::Vec2::ZERO);
    assert_eq!(
        runtime
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        1
    );
    let catch_up_delta = DESKTOP_FIXED_TIMESTEP * 2 + half_step;
    assert!(
        catch_up_delta
            <= runtime
                .world()
                .resource::<RuntimeTimeSettings>()
                .max_delta()
    );
    let catch_up = runtime.drive(catch_up_delta).unwrap().frame().unwrap();
    assert!(!catch_up.status.real_delta_clamped);
    assert_eq!(catch_up.status.virtual_delta, catch_up_delta);
    assert_eq!(catch_up.status.fixed_steps, 3);
    assert_eq!(catch_up.status.fixed_tick, 3);
    assert_eq!(catch_up.status.fixed_remainder, Duration::ZERO);
    assert_eq!(runtime.world().resource::<WaveSnapshot>().tick, 3);
    let first = player_position(&runtime);
    assert_eq!(
        first.y, 3.0,
        "one aggregate start must survive a zero-tick frame and three catch-up ticks"
    );

    keyboard(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    drive_fixed(&mut runtime);
    let alternate_held = player_position(&runtime);
    assert!(
        alternate_held.y > first.y,
        "Up must keep moving after W releases"
    );
    assert_eq!(
        runtime
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        1
    );

    enabled.store(false, Ordering::Relaxed);
    runtime.drive(Duration::ZERO).unwrap();
    drive_fixed(&mut runtime);
    assert_eq!(
        player_position(&runtime),
        alternate_held,
        "context exit must lower one terminal command to the real movement consumer"
    );
    enabled.store(true, Ordering::Relaxed);
    runtime.drive(Duration::ZERO).unwrap();
    drive_fixed(&mut runtime);
    assert_eq!(
        player_position(&runtime),
        alternate_held,
        "restoring the context must not resume an already-held Up key"
    );
    keyboard(&mut runtime, ButtonDriverInput::Release(KeyCode::ArrowUp));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        2,
        "the cancelled physical release must not duplicate the context's terminal command"
    );

    keyboard(&mut runtime, ButtonDriverInput::Press(KeyCode::ArrowUp));
    keyboard(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('s')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    drive_fixed(&mut runtime);
    assert_eq!(
        player_position(&runtime),
        alternate_held,
        "opposite semantic directions cancel in the game-owned movement consumer"
    );
    keyboard(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('s')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    drive_fixed(&mut runtime);
    let resumed = player_position(&runtime);
    assert!(
        resumed.y > alternate_held.y,
        "releasing the opposite direction preserves the freshly-pressed Up action"
    );
    keyboard(&mut runtime, ButtonDriverInput::Release(KeyCode::ArrowUp));
    runtime.drive(Duration::ZERO).unwrap();
    drive_fixed(&mut runtime);
    assert_eq!(player_position(&runtime), resumed);
    assert_eq!(
        runtime
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        6
    );
    stop_runtime(runtime);
}

#[test]
fn desktop_wave_starts_without_input_and_applies_wasd_movement() {
    let mut runtime = desktop_runtime();

    drive_fixed(&mut runtime);
    let running = runtime.world().resource::<WaveSnapshot>();
    assert_eq!(
        running.outcome,
        WaveOutcome::Running,
        "the desktop wave must become visibly active without a hidden input gate"
    );
    assert_eq!(running.tick, 1, "the first desktop fixed step must run");
    assert!(
        running.enemies.iter().any(|enemy| enemy.active),
        "at least one enemy must be visibly active on the first tick"
    );

    let before = player_position(&runtime);
    assert_eq!(
        before,
        nara::prelude::Vec2::ZERO,
        "the player must start centered until the user chooses a direction"
    );
    keyboard(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    drive_fixed(&mut runtime);
    let after = player_position(&runtime);
    assert_eq!(runtime.world().resource::<WaveSnapshot>().tick, 2);

    assert!(
        after.y > before.y,
        "W must move the player upward: before={before:?}, after={after:?}"
    );
    stop_runtime(runtime);
}

#[test]
fn desktop_profile_keeps_a_short_physical_key_press_visible_and_playable() {
    let mut runtime = desktop_runtime();
    let actual_timestep = runtime.world().resource::<FixedTime>().timestep();

    let before = player_position(&runtime);
    keyboard(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    drive_desktop_frames(&mut runtime, 25);

    keyboard(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    drive_desktop_frames(&mut runtime, 45);

    let after = player_position(&runtime);
    let snapshot = runtime.world().resource::<WaveSnapshot>();
    let outcome = snapshot.outcome;
    stop_runtime(runtime);

    assert_eq!(
        actual_timestep, DESKTOP_FIXED_TIMESTEP,
        "the desktop profile must use a human-observable simulation cadence"
    );
    assert_eq!(
        outcome,
        WaveOutcome::Running,
        "a 250 ms key press must not finish the game before the player can react"
    );
    assert!(
        after.y > before.y && after.y <= 4.0,
        "the player must move visibly without leaving the camera: before={before:?}, after={after:?}"
    );
}

#[test]
fn overlapping_wasd_keys_compose_and_releasing_one_preserves_the_other() {
    let mut runtime = desktop_runtime();
    let origin = player_position(&runtime);

    keyboard(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('s')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    keyboard(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('a')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    drive_fixed(&mut runtime);

    let diagonal = player_position(&runtime);

    keyboard(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('s')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    drive_fixed(&mut runtime);

    let left_only = player_position(&runtime);

    keyboard(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('a')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    drive_fixed(&mut runtime);

    let stopped = player_position(&runtime);
    stop_runtime(runtime);

    assert!(
        diagonal.x < origin.x && diagonal.y < origin.y,
        "held S+A must compose into down-left movement: origin={origin:?}, diagonal={diagonal:?}"
    );
    let diagonal_distance = (diagonal - origin).length();
    assert!(
        (diagonal_distance - 1.0).abs() < 1.0e-6,
        "diagonal movement must preserve cardinal speed: distance={diagonal_distance}"
    );
    assert!(
        left_only.x < diagonal.x && left_only.y == diagonal.y,
        "releasing S while A remains held must preserve left movement: diagonal={diagonal:?}, left_only={left_only:?}"
    );
    assert_eq!(
        stopped, left_only,
        "releasing the final movement key must stop the player"
    );
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
fn enter_without_presented_ui_cannot_admit_retry() {
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
        None
    );
    stop_runtime(running);

    let mut terminal = desktop_runtime();
    start_desktop_wave(&mut terminal);
    drive_to_terminal(&mut terminal);
    let runtime_generation = terminal.generation();
    let run_generation = terminal.world().resource::<WaveRunGeneration>().get();
    keyboard(&mut terminal, ButtonDriverInput::Press(KeyCode::Enter));
    terminal.drive(Duration::ZERO).unwrap();
    keyboard(&mut terminal, ButtonDriverInput::Release(KeyCode::Enter));
    terminal.drive(Duration::ZERO).unwrap();
    keyboard(&mut terminal, ButtonDriverInput::Press(KeyCode::Enter));
    terminal.drive(Duration::ZERO).unwrap();
    drive_fixed(&mut terminal);
    assert_eq!(
        terminal.world().resource::<WaveRetryStatus>().phase(),
        WaveRetryPhase::Idle
    );
    assert_eq!(
        terminal
            .world()
            .resource::<WaveRetryStatus>()
            .last_rejection(),
        None
    );
    assert_eq!(terminal.generation(), runtime_generation);
    assert_eq!(
        terminal.world().resource::<WaveRunGeneration>().get(),
        run_generation
    );
    assert!(terminal.world().resource::<WaveSnapshot>().is_terminal());
    stop_runtime(terminal);
}

#[test]
fn rejected_physical_local_command_faults_the_managed_runtime() {
    let (project, plan, root) = desktop_candidate_plan_and_root();
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&project, &plan).unwrap();
    let runtime_time = plan.settings().runtime.runtime_time_settings();
    let fixed_time = plan.settings().runtime.fixed_time();
    let source = snapshot.scene_source();
    let sealed = plan.plugin_plan().instantiate().unwrap();
    let mut candidate = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            sealed,
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
    candidate
        .with_admission_scope(move |scope| {
            scope.apply_command(move |world: &mut nara::prelude::World| {
                world.insert_resource(runtime_time);
                world.insert_resource(fixed_time);
                let report = materialize_startup_scene(world, source)
                    .expect("the retained desktop scene should materialize");
                assert!(!report.has_errors(), "{report:#?}");
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
        }).map(|outcome| { assert!(outcome.post_operation_fault.is_none()); outcome.output })
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
    desktop_runtime_with_context(None)
}

fn desktop_runtime_with_context(context: Option<Arc<AtomicBool>>) -> RuntimeInstance {
    let (candidate, plan, root) = desktop_candidate_plan_and_root();
    let plan = match context {
        Some(enabled) => resolve_product_recipe_with_probe(
            &candidate,
            desktop_wave_recipe().unwrap(),
            RuntimeProbe::new(PluginDefinition::infallible::<InputContextProbe, _>(
                PluginDefinitionId::new("reference-game.test.input-context", 1),
                [],
                move || InputContextProbe(Arc::clone(&enabled)),
            )),
        )
        .unwrap(),
        None => plan,
    };
    let loader = ProjectContentLoader::new(root).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let runtime_time = plan.settings().runtime.runtime_time_settings();
    let fixed_time = plan.settings().runtime.fixed_time();
    let source = snapshot.scene_source();
    let sealed = plan.plugin_plan().instantiate().unwrap();
    let mut candidate = RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            sealed,
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap();
    candidate
        .with_admission_scope(move |scope| {
            scope.apply_command(move |world: &mut nara::prelude::World| {
                world.insert_resource(runtime_time);
                world.insert_resource(fixed_time);
                let report = materialize_startup_scene(world, source)
                    .expect("the retained desktop scene should materialize");
                assert!(!report.has_errors(), "{report:#?}");
            });
        }).map(|outcome| { assert!(outcome.post_operation_fault.is_none()); outcome.output })
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

fn drive_desktop_frames(runtime: &mut RuntimeInstance, frames: usize) {
    for _ in 0..frames {
        runtime.drive(DESKTOP_FRAME_DELTA).unwrap();
    }
}

fn keyboard(runtime: &mut RuntimeInstance, input: ButtonDriverInput<KeyCode>) {
    runtime
        .with_driver_scope(|scope| apply_keyboard_driver_input(scope, input)).map(|outcome| { assert!(outcome.post_operation_fault.is_none()); outcome.output })
        .unwrap()
        .unwrap();
}

fn player_position(runtime: &RuntimeInstance) -> nara::prelude::Vec2 {
    runtime
        .world()
        .iter_entities()
        .find_map(|entity| {
            if !entity.contains::<PlayerRole>() {
                return None;
            }
            entity
                .get::<Transform2d>()
                .map(|transform| transform.translation)
        })
        .expect("desktop fixture must contain one player")
}
