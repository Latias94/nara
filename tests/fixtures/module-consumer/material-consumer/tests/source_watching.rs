#![cfg(any(windows, target_os = "linux"))]

use std::{
    collections::BTreeMap,
    fs,
    num::NonZeroU32,
    path::PathBuf,
    time::{Duration, Instant},
};

use nara as nara_engine;
use nara_engine::{
    ProductConfiguration,
    advanced_prelude::CurrentScene,
    app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    asset::{
        AssetMeta, AssetMetaIdentity, AssetPath, AssetProductRef, AssetRef, AssetSourceKind,
        ImportedProductId, StableAssetId,
    },
    asset_watch::{AssetWatchQueueLimits, AssetWatchRuntimeObserver},
    ecs::{Component, Mut, Resource, World},
    identity::EntityLookup,
    image::ImageImportSettings,
    material::Material2dSource,
    project_host::{
        HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, ProjectSceneRequestId,
        ProjectSceneRequestState, ProjectSceneWatchPlugin, ProjectSceneWatchState, ProjectScenes,
    },
};

#[cfg(feature = "native-source-watch-gpu")]
#[path = "source_watching/captured.rs"]
mod captured;
#[cfg(feature = "native-source-watch-desktop")]
#[allow(dead_code)]
#[path = "source_watching/desktop.rs"]
pub(crate) mod desktop;
#[cfg(feature = "native-source-watch-gpu")]
#[path = "source_watching/gpu.rs"]
mod gpu;
#[path = "source_updates/observations.rs"]
mod observations;
#[allow(dead_code)]
#[path = "persistent_workflow/project.rs"]
mod project;
#[path = "source_watching/requests.rs"]
mod requests;
#[path = "source_watching/retirement.rs"]
mod retirement;

use observations::{Candidate, Resident, prepared, resident};
use project::{
    MATERIAL_PATH, Project, SHADER, SHADER_PATH, STARTUP, actor, indices_product, palette_product,
    recipe,
};
use requests::{acknowledge, commit, ready, rejected, update_status, watch_status};
use retirement::CloseProof;

const NEXT_SHADER: &str = "next.wgsl";
const INVALID_SHADER: &str = "fn material_fragment( {";
const NEW_IMAGE: &str = "discovered.png";
const PIXEL: [u8; 4] = [17, 61, 139, 255];
const PNG: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f, 0x15, 0xc4,
    0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x10, 0xb4, 0xed, 0xfe,
    0x0f, 0x00, 0x03, 0x15, 0x01, 0xd9, 0x03, 0x4b, 0x98, 0x6b, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
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
        output.extend_from_slice(b"consumer.palette.source-watching.v1\0");
        let root = self.0.to_string_lossy();
        output.extend_from_slice(&(root.len() as u64).to_le_bytes());
        output.extend_from_slice(root.as_bytes());
    }
}

struct PaletteGame(GameFiles);

impl Plugin for PaletteGame {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.palette.source-watching"),
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
    InitialRescan,
    Compatible,
    Frozen,
    Committed,
    Successor,
    Invalid,
    RepairPending,
    Repaired,
    Missing,
    MissingCreated,
    PalettePixels,
    TextureSelection,
    UnknownProduct,
    UnknownSettling,
    Catalogue,
    Settling,
    Done,
}

#[derive(Clone, Resource)]
struct Workflow {
    step: Step,
    deadline: Instant,
    hold_until: Option<Instant>,
    frames: u32,
    current: Option<Resident>,
    candidate: Option<Candidate>,
    request: Option<ProjectSceneRequestId>,
    requests: Vec<ProjectSceneRequestId>,
    commits: usize,
    rejections: usize,
    duplicates: usize,
    observed_native_activity: bool,
    observer: Option<AssetWatchRuntimeObserver>,
}

impl Default for Workflow {
    fn default() -> Self {
        Self {
            step: Step::Startup,
            deadline: Instant::now() + Duration::from_secs(15),
            hold_until: None,
            frames: 0,
            current: None,
            candidate: None,
            request: None,
            requests: Vec::new(),
            commits: 0,
            rejections: 0,
            duplicates: 0,
            observed_native_activity: false,
            observer: None,
        }
    }
}

impl Workflow {
    fn advance(&mut self, step: Step) {
        self.step = step;
        self.deadline = Instant::now() + Duration::from_secs(15);
        self.hold_until = None;
    }

    fn held(&mut self, duration: Duration) -> bool {
        Instant::now()
            >= *self
                .hold_until
                .get_or_insert_with(|| Instant::now() + duration)
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

fn select_material(world: &World, path: &str, texture: AssetProductRef) {
    let current =
        Material2dSource::decode(&fs::read(asset_path(world, MATERIAL_PATH)).unwrap()).unwrap();
    let next = Material2dSource::new(
        current.declaration().clone(),
        vec![AssetPath::new(path).unwrap()],
        current.default_parameters().clone(),
        BTreeMap::from([("palette".to_owned(), AssetRef::Product(texture))]),
    )
    .unwrap();
    write_asset(world, MATERIAL_PATH, next.encode().unwrap());
}

fn discovered_product() -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str("744cab2d-4c3c-43f0-bd13-8d72a8c698b4").unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn update_game(world: &mut World) {
    std::thread::sleep(Duration::from_millis(2));
    world.resource_scope(|world, mut game: Mut<Workflow>| {
        game.frames += 1;
        assert!(
            Instant::now() < game.deadline,
            "native source workflow timed out at {:?}: updates={:?}, watch={:?}",
            game.step,
            update_status(world),
            watch_status(world),
        );
        let watch = watch_status(world);
        game.observed_native_activity |= watch.queue.accepted_events() > 0 || watch.restarts > 0;
        assert_eq!(watch.state, ProjectSceneWatchState::Watching, "{watch:?}");
        assert_eq!(watch.error, None);
        assert!(watch.native.active() > 0);
        if let Some(current) = &game.current {
            assert_eq!(
                &resident(world),
                current,
                "last-good changed at {:?}",
                game.step
            );
        }
        match game.step {
            Step::Startup => {
                let entity = match CurrentScene::get(world)
                    .unwrap()
                    .receipt()
                    .resolve(world, &actor())
                {
                    EntityLookup::Resolved(entity) => entity,
                    other => panic!("the original startup actor must be resident: {other:?}"),
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
                game.observer =
                    ProjectScenes::scope(world, |scenes| scenes.source_watch_retirement());
                let observer = game.observer.as_ref().unwrap().clone();
                assert!(observer.stats().started() > 0);
                world.resource::<CloseProof>().observe(observer);
                assert!(update_status(world).pending || update_status(world).request.is_some());
                ProjectScenes::scope(world, |scenes| assert!(!scenes.retry_source_watch()));
                game.advance(Step::InitialRescan);
            }
            Step::InitialRescan => {
                if !ready(
                    world,
                    &mut game,
                    SHADER_PATH,
                    SHADER,
                    palette_product(),
                    false,
                ) {
                    return;
                }
                let candidate = game.candidate.as_ref().unwrap().stamp;
                let current = game.current.as_ref().unwrap().stamp;
                assert_eq!(candidate.schema_fingerprint(), current.schema_fingerprint());
                assert_eq!(candidate.manifest_digest(), current.manifest_digest());
                assert_eq!(candidate.content_digest(), current.content_digest());
                commit(world, &mut game);
                acknowledge(world, &mut game);
                write_asset(world, SHADER_PATH, shader("0.75"));
                game.advance(Step::Compatible);
            }
            Step::Compatible => {
                if !ready(
                    world,
                    &mut game,
                    SHADER_PATH,
                    &shader("0.75"),
                    palette_product(),
                    true,
                ) {
                    return;
                }
                assert_ne!(
                    game.candidate.as_ref().unwrap().material.shader,
                    game.current.as_ref().unwrap().material.shader
                );
                write_asset(world, SHADER_PATH, shader("0.5"));
                game.advance(Step::Frozen);
            }
            Step::Frozen => {
                let request = game.request.unwrap();
                assert_eq!(update_status(world).request, Some(request));
                assert_eq!(prepared(world, request), *game.candidate.as_ref().unwrap());
                ProjectScenes::scope(world, |scenes| {
                    assert_eq!(scenes.state(request), Some(ProjectSceneRequestState::Ready))
                });
                if !update_status(world).pending || !game.held(Duration::from_millis(30)) {
                    return;
                }
                let entity = game.current.as_ref().unwrap().entity;
                {
                    let mut progress = world.get_mut::<GameProgress>(entity).unwrap();
                    progress.score += 5;
                    progress.inventory.push("map".to_owned());
                }
                game.current = Some(resident(world));
                commit(world, &mut game);
                game.current
                    .as_ref()
                    .unwrap()
                    .material
                    .assert_shader(SHADER_PATH, &shader("0.75"));
                assert!(update_status(world).pending);
                game.advance(Step::Committed);
            }
            Step::Committed => {
                let request = game.request.unwrap();
                assert_eq!(update_status(world).request, Some(request));
                assert!(update_status(world).pending);
                ProjectScenes::scope(world, |scenes| {
                    assert_eq!(
                        scenes.state(request),
                        Some(ProjectSceneRequestState::Committed)
                    )
                });
                if !game.held(Duration::from_millis(30)) {
                    return;
                }
                acknowledge(world, &mut game);
                assert!(update_status(world).pending);
                game.advance(Step::Successor);
            }
            Step::Successor => {
                if !ready(
                    world,
                    &mut game,
                    SHADER_PATH,
                    &shader("0.5"),
                    palette_product(),
                    true,
                ) {
                    return;
                }
                commit(world, &mut game);
                acknowledge(world, &mut game);
                write_asset(world, SHADER_PATH, INVALID_SHADER);
                game.advance(Step::Invalid);
            }
            Step::Invalid => {
                if !rejected(world, &mut game) {
                    return;
                }
                write_asset(world, SHADER_PATH, shader("0.25"));
                game.advance(Step::RepairPending);
            }
            Step::RepairPending => {
                let request = game.request.unwrap();
                assert_eq!(update_status(world).request, Some(request));
                ProjectScenes::scope(world, |scenes| {
                    assert!(matches!(
                        scenes.state(request),
                        Some(ProjectSceneRequestState::Rejected(_))
                    ));
                    assert!(scenes.prepared(request).is_none());
                    assert!(scenes.diagnostics(request).unwrap().has_errors());
                });
                if !update_status(world).pending || !game.held(Duration::from_millis(30)) {
                    return;
                }
                acknowledge(world, &mut game);
                game.advance(Step::Repaired);
            }
            Step::Repaired => {
                if !ready(
                    world,
                    &mut game,
                    SHADER_PATH,
                    &shader("0.25"),
                    palette_product(),
                    true,
                ) {
                    return;
                }
                commit(world, &mut game);
                acknowledge(world, &mut game);
                assert!(!asset_path(world, NEXT_SHADER).exists());
                select_material(world, NEXT_SHADER, palette_product());
                game.advance(Step::Missing);
            }
            Step::Missing => {
                if !rejected(world, &mut game) {
                    return;
                }
                assert!(!asset_path(world, NEXT_SHADER).exists());
                acknowledge(world, &mut game);
                write_asset(world, NEXT_SHADER, shader("0.125"));
                game.advance(Step::MissingCreated);
            }
            Step::MissingCreated => {
                if !ready(
                    world,
                    &mut game,
                    NEXT_SHADER,
                    &shader("0.125"),
                    palette_product(),
                    true,
                ) {
                    return;
                }
                commit(world, &mut game);
                acknowledge(world, &mut game);
                write_asset(world, "palette.png", PNG);
                game.advance(Step::PalettePixels);
            }
            Step::PalettePixels => {
                if !ready(
                    world,
                    &mut game,
                    NEXT_SHADER,
                    &shader("0.125"),
                    palette_product(),
                    true,
                ) {
                    return;
                }
                let before = game.current.as_ref().unwrap().clone();
                commit(world, &mut game);
                let current = game.current.as_ref().unwrap();
                assert_eq!(current.material, before.material);
                assert_eq!(current.images[&palette_product()].pixels, PIXEL);
                assert_ne!(
                    current.images[&palette_product()].hash,
                    before.images[&palette_product()].hash
                );
                assert_ne!(
                    current.images[&palette_product()].state.source_hash(),
                    before.images[&palette_product()].state.source_hash()
                );
                assert_eq!(
                    current.images[&indices_product()].hash,
                    before.images[&indices_product()].hash
                );
                acknowledge(world, &mut game);
                select_material(world, NEXT_SHADER, indices_product());
                game.advance(Step::TextureSelection);
            }
            Step::TextureSelection => {
                if !ready(
                    world,
                    &mut game,
                    NEXT_SHADER,
                    &shader("0.125"),
                    indices_product(),
                    true,
                ) {
                    return;
                }
                let before = game.current.as_ref().unwrap().clone();
                commit(world, &mut game);
                let current = game.current.as_ref().unwrap();
                assert_eq!(current.material.shader, before.material.shader);
                assert_eq!(current.images.len(), 1);
                assert_eq!(
                    current.bound_texture,
                    current.images[&indices_product()].handle
                );
                assert_ne!(current.bound_texture, before.bound_texture);
                assert_eq!(
                    current.images[&indices_product()].hash,
                    before.images[&indices_product()].hash
                );
                acknowledge(world, &mut game);
                write_asset(world, NEW_IMAGE, PNG);
                select_material(world, NEXT_SHADER, discovered_product());
                game.advance(Step::UnknownProduct);
            }
            Step::UnknownProduct => {
                if !rejected(world, &mut game) {
                    return;
                }
                assert_eq!(fs::read(asset_path(world, NEW_IMAGE)).unwrap(), PNG);
                assert!(!asset_path(world, &format!("{NEW_IMAGE}.meta")).exists());
                acknowledge(world, &mut game);
                game.advance(Step::UnknownSettling);
            }
            Step::UnknownSettling => {
                if !requests::drain_failed_duplicate(world, &mut game) {
                    game.hold_until = None;
                    return;
                }
                let watch = watch_status(world);
                if update_status(world).pending
                    || watch.rescan_pending
                    || watch.queue.retained_events() != 0
                {
                    game.hold_until = None;
                    return;
                }
                if !game.held(Duration::from_millis(100)) {
                    return;
                }
                let metadata = AssetMeta::new(
                    AssetMetaIdentity::new(
                        discovered_product().source(),
                        AssetPath::new(NEW_IMAGE).unwrap(),
                        AssetSourceKind::Image,
                    ),
                    ImageImportSettings::default(),
                );
                write_asset(
                    world,
                    &format!("{NEW_IMAGE}.meta"),
                    metadata.to_json_string().unwrap(),
                );
                game.advance(Step::Catalogue);
            }
            Step::Catalogue => {
                if !ready(
                    world,
                    &mut game,
                    NEXT_SHADER,
                    &shader("0.125"),
                    discovered_product(),
                    true,
                ) {
                    return;
                }
                let before = game.current.as_ref().unwrap().clone();
                commit(world, &mut game);
                let current = game.current.as_ref().unwrap();
                assert_eq!(current.images.len(), 2);
                assert_eq!(current.images[&discovered_product()].pixels, PIXEL);
                assert_eq!(
                    current.bound_texture,
                    current.images[&discovered_product()].handle
                );
                assert_ne!(current.bound_texture, before.bound_texture);
                assert_eq!(
                    current.images[&indices_product()].hash,
                    before.images[&indices_product()].hash
                );
                acknowledge(world, &mut game);
                game.advance(Step::Settling);
            }
            Step::Settling => {
                if !requests::drain_duplicate(world, &mut game) {
                    game.hold_until = None;
                    return;
                }
                let watch = watch_status(world);
                let update = update_status(world);
                if update.pending || watch.rescan_pending || watch.queue.retained_events() != 0 {
                    game.hold_until = None;
                    return;
                }
                if game.held(Duration::from_millis(100)) {
                    assert_eq!(game.commits, 8);
                    assert_eq!(game.rejections, 3);
                    assert_eq!(update.error, None);
                    assert!(game.observed_native_activity);
                    #[cfg(windows)]
                    assert!(watch.restarts > 0);
                    game.advance(Step::Done);
                }
            }
            Step::Done => unreachable!("the finite Host must Stop after the completed workflow"),
        }
    });
}

#[test]
fn native_source_events_drive_explicit_scene_commits_and_join_original_watchers_on_stop() {
    let project = Project::new();
    let manifest_path = project.root().join("nara.toml");
    let manifest = fs::read_to_string(&manifest_path).unwrap();
    let watching_manifest = manifest.replace(
        "requested = ['runtime-2d']",
        "requested = ['runtime-2d', 'asset-watch']",
    );
    assert_ne!(watching_manifest, manifest);
    fs::write(manifest_path, watching_manifest).unwrap();
    let original_scene = fs::read(project.root().join(STARTUP)).unwrap();
    let proof = CloseProof::default();
    let watch = ProjectSceneWatchPlugin::new(project.root().to_path_buf())
        .unwrap()
        .with_queue_limits(AssetWatchQueueLimits::default());
    let product = recipe()
        .add_configured_plugin::<ProjectSceneWatchPlugin, _, _>(watch, Clone::clone)
        .unwrap()
        .add_configured_plugin::<PaletteGame, _, _>(
            GameFiles(project.root().to_path_buf()),
            |files| PaletteGame(files.clone()),
        )
        .unwrap()
        .add_configured_plugin::<retirement::ObserveClose, _, _>(proof.clone(), |proof| {
            retirement::ObserveClose(proof.clone())
        })
        .unwrap();
    let intent = HeadlessRunIntent::frames(NonZeroU32::new(60_000).unwrap(), Duration::ZERO)
        .with_recipe(product)
        .with_cleanup_timeout(Duration::from_secs(5))
        .stop_when(|game: &Workflow| game.step == Step::Done);
    let mut run = HeadlessRun::<Workflow>::new(project.capability(), intent, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(150);
    let outcome = loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(game) => {
                assert!(
                    !report.diagnostics().has_errors(),
                    "{:?}",
                    report.diagnostics()
                );
                break game.clone();
            }
            HeadlessRunOutcome::Failed => {
                panic!("physical source Host failed: {:?}", report.diagnostics())
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                assert!(
                    Instant::now() < deadline,
                    "the original source Host did not finish Stop"
                );
                std::thread::sleep(Duration::from_millis(2));
            }
        }
    };
    assert_eq!(outcome.step, Step::Done);
    assert_eq!(outcome.commits, 8);
    assert_eq!(outcome.rejections, 3);
    assert!(outcome.request.is_none());
    assert!(outcome.candidate.is_none());
    proof.assert_joined_before_world_drop();
    let observer = outcome.observer.unwrap();
    let stopped = observer.stats();
    assert!(stopped.started() > 0);
    assert_eq!(stopped.started(), stopped.joined());
    assert_eq!(stopped.active(), 0);
    assert_eq!(stopped.retiring(), 0);
    let final_state = outcome.current.unwrap();
    final_state
        .material
        .assert_shader(NEXT_SHADER, &shader("0.125"));
    assert_eq!(final_state.material.texture, discovered_product());
    assert_eq!(final_state.images[&discovered_product()].pixels, PIXEL);
    assert_eq!(
        final_state.gameplay,
        GameProgress {
            health: 7,
            score: 128,
            inventory: vec!["brass-key".to_owned(), "map".to_owned()],
        }
    );
    assert_eq!(
        fs::read(project.root().join(STARTUP)).unwrap(),
        original_scene
    );
    drop(run);
    assert_eq!(observer.stats(), stopped);
}
