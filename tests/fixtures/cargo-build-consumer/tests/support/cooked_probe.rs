//! Physical platform events, retained global Input, and explicit processed Scene travel.

use std::time::{Duration, Instant};

use engine::{
    advanced_prelude::{CurrentScene, add_startup_scene_activation_systems},
    app::{App, CoreStage, Plugin, PluginError, StartupStage},
    asset::{
        AssetDependencyGraph, AssetRef, AssetReloadRequests, AssetServer, AssetSourceChanges,
        AssetSourceRoot, Assets, ProjectAssetDatabase,
    },
    core::ItemLimit,
    ecs::{Res, ResMut, Resource, World, schedule::IntoScheduleConfigs},
    gameplay::{
        ActionCommandBinding, ActionCommandMap, GameplayCommandBatch, GameplayCommandSet,
        GameplayCommandTypeId,
    },
    identity::{EntityLookup, SpawnedSceneInstance},
    image::{ImageAsset, ImageDomain, ImageExtent},
    input::{
        ActionContext, ActionId, ActionMap, ActionMapBindings, ActionOutcomes, ActionPhase,
        ButtonDriverInput, ButtonInput, InputBinding, InputEventOrigin, InputPointerId,
        InputSeatId, InputTimeline, InputWindowId, KeyCode, MouseButton, SemanticActionCause,
        SemanticActions,
    },
    project_host::{
        ProjectDomainAssets, ProjectSceneRequestError, ProjectSceneRequestId,
        ProjectSceneRequestState, ProjectScenes, advanced::commit_prepared_project_scene,
    },
    scene::{SceneEntitySource, advanced::SceneProductTransactionLimits},
    tilemap::TileSetDomain,
};
use nara_asset_domain_consumer::TextBundleDomain;

pub const MOVE: &str = "consumer.cooked-player.move";
pub const FIRE: &str = "consumer.cooked-player.fire";
pub const KEY: KeyCode = KeyCode::ArrowUp;
pub const MOUSE: MouseButton = MouseButton::Right;
const WINDOW: InputWindowId = InputWindowId::new(7);
const SEAT: InputSeatId = InputSeatId::new(2);
const POINTER: InputPointerId = InputPointerId::new(3);

pub fn menu() -> ActionContext {
    ActionContext::new("menu").unwrap()
}

pub fn install(app: &mut App) -> Result<(), PluginError> {
    app.init_resource::<Outcome>()?;
    for (action, phase, command) in [
        (MOVE, ActionPhase::Started, "move-start"),
        (MOVE, ActionPhase::Released, "move-stop"),
        (FIRE, ActionPhase::Started, "fire-start"),
        (FIRE, ActionPhase::Released, "fire-stop"),
    ] {
        app.world_mut()?
            .resource_mut::<ActionCommandMap>()
            .bind(ActionCommandBinding::new(
                ActionId::new(action).unwrap(),
                phase,
                GameplayCommandTypeId::new(command).unwrap(),
            ))
            .map_err(|error| PluginError::SetupFailed {
                plugin: super::GamePlugin::declaration().id,
                message: error.to_string(),
            })?;
    }
    app.add_systems(StartupStage::Core, startup)?;
    add_startup_scene_activation_systems(app, activated)?;
    app.add_systems(
        CoreStage::FixedUpdate,
        simulate.in_set(GameplayCommandSet::Consume),
    )?;
    app.add_systems(CoreStage::Update, advance)?;
    Ok(())
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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

#[derive(Debug, Clone, Default, Resource)]
pub struct Outcome {
    phase: Phase,
    started: Option<Instant>,
    bindings: Option<ActionMapBindings>,
    receipts: Vec<SpawnedSceneInstance>,
    request: Option<ProjectSceneRequestId>,
    pub commits: u8,
    pub rejections: u8,
    ready: u8,
    raw: usize,
    semantic: usize,
    moving: bool,
    firing: bool,
    move_starts: usize,
    move_stops: usize,
    fire_starts: usize,
    fire_stops: usize,
    distance: usize,
}

impl Outcome {
    pub fn finished(&self) -> bool {
        self.phase == Phase::Done
    }
    pub fn assert_complete(&self) {
        assert!(self.finished());
        assert!([(3, 0), (2, 1)].contains(&(self.commits, self.rejections)));
        assert_eq!(self.ready, self.commits);
        assert_eq!(self.receipts.len(), usize::from(self.commits) + 1);
        assert_eq!(
            (
                self.move_starts,
                self.move_stops,
                self.fire_starts,
                self.fire_stops
            ),
            (2, 1, 2, 1)
        );
        assert_eq!((self.raw, self.semantic), (6, 6));
        assert!(self.distance >= 2);
        for pair in self.receipts.windows(2) {
            assert_ne!(pair[0].instance_id(), pair[1].instance_id());
        }
    }
}

fn source_free(world: &World) {
    assert!(!world.contains_resource::<ProjectAssetDatabase>());
    assert!(!world.contains_resource::<AssetDependencyGraph>());
    assert!(!world.contains_resource::<AssetSourceRoot>());
    assert!(!world.contains_resource::<AssetSourceChanges>());
    assert!(!world.contains_resource::<AssetReloadRequests>());
    assert!(!world.contains_resource::<engine::image::ImageImporter>());
}

fn startup(world: &mut World) {
    source_free(world);
    {
        let mut timeline = world.resource_mut::<InputTimeline>();
        timeline.connect_endpoint(WINDOW, SEAT).unwrap();
        timeline.connect_pointer(WINDOW, SEAT, POINTER).unwrap();
    }
    let map = world.resource::<ActionMap>();
    assert_eq!(map.bindings().len(), 2);
    assert!(map.is_context_enabled(&ActionContext::gameplay()));
    assert!(!map.is_context_enabled(&menu()));
    let bindings = map.bindings_handle();
    let mut outcome = world.resource_mut::<Outcome>();
    assert!(outcome.started.is_none());
    outcome.started = Some(Instant::now());
    outcome.bindings = Some(bindings);
}

fn live(world: &World, scene: u8) -> SpawnedSceneInstance {
    let current = CurrentScene::get(world).unwrap();
    assert_eq!(current.source(), &super::document(scene));
    let receipt = current.receipt().clone();
    let EntityLookup::Resolved(entity) = receipt.resolve(world, &super::marker(scene)) else {
        panic!("live Scene marker")
    };
    assert!(world.get::<SceneEntitySource>(entity).is_some());
    let links = world.get::<super::Links>(entity).unwrap();
    assert_eq!(links.image, AssetRef::Product(super::product(super::IMAGE)));
    assert_eq!(
        links.tileset,
        AssetRef::Product(super::product(super::TILESET))
    );
    assert_eq!(links.captions[1], AssetRef::Product(super::caption()));
    let domains = world.resource::<ProjectDomainAssets>();
    let images = domains.collection::<ImageDomain>().unwrap();
    assert_eq!(images.source_records().len(), 0);
    assert_eq!(images.iter().len(), 1);
    let image = images.get(&links.image).unwrap();
    assert_eq!(image.extent(), ImageExtent::new(4, 1));
    assert_eq!(image.pixels(), super::cooked_pixels());
    assert!(image.source().is_none());
    let native = world.resource::<Assets<ImageAsset>>();
    assert_eq!(native.len(), 1);
    let (handle, published) = native.iter().next().unwrap();
    assert_eq!(published.pixels().as_ptr(), image.pixels().as_ptr());
    assert_eq!(
        world.resource::<AssetServer>().product(handle.id()),
        Some(super::product(super::IMAGE))
    );
    assert!(world.resource::<AssetServer>().path(handle.id()).is_none());
    let tiles = domains.collection::<TileSetDomain>().unwrap();
    assert_eq!(tiles.source_records().len(), 0);
    let tileset = tiles.get(&links.tileset).unwrap();
    assert_eq!(tileset.image_product(), super::product(super::IMAGE));
    tileset.validate_image_extent(image.extent()).unwrap();
    let texts = domains.collection::<TextBundleDomain>().unwrap();
    assert_eq!(texts.source_records().len(), 0);
    assert_eq!(texts.iter().len(), 1);
    let primary = texts.get(&links.captions[0]).unwrap();
    let auxiliary = texts.get(&links.captions[1]).unwrap();
    assert!(std::ptr::eq(primary, auxiliary));
    assert_eq!(
        auxiliary.get(&super::caption()).unwrap().text,
        "Hello cooked world"
    );
    receipt
}

fn activated(world: &mut World) {
    let receipt = live(world, 1);
    world.resource_mut::<Outcome>().receipts.push(receipt);
}

fn simulate(batch: Res<GameplayCommandBatch>, mut outcome: ResMut<Outcome>) {
    for command in batch.commands() {
        match command.command_type().as_str() {
            "move-start" => {
                outcome.moving = true;
                outcome.move_starts += 1;
            }
            "move-stop" => {
                outcome.moving = false;
                outcome.move_stops += 1;
            }
            "fire-start" => {
                outcome.firing = true;
                outcome.fire_starts += 1;
            }
            "fire-stop" => {
                outcome.firing = false;
                outcome.fire_stops += 1;
            }
            other => panic!("unexpected command {other}"),
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
                .pressed(&ActionId::new(action).unwrap(), &ActionContext::gameplay()),
            expected
        );
    }
    let outcome = world.resource::<Outcome>();
    assert_eq!((outcome.moving, outcome.firing), (expected, expected));
}

fn inject(world: &mut World, pressed: bool) {
    let mut timeline = world.resource_mut::<InputTimeline>();
    assert_eq!(
        timeline
            .admit_keyboard(
                WINDOW,
                SEAT,
                if pressed {
                    ButtonDriverInput::Press(KEY)
                } else {
                    ButtonDriverInput::Release(KEY)
                }
            )
            .unwrap()
            .admitted(),
        1
    );
    assert_eq!(
        timeline
            .admit_mouse_button(
                WINDOW,
                SEAT,
                POINTER,
                if pressed {
                    ButtonDriverInput::Press(MOUSE)
                } else {
                    ButtonDriverInput::Release(MOUSE)
                }
            )
            .unwrap()
            .admitted(),
        1
    );
}

fn travel_input(world: &World) {
    held(world, true);
    let outcome = world.resource::<Outcome>();
    let map = world.resource::<ActionMap>();
    assert!(
        map.bindings_handle()
            .shares_owner_with(outcome.bindings.as_ref().unwrap())
    );
    assert!(map.is_context_enabled(&menu()));
    assert_eq!((outcome.raw, outcome.semantic), (6, 6));
    assert_eq!(
        (
            outcome.move_starts,
            outcome.move_stops,
            outcome.fire_starts,
            outcome.fire_stops
        ),
        (2, 1, 2, 1)
    );
    assert!(world.resource::<ButtonInput<KeyCode>>().pressed(KEY));
    assert!(world.resource::<ButtonInput<MouseButton>>().pressed(MOUSE));
}

fn advance(world: &mut World) {
    source_free(world);
    let raw = world.resource::<ActionOutcomes>().as_slice().to_vec();
    let semantic = world.resource::<SemanticActions>().as_slice().to_vec();
    let (raw_start, semantic_start) = {
        let outcome = world.resource::<Outcome>();
        (outcome.raw, outcome.semantic)
    };
    for (offset, edge) in raw.iter().enumerate() {
        let index = raw_start + offset;
        let keyboard = index % 2 == 0;
        assert_eq!(
            edge.binding,
            if keyboard {
                InputBinding::Key(KEY)
            } else {
                InputBinding::Mouse(MOUSE)
            }
        );
        assert_eq!(edge.provenance.sequence(), index as u64 + 1);
        assert_eq!(edge.provenance.origin(), InputEventOrigin::Platform);
        assert_eq!(edge.provenance.route().window(), WINDOW);
        assert_eq!(edge.provenance.route().seat(), SEAT);
        assert_eq!(
            edge.provenance.route().pointer(),
            if keyboard { None } else { Some(POINTER) }
        );
        assert_eq!(
            edge.phase,
            if (2..4).contains(&index) {
                ActionPhase::Released
            } else {
                ActionPhase::Started
            }
        );
    }
    for (offset, edge) in semantic.iter().enumerate() {
        let index = semantic_start + offset;
        assert_eq!(edge.context, ActionContext::gameplay());
        assert_eq!(
            edge.cause,
            if (2..4).contains(&index) {
                SemanticActionCause::ContextChanged
            } else {
                SemanticActionCause::PhysicalInput
            }
        );
        assert_eq!(edge.provenance.is_none(), (2..4).contains(&index));
    }
    let phase = {
        let mut outcome = world.resource_mut::<Outcome>();
        outcome.raw += raw.len();
        outcome.semantic += semantic.len();
        assert!(outcome.started.unwrap().elapsed() < Duration::from_secs(30));
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
                    .unwrap()
            );
            world.resource_mut::<Outcome>().phase = Phase::Cancelled;
        }
        Phase::Cancelled => {
            held(world, false);
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
            assert_eq!(world.resource::<Outcome>().semantic, 4);
            inject(world, false);
            world.resource_mut::<Outcome>().phase = Phase::Released;
        }
        Phase::Released => {
            held(world, false);
            inject(world, true);
            world.resource_mut::<Outcome>().phase = Phase::Fresh;
        }
        Phase::Fresh => {
            travel_input(world);
            world.resource_mut::<Outcome>().phase = Phase::Request;
        }
        Phase::Request | Phase::Preparing | Phase::Ready => travel(world, phase),
        Phase::Done => {}
    }
}

fn travel(world: &mut World, phase: Phase) {
    travel_input(world);
    let commits = world.resource::<Outcome>().commits;
    let active = if commits == 1 { 2 } else { 1 };
    let target = [2, 1, 3][usize::from(commits)];
    let receipt = live(world, active);
    assert_eq!(world.resource::<Outcome>().receipts.last(), Some(&receipt));
    match phase {
        Phase::Request => {
            let request = ProjectScenes::scope(world, |scenes| {
                let request = scenes.request(super::product(target)).unwrap();
                assert_eq!(
                    scenes.state(request),
                    Some(ProjectSceneRequestState::Preparing)
                );
                assert!(scenes.prepared(request).is_none());
                request
            });
            let mut outcome = world.resource_mut::<Outcome>();
            outcome.request = Some(request);
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
                assert_eq!((commits, target), (2, 3));
                assert_eq!(error, ProjectSceneRequestError::RuntimeContent);
                ProjectScenes::scope(world, |scenes| {
                    assert!(scenes.prepared(request).is_none());
                    assert!(scenes.diagnostics(request).unwrap().has_errors());
                    assert!(scenes.acknowledge(request));
                });
                assert_eq!(live(world, active), receipt);
                travel_input(world);
                let mut outcome = world.resource_mut::<Outcome>();
                outcome.rejections += 1;
                outcome.request = None;
                outcome.phase = Phase::Done;
                return;
            }
            assert_eq!(state, ProjectSceneRequestState::Ready);
            ProjectScenes::scope(world, |scenes| {
                let prepared = scenes.prepared(request).unwrap();
                assert_eq!(
                    prepared.selected_scene_product(),
                    Some(super::product(target))
                );
                assert!(prepared.selected_scene_path().is_none());
                assert_eq!(prepared.expected_instance(), &receipt);
                assert_eq!(prepared.scene(), &super::document(target));
            });
            let mut outcome = world.resource_mut::<Outcome>();
            outcome.ready += 1;
            outcome.phase = Phase::Ready;
        }
        Phase::Ready => {
            let request = world.resource::<Outcome>().request.unwrap();
            let EntityLookup::Resolved(old) = receipt.resolve(world, &super::marker(active)) else {
                panic!("previous live entity")
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
            assert_eq!(live(world, target), published);
            assert_ne!(published.instance_id(), receipt.instance_id());
            assert!(world.get::<SceneEntitySource>(old).is_none());
            travel_input(world);
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
            outcome.phase = if outcome.commits == 3 {
                Phase::Done
            } else {
                Phase::Request
            };
        }
        _ => unreachable!(),
    }
}
