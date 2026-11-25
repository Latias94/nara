//! CPU fixtures inject activation and physical-key snapshots without asserting native focus.
//! Input admission, focus dispatch, paused frames, and control tickets retain their real owners.

use super::*;
use crate::winit::keyboard_continuation::physical_key;
use nara_app::{CoreStage, FixedUpdateSet, RuntimeFaultKind, RuntimeTimeSettings};
use nara_ecs::{Local, Res, ResMut, Resource, schedule::IntoScheduleConfigs};
use nara_input::{
    InputEventOrigin, InputInvalidationReason, KeyboardActionBinding, KeyboardContinuationError,
    KeyboardHold, SemanticActions,
};

#[test]
fn windows_focus_snapshot_roundtrips_supported_codes() {
    for code in [
        WinitKeyCode::Escape,
        WinitKeyCode::Space,
        WinitKeyCode::Enter,
        WinitKeyCode::ArrowUp,
        WinitKeyCode::ArrowDown,
        WinitKeyCode::ArrowLeft,
        WinitKeyCode::ArrowRight,
        WinitKeyCode::KeyA,
        WinitKeyCode::KeyB,
        WinitKeyCode::KeyC,
        WinitKeyCode::KeyD,
        WinitKeyCode::KeyE,
        WinitKeyCode::KeyF,
        WinitKeyCode::KeyG,
        WinitKeyCode::KeyH,
        WinitKeyCode::KeyI,
        WinitKeyCode::KeyJ,
        WinitKeyCode::KeyK,
        WinitKeyCode::KeyL,
        WinitKeyCode::KeyM,
        WinitKeyCode::KeyN,
        WinitKeyCode::KeyO,
        WinitKeyCode::KeyP,
        WinitKeyCode::KeyQ,
        WinitKeyCode::KeyR,
        WinitKeyCode::KeyS,
        WinitKeyCode::KeyT,
        WinitKeyCode::KeyU,
        WinitKeyCode::KeyV,
        WinitKeyCode::KeyW,
        WinitKeyCode::KeyX,
        WinitKeyCode::KeyY,
        WinitKeyCode::KeyZ,
        WinitKeyCode::Digit0,
        WinitKeyCode::Digit1,
        WinitKeyCode::Digit2,
        WinitKeyCode::Digit3,
        WinitKeyCode::Digit4,
        WinitKeyCode::Digit5,
        WinitKeyCode::Digit6,
        WinitKeyCode::Digit7,
        WinitKeyCode::Digit8,
        WinitKeyCode::Digit9,
    ] {
        let key = convert_key_code(code).expect("the declared physical code must be supported");
        let physical = physical_key(key).expect("the admitted key must have a physical inverse");
        assert_eq!(physical, code, "{key:?}");
        assert_eq!(convert_key_code(physical), Some(key));
    }
}

#[test]
fn windows_focus_snapshot_rejects_unsupported_characters() {
    for character in ('\0'..='\u{7f}')
        .filter(|character| !character.is_ascii_lowercase() && !character.is_ascii_digit())
    {
        assert_eq!(
            physical_key(KeyCode::Character(character)),
            None,
            "{character:?}"
        );
    }
    for character in [
        'é', 'É', 'ß', 'λ', 'Ж', '中', 'あ', 'ａ', '１', '😀', '\u{301}',
    ] {
        assert_eq!(
            physical_key(KeyCode::Character(character)),
            None,
            "{character:?}"
        );
    }
}

#[cfg(target_os = "windows")]
#[test]
fn cpu_windows_arrow_scancode_table_preserves_extended_prefix() {
    use winit::platform::scancode::PhysicalKeyExtScancode;

    for (key, scan, numpad_code, numpad_key) in [
        (
            KeyCode::ArrowUp,
            0xe048,
            WinitKeyCode::Numpad8,
            KeyCode::Numpad8,
        ),
        (
            KeyCode::ArrowDown,
            0xe050,
            WinitKeyCode::Numpad2,
            KeyCode::Numpad2,
        ),
        (
            KeyCode::ArrowLeft,
            0xe04b,
            WinitKeyCode::Numpad4,
            KeyCode::Numpad4,
        ),
        (
            KeyCode::ArrowRight,
            0xe04d,
            WinitKeyCode::Numpad6,
            KeyCode::Numpad6,
        ),
    ] {
        let code = physical_key(key).unwrap();
        assert_eq!(code.to_scancode(), Some(scan));
        let physical = PhysicalKey::from_scancode(scan);
        assert_eq!(physical, PhysicalKey::Code(code));
        assert_eq!(convert_physical_key(physical), Some(key));
        let unextended = PhysicalKey::from_scancode(scan & 0xff);
        assert_eq!(unextended, PhysicalKey::Code(numpad_code));
        assert_eq!(convert_physical_key(unextended), Some(numpad_key));
        assert_eq!(physical_key(numpad_key), None);
    }
}

#[derive(Debug, Default, Resource)]
struct Movement {
    held: bool,
    position: u64,
    starts: u64,
    stops: u64,
}

const MOVE_RIGHT: &str = "test.keyboard-d";

fn bindings() -> Vec<KeyboardActionBinding> {
    ['a', 'd', 'w', 's']
        .into_iter()
        .map(|key| KeyboardActionBinding {
            key: KeyCode::Character(key),
            action: ActionId::new(format!("test.keyboard-{key}")).unwrap(),
            context: ActionContext::gameplay(),
        })
        .collect()
}

fn holds() -> Vec<KeyboardHold> {
    bindings()
        .into_iter()
        .map(|binding| KeyboardHold {
            held: binding.key == KeyCode::Character('d'),
            binding,
        })
        .collect()
}

fn keyboard_session(isolated: bool, paused: bool) -> WinitSession {
    let mut app = window_app();
    app.insert_resource(RuntimeTimeSettings::default().with_paused(paused))
        .unwrap();
    app.add_plugin(InputPlugin).unwrap();
    app.insert_resource(ActionMaps::new(
        ActionMap::from_bindings(
            bindings()
                .into_iter()
                .map(|binding| ActionBinding::key(binding.action, binding.key)),
        )
        .unwrap(),
    ))
    .unwrap();
    app.init_resource::<Movement>().unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        (|actions: Res<ActionState>,
          time: Res<RealTime>,
          mut observed_frame: Local<Option<u64>>,
          mut movement: ResMut<Movement>| {
            if *observed_frame != Some(time.frame) {
                *observed_frame = Some(time.frame);
                for edge in actions.as_slice().iter().filter(|edge| {
                    edge.seat == InputSeatId::PRIMARY && edge.action.as_str() == MOVE_RIGHT
                }) {
                    match edge.phase {
                        ActionPhase::Started => movement.starts += 1,
                        ActionPhase::Released => movement.stops += 1,
                        ActionPhase::Changed => {}
                    }
                }
            }
            movement.held = actions.button(MOVE_RIGHT);
            if movement.held {
                movement.position += 1;
            }
        })
        .in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    let mut state = platform_app(start_runtime(app));
    state.configure_input_isolation(isolated);
    state.connect_window_input(WindowId::PRIMARY).unwrap();
    WinitSession {
        app: Some(state),
        event_loop: None,
        executed: false,
    }
}

fn poll(session: &mut WinitSession) -> WinitRunProgress {
    session.poll_with(|app, event_loop, _| {
        assert!(event_loop.is_none());
        app.service_boundary_transition(Instant::now());
        false
    })
}

fn close(mut session: WinitSession) {
    session.request_close();
    assert_eq!(session.execute(), WinitRunProgress::Retired);
    assert!(session.into_retired().unwrap().result.is_ok());
}

fn restored_session() -> WinitSession {
    let mut session = keyboard_session(true, true);
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    session.restore_keyboard_holds(holds()).unwrap();
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.capture_keyboard_holds(&bindings()).unwrap(),
        holds()
    );
    session
}

/// Seeds Active for CPU dispatch without a native window or an OS focus acknowledgement.
fn cpu_active_keyboard_session() -> WinitSession {
    let mut session = keyboard_session(true, true);
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    let neutral = bindings()
        .into_iter()
        .map(|binding| KeyboardHold {
            binding,
            held: false,
        })
        .collect();
    session.restore_keyboard_holds(neutral).unwrap();
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    let state = session.app.as_mut().unwrap();
    assert!(state.platform_windows.is_empty());
    state.input_gate.gain_focus();
    state.keyboard_continuation.activation = WinitInputActivation::Active;
    assert_eq!(session.runtime().state(), RuntimeState::Paused);
    session
}

#[test]
fn restoration_requires_isolation_and_a_full_paused_boundary_and_refusal_preserves_it() {
    let mut session = keyboard_session(false, true);
    assert_eq!(
        session.restore_keyboard_holds(holds()),
        Err(WinitKeyboardContinuationError::IncompletePausedFrame)
    );
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.restore_keyboard_holds(holds()),
        Err(WinitKeyboardContinuationError::NotIsolated)
    );
    assert!(session.paused_frame_complete());
    session
        .app
        .as_mut()
        .unwrap()
        .configure_input_isolation(true);
    assert_eq!(
        session.restore_keyboard_holds(Vec::new()),
        Err(WinitKeyboardContinuationError::Input(
            KeyboardContinuationError::InvalidCount
        ))
    );
    let limit = session
        .runtime()
        .world()
        .resource::<InputTimeline>()
        .limits()
        .members();
    assert_eq!(
        session.restore_keyboard_holds(vec![holds()[0].clone(); limit + 1]),
        Err(WinitKeyboardContinuationError::Input(
            KeyboardContinuationError::InvalidCount
        ))
    );
    assert!(session.paused_frame_complete());
    let mut invalid = holds();
    invalid[1].binding.action = ActionId::new("test.wrong-action").unwrap();
    assert_eq!(
        session.restore_keyboard_holds(invalid),
        Err(WinitKeyboardContinuationError::Input(
            KeyboardContinuationError::BindingMismatch
        ))
    );
    assert!(session.paused_frame_complete());
    let admission = session.restore_keyboard_holds(holds()).unwrap();
    assert_eq!(admission.admitted(), 1);
    assert!(!session.paused_frame_complete());
    assert_eq!(
        session.capture_keyboard_holds(&bindings()),
        Err(WinitKeyboardContinuationError::IncompletePausedFrame)
    );
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.capture_keyboard_holds(&bindings()).unwrap(),
        holds()
    );
    assert_eq!(session.runtime().world().resource::<Movement>().stops, 0);
    assert!(
        session
            .runtime()
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::Character('d'))
    );
    assert!(
        session
            .runtime()
            .world()
            .resource::<SemanticActions>()
            .pressed(
                InputSeatId::PRIMARY,
                &bindings()[1].action,
                &ActionContext::gameplay(),
            )
    );
    assert_eq!(
        session.restore_keyboard_holds(holds()),
        Err(WinitKeyboardContinuationError::Input(
            KeyboardContinuationError::MembersPresent
        ))
    );
    assert!(session.paused_frame_complete());
    close(session);

    let mut running = keyboard_session(true, false);
    assert_eq!(poll(&mut running), WinitRunProgress::Running);
    assert_eq!(
        running.restore_keyboard_holds(holds()),
        Err(WinitKeyboardContinuationError::NotPaused)
    );
    close(running);
}

#[test]
fn dormant_focus_loss_and_native_edges_preserve_restored_intent_without_simulating() {
    let mut session = restored_session();
    let generation = session.runtime().generation();
    let state = session.app.as_mut().unwrap();
    state
        .apply_focus_driver_event(WindowId::PRIMARY, false)
        .unwrap();
    for (phase, synthetic) in [
        (ElementState::Pressed, false),
        (ElementState::Pressed, true),
        (ElementState::Released, false),
    ] {
        assert!(
            !state
                .apply_keyboard_event_with_origin(
                    raw_key_event(WinitKeyCode::KeyD, phase),
                    false,
                    synthetic
                )
                .unwrap()
        );
    }
    assert_eq!(
        state.mouse_driver_input(ElementState::Pressed, MouseButton::Left),
        None
    );
    assert!(!state.ordinary_input_enabled());
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(session.input_activation(), WinitInputActivation::Isolated);
    assert_eq!(
        session.capture_keyboard_holds(&bindings()).unwrap(),
        holds()
    );
    assert_eq!(session.runtime().generation(), generation);
    assert_eq!(session.runtime().world().resource::<Movement>().stops, 0);
    assert!(matches!(
        session.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Rejected(_)
    ));
    close(session);
}

#[test]
fn synthetic_down_cannot_create_a_member_in_an_active_original_session() {
    let mut session = keyboard_session(false, true);
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert!(
        !session
            .app
            .as_mut()
            .unwrap()
            .apply_keyboard_event_with_origin(
                raw_key_event(WinitKeyCode::KeyD, ElementState::Pressed),
                false,
                true,
            )
            .unwrap()
    );
    assert!(!session.capture_keyboard_holds(&bindings()).unwrap()[1].held);
    assert!(
        session
            .app
            .as_mut()
            .unwrap()
            .apply_physical_keyboard_driver_event(
                raw_key_event(WinitKeyCode::KeyD, ElementState::Pressed),
                false,
            )
            .unwrap()
    );
    assert_eq!(
        session.capture_keyboard_holds(&bindings()),
        Err(WinitKeyboardContinuationError::IncompletePausedFrame)
    );
    close(session);
}

#[test]
fn missing_native_owner_or_focus_never_activates_a_restored_candidate() {
    let mut session = restored_session();
    assert_eq!(
        session.request_input_activation(),
        Err(WinitKeyboardContinuationError::Unavailable)
    );
    assert_eq!(session.input_activation(), WinitInputActivation::Isolated);
    let state = session.app.as_mut().unwrap();
    state.keyboard_continuation.activation = WinitInputActivation::ReconciliationPending;
    state.finish_keyboard_activation().unwrap();
    assert_eq!(
        session.input_activation(),
        WinitInputActivation::AwaitingFocus
    );
    assert!(matches!(
        session.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Rejected(_)
    ));
    assert!(matches!(
        session.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Rejected(_)
    ));
    let state = session.app.as_mut().unwrap();
    state.input_gate.gain_focus();
    state.acknowledge_keyboard_focus().unwrap();
    assert_eq!(
        session.input_activation(),
        WinitInputActivation::Unavailable
    );
    assert_eq!(
        session.input_activation_error(),
        Some(&WinitKeyboardContinuationError::Unavailable)
    );
    assert_eq!(session.runtime().state(), RuntimeState::Paused);
    assert_eq!(
        session.capture_keyboard_holds(&bindings()).unwrap(),
        holds()
    );
    close(session);
}

#[test]
fn mismatch_stays_isolated_until_one_original_step_consumes_stop_before_movement() {
    let mut session = restored_session();
    let generation = session.runtime().generation();
    session
        .app
        .as_mut()
        .unwrap()
        .reconcile_keyboard_activation(Vec::new())
        .unwrap();
    assert_eq!(
        session.input_activation(),
        WinitInputActivation::ReconciliationPending
    );
    let timeline = session.runtime().world().resource::<InputTimeline>();
    let release = timeline
        .entries()
        .find(|entry| {
            matches!(
                entry.kind(),
                InputEventKind::Key {
                    key: KeyCode::Character('d'),
                    phase: nara_input::ButtonTransitionPhase::Released,
                }
            )
        })
        .unwrap();
    assert_eq!(release.origin(), InputEventOrigin::Platform);
    assert_eq!(release.member_origin(), Some(InputEventOrigin::Restored));
    assert_eq!(
        release.invalidation(),
        Some(InputInvalidationReason::ActivationMismatch)
    );
    assert!(matches!(
        session.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Rejected(_)
    ));
    let RuntimeControlRequestResult::Accepted(step) =
        session.request_control(RuntimeControl::StepFixedTick)
    else {
        panic!("reconciliation must admit one explicit Step");
    };
    assert!(matches!(
        session.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Rejected(_)
    ));
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.control_status(step),
        Some(RuntimeControlStatus::Applied)
    );
    let movement = session.runtime().world().resource::<Movement>();
    assert!(!movement.held);
    assert_eq!(movement.position, 0);
    assert_eq!(movement.stops, 1);
    assert_eq!(session.runtime().generation(), generation);
    assert_eq!(session.runtime().state(), RuntimeState::Paused);
    assert_eq!(
        session.input_activation(),
        WinitInputActivation::ReconciliationPending
    );
    assert!(matches!(
        session.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Rejected(_)
    ));
    close(session);
}

#[test]
fn cpu_active_paused_refocus_restores_original_endpoint_before_a_real_press() {
    let mut session = cpu_active_keyboard_session();
    let generation = session.runtime().generation();
    session
        .app
        .as_mut()
        .unwrap()
        .apply_focus_driver_event(WindowId::PRIMARY, false)
        .unwrap();
    assert_eq!(
        session.input_activation(),
        WinitInputActivation::AwaitingFocus
    );
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert!(
        session
            .capture_keyboard_holds(&bindings())
            .unwrap()
            .iter()
            .all(|hold| !hold.held)
    );
    let state = session.app.as_mut().unwrap();
    state
        .apply_focus_driver_event(WindowId::PRIMARY, true)
        .unwrap();
    state.reconcile_keyboard_activation(Vec::new()).unwrap();
    assert!(state.input_gate.focused);
    assert!(
        state
            .runtime
            .world()
            .resource::<InputTimeline>()
            .entries()
            .any(|entry| matches!(entry.kind(), InputEventKind::FocusChanged { focused: true })),
        "CPU focus acknowledgement must reach the original Input endpoint"
    );
    state.keyboard_continuation.activation = WinitInputActivation::Active;
    assert!(
        state
            .apply_physical_keyboard_driver_event(
                raw_key_event(WinitKeyCode::KeyD, ElementState::Pressed),
                false,
            )
            .unwrap()
    );
    let timeline = session.runtime().world().resource::<InputTimeline>();
    assert!(timeline.entries().any(|entry| {
        matches!(
            entry.kind(),
            InputEventKind::Key {
                key: KeyCode::Character('d'),
                phase: nara_input::ButtonTransitionPhase::Pressed,
            }
        ) && entry.origin() == InputEventOrigin::Platform
            && entry.member_origin() == Some(InputEventOrigin::Platform)
    }));
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert!(
        session
            .runtime()
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::Character('d'))
    );
    assert_eq!(session.runtime().generation(), generation);
    assert_eq!(session.runtime().state(), RuntimeState::Paused);
    close(session);
}

#[test]
fn cpu_active_paused_refocus_settles_platform_release_through_one_original_step() {
    let mut session = cpu_active_keyboard_session();
    let generation = session.runtime().generation();
    assert!(
        session
            .app
            .as_mut()
            .unwrap()
            .apply_physical_keyboard_driver_event(
                raw_key_event(WinitKeyCode::KeyD, ElementState::Pressed),
                false,
            )
            .unwrap()
    );
    let RuntimeControlRequestResult::Accepted(started) =
        session.request_control(RuntimeControl::StepFixedTick)
    else {
        panic!("the Active CPU fixture must admit the original Press step");
    };
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.control_status(started),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert!(session.capture_keyboard_holds(&bindings()).unwrap()[1].held);
    let position = session.runtime().world().resource::<Movement>().position;
    assert_eq!(position, 1);
    session
        .app
        .as_mut()
        .unwrap()
        .apply_focus_driver_event(WindowId::PRIMARY, false)
        .unwrap();
    assert_eq!(
        session.input_activation(),
        WinitInputActivation::AwaitingFocus
    );
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.capture_keyboard_holds(&bindings()),
        Err(WinitKeyboardContinuationError::Input(
            KeyboardContinuationError::PendingInput
        ))
    );
    let timeline = session.runtime().world().resource::<InputTimeline>();
    let release = timeline
        .entries()
        .find(|entry| {
            matches!(
                entry.kind(),
                InputEventKind::Key {
                    key: KeyCode::Character('d'),
                    phase: nara_input::ButtonTransitionPhase::Released,
                }
            )
        })
        .unwrap();
    assert_eq!(release.origin(), InputEventOrigin::Platform);
    assert_eq!(release.member_origin(), Some(InputEventOrigin::Platform));
    assert_eq!(
        release.invalidation(),
        Some(InputInvalidationReason::FocusLost)
    );
    let state = session.app.as_mut().unwrap();
    state
        .apply_focus_driver_event(WindowId::PRIMARY, true)
        .unwrap();
    state.reconcile_keyboard_activation(Vec::new()).unwrap();
    assert_eq!(
        session.input_activation(),
        WinitInputActivation::ReconciliationPending
    );
    assert!(matches!(
        session.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Rejected(_)
    ));
    let RuntimeControlRequestResult::Accepted(step) =
        session.request_control(RuntimeControl::StepFixedTick)
    else {
        panic!("a retained Platform Release must admit its original reconciliation Step");
    };
    assert!(matches!(
        session.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Rejected(_)
    ));
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.control_status(step),
        Some(RuntimeControlStatus::Applied)
    );
    let movement = session.runtime().world().resource::<Movement>();
    assert!(!movement.held);
    assert_eq!(movement.position, position);
    assert_eq!(movement.stops, 1);
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert!(session.paused_frame_complete());
    assert!(
        session
            .capture_keyboard_holds(&bindings())
            .unwrap()
            .iter()
            .all(|hold| !hold.held)
    );
    assert_eq!(session.runtime().world().resource::<Movement>().stops, 1);
    assert_eq!(session.runtime().generation(), generation);
    assert_eq!(session.runtime().state(), RuntimeState::Paused);
    close(session);
}

#[test]
fn activation_snapshot_suppresses_unowned_down_keys_until_physical_release() {
    let mut session = cpu_active_keyboard_session();
    let state = session.app.as_mut().unwrap();
    let previously_held = KeyCode::Character('d');
    let pressed_while_away = KeyCode::Character('w');
    state.input_gate.lose_focus([previously_held], []);
    state.input_gate.gain_focus();
    state
        .reconcile_keyboard_activation(vec![previously_held, pressed_while_away])
        .unwrap();
    assert_eq!(
        state.keyboard_continuation.activation,
        WinitInputActivation::ReconciliationPending
    );
    state.keyboard_continuation.activation = WinitInputActivation::Active;

    for key in [WinitKeyCode::KeyD, WinitKeyCode::KeyW] {
        assert!(
            !state
                .apply_keyboard_event_with_origin(
                    raw_key_event(key, ElementState::Released),
                    false,
                    true,
                )
                .unwrap()
        );
        assert!(
            !state
                .apply_physical_keyboard_driver_event(
                    raw_key_event(key, ElementState::Pressed),
                    false,
                )
                .unwrap()
        );
        for edge in [ElementState::Released, ElementState::Pressed] {
            assert!(
                state
                    .apply_physical_keyboard_driver_event(raw_key_event(key, edge), false)
                    .unwrap()
            );
        }
    }
    close(session);
}

#[test]
fn keyboard_admission_and_rejection_survive_the_same_post_operation_fault() {
    for reject in [false, true] {
        let mut session = keyboard_session(true, true);
        assert_eq!(poll(&mut session), WinitRunProgress::Running);
        let claim = session
            .app
            .as_ref()
            .unwrap()
            .keyboard_continuation
            .claim
            .clone()
            .unwrap();
        let reporter = session.runtime().fault_reporter();
        let fault = RuntimeFault::engine(
            RuntimeFaultKind::RequiredService,
            "test.keyboard-post-fault",
        );
        let result = session
            .app
            .as_mut()
            .unwrap()
            .apply_keyboard_operation(|scope| {
                let result = nara_input::restore_keyboard_holds(
                    scope,
                    claim,
                    if reject { Vec::new() } else { holds() },
                );
                reporter.report(fault.clone());
                result
            });
        let Err(WinitKeyboardContinuationError::Faulted {
            admission,
            rejection,
            fault: observed,
        }) = result
        else {
            panic!("the complete keyboard output must survive its post-fault");
        };
        assert_eq!(observed, fault);
        if reject {
            assert_eq!(admission, None);
            assert_eq!(rejection, Some(KeyboardContinuationError::InvalidCount));
        } else {
            assert_eq!(admission.unwrap().admitted(), 1);
            assert_eq!(rejection, None);
            assert!(
                session
                    .runtime()
                    .world()
                    .resource::<InputTimeline>()
                    .entries()
                    .any(|entry| {
                        matches!(
                            entry.kind(),
                            InputEventKind::KeyRestored {
                                key: KeyCode::Character('d')
                            }
                        )
                    })
            );
        }
        assert!(!session.paused_frame_complete());
        assert!(
            matches!(session.capture_keyboard_holds(&bindings()), Err(WinitKeyboardContinuationError::Faulted {
            admission: None, rejection: None, fault: observed,
        }) if observed == fault)
        );
        session.request_close();
        assert_eq!(session.execute(), WinitRunProgress::Retired);
        assert_eq!(
            session.into_retired().unwrap().runtime.fault(),
            Some(&fault)
        );
    }
}

#[test]
fn explicit_paused_maintenance_uses_the_original_full_frame_without_fixed_simulation() {
    let mut session = keyboard_session(true, true);
    assert!(!session.paused_frame_complete());
    let initial_frame = session.runtime().world().resource::<RealTime>().frame;
    let initial_tick = session
        .runtime()
        .world()
        .resource::<nara_app::FixedTime>()
        .tick();
    session.request_paused_maintenance().unwrap();
    session.request_paused_maintenance().unwrap();
    assert!(!session.paused_frame_complete());
    assert_eq!(
        session.runtime().world().resource::<RealTime>().frame,
        initial_frame
    );
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert!(session.paused_frame_complete());
    assert_eq!(
        session.runtime().world().resource::<RealTime>().frame,
        initial_frame + 1
    );
    assert_eq!(
        session
            .runtime()
            .world()
            .resource::<nara_app::FixedTime>()
            .tick(),
        initial_tick
    );
    let phase = session.capture_time_continuation().unwrap();
    let fixed = *session.runtime().world().resource::<nara_app::FixedTime>();
    let frame = session.runtime().world().resource::<RealTime>().frame;
    session.request_paused_maintenance().unwrap();
    assert!(!session.paused_frame_complete());
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert!(session.paused_frame_complete());
    assert_eq!(session.capture_time_continuation().unwrap(), phase);
    assert_eq!(
        *session.runtime().world().resource::<nara_app::FixedTime>(),
        fixed
    );
    assert_eq!(
        session.runtime().world().resource::<RealTime>().frame,
        frame + 1
    );
    assert_eq!(session.runtime().world().resource::<Movement>().position, 0);
    close(session);
}

#[test]
fn isolated_synthetic_press_and_release_share_the_original_mapping_and_one_tick() {
    let mut session = cpu_active_keyboard_session();
    let generation = session.runtime().generation();
    let key = KeyCode::Character('d');
    assert_eq!(
        session.admit_isolated_keyboard_input(ButtonDriverInput::Press(key)),
        Err(WinitKeyboardContinuationError::NotIsolated)
    );
    assert!(session.paused_frame_complete());
    let neutral = session.isolate_keyboard_input(&bindings()).unwrap();
    assert!(neutral.iter().all(|hold| !hold.held));
    assert!(!session.paused_frame_complete());
    let press = session
        .admit_isolated_keyboard_input(ButtonDriverInput::Press(key))
        .unwrap();
    assert_eq!(press.admitted(), 1);
    assert!(
        session
            .admit_isolated_keyboard_input(ButtonDriverInput::Press(key))
            .unwrap()
            .is_ignored()
    );
    assert_eq!(
        session.admit_isolated_keyboard_input(ButtonDriverInput::Press(KeyCode::Space)),
        Err(WinitKeyboardContinuationError::Input(
            KeyboardContinuationError::BindingMismatch
        ))
    );
    let release = session
        .admit_isolated_keyboard_input(ButtonDriverInput::Release(key))
        .unwrap();
    assert_eq!(release.admitted(), 1);
    assert_eq!(
        release.first_sequence(),
        press.last_sequence().map(|sequence| sequence + 1)
    );
    let entries = session
        .runtime()
        .world()
        .resource::<InputTimeline>()
        .entries()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 2);
    for entry in &entries {
        assert_eq!(entry.origin(), InputEventOrigin::Synthetic);
        assert_eq!(entry.member_origin(), Some(InputEventOrigin::Synthetic));
        assert_eq!(entry.press_sequence(), press.first_sequence());
        assert_eq!(
            entry.route().window(),
            InputWindowId::new(WindowId::PRIMARY.get())
        );
        assert_eq!(entry.route().seat(), InputSeatId::PRIMARY);
    }
    assert_eq!(
        session.capture_keyboard_holds(&bindings()),
        Err(WinitKeyboardContinuationError::IncompletePausedFrame)
    );
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.capture_keyboard_holds(&bindings()),
        Err(WinitKeyboardContinuationError::Input(
            KeyboardContinuationError::PendingInput
        ))
    );
    let RuntimeControlRequestResult::Accepted(step) =
        session.request_control(RuntimeControl::StepFixedTick)
    else {
        panic!("the controlled batch must use the original exact-tick action consumer");
    };
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.control_status(step),
        Some(RuntimeControlStatus::Applied)
    );
    let movement = session.runtime().world().resource::<Movement>();
    assert!(!movement.held);
    assert_eq!(movement.position, 0);
    assert_eq!(movement.starts, 1);
    assert_eq!(movement.stops, 1);
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.capture_keyboard_holds(&bindings()).unwrap(),
        neutral
    );
    assert!(matches!(
        session.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Accepted(_)
    ));
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(session.runtime().world().resource::<Movement>().stops, 1);
    assert_eq!(session.runtime().generation(), generation);
    assert_eq!(session.input_activation(), WinitInputActivation::Isolated);
    close(session);
}

#[test]
fn handoff_isolation_preserves_platform_intent_and_closes_ordinary_ingress_before_capture() {
    let mut session = cpu_active_keyboard_session();
    let generation = session.runtime().generation();
    assert!(
        session
            .app
            .as_mut()
            .unwrap()
            .apply_physical_keyboard_driver_event(
                raw_key_event(WinitKeyCode::KeyD, ElementState::Pressed),
                false,
            )
            .unwrap()
    );
    assert!(matches!(
        session.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Accepted(_)
    ));
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    let expected = session.capture_keyboard_holds(&bindings()).unwrap();
    assert_eq!(expected, holds());
    let position = session.runtime().world().resource::<Movement>().position;
    assert_eq!(position, 1);
    assert_eq!(
        session.isolate_keyboard_input(&[]),
        Err(WinitKeyboardContinuationError::Input(
            KeyboardContinuationError::InvalidCount
        ))
    );
    assert_eq!(session.input_activation(), WinitInputActivation::Active);
    assert!(session.paused_frame_complete());
    let mut incorrect = bindings();
    incorrect[1].action = ActionId::new("test.other-handoff-action").unwrap();
    assert_eq!(
        session.isolate_keyboard_input(&incorrect),
        Err(WinitKeyboardContinuationError::Input(
            KeyboardContinuationError::BindingMismatch
        ))
    );
    assert_eq!(session.input_activation(), WinitInputActivation::Active);
    let focused = session.app.as_ref().unwrap().input_gate.focused;
    let isolated = session.isolate_keyboard_input(&bindings()).unwrap();
    assert_eq!(isolated, expected);
    assert!(focused);
    assert_eq!(session.app.as_ref().unwrap().input_gate.focused, focused);
    assert_eq!(session.input_activation(), WinitInputActivation::Isolated);
    assert!(!session.paused_frame_complete());
    assert!(
        session
            .runtime()
            .world()
            .resource::<InputTimeline>()
            .entries()
            .next()
            .is_none()
    );
    let state = session.app.as_mut().unwrap();
    for edge in [ElementState::Released, ElementState::Pressed] {
        assert!(
            !state
                .apply_physical_keyboard_driver_event(
                    raw_key_event(WinitKeyCode::KeyD, edge),
                    false
                )
                .unwrap()
        );
    }
    assert_eq!(
        state.mouse_driver_input(ElementState::Pressed, MouseButton::Left),
        None
    );
    assert_eq!(
        session.capture_keyboard_holds(&bindings()),
        Err(WinitKeyboardContinuationError::IncompletePausedFrame)
    );
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.capture_keyboard_holds(&bindings()).unwrap(),
        isolated
    );
    assert!(matches!(
        session.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Rejected(_)
    ));
    let state = session.app.as_mut().unwrap();
    state.reconcile_keyboard_activation(Vec::new()).unwrap();
    assert_eq!(
        state.keyboard_continuation.activation,
        WinitInputActivation::ReconciliationPending
    );
    let release = state
        .runtime
        .world()
        .resource::<InputTimeline>()
        .entries()
        .find(|entry| {
            matches!(
                entry.kind(),
                InputEventKind::Key {
                    key: KeyCode::Character('d'),
                    phase: nara_input::ButtonTransitionPhase::Released
                }
            )
        })
        .copied()
        .unwrap();
    assert_eq!(release.origin(), InputEventOrigin::Platform);
    assert_eq!(release.member_origin(), Some(InputEventOrigin::Platform));
    assert_eq!(
        release.invalidation(),
        Some(InputInvalidationReason::ActivationMismatch)
    );
    let RuntimeControlRequestResult::Accepted(step) =
        session.request_control(RuntimeControl::StepFixedTick)
    else {
        panic!("a missing physical key after handoff must use the original Stop step");
    };
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.control_status(step),
        Some(RuntimeControlStatus::Applied)
    );
    let movement = session.runtime().world().resource::<Movement>();
    assert!(!movement.held);
    assert_eq!(movement.position, position);
    assert_eq!(movement.stops, 1);
    assert_eq!(session.runtime().generation(), generation);
    close(session);
}

#[test]
fn controlled_keyboard_refuses_unconfigured_running_foreign_faulted_and_retiring_owners() {
    let mut unconfigured = keyboard_session(true, true);
    assert_eq!(
        unconfigured.isolate_keyboard_input(&bindings()),
        Err(WinitKeyboardContinuationError::IncompletePausedFrame)
    );
    assert_eq!(
        unconfigured.admit_isolated_keyboard_input(ButtonDriverInput::ReleaseAll),
        Err(WinitKeyboardContinuationError::Input(
            KeyboardContinuationError::BindingMismatch
        ))
    );
    close(unconfigured);

    let mut running = keyboard_session(true, false);
    assert_eq!(poll(&mut running), WinitRunProgress::Running);
    assert_eq!(
        running.request_paused_maintenance(),
        Err(WinitKeyboardContinuationError::NotPaused)
    );
    assert_eq!(
        running.isolate_keyboard_input(&bindings()),
        Err(WinitKeyboardContinuationError::NotPaused)
    );
    assert_eq!(
        running.admit_isolated_keyboard_input(ButtonDriverInput::Press(KeyCode::Character('d'))),
        Err(WinitKeyboardContinuationError::NotPaused)
    );
    close(running);

    let foreign = restored_session();
    let mut session = restored_session();
    let claim = session
        .app
        .as_ref()
        .unwrap()
        .keyboard_continuation
        .claim
        .clone();
    session.app.as_mut().unwrap().keyboard_continuation.claim = foreign
        .app
        .as_ref()
        .unwrap()
        .keyboard_continuation
        .claim
        .clone();
    assert_eq!(
        session.admit_isolated_keyboard_input(ButtonDriverInput::ReleaseAll),
        Err(WinitKeyboardContinuationError::Input(
            KeyboardContinuationError::Ingress(nara_input::InputIngressError::ForeignEndpointClaim)
        ))
    );
    assert!(session.paused_frame_complete());
    session.app.as_mut().unwrap().keyboard_continuation.claim = claim;
    assert_eq!(
        session.capture_keyboard_holds(&bindings()).unwrap(),
        holds()
    );
    session.request_close();
    assert_eq!(
        session.request_paused_maintenance(),
        Err(WinitKeyboardContinuationError::Unavailable)
    );
    assert_eq!(
        session.admit_isolated_keyboard_input(ButtonDriverInput::ReleaseAll),
        Err(WinitKeyboardContinuationError::Unavailable)
    );
    assert_eq!(
        session.isolate_keyboard_input(&bindings()),
        Err(WinitKeyboardContinuationError::Unavailable)
    );
    close(session);
    close(foreign);

    let mut faulted = restored_session();
    let fault = RuntimeFault::engine(
        RuntimeFaultKind::RequiredService,
        "test.claimed-keyboard-entry",
    );
    faulted.runtime().fault_reporter().report(fault.clone());
    assert_eq!(
        faulted.request_paused_maintenance(),
        Err(WinitKeyboardContinuationError::Faulted {
            admission: None,
            rejection: None,
            fault: fault.clone()
        })
    );
    assert_eq!(
        faulted.admit_isolated_keyboard_input(ButtonDriverInput::ReleaseAll),
        Err(WinitKeyboardContinuationError::Faulted {
            admission: None,
            rejection: None,
            fault: fault.clone()
        })
    );
    assert_eq!(
        faulted.isolate_keyboard_input(&bindings()),
        Err(WinitKeyboardContinuationError::Faulted {
            admission: None,
            rejection: None,
            fault: fault.clone()
        })
    );
    faulted.request_close();
    assert_eq!(faulted.execute(), WinitRunProgress::Retired);
    assert_eq!(
        faulted.into_retired().unwrap().runtime.fault(),
        Some(&fault)
    );
}

#[test]
fn controlled_claimed_keyboard_output_survives_the_original_post_operation_fault() {
    let foreign = restored_session();
    for reject in [false, true] {
        let mut session = restored_session();
        let claim = if reject {
            foreign
                .app
                .as_ref()
                .unwrap()
                .keyboard_continuation
                .claim
                .clone()
                .unwrap()
        } else {
            session
                .app
                .as_ref()
                .unwrap()
                .keyboard_continuation
                .claim
                .clone()
                .unwrap()
        };
        let reporter = session.runtime().fault_reporter();
        let fault = RuntimeFault::engine(
            RuntimeFaultKind::RequiredService,
            "test.claimed-keyboard-post-fault",
        );
        let result = session
            .app
            .as_mut()
            .unwrap()
            .apply_keyboard_operation(|scope| {
                let result = nara_input::apply_claimed_keyboard_driver_input(
                    scope,
                    claim,
                    ButtonDriverInput::Release(KeyCode::Character('d')),
                )
                .map_err(|error| match error {
                    nara_input::ButtonInputDriverError::WorldAccess(error) => error.into(),
                    nara_input::ButtonInputDriverError::Timeline(error) => error.into(),
                });
                reporter.report(fault.clone());
                result
            });
        let Err(WinitKeyboardContinuationError::Faulted {
            admission,
            rejection,
            fault: observed,
        }) = result
        else {
            panic!("the original operation output must survive its post-fault");
        };
        assert_eq!(observed, fault);
        if reject {
            assert_eq!(admission, None);
            assert_eq!(
                rejection,
                Some(KeyboardContinuationError::Ingress(
                    nara_input::InputIngressError::ForeignEndpointClaim
                ))
            );
        } else {
            assert_eq!(admission.unwrap().admitted(), 1);
            assert_eq!(rejection, None);
            let release = session
                .runtime()
                .world()
                .resource::<InputTimeline>()
                .entries()
                .next()
                .copied()
                .unwrap();
            assert_eq!(release.origin(), InputEventOrigin::Synthetic);
            assert_eq!(release.member_origin(), Some(InputEventOrigin::Restored));
        }
        assert!(!session.paused_frame_complete());
        session.request_close();
        assert_eq!(session.execute(), WinitRunProgress::Retired);
        assert_eq!(
            session.into_retired().unwrap().runtime.fault(),
            Some(&fault)
        );
    }
    close(foreign);
}

/// Exercises CPU admission without a native window or an OS focus acknowledgement.
#[test]
fn cpu_press_only_activation_refusal_preserves_isolation_and_original_step() {
    let mut session = keyboard_session(true, true);
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    let neutral = bindings()
        .into_iter()
        .map(|binding| KeyboardHold {
            binding,
            held: false,
        })
        .collect::<Vec<_>>();
    session.restore_keyboard_holds(neutral.clone()).unwrap();
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert!(session.event_loop.is_none());
    assert!(session.app.as_ref().unwrap().platform_windows.is_empty());
    assert!(!session.app.as_ref().unwrap().input_gate.focused);
    let generation = session.runtime().generation();
    let tick = session
        .runtime()
        .world()
        .resource::<nara_app::FixedTime>()
        .tick();
    let key = KeyCode::Character('d');
    let press = session
        .admit_isolated_keyboard_input(ButtonDriverInput::Press(key))
        .unwrap();
    assert_eq!(press.admitted(), 1);
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert!(session.paused_frame_complete());
    let pending = session
        .runtime()
        .world()
        .resource::<InputTimeline>()
        .entries()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(pending.len(), 1);
    assert_eq!(
        pending[0].kind(),
        InputEventKind::Key {
            key,
            phase: nara_input::ButtonTransitionPhase::Pressed,
        }
    );
    assert_eq!(pending[0].origin(), InputEventOrigin::Synthetic);
    assert_eq!(pending[0].press_sequence(), press.first_sequence());
    assert_eq!(
        session.request_input_activation(),
        Err(WinitKeyboardContinuationError::Input(
            KeyboardContinuationError::PendingInput
        ))
    );
    assert_eq!(session.input_activation(), WinitInputActivation::Isolated);
    assert!(session.input_activation_error().is_none());
    assert!(session.paused_frame_complete());
    assert_eq!(
        session
            .runtime()
            .world()
            .resource::<InputTimeline>()
            .entries()
            .copied()
            .collect::<Vec<_>>(),
        pending
    );
    let RuntimeControlRequestResult::Accepted(step) =
        session.request_control(RuntimeControl::StepFixedTick)
    else {
        panic!("an early activation refusal must leave the original Step available");
    };
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.control_status(step),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(
        session
            .runtime()
            .world()
            .resource::<nara_app::FixedTime>()
            .tick(),
        tick + 1
    );
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.capture_keyboard_holds(&bindings()).unwrap(),
        holds()
    );
    assert_eq!(
        session.request_input_activation(),
        Err(WinitKeyboardContinuationError::Unavailable)
    );
    assert_eq!(session.input_activation(), WinitInputActivation::Isolated);
    assert_eq!(
        session.capture_keyboard_holds(&bindings()).unwrap(),
        holds()
    );
    let release = session
        .admit_isolated_keyboard_input(ButtonDriverInput::Release(key))
        .unwrap();
    assert_eq!(release.admitted(), 1);
    let RuntimeControlRequestResult::Accepted(step) =
        session.request_control(RuntimeControl::StepFixedTick)
    else {
        panic!("the original isolated endpoint must remain usable after activation refusal");
    };
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.control_status(step),
        Some(RuntimeControlStatus::Applied)
    );
    assert_eq!(poll(&mut session), WinitRunProgress::Running);
    assert_eq!(
        session.capture_keyboard_holds(&bindings()).unwrap(),
        neutral
    );
    assert_eq!(session.runtime().world().resource::<Movement>().stops, 1);
    assert_eq!(session.runtime().generation(), generation);
    close(session);
}
