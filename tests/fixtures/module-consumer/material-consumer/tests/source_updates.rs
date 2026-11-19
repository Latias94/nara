use std::{
    collections::BTreeMap,
    fs,
    num::NonZeroU32,
    path::PathBuf,
    time::{Duration, Instant},
};

use nara::{
    ProductConfiguration,
    advanced_prelude::CurrentScene,
    app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    asset::{
        AssetPath, AssetProductRef, AssetRef, AssetSourceChange, AssetSourceChangeKind,
        AssetSourcePath, AssetStates, Assets, LoadState,
    },
    core::ItemLimit,
    ecs::{Component, Mut, Resource, World},
    identity::EntityLookup,
    image::ImageAsset,
    material::Material2dSource,
    project_host::{
        HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, ProjectSceneRequestError,
        ProjectSceneRequestId, ProjectSceneRequestState, ProjectSceneSourceUpdateStatus,
        ProjectScenes, advanced::commit_prepared_project_scene,
    },
    scene::advanced::SceneProductTransactionLimits,
};

#[path = "source_updates/observations.rs"]
mod observations;
#[allow(dead_code)]
#[path = "persistent_workflow/project.rs"]
mod project;
#[path = "source_updates/stable_rename.rs"]
mod stable_rename;

use observations::{Candidate, Resident, prepared, resident};
use project::{
    MATERIAL_PATH, Project, SHADER, SHADER_PATH, STARTUP, actor, indices_product, palette_product,
    recipe,
};

const NEXT_SHADER: &str = "b.wgsl";
const INVALID_SHADER: &str = "fn material_fragment( {";
const PALETTE_PIXEL: [u8; 4] = [17, 61, 139, 255];
const INDICES_PIXEL: [u8; 4] = [211, 83, 29, 255];
const PALETTE_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x10, 0xb4, 0xed, 0xfe,
    0x0f, 0x00, 0x03, 0x15, 0x01, 0xd9, 0x03, 0x4b, 0x98, 0x6b, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];
const INDICES_PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0xb8, 0x1c, 0x2c, 0xfb,
    0x1f, 0x00, 0x05, 0x83, 0x02, 0x43, 0xe6, 0x58, 0x2a, 0x28, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
];

#[derive(Clone, Debug, PartialEq, Eq, Component)]
struct GameProgress {
    health: u32,
    score: u32,
    inventory: Vec<String>,
}

#[derive(Clone, Resource)]
struct GameFiles(PathBuf);

impl ProductConfiguration for GameFiles {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        output.extend_from_slice(b"consumer.palette.source-updates.v1\0");
        let root = self.0.to_string_lossy();
        output.extend_from_slice(&(root.len() as u64).to_le_bytes());
        output.extend_from_slice(root.as_bytes());
    }
}

struct PaletteGame(GameFiles);

impl Plugin for PaletteGame {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.palette.source-updates"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?.register_component::<GameProgress>();
        app.insert_resource(self.0.clone())?
            .init_resource::<Workflow>()?
            .add_systems(CoreStage::Update, update_game)?;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Step {
    #[default]
    Startup,
    Compatible,
    Frozen(u8),
    Committed(u8),
    Successor,
    Invalid,
    Rejected(u8),
    Repaired,
    Missing,
    MissingCreated,
    RemovedShader(u8),
    PalettePixels,
    TextureSelection,
    RemovedImage(u8),
    IndicesPixels,
    CancelReady,
    Cancelled(u8),
    CancelSuccessor,
    Settled(u8),
    Done,
}

#[derive(Clone, Debug, Resource)]
struct Workflow {
    step: Step,
    deadline: Instant,
    frames: u32,
    current: Option<Resident>,
    candidate: Option<Candidate>,
    request: Option<ProjectSceneRequestId>,
    requests: Vec<ProjectSceneRequestId>,
    commits: usize,
    rejections: usize,
    cancellations: usize,
}

impl Default for Workflow {
    fn default() -> Self {
        Self {
            step: Step::Startup,
            deadline: Instant::now() + Duration::from_secs(20),
            frames: 0,
            current: None,
            candidate: None,
            request: None,
            requests: Vec::new(),
            commits: 0,
            rejections: 0,
            cancellations: 0,
        }
    }
}

fn shader(gain: &str) -> String {
    let source = SHADER.replace(
        "palette.rgb, material_parameters.y",
        &format!("palette.rgb * {gain}, material_parameters.y"),
    );
    assert_ne!(source, SHADER);
    source
}

fn asset_path(world: &World, path: &str) -> PathBuf {
    world.resource::<GameFiles>().0.join("assets").join(path)
}

fn write_asset(world: &World, path: &str, bytes: impl AsRef<[u8]>) {
    fs::write(asset_path(world, path), bytes).unwrap();
}

fn select_material(world: &World, path: &str, shader_path: &str, texture: AssetProductRef) {
    let current = Material2dSource::decode(&fs::read(asset_path(world, path)).unwrap()).unwrap();
    let next = Material2dSource::new(
        current.declaration().clone(),
        vec![AssetPath::new(shader_path).unwrap()],
        current.default_parameters().clone(),
        BTreeMap::from([("palette".to_owned(), AssetRef::Product(texture))]),
    )
    .unwrap();
    write_asset(world, path, next.encode().unwrap());
}

fn status(world: &mut World) -> ProjectSceneSourceUpdateStatus {
    ProjectScenes::scope(world, |scenes| scenes.source_update_status().unwrap())
}

fn notify(world: &mut World, path: &str, kind: AssetSourceChangeKind, expected: bool) {
    let before = resident(world);
    let request = status(world).request;
    let change = AssetSourceChange::new(
        AssetSourcePath::new(format!("assets/{path}")).unwrap(),
        kind,
    );
    ProjectScenes::scope(world, |scenes| {
        assert_eq!(scenes.notify_source_change(&change), Ok(expected));
    });
    assert_eq!(status(world).request, request);
    assert_eq!(resident(world), before);
}

fn changed(world: &mut World, path: &str) {
    notify(world, path, AssetSourceChangeKind::Modified, true);
    assert!(status(world).pending);
}

fn idle(world: &mut World) {
    let status = status(world);
    assert_eq!(status.request, None);
    assert!(!status.pending);
}

fn next_result(world: &mut World, game: &mut Workflow) -> Option<ProjectSceneRequestState> {
    let update = status(world);
    let Some(request) = update.request else {
        assert!(update.pending);
        assert!(game.request.is_none());
        std::thread::sleep(Duration::from_millis(1));
        return None;
    };
    if let Some(expected) = game.request {
        assert_eq!(request, expected);
    } else {
        assert!(!game.requests.contains(&request));
        game.requests.push(request);
        game.request = Some(request);
    }
    let state = ProjectScenes::scope(world, |scenes| scenes.state(request)).unwrap();
    if state == ProjectSceneRequestState::Preparing {
        assert_eq!(update.error, None);
        ProjectScenes::scope(world, |scenes| {
            assert!(scenes.prepared(request).is_none());
            assert!(!scenes.acknowledge(request));
        });
        std::thread::sleep(Duration::from_millis(1));
        None
    } else {
        Some(state)
    }
}

fn ready(world: &mut World, game: &mut Workflow, path: &str, source: &str) -> bool {
    let Some(state) = next_result(world, game) else {
        return false;
    };
    assert_eq!(state, ProjectSceneRequestState::Ready);
    assert_eq!(status(world).error, None);
    let request = game.request.unwrap();
    let candidate = prepared(world, request);
    candidate.material.assert_shader(path, source);
    assert_eq!(
        candidate.material.interface,
        game.current.as_ref().unwrap().material.interface,
    );
    assert_eq!(
        candidate.material.declaration,
        game.current.as_ref().unwrap().material.declaration,
    );
    ProjectScenes::scope(world, |scenes| assert!(!scenes.acknowledge(request)));
    game.candidate = Some(candidate);
    true
}

fn reject(world: &mut World, game: &mut Workflow) -> bool {
    let Some(state) = next_result(world, game) else {
        return false;
    };
    let ProjectSceneRequestState::Rejected(error @ ProjectSceneRequestError::Content(_)) = state
    else {
        panic!("the edited source must reject before publication: {state:?}");
    };
    assert_eq!(status(world).error, Some(error));
    ProjectScenes::scope(world, |scenes| {
        let request = game.request.unwrap();
        assert!(scenes.prepared(request).is_none());
        assert!(scenes.diagnostics(request).unwrap().has_errors());
    });
    game.rejections += 1;
    true
}

fn acknowledge(world: &mut World, game: &mut Workflow) {
    let request = game.request.take().unwrap();
    ProjectScenes::scope(world, |scenes| {
        for previous in game.requests.iter().copied().filter(|id| *id != request) {
            assert!(!scenes.acknowledge(previous));
            assert!(scenes.state(previous).is_none());
        }
        assert_eq!(
            scenes.source_update_status().unwrap().request,
            Some(request)
        );
        assert!(scenes.acknowledge(request));
        assert!(!scenes.acknowledge(request));
        assert!(scenes.state(request).is_none());
        assert!(scenes.prepared(request).is_none());
        assert_eq!(scenes.source_update_status().unwrap().request, None);
    });
    game.candidate = None;
}

fn commit(world: &mut World, game: &mut Workflow) {
    let before = game.current.as_ref().unwrap().clone();
    let request = game.request.unwrap();
    let candidate = game.candidate.as_ref().unwrap().clone();
    assert_eq!(prepared(world, request), candidate);
    let gameplay = world.get::<GameProgress>(before.entity).unwrap().clone();
    let report = commit_prepared_project_scene(
        world,
        request,
        SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
        &[],
        |overlay| {
            overlay.insert_component(actor(), gameplay);
        },
    );
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let after = resident(world);
    assert_eq!(report.instance.as_ref(), Some(&after.instance));
    assert_ne!(after.instance, before.instance);
    assert_ne!(after.entity, before.entity);
    assert!(world.get::<GameProgress>(before.entity).is_none());
    assert!(world.get::<nara::sprite::Sprite>(before.entity).is_none());
    assert_eq!(after.gameplay, before.gameplay);
    assert_eq!(after.stamp, candidate.stamp);
    assert_ne!(after.stamp, before.stamp);
    assert_eq!(after.material, candidate.material);
    assert_eq!(after.material_handle, before.material_handle);
    assert_eq!(
        after.material_state.version().raw(),
        before.material_state.version().raw() + 1,
    );
    assert_ne!(after.material_revision, before.material_revision);
    for (product, old) in &before.images {
        if let Some(new) = after.images.get(product) {
            assert_eq!(new.handle, old.handle);
            assert_eq!(new.state.version().raw(), old.state.version().raw() + 1);
            assert_ne!(new.revision, old.revision);
        } else {
            assert!(
                world
                    .resource::<Assets<ImageAsset>>()
                    .get(old.handle)
                    .is_none(),
            );
            let removed = world
                .resource::<AssetStates>()
                .state(old.handle.id())
                .unwrap();
            assert_eq!(removed.load_state(), &LoadState::Removed);
            assert_eq!(removed.usable_version(), None);
            assert_eq!(removed.version().raw(), old.state.version().raw() + 1);
        }
    }
    ProjectScenes::scope(world, |scenes| {
        assert_eq!(
            scenes.state(request),
            Some(ProjectSceneRequestState::Committed),
        );
        assert!(scenes.prepared(request).is_none());
        assert!(!scenes.diagnostics(request).unwrap().has_errors());
    });
    game.current = Some(after);
    game.commits += 1;
}

fn update_game(world: &mut World) {
    world.resource_scope(|world, mut game: Mut<Workflow>| {
        game.frames += 1;
        assert!(
            Instant::now() < game.deadline,
            "source workflow timed out at {:?}",
            game.step,
        );
        if let Some(current) = &game.current {
            assert_eq!(
                &resident(world),
                current,
                "last-good changed at {:?}",
                game.step,
            );
        }
        match game.step {
            Step::Startup => {
                idle(world);
                assert_eq!(status(world).error, None);
                let lookup = CurrentScene::get(world)
                    .unwrap()
                    .receipt()
                    .resolve(world, &actor());
                let entity = match lookup {
                    EntityLookup::Resolved(entity) => entity,
                    other => panic!("the startup actor must be resident: {other:?}"),
                };
                world.entity_mut(entity).insert(GameProgress {
                    health: 7,
                    score: 123,
                    inventory: vec!["brass-key".to_owned()],
                });
                let initial = resident(world);
                initial.material.assert_shader(SHADER_PATH, SHADER);
                assert_eq!(initial.material.texture, palette_product());
                assert_eq!(initial.images.len(), 2);
                game.current = Some(initial);
                notify(
                    world,
                    "unrelated.wgsl",
                    AssetSourceChangeKind::Modified,
                    false,
                );
                idle(world);
                write_asset(world, SHADER_PATH, shader("0.75"));
                changed(world, SHADER_PATH);
                assert_eq!(status(world).request, None);
                game.step = Step::Compatible;
            }
            Step::Compatible => {
                if !ready(world, &mut game, SHADER_PATH, &shader("0.75")) {
                    return;
                }
                assert!(!status(world).pending);
                assert_ne!(
                    game.candidate.as_ref().unwrap().material.shader,
                    game.current.as_ref().unwrap().material.shader,
                );
                write_asset(world, SHADER_PATH, shader("0.5"));
                changed(world, SHADER_PATH);
                changed(world, SHADER_PATH);
                write_asset(
                    world,
                    "late-discovered.txt",
                    b"a notice before the next dependency cut",
                );
                changed(world, "late-discovered.txt");
                game.step = Step::Frozen(0);
            }
            Step::Frozen(frame) => {
                let request = game.request.unwrap();
                assert_eq!(status(world).request, Some(request));
                assert!(status(world).pending);
                assert_eq!(prepared(world, request), *game.candidate.as_ref().unwrap());
                assert_eq!(game.requests.len(), 1);
                if frame < 2 {
                    game.step = Step::Frozen(frame + 1);
                    return;
                }
                let entity = game.current.as_ref().unwrap().entity;
                {
                    let mut progress = world.get_mut::<GameProgress>(entity).unwrap();
                    progress.score += 5;
                    progress.inventory.push("map".to_owned());
                }
                game.current = Some(resident(world));
                let images = game.current.as_ref().unwrap().images.clone();
                commit(world, &mut game);
                let current = game.current.as_ref().unwrap();
                current.material.assert_shader(SHADER_PATH, &shader("0.75"));
                for (product, image) in images {
                    assert_eq!(current.images[&product].hash, image.hash);
                    assert_eq!(current.images[&product].pixels, image.pixels);
                }
                assert!(status(world).pending);
                game.step = Step::Committed(0);
            }
            Step::Committed(frame) => {
                let request = game.request.unwrap();
                assert_eq!(status(world).request, Some(request));
                assert!(status(world).pending);
                ProjectScenes::scope(world, |scenes| {
                    assert_eq!(
                        scenes.state(request),
                        Some(ProjectSceneRequestState::Committed),
                    );
                });
                assert_eq!(game.requests.len(), 1);
                if frame < 2 {
                    game.step = Step::Committed(frame + 1);
                    return;
                }
                acknowledge(world, &mut game);
                assert!(status(world).pending);
                game.step = Step::Successor;
            }
            Step::Successor => {
                if !ready(world, &mut game, SHADER_PATH, &shader("0.5")) {
                    return;
                }
                assert_eq!(game.requests.len(), 2);
                assert!(!status(world).pending);
                commit(world, &mut game);
                acknowledge(world, &mut game);
                idle(world);
                write_asset(world, SHADER_PATH, INVALID_SHADER);
                changed(world, SHADER_PATH);
                game.step = Step::Invalid;
            }
            Step::Invalid => {
                if !reject(world, &mut game) {
                    return;
                }
                assert!(!status(world).pending);
                write_asset(world, SHADER_PATH, shader("0.25"));
                changed(world, SHADER_PATH);
                game.step = Step::Rejected(0);
            }
            Step::Rejected(frame) => {
                let request = game.request.unwrap();
                assert_eq!(status(world).request, Some(request));
                assert!(status(world).pending);
                ProjectScenes::scope(world, |scenes| {
                    assert!(matches!(
                        scenes.state(request),
                        Some(ProjectSceneRequestState::Rejected(_)),
                    ));
                    assert!(scenes.prepared(request).is_none());
                    assert!(scenes.diagnostics(request).unwrap().has_errors());
                });
                if frame < 2 {
                    game.step = Step::Rejected(frame + 1);
                    return;
                }
                acknowledge(world, &mut game);
                assert!(status(world).pending);
                game.step = Step::Repaired;
            }
            Step::Repaired => {
                if !ready(world, &mut game, SHADER_PATH, &shader("0.25")) {
                    return;
                }
                commit(world, &mut game);
                acknowledge(world, &mut game);
                idle(world);
                assert!(!asset_path(world, NEXT_SHADER).exists());
                select_material(world, MATERIAL_PATH, NEXT_SHADER, palette_product());
                changed(world, MATERIAL_PATH);
                game.step = Step::Missing;
            }
            Step::Missing => {
                if !reject(world, &mut game) {
                    return;
                }
                assert!(!asset_path(world, NEXT_SHADER).exists());
                acknowledge(world, &mut game);
                idle(world);
                write_asset(world, NEXT_SHADER, shader("0.125"));
                changed(world, NEXT_SHADER);
                game.step = Step::MissingCreated;
            }
            Step::MissingCreated => {
                if !ready(world, &mut game, NEXT_SHADER, &shader("0.125")) {
                    return;
                }
                commit(world, &mut game);
                acknowledge(world, &mut game);
                idle(world);
                fs::remove_file(asset_path(world, SHADER_PATH)).unwrap();
                notify(world, SHADER_PATH, AssetSourceChangeKind::Removed, false);
                game.step = Step::RemovedShader(0);
            }
            Step::RemovedShader(frame) => {
                idle(world);
                if frame < 2 {
                    game.step = Step::RemovedShader(frame + 1);
                    return;
                }
                write_asset(world, "palette.png", PALETTE_PNG);
                changed(world, "palette.png");
                game.step = Step::PalettePixels;
            }
            Step::PalettePixels => {
                if !ready(world, &mut game, NEXT_SHADER, &shader("0.125")) {
                    return;
                }
                let before = game.current.as_ref().unwrap().clone();
                commit(world, &mut game);
                let current = game.current.as_ref().unwrap();
                assert_eq!(current.material, before.material);
                assert_eq!(current.images[&palette_product()].pixels, PALETTE_PIXEL);
                assert_ne!(
                    current.images[&palette_product()].hash,
                    before.images[&palette_product()].hash,
                );
                assert_ne!(
                    current.images[&palette_product()].state.source_hash(),
                    before.images[&palette_product()].state.source_hash(),
                );
                assert_eq!(
                    current.images[&indices_product()].hash,
                    before.images[&indices_product()].hash,
                );
                acknowledge(world, &mut game);
                idle(world);
                select_material(world, MATERIAL_PATH, NEXT_SHADER, indices_product());
                changed(world, MATERIAL_PATH);
                game.step = Step::TextureSelection;
            }
            Step::TextureSelection => {
                if !ready(world, &mut game, NEXT_SHADER, &shader("0.125")) {
                    return;
                }
                assert_eq!(
                    game.candidate.as_ref().unwrap().material.texture,
                    indices_product(),
                );
                let before = game.current.as_ref().unwrap().clone();
                commit(world, &mut game);
                let current = game.current.as_ref().unwrap();
                assert_eq!(current.material.shader, before.material.shader);
                assert_eq!(current.images.len(), 1);
                assert_eq!(
                    current.bound_texture,
                    current.images[&indices_product()].handle,
                );
                assert_ne!(current.bound_texture, before.bound_texture);
                assert_eq!(
                    current.images[&indices_product()].hash,
                    before.images[&indices_product()].hash,
                );
                acknowledge(world, &mut game);
                idle(world);
                fs::remove_file(asset_path(world, "palette.png")).unwrap();
                notify(world, "palette.png", AssetSourceChangeKind::Removed, false);
                notify(
                    world,
                    "palette.png",
                    AssetSourceChangeKind::MetaModified,
                    false,
                );
                game.step = Step::RemovedImage(0);
            }
            Step::RemovedImage(frame) => {
                idle(world);
                if frame < 2 {
                    game.step = Step::RemovedImage(frame + 1);
                    return;
                }
                write_asset(world, "indices.png", INDICES_PNG);
                changed(world, "indices.png");
                game.step = Step::IndicesPixels;
            }
            Step::IndicesPixels => {
                if !ready(world, &mut game, NEXT_SHADER, &shader("0.125")) {
                    return;
                }
                let before = game.current.as_ref().unwrap().clone();
                commit(world, &mut game);
                let current = game.current.as_ref().unwrap();
                assert_eq!(current.material, before.material);
                assert_eq!(current.images[&indices_product()].pixels, INDICES_PIXEL);
                assert_ne!(
                    current.images[&indices_product()].hash,
                    before.images[&indices_product()].hash,
                );
                assert_eq!(
                    current.bound_texture,
                    current.images[&indices_product()].handle,
                );
                acknowledge(world, &mut game);
                idle(world);
                ProjectScenes::scope(world, |scenes| scenes.request_source_rescan()).unwrap();
                assert_eq!(status(world).request, None);
                assert!(status(world).pending);
                game.step = Step::CancelReady;
            }
            Step::CancelReady => {
                if !ready(world, &mut game, NEXT_SHADER, &shader("0.125")) {
                    return;
                }
                let request = game.request.unwrap();
                write_asset(world, NEXT_SHADER, shader("0.625"));
                changed(world, NEXT_SHADER);
                assert_eq!(prepared(world, request), *game.candidate.as_ref().unwrap());
                ProjectScenes::scope(world, |scenes| {
                    assert!(scenes.cancel(request));
                    assert_eq!(
                        scenes.state(request),
                        Some(ProjectSceneRequestState::Cancelled),
                    );
                    assert!(scenes.prepared(request).is_none());
                });
                game.cancellations += 1;
                game.step = Step::Cancelled(0);
            }
            Step::Cancelled(frame) => {
                let request = game.request.unwrap();
                assert_eq!(status(world).request, Some(request));
                assert!(status(world).pending);
                ProjectScenes::scope(world, |scenes| {
                    assert_eq!(
                        scenes.state(request),
                        Some(ProjectSceneRequestState::Cancelled),
                    );
                    assert!(scenes.prepared(request).is_none());
                });
                if frame < 2 {
                    game.step = Step::Cancelled(frame + 1);
                    return;
                }
                acknowledge(world, &mut game);
                assert!(status(world).pending);
                game.step = Step::CancelSuccessor;
            }
            Step::CancelSuccessor => {
                if !ready(world, &mut game, NEXT_SHADER, &shader("0.625")) {
                    return;
                }
                commit(world, &mut game);
                acknowledge(world, &mut game);
                idle(world);
                game.step = Step::Settled(0);
            }
            Step::Settled(frame) => {
                idle(world);
                assert_eq!(status(world).error, None);
                assert_eq!(game.requests.len(), 11);
                assert_eq!(game.commits, 8);
                assert_eq!(game.rejections, 2);
                assert_eq!(game.cancellations, 1);
                if frame < 4 {
                    game.step = Step::Settled(frame + 1);
                } else {
                    game.step = Step::Done;
                }
            }
            Step::Done => unreachable!("the finite HeadlessRun must stop after completion"),
        }
    });
}

fn run_source_workflow<O: Resource + Clone>(project: &Project, intent: HeadlessRunIntent<O>) -> O {
    let mut run = HeadlessRun::<O>::new(project.capability(), intent, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(game) => return game.clone(),
            HeadlessRunOutcome::Failed => {
                panic!(
                    "semantic source HeadlessRun failed: {:?}",
                    report.diagnostics()
                );
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                assert!(
                    Instant::now() < deadline,
                    "source workflow cleanup did not settle",
                );
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

#[test]
fn game_owns_semantic_source_updates_and_preserves_last_good_residency_and_progress() {
    let project = Project::new();
    assert!(!project.root().join(format!("{STARTUP}.meta")).exists());
    let scene_bytes = fs::read(project.root().join(STARTUP)).unwrap();
    let files = GameFiles(project.root().to_path_buf());
    let product = recipe()
        .add_configured_plugin::<PaletteGame, _, _>(files, |files| PaletteGame(files.clone()))
        .unwrap();
    let intent = HeadlessRunIntent::frames(NonZeroU32::new(6000).unwrap(), Duration::ZERO)
        .with_recipe(product)
        .with_cleanup_timeout(Duration::from_secs(5))
        .stop_when(|game: &Workflow| game.step == Step::Done);
    let outcome = run_source_workflow(&project, intent);
    assert_eq!(outcome.step, Step::Done);
    assert_eq!(outcome.requests.len(), 11);
    assert_eq!(outcome.commits, 8);
    assert_eq!(outcome.rejections, 2);
    assert_eq!(outcome.cancellations, 1);
    assert!(outcome.frames > 20 && outcome.frames < 6000);
    assert!(outcome.request.is_none());
    assert!(outcome.candidate.is_none());
    let final_state = outcome.current.unwrap();
    final_state
        .material
        .assert_shader(NEXT_SHADER, &shader("0.625"));
    assert_eq!(final_state.material.texture, indices_product());
    assert_eq!(final_state.images.len(), 1);
    assert_eq!(final_state.images[&indices_product()].pixels, INDICES_PIXEL);
    assert_eq!(
        final_state.gameplay,
        GameProgress {
            health: 7,
            score: 128,
            inventory: vec!["brass-key".to_owned(), "map".to_owned()],
        },
    );
    assert_eq!(fs::read(project.root().join(STARTUP)).unwrap(), scene_bytes);
}

#[test]
fn semantic_source_update_ingress_requires_a_hosted_source_scene() {
    let mut world = World::new();
    let change = AssetSourceChange::new(
        AssetSourcePath::new(format!("assets/{SHADER_PATH}")).unwrap(),
        AssetSourceChangeKind::Modified,
    );
    ProjectScenes::scope(&mut world, |scenes| {
        assert!(scenes.source_update_status().is_none());
        assert_eq!(
            scenes.notify_source_change(&change),
            Err(ProjectSceneRequestError::Unavailable),
        );
        assert_eq!(
            scenes.request_source_rescan(),
            Err(ProjectSceneRequestError::Unavailable),
        );
        assert!(scenes.source_update_status().is_none());
    });
    assert!(CurrentScene::get(&world).is_none());
    assert!(!world.contains_resource::<Assets<ImageAsset>>());
    assert!(!world.contains_resource::<AssetStates>());
}
