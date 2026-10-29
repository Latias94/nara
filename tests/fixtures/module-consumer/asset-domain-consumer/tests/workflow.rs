use std::{
    fs,
    num::NonZeroU32,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant},
};

use nara::project_host::{
    ProjectSceneRequestId, ProjectSceneRequestState, ProjectScenes,
    advanced::commit_prepared_project_scene,
};
use nara::scene::advanced::SceneProductTransactionLimits;
use nara::{
    ProductRecipe, SchemaContribution,
    app::{
        App, CoreStage, Plugin, PluginCategory, PluginDeclaration, PluginError, PluginId,
        PluginSchemaProviderId,
    },
    asset::{AssetMeta, AssetMetaIdentity, AssetPath, AssetRef, AssetSourceKind, StableAssetId},
    core::ItemLimit,
    ecs::{Component, Mut, Res, ResMut, Resource, World, bevy_ecs::system::SystemState},
    fs::{CapabilityRights, DirectoryCapability, HostCapabilityOptions, RelativePath, TrustMode},
    project::ProjectPath,
    project_host::{
        HeadlessRun, HeadlessRunIntent, HeadlessRunOutcome, ProjectContentBudgetKind,
        ProjectContentErrorKind, ProjectContentLimits, ProjectContentLoader,
        ProjectContentMemberKind, ProjectDomainAssets, ProjectSettingsCandidate, RuntimePlan,
        ingest_project_manifest, resolve_product_recipe,
    },
    project_workflow::{ProjectCheckRequest, check_project},
    reflect::{
        ComponentSchemaOwnerId, ComponentSchemaProviderBindingId,
        ComponentSchemaProviderDefinition, ComponentSchemaVersion, ComponentTypeId,
        PersistentComponent, PersistentComponentProvider,
    },
    scene::{SceneComponentRecord, SceneDocument, SceneEntityId, SceneEntityRecord},
};
use nara_asset::{AssetImportError, AssetImportLimits, AssetImportSource, SourceAssetImporter};
use nara_asset_domain_consumer::{TextDomain, TextSettings};

const TEXT_ID: &str = "c4d051e8-73ee-4b16-9d13-8c5fcb8a6a32";
const PROVIDER_ID: PluginSchemaProviderId = PluginSchemaProviderId::new("consumer.text.schema");
const PROVIDER: ComponentSchemaProviderDefinition = ComponentSchemaProviderDefinition::new(
    ComponentSchemaOwnerId::new("consumer.text"),
    PROVIDER_ID,
    ComponentSchemaProviderBindingId::new("consumer.text.native", 1),
    components::initial_catalog,
    components::register,
);

#[derive(Component, PersistentComponent)]
#[nara(
    id = "consumer.text.Caption",
    version = 1,
    alias = "Caption",
    component_capabilities(scene),
    field_capabilities(scene)
)]
pub struct Caption {
    #[nara(id = "text", alias = "Text", capabilities(scene, asset_ref))]
    pub text: AssetRef,
}

nara::persistent_components! { pub mod components = [Caption]; }

#[derive(Default)]
struct CaptionPlugin;

#[derive(Clone, Default, Debug, PartialEq, Eq, Resource)]
struct Outcome(Vec<String>);

impl Plugin for CaptionPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration =
            PluginDeclaration::new(PluginId::new("consumer.text"), PluginCategory::Runtime)
                .requires_plugins(&[nara::reflect::COMPONENT_REGISTRY_PLUGIN_ID])
                .provides_schema(&[PROVIDER_ID]);
        &DECLARATION
    }

    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<Outcome>()?.add_systems(
            CoreStage::Update,
            |assets: Res<ProjectDomainAssets>, mut outcome: ResMut<Outcome>| {
                outcome.0 = assets
                    .collection::<TextDomain>()
                    .map_or_else(Vec::new, |texts| {
                        texts.iter().map(|value| value.text.clone()).collect()
                    });
            },
        )?;
        Ok(())
    }
}

fn schema_recipe() -> ProductRecipe {
    ProductRecipe::new()
        .add_contribution(SchemaContribution::<CaptionPlugin>::for_default([PROVIDER]).unwrap())
        .unwrap()
}

fn recipe() -> ProductRecipe {
    schema_recipe().add_asset_domain::<TextDomain>().unwrap()
}

fn caption_document(reference: AssetRef) -> SceneDocument {
    let caption = Caption { text: reference };
    SceneDocument::from_ordered_roots([SceneEntityRecord::new(
        SceneEntityId::new("caption").unwrap(),
    )
    .with_component(
        ComponentTypeId::new("consumer.text.Caption"),
        SceneComponentRecord::new(
            ComponentSchemaVersion::ONE,
            caption.__encode_persistent_component().unwrap(),
        ),
    )])
}

#[test]
fn independent_domain_checks_plays_and_receipts_its_exact_source_closure() {
    let project = Project::new();
    let delivery = check_project(ProjectCheckRequest::new(project.capability(), recipe())).unwrap();
    assert_eq!(
        delivery
            .scene_roots()
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>(),
        ["other.scene.json", "startup.scene.json"]
    );
    let assets = delivery
        .members()
        .iter()
        .filter(|member| {
            matches!(
                member.kind(),
                ProjectContentMemberKind::AssetMetadata | ProjectContentMemberKind::AssetSource
            )
        })
        .map(|member| member.path().as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        assets,
        [
            "assets/caption.txt",
            "assets/caption.txt.meta",
            "assets/suffix.txt"
        ]
    );
    assert_eq!(project.play(), Outcome(vec!["Hello world".to_owned()]));
}

#[test]
fn domain_setting_failure_preserves_its_actionable_diagnostic() {
    let project = Project::new();
    project.write_settings(2);
    let error =
        check_project(ProjectCheckRequest::new(project.capability(), recipe())).unwrap_err();
    let content = error.content_error().unwrap();
    assert_eq!(content.kind(), ProjectContentErrorKind::AssetDomain);
    assert!(
        content
            .diagnostics()
            .iter()
            .any(|entry| entry.code().as_str() == "consumer.text.too-long")
    );
    project.write_settings(0);
    let error =
        check_project(ProjectCheckRequest::new(project.capability(), recipe())).unwrap_err();
    assert!(
        error
            .content_error()
            .unwrap()
            .diagnostics()
            .iter()
            .any(|entry| entry.code().as_str() == "consumer.text.settings-invalid")
    );
}

#[test]
fn missing_domain_rejects_only_referenced_content_and_duplicate_ownership_rejects() {
    let project = Project::new();
    let error = check_project(ProjectCheckRequest::new(
        project.capability(),
        schema_recipe(),
    ))
    .unwrap_err();
    assert_eq!(
        error.content_error().unwrap().kind(),
        ProjectContentErrorKind::UnsupportedAssetKind
    );
    assert!(recipe().add_asset_domain::<TextDomain>().is_err());
    for path in ["startup.scene.json", "other.scene.json"] {
        fs::write(
            project.root.join("scenes").join(path),
            SceneDocument::from_ordered_roots([])
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
    }
    // The unreferenced domain files remain present, without requiring an installed handler.
    check_project(ProjectCheckRequest::new(
        project.capability(),
        schema_recipe(),
    ))
    .unwrap();
    assert!(project.root.join("assets/caption.txt").exists());
}

#[test]
fn missing_dependency_rejects_the_selected_closure() {
    let project = Project::new();
    fs::remove_file(project.root.join("assets/suffix.txt")).unwrap();
    let error =
        check_project(ProjectCheckRequest::new(project.capability(), recipe())).unwrap_err();
    let content = error.content_error().unwrap();
    assert_eq!(content.kind(), ProjectContentErrorKind::HostIo);
    assert!(
        content
            .diagnostics()
            .iter()
            .any(|entry| entry.code().as_str() == "project.content.host-io")
    );
    assert!(!project.root.join("export").exists());
}

#[test]
fn retained_domain_owner_keeps_its_generation_and_budget_alive() {
    let project = Project::new();
    let (candidate, plan, root) = project.inputs();
    let loader = ProjectContentLoader::new(root.share()).unwrap();
    let snapshot = loader.load(&candidate, &plan).unwrap();
    assert_eq!(
        loader
            .budget_snapshot()
            .high_water(ProjectContentBudgetKind::Files),
        5
    );
    let retained = snapshot.domain_assets().clone();
    drop(snapshot);
    assert_eq!(
        retained
            .collection::<TextDomain>()
            .unwrap()
            .get(&AssetRef::stable_id(TEXT_ID).unwrap())
            .unwrap()
            .text,
        "Hello world"
    );
    assert_eq!(loader.budget_snapshot().active_generations(), 1);
    drop(retained);
    assert_eq!(loader.budget_snapshot().active_generations(), 0);
    assert_eq!(loader.budget_snapshot().active_reservations(), 0);
    let limits = ProjectContentLimits::default()
        .with_limit(ProjectContentBudgetKind::RetainedBytes, 1)
        .unwrap();
    let limited = ProjectContentLoader::with_limits(root, limits).unwrap();
    assert!(limited.load(&candidate, &plan).is_err());
    assert_eq!(limited.budget_snapshot().active_reservations(), 0);
}

struct OversizedDomain;
impl nara_asset::AssetDomain for OversizedDomain {
    type Product = ();
}

impl SourceAssetImporter for OversizedDomain {
    type Settings = TextSettings;
    const SOURCE_KIND: &'static str = "consumer.text";
    fn limits() -> AssetImportLimits {
        AssetImportLimits::new(
            nara::core::ByteLimit::new(4096).unwrap(),
            nara::core::ByteLimit::new(4096).unwrap(),
            nara::core::ByteLimit::new(1024 * 1024 * 1024).unwrap(),
        )
    }
    fn prepare<E>(_: &mut AssetImportSource<'_, E>) -> Result<(), AssetImportError<E>> {
        panic!("the Host must reject the reservation before invoking this domain")
    }
}

struct PanickingDomain;
impl nara_asset::AssetDomain for PanickingDomain {
    type Product = ();
}

impl SourceAssetImporter for PanickingDomain {
    type Settings = TextSettings;
    const SOURCE_KIND: &'static str = "consumer.text";
    fn limits() -> AssetImportLimits {
        TextDomain::limits()
    }
    fn prepare<E>(_: &mut AssetImportSource<'_, E>) -> Result<(), AssetImportError<E>> {
        panic!("domain panic payload")
    }
}

#[test]
fn domain_reservations_precede_execution_and_panics_return_bounded_diagnostics() {
    let project = Project::new();
    let error = check_project(ProjectCheckRequest::new(
        project.capability(),
        schema_recipe()
            .add_asset_domain::<OversizedDomain>()
            .unwrap(),
    ))
    .unwrap_err();
    assert_eq!(
        error.content_error().unwrap().kind(),
        ProjectContentErrorKind::BudgetExceeded
    );
    let error = check_project(ProjectCheckRequest::new(
        project.capability(),
        schema_recipe()
            .add_asset_domain::<PanickingDomain>()
            .unwrap(),
    ))
    .unwrap_err();
    let content = error.content_error().unwrap();
    assert_eq!(content.kind(), ProjectContentErrorKind::AssetDomain);
    assert!(
        content
            .diagnostics()
            .iter()
            .any(|entry| entry.code().as_str() == "project.content.domain-panicked")
    );
    assert!(!format!("{content:?}").contains("domain panic payload"));
}

#[derive(Clone, Default, Debug, Resource)]
struct TravelState {
    phase: u8,
    request: Option<ProjectSceneRequestId>,
    finished: bool,
}

#[derive(Default)]
struct TravelPlugin;

impl Plugin for TravelPlugin {
    fn declaration() -> &'static PluginDeclaration {
        static DECLARATION: PluginDeclaration = PluginDeclaration::new(
            PluginId::new("consumer.text.travel"),
            PluginCategory::Runtime,
        );
        &DECLARATION
    }
    fn build(&self, app: &mut App) -> Result<(), PluginError> {
        app.init_resource::<TravelState>()?
            .add_systems(CoreStage::Update, travel)?;
        Ok(())
    }
}

fn request(world: &mut World, path: &str) -> ProjectSceneRequestId {
    let mut access = SystemState::<ProjectScenes>::new(world);
    access
        .get_mut(world)
        .unwrap()
        .request(ProjectPath::new(path).unwrap())
        .unwrap()
}

fn acknowledge(world: &mut World, id: ProjectSceneRequestId) {
    let mut access = SystemState::<ProjectScenes>::new(world);
    assert!(access.get_mut(world).unwrap().acknowledge(id));
}

fn text(world: &World) -> Option<&str> {
    world
        .resource::<ProjectDomainAssets>()
        .collection::<TextDomain>()?
        .iter()
        .next()
        .map(|product| product.text.as_str())
}

fn travel(world: &mut World) {
    world.resource_scope(|world, mut progress: Mut<TravelState>| {
        if progress.finished {
            return;
        }
        if progress.phase == 0 {
            assert_eq!(text(world), Some("Hello world"));
            progress.request = Some(request(world, "invalid.scene.json"));
            progress.phase = 1;
            return;
        }
        let id = progress.request.unwrap();
        let state = {
            let mut access = SystemState::<ProjectScenes>::new(world);
            access.get_mut(world).unwrap().state(id).unwrap()
        };
        if state == ProjectSceneRequestState::Preparing {
            std::thread::sleep(Duration::from_millis(1));
            return;
        }
        let limits = SceneProductTransactionLimits::new(ItemLimit::ONE, ItemLimit::ONE);
        match progress.phase {
            1 => {
                assert!(matches!(state, ProjectSceneRequestState::Rejected(_)));
                assert_eq!(text(world), Some("Hello world"));
                acknowledge(world, id);
                progress.request = Some(request(world, "other.scene.json"));
                progress.phase = 2;
            }
            2 => {
                assert_eq!(state, ProjectSceneRequestState::Ready);
                {
                    let mut access = SystemState::<ProjectScenes>::new(world);
                    let scenes = access.get_mut(world).unwrap();
                    let prepared = scenes.prepared(id).unwrap();
                    assert_eq!(
                        prepared
                            .domain_assets()
                            .collection::<TextDomain>()
                            .unwrap()
                            .iter()
                            .next()
                            .unwrap()
                            .text,
                        "Goodbye world"
                    );
                }
                let invalid = SceneProductTransactionLimits::new(
                    ItemLimit::new(SceneProductTransactionLimits::MAX_OVERLAY_WRITES + 1).unwrap(),
                    ItemLimit::ONE,
                );
                let rejected = commit_prepared_project_scene(world, id, invalid, &[], |_| {});
                assert!(rejected.diagnostics.has_errors());
                assert_eq!(text(world), Some("Hello world"));
                acknowledge(world, id);
                progress.request = Some(request(world, "other.scene.json"));
                progress.phase = 3;
            }
            3 => {
                assert_eq!(state, ProjectSceneRequestState::Ready);
                let committed = commit_prepared_project_scene(world, id, limits, &[], |_| {});
                assert!(
                    !committed.diagnostics.has_errors(),
                    "{:?}",
                    committed.diagnostics
                );
                assert_eq!(text(world), Some("Goodbye world"));
                acknowledge(world, id);
                progress.request = Some(request(world, "empty.scene.json"));
                progress.phase = 4;
            }
            4 => {
                assert_eq!(state, ProjectSceneRequestState::Ready);
                let committed = commit_prepared_project_scene(world, id, limits, &[], |_| {});
                assert!(
                    !committed.diagnostics.has_errors(),
                    "{:?}",
                    committed.diagnostics
                );
                assert_eq!(text(world), None);
                acknowledge(world, id);
                progress.finished = true;
            }
            _ => unreachable!(),
        }
    });
}

#[test]
fn scene_preparation_and_commit_failures_preserve_current_products_then_replace_and_retire() {
    let project = Project::new();
    let manifest = project.root.join("nara.toml");
    let changed = fs::read_to_string(&manifest).unwrap().replace(
        "scenes = [\"other.scene.json\"]",
        "scenes = [\"other.scene.json\", \"invalid.scene.json\", \"empty.scene.json\"]",
    );
    fs::write(manifest, changed).unwrap();
    for (path, source, id, bytes) in [
        (
            "other.scene.json",
            "next.txt",
            "c4d051e8-73ee-4b16-9d13-8c5fcb8a6a33",
            "suffix.txt\nGoodbye",
        ),
        (
            "invalid.scene.json",
            "broken.txt",
            "c4d051e8-73ee-4b16-9d13-8c5fcb8a6a34",
            "invalid",
        ),
    ] {
        fs::write(
            project.root.join("scenes").join(path),
            caption_document(AssetRef::path(source).unwrap())
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        fs::write(project.root.join("assets").join(source), bytes).unwrap();
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str(id).unwrap(),
                AssetPath::new(source).unwrap(),
                AssetSourceKind::Other("consumer.text".to_owned()),
            ),
            TextSettings { max_characters: 64 },
        );
        fs::write(
            project.root.join("assets").join(format!("{source}.meta")),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
    }
    fs::write(
        project.root.join("scenes/empty.scene.json"),
        SceneDocument::default().to_json_string().unwrap(),
    )
    .unwrap();
    let intent = HeadlessRunIntent::frames(NonZeroU32::new(2000).unwrap(), Duration::ZERO)
        .with_recipe(recipe().add_plugin::<TravelPlugin>().unwrap())
        .stop_when(|state: &TravelState| state.finished);
    let mut run = HeadlessRun::<TravelState>::new(project.capability(), intent, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(state) => {
                assert!(state.finished);
                break;
            }
            HeadlessRunOutcome::Failed => panic!("travel failed: {:?}", report.diagnostics()),
            HeadlessRunOutcome::CleanupIncomplete => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

fn play(root: DirectoryCapability) -> Outcome {
    let intent = HeadlessRunIntent::frames(NonZeroU32::new(1).unwrap(), Duration::ZERO)
        .with_recipe(recipe());
    let mut run = HeadlessRun::<Outcome>::new(root, intent, Vec::new());
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let report = run.execute_bounded();
        match report.outcome() {
            HeadlessRunOutcome::Completed(outcome) => return outcome.clone(),
            HeadlessRunOutcome::Failed => {
                panic!("headless Play failed: {:?}", report.diagnostics())
            }
            HeadlessRunOutcome::CleanupIncomplete => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }
}

struct Project {
    root: PathBuf,
}

impl Project {
    fn new() -> Self {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nara-external-assets-{}-{}",
            std::process::id(),
            SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        for directory in [
            "assets",
            "prefabs",
            "scenes",
            "schema",
            "src",
            "src/bin",
            "build-products",
        ] {
            fs::create_dir(root.join(directory)).unwrap();
        }
        fs::write(root.join("nara.toml"), "schema_version = 1\n[project]\nsource_universe = '7db08953-1037-5a66-806b-be16fccc7250'\nnative_document_identity = 'project_location'\nname = \"External assets\"\n[paths]\nassets = \"assets\"\nscenes = \"scenes\"\nprefabs = \"prefabs\"\n[startup]\ndefault_scene = \"startup.scene.json\"\n[content]\nscenes = [\"other.scene.json\"]\n[authoring]\nschema_bundle = \"schema/authoring.bundle.json\"\n[runtime]\npreset = \"local-headless\"\n[capabilities]\nrequested = [\"runtime-2d\"]\n").unwrap();
        for (path, reference) in [
            ("startup.scene.json", AssetRef::path("caption.txt").unwrap()),
            ("other.scene.json", AssetRef::stable_id(TEXT_ID).unwrap()),
        ] {
            fs::write(
                root.join("scenes").join(path),
                caption_document(reference).to_json_string().unwrap(),
            )
            .unwrap();
        }
        fs::write(root.join("assets/caption.txt"), b"suffix.txt\nHello").unwrap();
        fs::write(root.join("assets/suffix.txt"), b" world").unwrap();
        let project = Self { root };
        project.write_settings(64);
        let (_, plan, _) = project.inputs();
        fs::write(
            project.root.join("schema/authoring.bundle.json"),
            plan.schema_validation()
                .authoring_schema_bundle()
                .to_json_string()
                .unwrap(),
        )
        .unwrap();
        project
    }

    fn write_settings(&self, maximum: usize) {
        let metadata = AssetMeta::new(
            AssetMetaIdentity::new(
                StableAssetId::parse_str(TEXT_ID).unwrap(),
                AssetPath::new("caption.txt").unwrap(),
                AssetSourceKind::Other("consumer.text".to_owned()),
            ),
            TextSettings {
                max_characters: maximum,
            },
        );
        fs::write(
            self.root.join("assets/caption.txt.meta"),
            metadata.to_json_string().unwrap(),
        )
        .unwrap();
    }

    fn capability(&self) -> DirectoryCapability {
        capability(&self.root)
    }
    fn play(&self) -> Outcome {
        play(self.capability())
    }

    fn inputs(&self) -> (ProjectSettingsCandidate, RuntimePlan, DirectoryCapability) {
        let root = self.capability();
        let manifest = root
            .open_file(&RelativePath::new("nara.toml").unwrap())
            .unwrap();
        let candidate = ingest_project_manifest(&manifest, None).unwrap();
        let plan = resolve_product_recipe(&candidate, recipe()).unwrap();
        (candidate, plan, root)
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn capability(path: &Path) -> DirectoryCapability {
    #[cfg(windows)]
    let handle = {
        use std::os::windows::fs::OpenOptionsExt;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(7)
            .custom_flags(0x0200_0000 | 0x0020_0000)
            .open(path)
            .unwrap()
    };
    #[cfg(not(windows))]
    let handle = fs::File::open(path).unwrap();
    DirectoryCapability::from_host_handle(
        handle,
        HostCapabilityOptions::new(CapabilityRights::ReadWrite, TrustMode::TrustedLocal),
    )
    .unwrap()
}
