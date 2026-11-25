#![forbid(unsafe_code)]
#![cfg(feature = "development")]

#[allow(
    dead_code,
    reason = "reuse the original game and physical close helper"
)]
#[path = "support/resume_gameplay.rs"]
mod support;

use std::sync::{Arc, Mutex};

use nara::{
    app::{
        CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId, StartupStage,
    },
    ecs::{Resource, World, schedule::IntoScheduleConfigs},
    input::{
        ActionContext, ActionId, ActionMapFileCandidate, ButtonDriverInput, ButtonTransitionPhase,
        GamepadConnection, InputBinding, InputEventKind, InputEventOrigin, InputSeatId, InputSet,
        InputTimeline, InputWindowEvent, InputWindowId, KeyCode, SemanticActions,
    },
};
use support::Game;
use two_level_game::{input::movement_bindings, levels::LevelId, product_recipe};

const WINDOW: InputWindowId = InputWindowId::new(1);
const RIGHT: &str = "two-level-game.move-d";

#[derive(Clone, Copy)]
enum Operation {
    Key(bool),
    Button(u16, ButtonTransitionPhase),
    Disconnect,
    Focus(bool),
}

#[derive(Clone, Default, Resource)]
struct Driver(Arc<Mutex<Vec<Operation>>>);

struct DriverPlugin(Driver);

#[derive(Resource)]
struct Connection(GamepadConnection);

impl Plugin for DriverPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test.controller-input"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::input::INPUT_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut nara::app::App) -> Result<(), PluginError> {
        app.insert_resource(self.0.clone())?;
        app.add_systems(StartupStage::Core, |world: &mut World| {
            let mut timeline = world.resource_mut::<InputTimeline>();
            timeline
                .connect_endpoint(WINDOW, InputSeatId::PRIMARY)
                .unwrap();
            let connection = timeline
                .connect_gamepad(WINDOW, InputSeatId::PRIMARY, 7)
                .unwrap();
            assert!(timeline.entries().any(|entry| {
                matches!(entry.kind(), InputEventKind::GamepadConnected { .. })
                    && entry.origin() == InputEventOrigin::Platform
                    && entry.gamepad_connection_generation()
                        == Some(connection.connection_generation())
            }));
            drop(timeline);
            world.insert_resource(Connection(connection));
        })?;
        app.add_systems(CoreStage::First, inject.before(InputSet::ProjectTimeline))?;
        Ok(())
    }
}

fn inject(world: &mut World) {
    let operations = std::mem::take(&mut *world.resource::<Driver>().0.lock().unwrap());
    let connection = world.resource::<Connection>().0.clone();
    let mut timeline = world.resource_mut::<InputTimeline>();
    for operation in operations {
        match operation {
            Operation::Key(pressed) => {
                let input = if pressed {
                    ButtonDriverInput::Press(KeyCode::Character('d'))
                } else {
                    ButtonDriverInput::Release(KeyCode::Character('d'))
                };
                timeline
                    .admit_keyboard(WINDOW, InputSeatId::PRIMARY, input)
                    .unwrap();
            }
            Operation::Button(button, phase) => {
                let admission = timeline
                    .admit_gamepad_button(&connection, button, phase)
                    .unwrap();
                if let Some(sequence) = admission.first_sequence() {
                    let entry = timeline
                        .entries()
                        .find(|entry| entry.sequence() == sequence)
                        .unwrap();
                    assert_eq!(entry.origin(), InputEventOrigin::Platform);
                    assert_eq!(
                        entry.gamepad_connection_generation(),
                        Some(connection.connection_generation()),
                    );
                }
            }
            Operation::Disconnect => {
                timeline.disconnect_gamepad(&connection).unwrap();
            }
            Operation::Focus(focused) => {
                timeline
                    .admit_window_event(
                        WINDOW,
                        InputSeatId::PRIMARY,
                        InputWindowEvent::Focused(focused),
                    )
                    .unwrap();
            }
        }
    }
}

fn button(action: &str, pressed: bool) -> Operation {
    let candidate =
        ActionMapFileCandidate::decode_json_bytes(include_bytes!("../input/explorer.actions.json"))
            .unwrap();
    let code = candidate
        .definition()
        .bindings()
        .iter()
        .find_map(|binding| match binding.input {
            InputBinding::GamepadButton(code) if binding.action.as_str() == action => Some(code),
            _ => None,
        })
        .expect("authored controller Button alternative");
    Operation::Button(
        code,
        if pressed {
            ButtonTransitionPhase::Pressed
        } else {
            ButtonTransitionPhase::Released
        },
    )
}

fn game() -> (Game, Driver) {
    let driver = Driver::default();
    let plugin_driver = driver.clone();
    let recipe = product_recipe()
        .add_configured_plugin(b"two-level-game.controller-input-test/v1", move || {
            DriverPlugin(plugin_driver.clone())
        })
        .unwrap();
    (Game::with_recipe(recipe), driver)
}

fn command(driver: &Driver, operation: Operation) {
    driver.0.lock().unwrap().push(operation);
}

fn driver_step(game: &mut Game) {
    game.maintenance();
    game.step();
}

fn assert_pressed(game: &Game, action: &str, pressed: bool) {
    assert_eq!(
        game.session
            .observe(|world| {
                world.resource::<SemanticActions>().pressed(
                    InputSeatId::PRIMARY,
                    &ActionId::new(action).unwrap(),
                    &ActionContext::gameplay(),
                )
            })
            .unwrap(),
        pressed,
    );
}

#[test]
fn normalized_platform_dpad_press_and_release_move_the_real_game_in_all_four_directions() {
    let (mut game, driver) = game();
    assert!(game.session.observe(movement_bindings).unwrap().is_ok());
    for (direction, action, delta) in [
        (0, "two-level-game.move-a", [-3.2, 0.0]),
        (1, RIGHT, [3.2, 0.0]),
        (2, "two-level-game.move-w", [0.0, 3.2]),
        (3, "two-level-game.move-s", [0.0, -3.2]),
    ] {
        let before = game.snapshot().player;
        let accepted = game.queue().accepted;
        command(&driver, button(action, true));
        driver_step(&mut game);
        let moving = game.snapshot();
        let mut held = [false; 4];
        held[direction] = true;
        assert_eq!(moving.held, held);
        assert!((moving.player.x - before.x - delta[0]).abs() < 0.0001);
        assert!((moving.player.y - before.y - delta[1]).abs() < 0.0001);
        assert_eq!(game.queue().accepted, accepted + 1);
        assert_pressed(&game, action, true);
        command(&driver, button(action, false));
        driver_step(&mut game);
        assert_eq!(game.snapshot().held, [false; 4]);
        assert_eq!(game.snapshot().player, moving.player);
        assert_eq!(game.queue().accepted, accepted + 2);
        assert_pressed(&game, action, false);
    }
    game.close();
}

#[test]
fn releasing_either_keyboard_or_dpad_keeps_the_other_original_contribution_moving() {
    for release_keyboard_first in [true, false] {
        let (mut game, driver) = game();
        let accepted = game.queue().accepted;
        command(&driver, Operation::Key(true));
        command(&driver, button(RIGHT, true));
        driver_step(&mut game);
        assert_eq!(game.snapshot().held, [false, true, false, false]);
        assert_eq!(game.queue().accepted, accepted + 1);
        let position = game.snapshot().player;
        let (first, last) = if release_keyboard_first {
            (Operation::Key(false), button(RIGHT, false))
        } else {
            (button(RIGHT, false), Operation::Key(false))
        };
        command(&driver, first);
        driver_step(&mut game);
        assert_eq!(game.snapshot().held, [false, true, false, false]);
        assert!((game.snapshot().player.x - position.x - 3.2).abs() < 0.0001);
        assert_eq!(game.queue().accepted, accepted + 1);
        assert_pressed(&game, RIGHT, true);
        let position = game.snapshot().player;
        command(&driver, last);
        driver_step(&mut game);
        assert_eq!(game.snapshot().held, [false; 4]);
        assert_eq!(game.snapshot().player, position);
        assert_eq!(game.queue().accepted, accepted + 2);
        assert_pressed(&game, RIGHT, false);
        game.close();
    }
}

#[test]
fn disconnect_and_focus_loss_neutralize_the_original_accepted_controller_hold() {
    for terminal in [Operation::Disconnect, Operation::Focus(false)] {
        let (mut game, driver) = game();
        command(&driver, button(RIGHT, true));
        driver_step(&mut game);
        assert_eq!(game.snapshot().held, [false, true, false, false]);
        assert_pressed(&game, RIGHT, true);
        let position = game.snapshot().player;
        let accepted = game.queue().accepted;
        command(&driver, terminal);
        driver_step(&mut game);
        assert_eq!(game.snapshot().held, [false; 4]);
        assert_eq!(game.snapshot().player, position);
        assert_eq!(game.queue().accepted, accepted + 1);
        assert_pressed(&game, RIGHT, false);
        driver_step(&mut game);
        assert_eq!(game.snapshot().player, position);
        assert_eq!(game.queue().accepted, accepted + 1);
        command(&driver, Operation::Focus(true));
        driver_step(&mut game);
        assert_eq!(game.snapshot().player, position);
        command(&driver, Operation::Key(true));
        driver_step(&mut game);
        assert_eq!(game.snapshot().held, [false, true, false, false]);
        assert!(game.snapshot().player.x > position.x);
        command(&driver, Operation::Key(false));
        driver_step(&mut game);
        game.close();
    }
}

#[test]
fn controller_buttons_place_one_marker_per_press_and_travel_through_the_original_game() {
    let (mut game, driver) = game();
    let before = game.snapshot();
    command(&driver, button("two-level-game.m", true));
    driver_step(&mut game);
    assert_eq!(game.snapshot().markers.len(), before.markers.len() + 1);
    let markers = game.snapshot().markers;
    driver_step(&mut game);
    assert_eq!(game.snapshot().markers, markers);
    command(&driver, button("two-level-game.m", false));
    command(&driver, button("two-level-game.e", true));
    driver_step(&mut game);
    let traveled = game.snapshot();
    assert_eq!(traveled.level.unwrap().level(), LevelId::Cavern);
    assert_ne!(traveled.current_instance, before.current_instance);
    assert_eq!(traveled.travel_count, before.travel_count + 1);
    command(&driver, button("two-level-game.e", false));
    command(&driver, button("two-level-game.m", true));
    driver_step(&mut game);
    assert_eq!(game.snapshot().markers.len(), traveled.markers.len() + 1);
    command(&driver, button("two-level-game.m", false));
    driver_step(&mut game);
    assert_eq!(game.snapshot().held, [false; 4]);
    game.close();
}

#[test]
fn default_authored_gamepad_alternatives_preserve_actual_keyboard_capture_eligibility() {
    let (mut game, driver) = game();
    let capture = |game: &Game| {
        game.session
            .observe(|world| {
                nara::input::capture_keyboard_holds(
                    world,
                    WINDOW,
                    InputSeatId::PRIMARY,
                    &movement_bindings(world).unwrap(),
                )
            })
            .unwrap()
    };
    assert!(capture(&game).unwrap().iter().all(|hold| !hold.held));
    command(&driver, Operation::Key(true));
    driver_step(&mut game);
    let holds = capture(&game).unwrap();
    assert_eq!(
        holds.iter().map(|hold| hold.held).collect::<Vec<_>>(),
        [false, true, false, false]
    );
    command(&driver, button(RIGHT, true));
    driver_step(&mut game);
    assert_eq!(
        capture(&game),
        Err(nara::input::KeyboardContinuationError::UnsupportedMembers)
    );
    command(&driver, button(RIGHT, false));
    driver_step(&mut game);
    assert_eq!(capture(&game).unwrap(), holds);
    game.close();
}
