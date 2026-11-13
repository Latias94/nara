use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use nara_app::{App, CoreStage, RuntimeControl, RuntimeControlRequestResult, RuntimeInstance};
use nara_core::ByteLimit;
use nara_ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use nara_input::InputSet;
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBindings, ActionMapBuilder,
    ActionMapLimits, ActionMaps, ActionOutcome, ActionOutcomes, ActionPhase, ActionValue,
    AxisDeadZone, AxisDirection, AxisValue, ButtonDriverInput, InputBinding, InputEventOrigin,
    InputLimits, InputSeatId, InputTimeline, InputWindowEvent, InputWindowId, KeyCode, MouseButton,
    SemanticActionCause, SemanticActionOutcome, SemanticActions, apply_gamepad_axis_input,
    apply_keyboard_driver_input, apply_keyboard_driver_input_at, apply_mouse_driver_input,
    apply_window_input_driver_event, connect_gamepad_input, connect_input_endpoint,
    disconnect_gamepad_input,
};

#[derive(Debug, Default, Resource)]
struct CapturedActionOutcomes(Vec<ActionOutcome>);

fn capture_action_outcomes(
    outcomes: Res<ActionOutcomes>,
    mut captured: ResMut<CapturedActionOutcomes>,
) {
    captured.0.extend_from_slice(outcomes.as_slice());
}

#[derive(Debug, Default, Resource)]
struct CapturedSemanticActions(Vec<SemanticActionOutcome>);

fn capture_semantic_actions(
    actions: Res<SemanticActions>,
    mut captured: ResMut<CapturedSemanticActions>,
) {
    captured.0.extend_from_slice(actions.as_slice());
}

#[derive(Debug)]
enum MappingChange {
    Disable(InputSeatId),
    Enable(InputSeatId),
    Rebind(KeyCode),
    SaveAndRebind(KeyCode),
    RestoreBindings,
    Remove(InputSeatId),
    RestoreMap(InputSeatId),
}

#[derive(Debug, Default, Clone, Resource)]
struct MappingChanges(
    Arc<Mutex<Vec<MappingChange>>>,
    Arc<Mutex<Option<ActionMapBindings>>>,
    Arc<Mutex<Option<ActionMap>>>,
);

fn movement_bindings(key: KeyCode) -> ActionMapBindings {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    builder
        .bind(ActionBinding::key(ActionId::new("move").unwrap(), key))
        .unwrap();
    builder.compile().unwrap().into_bindings()
}

fn apply_mapping_changes(changes: Res<MappingChanges>, mut maps: ResMut<ActionMaps>) {
    for change in changes.0.lock().unwrap().drain(..) {
        match change {
            MappingChange::Disable(seat) => {
                maps.get_mut(seat)
                    .unwrap()
                    .disable_context(&ActionContext::gameplay())
                    .unwrap();
            }
            MappingChange::Enable(seat) => {
                maps.get_mut(seat)
                    .unwrap()
                    .enable_context(&ActionContext::gameplay())
                    .unwrap();
            }
            MappingChange::Rebind(key) => {
                maps.get_mut(InputSeatId::PRIMARY)
                    .unwrap()
                    .replace_bindings(movement_bindings(key));
            }
            MappingChange::SaveAndRebind(key) => {
                let map = maps.get_mut(InputSeatId::PRIMARY).unwrap();
                let saved = map.bindings_handle();
                map.replace_bindings(movement_bindings(key));
                *changes.1.lock().unwrap() = Some(saved);
            }
            MappingChange::RestoreBindings => {
                let bindings = changes.1.lock().unwrap().take().unwrap();
                maps.get_mut(InputSeatId::PRIMARY)
                    .unwrap()
                    .replace_bindings(bindings);
            }
            MappingChange::Remove(seat) => {
                *changes.2.lock().unwrap() = Some(maps.remove(seat).unwrap());
            }
            MappingChange::RestoreMap(seat) => {
                let map = changes.2.lock().unwrap().take().unwrap();
                assert!(maps.insert(seat, map).unwrap().is_none());
            }
        }
    }
}

fn semantic_runtime() -> (RuntimeInstance, MappingChanges) {
    semantic_runtime_with_seats(&[])
}

fn semantic_runtime_with_seats(seats: &[InputSeatId]) -> (RuntimeInstance, MappingChanges) {
    let mut app = App::new();
    let changes = MappingChanges::default();
    app.insert_resource(
        InputTimeline::with_limits(InputLimits::new(32, 3, 3, 8).unwrap()).unwrap(),
    )
    .unwrap();
    app.add_plugin(nara_input::InputPlugin).unwrap();
    app.insert_resource(changes.clone()).unwrap();
    app.insert_resource(CapturedSemanticActions::default())
        .unwrap();
    app.insert_resource(CapturedActionOutcomes::default())
        .unwrap();
    app.add_systems(
        CoreStage::First,
        apply_mapping_changes.before(InputSet::StageActions),
    )
    .unwrap();
    app.add_systems(
        CoreStage::Update,
        (capture_semantic_actions, capture_action_outcomes),
    )
    .unwrap();
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    let menu = ActionContext::new("menu").unwrap();
    builder.declare_context(menu.clone(), true).unwrap();
    let action = ActionId::new("move").unwrap();
    for binding in [
        ActionBinding::key(action.clone(), KeyCode::Character('w')),
        ActionBinding::key(action.clone(), KeyCode::ArrowUp),
        ActionBinding::mouse(action, MouseButton::Left),
        ActionBinding::key(ActionId::new("menu").unwrap(), KeyCode::Space).with_context(menu),
        ActionBinding::key_axis(
            ActionId::new("steer").unwrap(),
            KeyCode::ArrowLeft,
            AxisDirection::Negative,
        ),
        ActionBinding::key_axis(
            ActionId::new("steer").unwrap(),
            KeyCode::ArrowRight,
            AxisDirection::Positive,
        ),
        ActionBinding::key_axis(
            ActionId::new("steer").unwrap(),
            KeyCode::Character('d'),
            AxisDirection::Positive,
        ),
        ActionBinding::gamepad_axis(
            ActionId::new("steer").unwrap(),
            0,
            AxisDeadZone::new(0.25).unwrap(),
            false,
        ),
    ] {
        builder.bind(binding).unwrap();
    }
    let bindings = builder.compile().unwrap().into_bindings();
    let mut maps = ActionMaps::new(
        ActionMap::new(
            bindings.clone(),
            ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        )
        .unwrap(),
    );
    for &seat in seats {
        let map = ActionMap::new(
            bindings.clone(),
            ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        )
        .unwrap();
        assert!(map.bindings_handle().shares_owner_with(&bindings));
        assert!(maps.insert(seat, map).unwrap().is_none());
    }
    app.insert_resource(maps).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let connected = runtime
        .with_driver_scope(|scope| {
            connect_input_endpoint(scope, InputWindowId::new(1), InputSeatId::PRIMARY)
        })
        .unwrap();
    assert!(connected.post_operation_fault.is_none());
    connected.output.unwrap();
    (runtime, changes)
}

fn key_edge(runtime: &mut RuntimeInstance, edge: ButtonDriverInput<KeyCode>) {
    key_edge_at(runtime, InputWindowId::new(1), InputSeatId::PRIMARY, edge);
}

fn key_edge_at(
    runtime: &mut RuntimeInstance,
    window: InputWindowId,
    seat: InputSeatId,
    edge: ButtonDriverInput<KeyCode>,
) {
    runtime
        .with_driver_scope(|scope| apply_keyboard_driver_input_at(scope, window, seat, edge))
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
}

#[test]
fn independent_seat_actions_preserve_same_seat_contributions() {
    let other_window = InputWindowId::new(2);
    let other_seat = InputSeatId::new(1);
    let (mut runtime, _) = semantic_runtime_with_seats(&[other_seat]);
    runtime
        .with_driver_scope(|scope| connect_input_endpoint(scope, other_window, other_seat))
        .unwrap()
        .output
        .unwrap();
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    key_edge(&mut runtime, ButtonDriverInput::Press(KeyCode::ArrowUp));
    runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input_at(
                scope,
                other_window,
                other_seat,
                ButtonDriverInput::Press(KeyCode::Character('w')),
            )
        })
        .unwrap()
        .output
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let after_press = runtime
        .world()
        .resource::<CapturedSemanticActions>()
        .0
        .clone();

    key_edge(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    let after_first_release = runtime
        .world()
        .resource::<CapturedSemanticActions>()
        .0
        .clone();
    key_edge(&mut runtime, ButtonDriverInput::Release(KeyCode::ArrowUp));
    runtime.drive(Duration::ZERO).unwrap();
    let after_primary_release = runtime
        .world()
        .resource::<CapturedSemanticActions>()
        .0
        .clone();
    runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input_at(
                scope,
                other_window,
                other_seat,
                ButtonDriverInput::Release(KeyCode::Character('w')),
            )
        })
        .unwrap()
        .output
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let completed = runtime
        .world()
        .resource::<CapturedSemanticActions>()
        .0
        .clone();
    runtime.finish().unwrap();

    assert_eq!(
        after_press.len(),
        2,
        "each seat must start its own move action"
    );
    assert_eq!(
        after_first_release, after_press,
        "the other same-seat binding remains held"
    );
    assert_eq!(
        after_primary_release.len(),
        3,
        "the first seat must stop independently"
    );
    assert_eq!(completed.len(), 4);
    let trajectory = completed
        .iter()
        .map(|outcome| {
            let provenance = outcome.provenance.unwrap();
            assert_eq!(outcome.seat, provenance.route().seat());
            (provenance.route().seat(), outcome.phase)
        })
        .collect::<Vec<_>>();
    assert_eq!(
        trajectory,
        [
            (InputSeatId::PRIMARY, ActionPhase::Started),
            (other_seat, ActionPhase::Started),
            (InputSeatId::PRIMARY, ActionPhase::Released),
            (other_seat, ActionPhase::Released),
        ]
    );
}

fn synthetic_edge(runtime: &mut RuntimeInstance, input: InputBinding, pressed: bool) {
    let outcome = runtime
        .with_driver_scope(|scope| match input {
            InputBinding::Key(key) => apply_keyboard_driver_input(
                scope,
                if pressed {
                    ButtonDriverInput::Press(key)
                } else {
                    ButtonDriverInput::Release(key)
                },
            )
            .map(|_| ()),
            InputBinding::Mouse(button) => apply_mouse_driver_input(
                scope,
                if pressed {
                    ButtonDriverInput::Press(button)
                } else {
                    ButtonDriverInput::Release(button)
                },
            )
            .map(|_| ()),
            InputBinding::GamepadButton(_) | InputBinding::GamepadAxis(_) => {
                unreachable!("this fixture admits keyboard and mouse input")
            }
        })
        .unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output.unwrap();
}

fn movement_is_held(runtime: &RuntimeInstance) -> bool {
    movement_is_held_at(runtime, InputSeatId::PRIMARY)
}

fn movement_is_held_at(runtime: &RuntimeInstance, seat: InputSeatId) -> bool {
    runtime.world().resource::<SemanticActions>().pressed(
        seat,
        &ActionId::new("move").unwrap(),
        &ActionContext::gameplay(),
    )
}

#[test]
fn semantic_observed_map_removal_cancels_only_its_seat_and_cannot_restore_old_holds() {
    let other_window = InputWindowId::new(2);
    let other_seat = InputSeatId::new(1);
    let (mut runtime, changes) = semantic_runtime_with_seats(&[other_seat]);
    runtime
        .with_driver_scope(|scope| connect_input_endpoint(scope, other_window, other_seat))
        .unwrap()
        .output
        .unwrap();
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    key_edge_at(
        &mut runtime,
        other_window,
        other_seat,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    let original_bindings = runtime
        .world()
        .resource::<ActionMaps>()
        .get(InputSeatId::PRIMARY)
        .unwrap()
        .bindings_handle();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    key_edge(&mut runtime, ButtonDriverInput::Press(KeyCode::ArrowUp));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        runtime
            .world()
            .resource::<InputTimeline>()
            .cursors()
            .oldest_deferred_action()
            .is_some()
    );

    changes
        .0
        .lock()
        .unwrap()
        .push(MappingChange::Remove(InputSeatId::PRIMARY));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        runtime
            .world()
            .resource::<ActionMaps>()
            .get(InputSeatId::PRIMARY)
            .is_none()
    );
    assert!(
        changes
            .2
            .lock()
            .unwrap()
            .as_ref()
            .unwrap()
            .bindings_handle()
            .shares_owner_with(&original_bindings)
    );
    changes
        .0
        .lock()
        .unwrap()
        .push(MappingChange::RestoreMap(InputSeatId::PRIMARY));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        runtime
            .world()
            .resource::<ActionMaps>()
            .get(InputSeatId::PRIMARY)
            .unwrap()
            .bindings_handle()
            .shares_owner_with(&original_bindings)
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!movement_is_held(&runtime));
    assert!(movement_is_held_at(&runtime, other_seat));
    let captured = &runtime.world().resource::<CapturedSemanticActions>().0;
    assert_eq!(captured.len(), 3);
    assert_eq!(captured[2].seat, InputSeatId::PRIMARY);
    assert_eq!(captured[2].phase, ActionPhase::Released);
    assert_eq!(captured[2].value, ActionValue::released());
    assert_eq!(captured[2].cause, SemanticActionCause::MappingChanged);
    assert_eq!(captured[2].provenance, None);
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );

    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!movement_is_held(&runtime));
    key_edge(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('w')),
    );
    key_edge(&mut runtime, ButtonDriverInput::Release(KeyCode::ArrowUp));
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        3
    );
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    key_edge_at(
        &mut runtime,
        other_window,
        other_seat,
        ButtonDriverInput::Release(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert!(!movement_is_held_at(&runtime, other_seat));
    let captured = &runtime.world().resource::<CapturedSemanticActions>().0;
    assert_eq!(captured.len(), 5);
    assert_eq!(captured[3].seat, InputSeatId::PRIMARY);
    assert_eq!(captured[3].phase, ActionPhase::Started);
    assert_eq!(captured[4].seat, other_seat);
    assert_eq!(captured[4].phase, ActionPhase::Released);
    runtime.finish().unwrap();
}

#[test]
fn semantic_context_cancellation_preserves_another_seats_staged_press() {
    let other_window = InputWindowId::new(2);
    let other_seat = InputSeatId::new(1);
    let (mut runtime, changes) = semantic_runtime_with_seats(&[other_seat]);
    runtime
        .with_driver_scope(|scope| connect_input_endpoint(scope, other_window, other_seat))
        .unwrap()
        .output
        .unwrap();
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    key_edge_at(
        &mut runtime,
        other_window,
        other_seat,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    changes
        .0
        .lock()
        .unwrap()
        .push(MappingChange::Disable(InputSeatId::PRIMARY));
    runtime.drive(Duration::ZERO).unwrap();
    let maps = runtime.world().resource::<ActionMaps>();
    let primary = maps.get(InputSeatId::PRIMARY).unwrap();
    let secondary = maps.get(other_seat).unwrap();
    assert!(!primary.is_context_enabled(&ActionContext::gameplay()));
    assert!(secondary.is_context_enabled(&ActionContext::gameplay()));
    assert!(
        primary
            .bindings_handle()
            .shares_owner_with(&secondary.bindings_handle())
    );
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!movement_is_held(&runtime));
    assert!(movement_is_held_at(&runtime, other_seat));
    let captured = &runtime.world().resource::<CapturedSemanticActions>().0;
    assert_eq!(captured.len(), 3);
    assert_eq!(captured[1].seat, InputSeatId::PRIMARY);
    assert_eq!(captured[1].phase, ActionPhase::Released);
    assert_eq!(captured[1].value, ActionValue::released());
    assert_eq!(captured[1].cause, SemanticActionCause::ContextChanged);
    assert_eq!(captured[1].provenance, None);
    assert_eq!(captured[2].seat, other_seat);
    assert_eq!(captured[2].phase, ActionPhase::Started);
    assert_eq!(captured[2].value, ActionValue::pressed());

    changes
        .0
        .lock()
        .unwrap()
        .push(MappingChange::Enable(InputSeatId::PRIMARY));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!movement_is_held(&runtime));
    assert!(movement_is_held_at(&runtime, other_seat));
    key_edge(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        3
    );
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert!(movement_is_held_at(&runtime, other_seat));
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        4
    );
    runtime.finish().unwrap();
}

#[test]
fn semantic_axis_aggregation_retains_neutral_contributors_and_clamps_only_the_final_sum() {
    let (mut runtime, _) = semantic_runtime();
    let action = ActionId::new("steer").unwrap();
    let context = ActionContext::gameplay();
    assert_eq!(
        runtime.world().resource::<SemanticActions>().value(
            InputSeatId::PRIMARY,
            &action,
            &context
        ),
        None
    );
    let negative = ActionValue::Axis(AxisValue::new(-1.0).unwrap());
    let positive = ActionValue::Axis(AxisValue::new(1.0).unwrap());
    let neutral = ActionValue::Axis(AxisValue::ZERO);
    let mut expected = Vec::new();
    for (key, pressed, value, phase, retained) in [
        (
            KeyCode::ArrowLeft,
            true,
            negative,
            Some(ActionPhase::Started),
            true,
        ),
        (
            KeyCode::ArrowRight,
            true,
            neutral,
            Some(ActionPhase::Released),
            true,
        ),
        (
            KeyCode::Character('d'),
            true,
            positive,
            Some(ActionPhase::Started),
            true,
        ),
        (
            KeyCode::ArrowRight,
            false,
            neutral,
            Some(ActionPhase::Released),
            true,
        ),
        (
            KeyCode::ArrowLeft,
            false,
            positive,
            Some(ActionPhase::Started),
            true,
        ),
        (KeyCode::ArrowRight, true, positive, None, true),
        (KeyCode::ArrowLeft, true, positive, None, true),
        (
            KeyCode::Character('d'),
            false,
            neutral,
            Some(ActionPhase::Released),
            true,
        ),
        (
            KeyCode::ArrowRight,
            false,
            negative,
            Some(ActionPhase::Started),
            true,
        ),
        (
            KeyCode::ArrowLeft,
            false,
            neutral,
            Some(ActionPhase::Released),
            false,
        ),
    ] {
        key_edge(
            &mut runtime,
            if pressed {
                ButtonDriverInput::Press(key)
            } else {
                ButtonDriverInput::Release(key)
            },
        );
        runtime.drive(Duration::ZERO).unwrap();
        let semantic = runtime.world().resource::<SemanticActions>();
        assert_eq!(
            semantic.value(InputSeatId::PRIMARY, &action, &context),
            retained.then_some(value)
        );
        assert_eq!(
            semantic.pressed(InputSeatId::PRIMARY, &action, &context),
            value.is_active()
        );
        if let Some(phase) = phase {
            expected.push((phase, value));
        }
        let captured = &runtime.world().resource::<CapturedSemanticActions>().0;
        assert_eq!(
            captured
                .iter()
                .map(|outcome| (outcome.phase, outcome.value))
                .collect::<Vec<_>>(),
            expected
        );
        assert!(captured.iter().all(|outcome| {
            outcome.seat == InputSeatId::PRIMARY
                && outcome.cause == SemanticActionCause::PhysicalInput
                && outcome.provenance.unwrap().route().seat() == outcome.seat
        }));
    }
    runtime.finish().unwrap();
}

#[test]
fn semantic_gamepad_axis_changes_are_seat_local_and_disconnect_keeps_other_contributions() {
    let other_window = InputWindowId::new(2);
    let other_seat = InputSeatId::new(1);
    let (mut runtime, _) = semantic_runtime_with_seats(&[other_seat]);
    let connected = runtime
        .with_driver_scope(|scope| {
            connect_input_endpoint(scope, other_window, other_seat)?;
            let primary =
                connect_gamepad_input(scope, InputWindowId::new(1), InputSeatId::PRIMARY, 7)?;
            let secondary = connect_gamepad_input(scope, other_window, other_seat, 7)?;
            Ok::<_, nara_input::ButtonInputDriverError>((primary, secondary))
        })
        .unwrap();
    assert!(connected.post_operation_fault.is_none());
    let (primary, secondary) = connected.output.unwrap();
    let action = ActionId::new("steer").unwrap();
    let context = ActionContext::gameplay();
    let neutral = ActionValue::Axis(AxisValue::ZERO);
    for (raw, expected, phase) in [
        (0.25, 0.0, None),
        (0.625, 0.5, Some(ActionPhase::Started)),
        (1.0, 1.0, Some(ActionPhase::Changed)),
        (-0.625, -0.5, Some(ActionPhase::Changed)),
        (-0.25, 0.0, Some(ActionPhase::Released)),
    ] {
        let before = runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len();
        let sample = runtime
            .with_driver_scope(|scope| {
                apply_gamepad_axis_input(scope, &primary, 0, AxisValue::new(raw).unwrap())
            })
            .unwrap();
        assert!(sample.post_operation_fault.is_none());
        sample.output.unwrap();
        runtime.drive(Duration::ZERO).unwrap();
        let expected = ActionValue::Axis(AxisValue::new(expected).unwrap());
        let semantic = runtime.world().resource::<SemanticActions>();
        assert_eq!(
            semantic.value(InputSeatId::PRIMARY, &action, &context),
            Some(expected)
        );
        assert_eq!(
            semantic.pressed(InputSeatId::PRIMARY, &action, &context),
            expected.is_active()
        );
        assert_eq!(semantic.value(other_seat, &action, &context), None);
        let captured = &runtime.world().resource::<CapturedSemanticActions>().0;
        assert_eq!(captured.len(), before + usize::from(phase.is_some()));
        if let Some(phase) = phase {
            let outcome = captured.last().unwrap();
            assert_eq!(outcome.seat, InputSeatId::PRIMARY);
            assert_eq!(outcome.phase, phase);
            assert_eq!(outcome.value, expected);
            assert_eq!(outcome.cause, SemanticActionCause::PhysicalInput);
            assert_eq!(
                outcome.provenance.unwrap().origin(),
                InputEventOrigin::Platform
            );
        }
    }
    let samples = runtime
        .with_driver_scope(|scope| {
            apply_gamepad_axis_input(scope, &primary, 0, AxisValue::new(0.625).unwrap())?;
            apply_gamepad_axis_input(scope, &secondary, 0, AxisValue::new(-0.625).unwrap())?;
            Ok::<_, nara_input::ButtonInputDriverError>(())
        })
        .unwrap();
    assert!(samples.post_operation_fault.is_none());
    samples.output.unwrap();
    key_edge(&mut runtime, ButtonDriverInput::Press(KeyCode::ArrowRight));
    runtime.drive(Duration::ZERO).unwrap();
    let captured = &runtime.world().resource::<CapturedSemanticActions>().0;
    assert_eq!(captured.len(), 7);
    assert_eq!(captured[6].phase, ActionPhase::Changed);
    assert_eq!(
        captured[6].value,
        ActionValue::Axis(AxisValue::new(1.0).unwrap())
    );
    let before_disconnect = captured.len();
    let disconnected = runtime
        .with_driver_scope(|scope| disconnect_gamepad_input(scope, &primary))
        .unwrap();
    assert!(disconnected.post_operation_fault.is_none());
    disconnected.output.unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let semantic = runtime.world().resource::<SemanticActions>();
    assert_eq!(
        semantic.value(InputSeatId::PRIMARY, &action, &context),
        Some(ActionValue::Axis(AxisValue::new(1.0).unwrap()))
    );
    assert_eq!(
        semantic.value(other_seat, &action, &context),
        Some(ActionValue::Axis(AxisValue::new(-0.5).unwrap()))
    );
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        before_disconnect
    );
    let before = runtime.world().resource::<InputTimeline>().cursors();
    let late = runtime
        .with_driver_scope(|scope| {
            apply_gamepad_axis_input(scope, &primary, 0, AxisValue::new(1.0).unwrap())
        })
        .unwrap();
    assert!(late.post_operation_fault.is_none());
    assert!(late.output.is_err());
    assert_eq!(
        runtime.world().resource::<InputTimeline>().cursors(),
        before
    );

    key_edge(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::ArrowRight),
    );
    let disconnected = runtime
        .with_driver_scope(|scope| disconnect_gamepad_input(scope, &secondary))
        .unwrap();
    assert!(disconnected.post_operation_fault.is_none());
    disconnected.output.unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let semantic = runtime.world().resource::<SemanticActions>();
    assert_eq!(
        semantic.value(InputSeatId::PRIMARY, &action, &context),
        None
    );
    assert_eq!(semantic.value(other_seat, &action, &context), None);
    let captured = &runtime.world().resource::<CapturedSemanticActions>().0;
    assert_eq!(captured.len(), before_disconnect + 2);
    for (outcome, seat) in captured[before_disconnect..]
        .iter()
        .zip([InputSeatId::PRIMARY, other_seat])
    {
        assert_eq!(outcome.seat, seat);
        assert_eq!(outcome.phase, ActionPhase::Released);
        assert_eq!(outcome.value, neutral);
        assert_eq!(outcome.provenance.unwrap().route().seat(), seat);
    }
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    runtime.finish().unwrap();
}

#[test]
fn semantic_synthetic_paused_presses_keep_their_staged_lifetime() {
    let key = InputBinding::Key(KeyCode::Character('w'));
    for rebind in [false, true] {
        let (mut runtime, changes) = semantic_runtime();
        assert!(matches!(
            runtime.request_control(RuntimeControl::Pause),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        synthetic_edge(&mut runtime, key, true);
        synthetic_edge(&mut runtime, InputBinding::Key(KeyCode::Space), true);
        runtime.drive(Duration::ZERO).unwrap();
        if rebind {
            changes
                .0
                .lock()
                .unwrap()
                .push(MappingChange::Rebind(KeyCode::Character('w')));
        } else {
            changes.0.lock().unwrap().extend([
                MappingChange::Disable(InputSeatId::PRIMARY),
                MappingChange::Enable(InputSeatId::PRIMARY),
            ]);
        }
        runtime.drive(Duration::ZERO).unwrap();
        assert!(matches!(
            runtime.request_control(RuntimeControl::Resume),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        assert!(
            !movement_is_held(&runtime),
            "a synthetic press cannot borrow the resume configuration's lifetime"
        );
        let semantic = &runtime.world().resource::<CapturedSemanticActions>().0;
        assert!(
            semantic
                .iter()
                .all(|outcome| outcome.action.as_str() != "move")
        );
        if !rebind {
            assert_eq!(semantic.len(), 1);
            assert_eq!(
                semantic[0].action.as_str(),
                "menu",
                "a different context keeps its staged press"
            );
        }
        assert!(
            runtime
                .world()
                .resource::<CapturedActionOutcomes>()
                .0
                .iter()
                .any(|outcome| outcome.action.as_str() == "move"
                    && outcome.phase == ActionPhase::Started),
            "raw binding observations remain available"
        );
        synthetic_edge(&mut runtime, key, false);
        runtime.drive(Duration::ZERO).unwrap();
        assert!(!movement_is_held(&runtime));
        synthetic_edge(&mut runtime, key, true);
        runtime.drive(Duration::ZERO).unwrap();
        assert!(movement_is_held(&runtime));
        runtime.finish().unwrap();
    }
}

#[test]
fn semantic_synthetic_paused_release_and_fresh_press_keep_only_the_new_lifetime() {
    for input in [
        InputBinding::Key(KeyCode::Character('w')),
        InputBinding::Mouse(MouseButton::Left),
    ] {
        let (mut runtime, changes) = semantic_runtime();
        assert!(matches!(
            runtime.request_control(RuntimeControl::Pause),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        synthetic_edge(&mut runtime, input, true);
        runtime.drive(Duration::ZERO).unwrap();
        changes.0.lock().unwrap().extend([
            MappingChange::Disable(InputSeatId::PRIMARY),
            MappingChange::Enable(InputSeatId::PRIMARY),
        ]);
        runtime.drive(Duration::ZERO).unwrap();
        synthetic_edge(&mut runtime, input, false);
        synthetic_edge(&mut runtime, input, true);
        runtime.drive(Duration::ZERO).unwrap();
        assert!(matches!(
            runtime.request_control(RuntimeControl::Resume),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();
        assert!(movement_is_held(&runtime));
        let semantic = &runtime.world().resource::<CapturedSemanticActions>().0;
        assert_eq!(
            semantic.len(),
            1,
            "an expired press/release pair cannot start and end a new semantic lifetime"
        );
        assert_eq!(semantic[0].phase, ActionPhase::Started);
        assert_eq!(
            semantic[0].provenance.unwrap().origin(),
            InputEventOrigin::Synthetic
        );
        assert!(semantic[0].provenance.unwrap().sequence() > 0);
        assert_eq!(
            runtime.world().resource::<CapturedActionOutcomes>().0.len(),
            3
        );
        runtime.finish().unwrap();
    }
}

#[test]
fn semantic_observed_context_exit_survives_reenabling_the_context() {
    observed_configuration_restore(false);
}

#[test]
fn semantic_observed_rebinding_survives_restoring_the_original_bindings() {
    observed_configuration_restore(true);
}

fn observed_configuration_restore(rebind: bool) {
    let (mut runtime, changes) = semantic_runtime();
    let input = InputBinding::Key(KeyCode::Character('w'));
    synthetic_edge(&mut runtime, input, true);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    changes.0.lock().unwrap().push(if rebind {
        MappingChange::SaveAndRebind(KeyCode::Character('w'))
    } else {
        MappingChange::Disable(InputSeatId::PRIMARY)
    });
    runtime.drive(Duration::ZERO).unwrap();
    changes.0.lock().unwrap().push(if rebind {
        MappingChange::RestoreBindings
    } else {
        MappingChange::Enable(InputSeatId::PRIMARY)
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        !movement_is_held(&runtime),
        "restoring old configuration cannot erase an observed lifetime exit"
    );
    let semantic = &runtime.world().resource::<CapturedSemanticActions>().0;
    assert_eq!(semantic.len(), 2);
    assert_eq!(semantic[1].phase, ActionPhase::Released);
    assert_eq!(
        semantic[1].cause,
        if rebind {
            SemanticActionCause::MappingChanged
        } else {
            SemanticActionCause::ContextChanged
        }
    );
    synthetic_edge(&mut runtime, input, false);
    runtime.drive(Duration::ZERO).unwrap();
    synthetic_edge(&mut runtime, input, true);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        3
    );
    runtime.finish().unwrap();
}

#[test]
fn semantic_multiple_bindings_release_only_the_final_contribution() {
    let (mut runtime, _) = semantic_runtime();
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    key_edge(&mut runtime, ButtonDriverInput::Press(KeyCode::ArrowUp));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        1
    );
    assert_eq!(
        runtime.world().resource::<CapturedActionOutcomes>().0.len(),
        2
    );

    key_edge(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        1
    );

    key_edge(&mut runtime, ButtonDriverInput::Release(KeyCode::ArrowUp));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!movement_is_held(&runtime));
    let captured = &runtime.world().resource::<CapturedSemanticActions>().0;
    assert_eq!(
        captured
            .iter()
            .map(|outcome| outcome.phase)
            .collect::<Vec<_>>(),
        [ActionPhase::Started, ActionPhase::Released]
    );
    assert_eq!(captured[1].cause, SemanticActionCause::PhysicalInput);
    let release = captured[1].provenance.unwrap();
    let press = captured[0].provenance.unwrap();
    assert!(release.sequence() > press.sequence());
    assert_eq!(release.origin(), InputEventOrigin::Platform);
    assert!(
        runtime
            .world()
            .resource::<SemanticActions>()
            .as_slice()
            .is_empty(),
        "cleanup clears outcomes while retaining held state"
    );
    runtime.finish().unwrap();
}

#[test]
fn semantic_context_exit_is_terminal_once_and_restoration_requires_fresh_press() {
    let (mut runtime, changes) = semantic_runtime();
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    key_edge(&mut runtime, ButtonDriverInput::Press(KeyCode::Space));
    runtime.drive(Duration::ZERO).unwrap();

    changes
        .0
        .lock()
        .unwrap()
        .push(MappingChange::Disable(InputSeatId::PRIMARY));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!movement_is_held(&runtime));
    assert!(runtime.world().resource::<SemanticActions>().pressed(
        InputSeatId::PRIMARY,
        &ActionId::new("menu").unwrap(),
        &ActionContext::new("menu").unwrap()
    ));
    let ended = &runtime.world().resource::<CapturedSemanticActions>().0[2];
    assert_eq!(ended.phase, ActionPhase::Released);
    assert_eq!(ended.cause, SemanticActionCause::ContextChanged);
    assert_eq!(ended.provenance, None);
    assert_eq!(ended.seat, InputSeatId::PRIMARY);
    assert_eq!(ended.value, ActionValue::released());

    changes
        .0
        .lock()
        .unwrap()
        .push(MappingChange::Disable(InputSeatId::PRIMARY));
    runtime.drive(Duration::ZERO).unwrap();
    changes
        .0
        .lock()
        .unwrap()
        .push(MappingChange::Enable(InputSeatId::PRIMARY));
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!movement_is_held(&runtime));
    key_edge(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        3,
        "repeated disable, restore, and a cancelled physical release cannot duplicate the terminal edge"
    );

    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    changes.0.lock().unwrap().extend([
        MappingChange::Disable(InputSeatId::PRIMARY),
        MappingChange::Enable(InputSeatId::PRIMARY),
    ]);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        !movement_is_held(&runtime),
        "a context lifetime cannot survive disable/enable between resolution passes"
    );
    let captured = &runtime.world().resource::<CapturedSemanticActions>().0;
    assert_eq!(captured.len(), 5);
    assert_eq!(captured[4].cause, SemanticActionCause::ContextChanged);
    runtime.finish().unwrap();
}

#[test]
fn semantic_paused_mapping_changes_do_not_resurrect_staged_presses() {
    let (mut runtime, changes) = semantic_runtime();
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();

    changes
        .0
        .lock()
        .unwrap()
        .push(MappingChange::Rebind(KeyCode::ArrowUp));
    key_edge(&mut runtime, ButtonDriverInput::Press(KeyCode::ArrowUp));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        runtime
            .world()
            .resource::<InputTimeline>()
            .cursors()
            .oldest_deferred_action()
            .is_some()
    );
    changes
        .0
        .lock()
        .unwrap()
        .push(MappingChange::Rebind(KeyCode::Character('w')));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();

    assert!(!movement_is_held(&runtime));
    let captured = &runtime.world().resource::<CapturedSemanticActions>().0;
    assert_eq!(
        captured.len(),
        2,
        "the old mapping ends once; an obsolete deferred press never restarts it"
    );
    assert_eq!(captured[1].cause, SemanticActionCause::MappingChanged);
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    key_edge(&mut runtime, ButtonDriverInput::Release(KeyCode::ArrowUp));
    key_edge(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        2
    );
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        3
    );
    runtime.finish().unwrap();
}

#[test]
fn semantic_paused_context_restoration_rejects_its_old_press_and_preserves_other_contexts() {
    let (mut runtime, changes) = semantic_runtime();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    key_edge(&mut runtime, ButtonDriverInput::Press(KeyCode::Space));
    runtime.drive(Duration::ZERO).unwrap();
    changes.0.lock().unwrap().extend([
        MappingChange::Disable(InputSeatId::PRIMARY),
        MappingChange::Enable(InputSeatId::PRIMARY),
    ]);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();

    assert!(!movement_is_held(&runtime));
    let captured = &runtime.world().resource::<CapturedSemanticActions>().0;
    assert_eq!(captured.len(), 1);
    assert_eq!(captured[0].action.as_str(), "menu");
    assert!(runtime.world().resource::<SemanticActions>().pressed(
        InputSeatId::PRIMARY,
        &ActionId::new("menu").unwrap(),
        &ActionContext::new("menu").unwrap()
    ));
    key_edge(
        &mut runtime,
        ButtonDriverInput::Release(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    key_edge(
        &mut runtime,
        ButtonDriverInput::Press(KeyCode::Character('w')),
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        2
    );
    runtime.finish().unwrap();
}

#[test]
fn semantic_same_binding_held_by_two_endpoints_survives_one_endpoint_focus_loss() {
    let (mut runtime, _) = semantic_runtime();
    for (window, seat) in [
        (InputWindowId::new(1), InputSeatId::PRIMARY),
        (InputWindowId::new(2), InputSeatId::PRIMARY),
    ] {
        runtime
            .with_driver_scope(|scope| {
                connect_input_endpoint(scope, window, seat)?;
                apply_window_input_driver_event(
                    scope,
                    window,
                    seat,
                    InputWindowEvent::Focused(true),
                )?;
                apply_keyboard_driver_input_at(
                    scope,
                    window,
                    seat,
                    ButtonDriverInput::Press(KeyCode::Character('w')),
                )?;
                Ok::<(), nara_input::ButtonInputDriverError>(())
            })
            .map(|outcome| {
                assert!(outcome.post_operation_fault.is_none());
                outcome.output
            })
            .unwrap()
            .unwrap();
    }
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        1
    );
    runtime
        .with_driver_scope(|scope| {
            apply_window_input_driver_event(
                scope,
                InputWindowId::new(1),
                InputSeatId::PRIMARY,
                InputWindowEvent::Focused(false),
            )
        })
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(movement_is_held(&runtime));
    runtime
        .with_driver_scope(|scope| {
            apply_window_input_driver_event(
                scope,
                InputWindowId::new(2),
                InputSeatId::PRIMARY,
                InputWindowEvent::Focused(false),
            )
        })
        .map(|outcome| {
            assert!(outcome.post_operation_fault.is_none());
            outcome.output
        })
        .unwrap()
        .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!movement_is_held(&runtime));
    assert_eq!(
        runtime
            .world()
            .resource::<CapturedSemanticActions>()
            .0
            .len(),
        2
    );
    runtime.finish().unwrap();
}
