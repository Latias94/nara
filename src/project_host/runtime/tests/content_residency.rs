use super::*;

use nara_app::CoreStage;
use nara_asset::{
    ArtifactFormatVersion, ArtifactLabel, AssetEvent, AssetRecord, AssetSlotRevision, AssetState,
    AssetStateRevision, Handle, ImportArtifactKey, ImportArtifactRecord, ImportDependencyDigest,
    ImportProfile, ImportedAssetType, ImporterId, ImporterVersion, ProjectAssetDatabase,
    SourceHash,
};
use nara_core::{ByteLimit, ItemLimit, Vec2};
use nara_ecs::{
    ResMut, upstream::lifecycle::Remove, upstream::observer::On, upstream::system::SystemState,
};
use nara_identity::SpawnedSceneInstance;
use nara_image::{ImageExtent, ImageFormat, ImageSourceMetadata};
use nara_project::ProjectPath;
use nara_scene::advanced::SceneProductTransactionLimits;
use nara_tilemap::{
    TileAtlasLayout, TileSetAssetError, TileSetAssetLimits, TileSetAssets, TileSetContent,
    TileSetRetentionSnapshot, TileSetSource,
};

use super::super::content_residency::replace_project_scene;

const SHARED_ID: &str = "53000000-0000-4000-8000-000000000001";
const UNIQUE_ID: &str = "53000000-0000-4000-8000-000000000002";
const TILESET_ID: &str = "53000000-0000-4000-8000-000000000003";
const FOREIGN_IMAGE_ID: &str = "53000000-0000-4000-8000-000000000004";
const FOREIGN_TILESET_ID: &str = "53000000-0000-4000-8000-000000000005";

type ProbeAction = Box<dyn FnOnce(&mut World) + Send + Sync>;

#[derive(Default)]
struct ResidencyProbe {
    next: Mutex<Option<ProbeAction>>,
}

struct ResidencyProbePlugin {
    probe: Arc<ResidencyProbe>,
    foreign: bool,
    limits: Option<TileSetAssetLimits>,
}

impl Plugin for ResidencyProbePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("nara.test.project-residency"),
            PluginCategory::Runtime,
        )
        .requires_plugins(&[nara_tilemap::TILESET_ASSET_PLUGIN_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        if let Some(limits) = self.limits {
            app.insert_resource(TileSetAssets::with_limits(limits))?;
        }
        if self.foreign {
            install_foreign_publication(app.world_mut()?);
        }
        let probe = Arc::clone(&self.probe);
        app.add_systems(CoreStage::Update, move |world: &mut World| {
            let action = probe.next.lock().unwrap().take();
            if let Some(action) = action {
                action(world);
            }
        })?;
        Ok(())
    }
}

struct ResidencyFixture {
    project: TestProject,
    candidate: ProjectSettingsCandidate,
    plan: RuntimePlan,
    loader: ProjectContentLoader,
    probe: Arc<ResidencyProbe>,
    host: ProjectHost,
}

impl ResidencyFixture {
    fn new(foreign: bool, limits: Option<TileSetAssetLimits>) -> Self {
        let project = TestProject::new("project-scene-residency");
        write_image(&project, "shared.png", SHARED_ID, [255, 0, 0, 255]);
        write_image(&project, "unique.png", UNIQUE_ID, [0, 255, 0, 255]);
        write_tileset(&project, TileAtlasLayout::grid(Vec2::ONE, 1, 1));
        write_scene(&project, "startup.scene.json", "level-a", true, true);
        write_scene(&project, "b.scene.json", "level-b", true, false);
        write_scene(&project, "empty.scene.json", "empty", false, false);
        let root = project.capability();
        let manifest = root
            .open_file(&RelativePath::new(PROJECT_MANIFEST).unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        drop(manifest);
        let probe = Arc::new(ResidencyProbe::default());
        let factory_probe = Arc::clone(&probe);
        let definition = PluginDefinition::infallible::<ResidencyProbePlugin, _>(
            PluginDefinitionId::new("nara.test.project-residency", 1),
            [u8::from(foreign), u8::from(limits.is_some())],
            move || ResidencyProbePlugin {
                probe: Arc::clone(&factory_probe),
                foreign,
                limits,
            },
        );
        let request = project_runtime_plugins(&candidate)
            .insert_after::<nara_tilemap::TileSetAssetPlugin>(definition);
        let plan = resolve_runtime_plan(&candidate, request, built_in_schema_providers()).unwrap();
        let loader = ProjectContentLoader::new(root).unwrap();
        let snapshot = load_runtime_snapshot(&loader, &candidate, &plan);
        let mut host = ProjectHost::new(RuntimeClosePolicy::default());
        let mut attempt = host
            .begin_start(snapshot, plan.clone(), Vec::new())
            .unwrap();
        host.complete_start(&mut attempt).unwrap();
        drop(attempt);
        Self {
            project,
            candidate,
            plan,
            loader,
            probe,
            host,
        }
    }

    fn load(&self, path: &str) -> ProjectContentSnapshot {
        self.loader
            .load_scene(
                &self.candidate,
                &self.plan,
                &ProjectPath::new(path).unwrap(),
            )
            .unwrap()
    }

    fn step(&mut self, action: impl FnOnce(&mut World) + Send + Sync + 'static) {
        assert!(
            self.probe
                .next
                .lock()
                .unwrap()
                .replace(Box::new(action))
                .is_none()
        );
        self.host.drive_running_runtime(Duration::ZERO).unwrap();
        assert!(self.probe.next.lock().unwrap().is_none());
    }

    fn close(&mut self) {
        close_host(&mut self.host);
        assert_eq!(self.loader.budget_snapshot().active_generations(), 0);
        assert_eq!(self.loader.budget_snapshot().active_reservations(), 0);
        assert_eq!(
            self.loader
                .budget_snapshot()
                .active(ProjectContentBudgetKind::RetainedBytes),
            0
        );
    }
}

fn write_image(project: &TestProject, path: &str, id: &str, color: [u8; 4]) {
    image::RgbaImage::from_pixel(1, 1, image::Rgba(color))
        .save(project.root.join("assets").join(path))
        .unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str(id).unwrap(),
            AssetPath::new(path).unwrap(),
            AssetSourceKind::Image,
        ),
        ImageImportSettings::default(),
    );
    fs::write(
        project.root.join("assets").join(format!("{path}.meta")),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
}

fn write_tileset(project: &TestProject, atlas: TileAtlasLayout) {
    let source = TileSetSource::new(AssetRef::path("unique.png").unwrap(), atlas);
    fs::write(
        project.root.join("assets/terrain.tileset.ron"),
        source.encode_ron_bytes().unwrap(),
    )
    .unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str(TILESET_ID).unwrap(),
            AssetPath::new("terrain.tileset.ron").unwrap(),
            AssetSourceKind::Other(String::from("tileset")),
        ),
        (),
    );
    fs::write(
        project.root.join("assets/terrain.tileset.ron.meta"),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
}

fn write_scene(project: &TestProject, path: &str, name: &str, shared: bool, tiles: bool) {
    let mut entities = Vec::new();
    if shared {
        entities.push(
            SceneEntityRecord::new(nara_identity::SceneEntityId::new(name).unwrap())
                .with_component(
                    ComponentTypeId::new("nara.transform.Transform2d"),
                    identity_transform_record(),
                )
                .with_component(
                    ComponentTypeId::new("nara.sprite.Sprite"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        sprite_value(&AssetRef::path("shared.png").unwrap()),
                    ),
                ),
        );
    }
    if tiles {
        entities.push(
            SceneEntityRecord::new(nara_identity::SceneEntityId::new("terrain").unwrap())
                .with_component(
                    ComponentTypeId::new("nara.transform.Transform2d"),
                    identity_transform_record(),
                )
                .with_component(
                    ComponentTypeId::new("nara.tilemap.Tilemap"),
                    SceneComponentRecord::new(
                        ComponentSchemaVersion::ONE,
                        ComponentValue::map([
                            (
                                "tile_size",
                                ComponentValue::map([
                                    ("x", ComponentValue::f64(1.0).unwrap()),
                                    ("y", ComponentValue::f64(1.0).unwrap()),
                                ]),
                            ),
                            (
                                "tileset",
                                ComponentValue::map([
                                    ("kind", ComponentValue::String(String::from("path"))),
                                    (
                                        "value",
                                        ComponentValue::String(String::from("terrain.tileset.ron")),
                                    ),
                                ]),
                            ),
                            ("cells", ComponentValue::List(Vec::new())),
                        ]),
                    ),
                ),
        );
    }
    fs::write(
        project.root.join("scenes").join(path),
        SceneDocument::from_ordered_roots(entities)
            .to_json_string()
            .unwrap(),
    )
    .unwrap();
}

fn limits() -> SceneProductTransactionLimits {
    SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE)
}

fn replace(world: &mut World, snapshot: &ProjectContentSnapshot) -> SpawnedSceneInstance {
    let before = world.resource::<CurrentSceneOwner>().receipt().clone();
    let report = replace_project_scene(world, &before, snapshot, limits(), &[], |_| {});
    assert!(!report.diagnostics.has_errors(), "{:?}", report.diagnostics);
    let receipt = report.instance.unwrap();
    assert_ne!(receipt.instance_id(), before.instance_id());
    assert_eq!(world.resource::<CurrentSceneOwner>().receipt(), &receipt);
    assert_eq!(
        world.resource::<CurrentSceneOwner>().source(),
        snapshot.expanded_scene()
    );
    receipt
}

fn image_handle(world: &World, id: &str) -> Handle<ImageAsset> {
    let stable = StableAssetId::parse_str(id).unwrap();
    let server = world.resource::<AssetServer>();
    let record = world
        .resource::<ProjectAssetDatabase>()
        .record_for_stable_id(stable)
        .unwrap();
    let handle = world
        .resource::<Assets<ImageAsset>>()
        .iter()
        .find(|(_, image)| image.source().stable_id() == record.stable_id())
        .unwrap()
        .0;
    assert_eq!(server.get_handle::<ImageAsset>(handle.id()), Some(handle));
    handle
}

#[derive(Resource)]
struct ForeignPublication {
    image: Handle<ImageAsset>,
    tileset: Handle<TileSet>,
    value: TileSet,
    slot: AssetSlotRevision,
    state: AssetStateRevision,
    image_slot: AssetSlotRevision,
    image_state: AssetStateRevision,
}

fn foreign_image(record: &AssetRecord) -> ImageAsset {
    let pixels = vec![30, 40, 50, 255];
    let settings = ImageImportSettings::default();
    let key = ImportArtifactKey::new(
        record.stable_id(),
        SourceHash::from_bytes(&pixels),
        ImportDependencyDigest::empty(),
        ImporterId::new("test.residency").unwrap(),
        ImporterVersion::new(1),
        settings.settings_hash(),
        ImportProfile::default(),
        ImportedAssetType::new("image").unwrap(),
        ArtifactLabel::default(),
        ArtifactFormatVersion::new(1),
    );
    ImageAsset::new(
        ImageSourceMetadata::new(
            record.path().clone(),
            settings,
            ImportArtifactRecord::new(key).unwrap(),
        )
        .unwrap(),
        ImageExtent::new(1, 1),
        ImageFormat::Rgba8,
        pixels,
    )
    .unwrap()
}

fn install_foreign_publication(world: &mut World) {
    let image_record = AssetRecord::new(
        StableAssetId::parse_str(FOREIGN_IMAGE_ID).unwrap(),
        AssetPath::new("foreign.png").unwrap(),
        AssetSourceKind::Image,
    );
    let tileset_record = AssetRecord::new(
        StableAssetId::parse_str(FOREIGN_TILESET_ID).unwrap(),
        AssetPath::new("foreign.tileset.ron").unwrap(),
        AssetSourceKind::Other(String::from("tileset")),
    );
    world.init_resource::<ProjectAssetDatabase>();
    world
        .resource_mut::<ProjectAssetDatabase>()
        .insert(image_record.clone())
        .unwrap();
    world
        .resource_mut::<ProjectAssetDatabase>()
        .insert(tileset_record.clone())
        .unwrap();
    let image = world
        .resource_mut::<AssetServer>()
        .reserve_record::<ImageAsset>(&image_record)
        .unwrap();
    world.resource_scope(|world, mut images: Mut<Assets<ImageAsset>>| {
        world.resource_scope(|world, mut states: Mut<AssetStates>| {
            images
                .commit_loaded(
                    image,
                    foreign_image(&image_record),
                    &mut states,
                    &mut world.resource_mut::<AssetEvents>(),
                    None,
                    None,
                )
                .unwrap();
        });
    });
    let mut access = SystemState::<TileSetContent>::new(world);
    let tileset = {
        let mut content = access.get_mut(world).unwrap();
        let tileset = content
            .request(
                tileset_record,
                TileSetSource::new(
                    AssetRef::path("foreign.png").unwrap(),
                    TileAtlasLayout::grid(Vec2::ONE, 1, 1),
                ),
                SourceHash::from_bytes(b"foreign"),
            )
            .unwrap();
        content.reconcile();
        assert_eq!(content.error(tileset), None);
        tileset
    };
    world.insert_resource(ForeignPublication {
        image,
        tileset,
        value: world
            .resource::<Assets<TileSet>>()
            .get(tileset)
            .unwrap()
            .clone(),
        slot: world.resource::<Assets<TileSet>>().slot_revision(tileset),
        state: world.resource::<AssetStates>().state_revision(tileset.id()),
        image_slot: world.resource::<Assets<ImageAsset>>().slot_revision(image),
        image_state: world.resource::<AssetStates>().state_revision(image.id()),
    });
}

fn assert_foreign_unchanged(world: &World) {
    let foreign = world.resource::<ForeignPublication>();
    assert_eq!(
        world.resource::<Assets<TileSet>>().get(foreign.tileset),
        Some(&foreign.value)
    );
    assert_eq!(
        world
            .resource::<Assets<TileSet>>()
            .slot_revision(foreign.tileset),
        foreign.slot
    );
    assert_eq!(
        world
            .resource::<AssetStates>()
            .state_revision(foreign.tileset.id()),
        foreign.state
    );
    assert_eq!(
        world
            .resource::<Assets<ImageAsset>>()
            .slot_revision(foreign.image),
        foreign.image_slot
    );
    assert_eq!(
        world
            .resource::<AssetStates>()
            .state_revision(foreign.image.id()),
        foreign.image_state
    );
    assert_eq!(
        world.resource::<TileSetAssets>().error(foreign.tileset),
        None
    );
    for id in [FOREIGN_IMAGE_ID, FOREIGN_TILESET_ID] {
        assert!(
            world
                .resource::<ProjectAssetDatabase>()
                .record_for_stable_id(StableAssetId::parse_str(id).unwrap())
                .is_some()
        );
    }
}

#[test]
fn hosted_residency_hands_off_shared_pixels_retires_old_only_and_preserves_foreign_publications() {
    let mut fixture = ResidencyFixture::new(true, None);
    let (shared, unique, tile, a_pixels) = fixture
        .host
        .with_running_world(|world| {
            assert_foreign_unchanged(world); // Startup must never temporarily revoke foreign TileSets.
            let shared = image_handle(world, SHARED_ID);
            let unique = image_handle(world, UNIQUE_ID);
            let tile = world
                .resource::<TileSetAssets>()
                .sources()
                .find(|(_, record, _)| {
                    record.stable_id() == StableAssetId::parse_str(TILESET_ID).unwrap()
                })
                .unwrap()
                .0;
            (
                shared,
                unique,
                tile,
                world
                    .resource::<Assets<ImageAsset>>()
                    .get(shared)
                    .unwrap()
                    .pixels()
                    .as_ptr() as usize,
            )
        })
        .unwrap();
    write_image(&fixture.project, "shared.png", SHARED_ID, [0, 0, 255, 255]);
    let b = fixture.load("b.scene.json");
    let b_pixels = b.images()[0].image().pixels().as_ptr() as usize;
    assert_ne!(b_pixels, a_pixels);
    fixture.step(move |world| {
        replace(world, &b);
        assert_eq!(image_handle(world, SHARED_ID), shared);
        assert_eq!(
            world
                .resource::<Assets<ImageAsset>>()
                .get(shared)
                .unwrap()
                .pixels()
                .as_ptr() as usize,
            b_pixels
        );
        assert_eq!(
            world
                .resource::<Assets<ImageAsset>>()
                .get(shared)
                .unwrap()
                .pixels(),
            &[0, 0, 255, 255]
        );
        assert!(world.resource::<Assets<ImageAsset>>().get(unique).is_none());
        assert!(world.resource::<Assets<TileSet>>().get(tile).is_none());
        assert!(
            world
                .resource::<ProjectAssetDatabase>()
                .record_for_stable_id(StableAssetId::parse_str(UNIQUE_ID).unwrap())
                .is_none()
        );
        assert!(
            world
                .resource::<ProjectAssetDatabase>()
                .record_for_stable_id(StableAssetId::parse_str(TILESET_ID).unwrap())
                .is_none()
        );
        assert_eq!(world.resource::<TileSetAssets>().sources().count(), 1);
        assert_foreign_unchanged(world);
    });
    assert_eq!(fixture.loader.budget_snapshot().active_generations(), 1);
    write_image(&fixture.project, "shared.png", SHARED_ID, [255, 0, 0, 255]);
    let a = load_runtime_snapshot(&fixture.loader, &fixture.candidate, &fixture.plan);
    fixture.step(move |world| {
        replace(world, &a);
        assert_eq!(image_handle(world, SHARED_ID), shared);
        assert_eq!(image_handle(world, UNIQUE_ID), unique);
        assert_eq!(
            world
                .resource::<Assets<ImageAsset>>()
                .get(shared)
                .unwrap()
                .pixels(),
            &[255, 0, 0, 255]
        );
        assert!(world.resource::<Assets<TileSet>>().get(tile).is_some());
        assert_foreign_unchanged(world);
    });
    let empty = fixture.load("empty.scene.json");
    fixture.step(move |world| {
        replace(world, &empty);
        assert!(
            world
                .resource::<CurrentSceneOwner>()
                .receipt()
                .entity_ids()
                .is_empty()
        );
        assert!(world.resource::<Assets<ImageAsset>>().get(shared).is_none());
        assert!(world.resource::<Assets<ImageAsset>>().get(unique).is_none());
        assert!(world.resource::<Assets<TileSet>>().get(tile).is_none());
        assert_eq!(world.resource::<Assets<ImageAsset>>().len(), 1);
        assert_eq!(world.resource::<Assets<TileSet>>().len(), 1);
        assert_eq!(world.resource::<ProjectAssetDatabase>().len(), 2);
        assert_foreign_unchanged(world);
    });
    fixture.close();
}

#[test]
fn hosted_residency_keeps_escaped_a_generation_until_last_drop_before_c_admission() {
    let mut fixture = ResidencyFixture::new(false, None);
    let escaped = fixture
        .host
        .with_running_world(|world| {
            world
                .resource::<Assets<ImageAsset>>()
                .get(image_handle(world, SHARED_ID))
                .unwrap()
                .share_retained()
                .unwrap()
        })
        .unwrap();
    let b = fixture.load("b.scene.json");
    fixture.step(move |world| {
        replace(world, &b);
    });
    assert_eq!(fixture.loader.budget_snapshot().active_generations(), 2);
    let before = fixture.loader.budget_snapshot();
    let result = fixture.loader.load_scene(
        &fixture.candidate,
        &fixture.plan,
        &ProjectPath::new("empty.scene.json").unwrap(),
    );
    assert!(
        matches!(result, Err(error) if error.kind() == crate::project_content::ProjectContentErrorKind::GenerationCapacityExceeded)
    );
    assert_eq!(
        fixture.loader.budget_snapshot().active_reservations(),
        before.active_reservations()
    );
    assert_eq!(
        fixture
            .loader
            .budget_snapshot()
            .active(ProjectContentBudgetKind::RetainedBytes),
        before.active(ProjectContentBudgetKind::RetainedBytes)
    );
    assert_eq!(escaped.pixels(), &[255, 0, 0, 255]);
    drop(escaped);
    assert_eq!(fixture.loader.budget_snapshot().active_generations(), 1);
    let c = fixture.load("empty.scene.json");
    fixture.step(move |world| {
        replace(world, &c);
    });
    assert_eq!(fixture.loader.budget_snapshot().active_generations(), 1);
    fixture.close();
}

#[derive(Debug, PartialEq)]
struct PublicationBaseline {
    receipt: SpawnedSceneInstance,
    source: usize,
    entities: u32,
    server: AssetServer,
    catalog: ProjectAssetDatabase,
    images: Vec<(nara_asset::AssetId, AssetSlotRevision, usize, Vec<u8>)>,
    tilesets: Vec<(nara_asset::AssetId, AssetSlotRevision, TileSet)>,
    desired_tilesets: Vec<(
        Handle<TileSet>,
        AssetRecord,
        TileSetSource,
        Option<TileSetAssetError>,
    )>,
    tile_retention: TileSetRetentionSnapshot,
    states: Vec<(nara_asset::AssetId, AssetState, AssetStateRevision)>,
    events: Vec<AssetEvent>,
    discarded_events: u64,
}

impl PublicationBaseline {
    fn capture(world: &World) -> Self {
        let mut images = world
            .resource::<Assets<ImageAsset>>()
            .iter()
            .map(|(handle, image)| {
                (
                    handle.id(),
                    world.resource::<Assets<ImageAsset>>().slot_revision(handle),
                    image.pixels().as_ptr() as usize,
                    image.pixels().to_vec(),
                )
            })
            .collect::<Vec<_>>();
        images.sort_by_key(|image| image.0);
        let mut tilesets = world
            .resource::<Assets<TileSet>>()
            .iter()
            .map(|(handle, value)| {
                (
                    handle.id(),
                    world.resource::<Assets<TileSet>>().slot_revision(handle),
                    value.clone(),
                )
            })
            .collect::<Vec<_>>();
        tilesets.sort_by_key(|tileset| tileset.0);
        let states = world
            .resource::<AssetStates>()
            .iter()
            .map(|(id, state)| {
                (
                    id,
                    state.clone(),
                    world.resource::<AssetStates>().state_revision(id),
                )
            })
            .collect();
        let tile_owner = world.resource::<TileSetAssets>();
        let desired_tilesets = tile_owner
            .sources()
            .map(|(handle, record, source)| {
                (
                    handle,
                    record.clone(),
                    source.clone(),
                    tile_owner.error(handle).cloned(),
                )
            })
            .collect();
        let events = world.resource::<AssetEvents>();
        Self {
            receipt: world.resource::<CurrentSceneOwner>().receipt().clone(),
            source: world.resource::<CurrentSceneOwner>().source() as *const SceneDocument as usize,
            entities: world.entities().len(),
            server: world.resource::<AssetServer>().clone(),
            catalog: world.resource::<ProjectAssetDatabase>().clone(),
            images,
            tilesets,
            desired_tilesets,
            tile_retention: tile_owner.retention(),
            states,
            events: events.iter().copied().collect(),
            discarded_events: events.discarded_events(),
        }
    }
}

fn assert_rejected(
    world: &mut World,
    expected: &SpawnedSceneInstance,
    snapshot: &ProjectContentSnapshot,
    code: &str,
) {
    let before = PublicationBaseline::capture(world);
    let report = replace_project_scene(world, expected, snapshot, limits(), &[], |_| {
        panic!("residency rejection must precede product configuration");
    });
    assert!(report.instance.is_none());
    assert!(
        report_has_code(&report.diagnostics, code),
        "{:?}",
        report.diagnostics
    );
    assert_eq!(PublicationBaseline::capture(world), before);
    for id in before.receipt.entity_ids() {
        assert!(matches!(
            before.receipt.resolve(world, id),
            EntityLookup::Resolved(_)
        ));
    }
}

#[test]
fn hosted_residency_rejects_atlas_retention_stale_and_foreign_image_authority_before_scene_commit()
{
    for limited in [false, true] {
        let tile_limits = limited.then(|| {
            TileSetAssetLimits::new(
                ItemLimit::new(4).unwrap(),
                ItemLimit::ONE,
                ByteLimit::new(4).unwrap(),
            )
        });
        let mut fixture = ResidencyFixture::new(false, tile_limits);
        write_scene(&fixture.project, "b.scene.json", "level-b", true, true);
        if !limited {
            write_tileset(
                &fixture.project,
                TileAtlasLayout::grid(Vec2::new(0.5, 1.0), 2, 1),
            );
        }
        let b = fixture.load("b.scene.json");
        fixture.step(move |world| {
            let current = world.resource::<CurrentSceneOwner>().receipt().clone();
            assert_rejected(
                world,
                &current,
                &b,
                "project.scene.tileset-preparation-failed",
            );
        });
        let b = fixture.load("b.scene.json");
        fixture.step(move |world| {
            let shared = image_handle(world, SHARED_ID);
            // A raw storage writer invalidates the exact slot even if its bytes are untouched.
            assert!(
                world
                    .resource_mut::<Assets<ImageAsset>>()
                    .get_mut(shared)
                    .is_some()
            );
            let current = world.resource::<CurrentSceneOwner>().receipt().clone();
            assert_rejected(world, &current, &b, "project.scene.image-ownership-changed");
        });
        fixture.close();
    }
    let mut fixture = ResidencyFixture::new(false, None);
    write_image(
        &fixture.project,
        "new.png",
        "53000000-0000-4000-8000-000000000006",
        [9, 8, 7, 255],
    );
    let b = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        nara_identity::SceneEntityId::new("new-image").unwrap(),
    )
    .with_component(
        ComponentTypeId::new("nara.transform.Transform2d"),
        identity_transform_record(),
    )
    .with_component(
        ComponentTypeId::new("nara.sprite.Sprite"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            sprite_value(&AssetRef::path("new.png").unwrap()),
        ),
    )]);
    fs::write(
        fixture.project.root.join("scenes/b.scene.json"),
        b.to_json_string().unwrap(),
    )
    .unwrap();
    let b = fixture.load("b.scene.json");
    fixture.step(move |world| {
        let record = b
            .asset_database()
            .record_for_path(&AssetPath::new("new.png").unwrap())
            .unwrap();
        let handle = world
            .resource_mut::<AssetServer>()
            .reserve_record::<ImageAsset>(record)
            .unwrap();
        world.resource_mut::<AssetStates>().set_loading(handle.id());
        assert!(world.resource::<Assets<ImageAsset>>().get(handle).is_none());
        let current = world.resource::<CurrentSceneOwner>().receipt().clone();
        assert_rejected(world, &current, &b, "project.scene.image-ownership-changed");
    });
    // A fresh instance makes the old receipt stale without changing project asset ownership.
    let b = fixture.load("b.scene.json");
    fixture.step(move |world| {
        let stale = world.resource::<CurrentSceneOwner>().receipt().clone();
        let report = crate::startup_scene::restart_current_scene_with_product(
            world,
            &stale,
            limits(),
            &[],
            |_| {},
        );
        assert!(!report.diagnostics.has_errors());
        assert_rejected(
            world,
            &stale,
            &b,
            "scene.current.expected-instance-mismatch",
        );
    });
    fixture.close();
}

#[test]
fn hosted_residency_rejects_tileset_owner_remove_observer_before_callback_or_publication() {
    let mut fixture = ResidencyFixture::new(false, None);
    let calls = Arc::new(AtomicUsize::new(0));
    let observer_calls = Arc::clone(&calls);
    let b = fixture.load("b.scene.json");
    fixture.step(move |world| {
        let shared = image_handle(world, SHARED_ID);
        world.add_observer(
            move |_: On<Remove<TileSetAssets>>, mut images: ResMut<Assets<ImageAsset>>| {
                observer_calls.fetch_add(1, Ordering::SeqCst);
                let _ = images.get_mut(shared);
            },
        );
        world.flush();
        let current = world.resource::<CurrentSceneOwner>().receipt().clone();
        assert_rejected(
            world,
            &current,
            &b,
            "project.scene.residency-lifecycle-unsupported",
        );
    });
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    fixture.close();
}

#[test]
fn hosted_empty_scene_replacement_needs_no_asset_event_resource_when_no_publication_is_planned() {
    let mut fixture = ResidencyFixture::new(false, None);
    let empty = fixture.load("empty.scene.json");
    fixture.step(move |world| {
        replace(world, &empty);
    });
    assert_eq!(fixture.loader.budget_snapshot().active_generations(), 1);
    let empty = fixture.load("empty.scene.json");
    assert!(empty.images().is_empty());
    assert!(empty.tilesets().is_empty());
    fixture.step(move |world| {
        let before = PublicationBaseline::capture(world);
        assert!(before.images.is_empty());
        assert!(before.tilesets.is_empty());
        let events = world.remove_resource::<AssetEvents>().unwrap();
        // Restore the resource before returning even if the commit violates its no-work contract,
        // so later runtime systems cannot obscure the original failure with a missing-resource fault.
        let result = catch_unwind(AssertUnwindSafe(|| replace(world, &empty)));
        world.insert_resource(events);
        let receipt = result.unwrap();
        assert_ne!(receipt.instance_id(), before.receipt.instance_id());
        assert_eq!(world.resource::<CurrentSceneOwner>().receipt(), &receipt);
        assert_eq!(
            world.resource::<CurrentSceneOwner>().source(),
            empty.expanded_scene()
        );
        let after = PublicationBaseline::capture(world);
        assert_eq!(after.server, before.server);
        assert_eq!(after.catalog, before.catalog);
        assert_eq!(after.images, before.images);
        assert_eq!(after.tilesets, before.tilesets);
        assert_eq!(after.desired_tilesets, before.desired_tilesets);
        assert_eq!(after.tile_retention, before.tile_retention);
        assert_eq!(after.states, before.states);
        assert_eq!(after.events, before.events);
        assert_eq!(after.discarded_events, before.discarded_events);
    });
    fixture.close();
}
