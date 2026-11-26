use std::time::Duration;

use nara_app::{App, CoreStage, FrameIntent, RuntimeInstance};
use nara_core::Vec2;
use nara_ecs::{Res, ResMut, Resource, World, schedule::IntoScheduleConfigs};
use nara_input::{
    ActionBinding, ActionContext, ActionEdge, ActionId, ActionMap, ActionMaps, ActionPhase,
    ActionState, AxisDirection, ButtonInput, ButtonTransitionPhase, GamepadButton,
    GamepadInputKind, InputEdge, InputEdgeKind, InputFrame, InputPlugin, InputPointerId,
    InputSeatId, InputSet, InputWindowEvent, InputWindowId, KeyCode, MouseButton,
};

#[derive(Default, Resource)]
struct Seen {
    input: Vec<InputEdge>,
    actions: Vec<ActionEdge>,
    movement: Vec<Vec2>,
}

fn key(key: KeyCode, phase: ButtonTransitionPhase) -> InputEdgeKind {
    InputEdgeKind::Key { key, phase }
}

fn action(name: &str) -> ActionId {
    ActionId::new(name).unwrap()
}

fn test_app(bindings: impl IntoIterator<Item = ActionBinding>) -> App {
    let mut app = App::new();
    app.add_plugin(InputPlugin).unwrap();
    app.insert_resource(ActionMaps::new(ActionMap::from_bindings(bindings).unwrap()))
        .unwrap();
    app.init_resource::<Seen>().unwrap();
    let route = |mut input: ResMut<InputFrame>, mut seen: ResMut<Seen>| {
        if input.is_resolved() {
            return;
        }
        seen.input.extend_from_slice(input.edges());
        for index in 0..input.edges().len() {
            if matches!(
                input.edges()[index].kind,
                InputEdgeKind::PointerButton {
                    button: MouseButton::Left,
                    ..
                } | InputEdgeKind::PointerButton {
                    phase: ButtonTransitionPhase::Released,
                    ..
                }
            ) {
                assert!(input.consume(index));
            }
        }
    };
    app.add_systems(CoreStage::First, route.in_set(InputSet::RouteUi))
        .unwrap();
    app.add_systems(FrameIntent, route.in_set(InputSet::RouteUi))
        .unwrap();
    app.add_systems(
        CoreStage::PreUpdate,
        keep_actions.after(InputSet::ResolveActions),
    )
    .unwrap();
    app.add_systems(
        CoreStage::FixedUpdate,
        |actions: Res<ActionState>, mut seen: ResMut<Seen>| {
            seen.movement.push(actions.axis_2d("move"));
        },
    )
    .unwrap();
    app
}

fn keep_actions(actions: Res<ActionState>, mut seen: ResMut<Seen>) {
    seen.actions.extend_from_slice(actions.edges());
}

fn write(runtime: &mut RuntimeInstance, operation: impl FnOnce(&mut World)) {
    let outcome = runtime
        .with_driver_scope(|scope| scope.with_world_mut(operation))
        .unwrap();
    assert!(outcome.post_operation_fault.is_none());
}

#[test]
fn mixed_edges_keep_arrival_positions_and_ui_consumes_mouse_without_hiding_physical_edges() {
    use ButtonTransitionPhase::{Pressed, Released};
    let mut app = test_app([
        ActionBinding::key(action("key"), KeyCode::Space),
        ActionBinding::mouse(action("click"), MouseButton::Left),
        ActionBinding::mouse(action("aim"), MouseButton::Right),
    ]);
    let pointer = InputPointerId::PRIMARY;
    let at_press = Vec2::new(3.0, 5.0);
    let at_release = Vec2::new(21.0, 13.0);
    let click = |button, phase| InputEdgeKind::PointerButton {
        pointer,
        button,
        phase,
        position: None,
    };
    for kind in [
        InputEdgeKind::PointerMoved {
            pointer,
            position: at_press,
        },
        click(MouseButton::Left, Pressed),
        key(KeyCode::Space, Pressed),
        InputEdgeKind::PointerMoved {
            pointer,
            position: at_release,
        },
        click(MouseButton::Left, Released),
        key(KeyCode::Space, Released),
        click(MouseButton::Right, Pressed),
        click(MouseButton::Right, Released),
    ] {
        app.world_mut()
            .unwrap()
            .write_message(InputEdge::primary(kind));
    }
    app.update_with_delta(Duration::ZERO).unwrap();
    let seen = app.world().resource::<Seen>();
    assert_eq!(seen.input.len(), 8);
    assert_eq!(
        seen.input[1].kind,
        InputEdgeKind::PointerButton {
            pointer,
            button: MouseButton::Left,
            phase: Pressed,
            position: Some(at_press),
        }
    );
    assert_eq!(
        seen.input[4].kind,
        InputEdgeKind::PointerButton {
            pointer,
            button: MouseButton::Left,
            phase: Released,
            position: Some(at_release),
        }
    );
    assert_eq!(
        seen.actions
            .iter()
            .map(|edge| (edge.action.as_str(), edge.phase))
            .collect::<Vec<_>>(),
        [
            ("key", ActionPhase::Started),
            ("key", ActionPhase::Released),
            ("aim", ActionPhase::Started),
            ("aim", ActionPhase::Released),
        ]
    );
    assert!(app.world().resource::<InputFrame>().edges().is_empty());
}

#[test]
fn exact_and_ordinary_fixed_steps_share_held_axis_values() {
    use ButtonTransitionPhase::{Pressed, Released};
    let mut app = test_app([
        ActionBinding::key_axis(action("move.x"), KeyCode::KeyD, AxisDirection::Positive),
        ActionBinding::key_axis(action("move.x"), KeyCode::KeyA, AxisDirection::Negative),
        ActionBinding::key_axis(action("move.y"), KeyCode::KeyW, AxisDirection::Positive),
    ]);
    for code in [KeyCode::KeyD, KeyCode::KeyW] {
        app.world_mut()
            .unwrap()
            .write_message(InputEdge::primary(key(code, Pressed)));
    }
    app.step_fixed(Duration::from_millis(16)).unwrap();
    app.step_fixed(Duration::from_millis(16)).unwrap();
    let mut runtime = RuntimeInstance::start(app).unwrap();
    write(&mut runtime, |world| {
        world.write_message(InputEdge::primary(key(KeyCode::KeyA, Pressed)));
    });
    runtime.drive(Duration::from_millis(50)).unwrap();
    assert_eq!(
        runtime.world().resource::<ActionState>().axis("move.x"),
        0.0
    );
    write(&mut runtime, |world| {
        world.write_message(InputEdge::primary(key(KeyCode::KeyD, Released)));
    });
    runtime.drive(Duration::from_millis(20)).unwrap();
    let seen = runtime.world().resource::<Seen>();
    assert_eq!(seen.movement[..2], [Vec2::ONE; 2]);
    assert!(seen.movement[2..seen.movement.len() - 1].contains(&Vec2::Y));
    assert_eq!(seen.movement.last(), Some(&Vec2::new(-1.0, 1.0)));
    assert_eq!(seen.input.len(), 4);
    runtime.finish().unwrap();
}

#[test]
fn focus_loss_and_gamepad_disconnect_clear_only_matching_controls() {
    use ButtonTransitionPhase::Pressed;
    let mut app = test_app([
        ActionBinding::key(action("confirm"), KeyCode::Enter),
        ActionBinding::gamepad_button(action("confirm"), GamepadButton::South.code()),
    ]);
    let seat = InputSeatId::new(1);
    let window = InputWindowId::new(7);
    let world = app.world_mut().unwrap();
    let map =
        ActionMap::from_bindings([ActionBinding::key(action("confirm"), KeyCode::Enter)]).unwrap();
    world
        .resource_mut::<ActionMaps>()
        .insert(seat, map)
        .unwrap();
    world.write_message(InputEdge::new(window, seat, key(KeyCode::Enter, Pressed)));
    world.write_message(InputEdge::primary(key(KeyCode::Enter, Pressed)));
    world.write_message(InputEdge::primary(InputEdgeKind::Gamepad {
        gamepad: 42,
        kind: GamepadInputKind::Button {
            button: GamepadButton::South,
            phase: Pressed,
        },
    }));
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(
        runtime
            .world()
            .resource::<ActionState>()
            .for_seat(seat)
            .button("confirm")
    );
    write(&mut runtime, |world| {
        world.write_message(InputEdge::new(
            window,
            InputSeatId::PRIMARY,
            InputEdgeKind::Window(InputWindowEvent::Focused(false)),
        ));
        world.write_message(InputEdge::primary(key(
            KeyCode::Enter,
            ButtonTransitionPhase::Released,
        )));
    });
    runtime.drive(Duration::ZERO).unwrap();
    let state = runtime.world().resource::<ActionState>();
    assert!(!state.for_seat(seat).button("confirm"));
    assert!(state.button("confirm"));
    assert!(
        !runtime
            .world()
            .resource::<ButtonInput<KeyCode>>()
            .pressed(KeyCode::Enter)
    );
    let edges = &runtime.world().resource::<Seen>().input;
    assert!(matches!(
        edges[3].kind,
        InputEdgeKind::Window(InputWindowEvent::Focused(false))
    ));
    assert_eq!(
        edges[4].kind,
        key(KeyCode::Enter, ButtonTransitionPhase::Released)
    );
    assert_eq!(edges[4].seat, seat);
    write(&mut runtime, |world| {
        world.write_message(InputEdge::primary(InputEdgeKind::Gamepad {
            gamepad: 42,
            kind: GamepadInputKind::Disconnected,
        }));
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!runtime.world().resource::<ActionState>().button("confirm"));
    runtime.finish().unwrap();
}

#[test]
fn replacing_a_map_or_restarting_its_context_clears_old_holds() {
    let bindings = [ActionBinding::key(action("pause"), KeyCode::Escape)];
    let mut app = test_app(bindings);
    app.world_mut()
        .unwrap()
        .write_message(InputEdge::primary(key(
            KeyCode::Escape,
            ButtonTransitionPhase::Pressed,
        )));
    let mut runtime = RuntimeInstance::start(app).unwrap();
    runtime.drive(Duration::ZERO).unwrap();
    assert!(runtime.world().resource::<ActionState>().button("pause"));
    write(&mut runtime, |world| {
        let mut maps = world.resource_mut::<ActionMaps>();
        let map = maps.get_mut(InputSeatId::PRIMARY).unwrap();
        let context = ActionContext::gameplay();
        map.disable_context(&context).unwrap();
        map.enable_context(&context).unwrap();
    });
    runtime.drive(Duration::ZERO).unwrap();
    assert!(!runtime.world().resource::<ActionState>().button("pause"));
    assert_eq!(
        runtime
            .world()
            .resource::<Seen>()
            .actions
            .last()
            .unwrap()
            .phase,
        ActionPhase::Released
    );
    runtime.finish().unwrap();
}
