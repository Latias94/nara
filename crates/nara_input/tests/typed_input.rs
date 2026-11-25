use std::time::Duration;

use nara_app::{
    App, CoreStage, RuntimeControl, RuntimeControlRequestResult, RuntimeInstance, RuntimeState,
};
use nara_core::Vec2;
use nara_ecs::{
    Res, ResMut, Resource, bevy_ecs::message::MessageReader, schedule::IntoScheduleConfigs,
};
use nara_input::{
    ActionBinding, ActionId, ActionMap, ActionMapBindings, ActionMaps, ActionState, AxisDeadZone,
    AxisDirection, AxisValue, ButtonDriverInput, ButtonInput, ButtonInputDriverError, GamepadAxis,
    GamepadInput, GamepadInputKind, ImeEvent, ImeEventKind, InputAdmission,
    InputGeometryGeneration, InputIngressError, InputLimits, InputPlugin, InputPointerId,
    InputPresentationEpoch, InputPresentedRoute, InputRouteGeneration, InputSeatId, InputSet,
    InputSlotGeneration, InputSurfaceId, InputTimeline, InputWindowEvent, InputWindowId, KeyCode,
    MouseWheel, MouseWheelUnit, PointerDriverInput, TextInput, TouchInput, TouchPhase,
    WindowDeviceInput, apply_gamepad_axis_input, apply_keyboard_driver_input_at,
    apply_mouse_wheel_driver_input_at, apply_pointer_driver_input_at, apply_window_device_input,
    apply_window_input_driver_event, connect_gamepad_input, connect_input_endpoint,
    disconnect_gamepad_input,
};

const WINDOW: InputWindowId = InputWindowId::new(1);
const SEAT: InputSeatId = InputSeatId::PRIMARY;
const POINTER: InputPointerId = InputPointerId::PRIMARY;

#[derive(Default, Resource)]
struct DeviceObservations {
    text: Vec<TextInput>,
    ime: Vec<ImeEvent>,
    touch: Vec<TouchInput>,
    wheel: Vec<MouseWheel>,
    gamepad: Vec<GamepadInput>,
}

fn observe_devices(
    mut text: MessageReader<TextInput>,
    mut ime: MessageReader<ImeEvent>,
    mut touch: MessageReader<TouchInput>,
    mut wheel: MessageReader<MouseWheel>,
    mut gamepad: MessageReader<GamepadInput>,
    mut observed: ResMut<DeviceObservations>,
) {
    observed.text.extend(text.read().cloned());
    observed.ime.extend(ime.read().cloned());
    observed.touch.extend(touch.read().copied());
    observed.wheel.extend(wheel.read().copied());
    observed.gamepad.extend(gamepad.read().copied());
}

fn device_app(limits: InputLimits) -> App {
    let mut app = App::new();
    app.insert_resource(InputTimeline::with_limits(limits).unwrap())
        .unwrap()
        .add_plugin(InputPlugin)
        .unwrap()
        .init_resource::<DeviceObservations>()
        .unwrap()
        .add_systems(CoreStage::Update, observe_devices)
        .unwrap();
    app
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
    assert_eq!(runtime.state(), RuntimeState::Stopped);
}

fn text(value: &str) -> WindowDeviceInput {
    WindowDeviceInput::Text(TextInput {
        window: WINDOW,
        seat: SEAT,
        text: value.into(),
    })
}

fn ime(kind: ImeEventKind) -> WindowDeviceInput {
    WindowDeviceInput::Ime(ImeEvent {
        window: WINDOW,
        seat: SEAT,
        kind,
    })
}

fn touch(id: u64, phase: TouchPhase, position: Vec2) -> WindowDeviceInput {
    WindowDeviceInput::Touch(TouchInput {
        window: WINDOW,
        seat: SEAT,
        id,
        phase,
        position,
        force: Some(0.6),
    })
}

#[test]
fn text_ime_and_touch_are_delivered_once_and_loss_cancels_open_sessions_at_capacity() {
    let app = device_app(InputLimits::new(12, 2, 2, 4).unwrap());
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let initial = Vec2::new(4.0, 5.0);
    let moved = Vec2::new(7.0, 8.0);
    let admission = runtime
        .with_driver_scope(|scope| {
            connect_input_endpoint(scope, WINDOW, SEAT).unwrap();
            apply_window_input_driver_event(scope, WINDOW, SEAT, InputWindowEvent::Focused(true))
                .unwrap();
            for input in [
                text("é"),
                ime(ImeEventKind::Enabled),
                ime(ImeEventKind::Preedit {
                    text: "你好".into(),
                    cursor: Some((3, 6)),
                }),
                text("suppressed during composition"),
                ime(ImeEventKind::Commit("你好".into())),
                text("!"),
                touch(7, TouchPhase::Started, initial),
                touch(7, TouchPhase::Moved, moved),
                touch(8, TouchPhase::Started, initial),
                touch(8, TouchPhase::Ended, initial),
                text("fits"),
            ] {
                apply_window_device_input(scope, input).unwrap();
            }
            assert!(matches!(
                apply_window_device_input(scope, text("overflow")),
                Err(ButtonInputDriverError::Timeline(
                    InputIngressError::CapacityExceeded { limit: 12, .. }
                ))
            ));
            apply_window_input_driver_event(scope, WINDOW, SEAT, InputWindowEvent::Focused(false))
                .unwrap();
            apply_window_device_input(scope, touch(7, TouchPhase::Ended, moved)).unwrap();
            apply_window_device_input(scope, touch(9, TouchPhase::Started, initial)).unwrap();
            apply_window_device_input(scope, text("unfocused")).unwrap();
        })
        .unwrap();
    assert_eq!(admission.post_operation_fault, None);
    runtime.drive(Duration::ZERO).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let seen = runtime.world().resource::<DeviceObservations>();
    assert_eq!(
        seen.text
            .iter()
            .map(|input| input.text.as_str())
            .collect::<Vec<_>>(),
        ["é", "你好", "!", "fits"]
    );
    assert_eq!(
        seen.ime.iter().map(|event| &event.kind).collect::<Vec<_>>(),
        [
            &ImeEventKind::Enabled,
            &ImeEventKind::Preedit {
                text: "你好".into(),
                cursor: Some((3, 6)),
            },
            &ImeEventKind::Commit("你好".into()),
            &ImeEventKind::Disabled,
        ]
    );
    assert_eq!(
        seen.touch
            .iter()
            .map(|event| (event.id, event.phase))
            .collect::<Vec<_>>(),
        [
            (7, TouchPhase::Started),
            (7, TouchPhase::Moved),
            (8, TouchPhase::Started),
            (8, TouchPhase::Ended),
            (7, TouchPhase::Cancelled),
        ],
    );
    assert_eq!(seen.touch.last().unwrap().position, moved);
    assert_eq!(seen.touch.last().unwrap().force, Some(0.6));
    stop_runtime(&mut runtime);
}

#[test]
fn wheels_keep_the_admitted_route_and_units_across_focus_loss() {
    let mut app = device_app(InputLimits::default());
    let surface = InputSurfaceId::new(51);
    {
        let mut timeline = app.world_mut().unwrap().resource_mut::<InputTimeline>();
        timeline.connect_endpoint(WINDOW, SEAT).unwrap();
        timeline.connect_pointer(WINDOW, SEAT, POINTER).unwrap();
        timeline
            .publish_presented_route(InputPresentedRoute::new(
                WINDOW,
                surface,
                SEAT,
                InputPresentationEpoch::new(1).unwrap(),
                InputSlotGeneration::new(1).unwrap(),
                InputRouteGeneration::new(1).unwrap(),
                InputGeometryGeneration::new(1).unwrap(),
            ))
            .unwrap();
    }
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let position = Vec2::new(12.0, 6.0);
    let admission = runtime
        .with_driver_scope(|scope| {
            apply_pointer_driver_input_at(
                scope,
                WINDOW,
                SEAT,
                POINTER,
                PointerDriverInput::Moved(position),
            )
            .unwrap();
            for (delta, unit, phase) in [
                (
                    Vec2::new(0.0, -2.0),
                    MouseWheelUnit::Lines,
                    TouchPhase::Moved,
                ),
                (
                    Vec2::new(4.0, 0.0),
                    MouseWheelUnit::Pixels,
                    TouchPhase::Ended,
                ),
            ] {
                let admission: InputAdmission = apply_mouse_wheel_driver_input_at(
                    scope, WINDOW, SEAT, POINTER, delta, unit, phase,
                )
                .unwrap();
                assert_eq!(admission.admitted(), 1);
            }
            apply_window_input_driver_event(scope, WINDOW, SEAT, InputWindowEvent::Focused(false))
                .unwrap();
            assert_eq!(
                apply_mouse_wheel_driver_input_at(
                    scope,
                    WINDOW,
                    SEAT,
                    POINTER,
                    Vec2::ONE,
                    MouseWheelUnit::Lines,
                    TouchPhase::Moved,
                )
                .unwrap()
                .admitted(),
                0
            );
        })
        .unwrap();
    assert_eq!(admission.post_operation_fault, None);
    runtime.drive(Duration::ZERO).unwrap();
    let seen = &runtime.world().resource::<DeviceObservations>().wheel;
    assert_eq!(seen.len(), 2);
    for event in seen {
        assert_eq!(event.position, Some(position));
        assert_eq!(event.route.surface(), Some(surface));
        assert_eq!(event.route.pointer(), Some(POINTER));
    }
    assert_eq!(seen[0].unit, MouseWheelUnit::Lines);
    assert_eq!(seen[0].delta, Vec2::new(0.0, -2.0));
    assert_eq!(seen[1].unit, MouseWheelUnit::Pixels);
    assert_eq!(seen[1].delta, Vec2::new(4.0, 0.0));
    assert_eq!(seen[1].phase, TouchPhase::Ended);
    stop_runtime(&mut runtime);
}

#[derive(Default, Resource)]
struct FixedObservations(Vec<(Vec2, bool)>);

#[derive(Resource)]
struct NewBindings(ActionMapBindings);

fn record_fixed(actions: Res<ActionState>, mut values: ResMut<FixedObservations>) {
    values
        .0
        .push((actions.axis_2d("move"), actions.button("jump")));
}

fn replace_on_key(
    keys: Res<ButtonInput<KeyCode>>,
    new_bindings: Res<NewBindings>,
    mut maps: ResMut<ActionMaps>,
) {
    if keys.just_pressed(KeyCode::F2) {
        maps.get_mut(SEAT)
            .unwrap()
            .replace_bindings(new_bindings.0.clone());
    }
}

fn move_binding(axis: &str, key: KeyCode) -> ActionBinding {
    ActionBinding::key_axis(ActionId::new(axis).unwrap(), key, AxisDirection::Positive)
}

#[test]
fn ordinary_fixed_system_reads_fused_actions_and_the_existing_map_can_rebind() {
    let mut app = device_app(InputLimits::default());
    app.insert_resource(ActionMaps::new(
        ActionMap::from_bindings([
            move_binding("move.x", KeyCode::KeyD),
            move_binding("move.x", KeyCode::ArrowRight),
            move_binding("move.y", KeyCode::KeyW),
            ActionBinding::gamepad_axis(
                ActionId::new("move.x").unwrap(),
                GamepadAxis::LeftStickX.code(),
                AxisDeadZone::default(),
                false,
            ),
            ActionBinding::key(ActionId::new("jump").unwrap(), KeyCode::Space),
        ])
        .unwrap(),
    ))
    .unwrap()
    .insert_resource(NewBindings(
        ActionMap::from_bindings([move_binding("move.x", KeyCode::KeyL)])
            .unwrap()
            .bindings_handle(),
    ))
    .unwrap()
    .init_resource::<FixedObservations>()
    .unwrap()
    .add_systems(
        CoreStage::First,
        replace_on_key
            .after(InputSet::ProjectTimeline)
            .before(InputSet::StageActions),
    )
    .unwrap()
    .add_systems(CoreStage::FixedUpdate, record_fixed)
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    let axis = GamepadAxis::LeftStickX.code();
    let admission = runtime
        .with_driver_scope(|scope| {
            connect_input_endpoint(scope, WINDOW, SEAT).unwrap();
            let connection = connect_gamepad_input(scope, WINDOW, SEAT, 41).unwrap();
            for key in [KeyCode::KeyD, KeyCode::KeyW, KeyCode::Space] {
                apply_keyboard_driver_input_at(scope, WINDOW, SEAT, ButtonDriverInput::Press(key))
                    .unwrap();
            }
            apply_gamepad_axis_input(scope, &connection, axis, AxisValue::new(-0.25).unwrap())
                .unwrap();
            connection
        })
        .unwrap();
    assert_eq!(admission.post_operation_fault, None);
    let connection = admission.output;
    let fixed_frame = Duration::from_millis(20);
    runtime.drive(fixed_frame).unwrap();
    assert_eq!(
        runtime.world().resource::<FixedObservations>().0.last(),
        Some(&(Vec2::new(0.75, 1.0), true))
    );
    let admission = runtime
        .with_driver_scope(|scope| {
            apply_keyboard_driver_input_at(
                scope,
                WINDOW,
                SEAT,
                ButtonDriverInput::Release(KeyCode::KeyD),
            )
            .unwrap();
            apply_keyboard_driver_input_at(
                scope,
                WINDOW,
                SEAT,
                ButtonDriverInput::Press(KeyCode::ArrowRight),
            )
            .unwrap();
            disconnect_gamepad_input(scope, &connection).unwrap();
        })
        .unwrap();
    assert_eq!(admission.post_operation_fault, None);
    runtime.drive(fixed_frame).unwrap();
    assert_eq!(
        runtime.world().resource::<FixedObservations>().0.last(),
        Some(&(Vec2::ONE, true))
    );
    let gamepad = &runtime.world().resource::<DeviceObservations>().gamepad;
    assert!(
        gamepad
            .iter()
            .all(|event| event.connection_generation == connection.connection_generation())
    );
    assert_eq!(
        gamepad.iter().map(|event| event.kind).collect::<Vec<_>>(),
        [
            GamepadInputKind::Connected,
            GamepadInputKind::Axis {
                axis: GamepadAxis::LeftStickX,
                value: AxisValue::new(-0.25).unwrap(),
            },
            GamepadInputKind::Axis {
                axis: GamepadAxis::LeftStickX,
                value: AxisValue::ZERO,
            },
            GamepadInputKind::Disconnected,
        ]
    );
    for (key, expected) in [(KeyCode::F2, Vec2::ZERO), (KeyCode::KeyL, Vec2::X)] {
        let admission = runtime
            .with_driver_scope(|scope| {
                apply_keyboard_driver_input_at(scope, WINDOW, SEAT, ButtonDriverInput::Press(key))
                    .unwrap();
            })
            .unwrap();
        assert_eq!(admission.post_operation_fault, None);
        runtime.drive(fixed_frame).unwrap();
        assert_eq!(
            runtime.world().resource::<FixedObservations>().0.last(),
            Some(&(expected, false))
        );
    }
    stop_runtime(&mut runtime);
}
