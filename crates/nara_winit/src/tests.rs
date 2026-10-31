use super::*;
use nara_app::{
    App, RuntimeAdmissionReservation, RuntimeClosePolicy, RuntimeFaultKind, RuntimeObligationLedger,
};
use nara_gameplay::{
    ActionCommandBinding, ActionCommandMap, GameplayCommandPlugin, GameplayCommandQueue,
    GameplayCommandTypeId,
};
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapLimits,
    ActionPhase, ButtonInput, InputEventKind, InputEventOrigin, InputGeometryGeneration,
    InputInvalidationReason, InputPlugin, InputPresentationEpoch, InputPresentedRoute,
    InputRouteGeneration, InputSlotGeneration, InputSurfaceId, PointerState,
};
use nara_window::{WindowEvents, WindowPlugin};

mod owned_session;

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
fn converts_common_keyboard_codes_and_physical_keys() {
    for (code, key) in [
        (WinitKeyCode::Escape, KeyCode::Escape),
        (WinitKeyCode::Enter, KeyCode::Enter),
        (WinitKeyCode::Space, KeyCode::Space),
        (WinitKeyCode::ArrowLeft, KeyCode::ArrowLeft),
        (WinitKeyCode::KeyA, KeyCode::Character('a')),
        (WinitKeyCode::Digit1, KeyCode::Character('1')),
    ] {
        assert_eq!(convert_key_code(code), Some(key));
        assert_eq!(convert_physical_key(PhysicalKey::Code(code)), Some(key));
    }
    assert_eq!(convert_key_code(WinitKeyCode::F1), None);
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

#[test]
fn focus_gate_rejects_repeats_and_requires_a_fresh_press_after_regain() {
    let key = KeyCode::Character('w');
    let mut gate = WinitInputGate::default();
    assert_eq!(
        gate.keyboard_input(key, ElementState::Pressed, false),
        Some(ButtonDriverInput::Press(key))
    );
    assert_eq!(gate.keyboard_input(key, ElementState::Pressed, true), None);
    gate.lose_focus([key], [MouseButton::Left]);
    gate.gain_focus();
    assert_eq!(gate.keyboard_input(key, ElementState::Pressed, false), None);
    assert_eq!(
        gate.mouse_input(MouseButton::Left, ElementState::Pressed),
        None
    );
    assert_eq!(
        gate.keyboard_input(key, ElementState::Released, false),
        Some(ButtonDriverInput::Release(key))
    );
    assert_eq!(
        gate.mouse_input(MouseButton::Left, ElementState::Released),
        Some(ButtonDriverInput::Release(MouseButton::Left))
    );
    assert_eq!(
        gate.keyboard_input(key, ElementState::Pressed, false),
        Some(ButtonDriverInput::Press(key))
    );
    assert_eq!(
        gate.mouse_input(MouseButton::Left, ElementState::Pressed),
        Some(ButtonDriverInput::Press(MouseButton::Left))
    );
}

#[test]
fn initial_focus_state_is_applied_to_the_input_gate() {
    let mut gate = WinitInputGate::reset_for_focus(false);
    assert_eq!(
        gate.keyboard_input(KeyCode::Enter, ElementState::Pressed, false),
        None
    );
    gate.gain_focus();
    assert_eq!(
        gate.keyboard_input(KeyCode::Enter, ElementState::Pressed, false),
        Some(ButtonDriverInput::Press(KeyCode::Enter))
    );
}

fn raw_key_event(code: WinitKeyCode, state: ElementState) -> RawKeyEvent {
    RawKeyEvent {
        physical_key: PhysicalKey::Code(code),
        state,
    }
}

#[test]
fn focus_event_wiring_releases_input_and_lowers_a_semantic_stop_command() {
    let mut app = window_app();
    app.add_plugin(InputPlugin).unwrap();
    app.add_plugin(GameplayCommandPlugin::default()).unwrap();
    let action = ActionId::new("test.move-up").unwrap();
    let mut bindings = ActionMapBuilder::new(ActionMapLimits::default());
    bindings
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    bindings
        .bind(ActionBinding::key(action.clone(), KeyCode::Character('w')))
        .unwrap();
    app.insert_resource(
        ActionMap::new(
            bindings.compile().unwrap().into_bindings(),
            nara_core::ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
        )
        .unwrap(),
    )
    .unwrap();
    for (phase, command) in [
        (ActionPhase::Started, "test.move.started"),
        (ActionPhase::Released, "test.move.stop"),
    ] {
        app.world_mut()
            .unwrap()
            .resource_mut::<ActionCommandMap>()
            .bind(ActionCommandBinding::new(
                action.clone(),
                phase,
                GameplayCommandTypeId::new(command).unwrap(),
            ))
            .unwrap();
    }
    let mut state = platform_app(start_runtime(app));
    state.connect_window_input(WindowId::PRIMARY).unwrap();
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
        state
            .runtime
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        1
    );
    state
        .apply_focus_driver_event(WindowId::PRIMARY, false)
        .unwrap();
    state.runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        state
            .runtime
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        2
    );
    assert!(
        !state
            .runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::Character('w'))
    );
    state
        .apply_focus_driver_event(WindowId::PRIMARY, true)
        .unwrap();
    assert!(
        !state
            .apply_physical_keyboard_driver_event(
                raw_key_event(WinitKeyCode::KeyW, ElementState::Pressed),
                true
            )
            .unwrap()
    );
    assert!(
        !state
            .apply_physical_keyboard_driver_event(
                raw_key_event(WinitKeyCode::KeyW, ElementState::Pressed),
                false
            )
            .unwrap()
    );
    assert!(
        state
            .apply_physical_keyboard_driver_event(
                raw_key_event(WinitKeyCode::KeyW, ElementState::Released),
                false
            )
            .unwrap()
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
        state
            .runtime
            .world()
            .resource::<GameplayCommandQueue>()
            .stats()
            .accepted,
        3
    );
    finish_unit_app(state);
}

#[test]
fn winit_ingress_captures_the_exact_route_before_focus_loss_invalidation() {
    let mut app = window_app();
    app.add_plugin(InputPlugin).unwrap();
    let window = InputWindowId::new(WindowId::PRIMARY.get());
    {
        let mut timeline = app.world_mut().unwrap().resource_mut::<InputTimeline>();
        timeline
            .connect_endpoint(window, InputSeatId::PRIMARY)
            .unwrap();
        timeline
            .connect_pointer(window, InputSeatId::PRIMARY, InputPointerId::PRIMARY)
            .unwrap();
        timeline
            .publish_presented_route(InputPresentedRoute::new(
                window,
                InputSurfaceId::new(41),
                InputSeatId::PRIMARY,
                InputPresentationEpoch::new(73).unwrap(),
                InputSlotGeneration::new(3).unwrap(),
                InputRouteGeneration::new(5).unwrap(),
                InputGeometryGeneration::new(7).unwrap(),
            ))
            .unwrap();
    }
    let mut state = platform_app(start_runtime(app));
    state
        .apply_focus_driver_event(WindowId::PRIMARY, true)
        .unwrap();
    assert!(
        state
            .apply_physical_keyboard_driver_event(
                raw_key_event(WinitKeyCode::Enter, ElementState::Pressed),
                false
            )
            .unwrap()
    );
    state
        .apply_focus_driver_event(WindowId::PRIMARY, false)
        .unwrap();
    let entries = state
        .runtime
        .world()
        .resource::<InputTimeline>()
        .entries()
        .copied()
        .collect::<Vec<_>>();
    let pressed = entries
        .iter()
        .find(|entry| {
            matches!(
                entry.kind(),
                InputEventKind::Key {
                    key: KeyCode::Enter,
                    phase: nara_input::ButtonTransitionPhase::Pressed,
                }
            )
        })
        .unwrap();
    assert_eq!(pressed.route().surface(), Some(InputSurfaceId::new(41)));
    assert_eq!(pressed.origin(), InputEventOrigin::Platform);
    assert_eq!(
        pressed.route().presentation_epoch(),
        InputPresentationEpoch::new(73)
    );
    assert_eq!(
        pressed.route().slot_generation(),
        InputSlotGeneration::new(3)
    );
    assert_eq!(
        pressed.route().route_generation(),
        InputRouteGeneration::new(5)
    );
    assert_eq!(
        pressed.route().geometry_generation(),
        InputGeometryGeneration::new(7)
    );
    let focus_loss = entries.last().unwrap();
    assert!(matches!(
        focus_loss.kind(),
        InputEventKind::FocusChanged { focused: false }
    ));
    assert_eq!(
        focus_loss.invalidation(),
        Some(InputInvalidationReason::FocusLost)
    );
    assert_eq!(focus_loss.route(), pressed.route());
    finish_unit_app(state);
}

#[test]
fn window_input_activation_without_input_plugin_keeps_input_optional() {
    let mut state = platform_app(start_runtime(window_app()));
    state.connect_window_input(WindowId::PRIMARY).unwrap();
    assert!(!state.input_timeline_installed());
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
fn window_input_activation_connects_pointer_and_retirement_rejects_late_input() {
    let mut app = window_app();
    app.add_plugin(InputPlugin).unwrap();
    let mut state = platform_app(start_runtime(app));
    state.connect_window_input(WindowId::PRIMARY).unwrap();
    assert!(
        state
            .apply_physical_keyboard_driver_event(
                raw_key_event(WinitKeyCode::Enter, ElementState::Pressed),
                false
            )
            .unwrap()
    );
    state
        .with_driver_scope(|scope| {
            apply_mouse_driver_input_at(
                scope,
                InputWindowId::new(WindowId::PRIMARY.get()),
                InputSeatId::PRIMARY,
                InputPointerId::PRIMARY,
                ButtonDriverInput::Press(MouseButton::Left),
            )
            .map(|_| ())
        })
        .unwrap();
    retire_runtime_input(&mut state.runtime, &[WindowId::PRIMARY]).unwrap();
    let entries = state
        .runtime
        .world()
        .resource::<InputTimeline>()
        .entries()
        .copied()
        .collect::<Vec<_>>();
    assert!(matches!(
        entries.last().unwrap().kind(),
        InputEventKind::SurfaceRetired
    ));
    for kind in [
        InputEventKind::Key {
            key: KeyCode::Enter,
            phase: nara_input::ButtonTransitionPhase::Released,
        },
        InputEventKind::PointerButton {
            button: MouseButton::Left,
            phase: nara_input::ButtonTransitionPhase::Released,
        },
    ] {
        assert!(entries.iter().any(|entry| entry.kind() == kind));
    }
    assert!(
        state
            .apply_physical_keyboard_driver_event(
                raw_key_event(WinitKeyCode::KeyW, ElementState::Pressed),
                false
            )
            .is_err()
    );
    assert_eq!(
        state
            .runtime
            .world()
            .resource::<InputTimeline>()
            .entries()
            .count(),
        entries.len()
    );
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

#[cfg(feature = "platform-accessibility")]
#[test]
fn platform_accessibility_activation_is_captured_without_borrowing_runtime() {
    let state = platform_app(start_runtime(window_app()));
    assert!(!state.accessibility_activation.is_enabled());
    assert!(state.accessibility_sessions.is_empty());
    finish_unit_app(state);
    let mut app = window_app();
    accessibility::__private::install_winit_accessibility_contribution(&mut app).unwrap();
    let state = platform_app(start_runtime(app));
    assert!(state.accessibility_activation.is_enabled());
    assert!(state.accessibility_sessions.is_empty());
    finish_unit_app(state);
}

#[cfg(feature = "platform-accessibility")]
#[test]
fn partial_platform_accessibility_contribution_retains_runtime_for_close() {
    let mut app = window_app();
    accessibility::__private::install_partial_winit_accessibility_contribution(&mut app).unwrap();
    let state = platform_app(start_runtime(app));
    assert!(
        state
            .failure()
            .unwrap()
            .to_string()
            .contains("Winit accessibility contribution is partial")
    );
    assert!(state.owned_window_ids.is_empty());
    finish_unit_app(state);
}
