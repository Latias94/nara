//! Source-authored physical Input through the public Headless Host and explicit Scene travel.

use std::{
    fs,
    num::NonZeroU32,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use nara_engine::{
    ProductConfiguration, ProductRecipe,
    advanced_prelude::{CurrentScene, add_startup_scene_activation_systems},
    app::{
        App, CoreStage, FixedTime, Plugin, PluginCategory, PluginDeclaration, PluginError,
        PluginId, StartupStage,
    },
    asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetProductRef, AssetSourceKind,
        ImportedProductId, StableAssetId,
    },
    core::{ByteLimit, ItemLimit},
    ecs::{Res, ResMut, Resource, World, schedule::IntoScheduleConfigs},
    fs::{ContentDigest, DirectoryCapability, RelativePath},
    gameplay::{
        ActionCommandBinding, ActionCommandMap, GameplayCommandBatch, GameplayCommandSet,
        GameplayCommandTypeId,
    },
    identity::{EntityLookup, SpawnedSceneInstance},
    input::{
        ActionBinding, ActionContext, ActionId, ActionMap, ActionMapBindings, ActionMapBuilder,
        ActionMapDefinition, ActionMapFileCandidate, ActionMapLimits, ActionOutcome,
        ActionOutcomes, ActionPhase, ButtonDriverInput, ButtonInput, InputBinding, InputPointerId,
        InputSeatId, InputTimeline, InputWindowId, KeyCode, MouseButton, SemanticActionCause,
        SemanticActionOutcome, SemanticActions,
    },
    project_host::{
        HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, HeadlessRunReport,
        ProjectContentErrorKind, ProjectContentMemberKind, ProjectSceneRequestError,
        ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes,
        advanced::commit_prepared_project_scene, ingest_project_manifest,
        open_explicit_product_project_root, resolve_product_recipe,
    },
    scene::{
        SceneDocument, SceneEntityId, SceneEntityRecord, SceneEntitySource,
        advanced::SceneProductTransactionLimits,
    },
};

const INPUT_PATH: &str = "input/player.actions.json";
const INPUT_SOURCE: &str = "186c8d14-57a8-4f5a-8d3a-dfe97c8a246b";
const MOVE: &str = "consumer.source-input.move";
const FIRE: &str = "consumer.source-input.fire";
const MOVE_START: &str = "consumer.source-input.move-start";
const MOVE_STOP: &str = "consumer.source-input.move-stop";
const FIRE_START: &str = "consumer.source-input.fire-start";
const FIRE_STOP: &str = "consumer.source-input.fire-stop";
const RESTART_REQUIRED: &str = "project.content.global-input-source-changed";
const WINDOW: InputWindowId = InputWindowId::new(7);
const SEAT: InputSeatId = InputSeatId::new(2);
const POINTER: InputPointerId = InputPointerId::new(3);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
enum Case {
    RoundTrip,
    ChangedBeforeTravel,
    ChangedAfterReady,
    Restart,
    CodeFirst,
    CompetingEmpty,
}

impl Case {
    fn key(self) -> KeyCode {
        if self == Self::Restart {
            KeyCode::ArrowUp
        } else {
            KeyCode::Character('w')
        }
    }
}

#[derive(Clone, Resource)]
struct Configuration {
    case: Case,
    root: PathBuf,
}

impl ProductConfiguration for Configuration {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.push(self.case as u8);
        let root = self.root.to_str().unwrap().as_bytes();
        output.extend_from_slice(&(root.len() as u64).to_le_bytes());
        output.extend_from_slice(root);
    }
}

struct ConsumerPlugin(Configuration);

impl Plugin for ConsumerPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.source-input"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[
            nara_engine::input::INPUT_PLUGIN_ID,
            nara_engine::gameplay::GAMEPLAY_COMMAND_PLUGIN_ID,
        ]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.insert_resource(self.0.clone())?
            .init_resource::<Outcome>()?;
        // The file-backed consumer configures semantic commands, never physical bindings.
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

// Only the two explicit code-configuration controls install a physical map in a plugin.
struct CodeInputPlugin(bool);

impl Plugin for CodeInputPlugin {
    fn declaration() -> &'static PluginDeclaration {
        const DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.source-input.code-control"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara_engine::input::INPUT_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        let map = if self.0 {
            ActionMap::default()
        } else {
            ActionMap::new(
                definition(KeyCode::Character('w')).into_bindings(),
                ByteLimit::new(ActionMap::required_state_bytes()).unwrap(),
            )
            .unwrap()
        };
        app.insert_resource(map)?;
        Ok(())
    }
}

fn definition(key: KeyCode) -> ActionMapDefinition {
    let mut builder = ActionMapBuilder::new(ActionMapLimits::default());
    builder
        .declare_context(ActionContext::gameplay(), true)
        .unwrap();
    builder.declare_context(menu(), false).unwrap();
    builder
        .bind(ActionBinding::key(ActionId::new(MOVE).unwrap(), key))
        .unwrap();
    builder
        .bind(ActionBinding::mouse(
            ActionId::new(FIRE).unwrap(),
            MouseButton::Left,
        ))
        .unwrap();
    builder.compile().unwrap()
}

fn input_json(key: KeyCode) -> String {
    ActionMapFileCandidate::new(
        AssetProductRef::new(
            StableAssetId::parse_str(INPUT_SOURCE).unwrap(),
            ImportedProductId::PRIMARY,
        )
        .unwrap(),
        definition(key),
    )
    .unwrap()
    .to_json_string()
    .unwrap()
}

fn menu() -> ActionContext {
    ActionContext::new("menu").unwrap()
}

fn scene_name(scene: u8) -> String {
    format!("{scene}.scene.json")
}

fn marker(scene: u8) -> SceneEntityId {
    SceneEntityId::new(format!("level-{scene}")).unwrap()
}

fn document(scene: u8) -> SceneDocument {
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(marker(scene))])
}

fn scene_product(scene: u8) -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str(format!("186c8d14-57a8-4f5a-8d3a-{scene:012x}")).unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn write_input(root: &Path, key: KeyCode) {
    let target = root.join(INPUT_PATH);
    assert!(root.is_absolute() && target.starts_with(root));
    fs::write(target, input_json(key)).unwrap();
}

struct Fixture {
    directory: tempfile::TempDir,
    root: DirectoryCapability,
}

impl Fixture {
    fn new(selected: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        for path in ["assets", "scenes", "prefabs", "input", "schema"] {
            fs::create_dir_all(root.join(path)).unwrap();
        }
        let mut manifest = concat!(
            "schema_version = 1\n[project]\nname = 'Source Input consumer'\n",
            "source_universe = '186c8d14-57a8-4f5a-8d3a-000000000099'\n",
            "native_document_identity = 'stable_metadata'\n",
            "[paths]\nassets = 'assets'\nscenes = 'scenes'\nprefabs = 'prefabs'\n",
            "[startup]\ndefault_scene = '1.scene.json'\n",
            "[content]\nscenes = ['2.scene.json', '3.scene.json']\n",
            "[authoring]\nschema_bundle = 'schema/authoring.bundle.json'\n",
            "[runtime]\npreset = 'local-headless'\n",
            "[capabilities]\nrequested = ['runtime-2d']\n",
            "[tasks.io]\nworkers = 1\npending_capacity = 16\n",
            "[tasks.compute]\nworkers = 1\npending_capacity = 16\n",
            "[tasks.async_compute]\nworkers = 1\npending_capacity = 16\n",
        )
        .to_owned();
        if selected {
            manifest.push_str(&format!("[input]\naction_map = '{INPUT_PATH}'\n"));
            write_input(root, KeyCode::Character('w'));
        }
        fs::write(root.join("nara.toml"), manifest).unwrap();
        for scene in [1, 2, 3] {
            let path = scene_name(scene);
            fs::write(
                root.join("scenes").join(&path),
                document(scene).to_json_string().unwrap(),
            )
            .unwrap();
            let metadata = AssetMeta::new(
                AssetMetaIdentity::new(
                    scene_product(scene).source(),
                    AssetPath::new(&path).unwrap(),
                    AssetSourceKind::Scene,
                ),
                (),
            )
            .to_json_string()
            .unwrap();
            fs::write(root.join("scenes").join(format!("{path}.meta")), metadata).unwrap();
        }
        // C is declared but is not a startup dependency; only its later request must fail.
        fs::write(root.join("scenes").join(scene_name(3)), b"{invalid Scene").unwrap();
        let capability = open_explicit_product_project_root(root).unwrap();
        Self {
            directory,
            root: capability,
        }
    }

    fn run(&self, case: Case) -> HeadlessRunReport<Outcome> {
        let configuration = Configuration {
            case,
            root: self.directory.path().to_owned(),
        };
        let mut recipe = ProductRecipe::new()
            .add_configured_plugin::<ConsumerPlugin, _, _>(configuration.clone(), |configuration| {
                ConsumerPlugin(configuration.clone())
            })
            .unwrap();
        if matches!(case, Case::CodeFirst | Case::CompetingEmpty) {
            recipe = recipe
                .add_configured_plugin::<CodeInputPlugin, _, _>(configuration, |configuration| {
                    CodeInputPlugin(configuration.case == Case::CompetingEmpty)
                })
                .unwrap();
        }
        let manifest = self
            .root
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, recipe.clone()).unwrap();
        fs::write(
            self.directory.path().join("schema/authoring.bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        drop(manifest);
        let intent = HeadlessRunIntent::frames(
            NonZeroU32::new(20_000).unwrap(),
            FixedTime::DEFAULT_TIMESTEP,
        )
        .with_recipe(recipe)
        .stop_when(|outcome: &Outcome| outcome.phase == Phase::Done);
        let mut run = HeadlessRun::<Outcome>::new(self.root.share(), intent, Vec::new());
        let mut cleanup_deadline = None;
        loop {
            let report = run.execute_bounded();
            if !matches!(report.outcome(), HeadlessRunOutcome::CleanupIncomplete) {
                return report;
            }
            let deadline =
                cleanup_deadline.get_or_insert_with(|| Instant::now() + Duration::from_secs(10));
            assert!(
                Instant::now() < *deadline,
                "source Input cleanup did not complete"
            );
            std::thread::sleep(Duration::from_millis(1));
        }
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
struct Outcome {
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
    rejection_codes: Vec<String>,
    moving: bool,
    firing: bool,
    move_starts: u32,
    move_stops: u32,
    shots: u32,
    fire_stops: u32,
    distance: u64,
}

fn inspect_first_startup(world: &mut World) {
    let key = world.resource::<Configuration>().case.key();
    let map = world.resource::<ActionMap>();
    assert_eq!(map.bindings(), definition(key).bindings());
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
        panic!("the current Scene must resolve its authored entity")
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
    let semantic = world.resource::<SemanticActions>();
    for action in [MOVE, FIRE] {
        assert_eq!(
            semantic.pressed(&ActionId::new(action).unwrap(), &ActionContext::gameplay()),
            expected
        );
    }
    let outcome = world.resource::<Outcome>();
    assert_eq!(outcome.moving, expected);
    assert_eq!(outcome.firing, expected);
}

fn inject(world: &mut World, pressed: bool) {
    let key = world.resource::<Configuration>().case.key();
    let mut timeline = world.resource_mut::<InputTimeline>();
    let keyboard = if pressed {
        ButtonDriverInput::Press(key)
    } else {
        ButtonDriverInput::Release(key)
    };
    let mouse = if pressed {
        ButtonDriverInput::Press(MouseButton::Left)
    } else {
        ButtonDriverInput::Release(MouseButton::Left)
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
        1
    );
}

fn assert_travel_input(world: &World) {
    let outcome = world.resource::<Outcome>();
    let map = world.resource::<ActionMap>();
    assert!(
        map.bindings_handle()
            .shares_owner_with(outcome.startup_bindings.as_ref().unwrap())
    );
    assert!(map.is_context_enabled(&ActionContext::gameplay()));
    assert!(
        map.is_context_enabled(&menu()),
        "travel must retain a changed context state"
    );
    held(world, true);
    assert_eq!(
        (
            outcome.move_starts,
            outcome.move_stops,
            outcome.shots,
            outcome.fire_stops
        ),
        (2, 1, 2, 1)
    );
    assert_eq!(
        outcome.semantic.len(),
        6,
        "travel cannot cancel or restart held actions"
    );
    assert_eq!(outcome.raw.len(), 6);
    assert!(
        world
            .resource::<ButtonInput<KeyCode>>()
            .pressed(world.resource::<Configuration>().case.key())
    );
    assert!(
        world
            .resource::<ButtonInput<MouseButton>>()
            .pressed(MouseButton::Left)
    );
}

fn advance(world: &mut World) {
    let raw = world.resource::<ActionOutcomes>().as_slice().to_vec();
    let semantic = world.resource::<SemanticActions>().as_slice().to_vec();
    {
        let mut outcome = world.resource_mut::<Outcome>();
        outcome.raw.extend(raw);
        outcome.semantic.extend(semantic);
        assert!(outcome.started.unwrap().elapsed() < Duration::from_secs(15));
    }
    let (phase, case) = (
        world.resource::<Outcome>().phase,
        world.resource::<Configuration>().case,
    );
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
                    .disable_context(ActionContext::gameplay())
                    .unwrap()
            );
            world.resource_mut::<Outcome>().phase = Phase::Cancelled;
        }
        Phase::Cancelled => {
            held(world, false);
            assert_eq!(world.resource::<Outcome>().semantic.len(), 4);
            {
                let mut map = world.resource_mut::<ActionMap>();
                assert!(!map.disable_context(ActionContext::gameplay()).unwrap());
                assert!(map.enable_context(&ActionContext::gameplay()).unwrap());
                assert!(map.enable_context(&menu()).unwrap());
            }
            world.resource_mut::<Outcome>().phase = Phase::Reenabled;
        }
        Phase::Reenabled => {
            held(world, false);
            assert_eq!(world.resource::<Outcome>().semantic.len(), 4);
            let key = case.key();
            assert!(world.resource::<ButtonInput<KeyCode>>().pressed(key));
            assert!(
                world
                    .resource::<ButtonInput<MouseButton>>()
                    .pressed(MouseButton::Left)
            );
            {
                let mut timeline = world.resource_mut::<InputTimeline>();
                assert!(
                    timeline
                        .admit_keyboard(WINDOW, SEAT, ButtonDriverInput::Press(key))
                        .unwrap()
                        .is_ignored()
                );
                assert!(
                    timeline
                        .admit_mouse_button(
                            WINDOW,
                            SEAT,
                            POINTER,
                            ButtonDriverInput::Press(MouseButton::Left)
                        )
                        .unwrap()
                        .is_ignored()
                );
            }
            inject(world, false);
            world.resource_mut::<Outcome>().phase = Phase::Released;
        }
        Phase::Released => {
            held(world, false);
            assert_eq!(
                world.resource::<Outcome>().semantic.len(),
                4,
                "cancelled releases cannot terminate an action twice"
            );
            inject(world, true);
            world.resource_mut::<Outcome>().phase = Phase::Fresh;
        }
        Phase::Fresh => {
            assert_travel_input(world);
            if matches!(case, Case::Restart | Case::CodeFirst) {
                world.resource_mut::<Outcome>().phase = Phase::Done;
            } else {
                if case == Case::ChangedBeforeTravel {
                    write_input(&world.resource::<Configuration>().root, KeyCode::ArrowUp);
                }
                world.resource_mut::<Outcome>().phase = Phase::Request;
            }
        }
        Phase::Request | Phase::Preparing | Phase::Ready => travel(world, phase, case),
        Phase::Done => {}
    }
}

fn travel(world: &mut World, phase: Phase, case: Case) {
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
            let target = if case == Case::RoundTrip && rejections == 0 {
                3
            } else if commits == 0 {
                2
            } else {
                1
            };
            let request = ProjectScenes::scope(world, |scenes| {
                let request = scenes.request(scene_product(target)).unwrap();
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
                if case == Case::RoundTrip {
                    assert_eq!(world.resource::<Outcome>().target, 3);
                    assert!(matches!(error, ProjectSceneRequestError::Content(_)));
                } else {
                    assert!(matches!(
                        case,
                        Case::ChangedBeforeTravel | Case::ChangedAfterReady
                    ));
                    assert_eq!(
                        error,
                        ProjectSceneRequestError::Content(
                            ProjectContentErrorKind::HostAuthorityRejected
                        )
                    );
                }
                let codes = ProjectScenes::scope(world, |scenes| {
                    assert!(scenes.prepared(request).is_none());
                    let diagnostics = scenes.diagnostics(request).unwrap();
                    assert!(diagnostics.has_errors());
                    let codes: Vec<_> = diagnostics
                        .iter()
                        .map(|entry| entry.code().as_str().to_owned())
                        .collect();
                    assert!(scenes.acknowledge(request));
                    codes
                });
                assert_eq!(live_scene(world, active), receipt);
                assert_travel_input(world);
                if case != Case::RoundTrip {
                    assert!(codes.iter().any(|code| code == RESTART_REQUIRED));
                }
                let mut outcome = world.resource_mut::<Outcome>();
                outcome.rejections += 1;
                outcome.rejection_codes.extend(codes);
                outcome.request = None;
                outcome.phase = if case == Case::RoundTrip {
                    Phase::Request
                } else {
                    Phase::Done
                };
                return;
            }
            assert_eq!(state, ProjectSceneRequestState::Ready);
            let target = world.resource::<Outcome>().target;
            ProjectScenes::scope(world, |scenes| {
                let prepared = scenes.prepared(request).unwrap();
                assert_eq!(
                    prepared.selected_scene_product(),
                    Some(scene_product(target))
                );
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
            if case == Case::ChangedAfterReady && commits == 0 {
                // Preparation already returned: commit consumes captured facts without a file read.
                write_input(&world.resource::<Configuration>().root, KeyCode::ArrowUp);
            }
            let EntityLookup::Resolved(old_entity) = receipt.resolve(world, &marker(active)) else {
                panic!("the previous Scene must still be live before explicit commit")
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

fn completed(report: HeadlessRunReport<Outcome>, key: KeyCode, selected: bool) -> Outcome {
    assert!(
        !report.diagnostics().has_errors(),
        "{:?}",
        report.diagnostics()
    );
    let input_members: Vec<_> = report
        .startup_content_closure()
        .unwrap()
        .members()
        .iter()
        .filter(|member| member.kind() == ProjectContentMemberKind::InputActionMap)
        .collect();
    assert_eq!(input_members.len(), usize::from(selected));
    if selected {
        assert_eq!(input_members[0].path().as_str(), INPUT_PATH);
        assert_eq!(
            input_members[0].digest(),
            ContentDigest::of_bytes(input_json(key).as_bytes())
        );
    }
    let HeadlessRunOutcome::Completed(outcome) = report.into_outcome() else {
        panic!("the public physical Input workflow must complete")
    };
    assert_eq!(outcome.phase, Phase::Done);
    assert!(outcome.distance >= 2);
    assert_eq!(
        (
            outcome.move_starts,
            outcome.move_stops,
            outcome.shots,
            outcome.fire_stops
        ),
        (2, 1, 2, 1)
    );
    assert_physical_outcomes(&outcome, key);
    outcome
}

fn assert_physical_outcomes(outcome: &Outcome, key: KeyCode) {
    assert_eq!(outcome.raw.len(), 6);
    for (index, edge) in outcome.raw.iter().enumerate() {
        let pointer = if index % 2 == 0 { None } else { Some(POINTER) };
        let binding = if pointer.is_none() {
            InputBinding::Key(key)
        } else {
            InputBinding::Mouse(MouseButton::Left)
        };
        assert_eq!(edge.binding, binding);
        assert_eq!(
            edge.phase,
            if (2..4).contains(&index) {
                ActionPhase::Released
            } else {
                ActionPhase::Started
            }
        );
        assert_eq!(edge.provenance.sequence(), Some(index as u64 + 1));
        assert_eq!(edge.provenance.route().window(), WINDOW);
        assert_eq!(edge.provenance.route().seat(), SEAT);
        assert_eq!(edge.provenance.route().pointer(), pointer);
        assert_eq!(
            edge.provenance.press_sequence(),
            Some([1, 2, 1, 2, 5, 6][index])
        );
        assert_eq!(edge.provenance.press_route().unwrap().pointer(), pointer);
    }
    assert_eq!(outcome.semantic.len(), 6);
    for (action, first, fresh) in [(MOVE, 1, 5), (FIRE, 2, 6)] {
        let edges: Vec<_> = outcome
            .semantic
            .iter()
            .filter(|edge| edge.action.as_str() == action)
            .collect();
        assert_eq!(
            edges.len(),
            3,
            "each context cancellation is terminal exactly once"
        );
        for edge in &edges {
            assert_eq!(edge.context, ActionContext::gameplay());
        }
        assert_eq!(edges[0].phase, ActionPhase::Started);
        assert_eq!(edges[0].cause, SemanticActionCause::PhysicalInput);
        assert_eq!(edges[0].provenance.unwrap().sequence(), Some(first));
        assert_eq!(edges[1].phase, ActionPhase::Released);
        assert_eq!(edges[1].cause, SemanticActionCause::ContextChanged);
        assert_eq!(edges[1].provenance, None);
        assert_eq!(edges[2].phase, ActionPhase::Started);
        assert_eq!(edges[2].cause, SemanticActionCause::PhysicalInput);
        assert_eq!(edges[2].provenance.unwrap().sequence(), Some(fresh));
    }
}

#[test]
fn file_input_drives_gameplay_and_survives_rejection_and_a_to_b_to_a_travel() {
    let fixture = Fixture::new(true);
    let outcome = completed(fixture.run(Case::RoundTrip), Case::RoundTrip.key(), true);
    assert_eq!(
        (
            outcome.commits,
            outcome.ready_boundaries,
            outcome.rejections
        ),
        (2, 2, 1)
    );
    assert_eq!(outcome.receipts.len(), 3);
    for (left, right) in [(0, 1), (1, 2), (0, 2)] {
        assert_ne!(
            outcome.receipts[left].instance_id(),
            outcome.receipts[right].instance_id()
        );
    }
    assert!(
        !outcome
            .rejection_codes
            .iter()
            .any(|code| code == RESTART_REQUIRED)
    );
}

#[test]
fn changed_input_refuses_travel_until_a_fresh_host_loads_the_new_bindings() {
    let fixture = Fixture::new(true);
    let before = completed(
        fixture.run(Case::ChangedBeforeTravel),
        Case::ChangedBeforeTravel.key(),
        true,
    );
    assert_eq!(
        (before.commits, before.ready_boundaries, before.rejections),
        (0, 0, 1)
    );
    assert_eq!(before.receipts.len(), 1);
    let restarted = completed(fixture.run(Case::Restart), Case::Restart.key(), true);
    let original = before.startup_bindings.as_ref().unwrap();
    let successor = restarted.startup_bindings.as_ref().unwrap();
    assert!(!original.shares_owner_with(successor));
    assert_eq!(
        original.definition().bindings()[0].input,
        InputBinding::Key(KeyCode::Character('w'))
    );
    assert_eq!(
        successor.definition().bindings()[0].input,
        InputBinding::Key(KeyCode::ArrowUp)
    );
}

#[test]
fn ready_commit_uses_captured_input_then_the_next_request_requires_restart() {
    let fixture = Fixture::new(true);
    let outcome = completed(
        fixture.run(Case::ChangedAfterReady),
        Case::ChangedAfterReady.key(),
        true,
    );
    assert_eq!(
        (
            outcome.commits,
            outcome.ready_boundaries,
            outcome.rejections
        ),
        (1, 1, 1)
    );
    assert_eq!(outcome.receipts.len(), 2);
    assert!(
        outcome
            .rejection_codes
            .iter()
            .any(|code| code == RESTART_REQUIRED)
    );
}

#[test]
fn no_file_selection_preserves_explicit_code_input() {
    let fixture = Fixture::new(false);
    let outcome = completed(fixture.run(Case::CodeFirst), Case::CodeFirst.key(), false);
    assert_eq!((outcome.commits, outcome.rejections), (0, 0));
}

#[test]
fn an_explicit_empty_plugin_map_competes_with_the_selected_file() {
    let fixture = Fixture::new(true);
    let report = fixture.run(Case::CompetingEmpty);
    assert!(matches!(report.outcome(), HeadlessRunOutcome::Failed));
    assert!(report.startup_content_closure().is_some());
    assert!(
        report
            .diagnostics()
            .iter()
            .any(|entry| entry.code().as_str() == "project.run.input-selection-rejected"),
        "{:?}",
        report.diagnostics()
    );
}
