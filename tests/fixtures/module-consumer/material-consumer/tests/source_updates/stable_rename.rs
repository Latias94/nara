use std::{fs, num::NonZeroU32, thread, time::Duration};

use nara::{
    ProductConfiguration, SchemaContribution,
    advanced_prelude::CurrentScene,
    app::{App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId},
    asset::{
        AssetEvent, AssetEventKind, AssetEvents, AssetId, AssetMeta, AssetMetaIdentity, AssetPath,
        AssetProductRef, AssetRef, AssetReferenceDemand, AssetServer, AssetSourceKind,
        AssetSourcePath, AssetStates, Assets, ImportedProductId, LoadState, ProjectAssetDatabase,
        StableAssetId,
    },
    core::ItemLimit,
    ecs::{Mut, Resource, World},
    identity::EntityLookup,
    image::{ImageAsset, ImageImportSettings},
    material::{Material2dAsset, Material2dDomain},
    project::ProjectPath,
    project_host::{
        HeadlessRunIntent, ProjectContentErrorKind, ProjectDomainAssets, ProjectSceneRequestError,
        ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes,
        advanced::commit_prepared_project_scene,
    },
    reflect::{
        AuthoringSchemaBundle, ComponentCapability, ComponentFieldId, ComponentFieldPath,
        ComponentFieldSchema, ComponentSchema, ComponentSchemaCatalog,
        ComponentSchemaOwnerSnapshot, ComponentSchemaVersion, ComponentTypeId, ComponentValueShape,
        PersistentComponentSchemaOwnerId, decode_asset_reference_value,
        encode_asset_reference_value,
    },
    scene::{
        SceneComponentRecord, SceneDocument, SceneEntityRecord, SceneSpawnReport,
        SourceDocumentOrigin, advanced::SceneProductTransactionLimits,
    },
};

use super::{
    GameFiles, GameProgress, INDICES_PIXEL, INDICES_PNG, NEXT_SHADER, PALETTE_PIXEL, PALETTE_PNG,
    asset_path, idle,
    observations::{Resident, assert_scene_owns_updates, resident},
    project::{
        MATERIAL_PATH, Project, SHADER, SHADER_PATH, STARTUP, actor, material_product,
        palette_product, recipe,
    },
    run_source_workflow, select_material, shader, status, write_asset,
};

const EMPTY_SCENE: &str = "empty.scene.json";
const STARTUP_SCENE: &str = "startup.scene.json";
const DEFERRED_SCENE: &str = "deferred.scene.json";
const DEFERRED_MATERIAL: &str = "consumer.palette.DeferredMaterial";

#[derive(Clone, Copy, Debug)]
enum Source {
    Image,
    Material,
}

impl Source {
    fn paths(self) -> [&'static str; 4] {
        match self {
            Self::Image => [
                "palette.png",
                "active-palette.png",
                "returned-palette.png",
                "duplicate-palette.png",
            ],
            Self::Material => [
                MATERIAL_PATH,
                "active-palette.material2d.json",
                "returned-palette.material2d.json",
                "duplicate-palette.material2d.json",
            ],
        }
    }

    fn product(self) -> AssetProductRef {
        match self {
            Self::Image => palette_product(),
            Self::Material => material_product(),
        }
    }

    fn kind(self) -> AssetSourceKind {
        match self {
            Self::Image => AssetSourceKind::Image,
            Self::Material => AssetSourceKind::Other(Material2dDomain::SOURCE_KIND.to_owned()),
        }
    }

    fn id(self, live: &Resident) -> AssetId {
        match self {
            Self::Image => live.images[&palette_product()].handle.id(),
            Self::Material => live.material_handle.id(),
        }
    }

    fn version(self, live: &Resident) -> u64 {
        let state = match self {
            Self::Image => &live.images[&palette_product()].state,
            Self::Material => &live.material_state,
        };
        state.version().raw()
    }

    fn write_sidecar(self, world: &World, path: &str) {
        let identity = AssetMetaIdentity::new(
            self.product().source(),
            AssetPath::new(path).unwrap(),
            self.kind(),
        );
        let metadata = match self {
            Self::Image => {
                AssetMeta::new(identity, ImageImportSettings::default()).to_json_string()
            }
            Self::Material => AssetMeta::new(identity, ()).to_json_string(),
        }
        .unwrap();
        write_asset(world, &format!("{path}.meta"), metadata);
    }
}

struct SourceEdit {
    png: &'static [u8],
    pixel: [u8; 4],
    shader_path: &'static str,
    gain: &'static str,
}

const ACTIVE_EDIT: SourceEdit = SourceEdit {
    png: PALETTE_PNG,
    pixel: PALETTE_PIXEL,
    shader_path: NEXT_SHADER,
    gain: "0.75",
};
const RETURN_EDIT: SourceEdit = SourceEdit {
    png: INDICES_PNG,
    pixel: INDICES_PIXEL,
    shader_path: "returned.wgsl",
    gain: "0.25",
};

#[derive(Clone, Copy, Debug)]
enum Exercise {
    ActiveThenAway,
    Away,
    Ambiguous,
    Occupied,
    DeferredOnly,
}

#[derive(Clone)]
struct RenameGame {
    files: GameFiles,
    source: Source,
    exercise: Exercise,
}

impl ProductConfiguration for RenameGame {
    fn write_canonical(&self, output: &mut Vec<u8>) {
        self.files.write_canonical(output);
        output.extend_from_slice(&[self.source as u8, self.exercise as u8]);
    }
}

impl Plugin for RenameGame {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.palette.stable-rename"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.world_mut()?.register_component::<GameProgress>();
        app.insert_resource(self.files.clone())?
            .insert_resource(RenameWorkflow {
                source: self.source,
                exercise: self.exercise,
                step: Step::Startup,
            })?
            .add_systems(CoreStage::Update, update_rename)?;
        Ok(())
    }
}

#[derive(Clone)]
enum Rescan {
    Rename,
    Ambiguous,
    Occupied(AssetId),
}

#[derive(Clone)]
enum Step {
    Startup,
    Rescanning(Rescan, Resident),
    Leaving(ProjectSceneRequestId, Resident, &'static str),
    Returning(ProjectSceneRequestId, Resident, &'static str),
    Deferring(ProjectSceneRequestId, Resident),
    Done,
}

#[derive(Clone, Resource)]
struct RenameWorkflow {
    source: Source,
    exercise: Exercise,
    step: Step,
}

fn startup_product() -> AssetProductRef {
    AssetProductRef::new(
        StableAssetId::parse_str("744cab2d-4c3c-43f0-bd13-8d72a8c698b4").unwrap(),
        ImportedProductId::PRIMARY,
    )
    .unwrap()
}

fn move_source(world: &World, source: Source, from: &str, to: &str) {
    let old_file = asset_path(world, from);
    let new_file = asset_path(world, to);
    let old_sidecar = asset_path(world, &format!("{from}.meta"));
    let new_sidecar = asset_path(world, &format!("{to}.meta"));
    fs::rename(&old_file, &new_file).unwrap();
    fs::rename(&old_sidecar, &new_sidecar).unwrap();
    source.write_sidecar(world, to);
    assert!(!old_file.exists() && !old_sidecar.exists());
    assert!(new_file.is_file() && new_sidecar.is_file());
}

fn rename(world: &World, source: Source, from: &str, to: &str, edit: &SourceEdit) {
    move_source(world, source, from, to);
    match source {
        Source::Image => write_asset(world, to, edit.png),
        Source::Material => {
            write_asset(world, edit.shader_path, shader(edit.gain));
            select_material(world, to, edit.shader_path, palette_product());
        }
    }
}

fn source_path(local: &str) -> AssetSourcePath {
    AssetSourcePath::new(format!("assets/{local}")).unwrap()
}

fn assert_location(world: &World, source: Source, live: &Resident, path: &str) {
    let database = world.resource::<ProjectAssetDatabase>();
    let record = database
        .record_for_stable_id(source.product().source())
        .unwrap();
    let canonical = source_path(path);
    assert_eq!(record.path(), &canonical);
    assert_eq!(record.source_kind(), &source.kind());
    assert_eq!(database.record_for_path(&canonical), Some(record));
    for other in source.paths().into_iter().filter(|other| *other != path) {
        assert_eq!(database.record_for_path(&source_path(other)), None);
    }
    let server = world.resource::<AssetServer>();
    assert_eq!(server.product(source.id(live)), Some(source.product()));
    assert_eq!(server.path(source.id(live)), Some(&canonical));
    match source {
        Source::Image => {
            let image = world
                .resource::<Assets<ImageAsset>>()
                .get(live.images[&palette_product()].handle)
                .unwrap();
            assert_eq!(image.source().unwrap().path(), &canonical);
        }
        Source::Material => {
            let collection = world
                .resource::<ProjectDomainAssets>()
                .collection::<Material2dDomain>()
                .unwrap();
            let mut records = collection.source_records();
            assert_eq!(records.len(), 1);
            assert_eq!(records.next(), Some(record));
        }
    }
}

fn assert_publication(
    world: &mut World,
    source: Source,
    before: &Resident,
    advance: u64,
    kind: AssetEventKind,
) {
    let id = source.id(before);
    let version = world.resource::<AssetStates>().state(id).unwrap().version();
    assert_eq!(version.raw(), source.version(before) + advance);
    let mut events = world.resource_mut::<AssetEvents>();
    assert!(!events.requires_rescan());
    let for_source = events
        .drain()
        .into_iter()
        .filter(|event| event.id() == id)
        .collect::<Vec<_>>();
    assert_eq!(for_source, [AssetEvent::new(id, version, kind)]);
}

fn assert_no_publication(world: &World) {
    let events = world.resource::<AssetEvents>();
    assert!(!events.requires_rescan());
    assert!(
        events.is_empty(),
        "unexpected asset publications: {events:?}"
    );
}

fn assert_unchanged(world: &mut World, source: Source, before: &Resident, path: &str) {
    assert_eq!(&resident(world), before);
    assert_location(world, source, before, path);
    assert_no_publication(world);
}

fn renamed_resident(
    world: &mut World,
    source: Source,
    before: &Resident,
    from: &str,
    to: &str,
    edit: &SourceEdit,
) -> Resident {
    let after = resident(world);
    assert_location(world, source, &after, to);
    assert_eq!(after.gameplay, before.gameplay);
    assert_ne!(after.stamp, before.stamp);
    let server = world.resource::<AssetServer>();
    let mut lookup = server.clone();
    assert_eq!(
        lookup.reserve_path_id(source_path(to)).unwrap(),
        source.id(before),
    );
    lookup.reserve_path_id(source_path(from)).unwrap();
    assert_ne!(&lookup, server, "the previous forward alias must be vacant");
    match source {
        Source::Image => {
            let previous = &before.images[&palette_product()];
            let current = &after.images[&palette_product()];
            assert_eq!(current.handle, previous.handle);
            assert_eq!(after.bound_texture, previous.handle);
            assert_eq!(current.pixels, edit.pixel);
            assert_ne!(current.hash, previous.hash);
            assert_ne!(current.state.source_hash(), previous.state.source_hash());
            assert_ne!(current.revision, previous.revision);
        }
        Source::Material => {
            assert_eq!(after.material_handle, before.material_handle);
            after
                .material
                .assert_shader(edit.shader_path, &shader(edit.gain));
            assert_ne!(after.material.shader, before.material.shader);
            assert_eq!(after.material.interface, before.material.interface);
            assert_eq!(after.material.declaration, before.material.declaration);
            assert_eq!(after.material.texture, palette_product());
            assert_ne!(after.material_revision, before.material_revision);
        }
    }
    after
}

fn assert_no_material_payload(domains: &ProjectDomainAssets) {
    assert!(
        domains
            .collection::<Material2dDomain>()
            .is_none_or(|collection| {
                collection.iter().len() == 0 && collection.source_records().len() == 0
            }),
    );
}

fn assert_retired_values(world: &World, source: Source, before: &Resident) {
    assert!(world.get::<GameProgress>(before.entity).is_none());
    assert!(world.resource::<Assets<ImageAsset>>().is_empty());
    assert!(world.resource::<Assets<Material2dAsset>>().is_empty());
    assert_no_material_payload(world.resource::<ProjectDomainAssets>());
    let states = world.resource::<AssetStates>();
    let state = states.state(source.id(before)).unwrap();
    assert_eq!(state.load_state(), &LoadState::Removed);
    assert_eq!(state.usable_version(), None);
    assert_eq!(state.version().raw(), source.version(before) + 1);
}

fn assert_retired(world: &World, source: Source, before: &Resident) {
    let current = CurrentScene::get(world).unwrap();
    assert!(current.source().roots().is_empty());
    assert_eq!(current.receipt().domain_id(), before.instance.domain_id());
    assert_ne!(current.receipt(), &before.instance);
    assert_retired_values(world, source, before);
    let database = world.resource::<ProjectAssetDatabase>();
    assert_eq!(
        database.record_for_stable_id(source.product().source()),
        None
    );
    for path in source.paths() {
        assert_eq!(database.record_for_path(&source_path(path)), None);
    }
}

fn rescan(world: &mut World, action: Rescan, incumbent: Resident) -> Step {
    ProjectScenes::scope(world, |scenes| scenes.request_source_rescan()).unwrap();
    let update = status(world);
    assert_eq!(update.request, None);
    assert!(update.pending);
    Step::Rescanning(action, incumbent)
}

fn leave(world: &mut World, incumbent: Resident, path: &'static str) -> Step {
    let request = ProjectScenes::scope(world, |scenes| {
        scenes.request(ProjectPath::new(EMPTY_SCENE).unwrap())
    })
    .unwrap();
    Step::Leaving(request, incumbent, path)
}

fn is_preparing(world: &mut World, request: ProjectSceneRequestId) -> bool {
    let state = ProjectScenes::scope(world, |scenes| scenes.state(request)).unwrap();
    if state == ProjectSceneRequestState::Preparing {
        thread::sleep(Duration::from_millis(1));
        return true;
    }
    false
}

fn commit_scene(
    world: &mut World,
    request: ProjectSceneRequestId,
    progress: Option<GameProgress>,
) -> SceneSpawnReport {
    commit_prepared_project_scene(
        world,
        request,
        SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE),
        &[],
        move |overlay| {
            if let Some(progress) = progress {
                overlay.insert_component(actor(), progress);
            }
        },
    )
}

fn commit_ready(
    world: &mut World,
    request: ProjectSceneRequestId,
    scene_path: &str,
    scene_product: Option<AssetProductRef>,
    progress: Option<GameProgress>,
) {
    let expected = CurrentScene::get(world).unwrap().receipt().clone();
    let actor_root = progress.as_ref().map(|_| actor());
    let stamp = ProjectScenes::scope(world, |scenes| {
        assert_eq!(
            scenes.state(request),
            Some(ProjectSceneRequestState::Ready),
            "{scene_path}: {:?}",
            scenes.diagnostics(request),
        );
        let view = scenes.prepared(request).unwrap();
        assert_eq!(view.expected_instance(), &expected);
        assert_eq!(view.selected_scene_path().unwrap().as_str(), scene_path);
        assert_eq!(view.selected_scene_product(), scene_product);
        assert_eq!(view.scene().roots(), actor_root.as_slice());
        view.source_content_stamp().unwrap()
    });
    let report = commit_scene(world, request, progress);
    assert!(
        !report.diagnostics.has_errors(),
        "{scene_path}: {:?}",
        report.diagnostics,
    );
    let current = CurrentScene::get(world).unwrap();
    assert_eq!(report.instance.as_ref(), Some(current.receipt()));
    assert_eq!(current.receipt().domain_id(), expected.domain_id());
    assert_ne!(current.receipt(), &expected);
    assert_eq!(current.source_content_stamp(), Some(stamp));
    let source = current.source_view();
    let scene_location = format!("scenes/{scene_path}");
    assert_eq!(
        source.with_compiled_scene(|candidate, _| {
            matches!(
                candidate.source_key().origin(),
                SourceDocumentOrigin::ProjectLocation { path, .. }
                    if path.as_str() == scene_location.as_str()
            )
        }),
        Some(true),
    );
    ProjectScenes::scope(world, |scenes| {
        assert_eq!(
            scenes.state(request),
            Some(ProjectSceneRequestState::Committed),
        );
    });
}

fn assert_ambiguous(world: &mut World, request: ProjectSceneRequestId) {
    let error = ProjectSceneRequestError::Content(ProjectContentErrorKind::AssetMetaMismatch);
    assert_eq!(status(world).error, Some(error));
    ProjectScenes::scope(world, |scenes| {
        let diagnostics = scenes.diagnostics(request).unwrap();
        assert_eq!(
            scenes.state(request),
            Some(ProjectSceneRequestState::Rejected(error)),
            "{diagnostics:?}",
        );
        assert!(scenes.prepared(request).is_none());
        assert!(
            diagnostics.iter().any(|diagnostic| {
                diagnostic.code().as_str() == "project.content.asset-identity-conflict"
            }),
            "{diagnostics:?}",
        );
    });
}

fn assert_occupied(
    world: &mut World,
    request: ProjectSceneRequestId,
    before: &Resident,
    target: &str,
    occupied: AssetId,
) {
    ProjectScenes::scope(world, |scenes| {
        assert_eq!(
            scenes.state(request),
            Some(ProjectSceneRequestState::Ready),
            "{:?}",
            scenes.diagnostics(request),
        );
        assert!(scenes.prepared(request).is_some());
    });
    let server = world.resource::<AssetServer>().clone();
    assert_eq!(server.path(occupied), Some(&source_path(target)));
    let report = commit_scene(world, request, Some(before.gameplay.clone()));
    assert!(report.diagnostics.has_errors());
    assert!(report.instance.is_none());
    assert_eq!(world.resource::<AssetServer>(), &server);
    ProjectScenes::scope(world, |scenes| {
        assert_eq!(
            scenes.state(request),
            Some(ProjectSceneRequestState::Rejected(
                ProjectSceneRequestError::CommitRejected,
            )),
        );
    });
}

fn commit_deferred_material(
    world: &mut World,
    request: ProjectSceneRequestId,
    before: &Resident,
    original: &str,
    moved: &str,
) {
    let product = material_product();
    ProjectScenes::scope(world, |scenes| {
        let view = scenes.prepared(request).unwrap_or_else(|| {
            panic!(
                "Deferred Material preparation failed: {:?}",
                scenes.diagnostics(request),
            );
        });
        assert_no_material_payload(view.domain_assets());
        let components = view.scene().entities()[0].components();
        assert_eq!(components.len(), 1);
        let record = &components[&ComponentTypeId::new(DEFERRED_MATERIAL)];
        assert_eq!(
            decode_asset_reference_value(record.value()).unwrap(),
            AssetRef::Product(product),
        );
    });
    let server = world.resource::<AssetServer>().clone();
    commit_ready(
        world,
        request,
        DEFERRED_SCENE,
        None,
        Some(before.gameplay.clone()),
    );
    assert_retired_values(world, Source::Material, before);
    let current_server = world.resource::<AssetServer>();
    assert_eq!(current_server, &server);
    assert_eq!(
        current_server.path(before.material_handle.id()),
        Some(&source_path(original)),
    );
    let database = world.resource::<ProjectAssetDatabase>();
    let record = database.record_for_stable_id(product.source()).unwrap();
    assert_eq!(record.path(), &source_path(moved));
    assert_eq!(record.source_kind(), &Source::Material.kind());
    assert_eq!(database.record_for_path(record.path()), Some(record));
    assert_eq!(database.record_for_path(&source_path(original)), None);
    assert_publication(world, Source::Material, before, 1, AssetEventKind::Removed);
}

fn acknowledge(world: &mut World, request: ProjectSceneRequestId) {
    ProjectScenes::scope(world, |scenes| {
        assert!(scenes.acknowledge(request));
        assert_eq!(scenes.state(request), None);
    });
    idle(world);
}

fn update_rename(world: &mut World) {
    assert_scene_owns_updates(world);
    world.resource_scope(|world, mut workflow: Mut<RenameWorkflow>| {
        let source = workflow.source;
        let [original, active, returned, duplicate] = source.paths();
        match &workflow.step {
            Step::Startup => {
                idle(world);
                let lookup = CurrentScene::get(world)
                    .unwrap()
                    .receipt()
                    .resolve(world, &actor());
                let EntityLookup::Resolved(entity) = lookup else {
                    panic!("the startup actor must be resident");
                };
                world.entity_mut(entity).insert(GameProgress {
                    health: 7,
                    score: 123,
                    inventory: vec!["brass-key".to_owned()],
                });
                let before = resident(world);
                before.material.assert_shader(SHADER_PATH, SHADER);
                assert_location(world, source, &before, original);
                assert_publication(world, source, &before, 0, AssetEventKind::Added);
                workflow.step = match workflow.exercise {
                    Exercise::ActiveThenAway => {
                        rename(world, source, original, active, &ACTIVE_EDIT);
                        rescan(world, Rescan::Rename, before)
                    }
                    Exercise::Away => leave(world, before, original),
                    Exercise::Ambiguous => {
                        fs::copy(asset_path(world, original), asset_path(world, duplicate))
                            .unwrap();
                        source.write_sidecar(world, duplicate);
                        rescan(world, Rescan::Ambiguous, before)
                    }
                    Exercise::Occupied => {
                        let path = source_path(active);
                        let mut server = world.resource_mut::<AssetServer>();
                        let occupied = match source {
                            Source::Image => {
                                server.reserve::<ImageAsset>(path.as_str()).unwrap().id()
                            }
                            Source::Material => server
                                .reserve::<Material2dAsset>(path.as_str())
                                .unwrap()
                                .id(),
                        };
                        assert_ne!(occupied, source.id(&before));
                        rename(world, source, original, active, &ACTIVE_EDIT);
                        rescan(world, Rescan::Occupied(occupied), before)
                    }
                    Exercise::DeferredOnly => {
                        move_source(world, source, original, active);
                        let request = ProjectScenes::scope(world, |scenes| {
                            scenes.request(ProjectPath::new(DEFERRED_SCENE).unwrap())
                        })
                        .unwrap();
                        Step::Deferring(request, before)
                    }
                };
            }
            Step::Rescanning(action, before) => {
                assert_unchanged(world, source, before, original);
                let update = status(world);
                let Some(request) = update.request else {
                    assert!(update.pending);
                    return;
                };
                if is_preparing(world, request) {
                    return;
                }
                match action {
                    Rescan::Rename => {
                        commit_ready(
                            world,
                            request,
                            STARTUP_SCENE,
                            None,
                            Some(before.gameplay.clone()),
                        );
                        let after =
                            renamed_resident(world, source, before, original, active, &ACTIVE_EDIT);
                        assert_publication(world, source, before, 1, AssetEventKind::Modified);
                        acknowledge(world, request);
                        workflow.step = leave(world, after, active);
                        return;
                    }
                    Rescan::Ambiguous => assert_ambiguous(world, request),
                    Rescan::Occupied(occupied) => {
                        assert_occupied(world, request, before, active, *occupied);
                    }
                }
                acknowledge(world, request);
                assert_unchanged(world, source, before, original);
                workflow.step = Step::Done;
            }
            Step::Leaving(request, before, path) => {
                let path = *path;
                assert_unchanged(world, source, before, path);
                if is_preparing(world, *request) {
                    return;
                }
                commit_ready(world, *request, EMPTY_SCENE, None, None);
                assert_retired(world, source, before);
                assert_publication(world, source, before, 1, AssetEventKind::Removed);
                acknowledge(world, *request);
                rename(world, source, path, returned, &RETURN_EDIT);
                let request =
                    ProjectScenes::scope(world, |scenes| scenes.request(startup_product()))
                        .unwrap();
                workflow.step = Step::Returning(request, before.clone(), path);
            }
            Step::Returning(request, before, path) => {
                assert_retired(world, source, before);
                assert_no_publication(world);
                if is_preparing(world, *request) {
                    return;
                }
                commit_ready(
                    world,
                    *request,
                    STARTUP_SCENE,
                    Some(startup_product()),
                    Some(before.gameplay.clone()),
                );
                renamed_resident(world, source, before, path, returned, &RETURN_EDIT);
                assert_publication(world, source, before, 2, AssetEventKind::Added);
                acknowledge(world, *request);
                workflow.step = Step::Done;
            }
            Step::Deferring(request, before) => {
                assert_unchanged(world, source, before, original);
                if is_preparing(world, *request) {
                    return;
                }
                commit_deferred_material(world, *request, before, original, active);
                acknowledge(world, *request);
                workflow.step = Step::Done;
            }
            Step::Done => unreachable!("the finite HeadlessRun must stop after completion"),
        }
    });
}

fn write_deferred_scene(project: &Project) -> ComponentSchemaOwnerSnapshot {
    let component = ComponentTypeId::new(DEFERRED_MATERIAL);
    let field = ComponentFieldSchema::required(
        ComponentFieldId::new("target"),
        "Target",
        ComponentFieldPath::empty(),
        ComponentValueShape::asset_ref(AssetReferenceDemand::Deferred),
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_capability(ComponentCapability::AssetRef);
    let schema = ComponentSchema::new(
        component.clone(),
        "Deferred Material",
        ComponentSchemaVersion::ONE,
    )
    .with_capabilities(ComponentCapability::SCENE_AUTHORING)
    .with_fields([field]);
    let owner = ComponentSchemaOwnerSnapshot::admit(
        PersistentComponentSchemaOwnerId::try_new("consumer.palette.deferred-material").unwrap(),
        ComponentSchemaCatalog {
            components: vec![schema],
            ..Default::default()
        },
        None,
    )
    .unwrap();
    let bundle = AuthoringSchemaBundle::from_owner_snapshots(
        project
            .bundle
            .owners()
            .iter()
            .cloned()
            .chain([owner.clone()]),
    )
    .unwrap();
    fs::write(
        project.root().join("schema/authoring.bundle.json"),
        bundle.to_json_string().unwrap(),
    )
    .unwrap();
    let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(actor())
        .with_component(
            component,
            SceneComponentRecord::new(
                ComponentSchemaVersion::ONE,
                encode_asset_reference_value(&AssetRef::Product(material_product())),
            )
            .with_owner(owner.owner_id().clone()),
        )]);
    project.declare_scenes(&[DEFERRED_SCENE]);
    project.write_scene(&format!("scenes/{DEFERRED_SCENE}"), &scene);
    owner
}

fn run_rename(source: Source, exercise: Exercise) {
    let project = Project::new();
    let startup = project.root().join(STARTUP);
    let original_scene = fs::read(&startup).unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            startup_product().source(),
            AssetPath::new(STARTUP_SCENE).unwrap(),
            AssetSourceKind::Scene,
        ),
        (),
    );
    fs::write(
        project.root().join(format!("{STARTUP}.meta")),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
    let game = RenameGame {
        files: GameFiles(project.root().to_path_buf()),
        source,
        exercise,
    };
    let product = if matches!(exercise, Exercise::DeferredOnly) {
        let owner = write_deferred_scene(&project);
        recipe().add_contribution(
            SchemaContribution::<RenameGame>::data_configured(game, RenameGame::clone, [owner])
                .unwrap(),
        )
    } else {
        project.declare_scenes(&[EMPTY_SCENE]);
        project.write_scene(&format!("scenes/{EMPTY_SCENE}"), &SceneDocument::default());
        recipe().add_configured_plugin::<RenameGame, _, _>(game, RenameGame::clone)
    }
    .unwrap();
    let intent = HeadlessRunIntent::frames(NonZeroU32::new(6000).unwrap(), Duration::ZERO)
        .with_recipe(product)
        .with_cleanup_timeout(Duration::from_secs(5))
        .stop_when(|game: &RenameWorkflow| matches!(&game.step, Step::Done));
    let outcome = run_source_workflow(&project, intent);
    assert!(matches!(outcome.step, Step::Done));
    assert_eq!(fs::read(startup).unwrap(), original_scene);
}

#[test]
fn stable_image_rename_keeps_the_active_handle_and_reuses_it_after_travel() {
    run_rename(Source::Image, Exercise::ActiveThenAway);
}

#[test]
fn stable_material_rename_keeps_the_active_handle_and_reuses_it_after_travel() {
    run_rename(Source::Material, Exercise::ActiveThenAway);
}

#[test]
fn stable_image_rename_while_away_reuses_the_retired_handle() {
    run_rename(Source::Image, Exercise::Away);
}

#[test]
fn stable_material_rename_while_away_reuses_the_retired_handle() {
    run_rename(Source::Material, Exercise::Away);
}

#[test]
fn stable_image_rename_refuses_two_actual_source_claims() {
    run_rename(Source::Image, Exercise::Ambiguous);
}

#[test]
fn stable_material_rename_refuses_two_actual_source_claims() {
    run_rename(Source::Material, Exercise::Ambiguous);
}

#[test]
fn stable_image_rename_refuses_a_path_reserved_for_another_handle() {
    run_rename(Source::Image, Exercise::Occupied);
}

#[test]
fn stable_material_rename_refuses_a_path_reserved_for_another_handle() {
    run_rename(Source::Material, Exercise::Occupied);
}

#[test]
fn stable_material_rename_to_deferred_only_retires_the_old_value() {
    run_rename(Source::Material, Exercise::DeferredOnly);
}
