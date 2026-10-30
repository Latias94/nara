//! Fixed physical-input consumer shared by the producer test and its prebuilt Player.

use std::{
    sync::atomic::{AtomicUsize, Ordering},
    time::{Duration, Instant},
};

use engine::{
    ProductRecipe,
    advanced_prelude::{CurrentScene, add_startup_scene_activation_systems},
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        StartupStage,
    },
    asset::{
        AssetDependencyGraph, AssetProductRef, AssetReloadRequests, AssetSourceChanges,
        AssetSourceRoot, ImportedProductId, ProjectAssetDatabase, StableAssetId,
    },
    core::ItemLimit,
    ecs::{Res, ResMut, Resource, World, schedule::IntoScheduleConfigs},
    gameplay::{
        ActionCommandBinding, ActionCommandMap, GameplayCommandBatch, GameplayCommandSet,
        GameplayCommandTypeId,
    },
    identity::{EntityLookup, SpawnedSceneInstance},
    input::{
        ActionContext, ActionId, ActionMap, ActionMapBindings, ActionMapDomain,
        ActionMapRuntimeCodec, ActionOutcome, ActionOutcomes, ActionPhase, ButtonDriverInput,
        ButtonInput, InputBinding, InputPointerId, InputSeatId, InputTimeline, InputWindowId,
        KeyCode, MouseButton, SemanticActionCause, SemanticActionOutcome, SemanticActions,
    },
    project_host::{
        ProjectSceneRequestError, ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes,
        advanced::commit_prepared_project_scene,
    },
    runtime_content::RuntimeCodecDefinitionId,
    scene::{
        SceneDocument, SceneEntityId, SceneEntityRecord, SceneEntitySource,
        advanced::SceneProductTransactionLimits,
    },
};

pub const INPUT_FILE: &str = "input.runtime.json";
pub const KEY: KeyCode = KeyCode::ArrowUp;
pub const MOUSE: MouseButton = MouseButton::Right;
pub const MOVE: &str = "consumer.processed-input.move";
pub const FIRE: &str = "consumer.processed-input.fire";
const MOVE_START: &str = "consumer.processed-input.move-start";
const MOVE_STOP: &str = "consumer.processed-input.move-stop";
const FIRE_START: &str = "consumer.processed-input.fire-start";
const FIRE_STOP: &str = "consumer.processed-input.fire-stop";
const WINDOW: InputWindowId = InputWindowId::new(7);
const SEAT: InputSeatId = InputSeatId::new(2);
const POINTER: InputPointerId = InputPointerId::new(3);
static STARTUPS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Case {
    Physical,
    MissingInput,
    MalformedInput,
    WrongIdentity,
    ConflictingInput,
    WrongDigest,
    WrongTarget,
}

impl Case {
    pub const ALL: [Self; 7] = [
        Self::Physical,
        Self::MissingInput,
        Self::MalformedInput,
        Self::WrongIdentity,
        Self::ConflictingInput,
        Self::WrongDigest,
        Self::WrongTarget,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Self::Physical => "physical",
            Self::MissingInput => "missing-input",
            Self::MalformedInput => "malformed-input",
            Self::WrongIdentity => "wrong-identity",
            Self::ConflictingInput => "conflicting-input",
            Self::WrongDigest => "wrong-digest",
            Self::WrongTarget => "wrong-target",
        }
    }

    pub fn parse(name: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|case| case.name() == name)
            .expect("known fixture case")
    }
}

pub fn startup_count() -> usize {
    STARTUPS.load(Ordering::SeqCst)
}

pub fn input_product() -> AssetProductRef {
    product(10)
}

pub fn product(index: u8) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(format!("75e12840-0000-4000-8000-{index:012x}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

pub fn menu() -> ActionContext {
    ActionContext::new("menu").unwrap()
}

fn marker(scene: u8) -> SceneEntityId {
    SceneEntityId::new(format!("level-{scene}")).unwrap()
}

pub fn document(scene: u8) -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(marker(scene))])
}

pub fn recipe(case: Case) -> ProductRecipe {
    let recipe = ProductRecipe::new()
        .add_plugin::<ProbePlugin>()
        .unwrap()
        .add_runtime_asset_codec::<ActionMapDomain, ActionMapRuntimeCodec>(
            RuntimeCodecDefinitionId::new("consumer.processed-input.action-map", 1),
        )
        .unwrap();
    if case == Case::ConflictingInput {
        recipe.add_plugin::<CompetingInputPlugin>().unwrap()
    } else {
        recipe
    }
}

#[derive(Default)]
struct ProbePlugin;

impl Plugin for ProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.processed-input"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[
            engine::input::INPUT_PLUGIN_ID,
            engine::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
        ]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Outcome>()?;
        // Physical bindings are installed exclusively by processed startup admission.
        for (action, phase, command) in [
            (MOVE, ActionPhase::Started, MOVE_START),
            (MOVE, ActionPhase::Released, MOVE_STOP),
            (FIRE, ActionPhase::Started, FIRE_START),
            (FIRE, ActionPhase::Released, FIRE_STOP),
        ] {
            app.world_mut()?
                .resource_mut::<ActionCommandMap>()
                .bind(ActionCommandBinding::new(
                    ActionId::new(action).unwrap(),
                    phase,
                    GameplayCommandTypeId::new(command).unwrap(),
                ))
                .map_err(|error| PluginError::SetupFailed {
                    plugin: Self::declaration().id,
                    message: error.to_string(),
                })?;
        }
        app.add_systems(StartupStage::Core, inspect_first_startup)?;
        add_startup_scene_activation_systems(app, inspect_startup_scene)?;
        app.add_systems(
            CoreStage::FixedUpdate,
            simulate.in_set(GameplayCommandSet::Consume),
        )?;
        app.add_systems(CoreStage::Update, advance)?;
        Ok(())
    }
}

#[derive(Default)]
struct CompetingInputPlugin;

impl Plugin for CompetingInputPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.processed-input.competing-empty-map"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[engine::input::INPUT_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(ActionMap::default())?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Phase {
    #[default]
    Inject,
    Pressed,
    Cancelled,
    Reenabled,
    Released,
    Fresh,
    Request,
    Preparing,
    Ready,
    Done,
}

#[derive(Clone, Debug, Default, Resource)]
pub struct Outcome {
    phase: Phase,
    started: Option<Instant>,
    startup_bindings: Option<ActionMapBindings>,
    raw: Vec<ActionOutcome>,
    semantic: Vec<SemanticActionOutcome>,
    receipts: Vec<SpawnedSceneInstance>,
    request: Option<ProjectSceneRequestId>,
    target: u8,
    commits: u8,
    ready_boundaries: u8,
    rejections: u8,
    moving: bool,
    firing: bool,
    move_starts: u32,
    move_stops: u32,
    shots: u32,
    fire_stops: u32,
    distance: u64,
}

impl Outcome {
    pub fn finished(&self) -> bool {
        self.phase == Phase::Done
    }

    pub fn assert_complete(&self) {
        assert!(self.finished());
        assert_eq!(startup_count(), 1);
        assert_eq!(
            (self.commits, self.ready_boundaries, self.rejections),
            (2, 2, 1)
        );
        assert_eq!(self.receipts.len(), 3);
        for (left, right) in [(0, 1), (1, 2), (0, 2)] {
            assert_ne!(
                self.receipts[left].instance_id(),
                self.receipts[right].instance_id(),
            );
        }
        assert!(self.distance >= 2);
        assert_eq!(
            (
                self.move_starts,
                self.move_stops,
                self.shots,
                self.fire_stops
            ),
            (2, 1, 2, 1),
        );
        assert_eq!(self.raw.len(), 6);
        for (index, edge) in self.raw.iter().enumerate() {
            let pointer = if index % 2 == 0 { None } else { Some(POINTER) };
            let binding = if pointer.is_none() {
                InputBinding::Key(KEY)
            } else {
                InputBinding::Mouse(MOUSE)
            };
            assert_eq!(edge.binding, binding);
            assert_eq!(
                edge.phase,
                if (2..4).contains(&index) {
                    ActionPhase::Released
                } else {
                    ActionPhase::Started
                },
            );
            assert_eq!(edge.provenance.sequence(), Some(index as u64 + 1));
            assert_eq!(edge.provenance.route().window(), WINDOW);
            assert_eq!(edge.provenance.route().seat(), SEAT);
            assert_eq!(edge.provenance.route().pointer(), pointer);
            assert_eq!(
                edge.provenance.press_sequence(),
                Some([1, 2, 1, 2, 5, 6][index])
            );
            let route = edge.provenance.press_route().unwrap();
            assert_eq!(
                (route.window(), route.seat(), route.pointer()),
                (WINDOW, SEAT, pointer),
            );
        }
        assert_eq!(self.semantic.len(), 6);
        for (action, first, fresh) in [(MOVE, 1, 5), (FIRE, 2, 6)] {
            let edges: Vec<_> = self
                .semantic
                .iter()
                .filter(|edge| edge.action.as_str() == action)
                .collect();
            assert_eq!(
                edges.len(),
                3,
                "context cancellation is terminal exactly once"
            );
            assert!(
                edges
                    .iter()
                    .all(|edge| edge.context == ActionContext::gameplay())
            );
            assert_eq!(edges[0].phase, ActionPhase::Started);
            assert_eq!(edges[0].cause, SemanticActionCause::PhysicalInput);
            assert_eq!(edges[0].provenance.unwrap().sequence(), Some(first));
            assert_eq!(edges[1].phase, ActionPhase::Released);
            assert_eq!(edges[1].cause, SemanticActionCause::ContextChanged);
            assert!(edges[1].provenance.is_none());
            assert_eq!(edges[2].phase, ActionPhase::Started);
            assert_eq!(edges[2].cause, SemanticActionCause::PhysicalInput);
            assert_eq!(edges[2].provenance.unwrap().sequence(), Some(fresh));
        }
    }
}

fn assert_source_free(world: &World) {
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    assert!(!world.contains_resource::<AssetDependencyGraph>());
    assert!(!world.contains_resource::<AssetSourceRoot>());
    assert!(!world.contains_resource::<AssetSourceChanges>());
    assert!(!world.contains_resource::<AssetReloadRequests>());
    assert!(!world.contains_resource::<engine::image::ImageImporter>());
}

fn inspect_first_startup(world: &mut World) {
    assert_eq!(STARTUPS.fetch_add(1, Ordering::SeqCst), 0);
    assert_source_free(world);
    let map = world.resource::<ActionMap>();
    assert_eq!(map.bindings().len(), 2);
    assert!(map.is_context_enabled(&ActionContext::gameplay()));
    assert!(!map.is_context_enabled(&menu()));
    let bindings = map.bindings_handle();
    let mut outcome = world.resource_mut::<Outcome>();
    assert!(outcome.startup_bindings.is_none());
    outcome.startup_bindings = Some(bindings);
    outcome.started = Some(Instant::now());
}

fn live_scene(world: &World, scene: u8) -> SpawnedSceneInstance {
    let current = CurrentScene::get(world).unwrap();
    assert_eq!(current.source(), &document(scene));
    let receipt = current.receipt().clone();
    let EntityLookup::Resolved(entity) = receipt.resolve(world, &marker(scene)) else {
        panic!("the current Scene must resolve its authored marker")
    };
    assert!(world.get::<SceneEntitySource>(entity).is_some());
    receipt
}

fn inspect_startup_scene(world: &mut World) {
    let receipt = live_scene(world, 1);
    world.resource_mut::<Outcome>().receipts.push(receipt);
}

fn simulate(batch: Res<GameplayCommandBatch>, mut outcome: ResMut<Outcome>) {
    for command in batch.commands() {
        match command.command_type().as_str() {
            MOVE_START => {
                outcome.moving = true;
                outcome.move_starts += 1;
            }
            MOVE_STOP => {
                outcome.moving = false;
                outcome.move_stops += 1;
            }
            FIRE_START => {
                outcome.firing = true;
                outcome.shots += 1;
            }
            FIRE_STOP => {
                outcome.firing = false;
                outcome.fire_stops += 1;
            }
            other => panic!("unexpected gameplay command {other}"),
        }
    }
    if outcome.moving {
        outcome.distance += 1;
    }
}

fn held(world: &World, expected: bool) {
    for action in [MOVE, FIRE] {
        assert_eq!(
            world
                .resource::<SemanticActions>()
                .pressed(&ActionId::new(action).unwrap(), &ActionContext::gameplay(),),
            expected,
        );
    }
    let outcome = world.resource::<Outcome>();
    assert_eq!((outcome.moving, outcome.firing), (expected, expected));
}

fn inject(world: &mut World, pressed: bool) {
    let mut timeline = world.resource_mut::<InputTimeline>();
    let keyboard = if pressed {
        ButtonDriverInput::Press(KEY)
    } else {
        ButtonDriverInput::Release(KEY)
    };
    let mouse = if pressed {
        ButtonDriverInput::Press(MOUSE)
    } else {
        ButtonDriverInput::Release(MOUSE)
    };
    assert_eq!(
        timeline
            .admit_keyboard(WINDOW, SEAT, keyboard)
            .unwrap()
            .admitted(),
        1
    );
    assert_eq!(
        timeline
            .admit_mouse_button(WINDOW, SEAT, POINTER, mouse)
            .unwrap()
            .admitted(),
        1,
    );
}

fn assert_travel_input(world: &World) {
    let outcome = world.resource::<Outcome>();
    let map = world.resource::<ActionMap>();
    assert!(
        map.bindings_handle()
            .shares_owner_with(outcome.startup_bindings.as_ref().unwrap()),
    );
    assert!(map.is_context_enabled(&ActionContext::gameplay()));
    assert!(
        map.is_context_enabled(&menu()),
        "travel must retain changed context state"
    );
    held(world, true);
    assert_eq!(
        (
            outcome.move_starts,
            outcome.move_stops,
            outcome.shots,
            outcome.fire_stops
        ),
        (2, 1, 2, 1),
    );
    assert_eq!(
        (outcome.raw.len(), outcome.semantic.len()),
        (6, 6),
        "travel must not cancel or restart held input",
    );
    assert!(world.resource::<ButtonInput<KeyCode>>().pressed(KEY));
    assert!(world.resource::<ButtonInput<MouseButton>>().pressed(MOUSE));
}

fn advance(world: &mut World) {
    assert_source_free(world);
    let raw = world.resource::<ActionOutcomes>().as_slice().to_vec();
    let semantic = world.resource::<SemanticActions>().as_slice().to_vec();
    let phase = {
        let mut outcome = world.resource_mut::<Outcome>();
        outcome.raw.extend(raw);
        outcome.semantic.extend(semantic);
        assert!(outcome.started.unwrap().elapsed() < Duration::from_secs(15));
        outcome.phase
    };
    match phase {
        Phase::Inject => {
            inject(world, true);
            world.resource_mut::<Outcome>().phase = Phase::Pressed;
        }
        Phase::Pressed => {
            held(world, true);
            assert!(world.resource::<Outcome>().distance > 0);
            assert!(
                world
                    .resource_mut::<ActionMap>()
                    .disable_context(&ActionContext::gameplay())
                    .unwrap(),
            );
            world.resource_mut::<Outcome>().phase = Phase::Cancelled;
        }
        Phase::Cancelled => {
            held(world, false);
            assert_eq!(world.resource::<Outcome>().semantic.len(), 4);
            {
                let mut map = world.resource_mut::<ActionMap>();
                assert!(!map.disable_context(&ActionContext::gameplay()).unwrap());
                assert!(map.enable_context(&ActionContext::gameplay()).unwrap());
                assert!(map.enable_context(&menu()).unwrap());
            }
            world.resource_mut::<Outcome>().phase = Phase::Reenabled;
        }
        Phase::Reenabled => {
            held(world, false);
            assert_eq!(world.resource::<Outcome>().semantic.len(), 4);
            assert!(world.resource::<ButtonInput<KeyCode>>().pressed(KEY));
            assert!(world.resource::<ButtonInput<MouseButton>>().pressed(MOUSE));
            {
                let mut timeline = world.resource_mut::<InputTimeline>();
                assert!(
                    timeline
                        .admit_keyboard(WINDOW, SEAT, ButtonDriverInput::Press(KEY))
                        .unwrap()
                        .is_ignored(),
                );
                assert!(
                    timeline
                        .admit_mouse_button(WINDOW, SEAT, POINTER, ButtonDriverInput::Press(MOUSE))
                        .unwrap()
                        .is_ignored(),
                );
            }
            inject(world, false);
            world.resource_mut::<Outcome>().phase = Phase::Released;
        }
        Phase::Released => {
            held(world, false);
            assert_eq!(world.resource::<Outcome>().semantic.len(), 4);
            inject(world, true);
            world.resource_mut::<Outcome>().phase = Phase::Fresh;
        }
        Phase::Fresh => {
            assert_travel_input(world);
            world.resource_mut::<Outcome>().phase = Phase::Request;
        }
        Phase::Request | Phase::Preparing | Phase::Ready => travel(world, phase),
        Phase::Done => {}
    }
}

fn travel(world: &mut World, phase: Phase) {
    assert_travel_input(world);
    let (commits, rejections) = {
        let outcome = world.resource::<Outcome>();
        (outcome.commits, outcome.rejections)
    };
    let active = if commits == 0 { 1 } else { 2 };
    let receipt = live_scene(world, active);
    assert_eq!(world.resource::<Outcome>().receipts.last(), Some(&receipt));
    match phase {
        Phase::Request => {
            let target = if rejections == 0 {
                3
            } else if commits == 0 {
                2
            } else {
                1
            };
            let request = ProjectScenes::scope(world, |scenes| {
                let request = scenes.request(product(target)).unwrap();
                assert_eq!(
                    scenes.state(request),
                    Some(ProjectSceneRequestState::Preparing)
                );
                assert!(scenes.prepared(request).is_none());
                request
            });
            let mut outcome = world.resource_mut::<Outcome>();
            outcome.request = Some(request);
            outcome.target = target;
            outcome.phase = Phase::Preparing;
        }
        Phase::Preparing => {
            let request = world.resource::<Outcome>().request.unwrap();
            let state = ProjectScenes::scope(world, |scenes| scenes.state(request)).unwrap();
            if state == ProjectSceneRequestState::Preparing {
                std::thread::sleep(Duration::from_millis(1));
                return;
            }
            if let ProjectSceneRequestState::Rejected(error) = state {
                assert_eq!(world.resource::<Outcome>().target, 3);
                assert_eq!(rejections, 0);
                assert_eq!(error, ProjectSceneRequestError::RuntimeContent);
                ProjectScenes::scope(world, |scenes| {
                    assert!(scenes.prepared(request).is_none());
                    assert!(scenes.diagnostics(request).unwrap().has_errors());
                    assert!(scenes.acknowledge(request));
                });
                assert_eq!(live_scene(world, active), receipt);
                assert_travel_input(world);
                let mut outcome = world.resource_mut::<Outcome>();
                outcome.rejections += 1;
                outcome.request = None;
                outcome.phase = Phase::Request;
                return;
            }
            assert_eq!(state, ProjectSceneRequestState::Ready);
            let target = world.resource::<Outcome>().target;
            ProjectScenes::scope(world, |scenes| {
                let prepared = scenes.prepared(request).unwrap();
                assert_eq!(prepared.selected_scene_product(), Some(product(target)));
                assert!(prepared.selected_scene_path().is_none());
                assert_eq!(prepared.expected_instance(), &receipt);
                assert_eq!(prepared.scene(), &document(target));
            });
            let mut outcome = world.resource_mut::<Outcome>();
            outcome.ready_boundaries += 1;
            outcome.phase = Phase::Ready;
        }
        Phase::Ready => {
            let (request, target) = {
                let outcome = world.resource::<Outcome>();
                (outcome.request.unwrap(), outcome.target)
            };
            let EntityLookup::Resolved(old_entity) = receipt.resolve(world, &marker(active)) else {
                panic!("the previous Scene must remain live until explicit commit")
            };
            let report = commit_prepared_project_scene(
                world,
                request,
                SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
                &[],
                |_| {},
            );
            assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
            let published = report.instance.unwrap();
            assert_eq!(live_scene(world, target), published);
            assert_ne!(published.instance_id(), receipt.instance_id());
            assert!(world.get::<SceneEntitySource>(old_entity).is_none());
            assert_travel_input(world);
            ProjectScenes::scope(world, |scenes| {
                assert_eq!(
                    scenes.state(request),
                    Some(ProjectSceneRequestState::Committed)
                );
                assert!(scenes.acknowledge(request));
            });
            let mut outcome = world.resource_mut::<Outcome>();
            outcome.receipts.push(published);
            outcome.commits += 1;
            outcome.request = None;
            outcome.phase = if outcome.commits == 2 {
                Phase::Done
            } else {
                Phase::Request
            };
        }
        _ => unreachable!(),
    }
}
