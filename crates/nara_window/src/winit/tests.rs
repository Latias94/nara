use super::*;
use crate::{WindowEvents, WindowPlugin};
use nara_app::{App, CoreStage, RuntimeClosePolicy, RuntimeFaultKind};
use nara_ecs::{Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapLimits,
    ActionMaps, ActionPhase, ActionState, AxisDeadZone, ButtonInput, GamepadButton, InputPlugin,
    InputSet, PointerState,
};

mod owned_session;

fn start_runtime(app: App) -> RuntimeInstance {
    RuntimeInstance::start(app).unwrap()
}

fn platform_app(runtime: RuntimeInstance) -> WinitApp {
    WinitApp::new(
        runtime,
        Box::new(NoPresentationDriver),
        WinitControlFlow::Poll,
        DEFAULT_NATIVE_RETIREMENT_BUDGET,
    )
}

fn finish_unit_app(mut app: WinitApp) {
    app.begin_shutdown_transition(Instant::now());
    for _ in 0..8 {
        if app.shutdown == WinitShutdownState::Complete {
            break;
        }
        app.poll_native_shutdown_once(Instant::now());
    }
    assert_eq!(app.shutdown, WinitShutdownState::Complete);
    assert_eq!(app.runtime.state(), RuntimeState::Stopped);
}

fn window_app() -> App {
    let mut app = App::new();
    app.add_plugin(WindowPlugin::default()).unwrap();
    app
}

#[test]
fn runtime_scope_fault_preserves_managed_runtime_identity() {
    assert!(matches!(
        runtime_scope_error(RuntimeScopeError::Faulted {
            fault: RuntimeFault::engine(
                RuntimeFaultKind::RequiredService,
                "nara.test.winit-service"
            ),
        }),
        AppRunError::ManagedRuntime {
            kind: RuntimeFaultKind::RequiredService,
            fault_source: "nara.test.winit-service",
            ..
        }
    ));
}

#[test]
fn unavailable_runtime_scope_remains_a_runner_error() {
    assert!(matches!(
        runtime_scope_error(RuntimeScopeError::Unavailable {
            state: RuntimeState::Stopped
        }),
        AppRunError::Runner { .. }
    ));
}

#[test]
fn converts_named_physical_keys_and_ignores_unidentified_codes() {
    use winit::keyboard::NativeKeyCode;

    for (code, key) in [
        (WinitKeyCode::Escape, KeyCode::Escape),
        (WinitKeyCode::Enter, KeyCode::Enter),
        (WinitKeyCode::Space, KeyCode::Space),
        (WinitKeyCode::Tab, KeyCode::Tab),
        (WinitKeyCode::ArrowLeft, KeyCode::ArrowLeft),
        (WinitKeyCode::KeyA, KeyCode::KeyA),
        (WinitKeyCode::Digit1, KeyCode::Digit1),
        (WinitKeyCode::Numpad1, KeyCode::Numpad1),
        (WinitKeyCode::NumpadEnter, KeyCode::NumpadEnter),
        (WinitKeyCode::NumpadDivide, KeyCode::NumpadDivide),
        (WinitKeyCode::ControlLeft, KeyCode::ControlLeft),
        (WinitKeyCode::ControlRight, KeyCode::ControlRight),
        (WinitKeyCode::SuperLeft, KeyCode::MetaLeft),
        (WinitKeyCode::SuperRight, KeyCode::MetaRight),
        (WinitKeyCode::PageDown, KeyCode::PageDown),
        (WinitKeyCode::IntlBackslash, KeyCode::IntlBackslash),
        (WinitKeyCode::IntlRo, KeyCode::IntlRo),
        (WinitKeyCode::IntlYen, KeyCode::IntlYen),
        (WinitKeyCode::F1, KeyCode::F1),
        (WinitKeyCode::F35, KeyCode::F35),
        (WinitKeyCode::MediaTrackNext, KeyCode::MediaTrackNext),
        (WinitKeyCode::AudioVolumeMute, KeyCode::AudioVolumeMute),
    ] {
        assert_eq!(convert_key_code(code), Some(key));
        assert_eq!(convert_physical_key(PhysicalKey::Code(code)), Some(key));
    }
    assert_eq!(
        convert_physical_key(PhysicalKey::Unidentified(NativeKeyCode::Unidentified)),
        None
    );
}

#[test]
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn iso_extra_key_release_does_not_release_a_held_backquote() {
    use winit::platform::scancode::PhysicalKeyExtScancode;

    #[cfg(target_os = "macos")]
    let scans = [0x32, 0x0a];
    #[cfg(target_os = "windows")]
    let scans = [0x29, 0x56];
    let [backquote, iso] = scans.map(PhysicalKey::from_scancode);
    assert_eq!(backquote, PhysicalKey::Code(WinitKeyCode::Backquote));
    assert_eq!(iso, PhysicalKey::Code(WinitKeyCode::IntlBackslash));
    assert_eq!(
        [backquote, iso].map(PhysicalKey::to_scancode),
        scans.map(Some)
    );

    let context = ActionContext::gameplay();
    let actions = ["test.backquote", "test.iso"].map(|name| ActionId::new(name).unwrap());
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder.declare_context(context.clone(), true).unwrap();
    for (action, key) in actions
        .iter()
        .zip([KeyCode::Backquote, KeyCode::IntlBackslash])
    {
        builder
            .bind(ActionBinding::key(action.clone(), key))
            .unwrap();
    }
    let definition = builder.compile().unwrap();
    assert_eq!(validate_action_map_keyboard(&definition), Ok(()));
    let mut app = window_app();
    app.insert_resource(ActionMaps::new(
        ActionMap::new(
            definition.into_bindings(),
            nara_core::ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        )
        .unwrap(),
    ))
    .unwrap();
    app.add_plugin(InputPlugin).unwrap();
    let mut state = platform_app(start_runtime(app));
    for (physical_key, phase, expected) in [
        (backquote, ElementState::Pressed, [true, false]),
        (iso, ElementState::Pressed, [true, true]),
        (iso, ElementState::Released, [true, false]),
        (backquote, ElementState::Released, [false, false]),
    ] {
        assert!(
            state
                .apply_physical_keyboard_driver_event(
                    RawKeyEvent {
                        physical_key,
                        state: phase
                    },
                    false,
                )
                .unwrap()
        );
        state.runtime.drive(Duration::ZERO).unwrap();
        let semantic = state.runtime.world().resource::<ActionState>();
        for (action, expected) in actions.iter().zip(expected) {
            assert_eq!(semantic.button(action.as_str()), expected);
        }
    }
    finish_unit_app(state);
}

#[test]
fn keyboard_validation_accepts_non_keyboard_bindings() {
    let confirm = ActionId::new("test.confirm").unwrap();
    let movement = ActionId::new("test.move").unwrap();
    let mut bindings = ActionMapBuilder::new(ActionMapLimits::default());
    bindings
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    for binding in [
        ActionBinding::mouse(confirm.clone(), MouseButton::Left),
        ActionBinding::gamepad_button(confirm, GamepadButton::DPadRight.code()),
        ActionBinding::gamepad_axis(movement, 0, AxisDeadZone::default(), false),
    ] {
        bindings.bind(binding).unwrap();
    }
    let definition = bindings.compile().unwrap();
    assert_eq!(validate_action_map_keyboard(&definition), Ok(()));
}

#[test]
fn converts_mouse_buttons() {
    for (native, button) in [
        (WinitMouseButton::Left, MouseButton::Left),
        (WinitMouseButton::Right, MouseButton::Right),
        (WinitMouseButton::Middle, MouseButton::Middle),
        (WinitMouseButton::Back, MouseButton::Other(4)),
        (WinitMouseButton::Other(9), MouseButton::Other(9)),
    ] {
        assert_eq!(convert_mouse_button(native), button);
    }
}

fn raw_key_event(code: WinitKeyCode, state: ElementState) -> RawKeyEvent {
    RawKeyEvent {
        physical_key: PhysicalKey::Code(code),
        state,
    }
}

#[derive(Default, Resource)]
struct ObservedActionPhases(Vec<ActionPhase>);

#[test]
fn focus_event_wiring_preserves_ordered_semantic_action_edges() {
    let mut app = window_app();
    app.add_plugin(InputPlugin).unwrap();
    let action = ActionId::new("test.move-up").unwrap();
    app.insert_resource(ActionMaps::new(
        ActionMap::from_bindings([ActionBinding::key(action.clone(), KeyCode::KeyW)]).unwrap(),
    ))
    .unwrap()
    .init_resource::<ObservedActionPhases>()
    .unwrap()
    .add_systems(
        CoreStage::PreUpdate,
        (move |actions: Res<ActionState>, mut observed: ResMut<ObservedActionPhases>| {
            observed.0.extend(
                actions
                    .edges()
                    .iter()
                    .filter(|edge| edge.seat == InputSeatId::PRIMARY && edge.action == action)
                    .map(|edge| edge.phase),
            );
        })
        .after(InputSet::ResolveActions),
    )
    .unwrap();
    let mut state = platform_app(start_runtime(app));
    assert!(
        state
            .apply_physical_keyboard_driver_event(
                raw_key_event(WinitKeyCode::KeyW, ElementState::Pressed),
                false
            )
            .unwrap()
    );
    state.runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        state.runtime.world().resource::<ObservedActionPhases>().0,
        [ActionPhase::Started]
    );
    assert!(state.is_healthy_live());
    state
        .apply_focus_driver_event(WindowId::PRIMARY, false)
        .unwrap();
    state.runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        state.runtime.world().resource::<ObservedActionPhases>().0,
        [ActionPhase::Started, ActionPhase::Released]
    );
    assert!(
        !state
            .runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::KeyW)
    );
    state
        .apply_focus_driver_event(WindowId::PRIMARY, true)
        .unwrap();
    for (edge, repeat, synthetic) in [
        (ElementState::Pressed, false, true),
        (ElementState::Released, false, true),
        (ElementState::Pressed, true, false),
    ] {
        assert!(
            !state
                .apply_keyboard_event_with_origin(
                    WindowId::PRIMARY,
                    raw_key_event(WinitKeyCode::KeyW, edge),
                    repeat,
                    synthetic,
                )
                .unwrap()
        );
    }
    state.runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        state.runtime.world().resource::<ObservedActionPhases>().0,
        [ActionPhase::Started, ActionPhase::Released]
    );
    assert!(
        state
            .apply_physical_keyboard_driver_event(
                raw_key_event(WinitKeyCode::KeyW, ElementState::Pressed),
                false
            )
            .unwrap()
    );
    state.runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        state.runtime.world().resource::<ObservedActionPhases>().0,
        [
            ActionPhase::Started,
            ActionPhase::Released,
            ActionPhase::Started
        ]
    );
    finish_unit_app(state);
}

#[test]
fn window_input_activation_without_input_plugin_keeps_input_optional() {
    let mut state = platform_app(start_runtime(window_app()));
    assert!(!state.input_installed());
    assert!(
        !state
            .apply_physical_keyboard_driver_event(
                raw_key_event(WinitKeyCode::Enter, ElementState::Pressed),
                false
            )
            .unwrap()
    );
    state
        .apply_focus_driver_event(WindowId::PRIMARY, true)
        .unwrap();
    finish_unit_app(state);
}

#[test]
fn runner_configuration_does_not_install_runtime_prerequisites() {
    let app = App::new();
    let _runner = WinitRunner::default();
    assert!(!app.world().contains_resource::<WindowEvents>());
    assert!(!app.world().contains_resource::<BackendWindowHandles>());
    assert!(!app.world().contains_resource::<ButtonInput<KeyCode>>());
    assert!(!app.world().contains_resource::<ButtonInput<MouseButton>>());
    assert!(!app.world().contains_resource::<PointerState>());
}

#[test]
fn configured_window_requires_exactly_one_runtime_target() {
    let missing = start_runtime(App::new());
    assert!(configured_primary_window(&missing).is_err());
    finish_unit_app(platform_app(missing));
    let one = start_runtime(window_app());
    assert_eq!(
        configured_primary_window(&one).unwrap().id,
        WindowId::PRIMARY
    );
    finish_unit_app(platform_app(one));
    let mut app = window_app();
    app.world_mut()
        .unwrap()
        .spawn(Window::default().with_id(WindowId::new(2)));
    let multiple = start_runtime(app);
    assert!(configured_primary_window(&multiple).is_err());
    finish_unit_app(platform_app(multiple));
}
