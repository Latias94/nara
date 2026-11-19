//! Public CPU acceptance for the opt-in source game's round and menu policy.

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

use nara::{
    ProductConfiguration, ProductRecipe,
    advanced_prelude::CurrentScene,
    app::{
        AppExit, CoreStage, FixedTime, Plugin, PluginCategory, PluginDeclaration, PluginError,
        PluginId, RuntimeState, RuntimeTimeSettings, StartupStage, VirtualTime,
    },
    ecs::{Resource, World, schedule::IntoScheduleConfigs},
    gameplay::{
        GameplayCommandBatch, GameplayCommandQueue, GameplayCommandQueueStats,
        LocalGameplayCommands,
    },
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
    capture_resume, configured_recipe,
    flow::{FlowPhase, FlowSnapshot, with_play_flow},
    levels::{AUTHORED_LEVEL_MEMBERS, LevelId, LevelKey},
    open_explicit_project_root, product_recipe,
};

const WINDOW: InputWindowId = InputWindowId::new(37);
const STEP: Duration = Duration::from_millis(20);
const MENU: &str = "two-level-game.menu";
const RIGHT: KeyCode = KeyCode::Character('d');

#[derive(Clone, Default, Resource)]
struct Driver(Arc<Mutex<Vec<(KeyCode, bool)>>>);

impl ProductConfiguration for Driver {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"two-level-game.play-flow-test/v1");
    }
}

impl Driver {
    fn key(&self, key: KeyCode, pressed: bool) {
        self.0.lock().unwrap().push((key, pressed));
    }

    fn tap(&self, key: KeyCode) {
        let mut pending = self.0.lock().unwrap();
        pending.extend([(key, true), (key, false)]);
    }
}

struct DriverPlugin(Driver);

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
    let keys = std::mem::take(&mut *world.resource::<Driver>().0.lock().unwrap());
    let mut timeline = world.resource_mut::<InputTimeline>();
    for (key, pressed) in keys {
        timeline
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
            with_play_flow(product_recipe()).unwrap(),
        )
    }

    fn from_project(root: &Path, recipe: ProductRecipe) -> Self {
        let driver = Driver::default();
        let recipe = recipe
            .add_configured_plugin(driver.clone(), |driver| DriverPlugin(driver.clone()))
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
                let current = CurrentScene::get(world).unwrap();
                let id = SceneEntityId::new("explorer").unwrap();
                let EntityLookup::Resolved(entity) = current.receipt().resolve(world, &id) else {
                    panic!("the current Scene must resolve its animated explorer")
                };
                world.get::<Sprite>(entity).unwrap().texture_region.unwrap()
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

    fn queue(&self) -> GameplayCommandQueueStats {
        self.session
            .observe(|world| world.resource::<GameplayCommandQueue>().stats())
            .unwrap()
    }

    fn assert_idle(&self) {
        self.session
            .observe(|world| {
                let queue = world.resource::<GameplayCommandQueue>();
                let stats = queue.stats();
                assert!(queue.is_idle(), "{stats:?}");
                assert!(world.resource::<GameplayCommandBatch>().is_empty());
                assert!(world.resource::<LocalGameplayCommands>().is_empty());
                assert_eq!(stats.accepted, stats.admitted);
                assert_eq!(stats.accepted, stats.acknowledged);
                assert_eq!(
                    stats.acknowledged_through_tick,
                    world.resource::<FixedTime>().tick()
                );
                assert_eq!(stats.closed_through_tick, stats.acknowledged_through_tick);
                assert_eq!(stats.retained_commands, 0);
                assert_eq!(stats.retained_bytes, 0);
                assert_eq!(stats.rejected, 0);
                assert_eq!(stats.lifecycle_faults, 0);
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

    fn collect_near_pickup(&mut self) {
        let collected = self.snapshot().collected;
        assert_eq!(self.object("gem-near").1, 1.0);
        self.driver.key(RIGHT, true);
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
                self.driver.key(RIGHT, false);
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
fn game_pause_clears_held_movement_freezes_clocks_and_requires_a_release_and_fresh_press() {
    let mut game = Game::source();
    game.start_round();
    let spawn = game.snapshot().player;
    game.driver.key(RIGHT, true);
    game.frame(STEP);
    assert!((game.snapshot().player.x - spawn.x - 3.2).abs() < 0.0001);
    assert_eq!(game.snapshot().held, [false, true, false, false]);

    game.tap(KeyCode::Escape);
    assert_eq!(game.flow().phase, FlowPhase::Paused);
    assert_eq!(game.session.state(), Some(RuntimeState::Running));
    assert_eq!(game.time_policy(), (false, 0.0));
    assert_eq!(game.snapshot().held, [false; 4]);
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
    assert_eq!(
        acknowledged.acknowledged_ticks,
        pending.acknowledged_ticks + 1
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
fn real_pickups_complete_a_round_and_new_scene_success_resets_progress_counters_and_clock() {
    let project = ProjectCopy::new();
    let save = project.save_path();
    let recipe = with_play_flow(configured_recipe(RunOptions {
        save_path: Some(save.clone()),
        ..RunOptions::default()
    }))
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
    let mut game = Game::from_project(project.root(), with_play_flow(product_recipe()).unwrap());
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

    let mut game = Game::from_project(project.root(), with_play_flow(product_recipe()).unwrap());
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
