use std::time::Duration;

use nara_app::{RuntimeControl, RuntimeControlRequestResult, RuntimeInstance};
use nara_core::ByteLimit;

use super::*;

#[derive(Debug, Default, Resource)]
struct FrameSnapshot {
    keys: Vec<ButtonTransition<KeyCode>>,
    mouse: Vec<ButtonTransition<MouseButton>>,
    held_keys: Vec<KeyCode>,
    held_mouse: Vec<MouseButton>,
    position: Option<Vec2>,
    actions: Vec<ActionOutcome>,
    semantic: Vec<SemanticActionOutcome>,
}

fn capture_buttons(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    pointer: Res<PointerState>,
    mut snapshot: ResMut<FrameSnapshot>,
) {
    snapshot.keys = keys.transitions().to_vec();
    snapshot.mouse = mouse.transitions().to_vec();
    snapshot.held_keys = keys.pressed_buttons().collect();
    snapshot.held_mouse = mouse.pressed_buttons().collect();
    snapshot.position = pointer.position();
}

fn capture_actions(
    actions: Res<ActionOutcomes>,
    semantic: Res<SemanticActions>,
    mut snapshot: ResMut<FrameSnapshot>,
) {
    snapshot.actions = actions.as_slice().to_vec();
    snapshot.semantic = semantic.as_slice().to_vec();
}

fn runtime(limits: InputLimits) -> RuntimeInstance {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    for key in [KeyCode::Space, KeyCode::Enter] {
        builder
            .bind(ActionBinding::key(ActionId::new("activate").unwrap(), key))
            .unwrap();
    }
    builder
        .bind(ActionBinding::mouse(
            ActionId::new("activate").unwrap(),
            MouseButton::Left,
        ))
        .unwrap();
    let map = ActionMap::new(
        builder.compile().unwrap().into_bindings(),
        ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
    )
    .unwrap();
    let mut app = App::new();
    app.insert_resource(InputTimeline::with_limits(limits).unwrap())
        .unwrap();
    app.insert_resource(ActionMaps::new(map)).unwrap();
    app.add_plugin(InputPlugin).unwrap();
    app.insert_resource(FrameSnapshot::default()).unwrap();
    app.add_systems(
        CoreStage::First,
        capture_buttons.after(InputSet::StageActions),
    )
    .unwrap();
    app.add_systems(CoreStage::Update, capture_actions).unwrap();
    RuntimeInstance::start(app).unwrap()
}

fn input<T>(
    runtime: &mut RuntimeInstance,
    operation: impl FnOnce(&mut RuntimeDriverScope<'_>) -> T,
) -> T {
    let outcome = runtime.with_driver_scope(operation).unwrap();
    assert!(outcome.post_operation_fault.is_none());
    outcome.output
}

fn key(runtime: &mut RuntimeInstance, edge: ButtonDriverInput<KeyCode>) -> Vec<KeyCode> {
    input(runtime, |scope| apply_keyboard_driver_input(scope, edge)).unwrap()
}

fn activate_is_held(runtime: &RuntimeInstance) -> bool {
    runtime.world().resource::<SemanticActions>().pressed(
        InputSeatId::PRIMARY,
        &ActionId::new("activate").unwrap(),
        &ActionContext::gameplay(),
    )
}

#[test]
fn synthetic_edges_share_admitted_members_and_clear_after_each_frame() {
    let limits = InputLimits::new(24, 1, 1, 4).unwrap();
    let mut runtime = runtime(limits);
    key(&mut runtime, ButtonDriverInput::Press(KeyCode::Space));
    key(&mut runtime, ButtonDriverInput::Release(KeyCode::Space));
    key(&mut runtime, ButtonDriverInput::Press(KeyCode::Space));
    runtime.drive(Duration::ZERO).unwrap();

    let snapshot = runtime.world().resource::<FrameSnapshot>();
    assert_eq!(snapshot.keys.len(), 3);
    assert_eq!(snapshot.held_keys, [KeyCode::Space]);
    let [first, release, second] = snapshot.keys.as_slice() else {
        unreachable!();
    };
    assert_eq!(first.sequence(), first.press_sequence());
    assert_eq!(release.press_sequence(), first.press_sequence());
    assert_ne!(second.press_sequence(), first.press_sequence());
    assert!(first.sequence() < release.sequence());
    assert!(release.sequence() < second.sequence());
    for edge in &snapshot.keys {
        assert_eq!(edge.provenance().origin(), InputEventOrigin::Synthetic);
        assert_eq!(edge.provenance().sequence(), edge.sequence());
        assert_eq!(
            edge.provenance().press_sequence(),
            Some(edge.press_sequence())
        );
    }
    assert_eq!(
        snapshot
            .semantic
            .iter()
            .map(|outcome| outcome.phase)
            .collect::<Vec<_>>(),
        [
            ActionPhase::Started,
            ActionPhase::Released,
            ActionPhase::Started
        ]
    );
    assert!(activate_is_held(&runtime));
    let keys = runtime.world().resource::<ButtonInput<KeyCode>>();
    assert!(keys.pressed(KeyCode::Space));
    assert!(keys.transitions().is_empty());
    assert_eq!(keys.pressed.capacity(), limits.members());
    assert_eq!(keys.transitions.capacity(), limits.events());

    runtime.drive(Duration::ZERO).unwrap();
    let snapshot = runtime.world().resource::<FrameSnapshot>();
    assert!(snapshot.keys.is_empty());
    assert!(snapshot.actions.is_empty());
    assert!(snapshot.semantic.is_empty());
    assert!(activate_is_held(&runtime));
    runtime.finish().unwrap();
}

#[test]
fn release_all_projects_one_terminal_edge_per_accepted_key() {
    let mut runtime = runtime(InputLimits::new(24, 1, 1, 4).unwrap());
    key(&mut runtime, ButtonDriverInput::Press(KeyCode::Enter));
    key(&mut runtime, ButtonDriverInput::Press(KeyCode::Space));
    runtime.drive(Duration::ZERO).unwrap();
    let pressed = runtime.world().resource::<FrameSnapshot>().keys.clone();
    let mut released = key(&mut runtime, ButtonDriverInput::ReleaseAll);
    released.sort();
    assert_eq!(released, [KeyCode::Space, KeyCode::Enter]);
    runtime.drive(Duration::ZERO).unwrap();

    let snapshot = runtime.world().resource::<FrameSnapshot>();
    assert!(snapshot.held_keys.is_empty());
    assert_eq!(snapshot.keys.len(), 2);
    for edge in &snapshot.keys {
        assert_eq!(edge.phase(), ButtonTransitionPhase::Released);
        let press = pressed
            .iter()
            .find(|press| press.button() == edge.button())
            .unwrap();
        assert_eq!(edge.press_sequence(), press.press_sequence());
    }
    assert_eq!(snapshot.semantic.len(), 1);
    assert_eq!(snapshot.semantic[0].phase, ActionPhase::Released);
    assert!(!activate_is_held(&runtime));
    runtime.finish().unwrap();
}

#[test]
fn physical_and_synthetic_members_share_one_semantic_aggregate() {
    let mut runtime = runtime(InputLimits::new(32, 2, 1, 4).unwrap());
    let window = InputWindowId::new(81);
    let seat = InputSeatId::PRIMARY;
    input(&mut runtime, |scope| {
        connect_input_endpoint(scope, window, seat)
    })
    .unwrap();
    key(&mut runtime, ButtonDriverInput::Press(KeyCode::Space));
    input(&mut runtime, |scope| {
        apply_keyboard_driver_input_at(
            scope,
            window,
            seat,
            ButtonDriverInput::Press(KeyCode::Space),
        )
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();

    let snapshot = runtime.world().resource::<FrameSnapshot>();
    assert_eq!(snapshot.held_keys, [KeyCode::Space]);
    assert_eq!(snapshot.semantic.len(), 1);
    assert_eq!(snapshot.semantic[0].seat, InputSeatId::PRIMARY);
    assert_eq!(snapshot.semantic[0].value, ActionValue::pressed());
    assert_eq!(snapshot.actions.len(), 2);
    assert_eq!(
        snapshot.actions[0].provenance.origin(),
        InputEventOrigin::Synthetic
    );
    assert_eq!(
        snapshot.actions[1].provenance.origin(),
        InputEventOrigin::Platform
    );
    let platform_member = snapshot.keys[1].press_sequence();
    assert_ne!(snapshot.keys[0].press_sequence(), platform_member);

    key(&mut runtime, ButtonDriverInput::Release(KeyCode::Space));
    runtime.drive(Duration::ZERO).unwrap();
    let snapshot = runtime.world().resource::<FrameSnapshot>();
    assert_eq!(snapshot.held_keys, [KeyCode::Space]);
    assert!(snapshot.semantic.is_empty());
    assert!(activate_is_held(&runtime));

    input(&mut runtime, |scope| {
        apply_keyboard_driver_input_at(
            scope,
            window,
            seat,
            ButtonDriverInput::Release(KeyCode::Space),
        )
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let snapshot = runtime.world().resource::<FrameSnapshot>();
    assert!(snapshot.held_keys.is_empty());
    assert_eq!(snapshot.semantic.len(), 1);
    let released = snapshot.semantic[0].provenance.unwrap();
    assert_eq!(released.origin(), InputEventOrigin::Platform);
    assert_eq!(released.press_sequence(), Some(platform_member));
    assert_eq!(released.route().window(), window);
    assert_eq!(released.route().seat(), seat);
    runtime.finish().unwrap();
}

#[test]
fn unconfigured_seat_keeps_raw_input_without_borrowing_the_primary_mapping() {
    let mut runtime = runtime(InputLimits::new(32, 2, 1, 4).unwrap());
    let window = InputWindowId::new(82);
    let seat = InputSeatId::new(2);
    input(&mut runtime, |scope| {
        connect_input_endpoint(scope, window, seat)
    })
    .unwrap();
    input(&mut runtime, |scope| {
        apply_keyboard_driver_input_at(
            scope,
            window,
            seat,
            ButtonDriverInput::Press(KeyCode::Space),
        )
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let snapshot = runtime.world().resource::<FrameSnapshot>();
    assert_eq!(snapshot.held_keys, [KeyCode::Space]);
    assert_eq!(snapshot.keys.len(), 1);
    assert_eq!(snapshot.keys[0].provenance().route().seat(), seat);
    assert_eq!(
        snapshot.keys[0].provenance().origin(),
        InputEventOrigin::Platform
    );
    assert!(snapshot.actions.is_empty());
    assert!(snapshot.semantic.is_empty());
    assert!(!activate_is_held(&runtime));
    assert_eq!(
        runtime.world().resource::<SemanticActions>().value(
            seat,
            &ActionId::new("activate").unwrap(),
            &ActionContext::gameplay(),
        ),
        None
    );

    key(&mut runtime, ButtonDriverInput::Press(KeyCode::Space));
    runtime.drive(Duration::ZERO).unwrap();
    let snapshot = runtime.world().resource::<FrameSnapshot>();
    assert_eq!(snapshot.semantic.len(), 1);
    assert_eq!(snapshot.semantic[0].seat, InputSeatId::PRIMARY);
    assert!(activate_is_held(&runtime));
    input(&mut runtime, |scope| {
        apply_keyboard_driver_input_at(
            scope,
            window,
            seat,
            ButtonDriverInput::Release(KeyCode::Space),
        )
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let snapshot = runtime.world().resource::<FrameSnapshot>();
    assert_eq!(snapshot.held_keys, [KeyCode::Space]);
    assert!(snapshot.actions.is_empty());
    assert!(snapshot.semantic.is_empty());
    assert!(activate_is_held(&runtime));

    key(&mut runtime, ButtonDriverInput::Release(KeyCode::Space));
    runtime.drive(Duration::ZERO).unwrap();
    let snapshot = runtime.world().resource::<FrameSnapshot>();
    assert!(snapshot.held_keys.is_empty());
    assert_eq!(snapshot.semantic.len(), 1);
    assert_eq!(snapshot.semantic[0].seat, InputSeatId::PRIMARY);
    assert_eq!(snapshot.semantic[0].phase, ActionPhase::Released);
    assert_eq!(snapshot.semantic[0].value, ActionValue::released());
    assert!(!activate_is_held(&runtime));
    runtime.finish().unwrap();
}

#[test]
fn explicit_ingress_refuses_unconnected_endpoints_without_observable_edges() {
    let mut runtime = runtime(InputLimits::new(32, 2, 2, 4).unwrap());
    let window = InputWindowId::new(9);
    let seat = InputSeatId::PRIMARY;
    let pointer = InputPointerId::new(5);
    let before = runtime.world().resource::<InputTimeline>().retained_len();
    assert!(
        input(&mut runtime, |scope| {
            apply_mouse_driver_input_at(
                scope,
                window,
                seat,
                pointer,
                ButtonDriverInput::Press(MouseButton::Left),
            )
        })
        .is_err()
    );
    assert_eq!(
        runtime.world().resource::<InputTimeline>().retained_len(),
        before
    );
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<FrameSnapshot>().mouse.is_empty());
    assert!(!activate_is_held(&runtime));

    input(&mut runtime, |scope| {
        connect_input_endpoint(scope, window, seat)?;
        connect_input_pointer(scope, window, seat, pointer)
    })
    .unwrap();
    input(&mut runtime, |scope| {
        apply_mouse_driver_input_at(
            scope,
            window,
            seat,
            pointer,
            ButtonDriverInput::Press(MouseButton::Left),
        )
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let snapshot = runtime.world().resource::<FrameSnapshot>();
    assert_eq!(snapshot.held_mouse, [MouseButton::Left]);
    assert_eq!(
        snapshot.mouse[0].provenance().route().pointer(),
        Some(pointer)
    );
    assert!(activate_is_held(&runtime));
    runtime.finish().unwrap();
}

#[test]
fn member_saturation_cannot_evict_a_hold_or_fabricate_a_release() {
    let limits = InputLimits::new(16, 1, 1, 1).unwrap();
    let mut runtime = runtime(limits);
    key(&mut runtime, ButtonDriverInput::Press(KeyCode::Space));
    runtime.drive(Duration::ZERO).unwrap();
    let before = runtime.world().resource::<InputTimeline>().cursors();
    assert!(
        input(&mut runtime, |scope| {
            apply_keyboard_driver_input(scope, ButtonDriverInput::Press(KeyCode::Enter))
        })
        .is_err()
    );
    assert_eq!(
        runtime.world().resource::<InputTimeline>().cursors(),
        before
    );
    assert!(key(&mut runtime, ButtonDriverInput::Release(KeyCode::Enter)).is_empty());
    runtime.drive(Duration::ZERO).unwrap();
    let snapshot = runtime.world().resource::<FrameSnapshot>();
    assert_eq!(snapshot.held_keys, [KeyCode::Space]);
    assert!(snapshot.keys.is_empty());
    assert!(snapshot.actions.is_empty());
    assert!(snapshot.semantic.is_empty());
    assert!(activate_is_held(&runtime));

    key(&mut runtime, ButtonDriverInput::Release(KeyCode::Space));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!activate_is_held(&runtime));
    key(&mut runtime, ButtonDriverInput::Press(KeyCode::Enter));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(activate_is_held(&runtime));
    let keys = runtime.world().resource::<ButtonInput<KeyCode>>();
    assert_eq!(keys.pressed.capacity(), limits.members());
    assert_eq!(keys.transitions.capacity(), limits.events());
    runtime.finish().unwrap();
}

#[test]
fn paused_frames_clear_edge_copies_while_the_timeline_keeps_action_ownership() {
    let limits = InputLimits::new(24, 1, 1, 4).unwrap();
    let mut runtime = runtime(limits);
    assert!(matches!(
        runtime.request_control(RuntimeControl::Pause),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    let mut accepted = 0;
    for _ in 0..limits.events() {
        let press = input(&mut runtime, |scope| {
            apply_keyboard_driver_input(scope, ButtonDriverInput::Press(KeyCode::Space))
        });
        if press.is_err() {
            break;
        }
        key(&mut runtime, ButtonDriverInput::Release(KeyCode::Space));
        runtime.drive(Duration::ZERO).unwrap();
        accepted += 1;
        assert_eq!(runtime.world().resource::<FrameSnapshot>().keys.len(), 2);
        let keys = runtime.world().resource::<ButtonInput<KeyCode>>();
        assert!(keys.transitions().is_empty());
        assert!(keys.pressed_buttons().next().is_none());
        assert!(!activate_is_held(&runtime));
    }
    assert!(accepted > 0 && accepted < limits.events());
    assert!(runtime.world().resource::<InputTimeline>().retained_len() > 0);
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<FrameSnapshot>().keys.is_empty());

    assert!(matches!(
        runtime.request_control(RuntimeControl::Resume),
        RuntimeControlRequestResult::Accepted(_)
    ));
    runtime.drive(Duration::ZERO).unwrap();
    let snapshot = runtime.world().resource::<FrameSnapshot>();
    assert_eq!(snapshot.actions.len(), 2 * accepted);
    assert_eq!(snapshot.semantic.len(), 2 * accepted);
    assert!(snapshot.keys.is_empty());
    assert!(!activate_is_held(&runtime));
    key(&mut runtime, ButtonDriverInput::Press(KeyCode::Space));
    runtime.drive(Duration::ZERO).unwrap();
    assert!(activate_is_held(&runtime));
    runtime.finish().unwrap();
}

#[test]
fn synthetic_pointer_and_mouse_use_the_same_once_connected_child() {
    let mut runtime = runtime(InputLimits::new(24, 1, 1, 2).unwrap());
    let position = Vec2::new(12.0, 24.0);
    input(&mut runtime, |scope| {
        apply_pointer_driver_input(scope, PointerDriverInput::Moved(position))?;
        apply_mouse_driver_input(scope, ButtonDriverInput::Press(MouseButton::Left))
    })
    .unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    let snapshot = runtime.world().resource::<FrameSnapshot>();
    assert_eq!(snapshot.position, Some(position));
    assert_eq!(snapshot.held_mouse, [MouseButton::Left]);
    let member = snapshot.mouse[0].press_sequence();
    assert_eq!(
        snapshot.mouse[0].provenance().origin(),
        InputEventOrigin::Synthetic
    );

    let (_, released) = input(&mut runtime, |scope| {
        apply_pointer_driver_input(scope, PointerDriverInput::Left)
    })
    .unwrap();
    assert_eq!(released, [MouseButton::Left]);
    runtime.drive(Duration::ZERO).unwrap();
    let snapshot = runtime.world().resource::<FrameSnapshot>();
    assert_eq!(snapshot.position, None);
    assert!(snapshot.held_mouse.is_empty());
    assert_eq!(snapshot.mouse.len(), 1);
    assert_eq!(snapshot.mouse[0].phase(), ButtonTransitionPhase::Released);
    assert_eq!(snapshot.mouse[0].press_sequence(), member);
    assert!(!activate_is_held(&runtime));
    runtime.finish().unwrap();
}

#[derive(Default, Resource)]
struct LateProjectionWitness {
    phase: u8,
    rejected: bool,
    observed_edges: usize,
}

fn admit_after_frame_projection(
    mut timeline: ResMut<InputTimeline>,
    mut witness: ResMut<LateProjectionWitness>,
) {
    if witness.phase != 0 {
        return;
    }
    for _ in 0..4 {
        timeline
            .admit_keyboard(
                InputWindowId::UNSPECIFIED,
                InputSeatId::PRIMARY,
                ButtonDriverInput::Press(KeyCode::Space),
            )
            .unwrap();
        timeline
            .admit_keyboard(
                InputWindowId::UNSPECIFIED,
                InputSeatId::PRIMARY,
                ButtonDriverInput::Release(KeyCode::Space),
            )
            .unwrap();
    }
    witness.phase = 1;
}

fn try_reuse_after_next_frame_resolution(
    mut timeline: ResMut<InputTimeline>,
    mut witness: ResMut<LateProjectionWitness>,
) {
    if witness.phase != 1 {
        return;
    }
    timeline
        .admit_window_event(
            InputWindowId::UNSPECIFIED,
            InputSeatId::PRIMARY,
            InputWindowEvent::Focused(false),
        )
        .unwrap();
    assert!(
        timeline.retained_len() >= 9,
        "the current frame copies keep their original late records admitted"
    );
    witness.rejected = timeline
        .admit_keyboard(
            InputWindowId::UNSPECIFIED,
            InputSeatId::PRIMARY,
            ButtonDriverInput::Press(KeyCode::Space),
        )
        .is_err();
    witness.phase = 2;
}

fn observe_late_projection(
    keys: Res<ButtonInput<KeyCode>>,
    mut witness: ResMut<LateProjectionWitness>,
) {
    if witness.phase == 2 {
        witness.observed_edges = keys.transitions().len();
    }
}

#[test]
fn late_records_reacquire_frame_custody_when_next_frame_builds_projection_copies() {
    let mut app = App::new();
    app.insert_resource(
        InputTimeline::with_limits(InputLimits::new(12, 1, 1, 4).unwrap()).unwrap(),
    )
    .unwrap();
    app.add_plugin(InputPlugin).unwrap();
    app.init_resource::<LateProjectionWitness>().unwrap();
    app.add_systems(CoreStage::Update, admit_after_frame_projection)
        .unwrap();
    app.add_systems(
        CoreStage::PreUpdate,
        try_reuse_after_next_frame_resolution.after(InputSet::ResolveActions),
    )
    .unwrap();
    app.add_systems(CoreStage::Last, observe_late_projection)
        .unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    runtime
        .drive(nara_app::FixedTime::DEFAULT_TIMESTEP)
        .unwrap();
    let witness = runtime.world().resource::<LateProjectionWitness>();
    let result = (witness.phase, witness.rejected, witness.observed_edges);
    runtime.finish().unwrap();
    assert_eq!(result, (2, true, 8));
}
