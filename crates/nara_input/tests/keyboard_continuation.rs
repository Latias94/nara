use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

use nara_app::{
    App, CoreStage, FixedTime, FixedUpdateSet, RuntimeControl, RuntimeControlRequestResult,
    RuntimeDriverScope, RuntimeFrameKind, RuntimeInstance, RuntimeState, RuntimeTimeSettings,
    RuntimeWorldAccessError,
};
use nara_core::ByteLimit;
use nara_ecs::{Res, ResMut, Resource, World, schedule::IntoScheduleConfigs};
use nara_input::{
    ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBindings, ActionMapBuilder,
    ActionMapLimits, ActionMaps, ActionOutcome, ActionOutcomes, ActionPhase, ActionValue,
    ButtonDriverInput, ButtonInput, ButtonInputDriverError, ButtonTransition,
    ButtonTransitionPhase, InputAdmission, InputBinding, InputEventKind, InputEventOrigin,
    InputFrameObservations, InputIngressError, InputLimits, InputPlugin, InputSeatId, InputSet,
    InputTimeline, InputTimelineEntry, InputWindowEvent, InputWindowId, KeyCode,
    KeyboardActionBinding, KeyboardContinuationError, KeyboardHold, SemanticActionCause,
    SemanticActionOutcome, SemanticActions, apply_keyboard_driver_input,
    apply_keyboard_driver_input_at, apply_window_input_driver_event, capture_keyboard_holds,
    connect_input_endpoint, reconcile_keyboard_holds, restore_keyboard_holds,
    timeline::InputEndpointClaim,
};

const WINDOW: InputWindowId = InputWindowId::new(1);
const SEAT: InputSeatId = InputSeatId::PRIMARY;
const KEYS: [KeyCode; 4] = [
    KeyCode::Character('a'),
    KeyCode::Character('d'),
    KeyCode::Character('w'),
    KeyCode::Character('s'),
];
const ALL: [bool; 4] = [true; 4];
const NONE: [bool; 4] = [false; 4];

fn bindings() -> [KeyboardActionBinding; 4] {
    let actions = ["move_left", "move_right", "move_up", "move_down"];
    std::array::from_fn(|index| KeyboardActionBinding {
        key: KEYS[index],
        action: ActionId::new(actions[index]).unwrap(),
        context: ActionContext::gameplay(),
    })
}

fn holds(held: [bool; 4]) -> Vec<KeyboardHold> {
    bindings()
        .into_iter()
        .zip(held)
        .map(|(binding, held)| KeyboardHold { binding, held })
        .collect()
}

fn movement_bindings() -> ActionMapBindings {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    for binding in bindings() {
        builder
            .bind(ActionBinding::key(binding.action, binding.key).with_context(binding.context))
            .unwrap();
    }
    builder.compile().unwrap().into_bindings()
}

fn movement_map() -> ActionMap {
    ActionMap::new(
        movement_bindings(),
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
    )
    .unwrap()
}

#[derive(Debug)]
enum MapChange {
    Disable,
    Enable,
    Replace(ActionMapBindings),
}

#[derive(Debug, Default, Clone, Resource)]
struct MapChanges(Arc<Mutex<Option<MapChange>>>);

fn apply_map_change(changes: Res<MapChanges>, mut maps: ResMut<ActionMaps>) {
    let Some(change) = changes.0.lock().unwrap().take() else {
        return;
    };
    let map = maps.get_mut(SEAT).unwrap();
    match change {
        MapChange::Disable => {
            assert!(map.disable_context(&ActionContext::gameplay()).unwrap());
        }
        MapChange::Enable => {
            assert!(map.enable_context(&ActionContext::gameplay()).unwrap());
        }
        MapChange::Replace(bindings) => map.replace_bindings(bindings),
    }
}

#[derive(Debug)]
struct Sample {
    pressed: [bool; 4],
    just_pressed: [bool; 4],
    just_released: [bool; 4],
    transitions: Vec<ButtonTransition<KeyCode>>,
    held: [bool; 4],
    binding_edges: Vec<ActionOutcome>,
    semantic_edges: Vec<SemanticActionOutcome>,
    observations: Vec<InputTimelineEntry>,
}

#[derive(Debug, Default, Resource)]
struct Trace {
    frames: Vec<Sample>,
    ticks: Vec<Sample>,
}

fn semantic_held(world: &World, seat: InputSeatId) -> [bool; 4] {
    let actions = world.resource::<SemanticActions>();
    bindings().map(|binding| actions.pressed(seat, &binding.action, &binding.context))
}

fn sample(world: &World) -> Sample {
    let keys = world.resource::<ButtonInput<KeyCode>>();
    Sample {
        pressed: KEYS.map(|key| keys.pressed(key)),
        just_pressed: KEYS.map(|key| keys.just_pressed(key)),
        just_released: KEYS.map(|key| keys.just_released(key)),
        transitions: keys.transitions().to_vec(),
        held: semantic_held(world, SEAT),
        binding_edges: world.resource::<ActionOutcomes>().as_slice().to_vec(),
        semantic_edges: world.resource::<SemanticActions>().as_slice().to_vec(),
        observations: world
            .resource::<InputFrameObservations>()
            .as_slice()
            .to_vec(),
    }
}

fn observe_frame(world: &mut World) {
    let sample = sample(world);
    world.resource_mut::<Trace>().frames.push(sample);
}

fn observe_tick(world: &mut World) {
    let sample = sample(world);
    world.resource_mut::<Trace>().ticks.push(sample);
}

fn driver<T>(
    runtime: &mut RuntimeInstance,
    operation: impl FnOnce(&mut RuntimeDriverScope<'_>) -> T,
) -> T {
    let outcome = runtime.with_driver_scope(operation).unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output
}

fn paused_frame(runtime: &mut RuntimeInstance) {
    let outcome = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(outcome.state(), RuntimeState::Paused);
    assert_eq!(outcome.frame_kind(), Some(RuntimeFrameKind::FullFrame));
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 0);
    assert!(runtime.fault().is_none());
}

fn control(runtime: &mut RuntimeInstance, request: RuntimeControl, expected: RuntimeState) {
    assert!(matches!(
        runtime.request_control(request),
        RuntimeControlRequestResult::Accepted(_)
    ));
    let outcome = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(outcome.state(), expected);
    assert!(runtime.fault().is_none());
}

fn exact_tick(runtime: &mut RuntimeInstance) {
    let previous_tick = runtime.world().resource::<FixedTime>().tick();
    let samples = runtime.world().resource::<Trace>().ticks.len();
    assert!(matches!(
        runtime.request_control(RuntimeControl::StepFixedTick),
        RuntimeControlRequestResult::Accepted(_)
    ));
    let outcome = runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(outcome.state(), RuntimeState::Paused);
    assert_eq!(outcome.frame_kind(), Some(RuntimeFrameKind::ExactFixedTick));
    assert_eq!(outcome.frame().unwrap().status.fixed_steps, 1);
    assert_eq!(
        runtime.world().resource::<FixedTime>().tick(),
        previous_tick + 1,
    );
    assert_eq!(runtime.world().resource::<Trace>().ticks.len(), samples + 1);
    assert!(runtime.fault().is_none());
}

fn paused_runtime_with(
    limits: InputLimits,
    maps: ActionMaps,
) -> (RuntimeInstance, InputEndpointClaim, MapChanges) {
    let changes = MapChanges::default();
    let mut app = App::new();
    app.insert_resource(RuntimeTimeSettings::default().with_paused(true))
        .unwrap();
    app.insert_resource(InputTimeline::with_limits(limits).unwrap())
        .unwrap();
    app.insert_resource(maps).unwrap();
    app.add_plugin(InputPlugin).unwrap();
    app.insert_resource(changes.clone()).unwrap();
    app.init_resource::<Trace>().unwrap();
    app.add_systems(
        CoreStage::First,
        apply_map_change.before(InputSet::StageActions),
    )
    .unwrap();
    app.add_systems(CoreStage::Extract, observe_frame).unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        observe_tick.in_set(FixedUpdateSet::Simulate),
    )
    .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();

    // Complete the ordinary paused startup path before the native adapter connects its endpoint.
    paused_frame(&mut runtime);
    driver(&mut runtime, |scope| {
        connect_input_endpoint(scope, WINDOW, SEAT)
    })
    .unwrap();
    let claim = runtime
        .world()
        .resource::<InputTimeline>()
        .claim_endpoint(WINDOW, SEAT)
        .unwrap();
    // First observes the original map/context lifetime even though gameplay remains paused.
    paused_frame(&mut runtime);
    assert_settled(&runtime, NONE);
    (runtime, claim, changes)
}

fn paused_runtime() -> (RuntimeInstance, InputEndpointClaim, MapChanges) {
    paused_runtime_with(
        InputLimits::new(64, 3, 1, 8).unwrap(),
        ActionMaps::new(movement_map()),
    )
}

fn restore(
    runtime: &mut RuntimeInstance,
    claim: &InputEndpointClaim,
    holds: Vec<KeyboardHold>,
) -> Result<InputAdmission, KeyboardContinuationError> {
    driver(runtime, |scope| {
        restore_keyboard_holds(scope, claim.clone(), holds)
    })
}

fn keyboard(
    runtime: &mut RuntimeInstance,
    input: ButtonDriverInput<KeyCode>,
) -> Result<Vec<KeyCode>, ButtonInputDriverError> {
    driver(runtime, |scope| {
        apply_keyboard_driver_input_at(scope, WINDOW, SEAT, input)
    })
}

fn capture(runtime: &RuntimeInstance) -> Result<Vec<KeyboardHold>, KeyboardContinuationError> {
    capture_keyboard_holds(runtime.world(), WINDOW, SEAT, &bindings())
}

fn change_map(runtime: &mut RuntimeInstance, changes: &MapChanges, change: MapChange) {
    assert!(changes.0.lock().unwrap().replace(change).is_none());
    paused_frame(runtime);
}

fn assert_semantic(runtime: &RuntimeInstance, expected: [bool; 4]) {
    let actions = runtime.world().resource::<SemanticActions>();
    for (binding, held) in bindings().into_iter().zip(expected) {
        assert_eq!(
            actions.pressed(SEAT, &binding.action, &binding.context),
            held
        );
        assert_eq!(
            actions.value(SEAT, &binding.action, &binding.context),
            held.then_some(ActionValue::Button(true)),
        );
    }
}

fn assert_clean_transients(runtime: &RuntimeInstance) {
    let keys = runtime.world().resource::<ButtonInput<KeyCode>>();
    assert!(!keys.has_transitions());
    assert!(keys.transitions().is_empty());
    assert_eq!(keys.just_pressed_buttons().count(), 0);
    assert_eq!(keys.just_released_buttons().count(), 0);
    assert!(runtime.world().resource::<ActionOutcomes>().is_empty());
    assert!(
        runtime
            .world()
            .resource::<SemanticActions>()
            .as_slice()
            .is_empty()
    );
    assert!(
        runtime
            .world()
            .resource::<InputFrameObservations>()
            .is_empty()
    );
}

fn assert_settled(runtime: &RuntimeInstance, expected: [bool; 4]) {
    assert_semantic(runtime, expected);
    let keys = runtime.world().resource::<ButtonInput<KeyCode>>();
    assert_eq!(KEYS.map(|key| keys.pressed(key)), expected);
    assert_eq!(
        keys.pressed_buttons().count(),
        expected.into_iter().filter(|held| *held).count(),
    );
    assert_clean_transients(runtime);
    let timeline = runtime.world().resource::<InputTimeline>();
    assert_eq!(timeline.retained_len(), 0);
    assert_eq!(timeline.entries().count(), 0);
    assert_eq!(timeline.pending_ui_entries().count(), 0);
    assert_eq!(timeline.cursors().oldest_deferred_action(), None);
}

fn assert_no_edges(sample: &Sample, expected: [bool; 4]) {
    assert_eq!(sample.pressed, expected);
    assert_eq!(sample.held, expected);
    assert_eq!(sample.just_pressed, NONE);
    assert_eq!(sample.just_released, NONE);
    assert!(sample.transitions.is_empty());
    assert!(sample.binding_edges.is_empty());
    assert!(sample.semantic_edges.is_empty());
}

fn all_binding_edges(runtime: &RuntimeInstance) -> Vec<&ActionOutcome> {
    // Ordinary drives use zero delta; tick samples here belong only to explicit exact steps.
    let trace = runtime.world().resource::<Trace>();
    trace
        .frames
        .iter()
        .chain(&trace.ticks)
        .flat_map(|sample| &sample.binding_edges)
        .collect()
}

fn all_semantic_edges(runtime: &RuntimeInstance) -> Vec<&SemanticActionOutcome> {
    let trace = runtime.world().resource::<Trace>();
    trace
        .frames
        .iter()
        .chain(&trace.ticks)
        .flat_map(|sample| &sample.semantic_edges)
        .collect()
}

fn last_frame(runtime: &RuntimeInstance) -> &Sample {
    runtime.world().resource::<Trace>().frames.last().unwrap()
}

fn last_tick(runtime: &RuntimeInstance) -> &Sample {
    runtime.world().resource::<Trace>().ticks.last().unwrap()
}

#[test]
fn activation_reconciliation_uses_reserved_terminal_records_and_rejects_a_partial_batch() {
    let (mut runtime, claim, _) = paused_runtime();
    restore(&mut runtime, &claim, holds(ALL)).unwrap();
    paused_frame(&mut runtime);
    let before = capture(&runtime).unwrap();
    let invalid = driver(&mut runtime, |scope| {
        reconcile_keyboard_holds(scope, claim.clone(), vec![KEYS[1], KeyCode::Space])
    });
    assert_eq!(invalid, Err(KeyboardContinuationError::ProjectionMismatch));
    assert_eq!(capture(&runtime).unwrap(), before);
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    let accepted = driver(&mut runtime, |scope| {
        reconcile_keyboard_holds(scope, claim.clone(), vec![KEYS[1], KEYS[2]])
    })
    .unwrap();
    assert_eq!(accepted.admitted(), 2);
    let entries = runtime
        .world()
        .resource::<InputTimeline>()
        .entries()
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 2);
    for entry in entries {
        assert_eq!(entry.origin(), InputEventOrigin::Platform);
        assert_eq!(entry.member_origin(), Some(InputEventOrigin::Restored));
        assert_eq!(
            entry.invalidation(),
            Some(nara_input::InputInvalidationReason::ActivationMismatch)
        );
        assert!(matches!(
            entry.kind(),
            InputEventKind::Key {
                phase: ButtonTransitionPhase::Released,
                ..
            }
        ));
    }
    assert_eq!(
        capture(&runtime),
        Err(KeyboardContinuationError::PendingInput)
    );
    paused_frame(&mut runtime);
    assert_eq!(
        capture(&runtime),
        Err(KeyboardContinuationError::PendingInput)
    );
    exact_tick(&mut runtime);
    assert_settled(&runtime, [true, false, false, true]);
    assert_eq!(all_semantic_edges(&runtime).len(), 2);
    assert!(
        all_semantic_edges(&runtime)
            .iter()
            .all(|edge| edge.phase == ActionPhase::Released)
    );
    paused_frame(&mut runtime);
    assert_eq!(
        capture(&runtime).unwrap(),
        holds([true, false, false, true])
    );
    assert_eq!(all_semantic_edges(&runtime).len(), 2);
    runtime.finish().unwrap();
}

#[test]
fn physical_four_key_capture_restores_fresh_members_without_press_or_started() {
    let (mut source, _, _) = paused_runtime();
    control(&mut source, RuntimeControl::Resume, RuntimeState::Running);
    // Give the source different event identities. Only binding/held data crosses Runtime owners.
    for focused in [true, false, true] {
        driver(&mut source, |scope| {
            apply_window_input_driver_event(scope, WINDOW, SEAT, InputWindowEvent::Focused(focused))
        })
        .unwrap();
    }
    for key in KEYS {
        keyboard(&mut source, ButtonDriverInput::Press(key)).unwrap();
    }
    let source_sequences = source
        .world()
        .resource::<InputTimeline>()
        .entries()
        .filter_map(|entry| match entry.kind() {
            InputEventKind::Key {
                phase: ButtonTransitionPhase::Pressed,
                ..
            } => entry.press_sequence(),
            _ => None,
        })
        .collect::<Vec<_>>();
    source.drive(Duration::ZERO).unwrap();
    assert_settled(&source, ALL);
    assert_eq!(all_semantic_edges(&source).len(), 4);
    assert!(all_semantic_edges(&source).iter().all(|edge| {
        edge.phase == ActionPhase::Started && edge.cause == SemanticActionCause::PhysicalInput
    }));
    control(&mut source, RuntimeControl::Pause, RuntimeState::Paused);
    let captured = capture(&source).unwrap();
    assert_eq!(captured, holds(ALL));

    let (mut candidate, claim, _) = paused_runtime();
    let admission = restore(&mut candidate, &claim, captured).unwrap();
    assert_eq!(admission.admitted(), 4);
    assert_eq!(admission.first_sequence(), Some(1));
    assert_eq!(admission.last_sequence(), Some(4));
    assert_semantic(&candidate, ALL);
    assert_eq!(
        candidate
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed_buttons()
            .count(),
        0,
        "the original projection owner applies the restored members during maintenance",
    );
    assert_clean_transients(&candidate);
    let entries = candidate
        .world()
        .resource::<InputTimeline>()
        .entries()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 4);
    assert_eq!(source_sequences.len(), 4);
    for (index, entry) in entries.iter().enumerate() {
        assert_eq!(
            entry.kind(),
            InputEventKind::KeyRestored { key: KEYS[index] },
        );
        assert_eq!(entry.origin(), InputEventOrigin::Restored);
        assert_eq!(entry.member_origin(), Some(InputEventOrigin::Restored));
        assert_eq!(entry.sequence(), index as u64 + 1);
        assert_eq!(entry.press_sequence(), Some(entry.sequence()));
        assert_ne!(entry.press_sequence(), Some(source_sequences[index]));
        assert_eq!(entry.route().window(), WINDOW);
        assert_eq!(entry.route().seat(), SEAT);
        assert!(!entry.route().is_ui_routable());
        assert_eq!(entry.action_input(), None);
        assert_eq!(entry.action_transition(), None);
    }

    paused_frame(&mut candidate);
    assert_settled(&candidate, ALL);
    assert_eq!(capture(&candidate).unwrap(), holds(ALL));
    let observed = last_frame(&candidate);
    assert_no_edges(observed, ALL);
    assert_eq!(observed.observations, entries);
    assert_eq!(
        restore(&mut candidate, &claim, holds(ALL)),
        Err(KeyboardContinuationError::MembersPresent),
    );
    assert_settled(&candidate, ALL);
    exact_tick(&mut candidate);
    assert_no_edges(last_tick(&candidate), ALL);
    paused_frame(&mut candidate);
    assert_no_edges(last_frame(&candidate), ALL);
    assert!(last_frame(&candidate).observations.is_empty());
    control(
        &mut candidate,
        RuntimeControl::Resume,
        RuntimeState::Running,
    );
    assert_settled(&candidate, ALL);
    assert!(all_binding_edges(&candidate).is_empty());
    assert!(all_semantic_edges(&candidate).is_empty());
    source.finish().unwrap();
    candidate.finish().unwrap();
}

#[test]
fn gamepad_button_alternatives_preserve_keyboard_capture_and_restore_without_capturing_gamepads() {
    let make_map = || {
        let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
        builder
            .declare_context(ActionContext::gameplay(), true)
            .unwrap();
        for (code, binding) in bindings().into_iter().enumerate() {
            builder
                .bind(ActionBinding::key(binding.action.clone(), binding.key))
                .unwrap();
            builder
                .bind(ActionBinding::gamepad_button(binding.action, code as u16))
                .unwrap();
        }
        ActionMaps::new(
            ActionMap::new(
                builder.compile().unwrap().into_bindings(),
                ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
            )
            .unwrap(),
        )
    };
    let (mut source, claim, _) =
        paused_runtime_with(InputLimits::new(64, 3, 1, 8).unwrap(), make_map());
    assert_eq!(capture(&source).unwrap(), holds(NONE));
    restore(&mut source, &claim, holds(ALL)).unwrap();
    paused_frame(&mut source);
    let captured = capture(&source).unwrap();
    assert_eq!(captured, holds(ALL));
    assert_no_edges(last_frame(&source), ALL);
    let (mut candidate, target, _) =
        paused_runtime_with(InputLimits::new(64, 3, 1, 8).unwrap(), make_map());
    restore(&mut candidate, &target, captured).unwrap();
    paused_frame(&mut candidate);
    assert_eq!(capture(&candidate).unwrap(), holds(ALL));
    assert_no_edges(last_frame(&candidate), ALL);
    let gamepad = driver(&mut source, |scope| {
        nara_input::connect_gamepad_input(scope, WINDOW, SEAT, 4)
    })
    .unwrap();
    driver(&mut source, |scope| {
        nara_input::apply_gamepad_button_input(scope, &gamepad, 0, ButtonTransitionPhase::Pressed)
    })
    .unwrap();
    exact_tick(&mut source);
    paused_frame(&mut source);
    assert_eq!(
        capture(&source),
        Err(KeyboardContinuationError::UnsupportedMembers)
    );
    assert_semantic(&source, ALL);
    driver(&mut source, |scope| {
        nara_input::disconnect_gamepad_input(scope, &gamepad)
    })
    .unwrap();
    exact_tick(&mut source);
    paused_frame(&mut source);
    assert_eq!(capture(&source).unwrap(), holds(ALL));
    candidate.finish().unwrap();
    source.finish().unwrap();
}

#[test]
fn platform_release_projects_once_in_exact_tick_and_settles_restored_semantics_once() {
    let (mut runtime, claim, _) = paused_runtime();
    let admission = restore(&mut runtime, &claim, holds(ALL)).unwrap();
    paused_frame(&mut runtime);
    keyboard(&mut runtime, ButtonDriverInput::Release(KEYS[0])).unwrap();
    let release = *runtime
        .world()
        .resource::<InputTimeline>()
        .entries()
        .last()
        .unwrap();
    assert_eq!(
        release.kind(),
        InputEventKind::Key {
            key: KEYS[0],
            phase: ButtonTransitionPhase::Released,
        },
    );
    let provenance = release.action_provenance();
    assert_eq!(provenance.origin(), InputEventOrigin::Platform);
    assert_eq!(provenance.member_origin(), Some(InputEventOrigin::Restored));
    assert_eq!(provenance.press_sequence(), admission.first_sequence());
    assert_eq!(provenance.sequence(), 5);
    assert_eq!(provenance.press_route(), Some(release.route()));
    assert_eq!(provenance.route().window(), WINDOW);
    assert_eq!(provenance.route().seat(), SEAT);

    exact_tick(&mut runtime);
    let tick = last_tick(&runtime);
    assert_eq!(tick.pressed, [false, true, true, true]);
    assert_eq!(tick.just_pressed, NONE);
    assert_eq!(tick.just_released, [true, false, false, false]);
    assert_eq!(tick.transitions.len(), 1);
    assert_eq!(tick.transitions[0].button(), KEYS[0]);
    assert_eq!(tick.transitions[0].phase(), ButtonTransitionPhase::Released);
    assert_eq!(tick.transitions[0].provenance(), provenance);
    assert_eq!(tick.held, [false, true, true, true]);
    assert_eq!(tick.binding_edges.len(), 1);
    assert_eq!(tick.semantic_edges.len(), 1);
    assert_semantic(&runtime, [false, true, true, true]);
    assert_clean_transients(&runtime);
    assert_eq!(
        capture(&runtime),
        Err(KeyboardContinuationError::PendingInput)
    );

    paused_frame(&mut runtime);
    let frame = last_frame(&runtime);
    assert_no_edges(frame, [false, true, true, true]);
    assert_eq!(frame.observations, [release]);
    assert_settled(&runtime, [false, true, true, true]);
    assert_eq!(capture(&runtime).unwrap(), holds([false, true, true, true]));

    control(&mut runtime, RuntimeControl::Resume, RuntimeState::Running);
    assert_settled(&runtime, [false, true, true, true]);
    let low_level = all_binding_edges(&runtime);
    assert_eq!(low_level.len(), 1);
    assert_eq!(low_level[0].action, bindings()[0].action);
    assert_eq!(low_level[0].context, ActionContext::gameplay());
    assert_eq!(low_level[0].binding, InputBinding::Key(KEYS[0]));
    assert_eq!(low_level[0].phase, ActionPhase::Released);
    assert_eq!(low_level[0].value, ActionValue::Button(false));
    assert_eq!(low_level[0].provenance, provenance);
    let semantic = all_semantic_edges(&runtime);
    assert_eq!(semantic.len(), 1);
    assert_eq!(semantic[0].seat, SEAT);
    assert_eq!(semantic[0].action, bindings()[0].action);
    assert_eq!(semantic[0].context, ActionContext::gameplay());
    assert_eq!(semantic[0].phase, ActionPhase::Released);
    assert_eq!(semantic[0].value, ActionValue::Button(false));
    assert_eq!(semantic[0].cause, SemanticActionCause::PhysicalInput);
    assert_eq!(semantic[0].provenance, Some(provenance));

    keyboard(&mut runtime, ButtonDriverInput::Release(KEYS[0])).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    control(&mut runtime, RuntimeControl::Pause, RuntimeState::Paused);
    exact_tick(&mut runtime);
    assert_no_edges(last_tick(&runtime), [false, true, true, true]);
    paused_frame(&mut runtime);
    assert_settled(&runtime, [false, true, true, true]);
    assert_eq!(capture(&runtime).unwrap(), holds([false, true, true, true]));
    assert_eq!(all_binding_edges(&runtime).len(), 1);
    assert_eq!(all_semantic_edges(&runtime).len(), 1);
    runtime.finish().unwrap();
}

#[test]
fn release_all_keeps_synthetic_or_platform_trigger_separate_from_restored_member_origin() {
    for (window, origin) in [
        (InputWindowId::UNSPECIFIED, InputEventOrigin::Synthetic),
        (WINDOW, InputEventOrigin::Platform),
    ] {
        let (mut runtime, _, _) = paused_runtime();
        // InputPlugin already owns the default endpoint; claiming it must not reconnect it.
        let claim = runtime
            .world()
            .resource::<InputTimeline>()
            .claim_endpoint(window, SEAT)
            .unwrap();
        let admission = restore(&mut runtime, &claim, holds([true, false, false, false])).unwrap();
        paused_frame(&mut runtime);
        assert_settled(&runtime, [true, false, false, false]);
        let released = driver(&mut runtime, |scope| {
            if window == InputWindowId::UNSPECIFIED {
                apply_keyboard_driver_input(scope, ButtonDriverInput::ReleaseAll)
            } else {
                apply_keyboard_driver_input_at(scope, window, SEAT, ButtonDriverInput::ReleaseAll)
            }
        })
        .unwrap();
        assert_eq!(released, [KEYS[0]]);
        let entries = runtime
            .world()
            .resource::<InputTimeline>()
            .entries()
            .copied()
            .collect::<Vec<_>>();
        assert_eq!(entries.len(), 1);
        let release = entries[0];
        assert_eq!(
            release.kind(),
            InputEventKind::Key {
                key: KEYS[0],
                phase: ButtonTransitionPhase::Released,
            },
        );
        assert_eq!(release.origin(), origin);
        assert_eq!(release.member_origin(), Some(InputEventOrigin::Restored));
        assert_eq!(release.sequence(), 2);
        assert_eq!(release.press_sequence(), admission.first_sequence());
        assert_eq!(release.route().window(), window);
        assert_eq!(release.route().seat(), SEAT);
        let provenance = release.action_provenance();

        exact_tick(&mut runtime);
        let tick = last_tick(&runtime);
        assert_eq!(tick.pressed, NONE);
        assert_eq!(tick.held, NONE);
        assert_eq!(tick.just_pressed, NONE);
        assert_eq!(tick.just_released, [true, false, false, false]);
        assert_eq!(tick.transitions.len(), 1);
        assert_eq!(tick.transitions[0].provenance(), provenance);
        assert_eq!(tick.transitions[0].phase(), ButtonTransitionPhase::Released);
        assert_eq!(tick.binding_edges.len(), 1);
        assert_eq!(tick.binding_edges[0].action, bindings()[0].action);
        assert_eq!(tick.binding_edges[0].context, ActionContext::gameplay());
        assert_eq!(tick.binding_edges[0].binding, InputBinding::Key(KEYS[0]));
        assert_eq!(tick.binding_edges[0].phase, ActionPhase::Released);
        assert_eq!(tick.binding_edges[0].value, ActionValue::Button(false));
        assert_eq!(tick.binding_edges[0].provenance, provenance);
        assert_eq!(tick.semantic_edges.len(), 1);
        assert_eq!(tick.semantic_edges[0].seat, SEAT);
        assert_eq!(tick.semantic_edges[0].action, bindings()[0].action);
        assert_eq!(tick.semantic_edges[0].context, ActionContext::gameplay());
        assert_eq!(tick.semantic_edges[0].phase, ActionPhase::Released);
        assert_eq!(tick.semantic_edges[0].value, ActionValue::Button(false));
        assert_eq!(
            tick.semantic_edges[0].cause,
            SemanticActionCause::PhysicalInput
        );
        assert_eq!(tick.semantic_edges[0].provenance, Some(provenance));
        assert_clean_transients(&runtime);

        paused_frame(&mut runtime);
        assert_no_edges(last_frame(&runtime), NONE);
        assert_eq!(last_frame(&runtime).observations, entries);
        assert_settled(&runtime, NONE);
        assert_eq!(
            capture_keyboard_holds(runtime.world(), window, SEAT, &bindings()).unwrap(),
            holds(NONE),
        );
        exact_tick(&mut runtime);
        assert_no_edges(last_tick(&runtime), NONE);
        paused_frame(&mut runtime);
        assert!(last_frame(&runtime).observations.is_empty());
        assert_eq!(all_binding_edges(&runtime).len(), 1);
        assert_eq!(all_semantic_edges(&runtime).len(), 1);
        runtime.finish().unwrap();
    }
}

#[test]
fn capture_refuses_both_unscanned_and_paused_staged_release_despite_caught_up_cursors() {
    let (mut runtime, claim, _) = paused_runtime();
    restore(&mut runtime, &claim, holds(ALL)).unwrap();
    paused_frame(&mut runtime);
    keyboard(&mut runtime, ButtonDriverInput::Release(KEYS[1])).unwrap();
    let release = *runtime
        .world()
        .resource::<InputTimeline>()
        .entries()
        .last()
        .unwrap();
    assert_eq!(
        capture(&runtime),
        Err(KeyboardContinuationError::PendingInput)
    );
    assert_semantic(&runtime, ALL);
    assert!(
        runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KEYS[1])
    );

    paused_frame(&mut runtime);
    let timeline = runtime.world().resource::<InputTimeline>();
    let cursors = timeline.cursors();
    assert_eq!(cursors.observation(), release.sequence());
    assert_eq!(cursors.ui(), release.sequence());
    assert_eq!(cursors.action_scanned(), release.sequence());
    assert_eq!(cursors.oldest_deferred_action(), Some(release.sequence()));
    assert_eq!(timeline.retained_len(), 1);
    assert_eq!(timeline.entries().copied().collect::<Vec<_>>(), [release]);
    assert_eq!(
        capture(&runtime),
        Err(KeyboardContinuationError::PendingInput)
    );
    assert_semantic(&runtime, ALL);
    assert!(
        !runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KEYS[1])
    );
    assert_clean_transients(&runtime);
    paused_frame(&mut runtime);
    assert_eq!(
        capture(&runtime),
        Err(KeyboardContinuationError::PendingInput)
    );
    assert!(last_frame(&runtime).observations.is_empty());

    control(&mut runtime, RuntimeControl::Resume, RuntimeState::Running);
    control(&mut runtime, RuntimeControl::Pause, RuntimeState::Paused);
    assert_settled(&runtime, [true, false, true, true]);
    assert_eq!(capture(&runtime).unwrap(), holds([true, false, true, true]));
    assert_eq!(all_semantic_edges(&runtime).len(), 1);
    assert_eq!(all_semantic_edges(&runtime)[0].phase, ActionPhase::Released);
    runtime.finish().unwrap();
}

fn assert_cancelled_hold_requires_fresh_press(replace: bool) {
    let (mut runtime, claim, changes) = paused_runtime();
    let admission = restore(&mut runtime, &claim, holds([true, false, false, false])).unwrap();
    paused_frame(&mut runtime);
    if replace {
        change_map(
            &mut runtime,
            &changes,
            MapChange::Replace(movement_bindings()),
        );
    } else {
        change_map(&mut runtime, &changes, MapChange::Disable);
        assert_eq!(
            capture(&runtime),
            Err(KeyboardContinuationError::BindingMismatch),
        );
        change_map(&mut runtime, &changes, MapChange::Enable);
    }
    assert_eq!(
        capture(&runtime),
        Err(KeyboardContinuationError::ProjectionMismatch),
        "an observed configuration exit cannot be captured as continued intent",
    );
    keyboard(&mut runtime, ButtonDriverInput::Press(KEYS[0])).unwrap();
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    paused_frame(&mut runtime);
    control(&mut runtime, RuntimeControl::Resume, RuntimeState::Running);
    assert_semantic(&runtime, NONE);
    assert!(
        runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KEYS[0])
    );
    assert_clean_transients(&runtime);
    let semantic = all_semantic_edges(&runtime);
    assert_eq!(semantic.len(), 1);
    assert_eq!(semantic[0].seat, SEAT);
    assert_eq!(semantic[0].action, bindings()[0].action);
    assert_eq!(semantic[0].context, ActionContext::gameplay());
    assert_eq!(semantic[0].phase, ActionPhase::Released);
    assert_eq!(semantic[0].value, ActionValue::Button(false));
    assert_eq!(
        semantic[0].cause,
        if replace {
            SemanticActionCause::MappingChanged
        } else {
            SemanticActionCause::ContextChanged
        },
    );
    assert_eq!(semantic[0].provenance, None);
    assert!(all_binding_edges(&runtime).is_empty());

    keyboard(&mut runtime, ButtonDriverInput::Press(KEYS[0])).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_semantic(&runtime, NONE);
    assert_eq!(all_semantic_edges(&runtime).len(), 1);
    assert!(all_binding_edges(&runtime).is_empty());
    keyboard(&mut runtime, ButtonDriverInput::Release(KEYS[0])).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_settled(&runtime, NONE);
    assert_eq!(
        all_semantic_edges(&runtime).len(),
        1,
        "the physical release must not cancel twice",
    );
    let released = all_binding_edges(&runtime);
    assert_eq!(released.len(), 1);
    assert_eq!(released[0].phase, ActionPhase::Released);
    assert_eq!(released[0].provenance.origin(), InputEventOrigin::Platform);
    assert_eq!(
        released[0].provenance.member_origin(),
        Some(InputEventOrigin::Restored),
    );
    assert_eq!(
        released[0].provenance.press_sequence(),
        admission.first_sequence(),
    );

    keyboard(&mut runtime, ButtonDriverInput::Press(KEYS[0])).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert_settled(&runtime, [true, false, false, false]);
    control(&mut runtime, RuntimeControl::Pause, RuntimeState::Paused);
    assert_eq!(
        capture(&runtime).unwrap(),
        holds([true, false, false, false])
    );
    let semantic = all_semantic_edges(&runtime);
    assert_eq!(semantic.len(), 2);
    assert_eq!(semantic[1].phase, ActionPhase::Started);
    assert_eq!(semantic[1].action, bindings()[0].action);
    assert_eq!(semantic[1].value, ActionValue::Button(true));
    assert_eq!(semantic[1].cause, SemanticActionCause::PhysicalInput);
    let fresh = semantic[1].provenance.unwrap();
    assert_eq!(fresh.origin(), InputEventOrigin::Platform);
    assert_eq!(fresh.member_origin(), Some(InputEventOrigin::Platform));
    assert_eq!(fresh.press_sequence(), Some(fresh.sequence()));
    assert!(fresh.press_sequence() > admission.last_sequence());
    assert_eq!(all_binding_edges(&runtime).len(), 2);
    runtime.finish().unwrap();
}

#[test]
fn context_exit_and_identical_mapping_replacement_require_release_and_fresh_press() {
    assert_cancelled_hold_requires_fresh_press(false);
    assert_cancelled_hold_requires_fresh_press(true);
}

#[test]
fn invalid_inventory_and_disabled_bindings_refuse_without_partial_admission() {
    let (mut runtime, claim, changes) = paused_runtime();
    let mut duplicate_key = holds(ALL);
    duplicate_key[1].binding.key = KEYS[0];
    let mut duplicate_action = holds(ALL);
    duplicate_action[1].binding.action = duplicate_action[0].binding.action.clone();
    let mut wrong_action = holds(ALL);
    wrong_action[3].binding.action = ActionId::new("other_action").unwrap();
    let mut wrong_context = holds(ALL);
    wrong_context[3].binding.context = ActionContext::new("menu").unwrap();
    let mut wrong_key = holds(ALL);
    wrong_key[3].binding.key = KeyCode::Space;
    let mut extra_binding = holds(ALL);
    extra_binding.push(KeyboardHold {
        binding: KeyboardActionBinding {
            key: KeyCode::Enter,
            action: ActionId::new("confirm").unwrap(),
            context: ActionContext::gameplay(),
        },
        held: true,
    });
    for (invalid, expected) in [
        (Vec::new(), KeyboardContinuationError::InvalidCount),
        (duplicate_key, KeyboardContinuationError::DuplicateBinding),
        (
            duplicate_action,
            KeyboardContinuationError::DuplicateBinding,
        ),
        (wrong_action, KeyboardContinuationError::BindingMismatch),
        (wrong_context, KeyboardContinuationError::BindingMismatch),
        (wrong_key, KeyboardContinuationError::BindingMismatch),
        (extra_binding, KeyboardContinuationError::BindingMismatch),
    ] {
        let invalid_bindings = invalid
            .iter()
            .map(|hold| hold.binding.clone())
            .collect::<Vec<_>>();
        assert_eq!(
            capture_keyboard_holds(runtime.world(), WINDOW, SEAT, &invalid_bindings),
            Err(expected.clone()),
        );
        assert_eq!(restore(&mut runtime, &claim, invalid), Err(expected));
        assert_settled(&runtime, NONE);
        assert_eq!(capture(&runtime).unwrap(), holds(NONE));
    }
    change_map(&mut runtime, &changes, MapChange::Disable);
    assert_eq!(
        capture(&runtime),
        Err(KeyboardContinuationError::BindingMismatch),
    );
    assert_eq!(
        restore(&mut runtime, &claim, holds(ALL)),
        Err(KeyboardContinuationError::BindingMismatch),
    );
    assert_settled(&runtime, NONE);
    change_map(&mut runtime, &changes, MapChange::Enable);
    let admission = restore(&mut runtime, &claim, holds(ALL)).unwrap();
    assert_eq!(
        admission.first_sequence(),
        Some(1),
        "refusals must not spend event identities",
    );
    assert_eq!(admission.last_sequence(), Some(4));
    paused_frame(&mut runtime);
    assert_settled(&runtime, ALL);
    assert_eq!(capture(&runtime).unwrap(), holds(ALL));
    assert!(all_semantic_edges(&runtime).is_empty());
    assert!(all_binding_edges(&runtime).is_empty());
    runtime.finish().unwrap();
}

#[test]
fn capture_rejects_extra_held_key_even_when_declared_four_actions_are_consistent() {
    let (mut runtime, claim, _) = paused_runtime();
    restore(&mut runtime, &claim, holds([true, false, false, false])).unwrap();
    paused_frame(&mut runtime);
    keyboard(&mut runtime, ButtonDriverInput::Press(KeyCode::Space)).unwrap();
    control(&mut runtime, RuntimeControl::Resume, RuntimeState::Running);
    control(&mut runtime, RuntimeControl::Pause, RuntimeState::Paused);
    assert_semantic(&runtime, [true, false, false, false]);
    let keys = runtime.world().resource::<ButtonInput<KeyCode>>();
    assert!(keys.pressed(KEYS[0]));
    assert!(keys.pressed(KeyCode::Space));
    assert_eq!(keys.pressed_buttons().count(), 2);
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    assert_eq!(
        capture(&runtime),
        Err(KeyboardContinuationError::UnsupportedMembers),
    );
    assert_clean_transients(&runtime);
    assert!(all_semantic_edges(&runtime).is_empty());
    assert!(all_binding_edges(&runtime).is_empty());

    keyboard(&mut runtime, ButtonDriverInput::Release(KeyCode::Space)).unwrap();
    control(&mut runtime, RuntimeControl::Resume, RuntimeState::Running);
    control(&mut runtime, RuntimeControl::Pause, RuntimeState::Paused);
    assert_settled(&runtime, [true, false, false, false]);
    assert_eq!(
        capture(&runtime).unwrap(),
        holds([true, false, false, false])
    );
    assert!(all_semantic_edges(&runtime).is_empty());
    runtime.finish().unwrap();
}

#[test]
fn event_capacity_rejects_whole_restore_and_preserves_room_for_smaller_batch() {
    let (mut runtime, claim, _) = paused_runtime_with(
        InputLimits::new(13, 2, 1, 4).unwrap(),
        ActionMaps::new(movement_map()),
    );
    let cursors = runtime.world().resource::<InputTimeline>().cursors();
    assert_eq!(
        restore(&mut runtime, &claim, holds(ALL)),
        Err(KeyboardContinuationError::Ingress(
            InputIngressError::CapacityExceeded {
                limit: 13,
                retained: 0,
                requested: 14,
            },
        )),
    );
    assert_settled(&runtime, NONE);
    assert_eq!(
        runtime.world().resource::<InputTimeline>().cursors(),
        cursors
    );
    assert_eq!(capture(&runtime).unwrap(), holds(NONE));
    paused_frame(&mut runtime);
    assert_no_edges(last_frame(&runtime), NONE);
    let admission = restore(&mut runtime, &claim, holds([true, true, true, false])).unwrap();
    assert_eq!(admission.admitted(), 3);
    assert_eq!(admission.first_sequence(), Some(1));
    assert_eq!(admission.last_sequence(), Some(3));
    paused_frame(&mut runtime);
    assert_settled(&runtime, [true, true, true, false]);
    assert_eq!(capture(&runtime).unwrap(), holds([true, true, true, false]));
    assert!(all_semantic_edges(&runtime).is_empty());
    assert!(all_binding_edges(&runtime).is_empty());
    runtime.finish().unwrap();
}

#[test]
fn member_capacity_refusal_preserves_another_seats_member_and_all_target_actions() {
    let other_window = InputWindowId::new(2);
    let other_seat = InputSeatId::new(1);
    let mut maps = ActionMaps::new(movement_map());
    assert!(maps.insert(other_seat, movement_map()).unwrap().is_none());
    let (mut runtime, claim, _) = paused_runtime_with(InputLimits::new(32, 3, 1, 4).unwrap(), maps);
    driver(&mut runtime, |scope| {
        connect_input_endpoint(scope, other_window, other_seat)?;
        apply_keyboard_driver_input_at(
            scope,
            other_window,
            other_seat,
            ButtonDriverInput::Press(KEYS[0]),
        )
    })
    .unwrap();
    control(&mut runtime, RuntimeControl::Resume, RuntimeState::Running);
    control(&mut runtime, RuntimeControl::Pause, RuntimeState::Paused);
    assert_eq!(
        semantic_held(runtime.world(), other_seat),
        [true, false, false, false],
    );
    assert_eq!(
        restore(&mut runtime, &claim, holds(ALL)),
        Err(KeyboardContinuationError::Ingress(
            InputIngressError::MemberCapacityExceeded { limit: 4 },
        )),
    );
    assert_semantic(&runtime, NONE);
    assert_eq!(
        semantic_held(runtime.world(), other_seat),
        [true, false, false, false],
    );
    assert_eq!(capture(&runtime).unwrap(), holds(NONE));
    let keys = runtime.world().resource::<ButtonInput<KeyCode>>();
    assert_eq!(
        KEYS.map(|key| keys.pressed(key)),
        [true, false, false, false]
    );
    assert_eq!(keys.pressed_buttons().count(), 1);
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    assert_clean_transients(&runtime);
    assert_eq!(all_semantic_edges(&runtime).len(), 1);
    assert_eq!(all_semantic_edges(&runtime)[0].seat, other_seat);

    driver(&mut runtime, |scope| {
        apply_keyboard_driver_input_at(
            scope,
            other_window,
            other_seat,
            ButtonDriverInput::Release(KEYS[0]),
        )
    })
    .unwrap();
    control(&mut runtime, RuntimeControl::Resume, RuntimeState::Running);
    control(&mut runtime, RuntimeControl::Pause, RuntimeState::Paused);
    assert_settled(&runtime, NONE);
    assert_eq!(semantic_held(runtime.world(), other_seat), NONE);
    let admission = restore(&mut runtime, &claim, holds(ALL)).unwrap();
    assert_eq!(admission.admitted(), 4);
    assert_eq!(admission.first_sequence(), Some(3));
    assert_eq!(admission.last_sequence(), Some(6));
    paused_frame(&mut runtime);
    assert_settled(&runtime, ALL);
    assert_eq!(capture(&runtime).unwrap(), holds(ALL));
    assert_eq!(all_semantic_edges(&runtime).len(), 2);
    assert!(
        all_semantic_edges(&runtime)
            .iter()
            .all(|edge| edge.seat == other_seat)
    );
    runtime.finish().unwrap();
}

#[test]
fn restored_terminal_releases_remain_admissible_after_ordinary_event_saturation() {
    let (mut runtime, claim, _) = paused_runtime_with(
        InputLimits::new(16, 2, 1, 8).unwrap(),
        ActionMaps::new(movement_map()),
    );
    restore(&mut runtime, &claim, holds(ALL)).unwrap();
    paused_frame(&mut runtime);
    for key in [KeyCode::Space, KeyCode::Enter, KeyCode::Escape] {
        keyboard(&mut runtime, ButtonDriverInput::Press(key)).unwrap();
    }
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        3
    );
    assert_eq!(
        keyboard(&mut runtime, ButtonDriverInput::Press(KeyCode::ArrowUp)),
        Err(ButtonInputDriverError::Timeline(
            InputIngressError::CapacityExceeded {
                limit: 16,
                retained: 3,
                requested: 15,
            },
        )),
    );
    for key in KEYS {
        keyboard(&mut runtime, ButtonDriverInput::Release(key)).unwrap();
    }
    let entries = runtime
        .world()
        .resource::<InputTimeline>()
        .entries()
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 7);
    for (index, release) in entries[3..].iter().enumerate() {
        assert_eq!(release.sequence(), index as u64 + 8);
        assert_eq!(release.press_sequence(), Some(index as u64 + 1));
        assert_eq!(
            release.kind(),
            InputEventKind::Key {
                key: KEYS[index],
                phase: ButtonTransitionPhase::Released,
            },
        );
        assert_eq!(release.origin(), InputEventOrigin::Platform);
        assert_eq!(release.member_origin(), Some(InputEventOrigin::Restored));
    }
    paused_frame(&mut runtime);
    assert_semantic(&runtime, ALL);
    let observed = last_frame(&runtime);
    assert_eq!(observed.pressed, NONE);
    assert_eq!(observed.just_pressed, NONE);
    assert_eq!(observed.just_released, ALL);
    assert_eq!(observed.observations, entries);
    assert_eq!(observed.transitions.len(), 7);
    control(&mut runtime, RuntimeControl::Resume, RuntimeState::Running);
    assert_semantic(&runtime, NONE);
    assert_clean_transients(&runtime);
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        0
    );
    let keys = runtime.world().resource::<ButtonInput<KeyCode>>();
    assert_eq!(KEYS.map(|key| keys.pressed(key)), NONE);
    assert_eq!(keys.pressed_buttons().count(), 3);
    for key in [KeyCode::Space, KeyCode::Enter, KeyCode::Escape] {
        assert!(keys.pressed(key));
    }
    assert!(!keys.pressed(KeyCode::ArrowUp));
    let semantic = all_semantic_edges(&runtime);
    let low_level = all_binding_edges(&runtime);
    assert_eq!(semantic.len(), 4);
    assert_eq!(low_level.len(), 4);
    for (index, binding) in bindings().iter().enumerate() {
        let edge = semantic[index];
        assert_eq!(edge.seat, SEAT);
        assert_eq!(edge.action, binding.action);
        assert_eq!(edge.context, binding.context);
        assert_eq!(edge.phase, ActionPhase::Released);
        assert_eq!(edge.value, ActionValue::Button(false));
        assert_eq!(edge.cause, SemanticActionCause::PhysicalInput);
        assert_eq!(
            edge.provenance,
            Some(entries[index + 3].action_provenance())
        );
        assert_eq!(low_level[index].action, binding.action);
        assert_eq!(low_level[index].phase, ActionPhase::Released);
        assert_eq!(
            low_level[index].provenance,
            entries[index + 3].action_provenance(),
        );
    }
    runtime.drive(Duration::ZERO).unwrap();
    assert_eq!(all_semantic_edges(&runtime).len(), 4);
    assert_eq!(all_binding_edges(&runtime).len(), 4);
    runtime.finish().unwrap();
}

#[test]
fn foreign_and_recycled_endpoint_claims_cannot_admit_into_the_receiving_runtime() {
    let (other, foreign, _) = paused_runtime();
    let (mut runtime, stale, _) = paused_runtime();
    assert_eq!(
        restore(&mut runtime, &foreign, holds(ALL)),
        Err(KeyboardContinuationError::Ingress(
            InputIngressError::ForeignEndpointClaim,
        )),
    );
    assert_settled(&runtime, NONE);
    assert_eq!(capture(&runtime).unwrap(), holds(NONE));
    driver(&mut runtime, |scope| {
        apply_window_input_driver_event(scope, WINDOW, SEAT, InputWindowEvent::SurfaceRetired)
    })
    .unwrap();
    paused_frame(&mut runtime);
    driver(&mut runtime, |scope| {
        connect_input_endpoint(scope, WINDOW, SEAT)
    })
    .unwrap();
    let fresh = runtime
        .world()
        .resource::<InputTimeline>()
        .claim_endpoint(WINDOW, SEAT)
        .unwrap();
    paused_frame(&mut runtime);
    assert_eq!(
        restore(&mut runtime, &stale, holds(ALL)),
        Err(KeyboardContinuationError::StaleEndpoint),
    );
    assert_settled(&runtime, NONE);
    assert_eq!(capture(&runtime).unwrap(), holds(NONE));
    let admission = restore(&mut runtime, &fresh, holds(ALL)).unwrap();
    assert_eq!(admission.admitted(), 4);
    assert_eq!(admission.first_sequence(), Some(2));
    assert_eq!(admission.last_sequence(), Some(5));
    paused_frame(&mut runtime);
    assert_settled(&runtime, ALL);
    assert_eq!(capture(&runtime).unwrap(), holds(ALL));
    assert!(all_semantic_edges(&runtime).is_empty());
    assert!(all_binding_edges(&runtime).is_empty());
    assert_settled(&other, NONE);
    other.finish().unwrap();
    runtime.finish().unwrap();
}

#[test]
fn restore_refuses_running_and_stopping_without_mutation() {
    let (mut runtime, claim, _) = paused_runtime();
    control(&mut runtime, RuntimeControl::Resume, RuntimeState::Running);
    assert!(matches!(
        restore(&mut runtime, &claim, holds(ALL)),
        Err(KeyboardContinuationError::WorldAccess(
            RuntimeWorldAccessError::Unavailable {
                state: RuntimeState::Running,
            },
        )),
    ));
    assert_settled(&runtime, NONE);
    control(&mut runtime, RuntimeControl::Pause, RuntimeState::Paused);
    let admission = restore(&mut runtime, &claim, holds(NONE)).unwrap();
    assert!(admission.is_ignored());
    assert_eq!(admission.admitted(), 0);
    assert_eq!(admission.first_sequence(), None);
    assert_eq!(admission.last_sequence(), None);
    assert_settled(&runtime, NONE);
    control(&mut runtime, RuntimeControl::Stop, RuntimeState::Stopping);
    assert!(matches!(
        restore(&mut runtime, &claim, holds(ALL)),
        Err(KeyboardContinuationError::WorldAccess(
            RuntimeWorldAccessError::Unavailable {
                state: RuntimeState::Stopping,
            },
        )),
    ));
    assert_settled(&runtime, NONE);
    assert!(all_semantic_edges(&runtime).is_empty());
    assert!(all_binding_edges(&runtime).is_empty());
    runtime.finish().unwrap();
}
