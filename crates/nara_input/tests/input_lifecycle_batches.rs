use std::time::Duration;

use nara_app::{
    App, CoreStage, FixedTime, FixedUpdateSet, RuntimeControl, RuntimeControlRequestResult,
    RuntimeDriverScope, RuntimeInstance,
};
use nara_core::{ByteLimit, Vec2};
use nara_ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapLimits,
    ActionMaps, ActionOutcome, ActionOutcomes, ActionPhase, ActionTransform, ActionValue,
    AxisDeadZone, AxisDirection, AxisValue, ButtonDriverInput, ButtonInput, ButtonInputDriverError,
    ButtonTransitionPhase, InputAdmission, InputBinding, InputEventKind, InputEventOrigin,
    InputGeometryGeneration, InputInvalidationReason, InputPlugin, InputPointerId,
    InputPresentationEpoch, InputPresentedRoute, InputRouteGeneration, InputSeatId, InputSet,
    InputSlotGeneration, InputSurfaceId, InputTimeline, InputUiDisposition, InputWindowEvent,
    InputWindowId, KeyCode, MouseButton, PointerDriverInput, SemanticActionCause,
    SemanticActionOutcome, SemanticActions, apply_gamepad_axis_input, apply_gamepad_button_input,
    apply_keyboard_driver_input_at, apply_mouse_driver_input_at, apply_pointer_driver_input_at,
    apply_window_input_driver_event, connect_gamepad_input, connect_input_pointer,
    disconnect_gamepad_input, retire_input_pointer,
};

const SEAT: InputSeatId = InputSeatId::PRIMARY;
const WINDOW: InputWindowId = InputWindowId::new(1);
const OTHER_WINDOW: InputWindowId = InputWindowId::new(2);
const LEFT: KeyCode = KeyCode::Character('a');
const RIGHT: KeyCode = KeyCode::Character('d');

#[derive(Debug, Default, Resource)]
struct FixedActions {
    value: Option<ActionValue>,
    binding: Vec<ActionOutcome>,
    semantic: Vec<SemanticActionOutcome>,
}

fn capture_fixed_actions(
    actions: Res<SemanticActions>,
    outcomes: Res<ActionOutcomes>,
    mut captured: ResMut<FixedActions>,
) {
    captured.value = actions.value(
        SEAT,
        &ActionId::new("steer").unwrap(),
        &ActionContext::gameplay(),
    );
    captured.binding.clear();
    captured.binding.extend_from_slice(outcomes.as_slice());
    captured.semantic.clear();
    captured.semantic.extend_from_slice(actions.as_slice());
}

fn runtime(routed: bool) -> RuntimeInstance {
    runtime_with_route(routed, routed.then_some(LEFT))
}

fn runtime_with_route(routed: bool, consumed_key: Option<KeyCode>) -> RuntimeInstance {
    let action = ActionId::new("steer").unwrap();
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    for binding in [
        ActionBinding::key_axis(action.clone(), LEFT, AxisDirection::Negative),
        ActionBinding::key_axis(action.clone(), RIGHT, AxisDirection::Positive),
        ActionBinding::gamepad_axis(action.clone(), 0, AxisDeadZone::new(0.0).unwrap(), false),
        ActionBinding {
            transform: ActionTransform::AxisButton {
                direction: AxisDirection::Positive,
            },
            ..ActionBinding::gamepad_button(action.clone(), 7)
        },
        ActionBinding {
            transform: ActionTransform::AxisButton {
                direction: AxisDirection::Negative,
            },
            ..ActionBinding::mouse(action.clone(), MouseButton::Left)
        },
        ActionBinding {
            transform: ActionTransform::AxisButton {
                direction: AxisDirection::Positive,
            },
            ..ActionBinding::mouse(action, MouseButton::Right)
        },
    ] {
        builder.bind(binding).unwrap();
    }
    let map = ActionMap::new(
        builder.compile().unwrap().into_bindings(),
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
    )
    .unwrap();
    let mut app = App::new();
    app.add_plugin(InputPlugin).unwrap();
    app.insert_resource(ActionMaps::new(map)).unwrap();
    app.init_resource::<FixedActions>().unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        capture_fixed_actions.in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    {
        let mut timeline = app.world_mut().unwrap().resource_mut::<InputTimeline>();
        timeline.connect_endpoint(WINDOW, SEAT).unwrap();
        timeline.connect_endpoint(OTHER_WINDOW, SEAT).unwrap();
        if routed {
            timeline
                .publish_presented_route(InputPresentedRoute::new(
                    WINDOW,
                    InputSurfaceId::new(1),
                    SEAT,
                    InputPresentationEpoch::new(1).unwrap(),
                    InputSlotGeneration::new(1).unwrap(),
                    InputRouteGeneration::new(1).unwrap(),
                    InputGeometryGeneration::new(1).unwrap(),
                ))
                .unwrap();
        }
    }
    if let Some(consumed) = consumed_key {
        app.add_systems(
            CoreStage::First,
            (move |mut timeline: ResMut<InputTimeline>| {
                timeline.route_pending_ui_input(|entry| match entry.kind() {
                    InputEventKind::Key { key, .. } if key == consumed => {
                        Some(InputUiDisposition::Consumed)
                    }
                    _ => None,
                });
            })
            .in_set(InputSet::RouteUi),
        )
        .unwrap();
    }
    RuntimeInstance::start(app).unwrap()
}

fn driver<R>(
    runtime: &mut RuntimeInstance,
    operation: impl FnOnce(&mut RuntimeDriverScope<'_>) -> Result<R, ButtonInputDriverError>,
) -> R {
    let outcome = runtime.with_driver_scope(operation).unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output.unwrap()
}

fn key(runtime: &mut RuntimeInstance, edge: ButtonDriverInput<KeyCode>) -> u64 {
    key_at(runtime, WINDOW, edge)
}

fn key_at(
    runtime: &mut RuntimeInstance,
    window: InputWindowId,
    edge: ButtonDriverInput<KeyCode>,
) -> u64 {
    driver(runtime, |scope| {
        apply_keyboard_driver_input_at(scope, window, SEAT, edge)
    });
    runtime
        .world()
        .resource::<InputTimeline>()
        .entries()
        .last()
        .unwrap()
        .sequence()
}

fn lose_focus(runtime: &mut RuntimeInstance) -> (InputAdmission, Vec<KeyCode>) {
    let (admission, keys, _) = driver(runtime, |scope| {
        apply_window_input_driver_event(scope, WINDOW, SEAT, InputWindowEvent::Focused(false))
    });
    (admission, keys)
}

fn tick(runtime: &mut RuntimeInstance) -> &FixedActions {
    runtime.drive(FixedTime::DEFAULT_TIMESTEP).unwrap();
    runtime.world().resource::<FixedActions>()
}

fn step(runtime: &mut RuntimeInstance) -> &FixedActions {
    assert!(matches!(
        runtime.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    runtime.world().resource::<FixedActions>()
}

fn axis(value: f32) -> ActionValue {
    ActionValue::Axis(AxisValue::new(value).unwrap())
}

#[test]
fn focus_loss_clears_opposed_keys_without_a_fixed_semantic_edge() {
    let mut runtime = runtime(false);
    for order in [[LEFT, RIGHT], [RIGHT, LEFT]] {
        let presses = order.map(|button| key(&mut runtime, ButtonDriverInput::Press(button)));
        assert_eq!(tick(&mut runtime).value, Some(axis(0.0)));

        let (loss, released) = lose_focus(&mut runtime);
        assert_eq!(released, order);
        let fixed = tick(&mut runtime);
        assert_eq!(fixed.value, None);
        assert!(fixed.semantic.is_empty());
        assert_eq!(fixed.binding.len(), 2);
        for (index, binding) in fixed.binding.iter().enumerate() {
            assert_eq!(binding.binding, InputBinding::Key(order[index]));
            assert_eq!(binding.phase, ActionPhase::Released);
            assert_eq!(binding.provenance.press_sequence(), Some(presses[index]));
            assert_eq!(binding.provenance.origin(), InputEventOrigin::Platform);
            assert_eq!(binding.provenance.route().window(), WINDOW);
            assert_eq!(
                binding.provenance.sequence(),
                loss.first_sequence().unwrap() + index as u64
            );
        }
    }
    runtime.finish().unwrap();
}

#[test]
fn separate_key_releases_keep_each_semantic_edge_within_one_fixed_tick() {
    let mut runtime = runtime(false);
    key(&mut runtime, ButtonDriverInput::Press(LEFT));
    key(&mut runtime, ButtonDriverInput::Press(RIGHT));
    assert_eq!(tick(&mut runtime).value, Some(axis(0.0)));

    let first = key(&mut runtime, ButtonDriverInput::Release(LEFT));
    let second = key(&mut runtime, ButtonDriverInput::Release(RIGHT));
    let fixed = tick(&mut runtime);
    assert_eq!(fixed.value, None);
    assert_eq!(fixed.semantic.len(), 2);
    for (outcome, expected) in fixed.semantic.iter().zip([
        (ActionPhase::Started, axis(1.0), first),
        (ActionPhase::Released, axis(0.0), second),
    ]) {
        assert_eq!(
            (
                outcome.phase,
                outcome.value,
                outcome.provenance.unwrap().sequence()
            ),
            expected
        );
    }
    runtime.finish().unwrap();
}

#[test]
fn gamepad_disconnect_preserves_other_members_and_each_admitted_batch() {
    let mut runtime = runtime(false);
    let removed = driver(&mut runtime, |scope| {
        connect_gamepad_input(scope, WINDOW, SEAT, 4)
    });
    let surviving = driver(&mut runtime, |scope| {
        connect_gamepad_input(scope, OTHER_WINDOW, SEAT, 4)
    });
    let stick = driver(&mut runtime, |scope| {
        apply_gamepad_axis_input(scope, &removed, 0, AxisValue::new(-1.0).unwrap())
    });
    let dpad = driver(&mut runtime, |scope| {
        apply_gamepad_button_input(scope, &removed, 7, ButtonTransitionPhase::Pressed)
    });
    driver(&mut runtime, |scope| {
        apply_gamepad_axis_input(scope, &surviving, 0, AxisValue::new(0.5).unwrap())
    });
    assert_eq!(tick(&mut runtime).value, Some(axis(0.5)));

    driver(&mut runtime, |scope| {
        disconnect_gamepad_input(scope, &removed)
    });
    let fixed = tick(&mut runtime);
    assert_eq!(fixed.value, Some(axis(0.5)));
    assert!(fixed.semantic.is_empty());
    assert_eq!(fixed.binding.len(), 2);
    for (binding, (input, press)) in fixed.binding.iter().zip([
        (InputBinding::GamepadAxis(0), stick),
        (InputBinding::GamepadButton(7), dpad),
    ]) {
        assert_eq!(binding.binding, input);
        assert_eq!(binding.phase, ActionPhase::Released);
        assert_eq!(binding.provenance.press_sequence(), press.first_sequence());
        assert_eq!(
            binding.provenance.gamepad_connection_generation(),
            Some(removed.connection_generation())
        );
    }

    let replacement = driver(&mut runtime, |scope| {
        connect_gamepad_input(scope, WINDOW, SEAT, 4)
    });
    driver(&mut runtime, |scope| {
        apply_gamepad_axis_input(scope, &replacement, 0, AxisValue::new(-0.5).unwrap())
    });
    assert_eq!(tick(&mut runtime).value, Some(axis(0.0)));

    let first = driver(&mut runtime, |scope| {
        disconnect_gamepad_input(scope, &replacement)
    });
    let second = driver(&mut runtime, |scope| {
        disconnect_gamepad_input(scope, &surviving)
    });
    let fixed = tick(&mut runtime);
    assert_eq!(fixed.value, None);
    assert_eq!(fixed.semantic.len(), 2);
    for (outcome, (phase, value, admission)) in fixed.semantic.iter().zip([
        (ActionPhase::Started, axis(0.5), first),
        (ActionPhase::Released, axis(0.0), second),
    ]) {
        assert_eq!((outcome.phase, outcome.value), (phase, value));
        assert_eq!(
            Some(outcome.provenance.unwrap().sequence()),
            admission.first_sequence()
        );
    }
    runtime.finish().unwrap();
}

#[test]
fn exact_step_waits_for_ui_before_applying_the_focus_loss_as_a_unit() {
    let mut runtime = runtime(true);
    let right = key(&mut runtime, ButtonDriverInput::Press(RIGHT));
    assert_eq!(tick(&mut runtime).value, Some(axis(1.0)));
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();

    let left = key(&mut runtime, ButtonDriverInput::Press(LEFT));
    let (loss, released) = lose_focus(&mut runtime);
    assert_eq!(released, [RIGHT, LEFT]);
    let fixed = step(&mut runtime);
    assert_eq!(fixed.value, Some(axis(1.0)));
    assert!(fixed.binding.is_empty());
    assert!(fixed.semantic.is_empty());
    let physical = runtime.world().resource::<ButtonInput<KeyCode>>();
    assert!(!physical.pressed(RIGHT));
    assert!(!physical.pressed(LEFT));
    assert_eq!(
        runtime
            .world()
            .resource::<InputTimeline>()
            .cursors()
            .oldest_deferred_action(),
        Some(left)
    );

    runtime.drive(Duration::ZERO).unwrap();
    let fixed = step(&mut runtime);
    assert_eq!(fixed.value, None);
    let [binding] = fixed.binding.as_slice() else {
        panic!("expected one gameplay binding release")
    };
    let [outcome] = fixed.semantic.as_slice() else {
        panic!("expected one semantic release")
    };
    assert_eq!(binding.binding, InputBinding::Key(RIGHT));
    assert_eq!(binding.phase, ActionPhase::Released);
    assert_eq!(outcome.phase, ActionPhase::Released);
    assert_eq!(outcome.value, axis(0.0));
    assert_eq!(outcome.cause, SemanticActionCause::PhysicalInput);
    assert_eq!(outcome.provenance, Some(binding.provenance));
    assert_eq!(binding.provenance.press_sequence(), Some(right));
    assert_eq!(Some(binding.provenance.sequence()), loss.first_sequence());
    assert!(binding.provenance.press_route().unwrap().is_ui_routable());
    assert!(!binding.provenance.route().is_ui_routable());
    runtime.finish().unwrap();
}

#[test]
fn exact_step_does_not_reorder_a_ready_release_past_a_pending_focus_loss() {
    let mut runtime = runtime_with_route(true, None);
    let right = key(&mut runtime, ButtonDriverInput::Press(RIGHT));
    let other_left = key_at(&mut runtime, OTHER_WINDOW, ButtonDriverInput::Press(LEFT));
    assert_eq!(tick(&mut runtime).value, Some(axis(0.0)));
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();

    let left = key(&mut runtime, ButtonDriverInput::Press(LEFT));
    let (loss, released) = lose_focus(&mut runtime);
    assert_eq!(released, [RIGHT, LEFT]);
    let other_release = key_at(&mut runtime, OTHER_WINDOW, ButtonDriverInput::Release(LEFT));
    let fixed = step(&mut runtime);
    assert_eq!(fixed.value, Some(axis(0.0)));
    assert!(fixed.binding.is_empty());
    assert!(fixed.semantic.is_empty());

    runtime.drive(Duration::ZERO).unwrap();
    let fixed = step(&mut runtime);
    assert_eq!(fixed.value, None);
    let [activation, terminal] = fixed.semantic.as_slice() else {
        panic!("expected only the earlier press's activation and the last explicit release");
    };
    assert_eq!(activation.phase, ActionPhase::Started);
    assert_eq!(activation.value, axis(-1.0));
    assert_eq!(activation.provenance.unwrap().sequence(), left);
    assert_eq!(terminal.phase, ActionPhase::Released);
    assert_eq!(terminal.value, axis(0.0));
    assert_eq!(terminal.provenance.unwrap().sequence(), other_release);
    let [press, focus_right, focus_left, other] = fixed.binding.as_slice() else {
        panic!("expected every original binding edge in admission order");
    };
    assert_eq!(press.provenance.sequence(), left);
    assert_eq!(focus_right.provenance.press_sequence(), Some(right));
    assert_eq!(
        Some(focus_right.provenance.sequence()),
        loss.first_sequence()
    );
    assert_eq!(focus_left.provenance.press_sequence(), Some(left));
    assert_eq!(other.provenance.press_sequence(), Some(other_left));
    assert_eq!(other.provenance.sequence(), other_release);
    runtime.finish().unwrap();
}

#[test]
fn disconnect_waits_for_an_earlier_routed_press_on_the_same_seat() {
    for consumed in [None, Some(RIGHT)] {
        let mut runtime = runtime_with_route(true, consumed);
        let gamepad = driver(&mut runtime, |scope| {
            connect_gamepad_input(scope, OTHER_WINDOW, SEAT, 4)
        });
        let pad_press = driver(&mut runtime, |scope| {
            apply_gamepad_button_input(scope, &gamepad, 7, ButtonTransitionPhase::Pressed)
        });
        assert_eq!(tick(&mut runtime).value, Some(axis(1.0)));
        assert!(matches!(
            runtime.request_control(RuntimeControl::Pause),
            RuntimeControlRequestResult::Accepted(_)
        ));
        runtime.drive(Duration::ZERO).unwrap();

        let key_press = key(&mut runtime, ButtonDriverInput::Press(RIGHT));
        let disconnected = driver(&mut runtime, |scope| {
            disconnect_gamepad_input(scope, &gamepad)
        });
        let fixed = step(&mut runtime);
        assert_eq!(fixed.value, Some(axis(1.0)));
        assert!(fixed.semantic.is_empty() && fixed.binding.is_empty());

        runtime.drive(Duration::ZERO).unwrap();
        let fixed = step(&mut runtime);
        let release = fixed.binding.last().unwrap();
        assert_eq!(release.phase, ActionPhase::Released);
        assert_eq!(
            release.provenance.press_sequence(),
            pad_press.first_sequence()
        );
        assert_eq!(
            Some(release.provenance.sequence()),
            disconnected.first_sequence()
        );
        if consumed.is_some() {
            assert_eq!(fixed.value, None);
            assert_eq!(fixed.binding.len(), 1);
            let [outcome] = fixed.semantic.as_slice() else {
                panic!("expected the final gameplay contribution to release");
            };
            assert_eq!(outcome.phase, ActionPhase::Released);
            assert_eq!(outcome.provenance, Some(release.provenance));
        } else {
            assert_eq!(fixed.value, Some(axis(1.0)));
            assert!(fixed.semantic.is_empty());
            assert_eq!(fixed.binding.len(), 2);
            assert_eq!(fixed.binding[0].provenance.sequence(), key_press);
        }
        runtime.finish().unwrap();
    }
}

#[test]
fn pointer_leave_and_retirement_neutralize_both_buttons_together() {
    let pointer = InputPointerId::new(1);
    for retire in [false, true] {
        let mut runtime = runtime(false);
        driver(&mut runtime, |scope| {
            connect_input_pointer(scope, WINDOW, SEAT, pointer)?;
            apply_pointer_driver_input_at(
                scope,
                WINDOW,
                SEAT,
                pointer,
                PointerDriverInput::Moved(Vec2::new(4.0, 5.0)),
            )?;
            for button in [MouseButton::Left, MouseButton::Right] {
                apply_mouse_driver_input_at(
                    scope,
                    WINDOW,
                    SEAT,
                    pointer,
                    ButtonDriverInput::Press(button),
                )?;
            }
            Ok(())
        });
        assert_eq!(tick(&mut runtime).value, Some(axis(0.0)));
        let (admission, released) = driver(&mut runtime, |scope| {
            if retire {
                retire_input_pointer(scope, WINDOW, SEAT, pointer)
            } else {
                apply_pointer_driver_input_at(
                    scope,
                    WINDOW,
                    SEAT,
                    pointer,
                    PointerDriverInput::Left,
                )
            }
        });
        assert_eq!(released, [MouseButton::Left, MouseButton::Right]);
        let timeline = runtime.world().resource::<InputTimeline>();
        assert!(
            timeline.entries().take(2).all(|entry| {
                entry.invalidation() == Some(InputInvalidationReason::PointerLeft)
            })
        );
        let fixed = tick(&mut runtime);
        assert_eq!(fixed.value, None);
        assert!(fixed.semantic.is_empty());
        assert_eq!(fixed.binding.len(), 2);
        assert_eq!(
            Some(fixed.binding[0].provenance.sequence()),
            admission.first_sequence()
        );
        assert!(
            fixed
                .binding
                .iter()
                .all(|edge| edge.phase == ActionPhase::Released)
        );
        runtime.finish().unwrap();
    }
}
