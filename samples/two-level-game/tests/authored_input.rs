#![cfg(any(feature = "development", feature = "cook"))]
#![forbid(unsafe_code)]

#[allow(
    dead_code,
    reason = "reuse the original game and physical close helper"
)]
#[path = "support/resume_gameplay.rs"]
mod support;

use std::{
    fs,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use nara::{
    ProductConfiguration,
    advanced_prelude::CurrentScene,
    app::{
        CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId, StartupStage,
    },
    ecs::{Resource, World, schedule::IntoScheduleConfigs},
    identity::{EntityLookup, SceneEntityId},
    input::{
        ActionContext, ActionMaps, ButtonDriverInput, InputSeatId, InputSet, InputTimeline,
        InputWindowEvent, InputWindowId, KeyCode,
    },
    sprite::Sprite,
};
use serde_json::{Value, json};
use support::{DEADLINE, Game, MARKER, ProjectCopy, TRAVEL, action, movement};
use two_level_game::{
    GamePhase, GameResumeError, capture_resume,
    input::{INPUT_PATH, movement_bindings},
    levels::LevelId,
    product_recipe, request_resume,
    resume::encode_resume,
};

const WINDOW: InputWindowId = InputWindowId::new(31);

#[derive(Clone, Copy)]
enum Operation {
    Key(KeyCode, bool),
    Context(bool),
    Focus(bool),
}

#[derive(Clone, Default, Resource)]
struct Driver(Arc<Mutex<Vec<Operation>>>);

impl ProductConfiguration for Driver {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"authored-input-game-test/v1");
    }
}

struct DriverPlugin(Driver);

impl Plugin for DriverPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test.authored-input"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::input::INPUT_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut nara::app::App) -> Result<(), PluginError> {
        app.insert_resource(self.0.clone())?;
        app.add_systems(StartupStage::Core, |world: &mut World| {
            world
                .resource_mut::<InputTimeline>()
                .connect_endpoint(WINDOW, InputSeatId::PRIMARY)
                .unwrap();
        })?;
        app.add_systems(CoreStage::First, inject.before(InputSet::ProjectTimeline))?;
        Ok(())
    }
}

fn inject(world: &mut World) {
    let operations = std::mem::take(&mut *world.resource::<Driver>().0.lock().unwrap());
    for operation in operations {
        match operation {
            Operation::Key(key, pressed) => {
                let input = if pressed {
                    ButtonDriverInput::Press(key)
                } else {
                    ButtonDriverInput::Release(key)
                };
                world
                    .resource_mut::<InputTimeline>()
                    .admit_keyboard(WINDOW, InputSeatId::PRIMARY, input)
                    .unwrap();
            }
            Operation::Context(enabled) => {
                world
                    .resource_mut::<ActionMaps>()
                    .get_mut(InputSeatId::PRIMARY)
                    .unwrap()
                    .set_context_enabled(&ActionContext::gameplay(), enabled)
                    .unwrap();
            }
            Operation::Focus(focused) => {
                world
                    .resource_mut::<InputTimeline>()
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

fn source(project: &ProjectCopy) -> Value {
    serde_json::from_slice(&fs::read(project.root().join(INPUT_PATH)).unwrap()).unwrap()
}

fn write(project: &ProjectCopy, value: &Value) {
    let path = project.root().join(INPUT_PATH);
    let path = path.canonicalize().unwrap();
    assert!(path.starts_with(project.root().canonicalize().unwrap()));
    assert!(!path.starts_with(fs::canonicalize(env!("CARGO_MANIFEST_DIR")).unwrap()));
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn rebound(project: &ProjectCopy) -> Value {
    let mut value = source(project);
    for binding in value["payload"]["bindings"].as_array_mut().unwrap() {
        if binding["input"].get("Key").is_none() {
            continue;
        }
        let input = match binding["action"].as_str().unwrap() {
            "two-level-game.move-a" => json!({"Key": "ArrowLeft"}),
            "two-level-game.move-d" => json!({"Key": "ArrowRight"}),
            "two-level-game.move-w" => json!({"Key": "ArrowUp"}),
            "two-level-game.move-s" => json!({"Key": "ArrowDown"}),
            "two-level-game.m" => json!({"Key": {"Character": "p"}}),
            "two-level-game.e" => json!({"Key": {"Character": "t"}}),
            _ => continue,
        };
        binding["input"] = input;
    }
    value
}

fn command(driver: &Driver, operation: Operation) {
    driver.0.lock().unwrap().push(operation);
}

fn driver_step(game: &mut Game) {
    game.maintenance();
    game.step();
}

fn actual_column(game: &Game) -> f32 {
    game.session
        .observe(|world| {
            let scene = CurrentScene::get(world).unwrap();
            let id = SceneEntityId::new("explorer").unwrap();
            let EntityLookup::Resolved(entity) = scene.receipt().resolve(world, &id) else {
                panic!("the current original Scene must resolve its explorer")
            };
            world
                .get::<Sprite>(entity)
                .unwrap()
                .texture_region
                .unwrap()
                .min
                .x
        })
        .unwrap()
}

#[test]
fn edited_keys_drive_original_commands_pause_collision_animation_and_context_lifetimes() {
    let project = ProjectCopy::new();
    write(&project, &rebound(&project));
    let driver = Driver::default();
    let recipe = product_recipe()
        .add_configured_plugin(driver.clone(), |driver| DriverPlugin(driver.clone()))
        .unwrap();
    let mut game = Game::from_project_with_recipe(project.root(), recipe);
    let bindings = game.session.observe(movement_bindings).unwrap().unwrap();
    assert_eq!(
        bindings.map(|binding| binding.key),
        [
            KeyCode::ArrowLeft,
            KeyCode::ArrowRight,
            KeyCode::ArrowUp,
            KeyCode::ArrowDown,
        ]
    );
    let original = game.snapshot();
    command(&driver, Operation::Key(KeyCode::Character('d'), true));
    game.maintenance();
    assert_eq!(game.snapshot().player, original.player);
    driver_step(&mut game);
    assert_eq!(game.snapshot().held, [false; 4]);
    assert_eq!(game.snapshot().player, original.player);
    command(&driver, Operation::Key(KeyCode::Character('d'), false));
    command(&driver, Operation::Key(KeyCode::ArrowRight, true));
    let before = game.snapshot();
    game.maintenance();
    assert_eq!(game.snapshot().logical_tick, before.logical_tick);
    assert_eq!(game.snapshot().player, before.player);
    assert_eq!(game.session.frame(Duration::from_secs(1)).unwrap(), None);
    assert_eq!(game.snapshot().player, before.player);
    driver_step(&mut game);
    assert!((game.snapshot().player.x - before.player.x - 3.2).abs() < 0.0001);
    assert_eq!(game.snapshot().held, [false, true, false, false]);
    assert_ne!(actual_column(&game), 0.0);

    command(&driver, Operation::Context(false));
    let position = game.snapshot().player;
    driver_step(&mut game);
    assert_eq!(game.snapshot().held, [false; 4]);
    assert_eq!(game.snapshot().player, position);
    assert!(game.session.observe(movement_bindings).unwrap().is_err());
    command(&driver, Operation::Context(true));
    driver_step(&mut game);
    assert_eq!(game.snapshot().held, [false; 4]);
    command(&driver, Operation::Key(KeyCode::ArrowRight, false));
    driver_step(&mut game);
    command(&driver, Operation::Key(KeyCode::ArrowRight, true));
    driver_step(&mut game);
    assert_eq!(game.snapshot().held, [false, true, false, false]);
    for _ in 0..64 {
        driver_step(&mut game);
    }
    let contact = game.snapshot().player;
    assert!((contact.x - -15.0101).abs() < 0.001, "{contact:?}");
    driver_step(&mut game);
    assert_eq!(game.snapshot().player, contact);
    assert_ne!(actual_column(&game), 0.0);
    command(&driver, Operation::Focus(false));
    driver_step(&mut game);
    assert_eq!(game.snapshot().held, [false; 4]);
    assert_eq!(actual_column(&game), 0.0);
    command(&driver, Operation::Focus(true));
    command(&driver, Operation::Key(KeyCode::Character('p'), true));
    driver_step(&mut game);
    assert_eq!(game.snapshot().markers.len(), 1);
    driver_step(&mut game);
    assert_eq!(game.snapshot().markers.len(), 1);
    command(&driver, Operation::Key(KeyCode::Character('p'), false));
    command(&driver, Operation::Key(KeyCode::Character('t'), true));
    driver_step(&mut game);
    assert_eq!(game.snapshot().level.unwrap().level(), LevelId::Cavern);
    let after = game.session.observe(movement_bindings).unwrap().unwrap();
    assert_eq!(after[1].key, KeyCode::ArrowRight);
    command(&driver, Operation::Key(KeyCode::Character('t'), false));
    driver_step(&mut game);
    game.command(action("two-level-game.retry"));
    assert_eq!(game.snapshot().level.unwrap().level(), LevelId::Cavern);
    assert_eq!(game.snapshot().markers.len(), 0);
    game.close();
}

#[test]
fn fresh_game_restores_unchanged_authored_input_with_nonfirst_animation_and_continues() {
    let project = ProjectCopy::new();
    write(&project, &rebound(&project));
    let mut original = Game::from_project(project.root());
    original.command(action(MARKER));
    original.command(movement(1, true));
    for _ in 0..5 {
        original.step();
    }
    let state = original.capture();
    let retained = original.frozen();
    assert_ne!(actual_column(&original), 0.0);
    let mut candidate = Game::from_project(project.root());
    request_resume(&mut candidate.session, &state).unwrap();
    candidate.settle();
    assert_eq!(
        encode_resume(&candidate.capture()).unwrap(),
        encode_resume(&state).unwrap()
    );
    assert_eq!(actual_column(&candidate), actual_column(&original));
    assert_eq!(original.frozen(), retained);
    assert_eq!(
        candidate
            .session
            .observe(movement_bindings)
            .unwrap()
            .unwrap()[1]
            .key,
        KeyCode::ArrowRight
    );
    for _ in 0..4 {
        original.step();
        candidate.step();
        assert_eq!(candidate.capture(), original.capture());
        assert_eq!(actual_column(&candidate), actual_column(&original));
    }
    for game in [&mut original, &mut candidate] {
        game.command(movement(1, false));
        game.command(action(TRAVEL));
    }
    assert_eq!(candidate.capture(), original.capture());
    candidate.close();
    original.close();
}

#[test]
fn changed_global_input_refuses_travel_and_old_resume_without_replacing_the_incumbent() {
    let project = ProjectCopy::new();
    let mut original = Game::from_project(project.root());
    original.command(action(MARKER));
    let state = original.capture();
    let retained = original.frozen();
    write(&project, &rebound(&project));
    original.maintenance();
    assert_eq!(original.frozen(), retained);
    let mut candidate = Game::from_project(project.root());
    assert_eq!(
        candidate
            .session
            .observe(movement_bindings)
            .unwrap()
            .unwrap()[1]
            .key,
        KeyCode::ArrowRight
    );
    let result = request_resume(&mut candidate.session, &state);
    match result {
        Err(GameResumeError::Ineligible(message)) => {
            assert!(message.contains("incompatible"), "{message}")
        }
        Err(error) => panic!("unexpected resume refusal: {error}"),
        Ok(()) => {
            let deadline = Instant::now() + DEADLINE;
            while candidate.snapshot().error.is_none() {
                assert!(Instant::now() < deadline);
                candidate.maintenance();
            }
        }
    }
    assert_eq!(original.frozen(), retained);
    candidate.close();
    original.submit(action(TRAVEL));
    assert_eq!(original.session.step().unwrap(), None);
    let deadline = Instant::now() + DEADLINE;
    while original.snapshot().error.is_none() {
        assert!(Instant::now() < deadline);
        original.maintenance();
    }
    let rejected = original.snapshot();
    assert_eq!(rejected.phase, GamePhase::Running);
    assert_eq!(rejected.current_instance, Some(retained.instance));
    assert_eq!(rejected.player.to_array(), state.player.translation);
    assert_eq!(rejected.markers.len(), 1);
    assert!(matches!(
        capture_resume(&original.session),
        Err(GameResumeError::Ineligible(_))
    ));
    original.submit(movement(1, true));
    original.command(action(MARKER));
    assert!(original.snapshot().player.x > state.player.translation[0]);
    original.command(movement(1, false));
    original.close();
}
