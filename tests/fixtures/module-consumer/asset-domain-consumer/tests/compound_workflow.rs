use std::{
    fs,
    num::NonZeroU32,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use nara::{
    ProductRecipe, SchemaContribution,
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId},
    ecs::{Component, Query, Resource, World, bevy_ecs::system::SystemState},
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode},
    project::ProjectPath,
    project_host::{
        HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, ProjectContentErrorKind,
        ProjectContentLoader, ProjectDomainAssets, ProjectSceneRequestId, ProjectSceneRequestState,
        ProjectScenes, advanced::commit_prepared_project_scene, ingest_project_manifest,
        resolve_product_recipe,
    },
    project_workflow::{ProjectCheckRequest, check_project},
    reflect::{
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponent, PersistentComponentProvider,
    },
    scene::{
        SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord,
        advanced::SceneProductTransactionLimits,
    },
};
use nara_asset::{AssetProductRef, ImportedProductId, RuntimeAssetCodec, SourceAssetImporter};
use nara_asset_domain_consumer::bundle::{TextBundle, TextBundleDomain, TextBundleSettings};

const SOURCE: &str = "cc91a0c8-e2b4-42ae-9d78-3481b9f34d65";
const FIRST: &str = "10a5c635-e71b-478d-86c4-ef933956a425";
const SECOND: &str = "f319f4c7-d809-40d9-8c97-4abb6c68b6da";
const RAW: &[u8] = br#"{"suffix":"suffix.txt","entries":[{"name":"Opening","text":"Hello"},{"name":"Ending","text":"Goodbye"}]}"#;

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.bundle.Caption",
    version = 1,
    alias = "Bundle Caption",
    component_capabilities(scene),
    field_capabilities(scene)
)]
struct Caption {
    #[nara(id = "bundle", alias = "Bundle", capabilities(scene, asset_ref))]
    bundle: AssetRef,
    #[nara(id = "product", alias = "Product")]
    product: String,
}

nara::persistent_components! { mod components = [Caption]; }

const PROVIDER_ID: PluginSchemaProviderId = PluginSchemaProviderId::new("consumer.bundle.schema");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.bundle"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.bundle.native", 1),
    components::initial_catalog,
    components::register,
);

#[derive(Default)]
struct BundlePlugin;

struct RuntimeOnlyFamily;

impl nara_asset::AssetDomain for RuntimeOnlyFamily {
    type Product = TextBundle;
}

#[derive(Clone, Default, Debug, Resource)]
struct Travel {
    request: Option<ProjectSceneRequestId>,
    finished: bool,
}

impl Plugin for BundlePlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PluginId::new("consumer.bundle"), PluginCategory::Runtime)
                .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
                .provides_schema(&[PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Travel>()?
            .add_systems(CoreStage::Update, travel)?;
        Ok(())
    }
}

fn recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_contribution(SchemaContribution::<BundlePlugin>::for_default([PROVIDER]).unwrap())
        .unwrap()
        .add_asset_domain::<TextBundleDomain>()
        .unwrap()
}

fn selected_text(world: &mut World) -> String {
    let mut query =
        SystemState::<(Query<&Caption>, nara::ecs::Res<ProjectDomainAssets>)>::new(world);
    let (captions, assets) = query.get(world).unwrap();
    let caption = captions.single().unwrap();
    let bundle = assets
        .collection::<TextBundleDomain>()
        .unwrap()
        .get(&caption.bundle)
        .unwrap();
    let reference = AssetProductRef::new(
        StableAssetId::parse_str(SOURCE).unwrap(),
        ImportedProductId::parse_subproduct(&caption.product).unwrap(),
    )
    .unwrap();
    bundle.get(&reference).unwrap().text.clone()
}

fn travel(world: &mut World) {
    let request = world.resource::<Travel>().request;
    if let Some(request) = request {
        let ready = {
            let mut access = SystemState::<ProjectScenes>::new(world);
            let scenes = access.get_mut(world).unwrap();
            scenes.state(request) == Some(ProjectSceneRequestState::Ready)
        };
        if ready {
            {
                let mut access = SystemState::<ProjectScenes>::new(world);
                let scenes = access.get_mut(world).unwrap();
                let prepared = scenes.prepared(request).unwrap();
                let bundle = prepared
                    .domain_assets()
                    .collection::<TextBundleDomain>()
                    .unwrap()
                    .get(&AssetRef::stable_id(SOURCE).unwrap())
                    .unwrap();
                let encoded = TextBundleDomain::encode_runtime(bundle).unwrap();
                let decoded = TextBundleDomain::decode_runtime(&encoded).unwrap();
                let second = AssetProductRef::new(
                    StableAssetId::parse_str(SOURCE).unwrap(),
                    ImportedProductId::parse_subproduct(SECOND).unwrap(),
                )
                .unwrap();
                assert_eq!(decoded.get(&second).unwrap().text, "Goodbye world");
            }
            let result = commit_prepared_project_scene(
                world,
                request,
                SceneProductTransactionLimits::new(
                    nara::core::ItemLimit::ONE,
                    nara::core::ItemLimit::ONE,
                ),
                &[],
                |_| {},
            );
            assert!(!result.diagnostics.has_errors(), "{:?}", result.diagnostics);
            assert_eq!(selected_text(world), "Goodbye world");
            let mut access = SystemState::<ProjectScenes>::new(world);
            assert!(access.get_mut(world).unwrap().acknowledge(request));
            world.resource_mut::<Travel>().finished = true;
        }
    } else {
        assert_eq!(selected_text(world), "Hello world");
        let mut access = SystemState::<ProjectScenes>::new(world);
        let request = access
            .get_mut(world)
            .unwrap()
            .request(ProjectPath::new("other.scene.json").unwrap())
            .unwrap();
        world.resource_mut::<Travel>().request = Some(request);
    }
}

#[test]
fn compound_products_pass_strict_check_and_actual_two_scene_travel() {
    let fixture = Fixture::new();
    let delivery = check_project(ProjectCheckRequest::new(fixture.capability(), recipe())).unwrap();
    assert_eq!(delivery.scene_roots().len(), 2);
    for expected in [
        "assets/bundle.json",
        "assets/bundle.json.meta",
        "assets/suffix.txt",
    ] {
        assert!(
            delivery
                .members()
                .iter()
                .any(|member| member.path().as_str() == expected)
        );
    }
    let intent = HeadlessRunIntent::frames(NonZeroU32::new(2000).unwrap(), Duration::ZERO)
        .with_recipe(recipe())
        .stop_when(|state: &Travel| state.finished);
    let mut run = HeadlessRun::<Travel>::new(fixture.capability(), intent, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(state) => {
                assert!(state.finished);
                break;
            }
            HeadlessRunOutcome::Failed => {
                panic!("compound travel failed: {:?}", report.diagnostics())
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

#[test]
fn rebuild_and_source_move_preserve_products_while_stale_continuity_refuses() {
    let fixture = Fixture::new();
    let encoded = || {
        let directory = fixture.capability();
        let manifest = directory
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, recipe()).unwrap();
        let loader = ProjectContentLoader::new(directory).unwrap();
        let snapshot = loader.load(&candidate, &plan).unwrap();
        assert!(
            snapshot
                .domain_assets()
                .collection::<RuntimeOnlyFamily>()
                .is_none()
        );
        let bundle = snapshot
            .domain_assets()
            .collection::<TextBundleDomain>()
            .unwrap()
            .get(&AssetRef::stable_id(SOURCE).unwrap())
            .unwrap();
        TextBundleDomain::encode_runtime(bundle).unwrap()
    };
    let original_metadata = fs::read(fixture.root.join("assets/bundle.json.meta")).unwrap();
    let before = encoded();
    assert_eq!(encoded(), before);
    assert_eq!(
        fs::read(fixture.root.join("assets/bundle.json.meta")).unwrap(),
        original_metadata
    );

    fs::rename(
        fixture.root.join("assets/bundle.json"),
        fixture.root.join("assets/renamed.json"),
    )
    .unwrap();
    let settings = TextBundleSettings::initial(
        RAW,
        64,
        [
            ImportedProductId::parse_subproduct(FIRST).unwrap(),
            ImportedProductId::parse_subproduct(SECOND).unwrap(),
        ],
    )
    .unwrap();
    let metadata = AssetMeta::new(
        AssetMetaIdentity::new(
            StableAssetId::parse_str(SOURCE).unwrap(),
            AssetPath::new("renamed.json").unwrap(),
            AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
        ),
        settings,
    );
    fs::write(
        fixture.root.join("assets/renamed.json.meta"),
        metadata.to_json_string().unwrap(),
    )
    .unwrap();
    fs::remove_file(fixture.root.join("assets/bundle.json.meta")).unwrap();
    assert_eq!(encoded(), before);
    check_project(ProjectCheckRequest::new(fixture.capability(), recipe())).unwrap();

    fs::write(fixture.root.join("assets/renamed.json"),
        br#"{"suffix":"suffix.txt","entries":[{"name":"Ending","text":"Goodbye"},{"name":"Opening","text":"Hello"}]}"#).unwrap();
    let error =
        check_project(ProjectCheckRequest::new(fixture.capability(), recipe())).unwrap_err();
    assert_eq!(
        error.content_error().unwrap().kind(),
        ProjectContentErrorKind::AssetDomain
    );
    assert_eq!(
        TextBundleDomain::decode_runtime(&before)
            .unwrap()
            .get(
                &AssetProductRef::new(
                    StableAssetId::parse_str(SOURCE).unwrap(),
                    ImportedProductId::parse_subproduct(FIRST).unwrap()
                )
                .unwrap()
            )
            .unwrap()
            .text,
        "Hello world"
    );
}

#[test]
fn copied_source_needs_a_new_enclosing_identity_before_catalogue_admission() {
    let fixture = Fixture::new();
    let copy_id = "7ed195c9-0a78-4c4a-a90e-36245eb8ef88";
    fs::write(fixture.root.join("assets/copy.json"), RAW).unwrap();
    let write_copy_metadata = |source: &str| {
        let settings = TextBundleSettings::initial(
            RAW,
            64,
            [
                ImportedProductId::parse_subproduct(FIRST).unwrap(),
                ImportedProductId::parse_subproduct(SECOND).unwrap(),
            ],
        )
        .unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str(source).unwrap(),
                AssetPath::new("copy.json").unwrap(),
                AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
            ),
            settings,
        );
        fs::write(
            fixture.root.join("assets/copy.json.meta"),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
    };
    write_copy_metadata(SOURCE);
    assert!(check_project(ProjectCheckRequest::new(fixture.capability(), recipe())).is_err());
    write_copy_metadata(copy_id);
    let directory = fixture.capability();
    let manifest = directory
        .open_file(&RelativePath::new("nara.toml").unwrap())
        .unwrap();
    let candidate = ingest_project_manifest(&manifest, None).unwrap();
    let plan = resolve_product_recipe(&candidate, recipe()).unwrap();
    let loader = ProjectContentLoader::new(directory).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    let original = snapshot
        .domain_assets()
        .collection::<TextBundleDomain>()
        .unwrap()
        .get(&AssetRef::stable_id(SOURCE).unwrap())
        .unwrap();
    let foreign = AssetProductRef::new(
        StableAssetId::parse_str(copy_id).unwrap(),
        ImportedProductId::parse_subproduct(FIRST).unwrap(),
    )
    .unwrap();
    assert!(original.get(&foreign).is_none());
    check_project(ProjectCheckRequest::new(fixture.capability(), recipe())).unwrap();
}

struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        let root = std::env::temp_dir().join(format!(
            "nara-compound-source-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        for directory in ["assets", "prefabs", "scenes", "schema"] {
            fs::create_dir(root.join(directory)).unwrap();
        }
        fs::write(root.join("nara.toml"), "schema_version = 1\n[project]\nsource_universe = 'af3176df-4556-45a5-9b2f-334ce47a5fbf'\nnative_document_identity = 'project_location'\nname = 'Compound'\n[startup]\ndefault_scene = 'startup.scene.json'\n[content]\nscenes = ['other.scene.json']\n[authoring]\nschema_bundle = 'schema/bundle.json'\n[runtime]\npreset = 'local-headless'\n[capabilities]\nrequested = ['runtime-2d']\n").unwrap();
        for (path, product) in [("startup.scene.json", FIRST), ("other.scene.json", SECOND)] {
            let caption = Caption {
                bundle: AssetRef::stable_id(SOURCE).unwrap(),
                product: product.into(),
            };
            let scene = SceneDocument::from_ordered_roots([SceneEntityRecord::new(
                SceneEntityId::new("caption").unwrap(),
            )
            .with_component(
                ComponentTypeId::new("consumer.bundle.Caption"),
                SceneComponentRecord::new(
                    ComponentSchemaVersion::ONE,
                    caption.__encode_persistent_component().unwrap(),
                ),
            )]);
            fs::write(
                root.join("scenes").join(path),
                scene.to_json_string().unwrap(),
            )
            .unwrap();
        }
        fs::write(root.join("assets/bundle.json"), RAW).unwrap();
        fs::write(root.join("assets/suffix.txt"), b" world").unwrap();
        let settings = TextBundleSettings::initial(
            RAW,
            64,
            [
                ImportedProductId::parse_subproduct(FIRST).unwrap(),
                ImportedProductId::parse_subproduct(SECOND).unwrap(),
            ],
        )
        .unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str(SOURCE).unwrap(),
                AssetPath::new("bundle.json").unwrap(),
                AssetSourceKind::Other(TextBundleDomain::SOURCE_KIND.into()),
            ),
            settings,
        );
        fs::write(
            root.join("assets/bundle.json.meta"),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
        let fixture = Self { root };
        let directory = fixture.capability();
        let file = directory
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&file, None).unwrap();
        let plan = resolve_product_recipe(&candidate, recipe()).unwrap();
        fs::write(
            fixture.root.join("schema/bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fixture
    }

    fn capability(&self) -> DirectoryCapability {
        #[cfg(windows)]
        let file = {
            use std::os::windows::fs::OpenOptionsExt;
            fs::OpenOptions::new()
                .read(true)
                .share_mode(7)
                .custom_flags(0x0200_0000 | 0x0020_0000)
                .open(&self.root)
                .unwrap()
        };
        #[cfg(not(windows))]
        let file = fs::File::open(&self.root).unwrap();
        DirectoryCapability::from_host_handle(
            file,
            HostCapabilityOptions::new(CapabilityRights::ReadOnly, TrustMode::TrustedLocal),
        )
        .unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let root = self.root.canonicalize().unwrap();
        assert!(root.starts_with(std::env::temp_dir().canonicalize().unwrap()));
        assert!(
            root.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("nara-compound-source-")
        );
        fs::remove_dir_all(root).unwrap();
    }
}
