use std::time::Duration;

use nara_app::{App, RuntimeDriverScope, RuntimeInstance, RuntimeState, RuntimeTimeSettings};
use nara_core::ByteLimit;
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBuilder, ActionMapLimits,
    ActionMaps, ButtonDriverInput, ButtonInputDriverError, ButtonTransitionPhase, InputAdmission,
    InputEventKind, InputEventOrigin, InputIngressError, InputLimits, InputPlugin, InputSeatId,
    InputTimeline, InputTimelineEntry, InputWindowEvent, InputWindowId, KeyCode,
    KeyboardActionBinding, KeyboardHold, apply_claimed_keyboard_driver_input,
    apply_window_input_driver_event, capture_keyboard_holds, connect_input_endpoint,
    restore_keyboard_holds, timeline::InputEndpointClaim,
};

const WINDOW: InputWindowId = InputWindowId::new(41);
const SEAT: InputSeatId = InputSeatId::PRIMARY;
const KEYS: [KeyCode; 4] = [
    KeyCode::Character('a'),
    KeyCode::Character('d'),
    KeyCode::Character('w'),
    KeyCode::Character('s'),
];

fn bindings() -> [KeyboardActionBinding; 4] {
    std::array::from_fn(|index| KeyboardActionBinding {
        key: KEYS[index],
        action: ActionId::new(format!("claimed.key-{index}")).unwrap(),
        context: ActionContext::gameplay(),
    })
}

fn driver<T>(
    runtime: &mut RuntimeInstance,
    operation: impl FnOnce(&mut RuntimeDriverScope<'_>) -> T,
) -> T {
    let outcome = runtime.with_driver_scope(operation).unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output
}

fn maintain(runtime: &mut RuntimeInstance) {
    assert_eq!(
        runtime.drive(Duration::ZERO).unwrap().state(),
        RuntimeState::Paused
    );
    assert!(runtime.fault().is_none());
}

fn runtime(limits: InputLimits) -> (RuntimeInstance, InputEndpointClaim) {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    for binding in bindings() {
        builder
            .bind(ActionBinding::key(binding.action, binding.key))
            .unwrap();
    }
    let map = ActionMap::new(
        builder.compile().unwrap().into_bindings(),
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
    )
    .unwrap();
    let mut app = App::new();
    app.insert_resource(RuntimeTimeSettings::default().with_paused(true))
        .unwrap();
    app.insert_resource(InputTimeline::with_limits(limits).unwrap())
        .unwrap();
    app.insert_resource(ActionMaps::new(map)).unwrap();
    app.add_plugin(InputPlugin).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    maintain(&mut runtime);
    driver(&mut runtime, |scope| {
        connect_input_endpoint(scope, WINDOW, SEAT)
    })
    .unwrap();
    maintain(&mut runtime);
    let claim = runtime
        .world()
        .resource::<InputTimeline>()
        .claim_endpoint(WINDOW, SEAT)
        .unwrap();
    (runtime, claim)
}

fn admit(
    runtime: &mut RuntimeInstance,
    claim: &InputEndpointClaim,
    input: ButtonDriverInput<KeyCode>,
) -> Result<InputAdmission, ButtonInputDriverError> {
    driver(runtime, |scope| {
        apply_claimed_keyboard_driver_input(scope, claim.clone(), input)
    })
}

fn entries(runtime: &RuntimeInstance) -> Vec<InputTimelineEntry> {
    runtime
        .world()
        .resource::<InputTimeline>()
        .entries()
        .copied()
        .collect()
}

#[test]
fn nominal_claim_is_checked_before_empty_foreign_retired_or_recycled_input() {
    let limits = InputLimits::new(64, 2, 1, 8).unwrap();
    let (mut receiver, original) = runtime(limits);
    let (foreign_runtime, foreign) = runtime(limits);
    for input in [
        ButtonDriverInput::Press(KEYS[1]),
        ButtonDriverInput::Release(KEYS[1]),
        ButtonDriverInput::ReleaseAll,
    ] {
        assert_eq!(
            admit(&mut receiver, &foreign, input),
            Err(ButtonInputDriverError::Timeline(
                InputIngressError::ForeignEndpointClaim
            ))
        );
        assert!(entries(&receiver).is_empty());
    }
    assert!(
        admit(&mut receiver, &original, ButtonDriverInput::ReleaseAll)
            .unwrap()
            .is_ignored()
    );
    driver(&mut receiver, |scope| {
        apply_window_input_driver_event(scope, WINDOW, SEAT, InputWindowEvent::SurfaceRetired)
    })
    .unwrap();
    let before = entries(&receiver);
    assert!(matches!(
        admit(&mut receiver, &original, ButtonDriverInput::ReleaseAll),
        Err(ButtonInputDriverError::Timeline(
            InputIngressError::EndpointRetiring { .. }
                | InputIngressError::EndpointClaimExpired { .. }
        ))
    ));
    assert_eq!(entries(&receiver), before);
    maintain(&mut receiver);
    assert_eq!(
        admit(&mut receiver, &original, ButtonDriverInput::ReleaseAll),
        Err(ButtonInputDriverError::Timeline(
            InputIngressError::EndpointClaimExpired {
                window: WINDOW,
                seat: SEAT,
            }
        ))
    );
    driver(&mut receiver, |scope| {
        connect_input_endpoint(scope, WINDOW, SEAT)
    })
    .unwrap();
    maintain(&mut receiver);
    let replacement = receiver
        .world()
        .resource::<InputTimeline>()
        .claim_endpoint(WINDOW, SEAT)
        .unwrap();
    assert_ne!(original, replacement);
    for input in [
        ButtonDriverInput::Press(KEYS[1]),
        ButtonDriverInput::ReleaseAll,
    ] {
        assert_eq!(
            admit(&mut receiver, &original, input),
            Err(ButtonInputDriverError::Timeline(
                InputIngressError::EndpointClaimExpired {
                    window: WINDOW,
                    seat: SEAT,
                }
            ))
        );
        assert!(entries(&receiver).is_empty());
    }
    let admitted = admit(
        &mut receiver,
        &replacement,
        ButtonDriverInput::Press(KEYS[1]),
    )
    .unwrap();
    assert_eq!(admitted.admitted(), 1);
    assert_eq!(entries(&receiver)[0].route().window(), WINDOW);
    assert!(entries(&foreign_runtime).is_empty());
    receiver.finish().unwrap();
    foreign_runtime.finish().unwrap();
}

#[test]
fn claimed_edges_preserve_exact_endpoint_and_restored_member_origin() {
    let (mut runtime, claim) = runtime(InputLimits::new(64, 3, 1, 8).unwrap());
    let restored = driver(&mut runtime, |scope| {
        restore_keyboard_holds(
            scope,
            claim.clone(),
            bindings()
                .into_iter()
                .map(|binding| KeyboardHold {
                    held: binding.key == KEYS[1],
                    binding,
                })
                .collect(),
        )
    })
    .unwrap();
    maintain(&mut runtime);
    let other_window = InputWindowId::new(42);
    driver(&mut runtime, |scope| {
        connect_input_endpoint(scope, other_window, SEAT)
    })
    .unwrap();
    let other = runtime
        .world()
        .resource::<InputTimeline>()
        .claim_endpoint(other_window, SEAT)
        .unwrap();
    let other_press = admit(&mut runtime, &other, ButtonDriverInput::Press(KEYS[1])).unwrap();
    let press = admit(&mut runtime, &claim, ButtonDriverInput::Press(KEYS[0])).unwrap();
    let release = admit(&mut runtime, &claim, ButtonDriverInput::Release(KEYS[1])).unwrap();
    let all = admit(&mut runtime, &claim, ButtonDriverInput::ReleaseAll).unwrap();
    assert_eq!(press.admitted(), 1);
    assert_eq!(release.admitted(), 1);
    assert_eq!(all.admitted(), 1);
    let observed = entries(&runtime);
    assert_eq!(observed.len(), 4);
    assert_eq!(observed[0].press_sequence(), other_press.first_sequence());
    for entry in &observed[1..] {
        assert_eq!(entry.route().window(), WINDOW);
        assert_eq!(entry.route().seat(), SEAT);
        assert_eq!(entry.origin(), InputEventOrigin::Synthetic);
    }
    assert_eq!(
        observed[1].member_origin(),
        Some(InputEventOrigin::Synthetic)
    );
    assert_eq!(
        observed[2].member_origin(),
        Some(InputEventOrigin::Restored)
    );
    assert_eq!(observed[2].press_sequence(), restored.first_sequence());
    assert_eq!(
        observed[3].member_origin(),
        Some(InputEventOrigin::Synthetic)
    );
    assert_eq!(observed[3].press_sequence(), press.first_sequence());
    assert_eq!(observed[2].sequence(), release.first_sequence().unwrap());
    assert_eq!(observed[3].sequence(), all.first_sequence().unwrap());
    assert!(
        admit(&mut runtime, &claim, ButtonDriverInput::ReleaseAll)
            .unwrap()
            .is_ignored()
    );
    let other_release = admit(&mut runtime, &other, ButtonDriverInput::ReleaseAll).unwrap();
    assert_eq!(other_release.admitted(), 1);
    let last = *entries(&runtime).last().unwrap();
    assert_eq!(last.route().window(), other_window);
    assert_eq!(last.press_sequence(), other_press.first_sequence());
    runtime.finish().unwrap();
}

#[test]
fn synthetic_terminal_records_keep_reserved_capacity_after_original_press_refusal() {
    let (mut runtime, claim) = runtime(InputLimits::new(16, 2, 1, 8).unwrap());
    let restored = driver(&mut runtime, |scope| {
        restore_keyboard_holds(
            scope,
            claim.clone(),
            bindings()
                .into_iter()
                .map(|binding| KeyboardHold {
                    binding,
                    held: true,
                })
                .collect(),
        )
    })
    .unwrap();
    maintain(&mut runtime);
    assert!(
        capture_keyboard_holds(runtime.world(), WINDOW, SEAT, &bindings())
            .unwrap()
            .iter()
            .all(|hold| hold.held)
    );
    for key in [KeyCode::Space, KeyCode::Enter, KeyCode::Escape] {
        assert_eq!(
            admit(&mut runtime, &claim, ButtonDriverInput::Press(key))
                .unwrap()
                .admitted(),
            1
        );
    }
    let before = entries(&runtime);
    assert_eq!(before.len(), 3);
    assert!(matches!(
        admit(
            &mut runtime,
            &claim,
            ButtonDriverInput::Press(KeyCode::ArrowUp)
        ),
        Err(ButtonInputDriverError::Timeline(
            InputIngressError::CapacityExceeded { limit: 16, .. }
        ))
    ));
    assert_eq!(entries(&runtime), before);
    let release = admit(&mut runtime, &claim, ButtonDriverInput::ReleaseAll).unwrap();
    assert_eq!(release.admitted(), 7);
    let observed = entries(&runtime);
    assert_eq!(observed.len(), 10);
    for (index, entry) in observed[3..].iter().enumerate() {
        assert_eq!(entry.origin(), InputEventOrigin::Synthetic);
        assert_eq!(entry.route().window(), WINDOW);
        assert!(matches!(
            entry.kind(),
            InputEventKind::Key {
                phase: ButtonTransitionPhase::Released,
                ..
            }
        ));
        if index < KEYS.len() {
            assert_eq!(entry.member_origin(), Some(InputEventOrigin::Restored));
            assert_eq!(
                entry.press_sequence(),
                Some(restored.first_sequence().unwrap() + index as u64)
            );
        } else {
            assert_eq!(entry.member_origin(), Some(InputEventOrigin::Synthetic));
            assert_eq!(
                entry.press_sequence(),
                Some(before[index - KEYS.len()].sequence())
            );
        }
    }
    assert!(
        admit(&mut runtime, &claim, ButtonDriverInput::ReleaseAll)
            .unwrap()
            .is_ignored()
    );
    assert_eq!(entries(&runtime), observed);
    runtime.finish().unwrap();
}

/// Uses the original Input driver with a platform focus event, without claiming native focus.
#[test]
fn platform_focus_loss_of_claimed_synthetic_hold_keeps_lineage_and_settles_once() {
    use nara_app::{RuntimeControl, RuntimeControlRequestResult, RuntimeControlStatus};

    let (mut runtime, claim) = runtime(InputLimits::new(64, 2, 1, 8).unwrap());
    let key = KEYS[1];
    let press = admit(&mut runtime, &claim, ButtonDriverInput::Press(key)).unwrap();
    assert_eq!(press.admitted(), 1);
    let original = entries(&runtime)[0];
    let RuntimeControlRequestResult::Accepted(step) =
        runtime.request_control(RuntimeControl::StepFixedTick)
    else {
        panic!("the original exact tick must consume the synthetic Press");
    };
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.control_status(step),
        Some(RuntimeControlStatus::Applied)
    );
    maintain(&mut runtime);
    let held = bindings().map(|binding| KeyboardHold {
        held: binding.key == key,
        binding,
    });
    assert_eq!(
        capture_keyboard_holds(runtime.world(), WINDOW, SEAT, &bindings()).unwrap(),
        held
    );
    assert!(entries(&runtime).is_empty());
    let (admission, released_keys, released_mouse) = driver(&mut runtime, |scope| {
        apply_window_input_driver_event(scope, WINDOW, SEAT, InputWindowEvent::Focused(false))
    })
    .unwrap();
    assert_eq!(admission.admitted(), 2);
    assert_eq!(released_keys, vec![key]);
    assert!(released_mouse.is_empty());
    let pending = entries(&runtime);
    let releases = pending
        .iter()
        .filter(|entry| {
            entry.kind()
                == InputEventKind::Key {
                    key,
                    phase: ButtonTransitionPhase::Released,
                }
        })
        .collect::<Vec<_>>();
    assert_eq!(releases.len(), 1);
    let release = *releases[0];
    assert_eq!(release.origin(), InputEventOrigin::Platform);
    assert_eq!(release.member_origin(), Some(InputEventOrigin::Synthetic));
    assert_eq!(release.press_sequence(), press.first_sequence());
    assert_eq!(release.route(), original.route());
    assert_eq!(
        release.invalidation(),
        Some(nara_input::InputInvalidationReason::FocusLost)
    );
    let provenance = release.action_provenance();
    assert_eq!(provenance.origin(), InputEventOrigin::Platform);
    assert_eq!(
        provenance.member_origin(),
        Some(InputEventOrigin::Synthetic)
    );
    assert_eq!(provenance.press_sequence(), press.first_sequence());
    assert_eq!(provenance.press_route(), Some(original.route()));
    let (duplicate, released_keys, released_mouse) = driver(&mut runtime, |scope| {
        apply_window_input_driver_event(scope, WINDOW, SEAT, InputWindowEvent::Focused(false))
    })
    .unwrap();
    assert!(duplicate.is_ignored());
    assert!(released_keys.is_empty());
    assert!(released_mouse.is_empty());
    assert!(
        admit(&mut runtime, &claim, ButtonDriverInput::ReleaseAll)
            .unwrap()
            .is_ignored()
    );
    assert_eq!(entries(&runtime), pending);
    let RuntimeControlRequestResult::Accepted(step) =
        runtime.request_control(RuntimeControl::StepFixedTick)
    else {
        panic!("the original exact tick must consume the platform cancellation");
    };
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(
        runtime.control_status(step),
        Some(RuntimeControlStatus::Applied)
    );
    maintain(&mut runtime);
    let neutral = bindings().map(|binding| KeyboardHold {
        binding,
        held: false,
    });
    assert_eq!(
        capture_keyboard_holds(runtime.world(), WINDOW, SEAT, &bindings()).unwrap(),
        neutral
    );
    assert!(entries(&runtime).is_empty());
    runtime.finish().unwrap();
}
