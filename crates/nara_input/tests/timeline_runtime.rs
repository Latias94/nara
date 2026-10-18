use std::time::Duration;

use nara_app::{
    App, CoreStage, FixedTime, FixedUpdateSet, RuntimeAdmissionReservation, RuntimeClosePolicy,
    RuntimeControl, RuntimeControlRequestResult, RuntimeInstance, RuntimeObligationLedger,
    RuntimeState,
};
use nara_ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use nara_input::__private::InputSet;
use nara_input::{
    ActionId, ActionMap, ActionOutcome, ActionOutcomes, ActionPhase, ButtonDriverInput,
    ButtonInput, InputEventKind, InputFrameObservations, InputGeometryGeneration, InputPointerId,
    InputPresentationEpoch, InputPresentedRoute, InputRouteGeneration, InputSeatId,
    InputSlotGeneration, InputSurfaceId, InputTimeline, InputWindowEvent, InputWindowId, KeyCode,
    MouseButton, apply_keyboard_driver_input, apply_keyboard_driver_input_at,
    apply_mouse_driver_input_at, apply_window_input_driver_event,
};

#[derive(Debug, Default, Resource)]
struct ObservedSequences(Vec<u64>);

#[derive(Debug, Default, Resource)]
struct ObservedActions(usize);

#[derive(Debug, Default, Resource)]
struct CapturedActionOutcomes(Vec<ActionOutcome>);

fn capture_action_outcomes(
    outcomes: Res<ActionOutcomes>,
    mut captured: ResMut<CapturedActionOutcomes>,
) {
    captured.0.extend_from_slice(outcomes.as_slice());
}

#[derive(Debug, Default, Resource)]
struct FixedKeyObservations(Vec<(bool, bool, bool)>);

fn capture_fixed_key(keys: Res<ButtonInput<KeyCode>>, mut captured: ResMut<FixedKeyObservations>) {
    let key = KeyCode::Character('d');
    captured.0.push((
        keys.pressed(key),
        keys.just_pressed(key),
        keys.just_released(key),
    ));
}

#[derive(Debug, Default, Resource)]
struct FinalizedKeyObservations(Vec<(bool, bool, bool)>);

fn capture_finalized_key(
    keys: Res<ButtonInput<KeyCode>>,
    mut captured: ResMut<FinalizedKeyObservations>,
) {
    let key = KeyCode::Character('d');
    captured.0.push((
        keys.pressed(key),
        keys.just_pressed(key),
        keys.just_released(key),
    ));
}

fn consume_enter_for_ui(mut timeline: ResMut<InputTimeline>) {
    let consumed = timeline
        .pending_ui_entries()
        .filter_map(|entry| {
            matches!(
                entry.kind(),
                InputEventKind::Key {
                    key: KeyCode::Enter,
                    ..
                }
            )
            .then_some(entry.sequence())
        })
        .collect::<Vec<_>>();
    for sequence in consumed {
        timeline
            .__finalize_ui_entry(sequence, nara_input::InputUiDisposition::Consumed)
            .unwrap();
    }
}

fn capture_observations(
    observations: Res<InputFrameObservations>,
    mut captured: ResMut<ObservedSequences>,
) {
    captured.0 = observations
        .as_slice()
        .iter()
        .map(|entry| entry.sequence())
        .collect();
}

fn capture_actions(outcomes: Res<ActionOutcomes>, mut captured: ResMut<ObservedActions>) {
    captured.0 += outcomes.as_slice().len();
}

fn start_runtime(app: App) -> RuntimeInstance {
    RuntimeAdmissionReservation::try_acquire()
        .unwrap()
        .admit(
            app.seal().unwrap(),
            RuntimeObligationLedger::new(),
            RuntimeClosePolicy::default(),
        )
        .unwrap()
        .complete_startup()
        .unwrap()
        .promote()
}

fn stop_runtime(runtime: &mut RuntimeInstance) {
    assert!(matches!(
        runtime.request_control(RuntimeControl::Stop),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    if runtime.state() == RuntimeState::Stopping {
        runtime.drive(Duration::ZERO).unwrap();
    }
}

#[test]
fn endpoint_projection_preserves_other_seat_keys_after_release_and_focus_loss() {
    let first_window = InputWindowId::new(1);
    let second_window = InputWindowId::new(2);
    let first_seat = InputSeatId::PRIMARY;
    let second_seat = InputSeatId::new(1);
    let key = KeyCode::Character('d');
    let mut app = App::new();
    app.add_plugin(nara_input::InputPlugin)
        .unwrap()
        .insert_resource(CapturedActionOutcomes::default())
        .unwrap()
        .add_systems(CoreStage::Update, capture_action_outcomes)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<ActionMap>()
        .bind_key(ActionId::new("move").unwrap(), key);
    for (window, seat, surface) in [
        (first_window, first_seat, InputSurfaceId::new(9)),
        (second_window, second_seat, InputSurfaceId::new(10)),
    ] {
        app.world_mut()
            .unwrap()
            .resource_mut::<InputTimeline>()
            .publish_presented_route(InputPresentedRoute::new(
                window,
                surface,
                seat,
                InputPresentationEpoch::new(1).unwrap(),
                InputSlotGeneration::new(1).unwrap(),
                InputRouteGeneration::new(1).unwrap(),
                InputGeometryGeneration::new(1).unwrap(),
            ))
            .unwrap();
    }
    let mut runtime = start_runtime(app);
    runtime
        .with_driver_scope(|scope| {
            for (window, seat) in [(first_window, first_seat), (second_window, second_seat)] {
                apply_window_input_driver_event(
                    scope,
                    window,
                    seat,
                    InputWindowEvent::Focused(true),
                )?;
                apply_keyboard_driver_input_at(scope, window, seat, ButtonDriverInput::Press(key))?;
            }
            Ok::<(), nara_input::ButtonInputDriverError>(())
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input_at(
                scope,
                first_window,
                first_seat,
                ButtonDriverInput::Release(key),
            )
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let held_after_first_release = runtime
        .world()
        .resource::<ButtonInput<KeyCode>>()
        .pressed(key);
    runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input_at(
                scope,
                first_window,
                first_seat,
                ButtonDriverInput::Press(key),
            )
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    runtime
        .with_driver_scope(|scope| {
            apply_window_input_driver_event(
                scope,
                first_window,
                first_seat,
                InputWindowEvent::Focused(false),
            )
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let held_after_first_focus_loss = runtime
        .world()
        .resource::<ButtonInput<KeyCode>>()
        .pressed(key);
    runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input_at(
                scope,
                second_window,
                second_seat,
                ButtonDriverInput::Release(key),
            )
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let held_after_final_release = runtime
        .world()
        .resource::<ButtonInput<KeyCode>>()
        .pressed(key);
    let outcomes = runtime
        .world()
        .resource::<CapturedActionOutcomes>()
        .0
        .clone();
    stop_runtime(&mut runtime);

    assert!(held_after_first_release, "another seat still holds the key");
    assert!(
        held_after_first_focus_loss,
        "focus loss releases only the affected seat"
    );
    assert!(!held_after_final_release);
    assert_eq!(
        outcomes
            .iter()
            .map(|outcome| outcome.phase)
            .collect::<Vec<_>>(),
        [
            ActionPhase::Started,
            ActionPhase::Started,
            ActionPhase::Released,
            ActionPhase::Started,
            ActionPhase::Released,
            ActionPhase::Released,
        ]
    );
    let expected_origins = [
        (2, first_window, first_seat, 2, InputSurfaceId::new(9)),
        (4, second_window, second_seat, 4, InputSurfaceId::new(10)),
        (5, first_window, first_seat, 2, InputSurfaceId::new(9)),
        (6, first_window, first_seat, 6, InputSurfaceId::new(9)),
        (7, first_window, first_seat, 6, InputSurfaceId::new(9)),
        (9, second_window, second_seat, 4, InputSurfaceId::new(10)),
    ];
    for (outcome, (sequence, window, seat, press_sequence, surface)) in
        outcomes.iter().zip(expected_origins)
    {
        assert_eq!(outcome.provenance.sequence(), Some(sequence));
        assert_eq!(outcome.provenance.route().window(), window);
        assert_eq!(outcome.provenance.route().seat(), seat);
        assert_eq!(outcome.provenance.press_sequence(), Some(press_sequence));
        assert_eq!(
            outcome.provenance.press_route().unwrap().surface(),
            Some(surface)
        );
    }
}

#[test]
fn endpoint_projection_preserves_other_pointer_buttons_after_release() {
    let window = InputWindowId::new(1);
    let seat = InputSeatId::PRIMARY;
    let first_pointer = InputPointerId::PRIMARY;
    let second_pointer = InputPointerId::new(1);
    let button = MouseButton::Left;
    let mut app = App::new();
    app.add_plugin(nara_input::InputPlugin)
        .unwrap()
        .insert_resource(CapturedActionOutcomes::default())
        .unwrap()
        .add_systems(CoreStage::Update, capture_action_outcomes)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<ActionMap>()
        .bind_mouse(ActionId::new("select").unwrap(), button);
    let mut runtime = start_runtime(app);
    runtime
        .with_driver_scope(|scope| {
            for pointer in [first_pointer, second_pointer] {
                apply_mouse_driver_input_at(
                    scope,
                    window,
                    seat,
                    pointer,
                    ButtonDriverInput::Press(button),
                )?;
            }
            Ok::<(), nara_input::ButtonInputDriverError>(())
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    runtime
        .with_driver_scope(|scope| {
            apply_mouse_driver_input_at(
                scope,
                window,
                seat,
                first_pointer,
                ButtonDriverInput::Release(button),
            )
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let held_after_first_release = runtime
        .world()
        .resource::<ButtonInput<MouseButton>>()
        .pressed(button);
    runtime
        .with_driver_scope(|scope| {
            apply_window_input_driver_event(scope, window, seat, InputWindowEvent::Focused(false))
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let held_after_focus_loss = runtime
        .world()
        .resource::<ButtonInput<MouseButton>>()
        .pressed(button);
    let outcomes = runtime
        .world()
        .resource::<CapturedActionOutcomes>()
        .0
        .clone();
    stop_runtime(&mut runtime);

    assert!(
        held_after_first_release,
        "another pointer still holds the button"
    );
    assert!(!held_after_focus_loss);
    assert_eq!(
        outcomes
            .iter()
            .map(|outcome| outcome.phase)
            .collect::<Vec<_>>(),
        [
            ActionPhase::Started,
            ActionPhase::Started,
            ActionPhase::Released,
            ActionPhase::Released,
        ]
    );
    for (outcome, (sequence, pointer, press_sequence)) in outcomes.iter().zip([
        (1, first_pointer, 1),
        (2, second_pointer, 2),
        (3, first_pointer, 1),
        (4, second_pointer, 2),
    ]) {
        assert_eq!(outcome.provenance.sequence(), Some(sequence));
        assert_eq!(outcome.provenance.route().pointer(), Some(pointer));
        assert_eq!(outcome.provenance.press_sequence(), Some(press_sequence));
        assert_eq!(
            outcome.provenance.press_route().unwrap().pointer(),
            Some(pointer)
        );
    }
}

#[test]
fn endpoint_projection_keeps_direct_synthetic_and_default_driver_members_separate() {
    let key = KeyCode::Character('d');
    let mut app = App::new();
    app.add_plugin(nara_input::InputPlugin).unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(key)
        .unwrap();
    let mut runtime = start_runtime(app);
    runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input(scope, ButtonDriverInput::Press(key))
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input(scope, ButtonDriverInput::Release(key))
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let synthetic_survives_driver_release = runtime
        .world()
        .resource::<ButtonInput<KeyCode>>()
        .pressed(key);
    runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input(scope, ButtonDriverInput::Press(key))
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    runtime
        .with_driver_scope(|scope| {
            scope.__apply_port::<ButtonInput<KeyCode>>(ButtonDriverInput::Release(key))
        })
        .unwrap()
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let driver_survives_synthetic_release = runtime
        .world()
        .resource::<ButtonInput<KeyCode>>()
        .pressed(key);
    runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input(scope, ButtonDriverInput::Release(key))
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let held_after_final_release = runtime
        .world()
        .resource::<ButtonInput<KeyCode>>()
        .pressed(key);
    stop_runtime(&mut runtime);

    assert!(synthetic_survives_driver_release);
    assert!(driver_survives_synthetic_release);
    assert!(!held_after_final_release);
}

#[test]
fn exact_step_projects_admitted_key_and_focus_release_before_fixed_simulation() {
    let window = InputWindowId::new(1);
    let mut app = App::new();
    app.add_plugin(nara_input::InputPlugin)
        .unwrap()
        .insert_resource(FixedKeyObservations::default())
        .unwrap()
        .insert_resource(FinalizedKeyObservations::default())
        .unwrap()
        .add_systems(
            CoreStage::FixedUpdate,
            capture_fixed_key.in_set(FixedUpdateSet::Simulate),
        )
        .unwrap()
        .add_systems(
            CoreStage::FixedUpdate,
            capture_finalized_key.after(FixedUpdateSet::Finalize),
        )
        .unwrap()
        .insert_resource(ObservedSequences::default())
        .unwrap()
        .insert_resource(ObservedActions::default())
        .unwrap()
        .add_systems(CoreStage::Extract, capture_observations)
        .unwrap()
        .add_systems(CoreStage::Update, capture_actions)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<ActionMap>()
        .bind_key(ActionId::new("move").unwrap(), KeyCode::Character('d'));
    app.world_mut()
        .unwrap()
        .resource_mut::<InputTimeline>()
        .publish_presented_route(InputPresentedRoute::new(
            window,
            InputSurfaceId::new(9),
            InputSeatId::PRIMARY,
            InputPresentationEpoch::new(17).unwrap(),
            InputSlotGeneration::new(3).unwrap(),
            InputRouteGeneration::new(4).unwrap(),
            InputGeometryGeneration::new(5).unwrap(),
        ))
        .unwrap();
    let mut runtime = start_runtime(app);
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    runtime
        .with_driver_scope(|scope| {
            apply_window_input_driver_event(
                scope,
                window,
                InputSeatId::PRIMARY,
                InputWindowEvent::Focused(true),
            )?;
            apply_keyboard_driver_input_at(
                scope,
                window,
                InputSeatId::PRIMARY,
                ButtonDriverInput::Press(KeyCode::Character('d')),
            )?;
            Ok::<(), nara_input::ButtonInputDriverError>(())
        })
        .unwrap()
        .unwrap();
    for _ in 0..2 {
        assert!(matches!(
            runtime.request_control(RuntimeControl::StepFixedTick),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
    }
    let held_after_step = runtime
        .world()
        .resource::<ButtonInput<KeyCode>>()
        .pressed(KeyCode::Character('d'));
    runtime
        .with_driver_scope(|scope| {
            apply_window_input_driver_event(
                scope,
                window,
                InputSeatId::PRIMARY,
                InputWindowEvent::Focused(false),
            )
        })
        .unwrap()
        .unwrap();
    for _ in 0..2 {
        assert!(matches!(
            runtime.request_control(RuntimeControl::StepFixedTick),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
    }
    let observed = runtime.world().resource::<FixedKeyObservations>().0.clone();
    let finalized = runtime
        .world()
        .resource::<FinalizedKeyObservations>()
        .0
        .clone();
    let transitions_after_step = runtime
        .world()
        .resource::<ButtonInput<KeyCode>>()
        .transitions()
        .len();
    let retained_for_later_consumers = runtime.world().resource::<InputTimeline>().retained_len();
    let state = runtime.state();
    // A later ordinary frame must still publish UI observations and stage semantic actions.
    runtime.drive(Duration::ZERO).unwrap();
    let later_observations = runtime.world().resource::<ObservedSequences>().0.clone();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    let later_actions = runtime.world().resource::<ObservedActions>().0;
    runtime.drive(Duration::ZERO).unwrap();
    let actions_after_another_frame = runtime.world().resource::<ObservedActions>().0;
    stop_runtime(&mut runtime);
    assert_eq!(state, RuntimeState::Paused);
    assert!(held_after_step, "frame cleanup must retain held buttons");
    let expected = [
        (true, true, false),
        (true, false, false),
        (false, false, true),
        (false, false, false),
    ];
    assert_eq!(
        observed, expected,
        "each edge belongs to exactly one exact-step App frame"
    );
    assert_eq!(
        finalized, expected,
        "edges remain observable after the complete Finalize phase"
    );
    assert_eq!(
        transitions_after_step, 0,
        "completed exact steps retire frame observations"
    );
    assert!(retained_for_later_consumers > 0);
    assert_eq!(later_observations, [1, 2, 3, 4]);
    assert_eq!(
        later_actions, 2,
        "the delayed press/release actions remain deliverable"
    );
    assert_eq!(
        actions_after_another_frame, 2,
        "semantic actions are delivered once"
    );
}

#[test]
fn ordinary_catch_up_keeps_key_edges_through_all_ticks_and_last() {
    let mut app = App::new();
    app.add_plugin(nara_input::InputPlugin)
        .unwrap()
        .insert_resource(FixedKeyObservations::default())
        .unwrap()
        .insert_resource(FinalizedKeyObservations::default())
        .unwrap()
        .add_systems(
            CoreStage::FixedUpdate,
            capture_fixed_key.in_set(FixedUpdateSet::Simulate),
        )
        .unwrap()
        .add_systems(CoreStage::Last, capture_finalized_key)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<ButtonInput<KeyCode>>()
        .press(KeyCode::Character('d'))
        .unwrap();
    let mut runtime = start_runtime(app);
    runtime.drive(FixedTime::DEFAULT_TIMESTEP * 3).unwrap();
    let observed = runtime.world().resource::<FixedKeyObservations>().0.clone();
    let last = runtime
        .world()
        .resource::<FinalizedKeyObservations>()
        .0
        .clone();
    let transitions = runtime
        .world()
        .resource::<ButtonInput<KeyCode>>()
        .transitions()
        .len();
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    let next_frame = runtime
        .world()
        .resource::<FixedKeyObservations>()
        .0
        .last()
        .copied();
    stop_runtime(&mut runtime);
    assert_eq!(observed, [(true, true, false); 3]);
    assert_eq!(last, [(true, true, false)]);
    assert_eq!(transitions, 0);
    assert_eq!(next_frame, Some((true, false, false)));
}

#[test]
fn paused_ui_consumption_is_observed_once_and_skipped_by_gameplay_on_resume() {
    let window = InputWindowId::new(1);
    let mut app = App::new();
    app.add_plugin(nara_input::InputPlugin)
        .unwrap()
        .insert_resource(ObservedSequences::default())
        .unwrap()
        .insert_resource(ObservedActions::default())
        .unwrap()
        .add_systems(
            CoreStage::First,
            consume_enter_for_ui.in_set(InputSet::RouteUi),
        )
        .unwrap()
        .add_systems(CoreStage::Extract, capture_observations)
        .unwrap()
        .add_systems(CoreStage::Update, capture_actions)
        .unwrap();
    app.world_mut()
        .unwrap()
        .resource_mut::<ActionMap>()
        .bind_key(ActionId::new("confirm").unwrap(), KeyCode::Enter);
    app.world_mut()
        .unwrap()
        .resource_mut::<InputTimeline>()
        .publish_presented_route(InputPresentedRoute::new(
            window,
            InputSurfaceId::new(9),
            InputSeatId::PRIMARY,
            InputPresentationEpoch::new(17).unwrap(),
            InputSlotGeneration::new(3).unwrap(),
            InputRouteGeneration::new(4).unwrap(),
            InputGeometryGeneration::new(5).unwrap(),
        ))
        .unwrap();

    let mut runtime = start_runtime(app);
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    runtime
        .with_driver_scope(|scope| {
            apply_window_input_driver_event(
                scope,
                window,
                InputSeatId::PRIMARY,
                InputWindowEvent::Focused(true),
            )?;
            apply_keyboard_driver_input_at(
                scope,
                window,
                InputSeatId::PRIMARY,
                ButtonDriverInput::Press(KeyCode::Enter),
            )?;
            Ok::<(), nara_input::ButtonInputDriverError>(())
        })
        .unwrap()
        .unwrap();

    runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(runtime.state(), RuntimeState::Paused);
    assert_eq!(runtime.world().resource::<ObservedSequences>().0, [1, 2]);
    assert_eq!(runtime.world().resource::<ObservedActions>().0, 0);
    let cursors = runtime.world().resource::<InputTimeline>().cursors();
    assert_eq!(cursors.observation(), 2);
    assert_eq!(cursors.ui(), 2);
    assert_eq!(cursors.action_scanned(), 2);
    assert_eq!(cursors.oldest_deferred_action(), Some(2));

    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();

    assert_eq!(runtime.world().resource::<ObservedActions>().0, 0);
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    stop_runtime(&mut runtime);
}
