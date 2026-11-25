//! Public CPU acceptance for the opt-in source game's round and menu policy.

#![cfg(any(feature = "development", feature = "cook"))]
#![forbid(unsafe_code)]

#[allow(
    dead_code,
    reason = "reuse the existing isolated source-project fixture"
)]
#[path = "support/resume_gameplay.rs"]
mod support;

use std::{
    fs,
    path::Path,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[cfg(feature = "gamepad")]
use nara::input::{ButtonTransitionPhase, GamepadButton, GamepadConnection};
use nara::{
    ProductRecipe,
    advanced_prelude::CurrentScene,
    app::{
        AppExit, CoreStage, FixedTime, Plugin, PluginCategory, PluginDeclaration, PluginError,
        PluginId, RuntimeState, RuntimeTimeSettings, StartupStage, VirtualTime,
    },
    ecs::{Resource, World, schedule::IntoScheduleConfigs},
    identity::{EntityLookup, SceneEntityId, SceneInstanceId},
    input::{
        ActionBinding, ActionContext, ActionId, ActionMapFileCandidate, ActionMaps,
        ButtonDriverInput, InputSeatId, InputSet, InputTimeline, InputWindowId, KeyCode,
    },
    prelude::{Transform2d, Vec2},
    project_host::{HeadlessSession, HeadlessSessionClose},
    sprite::{Sprite, TextureRegion},
    tasks::{TaskPools, TaskReturnState},
};
use support::{DEADLINE, ProjectCopy};
use two_level_game::{
    GamePhase, GameResumeError, GameSnapshot, RunOptions,
    animation::ExplorerFrame,
    capture_resume,
    command::{
        CommandSchedule, CommandSequence, CommandSource, CommandStats, CommandTick,
        ScheduledCommand,
    },
    configured_recipe,
    flow::{FlowBindings, FlowPhase, FlowSnapshot, PlayFlowControls, with_play_flow},
    levels::{AUTHORED_LEVEL_MEMBERS, LevelId, LevelKey},
    open_explicit_project_root, product_recipe,
};

const WINDOW: InputWindowId = InputWindowId::new(37);
const STEP: Duration = Duration::from_millis(20);
const MENU: &str = "two-level-game.menu";
const RIGHT: KeyCode = KeyCode::Character('d');

#[derive(Clone, Default, Resource)]
struct Driver(Arc<Mutex<Vec<DriverInput>>>);

enum DriverInput {
    Key(KeyCode, bool),
    #[cfg(feature = "gamepad")]
    Button(usize, GamepadButton, bool),
    #[cfg(feature = "gamepad")]
    Disconnect(usize),
}

impl Driver {
    fn key(&self, key: KeyCode, pressed: bool) {
        self.0.lock().unwrap().push(DriverInput::Key(key, pressed));
    }

    fn tap(&self, key: KeyCode) {
        let mut pending = self.0.lock().unwrap();
        pending.extend([DriverInput::Key(key, true), DriverInput::Key(key, false)]);
    }

    #[cfg(feature = "gamepad")]
    fn button(&self, button: GamepadButton, pressed: bool) {
        self.controller_button(0, button, pressed);
    }

    #[cfg(feature = "gamepad")]
    fn controller_button(&self, controller: usize, button: GamepadButton, pressed: bool) {
        self.0
            .lock()
            .unwrap()
            .push(DriverInput::Button(controller, button, pressed));
    }

    #[cfg(feature = "gamepad")]
    fn tap_button(&self, button: GamepadButton) {
        self.0.lock().unwrap().extend([
            DriverInput::Button(0, button, true),
            DriverInput::Button(0, button, false),
        ]);
    }

    #[cfg(feature = "gamepad")]
    fn disconnect(&self, controller: usize) {
        self.0
            .lock()
            .unwrap()
            .push(DriverInput::Disconnect(controller));
    }
}

struct DriverPlugin(Driver);

#[cfg(feature = "gamepad")]
#[derive(Default, Resource)]
struct Connections([Option<GamepadConnection>; 2]);

#[derive(Default, Resource)]
struct ExplorerAtExtract(Option<(FlowPhase, TextureRegion)>);

impl Plugin for DriverPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("two-level-game.test.play-flow"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara::input::INPUT_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut nara::app::App) -> Result<(), PluginError> {
        app.insert_resource(self.0.clone())?;
        app.init_resource::<ExplorerAtExtract>()?;
        #[cfg(feature = "gamepad")]
        app.init_resource::<Connections>()?;
        app.add_systems(StartupStage::Core, |world: &mut World| {
            world
                .resource_mut::<InputTimeline>()
                .connect_endpoint(WINDOW, InputSeatId::PRIMARY)
                .unwrap();
        })?;
        app.add_systems(CoreStage::First, inject.before(InputSet::ProjectTimeline))?;
        app.add_systems(CoreStage::Extract, observe_explorer_at_extract)?;
        Ok(())
    }
}

fn observe_explorer_at_extract(world: &mut World) {
    let explorer = world
        .get_resource::<FlowSnapshot>()
        .and_then(|flow| explorer_region(world).map(|region| (flow.phase, region)));
    world.resource_mut::<ExplorerAtExtract>().0 = explorer;
}

fn explorer_region(world: &World) -> Option<TextureRegion> {
    let current = CurrentScene::get(world)?;
    let id = SceneEntityId::new("explorer").unwrap();
    let EntityLookup::Resolved(entity) = current.receipt().resolve(world, &id) else {
        return None;
    };
    world.get::<Sprite>(entity)?.texture_region
}

fn inject(world: &mut World) {
    let inputs = std::mem::take(&mut *world.resource::<Driver>().0.lock().unwrap());
    for input in inputs {
        match input {
            DriverInput::Key(key, pressed) => {
                world
                    .resource_mut::<InputTimeline>()
                    .admit_keyboard(
                        WINDOW,
                        InputSeatId::PRIMARY,
                        if pressed {
                            ButtonDriverInput::Press(key)
                        } else {
                            ButtonDriverInput::Release(key)
                        },
                    )
                    .unwrap();
            }
            #[cfg(feature = "gamepad")]
            DriverInput::Button(controller, button, pressed) => {
                if world.resource::<Connections>().0[controller].is_none() {
                    let connection = world
                        .resource_mut::<InputTimeline>()
                        .connect_gamepad(WINDOW, InputSeatId::PRIMARY, 7 + controller as u16)
                        .unwrap();
                    world.resource_mut::<Connections>().0[controller] = Some(connection);
                }
                let connection = world.resource::<Connections>().0[controller]
                    .as_ref()
                    .unwrap()
                    .clone();
                world
                    .resource_mut::<InputTimeline>()
                    .admit_gamepad_button(
                        &connection,
                        button.code(),
                        if pressed {
                            ButtonTransitionPhase::Pressed
                        } else {
                            ButtonTransitionPhase::Released
                        },
                    )
                    .unwrap();
            }
            #[cfg(feature = "gamepad")]
            DriverInput::Disconnect(controller) => {
                let connection = world.resource_mut::<Connections>().0[controller]
                    .take()
                    .unwrap();
                world
                    .resource_mut::<InputTimeline>()
                    .disconnect_gamepad(&connection)
                    .unwrap();
            }
        }
    }
}

/// Retains and physically closes the original Session, including during assertion unwinding.
struct Game {
    session: HeadlessSession,
    driver: Driver,
}

impl Game {
    fn source() -> Self {
        Self::from_project(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            keyboard_flow_recipe(),
        )
    }

    fn from_project(root: &Path, recipe: ProductRecipe) -> Self {
        let driver = Driver::default();
        let plugin_driver = driver.clone();
        let recipe = recipe
            .add_configured_plugin(b"two-level-game.play-flow-test/v1", move || {
                DriverPlugin(plugin_driver.clone())
            })
            .unwrap();
        let mut game = Self {
            session: HeadlessSession::new(open_explicit_project_root(root).unwrap(), recipe),
            driver,
        };
        game.session
            .start()
            .expect("ordinary public source startup");
        assert_eq!(game.session.state(), Some(RuntimeState::Running));
        assert_eq!(game.runtime_clock(), (0, Duration::ZERO));
        game
    }

    fn flow(&self) -> FlowSnapshot {
        self.session
            .observe(|world| world.resource::<FlowSnapshot>().clone())
            .unwrap()
    }

    fn snapshot(&self) -> GameSnapshot {
        self.session
            .observe(|world| world.resource::<GameSnapshot>().clone())
            .unwrap()
    }

    fn runtime_clock(&self) -> (u64, Duration) {
        self.session
            .observe(|world| {
                (
                    world.resource::<FixedTime>().tick(),
                    world.resource::<VirtualTime>().elapsed,
                )
            })
            .unwrap()
    }

    fn clocks(&self) -> (u64, Duration, u64, u64) {
        let (tick, elapsed) = self.runtime_clock();
        let game = self.snapshot();
        (tick, elapsed, game.logical_tick, game.logical_elapsed_nanos)
    }

    fn time_policy(&self) -> (bool, f32) {
        self.session
            .observe(|world| {
                let time = world.resource::<RuntimeTimeSettings>();
                (time.paused(), time.time_scale())
            })
            .unwrap()
    }

    fn instance(&self) -> SceneInstanceId {
        self.session
            .observe(|world| {
                let current = CurrentScene::get(world).expect("the original current Scene");
                assert!(current.source_content_stamp().is_some());
                assert!(current.source_view().with_runtime_scene(|_| ()).is_none());
                assert_eq!(current.receipt().entity_ids().len(), AUTHORED_LEVEL_MEMBERS);
                current.receipt().instance_id()
            })
            .unwrap()
    }

    fn object(&self, id: &str) -> (Vec2, f32) {
        self.session
            .observe(|world| {
                let current = CurrentScene::get(world).unwrap();
                let id = SceneEntityId::new(id).unwrap();
                let EntityLookup::Resolved(entity) = current.receipt().resolve(world, &id) else {
                    panic!("the original current Scene must resolve {id}")
                };
                (
                    world.get::<Transform2d>(entity).unwrap().translation,
                    world.get::<Sprite>(entity).unwrap().material.tint.a,
                )
            })
            .unwrap()
    }

    fn explorer_frame(&self) -> TextureRegion {
        self.session
            .observe(|world| {
                explorer_region(world).expect("the current Scene has an animated explorer")
            })
            .unwrap()
    }

    fn explorer_at_extract(&self) -> (FlowPhase, TextureRegion) {
        self.session
            .observe(|world| {
                world
                    .resource::<ExplorerAtExtract>()
                    .0
                    .expect("the frame extracted an explorer")
            })
            .unwrap()
    }

    fn gameplay_enabled(&self) -> bool {
        self.session
            .observe(|world| {
                world
                    .resource::<ActionMaps>()
                    .get(InputSeatId::PRIMARY)
                    .unwrap()
                    .is_context_enabled(&ActionContext::gameplay())
            })
            .unwrap()
    }

    fn queue(&self) -> CommandStats {
        self.session
            .observe(|world| world.resource::<CommandSchedule>().stats())
            .unwrap()
    }

    fn assert_idle(&self) {
        self.session
            .observe(|world| {
                let queue = world.resource::<CommandSchedule>();
                let stats = queue.stats();
                assert!(queue.is_idle(), "{stats:?}");
                assert!(queue.current().is_empty());
                assert_eq!(stats.accepted, stats.admitted);
                assert_eq!(stats.accepted, stats.acknowledged);
                assert_eq!(
                    stats.acknowledged_through_tick,
                    world.resource::<FixedTime>().tick()
                );
                assert_eq!(stats.closed_through_tick, stats.acknowledged_through_tick);
                assert_eq!(stats.retained_commands, 0);
                assert_eq!(stats.rejected, 0);
            })
            .unwrap();
    }

    fn frame(&mut self, delta: Duration) {
        assert_eq!(self.session.frame(delta).unwrap(), None);
        assert_eq!(self.snapshot().tick, self.runtime_clock().0);
    }

    fn tap(&mut self, key: KeyCode) {
        self.driver.tap(key);
        self.frame(Duration::ZERO);
    }

    fn begin_round(&mut self) {
        let old = self.instance();
        self.tap(KeyCode::Enter);
        assert_eq!(self.flow().phase, FlowPhase::Starting);
        assert_eq!(self.snapshot().phase, GamePhase::Preparing);
        assert_eq!(
            self.instance(),
            old,
            "confirm must begin original Scene preparation"
        );
        assert_eq!(self.time_policy(), (false, 0.0));
        assert!(!self.gameplay_enabled());
    }

    fn settle_start(&mut self, expected: FlowPhase) {
        let clock = self.runtime_clock();
        let deadline = Instant::now() + DEADLINE;
        while self.flow().phase == FlowPhase::Starting {
            assert_eq!(self.time_policy(), (false, 0.0));
            assert!(
                Instant::now() < deadline,
                "source Scene did not settle through TaskUpdate: {:?}; {:?}",
                self.flow(),
                self.snapshot(),
            );
            self.frame(Duration::ZERO);
            assert_eq!(
                self.runtime_clock(),
                clock,
                "Scene work must not require simulation time"
            );
            if self.flow().phase == FlowPhase::Starting {
                std::thread::sleep(Duration::from_millis(1));
            }
        }
        assert_eq!(
            self.flow().phase,
            expected,
            "{:?}; {:?}",
            self.flow(),
            self.snapshot()
        );
    }

    fn finish_resume(&mut self) {
        assert_eq!(self.flow().phase, FlowPhase::Resuming);
        assert_eq!(self.time_policy(), (false, 1.0));
        assert!(!self.gameplay_enabled());
        let before = self.snapshot();
        let clock = self.clocks();
        self.frame(Duration::ZERO);
        assert_eq!(self.flow().phase, FlowPhase::Resuming);
        assert_eq!(
            self.clocks(),
            clock,
            "a frame without a fixed batch cannot finish resume"
        );
        self.frame(STEP);
        assert_eq!(self.runtime_clock().0, clock.0 + 1);
        assert_eq!(self.flow().phase, FlowPhase::Playing);
        assert_eq!(self.snapshot().phase, GamePhase::Running);
        assert_eq!(self.snapshot().logical_tick, before.logical_tick);
        assert_eq!(
            self.snapshot().logical_elapsed_nanos,
            before.logical_elapsed_nanos
        );
        assert_eq!(self.snapshot().player, before.player);
        assert_eq!(self.object("explorer").0, before.player);
        assert_eq!(self.snapshot().held, [false; 4]);
        assert!(self.gameplay_enabled());
        assert!(self.flow().error.is_none(), "{:?}", self.flow());
        assert!(self.snapshot().error.is_none(), "{:?}", self.snapshot());
        self.assert_idle();
    }

    fn start_round(&mut self) {
        self.begin_round();
        self.settle_start(FlowPhase::Resuming);
        self.finish_resume();
    }

    fn command(&mut self, key: char) {
        self.tap(KeyCode::Character(key));
        self.frame(STEP);
        assert!(self.snapshot().error.is_none(), "{:?}", self.snapshot());
    }

    fn settle_level(&mut self, level: LevelId) {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let snapshot = self.snapshot();
            assert!(snapshot.error.is_none(), "{snapshot:?}");
            let returned = self
                .session
                .observe(|world| world.resource::<TaskPools>().return_state())
                .unwrap();
            assert_ne!(returned, TaskReturnState::CleanupFailed);
            if snapshot.phase == GamePhase::Running
                && snapshot.level == Some(LevelKey::main(level))
                && returned == TaskReturnState::Returned
            {
                assert_eq!(snapshot.current_instance, Some(self.instance()));
                self.assert_idle();
                return;
            }
            assert!(
                Instant::now() < deadline,
                "original level travel did not settle: {snapshot:?}"
            );
            self.frame(Duration::ZERO);
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[cfg(feature = "gamepad")]
    fn toggle_room(
        &mut self,
        button: GamepadButton,
        room: impl Fn(&GameSnapshot) -> two_level_game::SideRoomPhase,
        expected: two_level_game::SideRoomPhase,
    ) {
        self.driver.tap_button(button);
        self.frame(STEP);
        let deadline = Instant::now() + DEADLINE;
        while room(&self.snapshot()) == two_level_game::SideRoomPhase::Preparing {
            assert!(
                Instant::now() < deadline,
                "gamepad {button:?} room request did not settle"
            );
            self.frame(Duration::ZERO);
            std::thread::sleep(Duration::from_millis(1));
        }
        let snapshot = self.snapshot();
        assert!(snapshot.error.is_none(), "{snapshot:?}");
        assert_eq!(room(&snapshot), expected);
        assert_eq!(self.flow().phase, FlowPhase::Playing);
    }

    fn collect_near_pickup(&mut self) {
        self.collect_near_pickup_with(|driver, pressed| driver.key(RIGHT, pressed));
    }

    fn collect_near_pickup_with(&mut self, drive_right: impl Fn(&Driver, bool)) {
        let collected = self.snapshot().collected;
        assert_eq!(self.object("gem-near").1, 1.0);
        drive_right(&self.driver, true);
        for _ in 0..64 {
            self.frame(STEP);
            let snapshot = self.snapshot();
            assert!(snapshot.error.is_none(), "{snapshot:?}");
            if snapshot.collected == collected + 1 {
                assert!(
                    self.object("explorer")
                        .0
                        .distance(self.object("gem-near").0)
                        <= 16.0
                );
                assert_eq!(self.object("gem-near").1, 0.0);
                if self.flow().phase == FlowPhase::Complete {
                    assert_eq!(snapshot.held, [false; 4]);
                    assert_eq!(
                        self.explorer_at_extract(),
                        (FlowPhase::Complete, ExplorerFrame::Idle.texture_region())
                    );
                }
                drive_right(&self.driver, false);
                self.frame(Duration::ZERO);
                return;
            }
            assert_eq!(self.flow().phase, FlowPhase::Playing);
        }
        panic!(
            "actual movement did not reach the authored near pickup: {:?}",
            self.snapshot()
        );
    }

    fn close_terminal(&mut self) -> HeadlessSessionClose {
        let deadline = Instant::now() + DEADLINE;
        loop {
            let state = self.session.close();
            if state != HeadlessSessionClose::Pending || Instant::now() >= deadline {
                return state;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}

impl Drop for Game {
    fn drop(&mut self) {
        let terminal = self.close_terminal();
        if std::thread::panicking() {
            if terminal != HeadlessSessionClose::Complete || !self.session.is_closed() {
                eprintln!(
                    "play-flow cleanup: {terminal:?}; {:?}",
                    self.session.diagnostics()
                );
            }
        } else {
            assert_eq!(
                terminal,
                HeadlessSessionClose::Complete,
                "{:?}",
                self.session.diagnostics()
            );
            assert!(self.session.is_closed());
            assert_eq!(self.session.close(), HeadlessSessionClose::Complete);
        }
    }
}

fn keyboard_flow_recipe() -> ProductRecipe {
    with_play_flow(product_recipe(), PlayFlowControls::Keyboard).unwrap()
}

fn assert_zero_round(game: &Game, round: u32) {
    let flow = game.flow();
    let snapshot = game.snapshot();
    assert_eq!(flow.round, round);
    assert!(!flow.meadow_collected);
    assert!(!flow.cavern_collected);
    assert!(flow.error.is_none(), "{flow:?}");
    assert!(snapshot.initialized);
    assert_eq!(snapshot.phase, GamePhase::Running);
    assert_eq!(snapshot.level, Some(LevelKey::main(LevelId::Meadow)));
    assert_eq!(snapshot.current_instance, Some(game.instance()));
    assert_eq!(snapshot.player, Vec2::new(-120.0, 0.0));
    assert_eq!(game.object("explorer").0, snapshot.player);
    assert_eq!(snapshot.score, 0);
    assert_eq!(snapshot.collected, 0);
    assert_eq!(snapshot.visible_pickups, 2);
    assert!(snapshot.markers.is_empty());
    assert_eq!(snapshot.travel_count, 0);
    assert_eq!(snapshot.restarts, 0);
    assert_eq!(snapshot.saves, 0);
    assert_eq!(snapshot.restores, 0);
    assert_eq!(snapshot.logical_tick, 0);
    assert_eq!(snapshot.logical_elapsed_nanos, 0);
    assert_eq!(snapshot.held, [false; 4]);
    assert!(!snapshot.restoring);
    assert!(snapshot.error.is_none(), "{snapshot:?}");
}

#[test]
fn title_freezes_before_the_first_frame_and_new_round_prepares_a_real_source_scene_at_zero_scale() {
    {
        let mut default =
            Game::from_project(Path::new(env!("CARGO_MANIFEST_DIR")), product_recipe());
        assert!(
            default
                .session
                .observe(|world| world.get_resource::<FlowSnapshot>().is_none())
                .unwrap()
        );
        assert_eq!(default.time_policy(), (false, 1.0));
        assert!(default.gameplay_enabled());
        let spawn = default.object("explorer").0;
        default.driver.key(RIGHT, true);
        default.frame(STEP);
        assert_eq!(default.snapshot().logical_tick, 1);
        assert_eq!(default.snapshot().held, [false, true, false, false]);
        assert!((default.snapshot().player.x - spawn.x - 3.2).abs() < 0.0001);
        default.driver.key(RIGHT, false);
        default.command('m');
        assert_eq!(default.snapshot().markers.len(), 1);
        default.assert_idle();
    }

    let mut game = Game::source();
    assert_eq!(game.flow().phase, FlowPhase::Title);
    assert_eq!(game.flow().round, 0);
    assert_eq!(game.time_policy(), (false, 0.0));
    assert_eq!(game.runtime_clock(), (0, Duration::ZERO));
    let original = game.instance();
    let generation = game.session.runtime_generation().unwrap();

    let authored =
        ActionMapFileCandidate::decode_json_bytes(include_bytes!("../input/explorer.actions.json"))
            .unwrap();
    game.session
        .observe(|world| {
            let maps = world.resource::<ActionMaps>();
            let map = maps.get(InputSeatId::PRIMARY).unwrap();
            for binding in authored.definition().bindings() {
                assert!(
                    map.bindings().contains(binding),
                    "the derived map must retain {binding:?}"
                );
            }
            assert_eq!(
                map.bindings().len(),
                authored.definition().bindings().len() + 4
            );
            let menu = ActionContext::new(MENU).unwrap();
            assert!(map.is_context_enabled(&menu));
            assert!(!map.is_context_enabled(&ActionContext::gameplay()));
            for (name, key) in [
                ("confirm", KeyCode::Enter),
                ("pause", KeyCode::Escape),
                ("title", KeyCode::Character('t')),
                ("exit", KeyCode::Character('x')),
            ] {
                let binding = ActionBinding::key(
                    ActionId::new(format!("two-level-game.menu.{name}")).unwrap(),
                    key,
                )
                .with_context(menu.clone());
                assert!(map.bindings().contains(&binding));
            }
        })
        .unwrap();

    for key in [RIGHT, KeyCode::Character('m'), KeyCode::Character('e')] {
        game.driver.tap(key);
    }
    game.frame(Duration::from_secs(1));
    assert_eq!(game.flow().phase, FlowPhase::Title);
    assert_eq!(game.clocks(), (0, Duration::ZERO, 0, 0));
    assert_eq!(game.instance(), original);
    assert_zero_round(&game, 0);
    assert_eq!(game.queue().accepted, 0);

    game.begin_round();
    game.settle_start(FlowPhase::Resuming);
    assert_ne!(game.instance(), original);
    assert_eq!(game.runtime_clock(), (0, Duration::ZERO));
    assert_zero_round(&game, 1);
    game.finish_resume();
    assert_zero_round(&game, 1);
    game.frame(STEP);
    assert_eq!(game.snapshot().logical_tick, 1);
    assert_eq!(
        game.snapshot().logical_elapsed_nanos,
        STEP.as_nanos() as u64
    );
    assert_eq!(game.session.runtime_generation(), Some(generation));
    game.assert_idle();

    game.tap(KeyCode::Escape);
    game.tap(KeyCode::Character('t'));
    assert_eq!(game.flow().phase, FlowPhase::Title);
    game.driver.tap(KeyCode::Character('x'));
    assert_eq!(
        game.session.frame(Duration::ZERO).unwrap(),
        Some(AppExit::Requested)
    );
    assert_eq!(game.session.exit(), Some(AppExit::Requested));
    assert_eq!(game.close_terminal(), HeadlessSessionClose::Complete);
    assert!(game.session.is_closed());
}

#[test]
fn a_pending_player_save_launch_survives_title_and_is_consumed_once_by_the_first_resume_batch() {
    let project = ProjectCopy::new();
    let save = project.save_path();
    let mut game = Game::from_project(project.root(), keyboard_flow_recipe());
    let (admitted, installed) = game
        .session
        .observe(|world| {
            let flow = world.resource::<FlowBindings>();
            let original = nara::input::startup_action_map_bindings(world).unwrap();
            let map = world
                .resource::<ActionMaps>()
                .get(InputSeatId::PRIMARY)
                .unwrap();
            assert!(flow.admitted().shares_owner_with(original));
            assert!(flow.installed().shares_owner_with(&map.bindings_handle()));
            assert!(!flow.admitted().shares_owner_with(flow.installed()));
            (flow.admitted().clone(), flow.installed().clone())
        })
        .unwrap();
    let launch = two_level_game::player_launch(RunOptions {
        save_path: Some(save.clone()),
        ..RunOptions::default()
    })
    .unwrap()
    .unwrap();
    launch.submit_headless(&mut game.session).unwrap();
    for _ in 0..3 {
        game.frame(Duration::from_millis(100));
        assert_eq!(game.flow().phase, FlowPhase::Title);
        assert_eq!(game.snapshot().logical_tick, 0);
        assert_eq!(game.queue().pending_commands, 1);
        assert_eq!(
            game.session
                .observe(|world| two_level_game::player_launch_result(world)
                    .map(|result| result.map_err(str::to_owned)))
                .unwrap(),
            None
        );
    }
    game.start_round();
    assert_eq!(
        game.session
            .observe(|world| two_level_game::player_launch_result(world)
                .map(|result| result.map_err(str::to_owned)))
            .unwrap(),
        Some(Ok(()))
    );
    assert_eq!(game.queue().accepted, 1);
    assert_eq!(game.queue().acknowledged, 1);
    game.assert_idle();
    game.session
        .observe(|world| {
            let flow = world.resource::<FlowBindings>();
            assert!(admitted.shares_owner_with(flow.admitted()));
            assert!(installed.shares_owner_with(flow.installed()));
        })
        .unwrap();
    assert!(!save.exists());
    game.command('k');
    assert_eq!(game.snapshot().saves, 1);
    let saved = two_level_game::save::decode_checkpoint(&fs::read(&save).unwrap()).unwrap();
    assert_eq!(saved.checkpoint.score, 0);
    assert_eq!(
        game.session
            .observe(|world| two_level_game::player_launch_result(world)
                .map(|result| result.map_err(str::to_owned)))
            .unwrap(),
        Some(Ok(()))
    );
    game.tap(KeyCode::Escape);
    game.tap(KeyCode::Character('t'));
    game.start_round();
    assert_eq!(game.flow().round, 2);
    assert_eq!(game.snapshot().saves, 0);
    game.command('k');
    assert_eq!(game.snapshot().saves, 1);
    let saved = two_level_game::save::decode_checkpoint(&fs::read(&save).unwrap()).unwrap();
    assert_eq!(saved.checkpoint.score, 0);
    assert_eq!(
        game.session
            .observe(|world| two_level_game::player_launch_result(world)
                .map(|result| result.map_err(str::to_owned)))
            .unwrap(),
        Some(Ok(()))
    );
}

#[test]
fn a_refused_first_resume_save_launch_keeps_its_result_and_completes_the_original_batch() {
    let project = ProjectCopy::new();
    let missing_parent = project.save_path().join("missing");
    let invalid = missing_parent.join("checkpoint.json");
    assert!(!missing_parent.exists());
    let mut game = Game::from_project(project.root(), keyboard_flow_recipe());
    two_level_game::player_launch(RunOptions {
        save_path: Some(invalid.clone()),
        ..RunOptions::default()
    })
    .unwrap()
    .unwrap()
    .submit_headless(&mut game.session)
    .unwrap();
    game.begin_round();
    game.settle_start(FlowPhase::Resuming);
    game.frame(STEP);
    let result = game
        .session
        .observe(|world| {
            two_level_game::player_launch_result(world).map(|result| result.map_err(str::to_owned))
        })
        .unwrap()
        .unwrap();
    assert!(result.is_err());
    assert_eq!(game.snapshot().error.as_ref(), result.as_ref().err());
    assert_eq!(game.flow().phase, FlowPhase::Playing);
    assert_eq!(game.snapshot().logical_tick, 0);
    assert_eq!(game.queue().accepted, 1);
    assert_eq!(game.queue().acknowledged, 1);
    game.assert_idle();
    assert!(!missing_parent.exists() && !invalid.exists());
    game.tap(KeyCode::Escape);
    game.tap(KeyCode::Character('t'));
    game.start_round();
    assert_eq!(game.flow().round, 2);
    assert_eq!(
        game.session
            .observe(|world| two_level_game::player_launch_result(world)
                .map(|result| result.map_err(str::to_owned)))
            .unwrap(),
        Some(result)
    );
    assert_eq!(game.queue().accepted, 1);
    assert_eq!(game.queue().acknowledged, 1);
}

#[test]
fn game_pause_clears_held_movement_freezes_clocks_and_requires_a_release_and_fresh_press() {
    let mut game = Game::source();
    game.start_round();
    let spawn = game.snapshot().player;
    game.driver.key(RIGHT, true);
    game.frame(STEP);
    assert!((game.snapshot().player.x - spawn.x - 3.2).abs() < 0.0001);
    assert_eq!(game.snapshot().held, [false, true, false, false]);
    let (phase, moving_region) = game.explorer_at_extract();
    assert_eq!(phase, FlowPhase::Playing);
    assert_ne!(moving_region, ExplorerFrame::Idle.texture_region());

    game.tap(KeyCode::Escape);
    assert_eq!(game.flow().phase, FlowPhase::Paused);
    assert_eq!(game.session.state(), Some(RuntimeState::Running));
    assert_eq!(game.time_policy(), (false, 0.0));
    assert_eq!(game.snapshot().held, [false; 4]);
    assert_eq!(
        game.explorer_at_extract(),
        (FlowPhase::Paused, ExplorerFrame::Idle.texture_region())
    );
    assert!(!game.gameplay_enabled());
    let frozen = game.clocks();
    let position = game.snapshot().player;
    for _ in 0..3 {
        game.frame(Duration::from_secs(1));
        assert_eq!(game.clocks(), frozen);
        assert_eq!(game.snapshot().player, position);
        assert_eq!(game.object("explorer").0, position);
        assert_eq!(game.snapshot().held, [false; 4]);
    }

    game.tap(KeyCode::Enter);
    assert_eq!(game.flow().phase, FlowPhase::Resuming);
    game.finish_resume();
    for _ in 0..3 {
        game.frame(STEP);
        assert_eq!(game.snapshot().held, [false; 4]);
        assert_eq!(
            game.snapshot().player,
            position,
            "the physical D hold must not be restored"
        );
    }
    assert_eq!(game.snapshot().logical_tick, frozen.2 + 3);
    game.driver.key(RIGHT, false);
    game.frame(STEP);
    assert_eq!(game.snapshot().player, position);
    game.driver.key(RIGHT, true);
    game.frame(STEP);
    assert_eq!(game.snapshot().held, [false, true, false, false]);
    assert!((game.snapshot().player.x - position.x - 3.2).abs() < 0.0001);
    game.assert_idle();
}

#[test]
fn zero_tick_marker_travel_and_movement_backlog_is_drained_by_the_original_resumed_batch_and_ack() {
    let mut game = Game::source();
    game.start_round();
    let original = game.snapshot();
    let clock = game.clocks();
    let idle = game.queue();
    game.driver.tap(KeyCode::Character('m'));
    game.driver.tap(KeyCode::Character('e'));
    game.driver.key(RIGHT, true);
    game.driver.tap(KeyCode::Escape);
    game.frame(Duration::ZERO);
    assert_eq!(game.flow().phase, FlowPhase::Paused);
    assert_eq!(game.clocks(), clock);
    assert_eq!(game.queue().accepted, idle.accepted + 3);
    assert_eq!(game.queue().pending_commands, 3);
    assert_eq!(game.queue().acknowledged, idle.acknowledged);
    assert_eq!(game.snapshot().held, [false; 4]);

    game.frame(Duration::ZERO);
    assert_eq!(game.queue().accepted, idle.accepted + 4);
    assert_eq!(game.queue().pending_commands, 4);
    game.tap(KeyCode::Escape);
    assert_eq!(game.flow().phase, FlowPhase::Resuming);
    let pending = game.queue();
    assert_eq!(pending.active_commands, 0);
    assert_eq!(pending.admitted, idle.admitted);
    assert_eq!(pending.acknowledged_through_tick, clock.0);
    game.finish_resume();
    let acknowledged = game.queue();
    assert_eq!(acknowledged.accepted, pending.accepted);
    assert_eq!(
        acknowledged.admitted - pending.admitted,
        pending.pending_commands as u64
    );
    assert_eq!(
        acknowledged.acknowledged - pending.acknowledged,
        pending.pending_commands as u64
    );
    assert_eq!(acknowledged.acknowledged_through_tick, clock.0 + 1);
    for _ in 0..3 {
        game.frame(STEP);
        let snapshot = game.snapshot();
        assert_eq!(snapshot.current_instance, original.current_instance);
        assert_eq!(snapshot.level, original.level);
        assert_eq!(snapshot.player, original.player);
        assert_eq!(snapshot.travel_count, original.travel_count);
        assert_eq!(snapshot.score, 0);
        assert_eq!(snapshot.held, [false; 4]);
        assert!(snapshot.markers.is_empty());
        assert_eq!(snapshot.phase, GamePhase::Running);
        assert!(snapshot.error.is_none(), "{snapshot:?}");
        game.assert_idle();
    }
    game.command('m');
    assert_eq!(
        game.snapshot().markers.len(),
        1,
        "fresh gameplay commands must work after draining"
    );
    game.assert_idle();
}

#[test]
fn host_pause_is_independent_of_game_pause_and_opt_in_flow_refuses_exact_resume() {
    let mut game = Game::source();
    game.start_round();
    game.frame(STEP);
    assert_eq!(game.session.pause().unwrap(), None);
    assert_eq!(game.session.state(), Some(RuntimeState::Paused));
    assert_eq!(game.flow().phase, FlowPhase::Playing);
    assert_eq!(game.time_policy(), (true, 1.0));
    let frozen = game.clocks();
    match capture_resume(&game.session) {
        Err(GameResumeError::Ineligible(reason)) => {
            assert!(reason.contains("two-level-game.play-flow"), "{reason}");
        }
        result => panic!("the opt-in policy must refuse exact Resume explicitly: {result:?}"),
    }
    game.frame(Duration::from_secs(1));
    assert_eq!(game.flow().phase, FlowPhase::Playing);
    assert_eq!(game.clocks(), frozen);
    assert_eq!(game.time_policy(), (true, 1.0));
    assert_eq!(game.session.resume().unwrap(), None);
    assert_eq!(game.flow().phase, FlowPhase::Playing);
    assert_eq!(game.time_policy(), (false, 1.0));
    game.frame(STEP);
    assert_eq!(game.snapshot().logical_tick, frozen.2 + 1);

    game.tap(KeyCode::Escape);
    assert_eq!(game.flow().phase, FlowPhase::Paused);
    let frozen = game.clocks();
    assert_eq!(game.session.pause().unwrap(), None);
    assert_eq!(game.time_policy(), (true, 0.0));
    game.frame(Duration::from_secs(1));
    assert_eq!(game.flow().phase, FlowPhase::Paused);
    assert_eq!(game.clocks(), frozen);
    assert_eq!(game.session.resume().unwrap(), None);
    assert_eq!(game.session.state(), Some(RuntimeState::Running));
    assert_eq!(game.flow().phase, FlowPhase::Paused);
    assert_eq!(game.time_policy(), (false, 0.0));
    game.frame(Duration::from_secs(1));
    assert_eq!(game.clocks(), frozen);
    game.tap(KeyCode::Enter);
    game.finish_resume();
}

#[test]
fn host_step_projects_the_pickup_in_maintenance_without_advancing_the_menu() {
    let mut game = Game::source();
    game.start_round();
    let pickup = game.object("gem-near").0;
    game.driver.key(RIGHT, true);
    for _ in 0..64 {
        if game.snapshot().player.distance(pickup) <= 19.2 {
            break;
        }
        game.frame(STEP);
    }
    let before = game.snapshot();
    assert_eq!(before.collected, 0);
    assert!(before.player.distance(pickup) > 16.0 && before.player.distance(pickup) <= 19.2);
    assert!(!game.flow().meadow_collected);
    assert_eq!(game.session.pause().unwrap(), None);
    assert_eq!(game.session.step().unwrap(), None);
    let after = game.snapshot();
    assert_eq!(after.tick, before.tick + 1);
    assert_eq!(after.logical_tick, before.logical_tick + 1);
    assert_eq!(after.current_instance, before.current_instance);
    assert_eq!(after.collected, 1);
    assert_eq!(game.session.state(), Some(RuntimeState::Paused));
    let flow = game.flow();
    assert_eq!((flow.phase, flow.round), (FlowPhase::Playing, 1));
    assert!(flow.meadow_collected && !flow.cavern_collected);
    assert_eq!(game.object("gem-near").1, 0.0);
    game.driver.key(RIGHT, false);
}

#[test]
fn real_pickups_complete_a_round_and_new_scene_success_resets_progress_counters_and_clock() {
    let project = ProjectCopy::new();
    let save = project.save_path();
    let recipe = with_play_flow(
        configured_recipe(RunOptions {
            save_path: Some(save.clone()),
            ..RunOptions::default()
        }),
        PlayFlowControls::Keyboard,
    )
    .unwrap();
    let mut game = Game::from_project(project.root(), recipe);
    let generation = game.session.runtime_generation().unwrap();
    game.start_round();
    game.command('m');
    assert_eq!(game.snapshot().markers.len(), 1);
    game.command('r');
    assert_eq!(game.snapshot().restarts, 1);
    game.collect_near_pickup();
    assert_eq!(game.snapshot().score, 10);
    assert!(game.flow().meadow_collected);
    assert!(!game.flow().cavern_collected);
    assert_eq!(game.flow().phase, FlowPhase::Playing);
    game.command('k');
    assert_eq!(game.snapshot().saves, 1);
    assert!(save.is_file());
    game.command('e');
    game.settle_level(LevelId::Cavern);
    game.command('l');
    game.settle_level(LevelId::Meadow);
    assert_eq!(game.snapshot().restores, 1);
    game.command('e');
    game.settle_level(LevelId::Cavern);
    game.command('m');
    assert_eq!(game.snapshot().markers.len(), 1);
    game.collect_near_pickup();
    let complete = game.snapshot();
    assert_eq!(game.flow().phase, FlowPhase::Complete);
    assert!(game.flow().meadow_collected);
    assert!(game.flow().cavern_collected);
    assert_eq!(game.flow().round, 1);
    assert_eq!(complete.score, 20);
    assert_eq!(complete.collected, 2);
    assert_eq!(complete.visible_pickups, 1);
    assert_eq!(complete.travel_count, 3);
    assert_eq!(complete.restarts, 1);
    assert_eq!(complete.saves, 1);
    assert_eq!(complete.restores, 1);
    assert!(complete.logical_tick > 0);
    assert!(complete.logical_elapsed_nanos > 0);
    assert_eq!(complete.held, [false; 4]);
    assert_eq!(game.time_policy(), (false, 0.0));
    assert_eq!(game.session.state(), Some(RuntimeState::Running));
    assert!(!game.gameplay_enabled());
    let frozen = game.clocks();
    game.frame(Duration::from_secs(1));
    assert_eq!(game.clocks(), frozen);

    game.begin_round();
    let starting = game.snapshot();
    assert_eq!(game.flow().round, 1);
    assert_eq!(starting.current_instance, complete.current_instance);
    assert_eq!(starting.score, complete.score);
    assert_eq!(starting.travel_count, complete.travel_count);
    assert_eq!(starting.restarts, complete.restarts);
    assert_eq!(starting.saves, complete.saves);
    assert_eq!(starting.restores, complete.restores);
    assert_eq!(starting.logical_tick, complete.logical_tick);
    game.settle_start(FlowPhase::Resuming);
    assert_ne!(game.snapshot().current_instance, complete.current_instance);
    assert_eq!(game.runtime_clock(), (frozen.0, frozen.1));
    assert_zero_round(&game, 2);
    assert_eq!(game.object("gem-near").1, 1.0);
    game.finish_resume();
    assert_zero_round(&game, 2);
    assert_eq!(game.session.runtime_generation(), Some(generation));
    game.frame(STEP);
    assert_eq!(game.snapshot().logical_tick, 1);
    game.assert_idle();
}

#[test]
fn refused_source_preparation_retains_the_old_round_and_title_then_repaired_confirm_retries() {
    let project = ProjectCopy::new();
    let path = project
        .root()
        .join("scenes/meadow.scene.json")
        .canonicalize()
        .unwrap();
    assert!(path.starts_with(project.root().canonicalize().unwrap()));
    assert!(
        !path.starts_with(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .canonicalize()
                .unwrap()
        )
    );
    let original_bytes = fs::read(&path).unwrap();
    let mut game = Game::from_project(project.root(), keyboard_flow_recipe());
    game.start_round();
    game.collect_near_pickup();
    game.command('m');
    game.tap(KeyCode::Escape);
    game.tap(KeyCode::Character('t'));
    assert_eq!(game.flow().phase, FlowPhase::Title);
    let retained = game.snapshot();
    let frozen = game.clocks();
    let generation = game.session.runtime_generation().unwrap();
    assert_eq!(retained.score, 10);
    assert_eq!(retained.markers.len(), 1);

    fs::write(&path, b"{ broken new-round source").unwrap();
    game.begin_round();
    game.settle_start(FlowPhase::Title);
    let refused = game.snapshot();
    assert_eq!(game.flow().round, 1);
    assert!(game.flow().meadow_collected);
    assert!(!game.flow().cavern_collected);
    assert!(
        game.flow()
            .error
            .as_ref()
            .is_some_and(|error| error.contains("scene-format-invalid")),
        "{:?}",
        game.flow()
    );
    assert_eq!(refused.current_instance, retained.current_instance);
    assert_eq!(refused.current_instance, Some(game.instance()));
    assert_eq!(refused.phase, GamePhase::Running);
    assert_eq!(refused.player, retained.player);
    assert_eq!(game.object("explorer").0, retained.player);
    assert_eq!(game.object("gem-near").1, 0.0);
    assert_eq!(refused.score, retained.score);
    assert_eq!(refused.collected, retained.collected);
    assert_eq!(refused.markers, retained.markers);
    assert_eq!(refused.travel_count, retained.travel_count);
    assert_eq!(game.clocks(), frozen);
    assert_eq!(game.time_policy(), (false, 0.0));
    assert!(!game.gameplay_enabled());
    game.frame(Duration::from_secs(1));
    assert_eq!(game.clocks(), frozen);

    fs::write(&path, original_bytes).unwrap();
    game.begin_round();
    assert!(game.flow().error.is_none());
    game.settle_start(FlowPhase::Resuming);
    assert_ne!(game.snapshot().current_instance, retained.current_instance);
    assert_zero_round(&game, 2);
    game.finish_resume();
    assert_zero_round(&game, 2);
    assert_eq!(game.session.runtime_generation(), Some(generation));
}

#[cfg(feature = "gamepad")]
#[test]
fn controller_pause_then_save_keeps_the_older_checkpoint() {
    check_checkpoint_order(false);
}

#[cfg(feature = "gamepad")]
#[test]
fn controller_save_then_pause_writes_once() {
    check_checkpoint_order(true);
}

#[cfg(feature = "gamepad")]
#[test]
fn a_rejected_restore_preserves_the_other_rounds_save_until_restore_commits() {
    use GamepadButton::{LeftShoulder, RightShoulder, Start};
    use two_level_game::save::decode_checkpoint;

    let project = ProjectCopy::new();
    let save = project.save_path();
    let mut incumbent = controller_game_with_save(&project);
    incumbent.start_round();
    incumbent.driver.tap_button(LeftShoulder);
    incumbent.frame(STEP);
    assert_eq!(incumbent.snapshot().saves, 1);
    assert_eq!(
        decode_checkpoint(&fs::read(&save).unwrap())
            .unwrap()
            .checkpoint
            .score,
        0
    );
    let incumbent_scene = incumbent.instance();

    {
        let mut other_round = controller_game_with_save(&project);
        other_round.start_round();
        other_round.command('e');
        other_round.settle_level(LevelId::Cavern);
        other_round.collect_near_pickup();
        other_round.driver.tap_button(LeftShoulder);
        other_round.frame(STEP);
        assert_eq!(other_round.snapshot().saves, 1);
    }
    let other_bytes = fs::read(&save).unwrap();
    let other_checkpoint = decode_checkpoint(&other_bytes).unwrap().checkpoint;
    assert_eq!(
        other_checkpoint.location.level,
        LevelKey::main(LevelId::Cavern)
    );
    assert_eq!(other_checkpoint.score, 10);

    let assert_conflict = |game: &Game| {
        let state = game.snapshot();
        assert_eq!(state.saves, 1, "{state:?}");
        assert_eq!(state.restores, 0, "{state:?}");
        assert_eq!(state.score, 0);
        assert_eq!(game.instance(), incumbent_scene);
        assert!(
            state
                .error
                .as_ref()
                .is_some_and(|error| error.contains("checkpoint changed")),
            "{state:?}"
        );
        assert_eq!(fs::read(&save).unwrap(), other_bytes);
    };
    incumbent.driver.tap_button(LeftShoulder);
    incumbent.frame(STEP);
    assert_conflict(&incumbent);

    let destination = project.root().join("scenes/cavern.scene.json");
    let source = fs::read(&destination).unwrap();
    fs::write(&destination, b"{ malformed target").unwrap();
    incumbent.driver.tap_button(RightShoulder);
    incumbent.frame(STEP);
    let deadline = Instant::now() + DEADLINE;
    while incumbent.snapshot().phase == GamePhase::Preparing {
        assert!(
            Instant::now() < deadline,
            "the rejected Restore did not settle"
        );
        incumbent.frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    let rejected = incumbent.snapshot();
    assert_eq!(rejected.phase, GamePhase::Running);
    assert_eq!(rejected.restores, 0);
    assert_eq!(incumbent.instance(), incumbent_scene);
    assert!(
        rejected
            .error
            .as_ref()
            .is_some_and(|error| error.contains("scene-format-invalid")),
        "{rejected:?}"
    );
    fs::write(&destination, source).unwrap();

    incumbent.driver.tap_button(LeftShoulder);
    incumbent.frame(STEP);
    assert_conflict(&incumbent);

    incumbent.driver.tap_button(LeftShoulder);
    incumbent.driver.tap_button(Start);
    incumbent.frame(Duration::ZERO);
    assert_eq!(incumbent.flow().phase, FlowPhase::Paused);
    assert_eq!(fs::read(&save).unwrap(), other_bytes);
    incumbent.driver.tap_button(Start);
    incumbent.frame(Duration::ZERO);
    assert_eq!(incumbent.flow().phase, FlowPhase::Resuming);
    incumbent.frame(STEP);
    assert_eq!(incumbent.flow().phase, FlowPhase::Playing);
    assert_conflict(&incumbent);

    incumbent.driver.tap_button(RightShoulder);
    incumbent.frame(STEP);
    incumbent.settle_level(LevelId::Cavern);
    assert_eq!(incumbent.snapshot().restores, 1);
    assert_eq!(incumbent.snapshot().score, 10);
    assert_ne!(incumbent.instance(), incumbent_scene);
    incumbent.driver.tap_button(LeftShoulder);
    incumbent.frame(STEP);
    assert_eq!(incumbent.snapshot().saves, 2);
    assert_eq!(
        decode_checkpoint(&fs::read(&save).unwrap())
            .unwrap()
            .checkpoint,
        other_checkpoint
    );
}

#[cfg(feature = "gamepad")]
fn controller_game_with_save(project: &ProjectCopy) -> Game {
    let recipe = with_play_flow(
        configured_recipe(RunOptions {
            save_path: Some(project.save_path()),
            ..RunOptions::default()
        }),
        PlayFlowControls::KeyboardAndGamepad,
    )
    .unwrap();
    Game::from_project(project.root(), recipe)
}

#[cfg(feature = "gamepad")]
fn check_checkpoint_order(save_first: bool) {
    use GamepadButton::{East, LeftShoulder, RightShoulder, Start};
    use two_level_game::save::decode_checkpoint;

    for (delta, fixed_steps) in [
        (STEP - Duration::from_millis(1), 0),
        (STEP, 1),
        (STEP * 3, 3),
    ] {
        let project = ProjectCopy::new();
        let save = project.save_path();
        let mut game = controller_game_with_save(&project);
        game.start_round();
        game.collect_near_pickup();

        game.driver.tap_button(LeftShoulder);
        game.frame(Duration::ZERO);
        assert!(!save.exists());
        game.frame(STEP * 3);
        assert_eq!(game.snapshot().saves, 1);
        let old_bytes = fs::read(&save).unwrap();
        assert_eq!(decode_checkpoint(&old_bytes).unwrap().checkpoint.score, 10);

        game.driver.tap_button(Start);
        game.frame(Duration::ZERO);
        assert_eq!(game.flow().phase, FlowPhase::Paused);
        game.driver.tap_button(East);
        game.frame(Duration::ZERO);
        assert_eq!(game.flow().phase, FlowPhase::Title);
        game.start_round();
        assert_zero_round(&game, 2);

        let before = game.snapshot();
        let accepted = game.queue().accepted;
        let buttons = if save_first {
            [LeftShoulder, Start]
        } else {
            [Start, LeftShoulder]
        };
        for button in buttons {
            game.driver.tap_button(button);
        }
        game.frame(delta);
        let paused = game.snapshot();
        assert_eq!(game.flow().phase, FlowPhase::Paused, "{delta:?}");
        assert_eq!(paused.tick - before.tick, fixed_steps, "{delta:?}");
        assert_eq!(game.queue().accepted, accepted + 1, "{delta:?}");
        let wrote_in_pause_frame = save_first && fixed_steps != 0;
        assert_eq!(paused.saves, u32::from(wrote_in_pause_frame), "{delta:?}");
        if !wrote_in_pause_frame {
            assert_eq!(fs::read(&save).unwrap(), old_bytes, "{delta:?}");
        }

        game.driver.tap_button(LeftShoulder);
        game.frame(Duration::ZERO);
        assert_eq!(game.flow().phase, FlowPhase::Paused);
        assert_eq!(game.queue().accepted, accepted + 1);

        game.driver.tap_button(Start);
        game.frame(Duration::ZERO);
        game.finish_resume();
        let saved_bytes = fs::read(&save).unwrap();
        let expected_score = if save_first { 0 } else { 10 };
        assert_eq!(
            decode_checkpoint(&saved_bytes).unwrap().checkpoint.score,
            expected_score,
            "{delta:?}"
        );
        if !save_first {
            assert_eq!(saved_bytes, old_bytes, "{delta:?}");
        }
        game.driver.tap_button(RightShoulder);
        game.frame(STEP);
        game.settle_level(LevelId::Meadow);
        assert_eq!(game.snapshot().restores, 1, "{delta:?}");
        assert_eq!(game.snapshot().score, expected_score, "{delta:?}");
        assert_eq!(game.snapshot().saves, u32::from(save_first), "{delta:?}");
        assert_eq!(fs::read(&save).unwrap(), saved_bytes, "{delta:?}");
    }
}

#[cfg(feature = "gamepad")]
#[test]
fn controller_save_pending_from_prior_frame_writes_on_the_pause_frame() {
    use GamepadButton::{LeftShoulder, Start};
    use two_level_game::save::decode_checkpoint;

    let project = ProjectCopy::new();
    let save = project.save_path();
    let mut game = controller_game_with_save(&project);
    game.start_round();
    game.collect_near_pickup();
    game.frame(STEP);
    let before = game.snapshot();

    game.driver.tap_button(LeftShoulder);
    game.frame(Duration::from_millis(19));
    assert_eq!(game.snapshot().tick, before.tick);
    assert_eq!(game.queue().pending_commands, 1);
    assert!(!save.exists());

    game.driver.tap_button(Start);
    game.driver.tap_button(LeftShoulder);
    game.frame(Duration::from_millis(1));
    assert_eq!(game.flow().phase, FlowPhase::Paused);
    assert_eq!(game.snapshot().tick, before.tick + 1);
    assert_eq!(game.snapshot().saves, 1);
    let bytes = fs::read(&save).unwrap();
    assert_eq!(decode_checkpoint(&bytes).unwrap().checkpoint.score, 10);
    game.assert_idle();
}

#[cfg(feature = "gamepad")]
#[test]
fn the_first_paused_menu_action_decides_the_pending_round_save() {
    use GamepadButton::{East, LeftShoulder, Start};

    for title_first in [true, false] {
        let project = ProjectCopy::new();
        let save = project.save_path();
        let mut game = controller_game_with_save(&project);
        game.start_round();
        game.collect_near_pickup();
        game.driver.tap_button(LeftShoulder);
        game.frame(STEP);
        let original = fs::read(&save).unwrap();

        game.driver.tap_button(Start);
        game.frame(Duration::ZERO);
        game.driver.tap_button(East);
        game.frame(Duration::ZERO);
        game.start_round();
        assert_zero_round(&game, 2);

        game.driver.tap_button(LeftShoulder);
        game.driver.tap_button(Start);
        game.frame(Duration::ZERO);
        assert_eq!(game.flow().phase, FlowPhase::Paused);
        assert_eq!(game.queue().pending_commands, 1);
        let buttons = if title_first {
            [East, Start]
        } else {
            [Start, East]
        };
        for button in buttons {
            game.driver.tap_button(button);
        }
        game.frame(Duration::ZERO);
        if title_first {
            assert_eq!(game.flow().phase, FlowPhase::Title);
            game.start_round();
            assert_eq!(fs::read(&save).unwrap(), original);
            assert_eq!(game.snapshot().saves, 0);
        } else {
            assert_eq!(game.flow().phase, FlowPhase::Resuming);
            game.finish_resume();
            assert_eq!(game.snapshot().saves, 1);
            assert_eq!(
                two_level_game::save::decode_checkpoint(&fs::read(&save).unwrap())
                    .unwrap()
                    .checkpoint
                    .score,
                0,
            );
        }
        game.assert_idle();
    }
}

#[test]
fn refused_travel_preserves_the_last_admitted_typed_movement_decision() {
    let project = ProjectCopy::new();
    let mut game = Game::from_project(project.root(), keyboard_flow_recipe());
    game.start_round();
    game.driver.key(RIGHT, true);
    game.frame(STEP);
    assert!(game.snapshot().held[1]);
    let stopped_at = game.snapshot().player;
    let submission = ScheduledCommand::new(
        CommandTick::new(game.snapshot().tick + 1).unwrap(),
        CommandSource::external("two-level-game.test.typed-stop").unwrap(),
        CommandSequence::new(1).unwrap(),
        support::movement(1, false),
    );
    let key = submission.key().clone();
    assert_eq!(submission.submit_headless(&mut game.session).unwrap(), key);
    game.frame(STEP);
    assert_eq!(game.snapshot().player, stopped_at);
    assert!(!game.snapshot().held[1]);

    let starting_instance = game.instance();
    fs::remove_file(project.root().join("scenes/cavern.scene.json")).unwrap();
    game.tap(KeyCode::Character('e'));
    game.frame(STEP);
    let deadline = Instant::now() + DEADLINE;
    while game.snapshot().error.is_none() {
        assert!(
            Instant::now() < deadline,
            "the missing travel target was never refused"
        );
        game.frame(Duration::ZERO);
        std::thread::sleep(Duration::from_millis(1));
    }
    assert_eq!(game.snapshot().phase, GamePhase::Running);
    assert_eq!(game.instance(), starting_instance);
    assert!(!game.snapshot().held[1]);
    game.frame(STEP * 3);
    assert_eq!(game.snapshot().player, stopped_at);
    game.assert_idle();
}

#[cfg(feature = "gamepad")]
#[test]
fn host_exact_step_keeps_native_and_typed_save_in_the_original_batch() {
    use two_level_game::save::decode_checkpoint;

    let project = ProjectCopy::new();
    let save = project.save_path();
    let mut game = controller_game_with_save(&project);
    game.start_round();
    assert_eq!(game.session.pause().unwrap(), None);

    game.driver.tap_button(GamepadButton::Start);
    game.driver.tap_button(GamepadButton::LeftShoulder);
    game.frame(Duration::ZERO);
    assert!(!save.exists());
    assert_eq!(game.session.step().unwrap(), None);
    assert_eq!(game.flow().phase, FlowPhase::Playing);
    assert_eq!(game.snapshot().saves, 1);
    game.assert_idle();

    let submission = ScheduledCommand::new(
        CommandTick::new(game.snapshot().tick + 1).unwrap(),
        CommandSource::external("two-level-game.test.typed-save").unwrap(),
        CommandSequence::new(1).unwrap(),
        support::action("two-level-game.save"),
    );
    let key = submission.key().clone();
    assert_eq!(submission.submit_headless(&mut game.session).unwrap(), key);
    assert_eq!(game.snapshot().saves, 1);
    assert_eq!(game.session.step().unwrap(), None);
    assert_eq!(game.snapshot().saves, 2);
    let bytes = fs::read(&save).unwrap();
    assert_eq!(decode_checkpoint(&bytes).unwrap().checkpoint.score, 0);
    game.assert_idle();
}

#[cfg(feature = "gamepad")]
#[test]
fn primary_travel_preserves_movement_for_controller_and_keyboard_handoffs() {
    use GamepadButton::{DPadRight, South};

    for keyboard in [false, true] {
        for replacement_first in [true, false] {
            let recipe =
                with_play_flow(product_recipe(), PlayFlowControls::KeyboardAndGamepad).unwrap();
            let mut game = Game::from_project(Path::new(env!("CARGO_MANIFEST_DIR")), recipe);
            game.start_round();
            game.driver.button(DPadRight, true);
            game.frame(STEP);
            assert!(game.snapshot().held[1]);

            game.driver.tap_button(South);
            game.frame(STEP);
            game.settle_level(LevelId::Cavern);
            assert_eq!(game.flow().phase, FlowPhase::Playing);
            assert_eq!(game.snapshot().player.x, -120.0);

            let replacement = |driver: &Driver, pressed| {
                if keyboard {
                    driver.key(RIGHT, pressed);
                } else {
                    driver.controller_button(1, DPadRight, pressed);
                }
            };
            let check_step = |game: &mut Game, moving| {
                let before = game.snapshot().player;
                game.frame(STEP);
                let after = game.snapshot();
                let expected_dx = if moving { 3.2 } else { 0.0 };
                assert_eq!(
                    after.held,
                    [false, moving, false, false],
                    "keyboard={keyboard}, replacement_first={replacement_first}"
                );
                assert!((after.player.x - before.x - expected_dx).abs() < 0.0001);
                assert_eq!(after.player.y, before.y);
            };
            check_step(&mut game, true);
            if replacement_first {
                replacement(&game.driver, true);
                check_step(&mut game, true);
                game.driver.disconnect(0);
            } else {
                game.driver.disconnect(0);
                check_step(&mut game, false);
                replacement(&game.driver, true);
            }
            check_step(&mut game, true);
            replacement(&game.driver, false);
            check_step(&mut game, false);
            check_step(&mut game, false);
            game.assert_idle();
        }
    }
}

#[cfg(feature = "gamepad")]
#[test]
fn controller_can_complete_restart_and_exit_a_round_without_keyboard_input() {
    use nara::input::InputBinding;
    use two_level_game::SideRoomPhase::{Closed, Open};

    let project = ProjectCopy::new();
    let path = project.root().join(two_level_game::input::INPUT_PATH);
    let mut input: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    input["payload"]["contexts"]
        .as_array_mut()
        .unwrap()
        .push(serde_json::json!({
            "context": "photo-mode", "enabled": false,
        }));
    for (button, name) in [
        (GamepadButton::East, "photo"),
        (GamepadButton::Select, "gallery"),
    ] {
        input["payload"]["bindings"]
            .as_array_mut()
            .unwrap()
            .push(serde_json::json!({
                "action": format!("two-level-game.{name}"),
                "input": {"GamepadButton": button.code()},
                "context": "photo-mode",
                "transform": "Button",
            }));
    }
    fs::write(&path, serde_json::to_vec(&input).unwrap()).unwrap();
    let recipe = with_play_flow(product_recipe(), PlayFlowControls::KeyboardAndGamepad).unwrap();
    let mut game = Game::from_project(project.root(), recipe);
    game.session
        .observe(|world| {
            let flow = world.resource::<FlowBindings>();
            let map = world
                .resource::<ActionMaps>()
                .get(InputSeatId::PRIMARY)
                .unwrap();
            let retry = ActionId::new("two-level-game.r").unwrap();
            let start = GamepadButton::Start.code();
            let original = nara::input::startup_action_map_bindings(world).unwrap();
            assert!(flow.admitted().shares_owner_with(original));
            assert!(flow.installed().shares_owner_with(&map.bindings_handle()));
            assert!(
                flow.admitted()
                    .definition()
                    .bindings()
                    .contains(&ActionBinding::gamepad_button(retry.clone(), start))
            );
            assert!(
                map.bindings()
                    .contains(&ActionBinding::key(retry, KeyCode::Character('r')))
            );
            let bound: Vec<_> = map
                .bindings()
                .iter()
                .filter(|binding| binding.input == InputBinding::GamepadButton(start))
                .map(|binding| (binding.action.as_str(), binding.context.as_str()))
                .collect();
            assert_eq!(bound, [("two-level-game.menu.start", MENU)]);
            let photo = ActionContext::new("photo-mode").unwrap();
            assert!(!map.is_context_enabled(&photo));
            for (button, menu_action, gameplay_action) in [
                (
                    GamepadButton::East,
                    "two-level-game.menu.title",
                    "two-level-game.g",
                ),
                (
                    GamepadButton::Select,
                    "two-level-game.menu.exit",
                    "two-level-game.b",
                ),
            ] {
                let bindings: Vec<_> = map
                    .bindings()
                    .iter()
                    .filter(|binding| binding.input == InputBinding::GamepadButton(button.code()))
                    .collect();
                assert_eq!(bindings.len(), 3);
                assert!(bindings.iter().any(|binding| binding.context == photo));
                assert!(
                    bindings
                        .iter()
                        .any(|binding| binding.context == ActionContext::gameplay()
                            && binding.action.as_str() == gameplay_action)
                );
                assert!(bindings.iter().any(|binding| binding.context.as_str()
                    == "two-level-game.menu.gamepad"
                    && binding.action.as_str() == menu_action));
            }
        })
        .unwrap();
    assert_eq!(game.flow().phase, FlowPhase::Title);
    let boot = game.instance();
    game.driver.button(GamepadButton::East, true);
    game.frame(Duration::ZERO);
    assert_eq!(game.flow().phase, FlowPhase::Title);
    game.driver.button(GamepadButton::Start, true);
    game.frame(Duration::ZERO);
    assert_eq!(game.flow().phase, FlowPhase::Starting);
    assert_eq!(game.instance(), boot);
    game.settle_start(FlowPhase::Resuming);
    assert_ne!(game.instance(), boot);
    game.finish_resume();
    game.frame(STEP);
    assert_eq!(game.flow().phase, FlowPhase::Playing);
    assert_eq!(game.snapshot().restarts, 0);
    assert_eq!(game.snapshot().lookout, Closed);

    game.driver.button(GamepadButton::East, false);
    game.driver.button(GamepadButton::Start, false);
    game.driver.button(GamepadButton::DPadRight, true);
    game.frame(STEP);
    let moved = game.snapshot().player;
    assert!(moved.x > -120.0);
    assert_eq!(game.snapshot().held, [false, true, false, false]);
    game.driver.button(GamepadButton::Select, true);
    game.driver.tap_button(GamepadButton::Start);
    game.frame(Duration::ZERO);
    assert_eq!(game.flow().phase, FlowPhase::Paused);
    assert_eq!(game.snapshot().held, [false; 4]);
    assert!(game.queue().pending_commands > 0);
    let paused_clock = game.clocks();
    game.frame(Duration::from_millis(100));
    game.driver.button(GamepadButton::Select, false);
    game.frame(Duration::ZERO);
    assert_eq!(game.flow().phase, FlowPhase::Paused);
    assert_eq!(game.clocks(), paused_clock);
    assert_eq!(game.explorer_frame(), ExplorerFrame::Idle.texture_region());
    game.driver.tap_button(GamepadButton::Start);
    game.frame(Duration::ZERO);
    game.finish_resume();
    game.frame(STEP);
    assert_eq!(game.snapshot().player, moved);
    assert_eq!(game.snapshot().held, [false; 4]);
    assert_eq!(game.snapshot().side_room, Closed);
    game.driver.button(GamepadButton::DPadRight, false);
    game.frame(Duration::ZERO);

    game.toggle_room(GamepadButton::East, |state| state.lookout, Open);
    game.toggle_room(GamepadButton::East, |state| state.lookout, Closed);
    game.toggle_room(GamepadButton::Select, |state| state.side_room, Open);
    game.toggle_room(GamepadButton::Select, |state| state.side_room, Closed);

    game.collect_near_pickup_with(|driver, pressed| {
        driver.button(GamepadButton::DPadRight, pressed)
    });
    assert_eq!(game.snapshot().score, 10);
    assert!(game.flow().meadow_collected);
    assert!(!game.flow().cavern_collected);
    game.driver.tap_button(GamepadButton::South);
    game.frame(STEP);
    game.settle_level(LevelId::Cavern);
    game.collect_near_pickup_with(|driver, pressed| {
        driver.button(GamepadButton::DPadRight, pressed)
    });
    assert_eq!(game.flow().phase, FlowPhase::Complete);
    assert_eq!(game.snapshot().score, 20);
    assert_eq!(game.snapshot().travel_count, 1);
    assert!(game.flow().cavern_collected);
    let complete = game.instance();
    game.driver.tap_button(GamepadButton::Start);
    game.frame(Duration::ZERO);
    game.settle_start(FlowPhase::Resuming);
    assert_ne!(game.instance(), complete);
    game.finish_resume();
    assert_zero_round(&game, 2);

    game.driver.tap_button(GamepadButton::Start);
    game.frame(Duration::ZERO);
    assert_eq!(game.flow().phase, FlowPhase::Paused);
    game.driver.tap_button(GamepadButton::East);
    game.frame(Duration::ZERO);
    assert_eq!(game.flow().phase, FlowPhase::Title);
    game.driver.tap_button(GamepadButton::Select);
    assert_eq!(
        game.session.frame(Duration::ZERO).unwrap(),
        Some(AppExit::Requested)
    );
    assert_eq!(game.close_terminal(), HeadlessSessionClose::Complete);
    assert!(game.session.is_closed());
}

#[cfg(feature = "gamepad")]
#[test]
fn controller_round_refuses_an_authored_start_binding_for_a_different_command() {
    let project = ProjectCopy::new();
    let path = project.root().join(two_level_game::input::INPUT_PATH);
    let mut input: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    let start = input["payload"]["bindings"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|binding| binding["input"]["GamepadButton"] == GamepadButton::Start.code())
        .unwrap();
    start["action"] = serde_json::json!("two-level-game.q");
    let edited = serde_json::to_vec(&input).unwrap();
    fs::write(&path, &edited).unwrap();
    let recipe = with_play_flow(product_recipe(), PlayFlowControls::KeyboardAndGamepad).unwrap();
    let mut session =
        HeadlessSession::new(open_explicit_project_root(project.root()).unwrap(), recipe);
    let startup = session.start();
    let deadline = Instant::now() + DEADLINE;
    let result = loop {
        let result = session.close();
        if result != HeadlessSessionClose::Pending || Instant::now() >= deadline {
            break result;
        }
        std::thread::sleep(Duration::from_millis(1));
    };
    let error = startup.expect_err("an unrelated authored Start binding must refuse the Round");
    assert_eq!(result, HeadlessSessionClose::Failed);
    assert!(session.is_closed());
    let nara::project_host::HeadlessSessionError::Runtime(diagnostics) = error else {
        panic!("expected classified Runtime startup refusal, got {error:?}");
    };
    assert!(
        diagnostics
            .iter()
            .any(|entry| entry.code().as_str() == "two-level-game.round-start-conflict"),
        "{diagnostics:?}"
    );
    assert_eq!(fs::read(&path).unwrap(), edited);

    let mut game = Game::from_project(project.root(), keyboard_flow_recipe());
    game.driver.tap_button(GamepadButton::Start);
    game.frame(Duration::ZERO);
    assert_eq!(game.flow().phase, FlowPhase::Title);
    assert_eq!(game.runtime_clock(), (0, Duration::ZERO));
}

#[test]
fn invalid_startup_checkpoint_can_stay_at_title_then_recover_a_new_initialized_animated_scene() {
    let project = ProjectCopy::new();
    let path = project
        .root()
        .join("scenes/meadow.scene.json")
        .canonicalize()
        .unwrap();
    assert!(path.starts_with(project.root().canonicalize().unwrap()));
    assert!(
        !path.starts_with(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .canonicalize()
                .unwrap()
        )
    );
    let original = fs::read(&path).unwrap();
    let mut scene: serde_json::Value = serde_json::from_slice(&original).unwrap();
    let checkpoint = scene["payload"]["entities"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|entity| entity["id"] == "checkpoint")
        .unwrap();
    let role = &mut checkpoint["components"]["two-level-game.ObjectRole"]["value"]["value"]["kind"]
        ["value"];
    assert_eq!(role.as_u64(), Some(2));
    *role = serde_json::json!(3);
    fs::write(&path, serde_json::to_vec_pretty(&scene).unwrap()).unwrap();

    let mut game = Game::from_project(project.root(), keyboard_flow_recipe());
    assert_eq!(game.flow().phase, FlowPhase::Degraded);
    assert!(
        game.flow()
            .error
            .as_ref()
            .is_some_and(|error| error.contains("checkpoint"))
    );
    let failed_instance = game.instance();
    let generation = game.session.runtime_generation().unwrap();
    game.tap(KeyCode::Character('t'));
    let title = game.flow();
    assert_eq!(title.phase, FlowPhase::Title);
    assert_eq!(title.round, 0);
    for delta in [Duration::ZERO, Duration::from_secs(1), Duration::ZERO] {
        game.frame(delta);
        assert_eq!(
            game.flow(),
            title,
            "a degraded Session must not pull Title back to Degraded"
        );
        assert_eq!(game.snapshot().phase, GamePhase::Degraded);
        assert!(!game.snapshot().initialized);
        assert_eq!(game.instance(), failed_instance);
        assert_eq!(game.clocks(), (0, Duration::ZERO, 0, 0));
        assert_eq!(game.time_policy(), (false, 0.0));
    }

    fs::write(&path, original).unwrap();
    game.begin_round();
    game.settle_start(FlowPhase::Resuming);
    assert_ne!(game.instance(), failed_instance);
    assert_zero_round(&game, 1);
    game.finish_resume();
    assert_eq!(game.explorer_frame(), ExplorerFrame::Idle.texture_region());
    let spawn = game.snapshot().player;
    game.driver.key(RIGHT, true);
    for _ in 0..4 {
        game.frame(STEP);
    }
    assert_eq!(game.snapshot().logical_tick, 4);
    assert_eq!(game.snapshot().logical_elapsed_nanos, 80_000_000);
    assert!(game.snapshot().player.x > spawn.x);
    assert_eq!(game.snapshot().held, [false, true, false, false]);
    assert_eq!(
        game.explorer_frame(),
        ExplorerFrame::WalkOne.texture_region()
    );
    game.frame(STEP);
    assert_eq!(game.snapshot().logical_tick, 5);
    assert_eq!(game.snapshot().logical_elapsed_nanos, 100_000_000);
    assert_eq!(
        game.explorer_frame(),
        ExplorerFrame::WalkTwo.texture_region()
    );
    assert_eq!(game.session.runtime_generation(), Some(generation));
    game.assert_idle();
}
